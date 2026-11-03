//! A media filename collision is deliberately tested as a destructive
//! remote-wins update; the recovery ZIP retains the displaced local version.
use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn remote_wins_pending_local_media_conflict_but_private_recovery_keeps_local_bytes() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("conflict.txt", b"local unsynced payload")
        .unwrap();
    drop(media);
    let before = backend.collection_stats().unwrap();
    let index = root.path().join("source/collection.mdb");
    let dirty_before: i64 = rusqlite::Connection::open(&index)
        .unwrap()
        .query_row(
            "SELECT dirty FROM media WHERE fname='conflict.txt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dirty_before, 1);

    let recovery = root.path().join("before-media-conflict.zip");
    let recovery_at_server = recovery.clone();
    let media_zip = anki::sync::media::zip::zip_files_for_download(vec![(
        "conflict.txt".to_owned(),
        b"remote authoritative payload".to_vec(),
    )])
    .unwrap();
    const REMOTE_SHA1: &str = "c8266894f3e5ce613e6a54defebd1bd8f1538482";
    let changes = format!(
        r#"{{"data":[["conflict.txt",1,"{REMOTE_SHA1}"]],"err":""}}"#
    );
    let exchanges: Vec<(&str, Vec<u8>)> = vec![
        (
            "POST /sync/hostKey ",
            br#"{"key":"synthetic-conflict-key"}"#.to_vec(),
        ),
        (
            "POST /msync/begin ",
            br#"{"data":{"usn":1,"sk":"synthetic-conflict-key"},"err":""}"#.to_vec(),
        ),
        ("POST /msync/mediaChanges ", changes.into_bytes()),
        ("POST /msync/downloadFiles ", media_zip),
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
                        assert!(Instant::now() < deadline, "missing media conflict request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock media server accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            assert!(
                recovery_at_server.is_file(),
                "recovery ZIP must precede any remote connection"
            );

            let mut request = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let amount = stream.read(&mut buffer).unwrap();
                assert!(amount > 0, "incomplete synthetic conflict request");
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
                "unexpected media conflict route or upload attempt"
            );
            let packed = zstd::stream::encode_all(response.as_slice(), 1).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len(),
                packed.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&packed).unwrap();
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

    assert_eq!(
        std::fs::read(root.path().join("source/collection.media/conflict.txt")).unwrap(),
        b"remote authoritative payload"
    );
    let (dirty_after, last_usn): (i64, i64) = rusqlite::Connection::open(&index)
        .unwrap()
        .query_row(
            "SELECT media.dirty, meta.lastUsn FROM media, meta WHERE media.fname='conflict.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((dirty_after, last_usn), (0, 1));

    let restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "recovered",
        &recovery,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("recovered/collection.media/conflict.txt")).unwrap(),
        b"local unsynced payload"
    );
    let retained_dirty: i64 =
        rusqlite::Connection::open(root.path().join("recovered/collection.mdb"))
            .unwrap()
            .query_row(
                "SELECT dirty FROM media WHERE fname='conflict.txt'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(retained_dirty, 1);
    restored.close().unwrap();
}
