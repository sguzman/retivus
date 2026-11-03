//! Two independent clients edit the same collection note against the pinned
//! Anki Rust sync server. Assert convergence and private pre-merge recovery,
//! without assuming a winner or falsely promising conflict-free merging.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn simultaneous_note_edits_converge_and_both_original_edits_remain_recoverable() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());

    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Shared note").unwrap();
    let note_id = seed
        .add_basic_note(deck, "shared question", "original answer")
        .unwrap();
    let original = seed.collection_stats().unwrap();
    assert_eq!(original.notes, 1);
    seed.close().unwrap();

    let base = root.path().join("sender/collection.anki2");
    let server_collection = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    assert!(!server_collection.exists());
    fs::copy(&base, &server_collection).unwrap();
    let receiver_init = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    receiver_init.close().unwrap();
    // Replace only the collection of a brand-new disposable test profile.
    fs::copy(&base, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    std::thread::sleep(Duration::from_millis(25));
    sender
        .edit_note_fields(note_id, &["shared question", "sender private edit"])
        .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    receiver
        .edit_note_fields(note_id, &["shared question", "receiver private edit"])
        .unwrap();
    assert_eq!(sender.note_fields(note_id).unwrap()[1], "sender private edit");
    assert_eq!(
        receiver.note_fields(note_id).unwrap()[1],
        "receiver private edit"
    );

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let backup_sender = root.path().join("before-sender-conflict-sync.zip");
    let send = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(45),
            sender.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &backup_sender,
                client.clone(),
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    assert_eq!(
        send.expect("first collection sync timed out")
            .expect("first collection sync failed"),
        CollectionSyncOutcome::UpToDateOrMerged
    );

    let backup_receiver = root.path().join("before-receiver-conflict-sync.zip");
    let receive = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(45),
            receiver.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &backup_receiver,
                client.clone(),
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    assert_eq!(
        receive
            .expect("second collection sync timed out")
            .expect("second collection sync failed"),
        CollectionSyncOutcome::UpToDateOrMerged
    );

    let backup_sender_again = root.path().join("before-sender-convergence.zip");
    let followup = server.runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(45),
            sender.sync_ankiweb_collection_with_client(
                server.username,
                server.password,
                &backup_sender_again,
                client,
                Some(server.endpoint.clone()),
            ),
        )
        .await
    });
    assert_eq!(
        followup
            .expect("convergence sync timed out")
            .expect("convergence sync failed"),
        CollectionSyncOutcome::UpToDateOrMerged
    );
    assert!(backup_sender_again.is_file());
    let resulting_sender = sender.note_fields(note_id).unwrap();
    let resulting_receiver = receiver.note_fields(note_id).unwrap();
    assert_eq!(resulting_sender, resulting_receiver);
    assert_eq!(resulting_sender[0], "shared question");
    assert!(
        resulting_sender[1] == "sender private edit"
            || resulting_sender[1] == "receiver private edit",
        "actual server must converge to one of the two author edits"
    );
    assert_eq!(sender.collection_stats().unwrap(), original);
    assert_eq!(receiver.collection_stats().unwrap(), original);

    // Both contenders retain independently recoverable original content even
    // if the upstream merge chose to overwrite one client-side note edit.
    let restored_sender = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-sender-conflict",
        &backup_sender,
    )
    .unwrap();
    let restored_receiver = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-receiver-conflict",
        &backup_receiver,
    )
    .unwrap();
    assert_eq!(
        restored_sender.note_fields(note_id).unwrap()[1],
        "sender private edit"
    );
    assert_eq!(
        restored_receiver.note_fields(note_id).unwrap()[1],
        "receiver private edit"
    );
    restored_sender.close().unwrap();
    restored_receiver.close().unwrap();
}
