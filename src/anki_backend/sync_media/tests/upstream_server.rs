//! Cross-client media sync against the *actual pinned Anki Rust sync server*.
//! No AnkiWeb account, external listener, or user profile is involved.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn two_disposable_retivus_profiles_exchange_media_via_pinned_upstream_sync_server() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());
    let username = server.username;
    let password = server.password;
    let runtime = &server.runtime;
    let endpoint = server.endpoint.clone();

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut sender = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let media = sender.collection.media().unwrap();
    media
        .add_file("transferred.txt", b"real upstream server media")
        .unwrap();
    drop(media);
    let sender_before = sender.collection_stats().unwrap();
    let sender_backup = root.path().join("sender-before-real-server.zip");

    runtime
        .block_on(async {
            tokio::time::timeout(
                Duration::from_secs(40),
                sender.sync_ankiweb_media_with_client(
                    username,
                    password,
                    &sender_backup,
                    client.clone(),
                    Some(endpoint.clone()),
                ),
            )
            .await
        })
        .expect("upstream upload timed out")
        .expect("upstream upload failed");
    assert!(sender_backup.is_file());
    assert_eq!(sender.collection_stats().unwrap(), sender_before);
    let sender_index =
        rusqlite::Connection::open(root.path().join("sender/collection.mdb")).unwrap();
    let source_dirty: i64 = sender_index
        .query_row(
            "SELECT dirty FROM media WHERE fname='transferred.txt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_dirty, 0);
    drop(sender_index);

    let mut receiver = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    let receiver_before = receiver.collection_stats().unwrap();
    let receiver_backup = root.path().join("receiver-before-real-server.zip");
    runtime
        .block_on(async {
            tokio::time::timeout(
                Duration::from_secs(40),
                receiver.sync_ankiweb_media_with_client(
                    username,
                    password,
                    &receiver_backup,
                    client,
                    Some(endpoint),
                ),
            )
            .await
        })
        .expect("upstream download timed out")
        .expect("upstream download failed");
    assert!(receiver_backup.is_file());
    assert_eq!(receiver.collection_stats().unwrap(), receiver_before);
    assert_eq!(
        std::fs::read(
            root.path()
                .join("receiver/collection.media/transferred.txt")
        )
        .unwrap(),
        b"real upstream server media"
    );
    let receiver_index =
        rusqlite::Connection::open(root.path().join("receiver/collection.mdb")).unwrap();
    let (dirty, count): (i64, i64) = receiver_index
        .query_row(
            "SELECT media.dirty, (SELECT count(*) FROM media) FROM media WHERE fname='transferred.txt'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((dirty, count), (0, 1));
    drop(receiver_index);

    // The receiver's pre-sync recovery archive remains a usable empty state.
    let restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "receiver-pre-sync",
        &receiver_backup,
    )
    .unwrap();
    assert!(!root
        .path()
        .join("receiver-pre-sync/collection.media/transferred.txt")
        .exists());
    restored.close().unwrap();

}
