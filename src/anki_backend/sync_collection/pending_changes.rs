//! Conservative, read-only production preflight for local Anki changes
//! recorded with one-second modification timestamp resolution.
//!
//! This prevents an immediate sync while *this* profile has very fresh pending
//! notes or cards. It also rejects older pending notes and note/card/deck deletion
//! tombstones whose IDs predate the collection's last successful sync,
//! conservatively treating them as changes to possibly shared objects.
//! Some imported or preexisting unsynced changes will be blocked too.
//! These checks cannot discover remote-only edits, prove convergence,
//! or repair conflicts.
//! The known risk still needs separate consent.
use super::{AnkiBackend, BackendResult};
use std::time::{SystemTime, UNIX_EPOCH};

/// Treat the current second and the two previous seconds as too recent.
/// This conservative margin covers whole-second timestamp truncation.
const COLLISION_WINDOW_SECONDS: i64 = 2;

/// Must run before any archive, login or HTTP request in the public sync path.
pub(super) fn refuse_risky_pending_note_changes(backend: &AnkiBackend) -> BackendResult<()> {
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    refuse_risky_pending_note_changes_at(backend, now)
}

/// Inject a clock for deterministic boundary tests. Always query the active
/// Anki collection connection, never open a second SQLite writer/reader.
fn refuse_risky_pending_note_changes_at(backend: &AnkiBackend, now: i64) -> BackendResult<()> {
    let cutoff = now.saturating_sub(COLLISION_WINDOW_SECONDS);
    let has_recent: bool = backend.collection.storage.db().query_row(
        "SELECT EXISTS(SELECT 1 FROM notes WHERE usn = -1 AND mod >= ?1)",
        [cutoff],
        |row| row.get(0),
    )?;
    if has_recent {
        return Err(
            "collection sync refused: recently edited unsynced notes have whole-second modification timestamps; retry after the local collision window, while still acknowledging possible remote same-second divergence".into(),
        );
    }

    // The pinned Anki schema stores col.ls as a millisecond timestamp of the
    // last sync. Ordinary note IDs are generated from millisecond timestamps.
    // A pending note that already existed by that sync may represent an edit
    // to shared data. Waiting two seconds cannot make that edit conflict-safe.
    //
    // This deliberately errs on the side of blocking an older unsynced note
    // with an imported/custom ID or one left pending after an earlier sync.
    // It is NOT proof of the server's contents, nor a reliable way to identify
    // every previously shared note (some imported IDs are nonchronological).
    let has_possibly_shared_edit: bool = backend.collection.storage.db().query_row(
        "SELECT EXISTS(
            SELECT 1 FROM notes, col
            WHERE notes.usn = -1 AND col.ls > 0 AND notes.id <= col.ls
        )",
        [],
        |row| row.get(0),
    )?;
    if has_possibly_shared_edit {
        return Err(
            "collection sync refused: pending notes predate the last recorded sync and may overwrite independently edited shared notes; manual conflict-safe reconciliation is required, not just waiting".into(),
        );
    }

    // Reviewed cards carry second-resolution modification timestamps too.
    // A recently reviewed unsynced card may race with another client's
    // scheduling edits; the earlier note preflight cannot see card-only
    // changes. Waiting beyond the local window does NOT prove that a remote
    // review or scheduling change is absent.
    let has_recent_card: bool = backend.collection.storage.db().query_row(
        "SELECT EXISTS(SELECT 1 FROM cards WHERE usn = -1 AND mod >= ?1)",
        [cutoff],
        |row| row.get(0),
    )?;
    if has_recent_card {
        return Err(
            "collection sync refused: recently changed unsynced cards have whole-second modification timestamps; retry after the local collision window, while still acknowledging possible remote review divergence".into(),
        );
    }

    // The pinned Anki engine stores deleted notes in graves (type=1). A
    // deletion of a note that might already be shared is just as risky as an
    // edit: normal sync can send this tombstone without comparing the remote
    // note's content. There is no live notes row for the above check to see.
    // This is deliberately conservative for imported or nonchronological IDs.
    let has_possibly_shared_deletion: bool = backend.collection.storage.db().query_row(
        "SELECT EXISTS(
            SELECT 1 FROM graves, col
            WHERE graves.usn = -1 AND graves.type = 1
              AND col.ls > 0 AND graves.oid <= col.ls
        )",
        [],
        |row| row.get(0),
    )?;
    if has_possibly_shared_deletion {
        return Err(
            "collection sync refused: pending note deletions predate the last recorded sync and may erase independently edited shared notes; manual conflict-safe reconciliation is required".into(),
        );
    }

    // The pinned Anki grave table also holds deleted cards (type=0) and
    // decks (type=2). Unlike notes, these tombstones have no live row to
    // examine, and deleting a previously shared card or deck can discard
    // remotely changed scheduling or collection structure.
    //
    // An ordinary ID is millisecond-time based, but imported and custom IDs
    // can yield false positives and nonchronological false negatives. Fail
    // closed for potentially shared IDs; this is not remote state validation.
    let has_possibly_shared_card_or_deck_deletion: bool =
        backend.collection.storage.db().query_row(
            "SELECT EXISTS(
                SELECT 1 FROM graves, col
                WHERE graves.usn = -1 AND graves.type IN (0, 2)
                  AND col.ls > 0 AND graves.oid <= col.ls
            )",
            [],
            |row| row.get(0),
        )?;
    if has_possibly_shared_card_or_deck_deletion {
        return Err(
            "collection sync refused: pending card or deck deletions predate the last recorded sync and may erase previously shared study or deck data; manual conflict-safe reconciliation is required".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anki_backend::sync_collection::CollectionSyncConflictRisk;
    use tempfile::tempdir;

    #[test]
    fn recent_pending_note_window_is_exact_and_deterministic() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Unsynced note").unwrap();
        let note_id = backend
            .add_basic_note(deck, "note front", "private back")
            .unwrap();
        let note = backend
            .collection
            .storage
            .get_note(anki::notes::NoteId(note_id))
            .unwrap()
            .unwrap();
        assert_eq!(note.usn.0, -1, "new note must be pending upload");
        let modified = note.mtime.0;

        assert!(refuse_risky_pending_note_changes_at(&backend, modified).is_err());
        assert!(refuse_risky_pending_note_changes_at(&backend, modified + 1).is_err());
        assert!(refuse_risky_pending_note_changes_at(&backend, modified + 2).is_err());
        assert!(refuse_risky_pending_note_changes_at(&backend, modified + 3).is_ok());

        // The guard only considers unsynced notes. The clean fixture still
        // stays in the same managed profile, but it is no longer pending.
        let updated = backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        assert_eq!(updated, 1);
        // This fixture also created an unsynced card. Its own coarse
        // timestamp must still block a same-second collection sync.
        let error = refuse_risky_pending_note_changes_at(&backend, modified)
            .unwrap_err()
            .to_string();
        assert!(error.contains("recently changed unsynced cards"), "{error}");
        let card_updates = backend
            .collection
            .storage
            .db()
            .execute("UPDATE cards SET usn=0 WHERE nid=?1", [note_id])
            .unwrap();
        assert!(card_updates > 0);
        assert!(refuse_risky_pending_note_changes_at(&backend, modified).is_ok());
    }

    #[test]
    fn recent_pending_card_window_is_exact_and_clean_cards_are_allowed() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Card timing").unwrap();
        let note_id = backend
            .add_basic_note(deck, "recent card", "answer")
            .unwrap();
        // Isolate the card hazard from the pending-note guard, as happens
        // after a card review on an otherwise already-synced note.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        let (card_id, modified, usn): (i64, i64, i64) = backend
            .collection
            .storage
            .db()
            .query_row(
                "SELECT id, mod, usn FROM cards WHERE nid=?1 LIMIT 1",
                [note_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(usn, -1, "new card must be pending");
        for offset in 0..=2 {
            let error = refuse_risky_pending_note_changes_at(&backend, modified + offset)
                .unwrap_err()
                .to_string();
            assert!(error.contains("recently changed unsynced cards"), "{error}");
        }
        refuse_risky_pending_note_changes_at(&backend, modified + 3).unwrap();

        backend
            .collection
            .storage
            .db()
            .execute("UPDATE cards SET usn=0 WHERE id=?1", [card_id])
            .unwrap();
        refuse_risky_pending_note_changes_at(&backend, modified).unwrap();
    }

    #[test]
    fn recently_changed_card_refuses_public_sync_before_backup_or_login() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Card only pending").unwrap();
        let note_id = backend
            .add_basic_note(deck, "keep private", "original answer")
            .unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        let before = backend.collection_stats().unwrap();
        let archive = dir.path().join("must-not-create.zip");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = rt
            .block_on(backend.sync_ankiweb_collection_with_recovery(
                "fixture@example.invalid",
                "test-only-password",
                &archive,
                true,
                CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains("recently changed unsynced cards"), "{error}");
        assert!(!archive.exists());
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "original answer");
    }

    #[test]
    fn older_pending_shared_notes_fail_closed_beyond_the_recent_edit_window() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Potential shared conflict").unwrap();
        let note_id = backend
            .add_basic_note(deck, "shared question", "private pending edit")
            .unwrap();
        let modified = backend
            .collection
            .storage
            .get_note(anki::notes::NoteId(note_id))
            .unwrap()
            .unwrap()
            .mtime
            .0;
        // Establish a synthetic prior sync after this note's creation. The
        // note remains pending, just as if it had been edited since sync.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [note_id + 10_000])
            .unwrap();
        let error = refuse_risky_pending_note_changes_at(&backend, modified + 60)
            .unwrap_err()
            .to_string();
        assert!(error.contains("predate the last recorded sync"), "{error}");

        // A clean note must not trigger the guard, regardless of its age.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        refuse_risky_pending_note_changes_at(&backend, modified + 60).unwrap();

        // A newly created pending note after the last-sync marker is not
        // blocked merely because it has been waiting for several seconds.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [note_id - 10_000])
            .unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=-1 WHERE id=?1", [note_id])
            .unwrap();
        refuse_risky_pending_note_changes_at(&backend, modified + 60).unwrap();
    }

    #[test]
    fn older_pending_shared_note_stops_public_sync_before_backup_or_login() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Old divergent note").unwrap();
        let note_id = backend
            .add_basic_note(deck, "private front", "private answer")
            .unwrap();
        let before = backend.collection_stats().unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [note_id + 10_000])
            .unwrap();
        // Avoid waiting for wall-clock time: advance the note's timestamp
        // backwards only in this disposable fixture to model an old edit.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET mod=1 WHERE id=?1", [note_id])
            .unwrap();

        let output = dir.path().join("must-not-create.zip");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = rt
            .block_on(backend.sync_ankiweb_collection_with_recovery(
                "fixture@example.invalid",
                "test-only-password",
                &output,
                true,
                CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains("predate the last recorded sync"), "{error}");
        assert!(!output.exists());
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "private answer");
    }

    #[test]
    fn older_pending_note_deletion_blocks_sync_without_backup_or_network() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Shared note removal").unwrap();
        let note_id = backend
            .add_basic_note(deck, "remote-shared question", "private answer")
            .unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [note_id + 10_000])
            .unwrap();
        backend.remove_note(note_id).unwrap();
        let pending_grave: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "SELECT COUNT(*) FROM graves WHERE oid=?1 AND type=1 AND usn=-1",
                [note_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_grave, 1, "pinned Anki must retain a note tombstone");
        let before = backend.collection_stats().unwrap();
        let recovery = dir.path().join("never-written.zip");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = rt
            .block_on(backend.sync_ankiweb_collection_with_recovery(
                "fixture@example.invalid",
                "test-only-password",
                &recovery,
                true,
                CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains("pending note deletions"), "{error}");
        assert!(!recovery.exists(), "must refuse before backup or login");
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert!(backend.note_fields(note_id).is_err());
        backend.undo_last().unwrap();
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "private answer");
    }

    #[test]
    fn deletion_of_only_new_unsynced_note_does_not_trigger_shared_note_guard() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let deck = backend.create_deck("Never uploaded").unwrap();
        let note_id = backend
            .add_basic_note(deck, "new local question", "new local answer")
            .unwrap();
        // Model a previous sync that predates the new note's time-based ID.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [note_id - 10_000])
            .unwrap();
        backend.remove_note(note_id).unwrap();
        let pending_grave: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "SELECT COUNT(*) FROM graves WHERE oid=?1 AND type=1 AND usn=-1",
                [note_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_grave, 1);
        refuse_risky_pending_note_changes_at(&backend, 1_900_000_000).unwrap();

        // Without any recorded sync, the same tombstone cannot represent
        // an identified previously synced remote note.
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=0", [])
            .unwrap();
        refuse_risky_pending_note_changes_at(&backend, 1_900_000_000).unwrap();
    }

    #[test]
    fn older_pending_card_and_deck_deletions_fail_closed_before_backup_or_login() {
        for kind in [0, 2] {
            let dir = tempdir().unwrap();
            let mut backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
            let existing_id = 1_600_000_000_000_i64 + i64::from(kind);
            backend
                .collection
                .storage
                .db()
                .execute("UPDATE col SET ls=?1", [existing_id + 10_000])
                .unwrap();
            // Simulate the exact pinned grave schema for a previously shared
            // card or deck without mutating a real user profile.
            backend
                .collection
                .storage
                .db()
                .execute(
                    "INSERT INTO graves (usn, oid, type) VALUES (-1, ?1, ?2)",
                    rusqlite::params![existing_id, kind],
                )
                .unwrap();
            let before = backend.collection_stats().unwrap();
            let recovery = dir.path().join("never-written.zip");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let error = rt
                .block_on(backend.sync_ankiweb_collection_with_recovery(
                    "fixture@example.invalid",
                    "test-only-password",
                    &recovery,
                    true,
                    CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
                ))
                .unwrap_err()
                .to_string();
            assert!(error.contains("pending card or deck deletions"), "{error}");
            assert!(!recovery.exists(), "no backup or network before refusal");
            assert_eq!(backend.collection_stats().unwrap(), before);
        }
    }

    #[test]
    fn new_unshared_card_deck_tombstones_and_clean_historical_graves_pass_preflight() {
        let dir = tempdir().unwrap();
        let backend = AnkiBackend::create_managed_profile(dir.path(), "client").unwrap();
        let cutoff = 1_700_000_000_000_i64;
        backend
            .collection
            .storage
            .db()
            .execute("UPDATE col SET ls=?1", [cutoff])
            .unwrap();
        for kind in [0, 2] {
            // Newer, locally created objects are not presumed shared.
            backend
                .collection
                .storage
                .db()
                .execute(
                    "INSERT INTO graves (usn, oid, type) VALUES (-1, ?1, ?2)",
                    rusqlite::params![cutoff + 10_000 + i64::from(kind), kind],
                )
                .unwrap();
            // A completed, non-pending grave should not trip a fresh sync.
            backend
                .collection
                .storage
                .db()
                .execute(
                    "INSERT INTO graves (usn, oid, type) VALUES (0, ?1, ?2)",
                    rusqlite::params![cutoff - 10_000 - i64::from(kind), kind],
                )
                .unwrap();
        }
        refuse_risky_pending_note_changes_at(&backend, 1_900_000_000).unwrap();
    }

    #[test]
    fn live_collection_entry_point_blocks_fresh_pending_note_before_backup_or_network() {
        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "local").unwrap();
        let deck = backend.create_deck("Private data").unwrap();
        let note_id = backend
            .add_basic_note(deck, "do not upload", "private history")
            .unwrap();
        let before = backend.collection_stats().unwrap();
        let archive = dir.path().join("must-not-create.zip");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(backend.sync_ankiweb_collection_with_recovery(
            "fixture@example.invalid",
            "test-only-password",
            &archive,
            true,
            CollectionSyncConflictRisk::ExplicitlyAcceptPossibleSameSecondDivergence,
        ));

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("recently edited unsynced notes"));
        assert!(!archive.exists(), "no recovery file before accepted sync");
        assert_eq!(backend.collection_stats().unwrap(), before);
        assert_eq!(backend.note_fields(note_id).unwrap()[1], "private history");
    }

    #[test]
    fn clean_and_empty_profiles_can_pass_local_only_preflight() {
        let dir = tempdir().unwrap();
        let backend = AnkiBackend::create_managed_profile(dir.path(), "empty").unwrap();
        refuse_risky_pending_note_changes_at(&backend, 1_800_000_000).unwrap();
    }
}
