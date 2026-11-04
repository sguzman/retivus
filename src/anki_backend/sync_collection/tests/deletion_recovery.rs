//! Pin real-server note deletion behavior and receiver recovery on disposable clients.
//! The production consent/preflight path is tested separately.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn upstream_note_deletion_propagates_and_receiver_backup_restores_original() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());
    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Shared baseline").unwrap();
    let note_id = seed
        .add_basic_note(deck, "recoverable question", "recoverable answer")
        .unwrap();
    seed.close().unwrap();

    let baseline = root.path().join("sender/collection.anki2");
    let upstream = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    assert!(!upstream.exists());
    fs::copy(&baseline, &upstream).unwrap();
    let receiver_init =
        AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    receiver_init.close().unwrap();
    fs::copy(&baseline, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    let original = receiver.collection_stats().unwrap();
    assert_eq!(original.notes, 1);
    sender.remove_note(note_id).unwrap();

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let sender_backup = root.path().join("before-tombstone-upload.zip");
    let sent = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(45),
            sender.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &sender_backup,
                client.clone(),
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    assert_eq!(
        sent.expect("upstream note deletion timed out")
            .expect("upstream note deletion failed"),
        CollectionSyncOutcome::UpToDateOrMerged
    );
    assert!(sender_backup.is_file());

    let receiver_backup = root.path().join("before-tombstone-download.zip");
    let received = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(45),
            receiver.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &receiver_backup,
                client,
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    assert_eq!(
        received.expect("upstream deletion download timed out")
            .expect("upstream deletion download failed"),
        CollectionSyncOutcome::UpToDateOrMerged
    );
    assert!(receiver.note_fields(note_id).is_err());
    assert_eq!(receiver.collection_stats().unwrap().notes, 0);
    let restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "receiver-before-deletion",
        &receiver_backup,
    )
    .unwrap();
    assert_eq!(restored.collection_stats().unwrap(), original);
    assert_eq!(
        restored.note_fields(note_id).unwrap(),
        vec!["recoverable question", "recoverable answer"]
    );
    restored.close().unwrap();
}
