//! Feature-gated direct integration with the pinned, AGPL-licensed Anki Rust engine.
//! No user profile is opened by this module's initial construction or tests.
use anki::collection::CollectionBuilder;
use anki::decks::DeckId;
use std::error::Error;
use std::fs;
use std::path::Path;

type BackendResult<T> = Result<T, Box<dyn Error>>;

pub struct AnkiBackend {
    collection: anki::collection::Collection,
}

impl AnkiBackend {
    /// Opens a disposable in-memory Anki collection; does not access the user's Anki profile.
    pub fn in_memory() -> BackendResult<Self> {
        Ok(Self {
            collection: CollectionBuilder::default().build()?,
        })
    }

    /// Create a brand-new Retivus-owned profile under an existing user-selected root.
    ///
    /// Never opens an arbitrary Anki profile; refuses existing names and symlink paths.
    /// Explicitly `create_dir` (not `create_dir_all`) prevents replacement of existing
    /// profile contents. The containing root must already exist.
    pub fn create_managed_profile(root: &Path, profile_id: &str) -> BackendResult<Self> {
        let canonical_root = fs::canonicalize(root)?;
        if !canonical_root.is_dir() {
            return Err("profile root is not a directory".into());
        }
        validate_profile_id(profile_id)?;
        let profile_dir = canonical_root.join(profile_id);
        fs::create_dir(&profile_dir)?;
        let collection_path = profile_dir.join("collection.anki2");
        let collection = CollectionBuilder::new(&collection_path).build()?;
        Ok(Self { collection })
    }

    /// Reopen only a named profile within the selected Retivus profile root.
    /// An absent, symlinked or corrupt database is an error; it is never reset.
    pub fn open_managed_profile(root: &Path, profile_id: &str) -> BackendResult<Self> {
        let canonical_root = fs::canonicalize(root)?;
        if !canonical_root.is_dir() {
            return Err("profile root is not a directory".into());
        }
        validate_profile_id(profile_id)?;
        let dir = canonical_root.join(profile_id);
        let metadata = fs::symlink_metadata(&dir)?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("profile directory must be a real directory".into());
        }
        let collection_path = dir.join("collection.anki2");
        let metadata = fs::symlink_metadata(&collection_path)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("collection must be a regular file, not a symlink".into());
        }
        let mut builder = CollectionBuilder::new(&collection_path);
        builder.set_check_integrity(true);
        let collection = builder.build()?;
        Ok(Self { collection })
    }

    /// Gracefully release the SQLite connection so all committed changes can be reopened.
    pub fn close(self) -> BackendResult<()> {
        self.collection.close(None)?;
        Ok(())
    }

    /// The active upstream database schema version, for explicit migration diagnostics.
    pub fn schema_version(&self) -> BackendResult<i32> {
        Ok(self
            .collection
            .storage
            .db()
            .query_row("select ver from col", [], |row| row.get(0))?)
    }

    /// Explicitly report incompatible collections without overwriting them.
    /// The caller must display this error and offer a manual recovery/import choice.
    pub fn inspect_managed_profile(root: &Path, profile_id: &str) -> BackendResult<i32> {
        let backend = Self::open_managed_profile(root, profile_id)?;
        let version = backend.schema_version()?;
        backend.close()?;
        Ok(version)
    }

    pub fn create_deck(&mut self, name: &str) -> BackendResult<i64> {
        if name.trim().is_empty() {
            return Err("deck name cannot be blank".into());
        }
        let deck = self.collection.get_or_create_normal_deck(name)?;
        Ok(deck.id.0)
    }

    pub fn add_basic_note(&mut self, deck_id: i64, front: &str, back: &str) -> BackendResult<i64> {
        if front.trim().is_empty() || back.trim().is_empty() {
            return Err("front and back must not be blank".into());
        }
        let notetype = self
            .collection
            .get_notetype_by_name("Basic")?
            .ok_or("Anki collection does not contain Basic note type")?;
        let mut note = notetype.new_note();
        note.set_field(0, front)?;
        note.set_field(1, back)?;
        self.collection.add_note(&mut note, DeckId(deck_id))?;
        Ok(note.id.0)
    }

    pub fn due_counts(&mut self) -> BackendResult<(usize, usize, usize)> {
        let queued = self.collection.get_queued_cards(10, false)?;
        Ok((queued.new_count, queued.learning_count, queued.review_count))
    }

    pub fn next_card_id(&mut self) -> BackendResult<Option<i64>> {
        Ok(self
            .collection
            .get_queued_cards(1, false)?
            .cards
            .first()
            .map(|card| card.card.id().0))
    }
}

fn validate_profile_id(profile_id: &str) -> BackendResult<()> {
    if profile_id.is_empty()
        || profile_id.len() > 64
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("profile ID must be 1–64 ASCII letters, digits, underscores or dashes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anki::search::SortMode;
    use anki::storage::SchemaVersion;
    use tempfile::tempdir;

    #[test]
    fn creates_real_anki_deck_note_and_due_card_without_disk_profile() {
        let mut backend = AnkiBackend::in_memory().unwrap();
        let deck_id = backend.create_deck("Retivus integration").unwrap();
        assert_ne!(deck_id, 1);
        assert!(backend.create_deck(" ").is_err());
        assert!(backend.add_basic_note(1, "", "meaning").is_err());

        let note_id = backend.add_basic_note(1, "banqueta", "sidewalk").unwrap();
        assert!(note_id > 0);
        let cards = backend
            .collection
            .search_cards("front:banqueta", SortMode::NoOrder)
            .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(backend.next_card_id().unwrap(), Some(cards[0].0));
        let (new, learning, review) = backend.due_counts().unwrap();
        assert!(new >= 1);
        assert_eq!((learning, review), (0, 0));
    }
    #[test]
    fn managed_profiles_are_non_destructive_and_isolated() {
        let tmp = tempdir().unwrap();
        assert!(AnkiBackend::create_managed_profile(tmp.path(), "../escape").is_err());
        assert!(AnkiBackend::create_managed_profile(tmp.path(), "a/b").is_err());
        assert!(AnkiBackend::create_managed_profile(tmp.path(), "").is_err());
        assert!(!tmp.path().join("escape").exists());

        let mut first = AnkiBackend::create_managed_profile(tmp.path(), "first").unwrap();
        assert!(AnkiBackend::create_managed_profile(tmp.path(), "first").is_err());
        let id = first
            .add_basic_note(1, "retivus", "spaced repetition")
            .unwrap();
        first.close().unwrap();
        let reopened = AnkiBackend::open_managed_profile(tmp.path(), "first").unwrap();
        assert!(reopened
            .collection
            .storage
            .get_note(anki::notes::NoteId(id))
            .unwrap()
            .is_some());
        reopened.close().unwrap();

        let mut other = AnkiBackend::create_managed_profile(tmp.path(), "second").unwrap();
        assert!(other
            .collection
            .search_cards("front:retivus", SortMode::NoOrder)
            .unwrap()
            .is_empty());
        other.close().unwrap();
        assert!(AnkiBackend::open_managed_profile(tmp.path(), "missing").is_err());
        assert!(!tmp.path().join("missing").exists());
    }

    #[test]
    fn pinned_backend_upgrades_supported_schema_11_without_losing_note() {
        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "upgrade").unwrap();
        let note_id = backend
            .add_basic_note(1, "schema safety", "existing answer")
            .unwrap();
        backend.collection.close(Some(SchemaVersion::V11)).unwrap();

        let reopened = AnkiBackend::open_managed_profile(tmp.path(), "upgrade").unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 18);
        assert_eq!(
            reopened
                .collection
                .storage
                .get_note(anki::notes::NoteId(note_id))
                .unwrap()
                .unwrap()
                .fields(),
            &vec!["schema safety".to_owned(), "existing answer".to_owned()]
        );
        reopened.close().unwrap();
    }

    #[test]
    fn unknown_future_schema_never_silently_resets_the_profile() {
        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "future").unwrap();
        backend
            .add_basic_note(1, "future note", "must survive")
            .unwrap();
        backend
            .collection
            .storage
            .db()
            .execute("update col set ver = 99", [])
            .unwrap();
        backend.close().unwrap();

        let path = tmp.path().join("future/collection.anki2");
        assert!(path.exists());
        assert!(AnkiBackend::open_managed_profile(tmp.path(), "future").is_err());
        assert!(path.exists());
        assert!(AnkiBackend::create_managed_profile(tmp.path(), "future").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_profile_or_database_symlink_redirects() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "safe").unwrap();
        backend.add_basic_note(1, "safe", "private").unwrap();
        backend.close().unwrap();
        symlink(tmp.path().join("safe"), tmp.path().join("shortcut")).unwrap();
        assert!(AnkiBackend::open_managed_profile(tmp.path(), "shortcut").is_err());

        let folder = tmp.path().join("redirect");
        fs::create_dir(&folder).unwrap();
        symlink(
            tmp.path().join("safe/collection.anki2"),
            folder.join("collection.anki2"),
        )
        .unwrap();
        assert!(AnkiBackend::open_managed_profile(tmp.path(), "redirect").is_err());
    }
}
