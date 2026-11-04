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
    let receiver_init = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
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
        received
            .expect("upstream deletion download timed out")
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

#[test]
fn recently_reviewed_shared_card_after_real_server_sync_refuses_before_login() {
    use crate::anki_backend::CollectionSyncConflictRisk;

    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());
    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    seed.enable_fsrs().unwrap();
    seed.add_basic_note(1, "shared review question", "original answer")
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
    let empty_receiver = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    empty_receiver.close().unwrap();
    fs::copy(&baseline, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    // Create a real, nonempty normal-sync exchange rather than fabricating
    // col.ls. Both collections will have a successful upstream sync history.
    sender
        .add_basic_note(1, "additional shared question", "shared answer")
        .unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (backend, name) in [
        (&mut sender, "initial-review-sender.zip"),
        (&mut receiver, "initial-review-receiver.zip"),
    ] {
        let result = server.runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(45),
                backend.sync_ankiweb_collection_with_client(
                    server.username,
                    server.password,
                    &root.path().join(name),
                    client.clone(),
                    Some(server.endpoint.clone()),
                ),
            )
            .await
        });
        assert_eq!(
            result
                .expect("baseline review sync timed out")
                .expect("baseline review sync failed"),
            CollectionSyncOutcome::UpToDateOrMerged
        );
    }
    assert!(receiver.fsrs_enabled());
    let card_id = receiver
        .grade_next_card(4)
        .unwrap()
        .expect("a synced default-deck card must be due");
    assert_eq!(receiver.review_event_count(card_id).unwrap(), 1);
    let (card_usn, note_usn, last_sync): (i64, i64, i64) = receiver
        .collection
        .storage
        .db()
        .query_row(
            "SELECT cards.usn, notes.usn, col.ls
             FROM cards JOIN notes ON notes.id=cards.nid CROSS JOIN col
             WHERE cards.id=?1",
            [card_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(card_usn, -1, "native grade must make the card pending");
    assert!(note_usn >= 0, "the shared note itself remains clean");
    assert!(
        last_sync > card_id,
        "the reviewed card predates a real successful upstream sync"
    );

    let before = receiver.collection_stats().unwrap();
    let rejected_archive = root.path().join("never-write-review-archive.zip");
    let error = server
        .runtime
        .block_on(receiver.sync_ankiweb_collection_with_recovery(
            "synthetic@example.invalid",
            "synthetic-password",
            &rejected_archive,
            true,
            CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
        ))
        .unwrap_err()
        .to_string();
    assert!(error.contains("recently changed unsynced cards"), "{error}");
    assert!(
        !rejected_archive.exists(),
        "the public path must refuse before backup and login"
    );
    assert_eq!(receiver.collection_stats().unwrap(), before);
    assert_eq!(receiver.review_event_count(card_id).unwrap(), 1);
}

#[test]
fn previously_synced_shared_note_edit_and_deletion_fail_closed_before_login() {
    use crate::anki_backend::CollectionSyncConflictRisk;

    let root = tempdir().unwrap();
    let server = DisposableAnkiServer::start(root.path());
    let mut seed = AnkiBackend::create_managed_profile(root.path(), "sender").unwrap();
    let deck = seed.create_deck("Verified shared baseline").unwrap();
    let shared_id = seed
        .add_basic_note(deck, "shared question", "original answer")
        .unwrap();
    seed.close().unwrap();

    let baseline = root.path().join("sender/collection.anki2");
    let upstream = root
        .path()
        .join("upstream-server")
        .join(server.username)
        .join("collection.anki2");
    fs::copy(&baseline, &upstream).unwrap();
    let receiver_initial = AnkiBackend::create_managed_profile(root.path(), "receiver").unwrap();
    receiver_initial.close().unwrap();
    fs::copy(&baseline, root.path().join("receiver/collection.anki2")).unwrap();

    let mut sender = AnkiBackend::open_managed_profile(root.path(), "sender").unwrap();
    let mut receiver = AnkiBackend::open_managed_profile(root.path(), "receiver").unwrap();
    // Establish actual, successful normal sync history for both clients.
    // Unlike the isolated unit fixtures, col.ls is written by upstream Anki.
    sender
        .add_basic_note(deck, "trigger ordinary sync", "independent card")
        .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    for (backend, archive) in [
        (&mut sender, "baseline-sender.zip"),
        (&mut receiver, "baseline-receiver.zip"),
    ] {
        let result = server.runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(45),
                backend.sync_ankiweb_collection_with_client(
                    server.username,
                    server.password,
                    &root.path().join(archive),
                    http.clone(),
                    Some(server.endpoint.clone()),
                ),
            )
            .await
        });
        assert_eq!(
            result
                .expect("baseline upstream sync timed out")
                .expect("baseline upstream sync failed"),
            CollectionSyncOutcome::UpToDateOrMerged
        );
    }
    for backend in [&sender, &receiver] {
        let last_sync: i64 = backend
            .collection
            .storage
            .db()
            .query_row("SELECT ls FROM col", [], |row| row.get(0))
            .unwrap();
        assert!(
            last_sync > shared_id,
            "shared note must predate an actual upstream last-sync timestamp"
        );
    }

    // Independently edit and delete the same formerly synchronized note.
    // Age the edited note timestamp only in this disposable fixture to
    // ensure we exercise the older-shared-note guard, not the freshness guard.
    receiver
        .edit_note_fields(shared_id, &["shared question", "unsynced private revision"])
        .unwrap();
    receiver
        .collection
        .storage
        .db()
        .execute("UPDATE notes SET mod=1 WHERE id=?1", [shared_id])
        .unwrap();
    sender.remove_note(shared_id).unwrap();
    let sender_before = sender.collection_stats().unwrap();
    let receiver_before = receiver.collection_stats().unwrap();
    let deletion_archive = root.path().join("refused-deletion.zip");
    let edit_archive = root.path().join("refused-edit.zip");
    let rejection = |backend: &mut AnkiBackend, archive: &std::path::Path| {
        server
            .runtime
            .block_on(backend.sync_ankiweb_collection_with_recovery(
                "synthetic@example.invalid",
                "synthetic-password",
                archive,
                true,
                CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
            ))
            .unwrap_err()
            .to_string()
    };
    let deletion_error = rejection(&mut sender, &deletion_archive);
    assert!(
        deletion_error.contains("pending note deletions"),
        "{deletion_error}"
    );
    let edit_error = rejection(&mut receiver, &edit_archive);
    assert!(edit_error.contains("pending notes predate"), "{edit_error}");
    assert!(!deletion_archive.exists());
    assert!(!edit_archive.exists());
    assert_eq!(sender.collection_stats().unwrap(), sender_before);
    assert_eq!(receiver.collection_stats().unwrap(), receiver_before);
    assert!(sender.note_fields(shared_id).is_err());
    assert_eq!(
        receiver.note_fields(shared_id).unwrap()[1],
        "unsynced private revision"
    );
}
