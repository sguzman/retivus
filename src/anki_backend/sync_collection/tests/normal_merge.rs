//! A full normal (non-full-sync) collection exchange against the pinned Anki
//! server, using three separate disposable SQLite copies of one baseline.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn normal_collection_merge_transfers_new_note_between_disposable_clients() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());

    // Create a genuine common collection history. Only fresh disposable
    // Retivus-managed fixtures are copied, never any real learner profile.
    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Shared baseline").unwrap();
    seed.add_basic_note(deck, "baseline note", "baseline answer")
        .unwrap();
    let original_stats = seed.collection_stats().unwrap();
    assert_eq!(original_stats.notes, 1);
    seed.close().unwrap();

    let seed_path = root.path().join("sender/collection.anki2");
    let server_col = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    assert!(!server_col.exists(), "server must not have an existing collection");
    fs::copy(&seed_path, &server_col).unwrap();

    let receiver_initial = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    receiver_initial.close().unwrap();
    fs::copy(&seed_path, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    assert_eq!(sender.collection_stats().unwrap(), original_stats);
    assert_eq!(receiver.collection_stats().unwrap(), original_stats);

    // The cloned collections start with identical modification and schema
    // times. A later note edit must require *normal* sync, not full sync.
    std::thread::sleep(Duration::from_millis(25));
    sender
        .add_basic_note(deck, "new note from sender", "transferred answer")
        .unwrap();
    assert_eq!(sender.collection_stats().unwrap().notes, 2);

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let sender_backup = root.path().join("before-normal-collection-upload.zip");
    let sender_outcome = server
        .runtime
        .block_on(async {
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
        })
        .expect("normal upstream upload timed out")
        .expect("normal upstream upload failed");
    assert_eq!(sender_outcome, CollectionSyncOutcome::UpToDateOrMerged);
    assert!(sender_backup.is_file());

    let receiver_backup = root.path().join("before-normal-collection-download.zip");
    let receiver_outcome = server
        .runtime
        .block_on(async {
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
        })
        .expect("normal upstream download timed out")
        .expect("normal upstream download failed");
    assert_eq!(receiver_outcome, CollectionSyncOutcome::UpToDateOrMerged);
    assert!(receiver_backup.is_file());
    assert_eq!(receiver.collection_stats().unwrap().notes, 2);
    assert!(receiver
        .browse_notes("new note from sender", 10)
        .unwrap()
        .iter()
        .any(|note| note.fields.iter().any(|field| field == "new note from sender")));

    // Restore receiver independently to prove its pre-merge backup retained
    // the original state rather than silently accepting the new remote note.
    let mut restored = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "receiver-before-merge",
        &receiver_backup,
    )
    .unwrap();
    assert_eq!(restored.collection_stats().unwrap(), original_stats);
    assert!(restored
        .browse_notes("new note from sender", 10)
        .unwrap()
        .is_empty());
    restored.close().unwrap();
}
