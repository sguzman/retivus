//! Verify that a server-initiated deletion removes clean local media while
//! the private pre-sync archive retains the file and its original index.
use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn remote_media_deletion_is_recoverable_from_private_pre_sync_bundle() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("deleted.txt", b"original synced media")
        .unwrap();
    drop(media);

    let index = root.path().join("source/collection.mdb");
    let original = rusqlite::Connection::open(&index).unwrap();
    assert_eq!(
        original
            .execute(
                "UPDATE media SET dirty=0 WHERE fname='deleted.txt'",
                [],
            )
            .unwrap(),
        1
    );
    drop(original);
    let before = backend.collection_stats().unwrap();

    let recovery = root.path().join("before-media-deletion.zip");
    let recovery_at_server = recovery.clone();
    let exchanges: Vec<(&str, Vec<u8>)> = vec![
        (
            "POST /sync/hostKey ",
            br#"{"key":"synthetic-deletion-key"}"#.to_vec(),
        ),
        (
            "POST /msync/begin ",
            br#"{"data":{"usn":2,"sk":"synthetic-deletion-key"},"err":""}"#.to_vec(),
        ),
        (
            "POST /msync/mediaChanges ",
            br#"{"data":[["deleted.txt",2,""]],"err":""}"#.to_vec(),
        ),
        (
            "POST /msync/mediaChanges ",
            br#"{"data":[],"err":""}"#.to_vec(),
        ),
        (
            "POST /msync/mediaSanity ",
            br#"{"data":"OK","err":""}"#.to_vec(),
        ),
    ];

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        for (expected_route, response) in exchanges {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing media deletion request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock deletion server accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            assert!(
                recovery_at_server.is_file(),
                "pre-sync archive must exist before network traffic"
            );
            let mut request = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let amount = stream.read(&mut buffer).unwrap();
                assert!(amount > 0, "incomplete synthetic deletion request");
                request.extend_from_slice(&buffer[..amount]);
                assert!(request.len() < 2 * 1024 * 1024);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let body = &request[end + 4..];
                    if header
                        .to_ascii_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        if body.windows(5).any(|w| w == b"0\r\n\r\n") {
                            break;
                        }
                    } else if let Some(length) = header.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|len| len.trim().parse::<usize>().ok())
                    }) {
                        if body.len() >= length {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            assert!(
                String::from_utf8_lossy(&request).starts_with(expected_route),
                "server-triggered removal must not attempt a local upload"
            );
            let compressed = zstd::stream::encode_all(response.as_slice(), 1).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len(),
                compressed.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&compressed).unwrap();
            stream.flush().unwrap();
        }
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let outcome = runtime.block_on(backend.sync_ankiweb_media_with_client(
        "user@example.test",
        "synthetic-password",
        &recovery,
        client,
        Some(endpoint),
    ));
    server.join().unwrap();
    outcome.unwrap();
    assert_eq!(backend.collection_stats().unwrap(), before);

    assert!(!root.path().join("source/collection.media/deleted.txt").exists());
    let resulting = rusqlite::Connection::open(&index).unwrap();
    let (count, last_usn): (i64, i64) = resulting
        .query_row(
            "SELECT (SELECT count(*) FROM media), lastUsn FROM meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((count, last_usn), (0, 2));
    drop(resulting);

    let restored =
        AnkiBackend::restore_pre_sync_recovery_as_new_profile(root.path(), "recovered", &recovery)
            .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("recovered/collection.media/deleted.txt")).unwrap(),
        b"original synced media"
    );
    let restored_index =
        rusqlite::Connection::open(root.path().join("recovered/collection.mdb")).unwrap();
    let (dirty, last_usn): (i64, i64) = restored_index
        .query_row(
            "SELECT media.dirty, meta.lastUsn FROM media, meta WHERE media.fname='deleted.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((dirty, last_usn), (0, 0));
    drop(restored_index);
    restored.close().unwrap();
}
