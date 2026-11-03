//! Conservative, read-only production preflight for local note changes that
//! were recorded in Anki's one-second modification timestamp resolution.
//!
//! This prevents an immediate sync while *this* profile has very fresh pending
//! notes. It also rejects older pending notes whose IDs predate the collection's
//! last successful sync, conservatively treating them as possibly edited shared
//! notes. Some imported or preexisting unsynced notes will be blocked too.
//! Neither check can discover remote-only edits, prove convergence, or repair
//! conflicts. The known risk still needs separate consent.
use super::{AnkiBackend, BackendResult};
use std::time::{SystemTime, UNIX_EPOCH};

/// Treat the current second and the two previous seconds as too recent.
/// This conservative margin covers whole-second timestamp truncation.
const COLLISION_WINDOW_SECONDS: i64 = 2;

/// Must run before any archive, login or HTTP request in the public sync path.
pub(super) fn refuse_fresh_pending_notes(backend: &AnkiBackend) -> BackendResult<()> {
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    refuse_fresh_pending_notes_at(backend, now)
}

/// Inject a clock for deterministic boundary tests. Always query the active
/// Anki collection connection, never open a second SQLite writer/reader.
fn refuse_fresh_pending_notes_at(backend: &AnkiBackend, now: i64) -> BackendResult<()> {
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

        assert!(refuse_fresh_pending_notes_at(&backend, modified).is_err());
        assert!(refuse_fresh_pending_notes_at(&backend, modified + 1).is_err());
        assert!(refuse_fresh_pending_notes_at(&backend, modified + 2).is_err());
        assert!(refuse_fresh_pending_notes_at(&backend, modified + 3).is_ok());

        // The guard only considers unsynced notes. The clean fixture still
        // stays in the same managed profile, but it is no longer pending.
        let updated = backend
            .collection
            .storage
            .db()
            .execute("UPDATE notes SET usn=0 WHERE id=?1", [note_id])
            .unwrap();
        assert_eq!(updated, 1);
        assert!(refuse_fresh_pending_notes_at(&backend, modified).is_ok());
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
        let error = refuse_fresh_pending_notes_at(&backend, modified + 60)
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
        refuse_fresh_pending_notes_at(&backend, modified + 60).unwrap();

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
        refuse_fresh_pending_notes_at(&backend, modified + 60).unwrap();
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
        refuse_fresh_pending_notes_at(&backend, 1_800_000_000).unwrap();
    }
}
