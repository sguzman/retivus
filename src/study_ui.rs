//! Explicit native learner-workspace operations behind the Anki feature gate.
//! No profile opens, is created, or is migrated until a user chooses an action.
use eframe::egui;
use retivus::anki_backend::{AnkiBackend, CollectionStats, NoteRow, ReviewPreview};
use retivus::source_adapter::{SourceAdapter, SourceRecord};
use retivus::source_store::{SnapshotSenseChange, SourceCardCandidate, SourceCatalog};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BrowseScope {
    #[default]
    All,
    SourceLinked,
    Plain,
    Conflicts,
}

impl BrowseScope {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All notes",
            Self::SourceLinked => "Source-linked",
            Self::Plain => "Without source",
            Self::Conflicts => "Source conflicts",
        }
    }

    fn matches(self, note: &NoteRow) -> bool {
        match self {
            Self::All => true,
            Self::SourceLinked => {
                matches!(StudyPanel::source_provenance(&note.tags), Ok(Some(_)))
            }
            Self::Plain => matches!(StudyPanel::source_provenance(&note.tags), Ok(None)),
            Self::Conflicts => StudyPanel::source_provenance(&note.tags).is_err(),
        }
    }
}

#[derive(Default)]
pub struct StudyPanel {
    root: String,
    profile_id: String,
    active_root: Option<PathBuf>,
    active_profile_id: Option<String>,
    package: String,
    import_with_scheduling: bool,
    export_with_scheduling: bool,
    snapshot_id: String,
    backend: Option<AnkiBackend>,
    decks: Vec<(i64, String)>,
    deck_id: Option<i64>,
    new_deck: String,
    front: String,
    back: String,
    query: String,
    notes: Vec<NoteRow>,
    browse_scope: BrowseScope,
    stats: CollectionStats,
    selected_note: Option<i64>,
    source_report: String,
    mapping_to_snapshot: String,
    mapping_to_entry: String,
    mapping_to_sense: String,
    mapping_preview: Option<SourceCardCandidate>,
    mapping_approved: bool,
    diff_provider: String,
    diff_before: String,
    diff_after: String,
    diff_offset: usize,
    diff_rows: Vec<SnapshotSenseChange>,
    diff_status: String,
    confirm_delete: bool,
    review: Option<ReviewPreview>,
    revealed: bool,
    due: (usize, usize, usize),
    status: String,
}

impl Drop for StudyPanel {
    fn drop(&mut self) {
        // Window shutdown must release a live Anki collection explicitly.
        if let Some(backend) = self.backend.take() {
            let _ = backend.close();
        }
    }
}

/// Present changing status text as a polite native accessibility live region.
/// This augments normal visual text; it never changes focus or repeats a dialog.
fn live_status(ui: &mut egui::Ui, text: &str) {
    let response = ui.label(text);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
    });
}

impl StudyPanel {
    /// Seed only development/CI's disposable learner profile. The returned
    /// state is rendered by the real native learner UI, not a mock painter.
    #[cfg(debug_assertions)]
    pub(super) fn seed_visual_qa(&mut self, scene: &str) -> Result<(), String> {
        if scene != "review" && scene != "conflict" {
            return Err("Unknown visual QA scene".into());
        }
        let root = std::env::temp_dir().join(format!("retivus-visual-qa-{}", std::process::id()));
        fs::create_dir(&root).map_err(|error| format!("QA temp root: {error}"))?;
        self.root = root.to_string_lossy().into_owned();
        self.profile_id = "retivus-disposable-qa".into();
        self.connect(true);
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| format!("QA profile unavailable: {}", self.status))?;
        backend.enable_fsrs().map_err(|error| error.to_string())?;
        let deck = backend
            .create_deck("Accessibility fixture")
            .map_err(|error| error.to_string())?;
        let note_id = backend
            .add_basic_note(deck, "¿Qué tal? 日本語", "Hello / hola")
            .map_err(|error| error.to_string())?;
        self.deck_id = Some(deck);
        self.refresh();
        self.refresh_review();
        if self.review.is_none() {
            return Err("Disposable review card is not due".into());
        }
        match scene {
            "review" => {
                self.revealed = true;
                self.status = "QA review fixture: answer revealed, grade controls ready.".into();
            }
            "conflict" => {
                let row = self
                    .notes
                    .iter_mut()
                    .find(|row| row.id == note_id)
                    .ok_or("Disposable QA note missing from browse")?;
                // Malformed identity is only in the in-memory displayed row:
                // the Anki collection is never corrupted.
                row.tags = vec!["retivus::source::not-hex".into()];
                self.selected_note = Some(note_id);
                self.front = "¿Qué tal? 日本語".into();
                self.back = "Hello / hola".into();
                self.browse_scope = BrowseScope::Conflicts;
                self.status = "QA conflict fixture: malformed synthetic source identity.".into();
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn connect(&mut self, create: bool) {
        if self.backend.is_some() {
            self.status = "Close the active profile before switching profiles.".into();
            return;
        }
        let root = Path::new(&self.root);
        let canonical_root = match fs::canonicalize(root) {
            Ok(canonical) => canonical,
            Err(error) => {
                self.status = format!("Profile root cannot be opened: {error}");
                return;
            }
        };
        let result = if create {
            AnkiBackend::create_managed_profile(root, &self.profile_id)
        } else {
            AnkiBackend::open_managed_profile(root, &self.profile_id)
        };
        match result {
            Ok(backend) => {
                self.active_root = Some(canonical_root);
                self.active_profile_id = Some(self.profile_id.clone());
                self.backend = Some(backend);
                self.status = format!("Opened Retivus-managed profile {}.", self.profile_id);
                self.refresh();
                self.refresh_review();
            }
            Err(error) => self.status = format!("Profile was not opened: {error}"),
        }
    }

    fn refresh(&mut self) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        match (
            backend.list_decks(),
            backend.browse_notes(&self.query, 50),
            backend.current_review_deck(),
        ) {
            (Ok(decks), Ok(notes), Ok(current_deck)) => {
                self.decks = decks;
                if !self.decks.iter().any(|(id, _)| Some(*id) == self.deck_id) {
                    self.deck_id = self
                        .decks
                        .iter()
                        .find(|(id, _)| *id == current_deck)
                        .or_else(|| self.decks.first())
                        .map(|(id, _)| *id);
                }
                self.notes = notes;
                self.source_report.clear();
                if self
                    .selected_note
                    .is_some_and(|id| !self.notes.iter().any(|note| note.id == id))
                {
                    self.selected_note = None;
                    self.confirm_delete = false;
                    self.front.clear();
                    self.back.clear();
                }
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                self.source_report.clear();
                self.notes.clear();
                self.selected_note = None;
                self.confirm_delete = false;
                self.front.clear();
                self.back.clear();
                self.status = format!("Could not browse profile: {error}");
            }
        }
    }

    fn refresh_review(&mut self) {
        let Some(backend) = self.backend.as_mut() else {
            self.review = None;
            self.due = (0, 0, 0);
            return;
        };
        self.revealed = false;
        if let Some(deck_id) = self.deck_id {
            if let Err(error) = backend.select_review_deck(deck_id) {
                self.review = None;
                self.due = (0, 0, 0);
                self.status = format!("Could not select review deck: {error}");
                return;
            }
        }
        match (
            backend.due_counts(),
            backend.next_review_preview(),
            backend.collection_stats(),
        ) {
            (Ok(due), Ok(review), Ok(stats)) => {
                self.due = due;
                self.review = review;
                self.stats = stats;
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                self.review = None;
                self.stats = CollectionStats::default();
                self.status = format!("Could not read learner statistics or queue: {error}");
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.backend.is_some()
    }

    /// Catalog I/O must always follow the actual open profile, never editable
    /// text fields. This also refuses a changed or symlinked profile directory.
    fn managed_catalog_path(&self) -> Result<PathBuf, String> {
        if self.backend.is_none() {
            return Err("open a Retivus-managed profile first".into());
        }
        let root = self
            .active_root
            .as_ref()
            .ok_or("learner profile root is not pinned")?;
        let id = self
            .active_profile_id
            .as_deref()
            .ok_or("learner profile identity is not pinned")?;
        let dir = root.join(id);
        let meta = fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
        if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
            return Err("learner profile must be a real Retivus-managed directory".into());
        }
        Ok(dir.join("source-catalog.sqlite3"))
    }

    /// Stage a complete, validated generic source snapshot for future explicit
    /// sense selection and version comparison. Never creates or grades cards.
    /// Mexilex release ingestion remains guarded by external checksum checks.
    pub fn stage_generic_snapshot(
        &mut self,
        adapter: &dyn SourceAdapter,
        source_path: &Path,
    ) -> Result<usize, String> {
        if adapter.format_id() != "retivus-study-tsv-v1" {
            return Err("Only generic study TSV snapshots can be staged in the learner GUI".into());
        }
        adapter.descriptor().validate()?;
        let snapshot = if self.snapshot_id.trim().is_empty() {
            adapter.descriptor().edition.as_str()
        } else {
            self.snapshot_id.trim()
        };
        let file_info = fs::symlink_metadata(source_path).map_err(|e| e.to_string())?;
        if !file_info.file_type().is_file() || file_info.file_type().is_symlink() {
            return Err("generic source must be a regular file, not a symlink".into());
        }
        let input = fs::File::open(source_path).map_err(|e| e.to_string())?;
        let catalog_file = self.managed_catalog_path()?;
        let mut catalog = match fs::symlink_metadata(&catalog_file) {
            Ok(_) => SourceCatalog::open_existing(&catalog_file)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SourceCatalog::create_new(&catalog_file)?
            }
            Err(error) => return Err(error.to_string()),
        };
        let count = catalog.import_snapshot(snapshot, adapter, &mut BufReader::new(input))?;
        self.status =
            format!("Staged {count} source entries under {snapshot} without creating cards.");
        Ok(count)
    }

    /// Bounded source-only version diff. No Anki notes, scheduler state or
    /// learner history are loaded or changed by this comparison.
    fn load_source_differences(&mut self) {
        let result = (|| -> Result<Vec<SnapshotSenseChange>, String> {
            let path = self.managed_catalog_path()?;
            let catalog = SourceCatalog::open_existing(&path)?;
            catalog.diff_snapshots(
                self.diff_provider.trim(),
                self.diff_before.trim(),
                self.diff_after.trim(),
                50,
                self.diff_offset,
            )
        })();
        match result {
            Ok(rows) => {
                self.diff_status = format!(
                    "Showing {} differences at offset {}. IDs are exact; no automatic merges.",
                    rows.len(),
                    self.diff_offset
                );
                self.diff_rows = rows;
            }
            Err(error) => {
                self.diff_rows.clear();
                self.diff_status = format!("Source comparison refused: {error}");
            }
        }
    }

    /// Read exact source identity and immutable record for the selected note.
    /// The catalog belongs to the open profile, never an arbitrary Anki path.
    fn selected_source_context(&self) -> Result<(i64, SourceCatalog, SourceCardCandidate), String> {
        let id = self.selected_note.ok_or("Select a learner note first")?;
        let backend = self
            .backend
            .as_ref()
            .ok_or("Open a learner profile first")?;
        let tags = backend.note_tags(id).map_err(|e| e.to_string())?;
        let Some([provider, snapshot, entry, sense]) = Self::source_provenance(&tags)? else {
            return Err("Selected note has no unambiguous external source identity".into());
        };
        let path = self.managed_catalog_path()?;
        let catalog = SourceCatalog::open_existing(&path)?;
        let original = catalog.select_sense(&provider, &snapshot, &entry, &sense)?;
        Ok((id, catalog, original))
    }

    fn preview_source_mapping(&mut self) {
        self.mapping_preview = None;
        self.mapping_approved = false;
        let outcome = (|| -> Result<SourceCardCandidate, String> {
            let (_, catalog, from) = self.selected_source_context()?;
            if self.mapping_to_snapshot.trim().is_empty()
                || self.mapping_to_entry.trim().is_empty()
                || self.mapping_to_sense.trim().is_empty()
            {
                return Err("Specify exact target snapshot, entry and sense".into());
            }
            let target = catalog.select_sense(
                &from.provenance.provider_id,
                self.mapping_to_snapshot.trim(),
                self.mapping_to_entry.trim(),
                self.mapping_to_sense.trim(),
            )?;
            if from.snapshot == target.snapshot {
                return Err("A version mapping must use a distinct source snapshot".into());
            }
            if from.provenance.lexical_language != target.provenance.lexical_language
                || from.provenance.definition_language != target.provenance.definition_language
            {
                return Err("Source languages differ; mapping is not permitted".into());
            }
            Ok(target)
        })();
        match outcome {
            Ok(target) => {
                self.status = format!(
                    "Exact target found: {} → {}. Review the meaning and license before approval.",
                    target.word, target.definition
                );
                self.mapping_preview = Some(target);
            }
            Err(error) => self.status = format!("Mapping preview refused: {error}"),
        }
    }

    fn record_source_mapping(&mut self) {
        let result = (|| -> Result<String, String> {
            if !self.mapping_approved {
                return Err("Preview the exact target and explicitly approve the mapping".into());
            }
            let target = self
                .mapping_preview
                .as_ref()
                .ok_or("No verified target has been previewed")?;
            if target.snapshot != self.mapping_to_snapshot.trim()
                || target.entry_id != self.mapping_to_entry.trim()
                || target.sense_id != self.mapping_to_sense.trim()
            {
                return Err("Mapping target changed after preview".into());
            }
            let (id, mut catalog, from) = self.selected_source_context()?;
            let backend = self
                .backend
                .as_ref()
                .ok_or("Open a learner profile first")?;
            let fields = backend.note_fields(id).map_err(|e| e.to_string())?;
            if fields.len() != 2 || fields[0] != from.word || fields[1] != from.definition {
                return Err("Learner note was edited; resolve the conflict before mapping".into());
            }
            catalog.record_reconciliation(&from, target)?;
            Ok(format!(
                "Recorded explicit mapping to {} / {} / {}. Note and reviews remain unchanged.",
                target.snapshot, target.entry_id, target.sense_id
            ))
        })();
        self.mapping_approved = false;
        self.status = match result {
            Ok(message) => message,
            Err(error) => format!("Source mapping not recorded: {error}"),
        };
        self.inspect_selected_source();
    }

    /// Reverse only the catalog's recorded one-to-one source mapping.
    /// Anki-owned notes and review events are never changed by this operation.
    fn undo_source_mapping(&mut self) {
        let result = (|| -> Result<(), String> {
            let (_, mut catalog, from) = self.selected_source_context()?;
            catalog.undo_reconciliation(
                &from.provenance.provider_id,
                &from.snapshot,
                &from.entry_id,
                &from.sense_id,
            )
        })();
        self.mapping_approved = false;
        self.mapping_preview = None;
        self.status = match result {
            Ok(()) => "Source mapping removed; the note and review history are unchanged.".into(),
            Err(error) => format!("Source mapping not removed: {error}"),
        };
        self.inspect_selected_source();
    }

    /// This is a distinct user action after mapping: the Anki backend verifies
    /// unmodified source fields, unique provenance and preserves review history.
    fn apply_recorded_source_mapping(&mut self) {
        let result = (|| -> Result<(), String> {
            let (id, catalog, original) = self.selected_source_context()?;
            let backend = self
                .backend
                .as_mut()
                .ok_or("Open a learner profile first")?;
            backend
                .apply_approved_source_reconciliation(&catalog, id, &original)
                .map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => {
                self.status =
                    "Source revision applied to this note; Anki undo is available.".into();
                self.mapping_preview = None;
                self.mapping_approved = false;
                self.refresh();
                self.refresh_review();
            }
            Err(error) => self.status = format!("Source revision not applied: {error}"),
        }
    }

    /// Compare the selected note with its immutable source payload and any
    /// explicitly recorded one-to-one migration. This never changes cards.
    fn inspect_selected_source(&mut self) {
        let outcome = (|| -> Result<String, String> {
            let note_id = self.selected_note.ok_or("Select a learner note first")?;
            let backend = self
                .backend
                .as_ref()
                .ok_or("Open a learner profile first")?;
            let tags = backend.note_tags(note_id).map_err(|e| e.to_string())?;
            let Some([provider, snapshot, entry, sense]) = Self::source_provenance(&tags)? else {
                return Ok("This note has no linked external source.".into());
            };
            let path = self.managed_catalog_path()?;
            let catalog = SourceCatalog::open_existing(&path)
                .map_err(|e| format!("Source catalog unavailable or unsafe: {e}"))?;
            let original = catalog
                .select_sense(&provider, &snapshot, &entry, &sense)
                .map_err(|e| format!("Source identity missing or invalid: {e}"))?;
            let fields = backend.note_fields(note_id).map_err(|e| e.to_string())?;
            if fields.len() != 2 || fields[0] != original.word || fields[1] != original.definition {
                return Ok("Conflict: this note was edited independently of its source; automatic reconciliation is prohibited.".into());
            }
            let mapped = catalog
                .reconciled_sense(&provider, &snapshot, &entry, &sense)
                .map_err(|e| format!("Source reconciliation cannot be read: {e}"))?;
            if mapped.source_tag() != original.source_tag() {
                Ok(format!(
                    "Update available by an explicit source mapping: {} / {} / {}. The note and review history remain unchanged until separately approved.",
                    mapped.snapshot, mapped.entry_id, mapped.sense_id
                ))
            } else {
                Ok(format!(
                    "Source verified: exact immutable {provider} / {snapshot} / {entry} / {sense} matches this note."
                ))
            }
        })();
        self.source_report = match outcome {
            Ok(report) => report,
            Err(error) => format!("Conflict: {error}"),
        };
    }

    /// Use one guarded path for both the grade buttons and automated UI-state tests.
    /// Import a validated source into a durable provider catalog before an
    /// explicit sense can become a learner note. No homograph guessing.
    pub fn add_validated_source_choice(
        &mut self,
        adapter: &dyn SourceAdapter,
        source_path: &Path,
        record: &SourceRecord,
        sense_id: &str,
    ) -> Result<i64, String> {
        let deck_id = self.deck_id.ok_or("choose a learner deck first")?;
        if self.backend.is_none() {
            return Err("open a Retivus-managed profile first".into());
        }
        let origin = adapter.descriptor();
        origin.validate()?;
        if record.provenance.provider_id != origin.provider_id
            || record.provenance.lexical_language != origin.lexical_language
            || record.provenance.definition_language != origin.definition_language
        {
            return Err("preview provider identity differs from the source adapter".into());
        }
        let sense = record
            .senses
            .iter()
            .find(|sense| sense.sense_id == sense_id)
            .ok_or("selected sense no longer exists")?;
        let snapshot = if self.snapshot_id.trim().is_empty() {
            record.provenance.edition.clone()
        } else {
            self.snapshot_id.trim().to_owned()
        };
        let expected = SourceCardCandidate {
            provenance: record.provenance.clone(),
            snapshot: snapshot.clone(),
            entry_id: record.entry_id.clone(),
            sense_id: sense_id.to_owned(),
            word: record.word.clone(),
            part_of_speech: record.part_of_speech.clone(),
            definition: sense.definition.clone(),
        };
        let catalog_file = self.managed_catalog_path()?;
        let is_mexilex = adapter.format_id() == "mexilex-study-jsonl-v1";
        let mut catalog = match fs::symlink_metadata(&catalog_file) {
            Ok(_) => SourceCatalog::open_existing(&catalog_file)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if is_mexilex {
                    return Err("Mexilex must first be imported from the checksum-verified release into this profile's source catalog".into());
                }
                SourceCatalog::create_new(&catalog_file)?
            }
            Err(error) => return Err(error.to_string()),
        };
        if catalog
            .lookup_entry(&origin.provider_id, &snapshot, &record.entry_id)?
            .is_none()
        {
            if is_mexilex {
                return Err(
                    "Mexilex source snapshot is not in the verified release catalog".into(),
                );
            }
            let file = fs::symlink_metadata(source_path).map_err(|e| e.to_string())?;
            if !file.is_file() || file.file_type().is_symlink() {
                return Err("source must be a regular file, not a symlink".into());
            }
            let input = fs::File::open(source_path).map_err(|e| e.to_string())?;
            catalog.import_snapshot(&snapshot, adapter, &mut BufReader::new(input))?;
        }
        let actual =
            catalog.select_sense(&origin.provider_id, &snapshot, &record.entry_id, sense_id)?;
        if actual != expected {
            return Err("source selection differs from the immutable imported snapshot".into());
        }
        let note_id = self
            .backend
            .as_mut()
            .ok_or("learner profile closed during import")?
            .add_source_candidate(deck_id, &actual)
            .map_err(|e| e.to_string())?;
        self.status =
            format!("Created note {note_id} from an exact source sense; provenance saved.");
        self.refresh();
        self.refresh_review();
        Ok(note_id)
    }

    /// Accept exactly one valid Retivus source identity. Multiple source
    /// identities are a conflict, never a basis for automatic reconciliation.
    fn source_provenance(tags: &[String]) -> Result<Option<[String; 4]>, String> {
        let mut source_tags = tags
            .iter()
            .filter(|tag| tag.starts_with("retivus::source::"));
        let Some(tag) = source_tags.next() else {
            return Ok(None);
        };
        if source_tags.next().is_some() {
            return Err("multiple source identities require explicit resolution".into());
        }
        SourceCardCandidate::parse_source_tag(tag).map(Some)
    }

    fn grade_revealed(&mut self, rating: u8) {
        if !self.revealed {
            self.status = "Reveal the answer before choosing a rating.".into();
            return;
        }
        let Some(card_id) = self.review.as_ref().map(|card| card.card_id) else {
            self.status = "No review card is selected.".into();
            return;
        };
        let Some(backend) = self.backend.as_mut() else {
            self.status = "Open a Retivus-managed profile first.".into();
            return;
        };
        self.status = match backend.grade_previewed_card(card_id, rating) {
            Ok(()) => format!("Recorded rating {rating} using Anki's scheduler."),
            Err(error) => format!("Review not recorded: {error}"),
        };
        self.refresh_review();
    }

    /// Require explicit user confirmation before destructive note removal.
    /// The Anki backend still owns the transaction and provides undo.
    fn delete_selected_note(&mut self) {
        if !self.confirm_delete {
            self.status = "Confirm deletion before removing the selected note.".into();
            return;
        }
        self.confirm_delete = false;
        let Some(note_id) = self.selected_note else {
            self.status = "Select a note before deleting.".into();
            return;
        };
        let Some(backend) = self.backend.as_mut() else {
            self.status = "Open a learner profile before deleting.".into();
            return;
        };
        match backend.remove_note(note_id) {
            Ok(()) => {
                self.status =
                    format!("Deleted note {note_id}; Undo last Anki edit can restore it.");
                self.selected_note = None;
                self.source_report.clear();
                self.mapping_preview = None;
                self.mapping_approved = false;
                self.front.clear();
                self.back.clear();
                self.refresh();
                self.refresh_review();
            }
            Err(error) => self.status = format!("Note not deleted: {error}"),
        }
    }

    fn undo_last_edit(&mut self) {
        let Some(backend) = self.backend.as_mut() else {
            self.status = "Open a Retivus-managed profile first.".into();
            return;
        };
        match backend.undo_last() {
            Ok(()) => {
                self.status = "Last edit undone.".into();
                self.refresh();
                self.refresh_review();
            }
            Err(error) => self.status = format!("Cannot undo: {error}"),
        }
    }

    fn close(&mut self) {
        if let Some(backend) = self.backend.take() {
            self.status = match backend.close() {
                Ok(()) => "Closed the learner profile safely.".into(),
                Err(error) => format!("Profile close reported an error: {error}"),
            };
        }
        self.active_root = None;
        self.active_profile_id = None;
        self.decks.clear();
        self.notes.clear();
        self.browse_scope = BrowseScope::All;
        self.stats = CollectionStats::default();
        self.deck_id = None;
        self.selected_note = None;
        self.source_report.clear();
        self.mapping_to_snapshot.clear();
        self.mapping_to_entry.clear();
        self.mapping_to_sense.clear();
        self.mapping_preview = None;
        self.mapping_approved = false;
        self.diff_provider.clear();
        self.diff_before.clear();
        self.diff_after.clear();
        self.diff_rows.clear();
        self.diff_offset = 0;
        self.diff_status.clear();
        self.confirm_delete = false;
        // Never display editor contents, search terms, or file paths from
        // a previous profile after the learner switches collections.
        self.front.clear();
        self.back.clear();
        self.query.clear();
        self.package.clear();
        self.import_with_scheduling = false;
        self.export_with_scheduling = false;
        self.snapshot_id.clear();
        self.new_deck.clear();
        self.review = None;
        self.revealed = false;
        self.due = (0, 0, 0);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Learner workspace · managed Anki collections")
            .default_open(true)
            .show(ui, |ui| {
            ui.label("Choose an existing root directory and a Retivus profile ID. No ordinary Anki profile is opened automatically.");
            ui.horizontal_wrapped(|ui| {
                let root_label = ui.label("Profile root");
                ui.add_enabled(
                    self.backend.is_none(),
                    egui::TextEdit::singleline(&mut self.root).desired_width(280.0),
                )
                .labelled_by(root_label.id);
                let id_label = ui.label("Profile ID");
                ui.add_enabled(
                    self.backend.is_none(),
                    egui::TextEdit::singleline(&mut self.profile_id).desired_width(160.0),
                )
                .labelled_by(id_label.id);
            });
            ui.horizontal_wrapped(|ui| {
                let snapshot_label = ui.label("Source snapshot ID");
                ui.add(
                    egui::TextEdit::singleline(&mut self.snapshot_id)
                        .hint_text("blank = source edition")
                        .desired_width(240.0),
                )
                .labelled_by(snapshot_label.id);
                ui.weak("Mexilex must use its already checksum-verified catalog; raw JSONL is preview-only.");
            });
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(self.backend.is_none(), |ui| {
                    if ui.button("Create new profile").clicked() { self.connect(true); }
                    if ui.button("Open owned profile").clicked() { self.connect(false); }
                });
                if ui.add_enabled(self.backend.is_some(), egui::Button::new("Close profile")).clicked() {
                    self.close();
                }
            });
            if self.backend.is_some() {
                ui.separator();
                ui.heading("Decks and notes");
                ui.horizontal_wrapped(|ui| {
                    let previous_deck = self.deck_id;
                    let active_deck_label = ui.label("Active deck");
                    egui::ComboBox::from_id_salt("retivus-active-deck")
                        .selected_text(self.decks.iter().find(|(id, _)| Some(*id) == self.deck_id)
                            .map_or("Select deck", |(_, name)| name.as_str()))
                        .show_ui(ui, |ui| {
                            for (id, name) in &self.decks {
                                ui.selectable_value(&mut self.deck_id, Some(*id), name);
                            }
                        })
                        .response
                        .labelled_by(active_deck_label.id);
                    if self.deck_id != previous_deck {
                        self.refresh_review();
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.new_deck).hint_text("New deck name").desired_width(200.0));
                    if ui.button("Create deck").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.create_deck(&self.new_deck) {
                                Ok(id) => {
                                    self.deck_id = Some(id);
                                    self.new_deck.clear();
                                    self.status = "New deck created.".into();
                                    self.refresh();
                                    self.refresh_review();
                                }
                                Err(error) => self.status = format!("Deck not created: {error}"),
                            }
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let search_label = ui.label("Search notes");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Anki search syntax")
                            .desired_width(280.0),
                    )
                    .labelled_by(search_label.id);
                    if ui.button("Search / refresh").clicked() { self.refresh(); }
                    ui.weak("Up to 50 matching notes; queries use Anki's own search parser.");
                });
                ui.horizontal_wrapped(|ui| {
                    let browse_label = ui.label("Browse filter");
                    egui::ComboBox::from_id_salt("retivus-browse-scope")
                        .selected_text(self.browse_scope.label())
                        .show_ui(ui, |ui| {
                            for scope in [
                                BrowseScope::All,
                                BrowseScope::SourceLinked,
                                BrowseScope::Plain,
                                BrowseScope::Conflicts,
                            ] {
                                ui.selectable_value(&mut self.browse_scope, scope, scope.label());
                            }
                        })
                        .response
                        .labelled_by(browse_label.id);
                    let matching = self
                        .notes
                        .iter()
                        .filter(|note| self.browse_scope.matches(note))
                        .count();
                    ui.weak(format!("{matching} matching of {} loaded (50 maximum)", self.notes.len()));
                });
                egui::ScrollArea::vertical().max_height(135.0).show(ui, |ui| {
                    let scope = self.browse_scope;
                    for note in self.notes.clone().into_iter().filter(|note| scope.matches(note)) {
                        let summary = note.fields.first().map_or("(empty note)", String::as_str);
                        let label = format!("{} · {}", note.id, summary);
                        if ui.selectable_label(self.selected_note == Some(note.id), label).clicked() {
                            self.selected_note = Some(note.id);
                            self.source_report.clear();
                            self.mapping_preview = None;
                            self.mapping_approved = false;
                            self.confirm_delete = false;
                            self.front = note.fields.first().cloned().unwrap_or_default();
                            self.back = note.fields.get(1).cloned().unwrap_or_default();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("New basic note").clicked() {
                        self.selected_note = None;
                        self.source_report.clear();
                        self.mapping_preview = None;
                        self.mapping_approved = false;
                        self.confirm_delete = false;
                        self.front.clear();
                        self.back.clear();
                    }
                    if ui.button("Undo last Anki edit").clicked() {
                        self.undo_last_edit();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let front_label = ui.label("Front");
                    ui.add(egui::TextEdit::singleline(&mut self.front).desired_width(220.0))
                        .labelled_by(front_label.id);
                    let back_label = ui.label("Back");
                    ui.add(egui::TextEdit::singleline(&mut self.back).desired_width(220.0))
                        .labelled_by(back_label.id);
                });
                if let Some(note) = self
                    .notes
                    .iter()
                    .find(|note| Some(note.id) == self.selected_note)
                {
                    match Self::source_provenance(&note.tags) {
                        Ok(Some([provider, snapshot, entry, sense])) => {
                            ui.collapsing("External source provenance", |ui| {
                                ui.label(format!("Provider: {provider}"));
                                ui.label(format!("Snapshot: {snapshot}"));
                                ui.label(format!("Entry: {entry} · sense: {sense}"));
                                ui.weak(
                                    "Source records and learner review history are separate. Updates require explicit reconciliation.",
                                );
                            });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let color = ui.visuals().error_fg_color;
                            ui.colored_label(
                                color,
                                format!("Source identity conflict: {error}"),
                            );
                        }
                    }
                }
                if self.selected_note.is_some() {
                    if ui.button("Verify note against source catalog").clicked() {
                        self.inspect_selected_source();
                    }
                    if !self.source_report.is_empty() {
                        if self.source_report.starts_with("Conflict:") {
                            let color = ui.visuals().error_fg_color;
                            let label = ui.colored_label(color, &self.source_report);
                            ui.ctx().accesskit_node_builder(label.id, |node| {
                                node.set_live(egui::accesskit::Live::Polite);
                            });
                        } else {
                            ui.label(&self.source_report);
                        }
                    }
                    ui.collapsing("Explicit source version reconciliation", |ui| {
                        ui.weak("Stage a new generic source snapshot first. Never match on spelling alone.");
                        let mut changed = false;
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Target snapshot");
                            changed |= ui.text_edit_singleline(&mut self.mapping_to_snapshot).changed();
                            ui.label("Entry ID");
                            changed |= ui.text_edit_singleline(&mut self.mapping_to_entry).changed();
                            ui.label("Sense ID");
                            changed |= ui.text_edit_singleline(&mut self.mapping_to_sense).changed();
                        });
                        if changed {
                            self.mapping_preview = None;
                            self.mapping_approved = false;
                        }
                        if ui.button("Preview exact target sense").clicked() {
                            self.preview_source_mapping();
                        }
                        if let Some(target) = self.mapping_preview.as_ref() {
                            ui.label(format!(
                                "Target: {} → {} · {} / {} / {}",
                                target.word, target.definition,
                                target.snapshot, target.entry_id, target.sense_id
                            ));
                            ui.weak(format!("License notice: {}", target.provenance.license_notice));
                            ui.checkbox(
                                &mut self.mapping_approved,
                                "I approve recording this exact one-to-one source mapping",
                            );
                            if ui
                                .add_enabled(
                                    self.mapping_approved,
                                    egui::Button::new("Record source mapping (no note changes)"),
                                )
                                .clicked()
                            {
                                self.record_source_mapping();
                            }
                        }
                        if ui.button("Apply already recorded mapping to selected note").clicked() {
                            self.apply_recorded_source_mapping();
                        }
                        if ui.button("Undo recorded mapping (no note changes)").clicked() {
                            self.undo_source_mapping();
                        }
                        ui.weak("Mapping and applying are separate actions; manual edits or ambiguous identities block migration.");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.confirm_delete, "Confirm removal of selected note");
                        if ui
                            .add_enabled(
                                self.confirm_delete,
                                egui::Button::new("Delete selected note"),
                            )
                            .clicked()
                        {
                            self.delete_selected_note();
                        }
                        ui.weak("Undo is available while this collection remains open.");
                    });
                }
                if let Some(id) = self.selected_note {
                    if ui.button("Save edited note").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.edit_note_fields(id, &[&self.front, &self.back]) {
                                Ok(()) => { self.status = "Note updated; undo is available.".into(); self.refresh(); self.refresh_review(); }
                                Err(error) => self.status = format!("Note not edited: {error}"),
                            }
                        }
                    }
                } else if ui.button("Add basic note to selected deck").clicked() {
                    match (self.backend.as_mut(), self.deck_id) {
                        (Some(backend), Some(deck_id)) => match backend.add_basic_note(deck_id, &self.front, &self.back) {
                            Ok(_) => { self.status = "Note added to selected deck.".into(); self.refresh(); self.refresh_review(); }
                            Err(error) => self.status = format!("Note not added: {error}"),
                        },
                        _ => self.status = "Select a deck first.".into(),
                    }
                }
                ui.separator();
                ui.collapsing("Compare staged source snapshots", |ui| {
                    ui.weak("Exact provider and snapshot keys only. Removed/added homographs are never automatically matched.");
                    let mut changed = false;
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Provider");
                        changed |= ui.text_edit_singleline(&mut self.diff_provider).changed();
                        ui.label("From");
                        changed |= ui.text_edit_singleline(&mut self.diff_before).changed();
                        ui.label("To");
                        changed |= ui.text_edit_singleline(&mut self.diff_after).changed();
                    });
                    if changed {
                        self.diff_rows.clear();
                        self.diff_offset = 0;
                        self.diff_status.clear();
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Compare").clicked() {
                            self.diff_offset = 0;
                            self.load_source_differences();
                        }
                        if ui
                            .add_enabled(self.diff_offset >= 50, egui::Button::new("Previous 50"))
                            .clicked()
                        {
                            self.diff_offset -= 50;
                            self.load_source_differences();
                        }
                        if ui
                            .add_enabled(self.diff_rows.len() == 50, egui::Button::new("Next 50"))
                            .clicked()
                        {
                            self.diff_offset = self.diff_offset.saturating_add(50);
                            self.load_source_differences();
                        }
                    });
                    if !self.diff_status.is_empty() {
                        ui.label(&self.diff_status);
                    }
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for change in &self.diff_rows {
                            let kind = match (&change.before, &change.after) {
                                (None, Some(_)) => "Added",
                                (Some(_), None) => "Removed",
                                _ => "Changed",
                            };
                            ui.collapsing(
                                format!(
                                    "{kind} · entry {} · sense {}",
                                    change.entry_id, change.sense_id
                                ),
                                |ui| {
                                    if let Some(before) = &change.before {
                                        ui.label(format!(
                                            "Before: {} ({}) — {}",
                                            before.word, before.part_of_speech, before.definition
                                        ));
                                    }
                                    if let Some(after) = &change.after {
                                        ui.label(format!(
                                            "After: {} ({}) — {}",
                                            after.word, after.part_of_speech, after.definition
                                        ));
                                    }
                                },
                            );
                        }
                    });
                });
                ui.heading("Profile statistics");
                ui.label(format!(
                    "{} notes · {} cards · {} recorded reviews (all decks)",
                    self.stats.notes, self.stats.cards, self.stats.review_events
                ));
                ui.heading("Study · Anki scheduler");
                ui.horizontal_wrapped(|ui| {
                    let due = format!(
                        "Due: {} new · {} learning · {} review",
                        self.due.0, self.due.1, self.due.2
                    );
                    live_status(ui, &due);
                    if ui.button("Refresh due queue").clicked() { self.refresh_review(); }
                    if ui.button("Enable FSRS").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.enable_fsrs() {
                                Ok(()) => { self.status = "FSRS scheduling enabled.".into(); self.refresh_review(); }
                                Err(error) => self.status = format!("Could not enable FSRS: {error}"),
                            }
                        }
                    }
                });
                if let Some(card) = self.review.clone() {
                    ui.label(format!("Card {} · question", card.card_id));
                    ui.label(egui::RichText::new(&card.question_html).monospace());
                    if self.revealed {
                        ui.separator();
                        ui.label("Answer");
                        ui.label(egui::RichText::new(&card.answer_html).monospace());
                        ui.horizontal_wrapped(|ui| {
                            for (rating, name) in [(1, "Again"), (2, "Hard"), (3, "Good"), (4, "Easy")] {
                                if ui.button(name).clicked() {
                                    self.grade_revealed(rating);
                                }
                            }
                        });
                    } else if ui.button("Show answer").clicked() {
                        self.revealed = true;
                    }
                    ui.weak("Card HTML is shown as inert source text; embedded HTML/scripts are not executed. Full rich-card rendering remains a separate UX requirement.");
                } else {
                    ui.weak("No queued cards. Refresh after importing or adding study notes.");
                }
                ui.separator();
                ui.heading("Anki package");
                ui.checkbox(
                    &mut self.import_with_scheduling,
                    "Import review history and scheduling from package",
                );
                ui.weak("Off by default; enabling imports another package's study state.");
                ui.checkbox(
                    &mut self.export_with_scheduling,
                    "Include private review history and scheduling in export",
                );
                ui.weak(
                    "Off by default for shareable decks; enabling includes personal study activity.",
                );
                ui.horizontal_wrapped(|ui| {
                    let package_label = ui.label("Absolute .apkg path");
                    ui.add(egui::TextEdit::singleline(&mut self.package).desired_width(300.0))
                        .labelled_by(package_label.id);
                    if ui.button("Import package").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.import_apkg_with_scheduling(
                                Path::new(&self.package),
                                self.import_with_scheduling,
                            ) {
                                Ok(()) => { self.status = "Package imported into this profile.".into(); self.refresh(); self.refresh_review(); }
                                Err(error) => self.status = format!("Import refused: {error}"),
                            }
                        }
                    }
                    if ui.button("Export new package").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.export_apkg_to_with_scheduling(
                                Path::new(&self.package),
                                self.export_with_scheduling,
                            ) {
                                Ok(count) => {
                                    let detail = if self.export_with_scheduling {
                                        "including personal review history"
                                    } else {
                                        "without personal review history"
                                    };
                                    self.status = format!("Exported {count} notes and media {detail}.");
                                }
                                Err(error) => self.status = format!("Export refused: {error}"),
                            }
                        }
                    }
                });
                ui.weak("Imports never open personal Anki profiles. Exports refuse existing files.");
            }
            if !self.status.is_empty() { live_status(ui, &self.status); }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_scope_distinguishes_source_notes_plain_notes_and_identity_conflicts() {
        fn note(tags: Vec<String>) -> NoteRow {
            NoteRow {
                id: 42,
                fields: vec!["word".into(), "definition".into()],
                tags,
            }
        }
        let candidate = SourceCardCandidate {
            provenance: retivus::source_adapter::SourceInfo {
                provider_id: "fixture".into(),
                edition: "v1".into(),
                lexical_language: "es-MX".into(),
                definition_language: "en".into(),
                license_notice: "Synthetic".into(),
            },
            snapshot: "v1".into(),
            entry_id: "one".into(),
            sense_id: "s1".into(),
            word: "banqueta".into(),
            part_of_speech: "noun".into(),
            definition: "sidewalk".into(),
        };
        let good = note(vec![candidate.source_tag()]);
        let plain = note(vec!["ordinary".into()]);
        let corrupt = note(vec![format!("{}::extra", candidate.source_tag())]);
        let duplicate = note(vec![candidate.source_tag(), candidate.source_tag()]);
        assert!(BrowseScope::All.matches(&good));
        assert!(BrowseScope::SourceLinked.matches(&good));
        assert!(!BrowseScope::Plain.matches(&good));
        assert!(BrowseScope::Plain.matches(&plain));
        assert!(!BrowseScope::SourceLinked.matches(&plain));
        assert!(BrowseScope::Conflicts.matches(&corrupt));
        assert!(BrowseScope::Conflicts.matches(&duplicate));
        assert!(!BrowseScope::SourceLinked.matches(&corrupt));
    }

    #[test]
    fn stateful_native_study_flow_requires_reveal_and_preserves_review_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = dir.path().to_string_lossy().into_owned();
        panel.profile_id = "fixture-learner".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let backend = panel.backend.as_mut().unwrap();
        backend.enable_fsrs().unwrap();
        let deck = backend.create_deck("Sentences").unwrap();
        let note = backend
            .add_basic_note(deck, "¿Qué tal?", "How are you?")
            .unwrap();
        panel.query = "front:*tal*".into();
        panel.refresh();
        assert!(panel
            .decks
            .iter()
            .any(|(id, name)| *id == deck && name == "Sentences"));
        assert_eq!(panel.notes.len(), 1);
        assert_eq!(panel.notes[0].id, note);
        panel.deck_id = Some(deck);
        panel.refresh_review();
        let first = panel.review.as_ref().unwrap().card_id;
        assert!(panel.due.0 >= 1);

        panel.grade_revealed(4);
        assert!(panel.status.contains("Reveal the answer"));
        assert_eq!(panel.review.as_ref().unwrap().card_id, first);
        panel.revealed = true;
        panel.grade_revealed(4);
        assert!(panel.status.contains("Recorded rating 4"));
        assert!(!panel.revealed);
        assert_ne!(panel.review.as_ref().map(|v| v.card_id), Some(first));
        panel.close();

        panel.connect(false);
        assert!(panel.backend.is_some(), "{}", panel.status);
        panel.refresh();
        assert_eq!(panel.notes[0].id, note);
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(note).unwrap(),
            ["¿Qué tal?", "How are you?"]
        );
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(first)
                .unwrap(),
            1
        );
        panel.close();
    }

    #[test]
    fn source_preview_must_match_durable_snapshot_before_creating_study_note() {
        use retivus::source_adapter::{SourceInfo, StudyTsvV1};
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("source.tsv");
        let data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\t橋\tnoun\ta\tbridge\n2\t橋\tverb\ta\tconnect\n";
        fs::write(&source_file, data).unwrap();
        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "independent-japanese".into(),
            edition: "2026-edition".into(),
            lexical_language: "ja".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic CC0 test".into(),
        })
        .unwrap();
        let mut preview = Vec::new();
        adapter
            .visit(
                &mut BufReader::new(fs::File::open(&source_file).unwrap()),
                &mut |row| {
                    preview.push(row);
                },
            )
            .unwrap();
        let mut panel = StudyPanel::default();
        panel.root = dir.path().to_string_lossy().into_owned();
        panel.profile_id = "source-learner".into();
        panel.connect(true);
        let deck = panel
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Japanese")
            .unwrap();
        panel.deck_id = Some(deck);
        let first = panel
            .add_validated_source_choice(&adapter, &source_file, &preview[0], "a")
            .unwrap();
        assert!(dir
            .path()
            .join("source-learner/source-catalog.sqlite3")
            .exists());
        let mut forged = preview[1].clone();
        forged.senses[0].definition = "forged meaning".into();
        assert!(panel
            .add_validated_source_choice(&adapter, &source_file, &forged, "a")
            .is_err());
        let second = panel
            .add_validated_source_choice(&adapter, &source_file, &preview[1], "a")
            .unwrap();
        assert_ne!(first, second);
        let first_tags = panel.backend.as_ref().unwrap().note_tags(first).unwrap();
        assert_eq!(
            StudyPanel::source_provenance(&first_tags).unwrap(),
            Some([
                "independent-japanese".into(),
                "2026-edition".into(),
                "1".into(),
                "a".into()
            ])
        );
        let duplicate = vec![first_tags[0].clone(), first_tags[0].clone()];
        assert!(StudyPanel::source_provenance(&duplicate).is_err());
        let malformed = vec![String::from("retivus::source::not-hex")];
        assert!(StudyPanel::source_provenance(&malformed).is_err());
        let second_tags = panel.backend.as_ref().unwrap().note_tags(second).unwrap();
        assert_ne!(
            first_tags, second_tags,
            "homographs retain distinct source identities"
        );
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(first).unwrap(),
            ["橋", "bridge"]
        );
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(second).unwrap(),
            ["橋", "connect"]
        );
        panel.close();
        panel.connect(false);
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(second).unwrap(),
            ["橋", "connect"]
        );
        panel.close();
    }

    #[test]
    fn filtering_out_selected_note_clears_destructive_confirmation() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "filtered".into();
        panel.connect(true);
        let note_id = panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(1, "keep me", "safe")
            .unwrap();
        panel.selected_note = Some(note_id);
        panel.front = "keep me".into();
        panel.back = "safe".into();
        panel.confirm_delete = true;
        panel.query = "front:unmatched".into();
        panel.refresh();
        assert!(panel.notes.is_empty());
        assert!(panel.selected_note.is_none());
        assert!(!panel.confirm_delete);
        panel.delete_selected_note();
        assert!(panel.status.contains("Confirm deletion"));
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["keep me", "safe"]
        );
        panel.close();
    }

    #[test]
    fn active_profile_identity_is_pinned_even_if_input_fields_change() {
        use retivus::source_adapter::{SourceInfo, StudyTsvV1};
        let original = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let source = original.path().join("safe.tsv");
        fs::write(
            &source,
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tmar\tnoun\ts1\tsea\n",
        )
        .unwrap();
        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "fixture-identity".into(),
            edition: "v1".into(),
            lexical_language: "es".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic".into(),
        })
        .unwrap();
        let mut rows = Vec::new();
        adapter
            .visit(
                &mut BufReader::new(fs::File::open(&source).unwrap()),
                &mut |record| rows.push(record),
            )
            .unwrap();
        let mut panel = StudyPanel::default();
        panel.root = original.path().to_string_lossy().into_owned();
        panel.profile_id = "original".into();
        panel.connect(true);
        let deck = panel
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Spanish")
            .unwrap();
        panel.deck_id = Some(deck);
        panel.root = other.path().to_string_lossy().into_owned();
        panel.profile_id = "unrelated".into();
        panel
            .add_validated_source_choice(&adapter, &source, &rows[0], "s1")
            .unwrap();
        assert!(original
            .path()
            .join("original/source-catalog.sqlite3")
            .exists());
        assert!(!other.path().join("unrelated").exists());
        assert_eq!(panel.stats.notes, 1);
        panel.close();
        assert!(panel.active_root.is_none());
        assert!(panel.active_profile_id.is_none());
    }

    #[test]
    fn native_mouse_selection_keyboard_note_edit_and_search_workflow() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let root = tempfile::tempdir().unwrap();
        let mut harness = Harness::new_eframe(|_| crate::Retivus::default());
        let note_id = {
            let app = harness.state_mut();
            app.study.root = root.path().to_string_lossy().into_owned();
            app.study.profile_id = "edit".into();
            app.study.connect(true);
            let backend = app.study.backend.as_mut().unwrap();
            let deck = backend.create_deck("Browse").unwrap();
            let note = backend.add_basic_note(deck, "uno", "one").unwrap();
            app.study.deck_id = Some(deck);
            app.study.refresh();
            app.study.refresh_review();
            note
        };
        harness.run();

        let label = format!("{note_id} · uno");
        harness.get_by_label(&label).scroll_to_me();
        harness.run();
        harness.get_by_label(&label).click();
        harness.run();
        assert_eq!(harness.state().study.selected_note, Some(note_id));

        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Front")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Front")
            .focus();
        harness.run();
        harness.key_press_modifiers(
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            egui::Key::A,
        );
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Front")
            .type_text("dos");
        harness.run();
        harness.get_by_label("Save edited note").scroll_to_me();
        harness.run();
        harness.get_by_label("Save edited note").click();
        harness.run();
        assert_eq!(
            harness
                .state()
                .study
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["dos", "one"]
        );

        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .focus();
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .type_text("front:dos");
        harness.run();
        harness.get_by_label("Search / refresh").scroll_to_me();
        harness.run();
        harness.get_by_label("Search / refresh").click();
        harness.run();
        assert_eq!(harness.state().study.notes.len(), 1);
        assert_eq!(harness.state().study.notes[0].id, note_id);

        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .focus();
        harness.run();
        harness.key_press_modifiers(
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            egui::Key::A,
        );
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search notes")
            .type_text("front:missing");
        harness.run();
        harness.get_by_label("Search / refresh").scroll_to_me();
        harness.run();
        harness.get_by_label("Search / refresh").click();
        harness.run();
        assert!(harness.state().study.notes.is_empty());
        assert!(harness.state().study.selected_note.is_none());
        drop(harness);
        let mut reopened = AnkiBackend::open_managed_profile(root.path(), "edit").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["dos", "one"]);
        reopened.close().unwrap();
    }

    #[test]
    fn accessible_native_ui_completes_source_to_study_review_with_pointer_events() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("words.tsv");
        fs::write(
            &source_path,
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tmar\tnoun\ta\tsea\n",
        )
        .unwrap();
        let mut harness = Harness::new_eframe(|_| crate::Retivus::default());
        {
            let app = harness.state_mut();
            app.source_format = crate::SourceFormat::GenericTsv;
            app.source_id = "ui-qa-fixture".into();
            app.source_edition = "v1".into();
            app.source_language = "es".into();
            app.definition_language = "en".into();
            app.license_notice = "Synthetic fixture".into();
            app.source_path = source_path.to_string_lossy().into_owned();
            app.study.root = root.path().to_string_lossy().into_owned();
            app.study.profile_id = "qa".into();
            app.study.connect(true);
            assert!(app.study.backend.is_some(), "{}", app.study.status);
            let deck = app
                .study
                .backend
                .as_mut()
                .unwrap()
                .create_deck("Words")
                .unwrap();
            app.study.deck_id = Some(deck);
            app.study.refresh_review();
        }
        harness.run();

        harness.get_by_label("Validate source").scroll_to_me();
        harness.run();
        harness.get_by_label("Validate source").click();
        harness.run();
        assert_eq!(harness.state().preview.len(), 1);

        harness
            .get_by_label("Stage complete source snapshot (no cards)")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_label("Stage complete source snapshot (no cards)")
            .click();
        harness.run();
        assert!(harness
            .state()
            .import_status
            .contains("Staged 1 source entries"));
        assert_eq!(harness.state().study.stats.notes, 0);

        harness.get_by_label("Select sense").scroll_to_me();
        harness.run();
        harness.get_by_label("Select sense").click();
        harness.run();
        harness
            .get_by_label("Add selected meaning to learner deck")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_label("Add selected meaning to learner deck")
            .click();
        harness.run();
        assert_eq!(harness.state().study.stats.notes, 1);
        let note_id = harness.state().study.notes[0].id;
        let card_id = harness.state().study.review.as_ref().unwrap().card_id;

        harness.get_by_label("Show answer").scroll_to_me();
        harness.run();
        harness.get_by_label("Show answer").click();
        harness.run();
        assert!(harness.state().study.revealed);
        harness.get_by_label("Easy").scroll_to_me();
        harness.run();
        harness.get_by_label("Easy").click();
        harness.run();
        assert!(harness.state().study.status.contains("Recorded rating 4"));
        assert_eq!(
            harness
                .state()
                .study
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            1
        );
        drop(harness);
        let reopened = AnkiBackend::open_managed_profile(root.path(), "qa").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["mar", "sea"]);
        assert_eq!(reopened.review_event_count(card_id).unwrap(), 1);
        reopened.close().unwrap();
    }

    #[test]
    fn populated_native_review_and_conflict_remain_accessible_across_zoom_and_themes() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let root = tempfile::tempdir().unwrap();
        let mut harness = Harness::new_eframe(|_| crate::Retivus::default());
        let (note_id, card_id) = {
            let app = harness.state_mut();
            app.study.root = root.path().to_string_lossy().into_owned();
            app.study.profile_id = "populated-accessibility".into();
            app.study.connect(true);
            assert!(app.study.backend.is_some(), "{}", app.study.status);
            let backend = app.study.backend.as_mut().unwrap();
            backend.enable_fsrs().unwrap();
            let deck = backend.create_deck("Accessible study").unwrap();
            let note = backend.add_basic_note(deck, "生", "life").unwrap();
            app.study.deck_id = Some(deck);
            app.study.refresh();
            app.study.refresh_review();
            let card = app.study.review.as_ref().unwrap().card_id;
            app.theme = crate::Theme::Light;
            app.high_contrast = true;
            app.reduced_motion = true;
            (note, card)
        };

        // A real Anki queue and note are present, not only empty form controls.
        harness.set_size(egui::vec2(1080.0, 800.0));
        harness.set_pixels_per_point(1.0);
        harness.run();
        harness.get_by_label("Active deck").scroll_to_me();
        harness.run();
        harness.get_by_label("Browse filter").scroll_to_me();
        harness.run();
        harness.get_by_label("Show answer").scroll_to_me();
        harness.run();
        harness.get_by_label("Show answer").click();
        harness.run();
        assert!(harness.state().study.revealed);
        harness.get_by_label("Good").scroll_to_me();
        harness.run();
        harness.get_by_label("Good").focus();
        harness.run();
        assert!(harness.get_by_label("Good").is_focused());
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert!(harness.state().study.status.contains("Recorded rating 3"));
        assert_eq!(
            harness
                .state()
                .study
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            1
        );

        // Surface a deliberately malformed source tag in the populated browse
        // UI and make its conflict announcement visible under narrow scaling.
        let conflict_label = {
            let app = harness.state_mut();
            app.theme = crate::Theme::Dark;
            app.density = crate::Density::Compact;
            app.study.notes[0].tags = vec!["retivus::source::not-hex".into()];
            app.study.selected_note = Some(note_id);
            app.study.front = "生".into();
            app.study.back = "life".into();
            app.study.browse_scope = BrowseScope::Conflicts;
            let error = StudyPanel::source_provenance(&app.study.notes[0].tags).unwrap_err();
            format!("Source identity conflict: {error}")
        };
        harness.set_size(egui::vec2(720.0, 650.0));
        harness.set_pixels_per_point(1.5);
        harness.run();
        assert_eq!(
            harness
                .state()
                .study
                .notes
                .iter()
                .filter(|note| BrowseScope::Conflicts.matches(note))
                .count(),
            1
        );
        harness.get_by_label("Browse filter").scroll_to_me();
        harness.run();
        harness.get_by_label(&conflict_label).scroll_to_me();
        harness.run();
        harness.get_by_label("Front").scroll_to_me();
        harness.run();
        harness.get_by_label("Front").focus();
        harness.run();
        assert!(harness.get_by_label("Front").is_focused());
        drop(harness);

        // Keyboard navigation and conflict rendering never mutate the actual
        // note; the one intentional review grade remains durable on disk.
        let reopened =
            AnkiBackend::open_managed_profile(root.path(), "populated-accessibility").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["生", "life"]);
        assert_eq!(reopened.review_event_count(card_id).unwrap(), 1);
        reopened.close().unwrap();
    }

    #[test]
    fn populated_native_study_panel_renders_in_real_egui_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "ui-frame".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let deck = panel
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Visible")
            .unwrap();
        panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(deck, "hola", "hello")
            .unwrap();
        panel.deck_id = Some(deck);
        panel.refresh();
        panel.refresh_review();
        assert_eq!(panel.due.0, 1);

        let ctx = egui::Context::default();
        let frame = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| panel.ui(ui));
            });
        });
        assert!(!frame.shapes.is_empty());
        assert_eq!(panel.stats.notes, 1);
        assert_eq!(panel.stats.cards, 1);
        panel.close();
    }

    #[test]
    fn native_note_deletion_requires_confirmation_and_allows_undo() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "deletion".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let note_id = panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(1, "árbol", "tree")
            .unwrap();
        panel.selected_note = Some(note_id);
        panel.refresh();
        panel.delete_selected_note();
        assert!(panel.status.contains("Confirm deletion"));
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["árbol", "tree"]
        );
        panel.confirm_delete = true;
        panel.delete_selected_note();
        assert!(panel.status.contains("Deleted note"), "{}", panel.status);
        assert!(!panel.confirm_delete);
        assert!(panel.selected_note.is_none());
        assert!(panel
            .backend
            .as_ref()
            .unwrap()
            .note_fields(note_id)
            .is_err());
        panel.undo_last_edit();
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["árbol", "tree"]
        );
        panel.close();
        panel.connect(false);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["árbol", "tree"]
        );
        panel.close();
    }

    #[test]
    fn validated_file_selection_creates_exact_study_card_and_survives_restart() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("japanese.tsv");
        fs::write(
            &source_path,
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\t生\tnoun\ts1\tlife\n2\t生\tverb\ts1\tlive\n",
        )
        .unwrap();
        let mut app = crate::Retivus::default();
        app.source_format = crate::SourceFormat::GenericTsv;
        app.source_id = "study-integration-fixture".into();
        app.source_edition = "v1".into();
        app.source_language = "ja".into();
        app.definition_language = "en".into();
        app.license_notice = "Synthetic fixture".into();
        app.source_path = source_path.to_string_lossy().into_owned();
        app.study.root = root.path().to_string_lossy().into_owned();
        app.study.profile_id = "learner".into();
        app.study.connect(true);
        assert!(app.study.backend.is_some(), "{}", app.study.status);
        let deck = app
            .study
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Vocabulary")
            .unwrap();
        app.study.deck_id = Some(deck);
        app.study.refresh_review();

        app.validate_source_file();
        assert!(app.import_status.contains("Valid records: 2"));
        assert_eq!(app.preview.len(), 2);
        assert_eq!(app.preview[0].word, app.preview[1].word);
        app.stage_validated_source_to_study();
        assert!(
            app.import_status.contains("Staged 2 source entries"),
            "{}",
            app.import_status
        );
        assert_eq!(app.study.stats.notes, 0);
        app.stage_validated_source_to_study();
        assert!(app.import_status.contains("Source staging refused"));
        assert_eq!(app.study.stats.notes, 0);
        app.selected_sense = crate::SelectedSense::from_record(&app.preview[1], "s1");
        app.add_selected_sense_to_study();
        assert!(
            app.import_status.contains("Created study note"),
            "{}",
            app.import_status
        );
        assert_eq!(app.study.notes.len(), 1);
        assert_eq!(app.study.notes[0].fields, ["生", "live"]);
        let note_id = app.study.notes[0].id;
        let provenance = StudyPanel::source_provenance(&app.study.notes[0].tags).unwrap();
        assert_eq!(
            provenance,
            Some([
                "study-integration-fixture".into(),
                "v1".into(),
                "2".into(),
                "s1".into()
            ])
        );
        assert_eq!(app.study.due.0, 1);
        let review_id = app.study.review.as_ref().unwrap().card_id;
        assert!(app
            .study
            .review
            .as_ref()
            .unwrap()
            .question_html
            .contains("生"));
        app.study.revealed = true;
        app.study.grade_revealed(4);
        assert!(
            app.study.status.contains("Recorded rating 4"),
            "{}",
            app.study.status
        );
        app.study.close();
        app.study.connect(false);
        assert_eq!(
            app.study
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["生", "live"]
        );
        assert_eq!(
            app.study
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(review_id)
                .unwrap(),
            1
        );

        // Failed replacement validation clears the previous preview and choice.
        fs::write(&source_path, "corrupted").unwrap();
        app.validate_source_file();
        assert!(app.import_status.contains("Validation failed"));
        assert!(app.preview.is_empty());
        assert!(app.selected_sense.is_none());
        app.study.close();
    }

    #[test]
    fn source_inspection_detects_manual_changes_and_explicit_version_mapping() {
        use retivus::source_adapter::{SourceInfo, StudyTsvV1};
        use std::io::Cursor;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source.tsv");
        let original =
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tmar\tnoun\ta\tsea\n";
        fs::write(&path, original).unwrap();
        let v1 = StudyTsvV1::new(SourceInfo {
            provider_id: "version-fixture".into(),
            edition: "v1".into(),
            lexical_language: "es".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic fixture".into(),
        })
        .unwrap();
        let mut rows = Vec::new();
        v1.visit(&mut BufReader::new(Cursor::new(original)), &mut |row| {
            rows.push(row)
        })
        .unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "versioned".into();
        panel.connect(true);
        let deck = panel
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Spanish")
            .unwrap();
        panel.deck_id = Some(deck);
        let id = panel
            .add_validated_source_choice(&v1, &path, &rows[0], "a")
            .unwrap();
        panel.selected_note = Some(id);
        panel.inspect_selected_source();
        assert!(panel.source_report.starts_with("Source verified:"));
        panel
            .backend
            .as_mut()
            .unwrap()
            .edit_note_fields(id, &["mar", "ocean"])
            .unwrap();
        panel.inspect_selected_source();
        assert!(panel.source_report.contains("edited independently"));
        panel.backend.as_mut().unwrap().undo_last().unwrap();
        panel.inspect_selected_source();
        assert!(panel.source_report.starts_with("Source verified:"));

        let v2 = StudyTsvV1::new(SourceInfo {
            provider_id: "version-fixture".into(),
            edition: "v2".into(),
            lexical_language: "es".into(),
            definition_language: "en".into(),
            license_notice: "Synthetic fixture".into(),
        })
        .unwrap();
        let catalog_file = panel.managed_catalog_path().unwrap();
        let mut catalog = SourceCatalog::open_existing(&catalog_file).unwrap();
        catalog
            .import_snapshot(
                "v2",
                &v2,
                &mut BufReader::new(Cursor::new(
                    "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tmar\tnoun\ta\tocean\n",
                )),
            )
            .unwrap();
        drop(catalog);
        panel.diff_provider = "version-fixture".into();
        panel.diff_before = "v1".into();
        panel.diff_after = "v2".into();
        panel.load_source_differences();
        assert_eq!(panel.diff_rows.len(), 1);
        assert_eq!(panel.diff_rows[0].entry_id, "1");
        assert_eq!(
            panel.diff_rows[0].before.as_ref().unwrap().definition,
            "sea"
        );
        assert_eq!(
            panel.diff_rows[0].after.as_ref().unwrap().definition,
            "ocean"
        );
        let review_id = panel.review.as_ref().unwrap().card_id;
        panel.revealed = true;
        panel.grade_revealed(4);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(review_id)
                .unwrap(),
            1
        );
        panel.mapping_to_snapshot = "v2".into();
        panel.mapping_to_entry = "1".into();
        panel.mapping_to_sense = "a".into();
        panel.preview_source_mapping();
        assert!(panel.mapping_preview.is_some(), "{}", panel.status);
        panel.record_source_mapping();
        assert!(panel.status.contains("not recorded"));
        panel.mapping_approved = true;
        panel.record_source_mapping();
        assert!(panel.status.contains("Recorded explicit mapping"));
        panel.inspect_selected_source();
        assert!(panel.source_report.contains("Update available"));
        panel.undo_source_mapping();
        assert!(panel.status.contains("mapping removed"));
        assert!(panel.source_report.starts_with("Source verified:"));
        panel.preview_source_mapping();
        panel.mapping_approved = true;
        panel.record_source_mapping();
        assert!(panel.source_report.contains("Update available"));
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(id).unwrap(),
            ["mar", "sea"]
        );
        panel.apply_recorded_source_mapping();
        assert!(panel.status.contains("Source revision applied"));
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(id).unwrap(),
            ["mar", "ocean"]
        );
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(review_id)
                .unwrap(),
            1
        );
        panel.backend.as_mut().unwrap().undo_last().unwrap();
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(id).unwrap(),
            ["mar", "sea"]
        );
        panel.close();
    }

    #[test]
    fn quitting_with_open_managed_profile_releases_collection_for_restart() {
        let root = tempfile::tempdir().unwrap();
        let note_id;
        {
            let mut ui = StudyPanel::default();
            ui.root = root.path().to_string_lossy().into_owned();
            ui.profile_id = "shutdown".into();
            ui.connect(true);
            assert!(ui.backend.is_some(), "{}", ui.status);
            note_id = ui
                .backend
                .as_mut()
                .unwrap()
                .add_basic_note(1, "vida", "life")
                .unwrap();
            // No explicit close: dropping the window must shut down Anki cleanly.
        }
        let reopened = AnkiBackend::open_managed_profile(root.path(), "shutdown").unwrap();
        assert_eq!(reopened.note_fields(note_id).unwrap(), ["vida", "life"]);
        reopened.close().unwrap();
    }

    #[test]
    fn raw_mexilex_source_cannot_create_cards_without_verified_catalog() {
        use retivus::source_adapter::{MexilexV1, SourceMeaning, SourceRecord};
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "protected".into();
        panel.connect(true);
        let deck = panel
            .backend
            .as_mut()
            .unwrap()
            .create_deck("Vocabulary")
            .unwrap();
        panel.deck_id = Some(deck);
        let adapter = MexilexV1::default();
        let row = SourceRecord {
            provenance: adapter.descriptor().clone(),
            entry_id: "unverified-entry".into(),
            word: "banqueta".into(),
            part_of_speech: "noun".into(),
            senses: vec![SourceMeaning {
                sense_id: "sense".into(),
                definition: "sidewalk".into(),
            }],
        };
        assert!(panel
            .add_validated_source_choice(&adapter, &root.path().join("raw.jsonl"), &row, "sense")
            .is_err());
        assert!(!root
            .path()
            .join("protected/source-catalog.sqlite3")
            .exists());
        assert!(panel
            .backend
            .as_mut()
            .unwrap()
            .browse_notes("", 10)
            .unwrap()
            .is_empty());
        panel.close();
    }

    #[test]
    fn native_undo_action_reverses_grading_and_persists_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "review-undo".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let backend = panel.backend.as_mut().unwrap();
        backend.enable_fsrs().unwrap();
        let deck = backend.create_deck("Undo").unwrap();
        backend.add_basic_note(deck, "today", "hoy").unwrap();
        panel.deck_id = Some(deck);
        panel.refresh_review();
        let card_id = panel.review.as_ref().unwrap().card_id;
        panel.revealed = true;
        panel.grade_revealed(3);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            1
        );

        panel.undo_last_edit();
        assert_eq!(panel.status, "Last edit undone.");
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            0
        );
        panel.close();
        panel.connect(false);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            0
        );
        panel.close();
    }

    #[test]
    fn stale_displayed_review_never_grades_a_different_queued_card() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "guarded-review".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let backend = panel.backend.as_mut().unwrap();
        backend.enable_fsrs().unwrap();
        let deck = backend.create_deck("Guarded").unwrap();
        backend.add_basic_note(deck, "first", "one").unwrap();
        backend.add_basic_note(deck, "second", "two").unwrap();
        panel.deck_id = Some(deck);
        panel.refresh_review();
        let displayed_id = panel.review.as_ref().unwrap().card_id;
        assert_eq!(
            panel.backend.as_mut().unwrap().grade_next_card(4).unwrap(),
            Some(displayed_id)
        );

        panel.revealed = true;
        panel.grade_revealed(4);
        assert!(
            panel.status.contains("Review not recorded"),
            "{}",
            panel.status
        );
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(displayed_id)
                .unwrap(),
            1
        );
        if let Some(next) = panel.review.as_ref() {
            assert_ne!(next.card_id, displayed_id);
            assert_eq!(
                panel
                    .backend
                    .as_ref()
                    .unwrap()
                    .review_event_count(next.card_id)
                    .unwrap(),
                0
            );
        }
        panel.close();
    }

    #[test]
    fn closing_profile_clears_editor_state_without_erasing_saved_notes() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "privacy".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let note_id = panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(1, "sensitive front", "sensitive back")
            .unwrap();
        panel.selected_note = Some(note_id);
        panel.front = "sensitive front".into();
        panel.back = "sensitive back".into();
        panel.query = "sensitive".into();
        panel.package = "private.apkg".into();
        panel.snapshot_id = "private".into();
        panel.new_deck = "private deck".into();
        panel.close();
        assert!(panel.backend.is_none());
        assert!(panel.selected_note.is_none());
        assert!(panel.front.is_empty());
        assert!(panel.back.is_empty());
        assert!(panel.query.is_empty());
        assert!(panel.package.is_empty());
        assert!(panel.snapshot_id.is_empty());
        assert!(panel.new_deck.is_empty());

        panel.connect(false);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["sensitive front", "sensitive back"]
        );
        panel.close();
    }

    #[test]
    fn learner_workspace_never_opens_user_profiles_on_startup() {
        let state = StudyPanel::default();
        assert!(state.backend.is_none());
        assert!(state.root.is_empty());
        assert!(state.profile_id.is_empty());
        assert!(state.review.is_none());
        assert!(!state.revealed);
    }
}
