//! Characterize a proven limitation in the pinned Anki collection-sync engine:
//! note edit timestamps have whole-second resolution. Distinct, pending edits
//! with equal timestamps can complete normal sync without content convergence.
//! Verify that private Retivus recovery captures BOTH versions.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

fn modified_second(backend: &AnkiBackend, note_id: i64) -> i64 {
    backend
        .collection
        .storage
        .get_note(anki::notes::NoteId(note_id))
        .unwrap()
        .unwrap()
        .mtime
        .0
}

#[test]
fn equal_timestamp_note_edits_can_diverge_but_both_are_recoverable() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());
    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Same-second edit conflict").unwrap();
    let note_id = seed
        .add_basic_note(deck, "shared question", "original answer")
        .unwrap();
    seed.close().unwrap();

    let original_collection = root.path().join("sender/collection.anki2");
    let server_collection = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    assert!(!server_collection.exists());
    fs::copy(&original_collection, &server_collection).unwrap();
    let empty_receiver = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    empty_receiver.close().unwrap();
    fs::copy(
        &original_collection,
        root.path().join("receiver/collection.anki2"),
    )
    .unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    let baseline_mtime = modified_second(&sender, note_id);
    std::thread::sleep(Duration::from_millis(1_100));
    let (sender_edit, receiver_edit, collision_second) = {
        let mut collision = None;
        for attempt in 0..5 {
            // Start near the beginning of a second, to avoid straddling a
            // timestamp boundary when GitHub Actions is under CPU load.
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_millis();
            if millis > 250 {
                std::thread::sleep(Duration::from_millis(u64::from(1_050 - millis)));
            }
            let sender_text = format!("sender same-second edit {attempt}");
            let receiver_text = format!("receiver same-second edit {attempt}");
            sender
                .edit_note_fields(note_id, &["shared question", &sender_text])
                .unwrap();
            receiver
                .edit_note_fields(note_id, &["shared question", &receiver_text])
                .unwrap();
            let sender_second = modified_second(&sender, note_id);
            let receiver_second = modified_second(&receiver, note_id);
            if sender_second == receiver_second && sender_second > baseline_mtime {
                collision = Some((sender_text, receiver_text, sender_second));
                break;
            }
            std::thread::sleep(Duration::from_millis(1_100));
        }
        collision.expect("could not establish two note edits with equal timestamp seconds")
    };
    assert_eq!(modified_second(&receiver, note_id), collision_second);

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let sender_archive = root.path().join("sender-pre-collision-sync.zip");
    let receiver_archive = root.path().join("receiver-pre-collision-sync.zip");
    let followup_archive = root.path().join("sender-second-sync.zip");
    let sync = |backend: &mut AnkiBackend, recovery_zip: &std::path::Path| {
        let outcome = server.runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(45),
                backend.sync_ankiweb_collection_with_client(
                    server.username,
                    server.password,
                    recovery_zip,
                    client.clone(),
                    Some(server.endpoint.clone()),
                ),
            )
            .await
        });
        assert_eq!(
            outcome
                .expect("same-second upstream collection sync timed out")
                .expect("same-second upstream collection sync failed"),
            CollectionSyncOutcome::UpToDateOrMerged
        );
        assert!(recovery_zip.is_file());
    };
    sync(&mut sender, &sender_archive);
    sync(&mut receiver, &receiver_archive);
    sync(&mut sender, &followup_archive);

    // Characterization: the upstream completed result alone is *not* proof
    // of convergence on equal-mtime, simultaneously pending note edits.
    let final_sender = sender.note_fields(note_id).unwrap();
    let final_receiver = receiver.note_fields(note_id).unwrap();
    assert_ne!(
        final_sender, final_receiver,
        "pinned Anki equal-timestamp collision behavior changed: inspect safety guarantee"
    );
    assert_eq!(final_sender[0], "shared question");
    assert_eq!(final_receiver[0], "shared question");

    // This is not a remedy for remote divergence. It only proves that neither
    // original local edit is irretrievably lost from the recovery archives.
    let restored_sender = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-sender-collision",
        &sender_archive,
    )
    .unwrap();
    let restored_receiver = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-receiver-collision",
        &receiver_archive,
    )
    .unwrap();
    assert_eq!(
        restored_sender.note_fields(note_id).unwrap()[1],
        sender_edit
    );
    assert_eq!(
        restored_receiver.note_fields(note_id).unwrap()[1],
        receiver_edit
    );
    restored_sender.close().unwrap();
    restored_receiver.close().unwrap();
}
