mod conflicts;
mod deletions;
mod upstream_server;

use super::*;
use tempfile::tempdir;

#[test]
fn public_media_sync_refuses_dirty_previous_media_before_backup_or_login() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("local.txt", b"private local revision")
        .unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute("UPDATE meta SET lastUsn=7", []).unwrap();
    drop(db);

    let before = backend.collection_stats().unwrap();
    let error = backend.inspect_local_media_conflict_risk().unwrap_err();
    assert!(error.to_string().contains("pending local changes"));
    let archive = root.path().join("must-not-write.zip");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let refused = rt.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::RefuseUnverifiedRemoteReplacement,
    ));
    assert!(refused
        .unwrap_err()
        .to_string()
        .contains("pending local changes"));
    // Acknowledging remote overwrites cannot waive a separately detected
    // local dirty-index conflict. Neither attempt may create a backup or
    // contact AnkiWeb; both must fail on the same offline safety guard.
    let still_refused = rt.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::ExplicitlyAcceptPossibleRemoteReplacement,
    ));
    assert!(still_refused
        .unwrap_err()
        .to_string()
        .contains("pending local changes"));
    assert!(!archive.exists(), "do not create a recovery ZIP or log in");
    assert_eq!(backend.collection_stats().unwrap(), before);
    assert_eq!(
        std::fs::read(root.path().join("learner/collection.media/local.txt")).unwrap(),
        b"private local revision"
    );
    let (dirty, last_usn): (i64, i64) = rusqlite::Connection::open(&index)
        .unwrap()
        .query_row(
            "SELECT dirty, lastUsn FROM media, meta WHERE fname='local.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((dirty, last_usn), (1, 7));
}

#[cfg(unix)]
#[test]
fn first_media_sync_refuses_symlink_without_initializing_index_or_contacting_server() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media_dir = root.path().join("learner/collection.media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let private_file = root.path().join("outside-private.txt");
    std::fs::write(&private_file, b"not media for upload").unwrap();
    symlink(&private_file, media_dir.join("outside.txt")).unwrap();

    let index = root.path().join("learner/collection.mdb");
    assert!(!index.exists());
    let error = backend.inspect_local_media_conflict_risk().unwrap_err().to_string();
    assert!(error.contains("unexpected non-regular media entry"), "{error}");
    assert!(!index.exists(), "offline inspection must not create an index");

    let archive = root.path().join("must-not-write-symlink.zip");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let denied = rt.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::ExplicitlyAcceptPossibleRemoteReplacement,
    ));
    assert!(denied
        .unwrap_err()
        .to_string()
        .contains("unexpected non-regular media entry"));
    assert!(!archive.exists(), "refusal must precede backup and login");
    assert!(!index.exists(), "refusal must precede index initialization");
    assert_eq!(std::fs::read(&private_file).unwrap(), b"not media for upload");
}

#[test]
fn first_media_sync_with_new_index_refuses_nested_directory_before_backup() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media.add_file("regular.txt", b"allowed first upload").unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let sequence: i64 = rusqlite::Connection::open(&index)
        .unwrap()
        .query_row("SELECT lastUsn FROM meta", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sequence, 0);
    let nested = root.path().join("learner/collection.media/nested");
    std::fs::create_dir(&nested).unwrap();

    let error = backend.inspect_local_media_conflict_risk().unwrap_err().to_string();
    assert!(error.contains("unexpected non-regular media entry"), "{error}");

    let archive = root.path().join("must-not-write-nested.zip");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let denied = rt.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::ExplicitlyAcceptPossibleRemoteReplacement,
    ));
    assert!(denied
        .unwrap_err()
        .to_string()
        .contains("unexpected non-regular media entry"));
    assert!(!archive.exists(), "refusal must precede backup and login");
    assert_eq!(std::fs::read(root.path().join("learner/collection.media/regular.txt")).unwrap(), b"allowed first upload");
}

#[test]
fn offline_media_guard_allows_new_index_and_clean_earlier_sync() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let index = root.path().join("learner/collection.mdb");
    backend.inspect_local_media_conflict_risk().unwrap();
    assert!(!index.exists(), "read-only probe must not create an index");

    let media = backend.collection.media().unwrap();
    media.add_file("first.txt", b"first upload").unwrap();
    drop(media);
    backend.inspect_local_media_conflict_risk().unwrap();

    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute("UPDATE meta SET lastUsn=2", []).unwrap();
    drop(db);
    assert!(backend.inspect_local_media_conflict_risk().is_err());

    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute("UPDATE media SET dirty=0 WHERE fname='first.txt'", [])
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();

    // Anki's change tracker writes seconds, but direct media additions use
    // milliseconds. Both are valid clean index representations.
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute("UPDATE media SET mtime = mtime / 1000", [])
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();
}

#[test]
fn offline_media_guard_refuses_unindexed_files_after_a_prior_sync_before_network() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("tracked.txt", b"already synchronized")
        .unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute_batch("UPDATE media SET dirty=0; UPDATE meta SET lastUsn=7;")
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();

    let added = root.path().join("learner/collection.media/new-file.txt");
    std::fs::write(&added, b"manually added local media").unwrap();
    let before = backend.collection_stats().unwrap();
    let error = backend
        .inspect_local_media_conflict_risk()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("absent from the clean media index"),
        "{error}"
    );

    let archive = root.path().join("must-not-write-unindexed.zip");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let denied = runtime.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::RefuseUnverifiedRemoteReplacement,
    ));
    assert!(denied
        .unwrap_err()
        .to_string()
        .contains("absent from the clean media index"));
    assert!(!archive.exists(), "refuse before backup or network login");
    assert_eq!(backend.collection_stats().unwrap(), before);
    assert_eq!(
        std::fs::read(&added).unwrap(),
        b"manually added local media"
    );
}

#[test]
fn offline_media_guard_refuses_directly_modified_clean_media() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media.add_file("tracked.txt", b"original content").unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute_batch("UPDATE media SET dirty=0; UPDATE meta SET lastUsn=7;")
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();

    let tracked = root.path().join("learner/collection.media/tracked.txt");
    std::fs::write(&tracked, b"directly changed content").unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&tracked)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120))
        .unwrap();

    let error = backend
        .inspect_local_media_conflict_risk()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("indexed media file changed on disk"),
        "{error}"
    );
    assert_eq!(
        std::fs::read(&tracked).unwrap(),
        b"directly changed content"
    );
}

#[test]
fn offline_media_guard_refuses_deleted_clean_indexed_media() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media.add_file("tracked.txt", b"original content").unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute_batch("UPDATE media SET dirty=0; UPDATE meta SET lastUsn=7;")
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();

    let tracked = root.path().join("learner/collection.media/tracked.txt");
    std::fs::remove_file(&tracked).unwrap();
    let error = backend
        .inspect_local_media_conflict_risk()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("indexed media file is missing on disk"),
        "{error}"
    );
}

#[test]
fn offline_media_guard_refuses_mtime_preserving_external_content_change() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media.add_file("tracked.txt", b"original bytes").unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute_batch("UPDATE media SET dirty=0; UPDATE meta SET lastUsn=7;")
        .unwrap();
    drop(db);
    backend.inspect_local_media_conflict_risk().unwrap();

    let tracked = root.path().join("learner/collection.media/tracked.txt");
    let original_mtime = std::fs::metadata(&tracked).unwrap().modified().unwrap();
    std::fs::write(&tracked, b"changed  bytes").unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&tracked)
        .unwrap()
        .set_modified(original_mtime)
        .unwrap();
    assert_eq!(
        std::fs::metadata(&tracked).unwrap().modified().unwrap(),
        original_mtime
    );

    let error = backend
        .inspect_local_media_conflict_risk()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("media checksum differs from disk"),
        "{error}"
    );
    let archive = root.path().join("must-not-write-same-time.zip");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let denied = runtime.block_on(backend.sync_ankiweb_media_with_recovery(
        "synthetic@example.invalid",
        "synthetic-password",
        &archive,
        true,
        MediaSyncConflictRisk::RefuseUnverifiedRemoteReplacement,
    ));
    assert!(denied
        .unwrap_err()
        .to_string()
        .contains("media checksum differs from disk"));
    assert!(!archive.exists(), "refuse before recovery ZIP or network");
    assert_eq!(std::fs::read(&tracked).unwrap(), b"changed  bytes");
}

#[test]
fn offline_media_guard_refuses_corrupted_clean_index_checksum() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    let media = backend.collection.media().unwrap();
    media.add_file("tracked.txt", b"unchanged bytes").unwrap();
    drop(media);

    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute_batch(
        "UPDATE media SET dirty=0, csum='0000000000000000000000000000000000000000';
         UPDATE meta SET lastUsn=7;",
    )
    .unwrap();
    drop(db);
    let error = backend
        .inspect_local_media_conflict_risk()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("media checksum differs from disk"),
        "{error}"
    );
}

#[test]
fn malformed_preexisting_media_index_fails_closed_before_network() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
    drop(backend.collection.media().unwrap());
    let index = root.path().join("learner/collection.mdb");
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute("DELETE FROM meta", []).unwrap();
    drop(db);
    assert!(backend.inspect_local_media_conflict_risk().is_err());
    let archive = root.path().join("must-not-write-malformed.zip");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(runtime
        .block_on(backend.sync_ankiweb_media_with_recovery(
            "synthetic@example.invalid",
            "synthetic-password",
            &archive,
            true,
            MediaSyncConflictRisk::RefuseUnverifiedRemoteReplacement,
        ))
        .is_err());
    assert!(
        !archive.exists(),
        "invalid index must block before backup/login"
    );
}

#[test]
fn failed_media_request_preserves_recoverable_private_index_and_files() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
    backend.add_basic_note(1, "question", "answer").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("original.png", b"synthetic private picture")
        .unwrap();
    drop(media);
    let before = backend.collection_stats().unwrap();
    let archive = root.path().join("before-media-sync.zip");
    let archive_seen_by_server = archive.clone();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for request_number in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            assert!(
                archive_seen_by_server.is_file(),
                "recovery archive must exist before all network requests"
            );
            let mut request = Vec::new();
            let mut part = [0_u8; 8192];
            loop {
                let count = stream.read(&mut part).unwrap();
                assert!(count > 0, "incomplete synthetic media request");
                request.extend_from_slice(&part[..count]);
                assert!(request.len() < 1_048_576);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..end]);
                    let body = &request[end + 4..];
                    if head
                        .to_ascii_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        if body.windows(5).any(|w| w == b"0\r\n\r\n") {
                            break;
                        }
                    } else if let Some(size) = head.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    }) {
                        if body.len() >= size {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            let expected = if request_number == 0 {
                "POST /sync/hostKey "
            } else {
                "POST /msync/begin "
            };
            assert!(
                String::from_utf8_lossy(&request).starts_with(expected),
                "unexpected upstream endpoint"
            );
            if request_number == 0 {
                let json = br#"{"key":"synthetic-host-key"}"#;
                let compressed = zstd::stream::encode_all(json.as_slice(), 1).unwrap();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    json.len(),
                    compressed.len()
                );
                stream.write_all(headers.as_bytes()).unwrap();
                stream.write_all(&compressed).unwrap();
            } else {
                stream
                    .write_all(
                        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            }
            stream.flush().unwrap();
        }
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = runtime.block_on(backend.sync_ankiweb_media_with_client(
        "user@example.test",
        "synthetic-password",
        &archive,
        client,
        Some(endpoint),
    ));
    server.join().unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "AnkiWeb media sync failed; recovery ZIP retained"
    );
    assert_eq!(backend.collection_stats().unwrap(), before);
    assert!(archive.is_file());

    let restored =
        AnkiBackend::restore_pre_sync_recovery_as_new_profile(root.path(), "recovered", &archive)
            .unwrap();
    assert_eq!(restored.collection_stats().unwrap(), before);
    assert_eq!(
        std::fs::read(root.path().join("recovered/collection.media/original.png")).unwrap(),
        b"synthetic private picture"
    );
    let count: i64 = rusqlite::Connection::open(root.path().join("recovered/collection.mdb"))
        .unwrap()
        .query_row("SELECT count(*) FROM media", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn synthetic_media_upload_marks_index_clean_only_after_server_acknowledgment() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
    let media = backend.collection.media().unwrap();
    media
        .add_file("uploaded.txt", b"synthetic media upload")
        .unwrap();
    drop(media);
    let index_path = root.path().join("source/collection.mdb");
    let dirty_before: i64 = rusqlite::Connection::open(&index_path)
        .unwrap()
        .query_row(
            "SELECT dirty FROM media WHERE fname='uploaded.txt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dirty_before, 1);

    let recovery = root.path().join("before-synthetic-upload.zip");
    let recovery_at_server = recovery.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let routes = [
            (
                "POST /sync/hostKey ",
                br#"{"key":"synthetic-upload-key"}"#.as_slice(),
            ),
            (
                "POST /msync/begin ",
                br#"{"data":{"usn":0,"sk":"synthetic-upload-key"},"err":""}"#.as_slice(),
            ),
            (
                "POST /msync/uploadChanges ",
                br#"{"data":[1,1],"err":""}"#.as_slice(),
            ),
            (
                "POST /msync/mediaSanity ",
                br#"{"data":"OK","err":""}"#.as_slice(),
            ),
        ];
        for (route, response) in routes {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            assert!(
                recovery_at_server.is_file(),
                "recovery must exist before the first remote request"
            );
            let mut request = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let len = stream.read(&mut buffer).unwrap();
                assert!(len > 0, "incomplete mock media-sync request");
                request.extend_from_slice(&buffer[..len]);
                assert!(request.len() < 2 * 1024 * 1024);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let body = &request[end + 4..];
                    if header
                        .to_ascii_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        if body.windows(5).any(|part| part == b"0\r\n\r\n") {
                            break;
                        }
                    } else if let Some(len) = header.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|text| text.trim().parse::<usize>().ok())
                    }) {
                        if body.len() >= len {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            assert!(
                String::from_utf8_lossy(&request).starts_with(route),
                "wrong media sync endpoint or upload order"
            );
            let payload = zstd::stream::encode_all(response, 1).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len(),
                payload.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&payload).unwrap();
            stream.flush().unwrap();
        }
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(backend.sync_ankiweb_media_with_client(
            "user@example.test",
            "synthetic-password",
            &recovery,
            client,
            Some(endpoint),
        ))
        .unwrap();
    server.join().unwrap();

    let after = rusqlite::Connection::open(&index_path).unwrap();
    let (dirty, sync_usn): (i64, i64) = after
        .query_row(
            "SELECT media.dirty, meta.lastUsn FROM media, meta WHERE media.fname='uploaded.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(dirty, 0);
    assert_eq!(sync_usn, 1);
    assert_eq!(
        std::fs::read(root.path().join("source/collection.media/uploaded.txt")).unwrap(),
        b"synthetic media upload"
    );

    // A new profile restored from the PRE-sync bundle has the original
    // pending-media state, not the server-acknowledged state.
    drop(after);
    let restored =
        AnkiBackend::restore_pre_sync_recovery_as_new_profile(root.path(), "restored", &recovery)
            .unwrap();
    let original_dirty: i64 =
        rusqlite::Connection::open(root.path().join("restored/collection.mdb"))
            .unwrap()
            .query_row(
                "SELECT dirty FROM media WHERE fname='uploaded.txt'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(original_dirty, 1);
    restored.close().unwrap();
}

#[test]
fn synthetic_download_adds_remote_media_and_preserves_original_recovery_state() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
    let before = backend.collection_stats().unwrap();
    let recovery = root.path().join("before-synthetic-download.zip");
    let recovery_at_server = recovery.clone();

    let download_zip = anki::sync::media::zip::zip_files_for_download(vec![(
        "remote.txt".into(),
        b"synthetic downloaded media".to_vec(),
    )])
    .unwrap();
    let routes = [
        (
            "POST /sync/hostKey ",
            br#"{"key":"synthetic-download-key"}"#.to_vec(),
        ),
        (
            "POST /msync/begin ",
            br#"{"data":{"usn":1,"sk":"synthetic-download-key"},"err":""}"#.to_vec(),
        ),
        (
            "POST /msync/mediaChanges ",
            br#"{"data":[["remote.txt",1,"c04f12bd95da303e5861544933e6eca92861fc38"]],"err":""}"#
                .to_vec(),
        ),
        ("POST /msync/downloadFiles ", download_zip),
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
        for (route, payload) in routes {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing media-sync request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(err) => panic!("mock accept failed: {err}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            assert!(
                recovery_at_server.is_file(),
                "recovery must precede network I/O"
            );
            let mut request = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let len = stream.read(&mut buffer).unwrap();
                assert!(len > 0, "incomplete synthetic media-sync request");
                request.extend_from_slice(&buffer[..len]);
                assert!(request.len() < 2 * 1024 * 1024);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let body = &request[end + 4..];
                    if header
                        .to_ascii_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        if body.windows(5).any(|part| part == b"0\r\n\r\n") {
                            break;
                        }
                    } else if let Some(len) = header.lines().find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|text| text.trim().parse::<usize>().ok())
                    }) {
                        if body.len() >= len {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            assert!(
                String::from_utf8_lossy(&request).starts_with(route),
                "wrong media-sync request order"
            );
            let compressed = zstd::stream::encode_all(payload.as_slice(), 1).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nanki-original-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len(),
                compressed.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&compressed).unwrap();
            stream.flush().unwrap();
        }
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
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
        std::fs::read(root.path().join("source/collection.media/remote.txt")).unwrap(),
        b"synthetic downloaded media"
    );
    let media_index =
        rusqlite::Connection::open(root.path().join("source/collection.mdb")).unwrap();
    let (dirty, sync_usn): (i64, i64) = media_index
        .query_row(
            "SELECT media.dirty, meta.lastUsn FROM media, meta WHERE media.fname='remote.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((dirty, sync_usn), (0, 1));
    drop(media_index);

    let restored =
        AnkiBackend::restore_pre_sync_recovery_as_new_profile(root.path(), "restored", &recovery)
            .unwrap();
    assert!(!root
        .path()
        .join("restored/collection.media/remote.txt")
        .exists());
    let count: i64 = rusqlite::Connection::open(root.path().join("restored/collection.mdb"))
        .unwrap()
        .query_row("SELECT count(*) FROM media", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    restored.close().unwrap();
}

#[test]
fn public_media_sync_requires_separate_acknowledgement_of_remote_overwrite_risk() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "owned").unwrap();
    let media = root.path().join("owned/collection.media");
    std::fs::create_dir_all(&media).unwrap();
    std::fs::write(media.join("private.txt"), b"local first-sync media").unwrap();
    let backup = root.path().join("never-created.zip");
    let index = root.path().join("owned/collection.mdb");
    let before = backend.collection_stats().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Permission to write media is not automatic consent to same-name
    // replacement or remote deletion when no first-sync index exists.
    let error = runtime
        .block_on(backend.sync_ankiweb_media_with_recovery(
            "synthetic@example.invalid",
            "synthetic-password",
            &backup,
            true,
            MediaSyncConflictRisk::RefuseUnverifiedRemoteReplacement,
        ))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("separate explicit acknowledgement"),
        "{error}"
    );
    assert!(
        !backup.exists(),
        "do not create a recovery ZIP before risk consent"
    );
    assert!(
        !index.exists(),
        "do not initialize media index before risk consent"
    );
    assert_eq!(before, backend.collection_stats().unwrap());
    assert_eq!(
        std::fs::read(media.join("private.txt")).unwrap(),
        b"local first-sync media"
    );
}

#[test]
fn unapproved_media_sync_does_not_create_an_index_or_recovery_zip() {
    let root = tempdir().unwrap();
    let mut backend = AnkiBackend::create_managed_profile(root.path(), "owned").unwrap();
    let backup = root.path().join("never-created.zip");
    let index = root.path().join("owned/collection.mdb");
    let before = backend.collection_stats().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(runtime
        .block_on(backend.sync_ankiweb_media_with_recovery(
            "user@example.test",
            "synthetic-password",
            &backup,
            false,
            MediaSyncConflictRisk::ExplicitlyAcceptPossibleRemoteReplacement,
        ))
        .is_err());
    assert!(!backup.exists());
    assert!(!index.exists());
    assert_eq!(backend.collection_stats().unwrap(), before);
}
