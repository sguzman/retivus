//! Characterize two real Anki scheduler grades of the same shared card
//! through the pinned disposable upstream sync server. Recovery is guaranteed
//! independently of whether the upstream normal-sync merge converges.
use super::*;
use crate::anki_backend::sync_test_server::DisposableAnkiServer;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

fn card_schedule(backend: &AnkiBackend, card_id: i64) -> (i64, i64, i64, i64) {
    backend
        .collection
        .storage
        .db()
        .query_row(
            "SELECT type, queue, ivl, reps FROM cards WHERE id=?1",
            [card_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
}

fn original_review_rating(backend: &AnkiBackend, card_id: i64) -> i64 {
    backend
        .collection
        .storage
        .db()
        .query_row(
            "SELECT ease FROM revlog WHERE cid=?1 ORDER BY id LIMIT 1",
            [card_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn competing_real_card_grades_keep_both_private_pre_sync_histories() {
    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());

    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Shared review-grade conflict").unwrap();
    let note_id = seed
        .add_basic_note(deck, "what did we review?", "two private answers")
        .unwrap();
    seed.close().unwrap();

    let baseline = root.path().join("sender/collection.anki2");
    let server_collection = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    assert!(!server_collection.exists());
    fs::copy(&baseline, &server_collection).unwrap();

    let empty_receiver = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    empty_receiver.close().unwrap();
    fs::copy(&baseline, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    // Use the pinned scheduler, not a hand-edited SQLite scheduling row.
    let sender_card = sender.grade_next_card(4).unwrap().unwrap();
    let receiver_card = receiver.grade_next_card(1).unwrap().unwrap();
    assert_eq!(sender_card, receiver_card);
    assert_eq!(sender.review_event_count(sender_card).unwrap(), 1);
    assert_eq!(receiver.review_event_count(sender_card).unwrap(), 1);
    assert_eq!(original_review_rating(&sender, sender_card), 4);
    assert_eq!(original_review_rating(&receiver, receiver_card), 1);
    let sender_schedule = card_schedule(&sender, sender_card);
    let receiver_schedule = card_schedule(&receiver, receiver_card);
    assert_ne!(
        sender_schedule, receiver_schedule,
        "different native ratings must produce distinguishable review states"
    );

    // The backend-only client method is used here solely for a local pinned
    // server. Production sync requires separate explicit permission and
    // collision-risk acknowledgement; no real AnkiWeb account is accessed.
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let sender_backup = root.path().join("sender-before-card-grade-merge.zip");
    let receiver_backup = root.path().join("receiver-before-card-grade-merge.zip");
    let sync = |backend: &mut AnkiBackend, backup: &std::path::Path| {
        let outcome = server.runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(45),
                backend.sync_ankiweb_collection_with_client(
                    server.username,
                    server.password,
                    backup,
                    client.clone(),
                    Some(server.endpoint.clone()),
                ),
            )
            .await
        });
        assert_eq!(
            outcome
                .expect("competing review sync timed out")
                .expect("competing review sync failed"),
            CollectionSyncOutcome::UpToDateOrMerged
        );
        assert!(backup.is_file(), "the private pre-sync copy is required");
    };
    sync(&mut sender, &sender_backup);
    sync(&mut receiver, &receiver_backup);
    // One more normal exchange does not itself prove card-state convergence,
    // but exercises both clients' own changes and the upstream merge path.
    sync(
        &mut sender,
        &root.path().join("sender-after-competing-card-sync.zip"),
    );

    assert_eq!(sender.note_fields(note_id).unwrap()[1], "two private answers");
    assert_eq!(receiver.note_fields(note_id).unwrap()[1], "two private answers");

    // Independently restore each ORIGINAL pre-sync review history. An upstream
    // winner, lost update, or ambiguous same-second grade cannot erase these
    // private recovery copies. No claim is made about remote rollback.
    let restored_sender = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-sender-card-grade",
        &sender_backup,
    )
    .unwrap();
    let restored_receiver = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
        root.path(),
        "restored-receiver-card-grade",
        &receiver_backup,
    )
    .unwrap();
    assert_eq!(restored_sender.review_event_count(sender_card).unwrap(), 1);
    assert_eq!(restored_receiver.review_event_count(receiver_card).unwrap(), 1);
    assert_eq!(original_review_rating(&restored_sender, sender_card), 4);
    assert_eq!(original_review_rating(&restored_receiver, receiver_card), 1);
    assert_eq!(card_schedule(&restored_sender, sender_card), sender_schedule);
    assert_eq!(card_schedule(&restored_receiver, receiver_card), receiver_schedule);
    restored_sender.close().unwrap();
    restored_receiver.close().unwrap();
}
