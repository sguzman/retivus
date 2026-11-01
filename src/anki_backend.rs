//! Feature-gated direct integration with the pinned, AGPL-licensed Anki Rust engine.
//! No user profile is opened by this module's initial construction or tests.
use crate::source_store::SourceCardCandidate;
use anki::collection::CollectionBuilder;
use anki::config::BoolKey;
use anki::decks::DeckId;
use anki::import_export::package::{ExportAnkiPackageOptions, ImportAnkiPackageOptions};
use anki::notes::NoteId;
use anki::notetype::{CardTemplate, NotetypeId};
use anki::scheduler::answering::{CardAnswer, Rating};
use anki::search::SearchNode;
use anki::timestamp::TimestampMillis;
use rusqlite::OpenFlags;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{Builder, NamedTempFile};

type BackendResult<T> = Result<T, Box<dyn Error>>;

/// Bounded, read-only note listing for the native learner browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRow {
    pub id: i64,
    pub fields: Vec<String>,
    pub tags: Vec<String>,
}

/// Anki's own rendered card result; egui must treat HTML as untrusted text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewPreview {
    pub card_id: i64,
    pub question_html: String,
    pub answer_html: String,
    pub css: String,
}

/// Persisted collection-wide totals for a managed learner profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollectionStats {
    pub notes: usize,
    pub cards: usize,
    pub review_events: usize,
}

pub struct AnkiBackend {
    collection: anki::collection::Collection,
    managed_dir: Option<PathBuf>,
}

impl AnkiBackend {
    /// Opens a disposable in-memory Anki collection; does not access the user's Anki profile.
    pub fn in_memory() -> BackendResult<Self> {
        Ok(Self {
            collection: CollectionBuilder::default().build()?,
            managed_dir: None,
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
        let mut builder = CollectionBuilder::new(&collection_path);
        configure_managed_media(&mut builder, &profile_dir)?;
        let collection = builder.build()?;
        Ok(Self {
            collection,
            managed_dir: Some(profile_dir),
        })
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
        configure_managed_media(&mut builder, &dir)?;
        let collection = builder.build()?;
        Ok(Self {
            collection,
            managed_dir: Some(dir),
        })
    }

    /// Gracefully release the SQLite connection so all committed changes can be reopened.
    pub fn close(self) -> BackendResult<()> {
        self.collection.close(None)?;
        Ok(())
    }

    /// Import a package through the pinned upstream Anki engine into a
    /// Retivus-managed collection only. Do not open an arbitrary Anki profile.
    pub fn import_apkg(&mut self, path: &Path) -> BackendResult<()> {
        self.import_apkg_with_scheduling(path, false)
    }

    /// Package imports do not bring another person's review history by default.
    /// Use the opt-in variant only when intentionally restoring that history.
    pub fn import_apkg_with_scheduling(
        &mut self,
        path: &Path,
        include_review_history: bool,
    ) -> BackendResult<()> {
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("package import requires a Retivus-owned on-disk profile")?;
        check_managed_media(managed)?;
        check_apkg_path(path)?;
        self.collection.import_apkg(
            path,
            ImportAnkiPackageOptions {
                with_scheduling: include_review_history,
                ..Default::default()
            },
        )?;
        Ok(())
    }

    /// Export a shareable .apkg without private study history by default.
    pub fn export_apkg_to(&mut self, destination: &Path) -> BackendResult<usize> {
        self.export_apkg_to_with_scheduling(destination, false)
    }

    /// Include personal review/scheduling history only after explicit opt-in.
    /// Never overwrite an existing user file.
    pub fn export_apkg_to_with_scheduling(
        &mut self,
        destination: &Path,
        include_review_history: bool,
    ) -> BackendResult<usize> {
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("package export requires a Retivus-owned on-disk profile")?;
        check_managed_media(managed)?;
        if !destination.is_absolute()
            || destination.extension().and_then(|value| value.to_str()) != Some("apkg")
        {
            return Err("export destination must be an absolute .apkg path".into());
        }
        let name = destination
            .file_name()
            .ok_or("package filename is required")?;
        let parent = fs::canonicalize(
            destination
                .parent()
                .ok_or("export parent directory is required")?,
        )?;
        check_regular(&parent, true)?;
        let final_path = parent.join(name);
        if fs::symlink_metadata(&final_path).is_ok() {
            return Err("existing packages cannot be overwritten".into());
        }
        let stage = NamedTempFile::new_in(&parent)?;
        let exported = self.collection.export_apkg(
            stage.path(),
            ExportAnkiPackageOptions {
                with_scheduling: include_review_history,
                with_deck_configs: true,
                with_media: true,
                legacy: false,
            },
            SearchNode::WholeCollection,
            None,
        )?;
        stage.as_file().sync_all()?;
        let finished = stage.persist_noclobber(&final_path)?;
        finished.sync_all()?;
        fs::File::open(&parent)?.sync_all()?;
        Ok(exported)
    }

    /// Create a complete SQLite snapshot of a Retivus-owned collection using
    /// SQLite's own consistent VACUUM INTO; never copy a live database file.
    /// The requested absolute destination must not exist and its parent must exist.
    /// Returns only after integrity validation, atomic no-clobber publish and sync.
    pub fn backup_to(&mut self, destination: &Path) -> BackendResult<()> {
        let managed = self
            .managed_dir
            .as_ref()
            .ok_or("only Retivus-managed disk profiles may produce persistent backups")?;
        check_regular(managed, true)?;
        check_regular(&managed.join("collection.anki2"), false)?;
        if !destination.is_absolute() {
            return Err("backup destination must be an absolute file path".into());
        }
        let name = destination
            .file_name()
            .ok_or("backup filename is required")?;
        let parent = fs::canonicalize(
            destination
                .parent()
                .ok_or("backup parent directory is required")?,
        )?;
        check_regular(&parent, true)?;
        let final_path = parent.join(name);
        if fs::symlink_metadata(&final_path).is_ok() {
            return Err("backup destination already exists; never overwrite backups".into());
        }
        let staging = Builder::new()
            .prefix(".retivus-backup-")
            .tempdir_in(&parent)?;
        let staged = NamedTempFile::new_in(staging.path())?;
        let staged_name = staged
            .path()
            .to_str()
            .ok_or("temporary backup path is not UTF-8")?;
        self.collection
            .storage
            .db()
            .execute("VACUUM INTO ?1", [staged_name])?;
        verify_anki_staged_copy(staged.path())?;
        staged.as_file().sync_all()?;
        let finished = staged.persist_noclobber(&final_path)?;
        finished.sync_all()?;
        fs::File::open(&parent)?.sync_all()?;
        Ok(())
    }

    /// Recover a validated SQLite snapshot as a brand-new Retivus profile.
    /// Existing profile directories, files and Anki installations are never
    /// overwritten. The staged copy is checked using the pinned Anki engine.
    pub fn restore_backup_as_new_profile(
        root: &Path,
        profile_id: &str,
        backup_path: &Path,
    ) -> BackendResult<Self> {
        validate_profile_id(profile_id)?;
        let root = fs::canonicalize(root)?;
        check_regular(&root, true)?;
        check_regular(backup_path, false)?;
        check_sqlite_identity(backup_path)?;
        let target = root.join(profile_id);
        if fs::symlink_metadata(&target).is_ok() {
            return Err("restored profile already exists; never replace profiles".into());
        }

        let staging = Builder::new()
            .prefix(".retivus-restore-")
            .tempdir_in(&root)?;
        let staged_path = staging.path().join("collection.anki2");
        fs::copy(backup_path, &staged_path)?;
        verify_anki_staged_copy(&staged_path)?;
        fs::File::open(&staged_path)?.sync_all()?;

        // Publishing the complete file via a hard link (same root filesystem)
        // cannot overwrite another collection. The new directory is created
        // with exclusive create_dir after all validation has succeeded.
        fs::create_dir(&target)?;
        let final_database = target.join("collection.anki2");
        if let Err(err) = fs::hard_link(&staged_path, &final_database) {
            // Remove only the directory just created, and only if still empty.
            let _ = fs::remove_dir(&target);
            return Err(err.into());
        }
        fs::File::open(&target)?.sync_all()?;
        fs::File::open(&root)?.sync_all()?;
        Self::open_managed_profile(&root, profile_id)
    }

    /// Import a *closed*, offline Anki .anki2 database as a new Retivus profile.
    /// Never opens the source through Anki's potentially migrating writer.
    /// A live WAL/journal is refused; media must be imported separately as .apkg.
    pub fn import_offline_anki_collection_copy(
        root: &Path,
        profile_id: &str,
        source: &Path,
    ) -> BackendResult<Self> {
        validate_profile_id(profile_id)?;
        if !source.is_absolute()
            || source.extension().and_then(|extension| extension.to_str()) != Some("anki2")
        {
            return Err("an absolute offline .anki2 source file is required".into());
        }
        check_regular(source, false)?;
        ensure_no_sqlite_sidecars(source)?;
        let root = fs::canonicalize(root)?;
        check_regular(&root, true)?;
        if fs::symlink_metadata(root.join(profile_id)).is_ok() {
            return Err("target profile already exists; refusing replacement".into());
        }

        let initial = fs::metadata(source)?;
        let copied = NamedTempFile::new_in(&root)?;
        fs::copy(source, copied.path())?;
        ensure_no_sqlite_sidecars(source)?;
        let after_copy = fs::metadata(source)?;
        if initial.len() != after_copy.len()
            || initial.modified()? != after_copy.modified()?
            || fs::metadata(copied.path())?.len() != initial.len()
        {
            return Err("offline collection changed during copy; source may be active".into());
        }
        // Validate the detached copy with Anki's registered custom collations.
        // The original file is never passed to CollectionBuilder.
        verify_anki_staged_copy(copied.path())?;
        Self::restore_backup_as_new_profile(&root, profile_id, copied.path())
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

    /// The review queue is scoped to Anki's persisted current deck, rather
    /// than whichever deck was most recently used to create a note.
    pub fn current_review_deck(&mut self) -> BackendResult<i64> {
        Ok(self.collection.get_current_deck()?.id.0)
    }

    /// Change Anki's real review-queue scope, not just the egui selector.
    /// Refuse missing decks instead of silently falling back to Default.
    pub fn select_review_deck(&mut self, deck_id: i64) -> BackendResult<()> {
        if deck_id <= 0 || self.collection.get_deck(DeckId(deck_id))?.is_none() {
            return Err("selected review deck does not exist".into());
        }
        if self.current_review_deck()? != deck_id {
            self.collection.set_current_deck(DeckId(deck_id))?;
        }
        Ok(())
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

    /// Existing deck names come from the pinned upstream Anki collection engine.
    pub fn list_decks(&mut self) -> BackendResult<Vec<(i64, String)>> {
        let tree = self.collection.deck_tree(None)?;
        let mut pending = tree.children;
        let mut decks = Vec::new();
        while let Some(node) = pending.pop() {
            decks.push((node.deck_id, node.name));
            pending.extend(node.children);
        }
        decks.sort_by(|left, right| left.1.cmp(&right.1));
        Ok(decks)
    }

    /// Rename an existing deck using Anki's undoable transaction.
    pub fn rename_deck(&mut self, deck_id: i64, name: &str) -> BackendResult<()> {
        if deck_id <= 0 || name.trim().is_empty() {
            return Err("deck ID must be positive and name must not be blank".into());
        }
        if !self
            .collection
            .get_all_deck_names(false)?
            .iter()
            .any(|(id, _)| id.0 == deck_id)
        {
            return Err("requested deck does not exist".into());
        }
        self.collection.rename_deck(DeckId(deck_id), name)?;
        Ok(())
    }

    /// Deliberately destructive: Anki removes cards in the deck and orphaned notes.
    /// The caller must explicitly confirm that loss before invoking this method.
    /// Default deck deletion is never exposed through this adapter.
    pub fn delete_deck_and_cards(&mut self, deck_id: i64, confirmed: bool) -> BackendResult<()> {
        if deck_id <= 1 || !confirmed {
            return Err("deleting a non-default deck and its cards requires confirmation".into());
        }
        if !self
            .collection
            .get_all_deck_names(false)?
            .iter()
            .any(|(id, _)| id.0 == deck_id)
        {
            return Err("requested deck does not exist".into());
        }
        self.collection
            .remove_decks_and_child_decks(&[DeckId(deck_id)])?;
        Ok(())
    }

    /// Name and number of card templates, without exposing upstream internal types.
    pub fn list_notetypes(&mut self) -> BackendResult<Vec<(String, usize)>> {
        Ok(self
            .collection
            .get_all_notetypes()?
            .into_iter()
            .map(|notetype| (notetype.name.clone(), notetype.templates.len()))
            .collect())
    }

    /// Install a local two-direction template using Anki's own schema and card generator.
    pub fn create_bidirectional_notetype(&mut self, name: &str) -> BackendResult<i64> {
        if name.trim().is_empty() || self.collection.get_notetype_by_name(name)?.is_some() {
            return Err("note type name must be nonempty and unique".into());
        }
        let mut notetype = self
            .collection
            .get_notetype_by_name("Basic")?
            .ok_or("Anki Basic template unavailable")?
            .as_ref()
            .clone();
        notetype.id = NotetypeId(0);
        notetype.name = name.to_owned();
        notetype
            .templates
            .push(CardTemplate::new("Reverse", "{{Back}}", "{{Front}}"));
        self.collection.add_notetype(&mut notetype, false)?;
        Ok(notetype.id.0)
    }

    /// Rename a custom note type through Anki's own migration and undo logic.
    pub fn rename_notetype(&mut self, old_name: &str, new_name: &str) -> BackendResult<()> {
        if new_name.trim().is_empty() || self.collection.get_notetype_by_name(new_name)?.is_some() {
            return Err("replacement note type name must be nonempty and unique".into());
        }
        let mut notetype = self
            .collection
            .get_notetype_by_name(old_name)?
            .ok_or("requested note type does not exist")?
            .as_ref()
            .clone();
        notetype.name = new_name.to_owned();
        self.collection.update_notetype(&mut notetype, false)?;
        Ok(())
    }

    /// Edit one card template using upstream validation and a reversible transaction.
    /// Reject blank template content instead of silently generating empty cards.
    pub fn update_card_template(
        &mut self,
        notetype_name: &str,
        template_index: usize,
        template_name: &str,
        question: &str,
        answer: &str,
    ) -> BackendResult<()> {
        if template_name.trim().is_empty() || question.trim().is_empty() || answer.trim().is_empty()
        {
            return Err("template name, question and answer must not be blank".into());
        }
        let mut notetype = self
            .collection
            .get_notetype_by_name(notetype_name)?
            .ok_or("requested note type does not exist")?
            .as_ref()
            .clone();
        let template = notetype
            .templates
            .get_mut(template_index)
            .ok_or("card template index does not exist")?;
        template.name = template_name.to_owned();
        template.config.q_format = question.to_owned();
        template.config.a_format = answer.to_owned();
        self.collection.update_notetype(&mut notetype, false)?;
        Ok(())
    }

    pub fn add_note_with_notetype(
        &mut self,
        deck_id: i64,
        notetype_name: &str,
        fields: &[&str],
    ) -> BackendResult<i64> {
        let notetype = self
            .collection
            .get_notetype_by_name(notetype_name)?
            .ok_or("requested Anki note type does not exist")?;
        if fields.len() != notetype.fields.len()
            || fields.iter().any(|field| field.trim().is_empty())
        {
            return Err("note must supply all nonempty fields for the chosen note type".into());
        }
        let mut note = notetype.new_note();
        for (index, field) in fields.iter().enumerate() {
            note.set_field(index, *field)?;
        }
        self.collection.add_note(&mut note, DeckId(deck_id))?;
        Ok(note.id.0)
    }

    /// Materialize only a user-selected source sense. The separate source
    /// catalog remains read-only; Anki owns the new note, cards and review state.
    pub fn add_source_candidate(
        &mut self,
        deck_id: i64,
        candidate: &SourceCardCandidate,
    ) -> BackendResult<i64> {
        if candidate.word.trim().is_empty() || candidate.definition.trim().is_empty() {
            return Err("selected source word and definition must be nonempty".into());
        }
        let notetype = self
            .collection
            .get_notetype_by_name("Basic")?
            .ok_or("Anki collection does not contain Basic note type")?;
        let mut note = notetype.new_note();
        note.set_field(0, &candidate.word)?;
        note.set_field(1, &candidate.definition)?;
        note.tags.push(candidate.source_tag());
        self.collection.add_note(&mut note, DeckId(deck_id))?;
        Ok(note.id.0)
    }

    /// Apply only a user-approved, exact one-to-one source reconciliation.
    /// Refuse notes with manual field edits or multiple provenance identities.
    /// Anki updates the existing note in an undoable transaction, preserving its
    /// card identity and scheduler/review log instead of generating replacement cards.
    pub fn apply_approved_source_reconciliation(
        &mut self,
        catalog: &crate::source_store::SourceCatalog,
        note_id: i64,
        from: &SourceCardCandidate,
    ) -> BackendResult<()> {
        let verified = catalog.select_sense(
            &from.provenance.provider_id,
            &from.snapshot,
            &from.entry_id,
            &from.sense_id,
        )?;
        if &verified != from {
            return Err("original source selection does not match immutable snapshot".into());
        }
        let target = catalog.reconciled_sense(
            &from.provenance.provider_id,
            &from.snapshot,
            &from.entry_id,
            &from.sense_id,
        )?;
        let old_tag = from.source_tag();
        let new_tag = target.source_tag();
        if old_tag == new_tag {
            return Err("no explicit new source identity has been approved".into());
        }
        let mut note = self
            .collection
            .storage
            .get_note(NoteId(note_id))?
            .ok_or("note ID does not exist")?;
        if note.fields().len() != 2
            || note.fields()[0] != from.word
            || note.fields()[1] != from.definition
        {
            return Err(
                "source note has manual edits; explicit conflict resolution required".into(),
            );
        }
        if note
            .tags
            .iter()
            .filter(|tag| tag.starts_with("retivus::source::"))
            .count()
            != 1
            || !note.tags.contains(&old_tag)
        {
            return Err("source provenance identity does not match this note".into());
        }
        note.set_field(0, &target.word)?;
        note.set_field(1, &target.definition)?;
        note.tags.retain(|tag| tag != &old_tag);
        note.tags.push(new_tag);
        self.collection.update_note(&mut note)?;
        Ok(())
    }

    /// Return up to 100 Anki-search-matched notes without loading an unbounded
    /// collection into UI memory. Errors from Anki's query parser are visible.
    pub fn browse_notes(&mut self, query: &str, limit: usize) -> BackendResult<Vec<NoteRow>> {
        if !(1..=100).contains(&limit) {
            return Err("browse limit must be between 1 and 100".into());
        }
        let ids = self
            .collection
            .search_notes(query, anki::search::SortMode::NoOrder)?;
        let mut rows = Vec::new();
        for id in ids.into_iter().take(limit) {
            if let Some(note) = self.collection.storage.get_note(id)? {
                rows.push(NoteRow {
                    id: id.0,
                    fields: note.fields().clone(),
                    tags: note.tags,
                });
            }
        }
        Ok(rows)
    }

    /// Read actual stored totals without loading the entire learner collection.
    pub fn collection_stats(&self) -> BackendResult<CollectionStats> {
        let db = self.collection.storage.db();
        let count = |table: &str| -> BackendResult<usize> {
            let sql = match table {
                "notes" => "SELECT count(*) FROM notes",
                "cards" => "SELECT count(*) FROM cards",
                "revlog" => "SELECT count(*) FROM revlog",
                _ => return Err("unknown collection statistic".into()),
            };
            let value: i64 = db.query_row(sql, [], |row| row.get(0))?;
            Ok(usize::try_from(value)?)
        };
        Ok(CollectionStats {
            notes: count("notes")?,
            cards: count("cards")?,
            review_events: count("revlog")?,
        })
    }

    /// Inspect persisted Anki review history without changing scheduler state.
    pub fn review_event_count(&self, card_id: i64) -> BackendResult<usize> {
        if card_id <= 0 {
            return Err("card ID must be positive".into());
        }
        let count: i64 = self.collection.storage.db().query_row(
            "SELECT count(*) FROM revlog WHERE cid=?1",
            [card_id],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(count)?)
    }

    pub fn note_tags(&self, note_id: i64) -> BackendResult<Vec<String>> {
        let note = self
            .collection
            .storage
            .get_note(NoteId(note_id))?
            .ok_or("note ID does not exist")?;
        Ok(note.tags)
    }

    pub fn note_fields(&self, note_id: i64) -> BackendResult<Vec<String>> {
        let note = self
            .collection
            .storage
            .get_note(NoteId(note_id))?
            .ok_or("note ID does not exist")?;
        Ok(note.fields().clone())
    }

    pub fn edit_note_fields(&mut self, note_id: i64, fields: &[&str]) -> BackendResult<()> {
        let mut note = self
            .collection
            .storage
            .get_note(NoteId(note_id))?
            .ok_or("note ID does not exist")?;
        if fields.len() != note.fields().len() || fields.iter().any(|field| field.trim().is_empty())
        {
            return Err("updated note fields must match the existing note type".into());
        }
        for (index, field) in fields.iter().enumerate() {
            note.set_field(index, *field)?;
        }
        self.collection.update_note(&mut note)?;
        Ok(())
    }

    /// Explicit deletion, with Anki's in-session undo path available to callers.
    pub fn remove_note(&mut self, note_id: i64) -> BackendResult<()> {
        if self.collection.storage.get_note(NoteId(note_id))?.is_none() {
            return Err("cannot delete a missing note".into());
        }
        self.collection.remove_notes(&[NoteId(note_id)])?;
        Ok(())
    }

    pub fn undo_last(&mut self) -> BackendResult<()> {
        self.collection.undo()?;
        Ok(())
    }

    /// Enable upstream FSRS rather than implementing a competing scheduler.
    pub fn enable_fsrs(&mut self) -> BackendResult<()> {
        self.collection
            .set_config_bool(BoolKey::Fsrs, true, false)?;
        Ok(())
    }

    pub fn fsrs_enabled(&self) -> bool {
        self.collection.fsrs_enabled()
    }

    /// Grade only the next queued card, with Anki computing its actual next state.
    /// Rating matches Anki: 1=Again, 2=Hard, 3=Good, 4=Easy.
    pub fn grade_next_card(&mut self, rating: u8) -> BackendResult<Option<i64>> {
        let rating = match rating {
            1 => Rating::Again,
            2 => Rating::Hard,
            3 => Rating::Good,
            4 => Rating::Easy,
            _ => return Err("rating must be 1 (Again) through 4 (Easy)".into()),
        };
        let Some(queued) = self.collection.get_next_card()? else {
            return Ok(None);
        };
        let states = queued.states;
        let next = match rating {
            Rating::Again => states.again,
            Rating::Hard => states.hard,
            Rating::Good => states.good,
            Rating::Easy => states.easy,
        };
        let card_id = queued.card.id();
        let mut answer = CardAnswer {
            card_id,
            current_state: states.current,
            new_state: next,
            rating,
            answered_at: TimestampMillis::now(),
            milliseconds_taken: 1_000,
            custom_data: None,
            from_queue: true,
        };
        self.collection.answer_card(&mut answer)?;
        Ok(Some(card_id.0))
    }

    pub fn due_counts(&mut self) -> BackendResult<(usize, usize, usize)> {
        let queued = self.collection.get_queued_cards(10, false)?;
        Ok((queued.new_count, queued.learning_count, queued.review_count))
    }

    /// Grade exactly the queued card the learner saw. The single queue read
    /// makes it impossible for a second queue fetch to grade the wrong card.
    pub fn grade_previewed_card(&mut self, expected_card_id: i64, rating: u8) -> BackendResult<()> {
        let rating = match rating {
            1 => Rating::Again,
            2 => Rating::Hard,
            3 => Rating::Good,
            4 => Rating::Easy,
            _ => return Err("rating must be 1 (Again) through 4 (Easy)".into()),
        };
        let queued = self
            .collection
            .get_next_card()?
            .ok_or("review queue no longer contains the displayed card")?;
        if queued.card.id().0 != expected_card_id {
            return Err("review queue changed; refresh the displayed card before grading".into());
        }
        let states = queued.states;
        let next = match rating {
            Rating::Again => states.again,
            Rating::Hard => states.hard,
            Rating::Good => states.good,
            Rating::Easy => states.easy,
        };
        let mut answer = CardAnswer {
            card_id: queued.card.id(),
            current_state: states.current,
            new_state: next,
            rating,
            answered_at: TimestampMillis::now(),
            milliseconds_taken: 1_000,
            custom_data: None,
            from_queue: true,
        };
        self.collection.answer_card(&mut answer)?;
        Ok(())
    }

    /// Return Anki's actual next card and native-renderer output without
    /// advancing its scheduler or exposing a remote HTML execution surface.
    pub fn next_review_preview(&mut self) -> BackendResult<Option<ReviewPreview>> {
        let Some(card_id) = self.next_card_id()? else {
            return Ok(None);
        };
        let rendered =
            self.collection
                .render_existing_card(anki::card::CardId(card_id), false, false)?;
        Ok(Some(ReviewPreview {
            card_id,
            question_html: rendered.question().into_owned(),
            answer_html: rendered.answer().into_owned(),
            css: rendered.css,
        }))
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

/// Configure only a Retivus-owned media folder and its database. Existing
/// symlink paths are rejected before Anki's media manager can create files.
fn check_managed_media(dir: &Path) -> BackendResult<(PathBuf, PathBuf)> {
    check_regular(dir, true)?;
    let media = dir.join("collection.media");
    let database = dir.join("collection.mdb");
    if fs::symlink_metadata(&media).is_ok() {
        check_regular(&media, true)?;
    }
    if fs::symlink_metadata(&database).is_ok() {
        check_regular(&database, false)?;
    }
    Ok((media, database))
}

fn configure_managed_media(builder: &mut CollectionBuilder, dir: &Path) -> BackendResult<()> {
    let (media, database) = check_managed_media(dir)?;
    builder.set_media_paths(media, database);
    Ok(())
}

fn check_apkg_path(path: &Path) -> BackendResult<()> {
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()) != Some("apkg") {
        return Err("an absolute .apkg file path is required".into());
    }
    check_regular(path, false)?;
    reject_unsafe_package_members(path)?;
    Ok(())
}

/// Validate archive metadata before letting Anki extract media. This scan does
/// not unzip anything, access learner data, or follow filesystem paths.
fn reject_unsafe_package_members(path: &Path) -> BackendResult<()> {
    let archive = fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(archive)?;
    if zip.len() > 1_000_000 {
        return Err("package has too many archive entries".into());
    }
    let mut expanded = 0_u64;
    for index in 0..zip.len() {
        let file = zip.by_index(index)?;
        let raw = file.name()?;
        let enclosed = file
            .enclosed_name()
            .ok_or("package contains a path outside the archive root")?;
        if raw.starts_with('/')
            || raw.starts_with('\\')
            || raw.contains('\\')
            || raw.contains(':')
            || raw.chars().any(char::is_control)
            || enclosed
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err("package contains an unsafe media path".into());
        }
        if file
            .unix_mode()
            .is_some_and(|mode| (mode & 0o170000) == 0o120000)
        {
            return Err("package contains a symbolic link".into());
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or("package expanded size overflow")?;
        if expanded > 64 * 1024 * 1024 * 1024 {
            return Err("package exceeds maximum expanded archive size".into());
        }
        if file.size() > 1024 * 1024
            && (file.compressed_size() == 0 || file.size() / file.compressed_size() > 100)
        {
            return Err("package contains suspiciously compressed media".into());
        }
    }
    Ok(())
}

/// Refuse symlinks and special files before reading or publishing a snapshot.
fn check_regular(path: &Path, directory: bool) -> BackendResult<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink()
        || (directory && !meta.file_type().is_dir())
        || (!directory && !meta.file_type().is_file())
    {
        return Err("expected a real directory or regular file, not a symlink".into());
    }
    Ok(())
}

fn ensure_no_sqlite_sidecars(source: &Path) -> BackendResult<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut filename = source.as_os_str().to_os_string();
        filename.push(suffix);
        if fs::symlink_metadata(PathBuf::from(filename)).is_ok() {
            return Err(
                "source has SQLite journal/WAL sidecars; close Anki and export a safe copy".into(),
            );
        }
    }
    Ok(())
}

/// Read the version safely without evaluating Anki's custom-collated indexes.
/// Full integrity validation is performed on an isolated copy with Anki's engine.
fn check_sqlite_identity(path: &Path) -> BackendResult<()> {
    check_regular(path, false)?;
    let db = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let _: i32 = db.query_row("SELECT ver FROM col", [], |row| row.get(0))?;
    Ok(())
}

fn verify_anki_staged_copy(path: &Path) -> BackendResult<()> {
    check_sqlite_identity(path)?;
    let mut builder = CollectionBuilder::new(path);
    builder.set_check_integrity(true);
    let checked = builder.build()?;
    checked.close(None)?;
    Ok(())
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

    /// These public assets are downloaded by CI at pinned commits and validated
    /// by their Git blob hashes; no third-party binaries are bundled in Retivus.
    #[test]
    #[ignore = "requires public package fixtures downloaded by integration CI"]
    fn independent_public_apkg_fixtures_match_import_export_note_and_card_counts() {
        for (env_key, minimum_notes, exact_counts) in [
            ("RETIVUS_ANKIDROID_APKG", 1_usize, Some((1_usize, 1_usize))),
            ("RETIVUS_PUBLIC_SAMPLE_APKG", 3_usize, None),
        ] {
            let path = std::path::PathBuf::from(
                std::env::var_os(env_key).expect("CI must provide the pinned public package"),
            );
            let root = tempdir().unwrap();
            let mut original =
                AnkiBackend::create_managed_profile(root.path(), "independent").unwrap();
            original.import_apkg(&path).unwrap();
            let note_count = original
                .collection
                .search_notes("", SortMode::NoOrder)
                .unwrap()
                .len();
            let card_count = original
                .collection
                .search_cards("", SortMode::NoOrder)
                .unwrap()
                .len();
            assert!(
                note_count >= minimum_notes && card_count >= note_count,
                "unexpected public package counts for {env_key}"
            );
            if let Some((notes, cards)) = exact_counts {
                assert_eq!((note_count, card_count), (notes, cards));
            }
            let exported = root.path().join("independent-roundtrip.apkg");
            assert_eq!(original.export_apkg_to(&exported).unwrap(), note_count);

            let damaged = root.path().join("malformed.apkg");
            fs::write(&damaged, b"not a zip archive").unwrap();
            assert!(original.import_apkg(&damaged).is_err());
            assert_eq!(
                original
                    .collection
                    .search_notes("", SortMode::NoOrder)
                    .unwrap()
                    .len(),
                note_count
            );
            original.close().unwrap();

            let mut reimport =
                AnkiBackend::create_managed_profile(root.path(), "independent-again").unwrap();
            reimport.import_apkg(&exported).unwrap();
            assert_eq!(
                reimport
                    .collection
                    .search_notes("", SortMode::NoOrder)
                    .unwrap()
                    .len(),
                note_count
            );
            assert_eq!(
                reimport
                    .collection
                    .search_cards("", SortMode::NoOrder)
                    .unwrap()
                    .len(),
                card_count
            );
            reimport.close().unwrap();
        }
    }

    #[test]
    fn shareable_package_redacts_review_history_unless_explicitly_included() {
        let root = tempdir().unwrap();
        let mut original = AnkiBackend::create_managed_profile(root.path(), "private").unwrap();
        original
            .add_basic_note(1, "sensitive", "private review")
            .unwrap();
        let card = original.next_review_preview().unwrap().unwrap();
        original.grade_previewed_card(card.card_id, 4).unwrap();
        assert_eq!(original.collection_stats().unwrap().review_events, 1);

        let public_package = root.path().join("shareable.apkg");
        let opt_in_package = root.path().join("with-review-history.apkg");
        assert_eq!(original.export_apkg_to(&public_package).unwrap(), 1);
        assert_eq!(
            original
                .export_apkg_to_with_scheduling(&opt_in_package, true)
                .unwrap(),
            1
        );
        original.close().unwrap();

        let mut public = AnkiBackend::create_managed_profile(root.path(), "shareable").unwrap();
        public.import_apkg(&public_package).unwrap();
        let stats = public.collection_stats().unwrap();
        assert_eq!((stats.notes, stats.cards, stats.review_events), (1, 1, 0));
        public.close().unwrap();

        let mut no_export = AnkiBackend::create_managed_profile(root.path(), "no-export").unwrap();
        no_export
            .import_apkg_with_scheduling(&public_package, true)
            .unwrap();
        assert_eq!(no_export.collection_stats().unwrap().review_events, 0);
        no_export.close().unwrap();

        let mut no_import = AnkiBackend::create_managed_profile(root.path(), "no-import").unwrap();
        no_import.import_apkg(&opt_in_package).unwrap();
        assert_eq!(no_import.collection_stats().unwrap().review_events, 0);
        no_import.close().unwrap();

        let mut private = AnkiBackend::create_managed_profile(root.path(), "opted-in").unwrap();
        private
            .import_apkg_with_scheduling(&opt_in_package, true)
            .unwrap();
        let stats = private.collection_stats().unwrap();
        assert_eq!((stats.notes, stats.cards, stats.review_events), (1, 1, 1));
        private.close().unwrap();
    }

    #[test]
    fn package_round_trip_preserves_media_notes_decks_templates_and_tags() {
        let root = tempdir().unwrap();
        let mut producer = AnkiBackend::create_managed_profile(root.path(), "producer").unwrap();
        let deck_id = producer.create_deck("Package Fixture").unwrap();
        let kind = "Package Bidirectional";
        producer.create_bidirectional_notetype(kind).unwrap();
        let attachment = producer
            .collection
            .media()
            .unwrap()
            .add_file("fixture.png", b"Retivus PNG fixture")
            .unwrap();
        assert_eq!(attachment.as_ref(), "fixture.png");
        let note = producer
            .add_note_with_notetype(
                deck_id,
                kind,
                &["<img src=\"fixture.png\"> palabra", "word"],
            )
            .unwrap();
        let mut note_data = producer
            .collection
            .storage
            .get_note(NoteId(note))
            .unwrap()
            .unwrap();
        note_data.tags.push("retivus-package-test".into());
        producer.collection.update_note(&mut note_data).unwrap();
        let package_path = root.path().join("fixture.apkg");
        assert_eq!(producer.export_apkg_to(&package_path).unwrap(), 1);
        assert!(package_path.exists());
        assert!(producer.export_apkg_to(&package_path).is_err());
        producer.close().unwrap();

        let mut consumer = AnkiBackend::create_managed_profile(root.path(), "consumer").unwrap();
        assert!(consumer
            .import_apkg(&root.path().join("missing.apkg"))
            .is_err());
        consumer.import_apkg(&package_path).unwrap();
        assert!(consumer
            .list_decks()
            .unwrap()
            .iter()
            .any(|(_, name)| name == "Package Fixture"));
        assert!(consumer
            .list_notetypes()
            .unwrap()
            .iter()
            .any(|(name, templates)| name == kind && *templates == 2));
        let imported_notes = consumer
            .collection
            .search_notes("tag:retivus-package-test", SortMode::NoOrder)
            .unwrap();
        assert_eq!(imported_notes.len(), 1);
        let imported_id = imported_notes[0].0;
        assert_eq!(
            consumer.note_fields(imported_id).unwrap(),
            ["<img src=\"fixture.png\"> palabra", "word"]
        );
        assert!(consumer
            .note_tags(imported_id)
            .unwrap()
            .contains(&"retivus-package-test".to_owned()));
        assert_eq!(
            consumer
                .collection
                .search_cards("tag:retivus-package-test", SortMode::NoOrder)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            fs::read(root.path().join("consumer/collection.media/fixture.png")).unwrap(),
            b"Retivus PNG fixture"
        );
        consumer.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(root.path(), "consumer").unwrap();
        assert_eq!(
            reopened.note_fields(imported_id).unwrap(),
            ["<img src=\"fixture.png\"> palabra", "word"]
        );
        reopened.close().unwrap();
    }

    #[test]
    fn cloze_html_css_and_image_render_parity_survives_package_roundtrip() {
        use anki::card::CardId;

        fn render_cloze(backend: &mut AnkiBackend) -> Vec<(String, String, String)> {
            let cards = backend
                .collection
                .search_cards("tag:retivus-cloze-fixture", SortMode::NoOrder)
                .unwrap();
            assert_eq!(cards.len(), 2);
            let mut rendered: Vec<_> = cards
                .into_iter()
                .map(|id| {
                    let output = backend
                        .collection
                        .render_existing_card(CardId(id.0), false, false)
                        .unwrap();
                    (
                        output.question().into_owned(),
                        output.answer().into_owned(),
                        output.css,
                    )
                })
                .collect();
            rendered.sort();
            rendered
        }

        let root = tempdir().unwrap();
        let mut source = AnkiBackend::create_managed_profile(root.path(), "rich-source").unwrap();
        let deck = source.create_deck("Cloze Rich Text").unwrap();
        let mut cloze = (*source
            .collection
            .get_notetype_by_name("Cloze")
            .unwrap()
            .unwrap())
        .clone();
        cloze
            .config
            .css
            .push_str("\n.retivus-cloze { font-weight: 700; }\n");
        cloze.templates[0].config.q_format =
            "<section class=\"retivus-cloze\">{{cloze:Text}}</section>".into();
        cloze.templates[0].config.a_format =
            "<article class=\"retivus-cloze\">{{cloze:Text}}<p>{{Back Extra}}</p></article>".into();
        source
            .collection
            .update_notetype(&mut cloze, false)
            .unwrap();

        let image_bytes = br#"<svg xmlns="http://www.w3.org/2000/svg"
width="2" height="2"><rect width="2" height="2" fill="black"/></svg>"#;
        source
            .collection
            .media()
            .unwrap()
            .add_file("cloze-image.svg", image_bytes)
            .unwrap();
        let note_id = source
            .add_note_with_notetype(
                deck,
                "Cloze",
                &[
                    "{{c1::gato}} tiene {{c2::bigotes}} <img src=\"cloze-image.svg\">",
                    "<em>animal doméstico</em>",
                ],
            )
            .unwrap();
        let mut note = source
            .collection
            .storage
            .get_note(NoteId(note_id))
            .unwrap()
            .unwrap();
        note.tags.push("retivus-cloze-fixture".into());
        source.collection.update_note(&mut note).unwrap();

        let original = render_cloze(&mut source);
        assert!(original.iter().all(|(question, answer, css)| {
            question.contains("retivus-cloze")
                && answer.contains("retivus-cloze")
                && answer.contains("animal doméstico")
                && css.contains("font-weight: 700")
        }));
        assert!(original.iter().all(|(question, answer, _)| {
            question.contains("cloze-image.svg") && answer.contains("cloze-image.svg")
        }));
        let package = root.path().join("cloze-roundtrip.apkg");
        assert_eq!(source.export_apkg_to(&package).unwrap(), 1);
        source.close().unwrap();

        let mut imported = AnkiBackend::create_managed_profile(root.path(), "rich-target").unwrap();
        imported.import_apkg(&package).unwrap();
        assert_eq!(render_cloze(&mut imported), original);
        assert_eq!(
            fs::read(
                root.path()
                    .join("rich-target/collection.media/cloze-image.svg")
            )
            .unwrap(),
            image_bytes
        );
        imported.close().unwrap();

        let mut reopened = AnkiBackend::open_managed_profile(root.path(), "rich-target").unwrap();
        assert_eq!(render_cloze(&mut reopened), original);
        reopened.close().unwrap();
    }

    #[test]
    fn package_path_safety_rejects_unmanaged_and_symlinked_inputs() {
        let root = tempdir().unwrap();
        let path = root.path().join("existing.apkg");
        fs::write(&path, b"keep untouched").unwrap();
        assert!(AnkiBackend::in_memory()
            .unwrap()
            .import_apkg(&path)
            .is_err());
        assert!(AnkiBackend::in_memory()
            .unwrap()
            .export_apkg_to(&path)
            .is_err());
        let mut managed = AnkiBackend::create_managed_profile(root.path(), "managed").unwrap();
        assert!(managed.export_apkg_to(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"keep untouched");
        assert!(managed.import_apkg(Path::new("relative.apkg")).is_err());
        assert!(managed.export_apkg_to(Path::new("relative.apkg")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, root.path().join("symlink.apkg")).unwrap();
            assert!(managed
                .import_apkg(&root.path().join("symlink.apkg"))
                .is_err());
            fs::create_dir(root.path().join("managed/collection.media")).unwrap();
            let media = root.path().join("managed/collection.media");
            fs::remove_dir(&media).unwrap();
            std::os::unix::fs::symlink(root.path(), &media).unwrap();
            assert!(managed.import_apkg(&path).is_err());
        }
        managed.close().unwrap();
    }

    #[test]
    fn apkg_archive_preflight_blocks_traversal_windows_paths_and_zip_bombs() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "archive-safe").unwrap();
        let intact = backend.add_basic_note(1, "original", "safe").unwrap();
        for (index, archive_member) in [
            "../outside.txt",
            "C:\\windows\\outside.txt",
            "/absolute.txt",
        ]
        .into_iter()
        .enumerate()
        {
            let package = root.path().join(format!("unsafe-{index}.apkg"));
            let mut zip = zip::ZipWriter::new(fs::File::create(&package).unwrap());
            zip.start_file(archive_member, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"not a collection").unwrap();
            zip.finish().unwrap();
            assert!(
                reject_unsafe_package_members(&package).is_err(),
                "{archive_member}"
            );
            assert!(backend.import_apkg(&package).is_err(), "{archive_member}");
            assert!(!root.path().join("outside.txt").exists());
            assert_eq!(backend.note_fields(intact).unwrap(), ["original", "safe"]);
        }

        let bomb = root.path().join("compressed-bomb.apkg");
        let mut zip = zip::ZipWriter::new(fs::File::create(&bomb).unwrap());
        zip.start_file(
            "0",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&vec![0_u8; 2_000_000]).unwrap();
        zip.finish().unwrap();
        assert!(reject_unsafe_package_members(&bomb).is_err());
        assert!(backend.import_apkg(&bomb).is_err());
        assert_eq!(backend.note_fields(intact).unwrap(), ["original", "safe"]);

        let valid_unicode = root.path().join("unicode-media.apkg");
        let mut zip = zip::ZipWriter::new(fs::File::create(&valid_unicode).unwrap());
        zip.start_file("écriture/日本語.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"safe unicode path").unwrap();
        zip.finish().unwrap();
        assert!(reject_unsafe_package_members(&valid_unicode).is_ok());
        backend.close().unwrap();
    }

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

    #[test]
    fn offline_anki_copy_migrates_schema_without_modifying_source_and_refuses_active_files() {
        let root = tempdir().unwrap();
        let mut upstream = AnkiBackend::create_managed_profile(root.path(), "upstream").unwrap();
        let note_id = upstream.add_basic_note(1, "archivo", "archive").unwrap();
        upstream.collection.close(Some(SchemaVersion::V11)).unwrap();
        let source = root.path().join("upstream/collection.anki2");
        let pristine = fs::read(&source).unwrap();

        let wal = root.path().join("upstream/collection.anki2-wal");
        fs::write(&wal, b"simulated uncheckpointed write").unwrap();
        assert!(
            AnkiBackend::import_offline_anki_collection_copy(root.path(), "blocked", &source)
                .is_err()
        );
        assert!(!root.path().join("blocked").exists());
        fs::remove_file(&wal).unwrap();

        let copied =
            AnkiBackend::import_offline_anki_collection_copy(root.path(), "imported", &source)
                .unwrap();
        assert_eq!(copied.schema_version().unwrap(), 18);
        assert_eq!(copied.note_fields(note_id).unwrap(), ["archivo", "archive"]);
        copied.close().unwrap();
        assert_eq!(fs::read(&source).unwrap(), pristine);
        assert!(
            AnkiBackend::import_offline_anki_collection_copy(root.path(), "imported", &source)
                .is_err()
        );
        assert_eq!(fs::read(&source).unwrap(), pristine);

        #[cfg(unix)]
        {
            let linked = root.path().join("source-link.anki2");
            std::os::unix::fs::symlink(&source, &linked).unwrap();
            assert!(AnkiBackend::import_offline_anki_collection_copy(
                root.path(),
                "linked",
                &linked
            )
            .is_err());
            assert!(!root.path().join("linked").exists());
        }
        let damaged = root.path().join("malformed.anki2");
        fs::write(&damaged, b"not sqlite").unwrap();
        assert!(AnkiBackend::import_offline_anki_collection_copy(
            root.path(),
            "malformed",
            &damaged
        )
        .is_err());
        assert!(!root.path().join("malformed").exists());
        assert_eq!(fs::read(&damaged).unwrap(), b"not sqlite");
    }

    /// Official Anki Desktop collection fixture downloaded via the pinned upstream checkout.
    #[test]
    #[ignore = "requires pinned Anki Desktop fixture from CI"]
    fn official_upstream_anki_collection_fixture_imports_only_as_a_copy() {
        let source = std::path::PathBuf::from(
            std::env::var_os("RETIVUS_UPSTREAM_ANKI2")
                .expect("CI must supply pinned official .anki2 fixture"),
        );
        let original = fs::read(&source).unwrap();
        let root = tempdir().unwrap();
        let mut copied =
            AnkiBackend::import_offline_anki_collection_copy(root.path(), "official", &source)
                .unwrap();
        assert_eq!(copied.schema_version().unwrap(), 18);
        let notes = copied
            .collection
            .search_notes("", SortMode::NoOrder)
            .unwrap()
            .len();
        let cards = copied
            .collection
            .search_cards("", SortMode::NoOrder)
            .unwrap()
            .len();
        assert!(cards >= notes);
        copied.close().unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);

        let mut reopened = AnkiBackend::open_managed_profile(root.path(), "official").unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 18);
        assert_eq!(
            reopened
                .collection
                .search_notes("", SortMode::NoOrder)
                .unwrap()
                .len(),
            notes
        );
        reopened.close().unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
    }

    #[test]
    fn review_deck_selection_scopes_queue_and_survives_profile_reopen() {
        let root = tempdir().unwrap();
        let mut backend =
            AnkiBackend::create_managed_profile(root.path(), "deck-selection").unwrap();
        let first_deck = backend.create_deck("First").unwrap();
        let second_deck = backend.create_deck("Second").unwrap();
        backend.add_basic_note(first_deck, "uno", "one").unwrap();
        backend.add_basic_note(second_deck, "dos", "two").unwrap();

        assert_eq!(backend.current_review_deck().unwrap(), 1);
        assert!(backend.next_review_preview().unwrap().is_none());
        backend.select_review_deck(first_deck).unwrap();
        assert_eq!(backend.current_review_deck().unwrap(), first_deck);
        let first = backend.next_review_preview().unwrap().unwrap();
        assert!(first.question_html.contains("uno"));
        assert_eq!(backend.due_counts().unwrap().0, 1);

        backend.select_review_deck(second_deck).unwrap();
        let second = backend.next_review_preview().unwrap().unwrap();
        assert!(second.question_html.contains("dos"));
        assert_ne!(first.card_id, second.card_id);
        assert_eq!(backend.due_counts().unwrap().0, 1);
        assert!(backend.select_review_deck(i64::MAX).is_err());
        assert!(backend.select_review_deck(0).is_err());
        assert_eq!(backend.current_review_deck().unwrap(), second_deck);
        backend.close().unwrap();

        let mut reopened =
            AnkiBackend::open_managed_profile(root.path(), "deck-selection").unwrap();
        assert_eq!(reopened.current_review_deck().unwrap(), second_deck);
        assert_eq!(
            reopened.next_review_preview().unwrap().unwrap().card_id,
            second.card_id
        );
        reopened.close().unwrap();
    }

    #[test]
    fn persisted_collection_stats_match_notes_cards_and_review_log() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "stats").unwrap();
        assert_eq!(
            backend.collection_stats().unwrap(),
            CollectionStats::default()
        );
        backend.add_basic_note(1, "banqueta", "sidewalk").unwrap();
        assert_eq!(
            backend.collection_stats().unwrap(),
            CollectionStats {
                notes: 1,
                cards: 1,
                review_events: 0
            }
        );
        let card = backend.next_review_preview().unwrap().unwrap();
        backend.grade_previewed_card(card.card_id, 4).unwrap();
        assert_eq!(backend.collection_stats().unwrap().review_events, 1);
        backend.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(root.path(), "stats").unwrap();
        assert_eq!(
            reopened.collection_stats().unwrap(),
            CollectionStats {
                notes: 1,
                cards: 1,
                review_events: 1
            }
        );
        reopened.close().unwrap();
    }

    #[test]
    fn rendered_due_card_preview_reuses_anki_scheduler_without_grading() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "review").unwrap();
        backend.enable_fsrs().unwrap();
        let note_id = backend.add_basic_note(1, "perro", "dog").unwrap();
        let first = backend.next_review_preview().unwrap().unwrap();
        assert!(first.card_id > 0);
        assert!(first.question_html.contains("perro"));
        assert!(first.answer_html.contains("dog"));
        assert_eq!(
            first.card_id,
            backend.next_review_preview().unwrap().unwrap().card_id
        );
        assert_eq!(backend.due_counts().unwrap().0, 1);
        assert!(backend.grade_previewed_card(first.card_id + 1, 4).is_err());
        backend.grade_previewed_card(first.card_id, 4).unwrap();
        let reviews: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "SELECT count(*) FROM revlog WHERE cid=?1",
                [first.card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reviews, 1);
        backend.close().unwrap();
        let reopened = AnkiBackend::open_managed_profile(root.path(), "review").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["perro", "dog"]);
        reopened.close().unwrap();
    }

    #[test]
    fn native_note_browser_paginates_and_uses_anki_search_rules() {
        let mut backend = AnkiBackend::in_memory().unwrap();
        let needle = backend.add_basic_note(1, "banco", "bank").unwrap();
        backend.add_basic_note(1, "calle", "street").unwrap();
        assert_eq!(backend.browse_notes("", 1).unwrap().len(), 1);
        let matched = backend.browse_notes("front:banco", 10).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, needle);
        assert_eq!(matched[0].fields, ["banco", "bank"]);
        assert!(backend.browse_notes("front:banco", 0).is_err());
        assert!(backend.browse_notes("", 101).is_err());
        backend
            .edit_note_fields(needle, &["banca", "bench"])
            .unwrap();
        assert_eq!(backend.browse_notes("front:banca", 10).unwrap().len(), 1);
        backend.undo_last().unwrap();
        assert_eq!(backend.browse_notes("front:banco", 10).unwrap().len(), 1);
    }

    #[test]
    fn corrupt_managed_collection_is_never_reinitialized_or_overwritten() {
        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "damaged").unwrap();
        backend.add_basic_note(1, "original", "important").unwrap();
        backend.close().unwrap();

        // Model interrupted or damaged storage on a disposable profile only.
        let db = root.path().join("damaged/collection.anki2");
        let broken = b"simulated truncated sqlite collection";
        fs::write(&db, broken).unwrap();
        assert!(AnkiBackend::open_managed_profile(root.path(), "damaged").is_err());
        assert_eq!(fs::read(&db).unwrap(), broken);
        assert!(AnkiBackend::create_managed_profile(root.path(), "damaged").is_err());
        assert_eq!(fs::read(&db).unwrap(), broken);
    }

    #[test]
    fn deck_note_template_edit_delete_undo_and_reopen_roundtrip() {
        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "learning").unwrap();
        let deck_id = backend.create_deck("Bidirectional").unwrap();
        assert!(backend
            .list_decks()
            .unwrap()
            .iter()
            .any(|deck| deck.0 == deck_id));
        let kind = "Retivus Bidirectional";
        let type_id = backend.create_bidirectional_notetype(kind).unwrap();
        assert!(type_id > 0);
        assert!(backend.create_bidirectional_notetype(kind).is_err());
        assert!(backend
            .list_notetypes()
            .unwrap()
            .iter()
            .any(|item| item.0 == kind && item.1 == 2));
        assert!(backend
            .add_note_with_notetype(deck_id, kind, &["one"])
            .is_err());

        let note_id = backend
            .add_note_with_notetype(deck_id, kind, &["uno", "one"])
            .unwrap();
        assert_eq!(backend.note_fields(note_id).unwrap(), ["uno", "one"]);
        assert_eq!(
            backend
                .collection
                .search_cards("front:uno", SortMode::NoOrder)
                .unwrap()
                .len(),
            2
        );
        backend.edit_note_fields(note_id, &["dos", "two"]).unwrap();
        assert_eq!(backend.note_fields(note_id).unwrap(), ["dos", "two"]);
        assert_eq!(
            backend
                .collection
                .search_cards("front:dos", SortMode::NoOrder)
                .unwrap()
                .len(),
            2
        );
        backend.remove_note(note_id).unwrap();
        assert!(backend.note_fields(note_id).is_err());
        backend.undo_last().unwrap();
        assert_eq!(backend.note_fields(note_id).unwrap(), ["dos", "two"]);
        backend.close().unwrap();

        let mut reopened = AnkiBackend::open_managed_profile(tmp.path(), "learning").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["dos", "two"]);
        assert!(reopened
            .list_decks()
            .unwrap()
            .iter()
            .any(|deck| deck.0 == deck_id));
        assert!(reopened
            .list_notetypes()
            .unwrap()
            .iter()
            .any(|item| item.0 == kind && item.1 == 2));
        reopened.close().unwrap();
    }

    #[test]
    fn deck_and_template_changes_delete_undo_and_reopen_safely() {
        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "changes").unwrap();
        let deck_id = backend.create_deck("Original Deck").unwrap();
        let kind = "Temporary Reverse";
        backend.create_bidirectional_notetype(kind).unwrap();
        let note_id = backend
            .add_note_with_notetype(deck_id, kind, &["casa", "house"])
            .unwrap();

        assert!(backend.rename_deck(deck_id, " ").is_err());
        backend.rename_deck(deck_id, "Renamed Deck").unwrap();
        assert!(backend
            .list_decks()
            .unwrap()
            .iter()
            .any(|(id, name)| *id == deck_id && name == "Renamed Deck"));

        assert!(backend.rename_notetype(kind, "Basic").is_err());
        backend.rename_notetype(kind, "Renamed Reverse").unwrap();
        assert!(backend
            .update_card_template("Renamed Reverse", 5, "Missing", "{{Back}}", "{{Front}}")
            .is_err());
        backend
            .update_card_template(
                "Renamed Reverse",
                1,
                "Reverse verified",
                "{{Back}}",
                "{{Front}}",
            )
            .unwrap();
        let template = backend
            .collection
            .get_notetype_by_name("Renamed Reverse")
            .unwrap()
            .unwrap();
        assert_eq!(template.templates[1].name, "Reverse verified");
        assert_eq!(backend.note_fields(note_id).unwrap(), ["casa", "house"]);

        assert!(backend.delete_deck_and_cards(1, true).is_err());
        assert!(backend.delete_deck_and_cards(deck_id, false).is_err());
        assert_eq!(backend.note_fields(note_id).unwrap(), ["casa", "house"]);

        backend.delete_deck_and_cards(deck_id, true).unwrap();
        assert!(backend.note_fields(note_id).is_err());
        backend.undo_last().unwrap();
        assert_eq!(backend.note_fields(note_id).unwrap(), ["casa", "house"]);
        assert!(backend
            .list_decks()
            .unwrap()
            .iter()
            .any(|(id, name)| *id == deck_id && name == "Renamed Deck"));
        backend.close().unwrap();

        let mut reopened = AnkiBackend::open_managed_profile(tmp.path(), "changes").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["casa", "house"]);
        assert!(reopened
            .list_notetypes()
            .unwrap()
            .iter()
            .any(|(name, count)| name == "Renamed Reverse" && *count == 2));
        reopened.close().unwrap();
    }

    #[test]
    fn explicit_source_senses_make_separate_persistent_tagged_notes() {
        use crate::source_adapter::{SourceInfo, StudyTsvV1};
        use crate::source_store::SourceCatalog;
        use std::io::Cursor;

        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "independent".into(),
            edition: "v1".into(),
            lexical_language: "ja".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic test fixture".into(),
        })
        .unwrap();
        let data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n\
                    a\t生\tnoun\tone\tlife\n\
                    b\t生\tverb\ttwo\tto live\n";
        let mut source = SourceCatalog::in_memory().unwrap();
        source
            .import_snapshot("edition-1", &adapter, &mut Cursor::new(data))
            .unwrap();
        let life = source
            .select_sense("independent", "edition-1", "a", "one")
            .unwrap();
        let live = source
            .select_sense("independent", "edition-1", "b", "two")
            .unwrap();

        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "source").unwrap();
        let deck_id = backend.create_deck("Selected senses").unwrap();
        let first = backend.add_source_candidate(deck_id, &life).unwrap();
        let second = backend.add_source_candidate(deck_id, &live).unwrap();
        assert_ne!(first, second);
        assert_eq!(backend.note_fields(first).unwrap(), ["生", "life"]);
        assert_eq!(backend.note_fields(second).unwrap(), ["生", "to live"]);
        assert!(backend
            .note_tags(first)
            .unwrap()
            .contains(&life.source_tag()));
        assert!(backend
            .note_tags(second)
            .unwrap()
            .contains(&live.source_tag()));
        assert_ne!(life.source_tag(), live.source_tag());
        backend.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(tmp.path(), "source").unwrap();
        assert_eq!(reopened.note_fields(first).unwrap(), ["生", "life"]);
        assert_eq!(reopened.note_fields(second).unwrap(), ["生", "to live"]);
        assert!(reopened
            .note_tags(first)
            .unwrap()
            .contains(&life.source_tag()));
        assert!(reopened
            .note_tags(second)
            .unwrap()
            .contains(&live.source_tag()));
        reopened.close().unwrap();
    }

    #[test]
    fn approved_source_migration_preserves_review_history_and_supports_undo() {
        use crate::source_adapter::{SourceInfo, StudyTsvV1};
        use crate::source_store::SourceCatalog;
        use std::io::Cursor;

        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "disposable-provider".into(),
            edition: "2026".into(),
            lexical_language: "es".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic fixture".into(),
        })
        .unwrap();
        let before = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\na\tbanqueta\tnoun\ts1\tsidewalk\n";
        let after = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\nb\tbanqueta\tnoun\ts1\tpavement\n";
        let mut sources = SourceCatalog::in_memory().unwrap();
        sources
            .import_snapshot("old", &adapter, &mut Cursor::new(before))
            .unwrap();
        sources
            .import_snapshot("new", &adapter, &mut Cursor::new(after))
            .unwrap();
        let old = sources
            .select_sense("disposable-provider", "old", "a", "s1")
            .unwrap();
        let new = sources
            .select_sense("disposable-provider", "new", "b", "s1")
            .unwrap();

        let root = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(root.path(), "learner").unwrap();
        backend.enable_fsrs().unwrap();
        let note_id = backend.add_source_candidate(1, &old).unwrap();
        let card_id = backend.grade_next_card(4).unwrap().unwrap();
        assert!(backend
            .apply_approved_source_reconciliation(&sources, note_id, &old)
            .is_err());
        sources.record_reconciliation(&old, &new).unwrap();
        backend
            .apply_approved_source_reconciliation(&sources, note_id, &old)
            .unwrap();
        assert_eq!(
            backend.note_fields(note_id).unwrap(),
            ["banqueta", "pavement"]
        );
        assert!(backend
            .note_tags(note_id)
            .unwrap()
            .contains(&new.source_tag()));
        let history: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history, 1);

        backend.undo_last().unwrap();
        assert_eq!(
            backend.note_fields(note_id).unwrap(),
            ["banqueta", "sidewalk"]
        );
        assert!(backend
            .note_tags(note_id)
            .unwrap()
            .contains(&old.source_tag()));

        backend
            .edit_note_fields(note_id, &["custom text", "custom answer"])
            .unwrap();
        assert!(backend
            .apply_approved_source_reconciliation(&sources, note_id, &old)
            .is_err());
        assert_eq!(
            backend.note_fields(note_id).unwrap(),
            ["custom text", "custom answer"]
        );
        backend.undo_last().unwrap();
        backend
            .apply_approved_source_reconciliation(&sources, note_id, &old)
            .unwrap();
        backend.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(root.path(), "learner").unwrap();
        assert_eq!(
            reopened.note_fields(note_id).unwrap(),
            ["banqueta", "pavement"]
        );
        assert!(reopened
            .note_tags(note_id)
            .unwrap()
            .contains(&new.source_tag()));
        let history: i64 = reopened
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history, 1);
        reopened.close().unwrap();
        sources
            .undo_reconciliation("disposable-provider", "old", "a", "s1")
            .unwrap();
        assert_eq!(
            sources
                .reconciled_sense("disposable-provider", "old", "a", "s1")
                .unwrap()
                .source_tag(),
            old.source_tag()
        );
    }

    #[test]
    fn fsrs_review_grade_and_event_survive_managed_profile_restart() {
        let tmp = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(tmp.path(), "fsrs").unwrap();
        backend.enable_fsrs().unwrap();
        assert!(backend.fsrs_enabled());
        let note_id = backend.add_basic_note(1, "cielo", "sky").unwrap();
        assert!(backend.due_counts().unwrap().0 >= 1);
        // Interrupt before answering: reopening must preserve the due card.
        backend.close().unwrap();
        let mut backend = AnkiBackend::open_managed_profile(tmp.path(), "fsrs").unwrap();
        assert!(backend.fsrs_enabled());
        assert!(backend.due_counts().unwrap().0 >= 1);
        assert!(backend.grade_next_card(0).is_err());
        let card_id = backend.grade_next_card(3).unwrap().unwrap();
        assert!(card_id > 0);
        let events: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
        backend.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(tmp.path(), "fsrs").unwrap();
        assert!(reopened.fsrs_enabled());
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["cielo", "sky"]);
        let persisted: i64 = reopened
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, 1);
        reopened.close().unwrap();
    }

    #[test]
    fn backup_snapshot_restores_review_history_without_undoing_newer_changes() {
        let root = tempdir().unwrap();
        let mut original = AnkiBackend::create_managed_profile(root.path(), "original").unwrap();
        original.enable_fsrs().unwrap();
        let saved_note = original.add_basic_note(1, "banqueta", "sidewalk").unwrap();
        let card = original.grade_next_card(4).unwrap().unwrap();
        let backup_file = root.path().join("verified-backup.sqlite3");
        original.backup_to(&backup_file).unwrap();

        // A second backup cannot overwrite even if the source has changed.
        let late_note = original.add_basic_note(1, "después", "after").unwrap();
        assert!(original.backup_to(&backup_file).is_err());
        assert!(
            AnkiBackend::restore_backup_as_new_profile(root.path(), "original", &backup_file)
                .is_err()
        );
        assert_eq!(
            original.note_fields(late_note).unwrap(),
            ["después", "after"]
        );
        original.close().unwrap();

        let recovered =
            AnkiBackend::restore_backup_as_new_profile(root.path(), "recovered", &backup_file)
                .unwrap();
        assert!(recovered.fsrs_enabled());
        assert_eq!(
            recovered.note_fields(saved_note).unwrap(),
            ["banqueta", "sidewalk"]
        );
        assert!(recovered.note_fields(late_note).is_err());
        let (count, ease): (i64, i64) = recovered
            .collection
            .storage
            .db()
            .query_row(
                "select count(*), max(ease) from revlog where cid=?1",
                [card],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, ease), (1, 4));
        recovered.close().unwrap();
        assert_eq!(
            fs::read_dir(root.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(".retivus-"))
                .count(),
            0,
            "staging files and auxiliary Anki files must be cleaned after success"
        );

        let reopened = AnkiBackend::open_managed_profile(root.path(), "original").unwrap();
        assert_eq!(
            reopened.note_fields(late_note).unwrap(),
            ["después", "after"]
        );
        reopened.close().unwrap();
        assert!(AnkiBackend::in_memory()
            .unwrap()
            .backup_to(&backup_file)
            .is_err());
    }

    #[test]
    fn corrupt_or_symlinked_backups_cannot_create_recovered_profiles() {
        let root = tempdir().unwrap();
        let invalid = root.path().join("not-sqlite.db");
        fs::write(&invalid, b"not a collection").unwrap();
        assert!(
            AnkiBackend::restore_backup_as_new_profile(root.path(), "invalid", &invalid).is_err()
        );
        assert!(!root.path().join("invalid").exists());

        let intact = root.path().join("already-exists.db");
        fs::write(&intact, b"keep intact").unwrap();
        let mut original = AnkiBackend::create_managed_profile(root.path(), "source").unwrap();
        original.add_basic_note(1, "x", "y").unwrap();
        assert!(original.backup_to(&intact).is_err());
        assert_eq!(fs::read(&intact).unwrap(), b"keep intact");
        assert!(original.backup_to(Path::new("relative.db")).is_err());
        original.close().unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&invalid, root.path().join("linked.db")).unwrap();
            assert!(AnkiBackend::restore_backup_as_new_profile(
                root.path(),
                "linked",
                &root.path().join("linked.db")
            )
            .is_err());
            assert!(!root.path().join("linked").exists());
        }
    }

    #[test]
    fn fsrs_rating_fixture_records_each_choice_and_upstream_queue_transition() {
        for rating in 1_u8..=4 {
            let mut backend = AnkiBackend::in_memory().unwrap();
            backend.enable_fsrs().unwrap();
            assert!(backend.fsrs_enabled());
            let note_id = backend
                .add_basic_note(1, &format!("term {rating}"), "definition")
                .unwrap();
            assert_eq!(backend.due_counts().unwrap().0, 1);
            let before = backend.next_card_id().unwrap().unwrap();

            let card_id = backend.grade_next_card(rating).unwrap().unwrap();
            assert_eq!(card_id, before);
            assert_eq!(
                backend.note_fields(note_id).unwrap(),
                [format!("term {rating}"), "definition".to_owned()]
            );
            let (queue, reps): (i64, i64) = backend
                .collection
                .storage
                .db()
                .query_row(
                    "select queue, reps from cards where id=?1",
                    [card_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(reps, 1);
            // Anki default learning steps: Again stays in learning; Easy
            // graduates immediately. Intermediate choices use upstream FSRS.
            if rating == 1 {
                assert_eq!(queue, 1);
            } else if rating == 4 {
                assert_eq!(queue, 2);
            }
            let recorded_rating: i64 = backend
                .collection
                .storage
                .db()
                .query_row("select ease from revlog where cid=?1", [card_id], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(recorded_rating, i64::from(rating));
        }
    }

    #[test]
    fn undoing_fsrs_review_reverses_revlog_without_erasing_note() {
        let mut backend = AnkiBackend::in_memory().unwrap();
        backend.enable_fsrs().unwrap();
        let note = backend.add_basic_note(1, "agua", "water").unwrap();
        let card_id = backend.next_card_id().unwrap().unwrap();
        assert_eq!(backend.grade_next_card(4).unwrap(), Some(card_id));
        backend.undo_last().unwrap();

        let event_count: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [card_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 0);
        assert_eq!(backend.note_fields(note).unwrap(), ["agua", "water"]);
        assert_eq!(backend.next_card_id().unwrap(), Some(card_id));
    }

    #[test]
    fn independent_japanese_spanish_source_can_be_reviewed_without_mixing_homographs() {
        use crate::source_adapter::{SourceInfo, StudyTsvV1};
        use crate::source_store::SourceCatalog;
        use std::io::Cursor;

        let input = include_str!("../tests/fixtures/independent-ja-es.tsv");
        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "independent-ja-es".into(),
            edition: "2026-11-01".into(),
            lexical_language: "ja".into(),
            definition_language: "es".into(),
            license_notice: "Original test fixture: CC0-1.0".into(),
        })
        .unwrap();
        let mut catalog = SourceCatalog::in_memory().unwrap();
        assert_eq!(
            catalog
                .import_snapshot("ja-es-v1", &adapter, &mut Cursor::new(input))
                .unwrap(),
            3
        );
        let life = catalog
            .select_sense("independent-ja-es", "ja-es-v1", "ja1", "s1")
            .unwrap();
        let live = catalog
            .select_sense("independent-ja-es", "ja-es-v1", "ja2", "s1")
            .unwrap();
        assert_eq!(life.word, live.word);
        assert_ne!(life.definition, live.definition);
        assert_ne!(life.source_tag(), live.source_tag());

        let dir = tempdir().unwrap();
        let mut backend = AnkiBackend::create_managed_profile(dir.path(), "independent").unwrap();
        backend.enable_fsrs().unwrap();
        let life_id = backend.add_source_candidate(1, &life).unwrap();
        let live_id = backend.add_source_candidate(1, &live).unwrap();
        assert_ne!(life_id, live_id);
        assert_eq!(backend.note_fields(life_id).unwrap(), ["生", "vida"]);
        assert_eq!(backend.note_fields(live_id).unwrap(), ["生", "vivir"]);
        assert!(backend
            .note_tags(life_id)
            .unwrap()
            .contains(&life.source_tag()));
        assert!(backend
            .note_tags(live_id)
            .unwrap()
            .contains(&live.source_tag()));
        assert!(backend.due_counts().unwrap().0 >= 2);
        let answered = backend.grade_next_card(3).unwrap().unwrap();
        let count: i64 = backend
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [answered],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        backend.close().unwrap();

        let reopened = AnkiBackend::open_managed_profile(dir.path(), "independent").unwrap();
        assert_eq!(reopened.note_fields(life_id).unwrap(), ["生", "vida"]);
        assert_eq!(reopened.note_fields(live_id).unwrap(), ["生", "vivir"]);
        assert!(reopened
            .note_tags(life_id)
            .unwrap()
            .contains(&life.source_tag()));
        assert!(reopened
            .note_tags(live_id)
            .unwrap()
            .contains(&live.source_tag()));
        let saved_count: i64 = reopened
            .collection
            .storage
            .db()
            .query_row(
                "select count(*) from revlog where cid=?1",
                [answered],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved_count, 1);
        reopened.close().unwrap();
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
