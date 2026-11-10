//! Explicit native learner-workspace operations behind the Anki feature gate.
//! No profile opens, is created, or is migrated until a user chooses an action.
use eframe::egui;
use retivus::anki_backend::{AnkiBackend, CollectionStats, NoteRow, ReviewPreview};
use retivus::source_adapter::{SourceAdapter, SourceRecord};
use retivus::source_store::{SnapshotSenseChange, SourceCardCandidate, SourceCatalog};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};

// The SHA-256 was obtained from the upstream lex-v0.3.0 release only after
// CI verified the published package's official SHA256SUMS and its manifest.
// Hashes apply to uncompressed study.jsonl content, not a tarball.
const MEXILEX_SNAPSHOT: &str = "lex-v0.3.0";
const MEXILEX_STUDY_SHA256: &str =
    "c4e3f721a9f4004645ff7e1adfa5c628bfee870d0f46975732baafed03f3ea86";

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

/// A resumable, opt-in first-definition run. A cancellation stops after the
/// current bounded batch; already committed Anki notes are never deleted.
struct FirstDefinitionJob {
    deck_id: i64,
    total: usize,
    processed: usize,
    created: usize,
    skipped: usize,
    last_entry_id: String,
    source_tags: HashSet<String>,
}

#[derive(Default)]
pub struct StudyPanel {
    root: String,
    profile_id: String,
    // UI preference only; cards, credentials and review data are never saved here.
    last_root: String,
    last_profile_id: String,
    active_root: Option<PathBuf>,
    active_profile_id: Option<String>,
    package: String,
    import_with_scheduling: bool,
    export_with_scheduling: bool,
    snapshot_id: String,
    // Private recovery ZIPs contain study history and media. These paths and
    // one-shot consent live only in the non-persisted native study panel.
    recovery_zip: String,
    restore_profile_id: String,
    confirm_recovery: bool,
    backend: Option<AnkiBackend>,
    decks: Vec<(i64, String)>,
    deck_id: Option<i64>,
    new_deck: String,
    daily_new_limit: u32,
    daily_review_limit: u32,
    generation_plan: Option<usize>,
    generation_confirmed: bool,
    generation: Option<FirstDefinitionJob>,
    generation_status: String,
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
    focused_study: bool,
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
/// Anki renders the answer with the question followed by its standard HTML
/// separator. In the text-only native fallback, show just the back side.
/// Other markup remains inert rather than being executed as active HTML.
fn inert_review_answer(html: &str) -> &str {
    html.split_once("<hr id=answer>")
        .map_or(html, |(_, answer)| answer.trim())
}

// Explicit card foreground avoids dark-mode contrast regressions when the
// system preference changes or an egui RichText style overrides the default.
fn review_card_text_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::WHITE
    } else {
        egui::Color32::BLACK
    }
}

// Secondary text must remain legible across dark/light appearance, too.
fn focused_review_secondary_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_gray(190)
    } else {
        egui::Color32::from_gray(75)
    }
}

// Compute the four-button row's left inset, using the available width rather
// than relying on egui::Ui::vertical_centered to center its nested horizontal row.
fn rating_row_geometry(available_width: f32, spacing: f32) -> (f32, f32) {
    let button_width = ((available_width - 3.0 * spacing) / 4.0)
        .min(105.0)
        .max(64.0);
    let row_width = 4.0 * button_width + 3.0 * spacing;
    (button_width, ((available_width - row_width) * 0.5).max(0.0))
}

fn live_status(ui: &mut egui::Ui, text: &str) {
    let response = ui.label(text);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Status);
        node.set_label(text.to_owned());
        node.set_live(egui::accesskit::Live::Polite);
        node.set_live_atomic();
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

    fn valid_profile_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }

    fn existing_profile_ids(root: &Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(root) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for entry in entries.flatten() {
            let Ok(id) = entry.file_name().into_string() else {
                continue;
            };
            if !Self::valid_profile_id(&id) || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if fs::symlink_metadata(entry.path().join("collection.anki2"))
                .is_ok_and(|meta| meta.file_type().is_file())
            {
                result.push(id);
            }
        }
        result.sort_unstable();
        result
    }

    pub fn restore_profile_selection(&mut self, root: Option<String>, id: Option<String>) {
        if let (Some(root), Some(id)) = (root, id) {
            if Path::new(&root).is_absolute() && Self::valid_profile_id(&id) {
                self.root = root.clone();
                self.profile_id = id.clone();
                self.last_root = root;
                self.last_profile_id = id;
                return;
            }
        }
        // Recognize the pre-persistence folder of existing installations.
        if let Some(home) = std::env::var_os("HOME") {
            let previous = PathBuf::from(home).join("Retivus-profiles");
            if previous.is_dir() {
                self.root = previous.to_string_lossy().into_owned();
            }
        }
    }

    pub fn last_successful_profile(&self) -> Option<(&str, &str)> {
        if self.last_root.is_empty() || self.last_profile_id.is_empty() {
            None
        } else {
            Some((&self.last_root, &self.last_profile_id))
        }
    }

    fn remember_active_profile(&mut self) {
        if let (Some(root), Some(id)) = (&self.active_root, &self.active_profile_id) {
            self.last_root = root.to_string_lossy().into_owned();
            self.last_profile_id = id.clone();
        }
    }

    /// The status of a prior failed open/create attempt is no longer relevant
    /// after changing either part of the requested profile location.
    fn profile_location_changed(&mut self) {
        self.status.clear();
        self.confirm_recovery = false;
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
                self.status = format!("Profile root '{}' could not be opened: {error}", self.root);
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
                self.remember_active_profile();
                self.status = format!("Opened Retivus-managed profile {}.", self.profile_id);
                self.refresh();
                self.refresh_review();
            }
            Err(error) => {
                let action = if create { "create" } else { "open" };
                self.status = format!(
                    "Could not {action} profile '{}' under '{}': {error}",
                    self.profile_id, self.root
                );
            }
        }
    }

    /// Restore only a private pre-sync ZIP into a *new* owned profile.
    /// The backend audits the archive and refuses any existing destination.
    /// No network access, profile replacement, or automated merge occurs.
    fn restore_private_recovery(&mut self) {
        if !self.confirm_recovery {
            self.status = "Confirm the private recovery and new profile ID first.".into();
            return;
        }
        // A previous click must never authorize a different archive or ID.
        self.confirm_recovery = false;
        if self.backend.is_some() {
            self.status = "Close the active profile before restoring another.".into();
            return;
        }
        let result = (|| -> Result<(PathBuf, AnkiBackend), String> {
            let root = fs::canonicalize(&self.root)
                .map_err(|error| format!("Recovery profile root unavailable: {error}"))?;
            let backend = AnkiBackend::restore_pre_sync_recovery_as_new_profile(
                &root,
                &self.restore_profile_id,
                Path::new(&self.recovery_zip),
            )
            .map_err(|error| error.to_string())?;
            Ok((root, backend))
        })();
        match result {
            Ok((root, backend)) => {
                self.active_root = Some(root);
                self.profile_id = self.restore_profile_id.clone();
                self.active_profile_id = Some(self.profile_id.clone());
                self.backend = Some(backend);
                self.remember_active_profile();
                self.status = format!(
                    "Private recovery opened as new Retivus profile {}. Original profiles were not replaced.",
                    self.profile_id
                );
                self.recovery_zip.clear();
                self.restore_profile_id.clear();
                self.refresh();
                self.refresh_review();
            }
            Err(error) => self.status = format!("Private recovery refused: {error}"),
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
                self.load_daily_limits();
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

    fn load_daily_limits(&mut self) {
        if let (Some(backend), Some(id)) = (&mut self.backend, self.deck_id) {
            if let Ok((new, review)) = backend.deck_daily_limits(id) {
                self.daily_new_limit = new;
                self.daily_review_limit = review;
            }
        }
    }

    fn save_daily_limits(&mut self) {
        if let (Some(backend), Some(id)) = (&mut self.backend, self.deck_id) {
            match backend.set_deck_daily_limits(
                id, self.daily_new_limit, self.daily_review_limit
            ) {
                Ok(()) => {
                    self.status = "Anki deck-specific daily limits saved.".into();
                    self.refresh_review();
                }
                Err(error) => self.status = format!("Could not save daily limits: {error}"),
            }
        }
    }

    /// Previewing never writes notes, changes limits or creates a deck.
    /// Only the exact independently authenticated catalog may produce a plan.
    fn preview_first_definition_generation(&mut self) {
        self.generation_plan = None;
        self.generation_confirmed = false;
        let result = (|| -> Result<usize, String> {
            let path = self.managed_catalog_path()?;
            let catalog = SourceCatalog::open_existing(&path)?;
            if catalog.authenticated_export_digest("mexilex", MEXILEX_SNAPSHOT)?
                .as_deref() != Some(MEXILEX_STUDY_SHA256) {
                return Err("import the verified Mexilex release into this profile first".into());
            }
            let (entries, senses) = catalog.counts("mexilex", MEXILEX_SNAPSHOT)?;
            if entries != 124444 || senses != 155586 {
                return Err("Mexilex snapshot counts do not match the verified release".into());
            }
            let first = catalog.first_meaning_page("mexilex", MEXILEX_SNAPSHOT, "", 1)?;
            if first.is_empty() {
                return Err("verified source catalog contains no first meanings".into());
            }
            Ok(entries as usize)
        })();
        match result {
            Ok(total) => {
                self.generation_plan = Some(total);
                self.generation_status = format!(
                    "Plan: {total} lexical entries, one first English definition per entry.                      Existing source-tagged notes will be skipped; no cards created yet."
                );
            }
            Err(error) => self.generation_status = format!("Plan unavailable: {error}"),
        }
    }

    fn start_first_definition_generation(&mut self) {
        if !self.generation_confirmed || self.generation_plan.is_none()
            || self.generation.is_some() {
            self.generation_status = "Preview and confirm the plan before starting.".into();
            return;
        }
        // Recheck the authenticated source immediately before mutations.
        self.preview_first_definition_generation();
        let Some(total) = self.generation_plan else { return; };
        let outcome = (|| -> Result<(i64, HashSet<String>), String> {
            let backend = self.backend.as_mut().ok_or("open a managed profile first")?;
            let source_tags = backend.source_identity_tags().map_err(|e| e.to_string())?;
            let deck = backend.create_deck("Tarium::es-MX::mexilex")
                .map_err(|e| e.to_string())?;
            backend.set_deck_daily_limits(deck, self.daily_new_limit, self.daily_review_limit)
                .map_err(|e| e.to_string())?;
            Ok((deck, source_tags))
        })();
        match outcome {
            Ok((deck_id, source_tags)) => {
                self.generation = Some(FirstDefinitionJob {
                    deck_id,
                    total,
                    processed: 0,
                    created: 0,
                    skipped: 0,
                    last_entry_id: String::new(),
                    source_tags,
                });
                self.generation_status =
                    "Generation started. You may cancel or close the app; rerunning skips existing cards.".into();
            }
            Err(error) => self.generation_status = format!("Generation refused: {error}"),
        }
    }

    /// One bounded batch per GUI repaint. UI remains responsive and committed
    /// cards survive cancellation, restart or a later safe resume.
    fn advance_first_definition_generation(&mut self) {
        let Some(mut job) = self.generation.take() else { return; };
        let result = (|| -> Result<bool, String> {
            let catalog = SourceCatalog::open_existing(&self.managed_catalog_path()?)?;
            if catalog.authenticated_export_digest("mexilex", MEXILEX_SNAPSHOT)?
                .as_deref() != Some(MEXILEX_STUDY_SHA256) {
                return Err("verified source attestation changed during generation".into());
            }
            let batch = catalog.first_meaning_page(
                "mexilex", MEXILEX_SNAPSHOT, &job.last_entry_id, 64
            )?;
            let backend = self.backend.as_mut().ok_or("learner profile closed")?;
            for candidate in &batch {
                let tag = candidate.source_tag();
                if job.source_tags.contains(&tag) {
                    job.skipped += 1;
                } else {
                    backend.add_prechecked_source_candidate(job.deck_id, candidate)
                        .map_err(|e| format!("entry {}: {e}", candidate.entry_id))?;
                    job.source_tags.insert(tag);
                    job.created += 1;
                }
                job.processed += 1;
                job.last_entry_id.clone_from(&candidate.entry_id);
            }
            Ok(batch.len() < 64)
        })();
        match result {
            Ok(true) => {
                self.deck_id = Some(job.deck_id);
                self.generation_status = format!(
                    "Completed: {} entries, {} new cards, {} already present.",
                    job.processed, job.created, job.skipped
                );
                self.generation_plan = None;
                self.generation_confirmed = false;
                self.refresh();
                self.refresh_review();
            }
            Ok(false) => {
                self.generation_status = format!(
                    "{} / {} entries checked · {} new cards · {} existing skipped",
                    job.processed, job.total, job.created, job.skipped
                );
                self.generation = Some(job);
            }
            Err(error) => {
                self.generation_status = format!(
                    "Stopped after {} entries ({} added, {} skipped): {error}.                      Re-preview to resume safely without duplicates.",
                    job.processed, job.created, job.skipped
                );
                self.generation_plan = None;
                self.generation_confirmed = false;
                self.refresh();
                self.refresh_review();
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

    /// Copy and authenticate the exact published Mexilex export into a private
    /// temporary file BEFORE creating/mutating the source catalog. The pinned
    /// snapshot is then imported in one SQLite transaction with its attestation.
    /// No flashcards, decks or review history are created by this operation.
    pub fn stage_verified_mexilex_snapshot(&mut self, path: &Path) -> Result<usize, String> {
        let catalog_path = self.managed_catalog_path()?;
        let file_info = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !file_info.file_type().is_file() || file_info.file_type().is_symlink() {
            return Err("Mexilex export must be a regular file, not a symlink".into());
        }
        let mut source = fs::File::open(path).map_err(|e| e.to_string())?;
        // Use the immutable copied bytes for both hashing and parsing, not
        // a second read of a potentially changing path.
        let mut copy = tempfile::tempfile().map_err(|e| e.to_string())?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            copy.write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
        }
        let actual = format!("{:x}", digest.finalize());
        if actual != MEXILEX_STUDY_SHA256 {
            return Err(format!(
                "Mexilex export checksum differs from published {MEXILEX_SNAPSHOT}; use the verified release study.jsonl, not an unverified or newer export"
            ));
        }
        copy.rewind().map_err(|e| e.to_string())?;
        let mut catalog = match fs::symlink_metadata(&catalog_path) {
            Ok(_) => SourceCatalog::open_existing(&catalog_path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SourceCatalog::create_new(&catalog_path)?
            }
            Err(error) => return Err(error.to_string()),
        };
        let previous = catalog.authenticated_export_digest("mexilex", MEXILEX_SNAPSHOT)?;
        if previous.as_deref() == Some(MEXILEX_STUDY_SHA256) {
            let (entries, senses) = catalog.counts("mexilex", MEXILEX_SNAPSHOT)?;
            if entries != 124444 || senses != 155586 {
                return Err("verified Mexilex catalog has unexpected entry/sense counts".into());
            }
            self.status = format!(
                "Mexilex {MEXILEX_SNAPSHOT} already verified: {entries} words, {senses} senses; no cards created."
            );
            return Ok(entries as usize);
        }
        if previous.is_some() || catalog.counts("mexilex", MEXILEX_SNAPSHOT)?.0 != 0 {
            return Err("a Mexilex snapshot with this ID exists without the expected verification; refusing to overwrite or trust it".into());
        }
        let count = catalog.import_authenticated_snapshot(
            MEXILEX_SNAPSHOT,
            &retivus::source_adapter::MexilexV1::default(),
            &mut BufReader::new(copy),
            MEXILEX_STUDY_SHA256,
        )?;
        let (entries, senses) = catalog.counts("mexilex", MEXILEX_SNAPSHOT)?;
        if count != 124444 || entries != 124444 || senses != 155586 {
            return Err("verified source catalog imported unexpected record totals".into());
        }
        self.status = format!(
            "Imported verified Mexilex {MEXILEX_SNAPSHOT}: {entries} entries and {senses} senses. No cards or reviews created."
        );
        Ok(count)
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

    /// Use one deterministic Anki deck per lexical language and source.
    /// Delimiters and path-like names are refused, not silently normalized
    /// into a different or colliding provider identity.
    fn provider_deck_name(info: &retivus::source_adapter::SourceInfo) -> Result<String, String> {
        info.validate()?;
        for (label, value) in [
            ("provider", info.provider_id.as_str()),
            ("language", info.lexical_language.as_str()),
        ] {
            if value.len() > 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            {
                return Err(format!("unsafe {label} identifier for Tarium deck name"));
            }
        }
        Ok(format!(
            "Tarium::{}::{}",
            info.lexical_language, info.provider_id
        ))
    }

    /// The learner GUI creates/reuses only the chosen provider's deck, AFTER
    /// checking the exact immutable source sense. It never bulk-creates notes.
    pub fn add_validated_source_to_provider_deck(
        &mut self,
        adapter: &dyn SourceAdapter,
        source_path: &Path,
        record: &SourceRecord,
        sense_id: &str,
    ) -> Result<i64, String> {
        self.add_validated_source_choice_impl(adapter, source_path, record, sense_id, true)
    }

    /// The explicit-deck bridge remains available to integration callers;
    /// provider-specific naming is applied by the native learner GUI.
    pub fn add_validated_source_choice(
        &mut self,
        adapter: &dyn SourceAdapter,
        source_path: &Path,
        record: &SourceRecord,
        sense_id: &str,
    ) -> Result<i64, String> {
        self.add_validated_source_choice_impl(adapter, source_path, record, sense_id, false)
    }

    /// Import a validated source into a durable catalog before an individual
    /// selected sense may become an Anki note. Never guess a homograph.
    fn add_validated_source_choice_impl(
        &mut self,
        adapter: &dyn SourceAdapter,
        source_path: &Path,
        record: &SourceRecord,
        sense_id: &str,
        provider_deck: bool,
    ) -> Result<i64, String> {
        let explicit_deck_id = if provider_deck {
            None
        } else {
            Some(self.deck_id.ok_or("choose a learner deck first")?)
        };
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
        let is_mexilex = adapter.format_id() == "mexilex-study-jsonl-v1";
        let snapshot = if is_mexilex {
            MEXILEX_SNAPSHOT.to_owned()
        } else if self.snapshot_id.trim().is_empty() {
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
        if is_mexilex
            && catalog
                .authenticated_export_digest(&origin.provider_id, &snapshot)?
                .as_deref()
                != Some(MEXILEX_STUDY_SHA256)
        {
            return Err(
                "Mexilex requires an independently verified release snapshot in this learner profile's catalog".into(),
            );
        }
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
        // Check the *entire persisted collection*, not just the currently
        // displayed notes, before creating even an empty provider deck.
        if let Some(id) = self
            .backend
            .as_ref()
            .ok_or("learner profile closed during import")?
            .existing_source_note_id(&actual)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "this exact source meaning is already note {id}; no duplicate card or deck created"
            ));
        }
        let (deck_id, deck_name) = if provider_deck {
            let name = Self::provider_deck_name(&actual.provenance)?;
            let deck_id = self
                .backend
                .as_mut()
                .ok_or("learner profile closed during import")?
                .create_deck(&name)
                .map_err(|e| e.to_string())?;
            (deck_id, Some(name))
        } else {
            (explicit_deck_id.ok_or("choose a learner deck first")?, None)
        };
        let note_id = self
            .backend
            .as_mut()
            .ok_or("learner profile closed during import")?
            .add_source_candidate(deck_id, &actual)
            .map_err(|e| e.to_string())?;
        if provider_deck {
            self.deck_id = Some(deck_id);
        }
        self.status = if let Some(deck) = deck_name {
            format!("Created note {note_id} in {deck}; exact source sense and provenance saved.")
        } else {
            format!("Created note {note_id} from an exact source sense; provenance saved.")
        };
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

    /// Show only the local fail-closed result, never start a network sync.
    fn check_local_sync_readiness(&mut self) {
        self.status = match self.backend.as_ref() {
            Some(backend) => match backend
                .inspect_local_collection_sync_readiness()
                .and_then(|()| backend.inspect_local_media_conflict_risk())
            {
                Ok(()) => "Offline check: no local sync blockers detected in collection or media files at inspection time. Remote conflicts, media edits after inspection, deliberate checksum collisions, and AnkiWeb behavior remain unverified; no sync was performed.".into(),
                Err(error) => format!("Offline check refused: {error}. No network contacted."),
            },
            None => "Open a Retivus-managed learner profile before checking sync blockers.".into(),
        };
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
        self.recovery_zip.clear();
        self.restore_profile_id.clear();
        self.confirm_recovery = false;
        self.new_deck.clear();
        self.review = None;
        self.revealed = false;
        self.focused_study = false;
        self.due = (0, 0, 0);
    }

    pub fn is_focused_study(&self) -> bool {
        self.focused_study && self.backend.is_some()
    }

    fn start_focused_study(&mut self) {
        if self.backend.is_some() {
            self.refresh_review();
            self.focused_study = true;
        }
    }

    /// A separate study workspace, not a small widget among editing controls.
    /// The existing Anki backend still owns due queues, grading and persistence.
    pub fn focused_ui(&mut self, ui: &mut egui::Ui) {
        if !self.is_focused_study() {
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Back to decks").clicked() {
                self.focused_study = false;
            }
            ui.separator();
            let dark_mode = ui.visuals().dark_mode;
            ui.label(
                egui::RichText::new("Retivus · Study")
                    .strong()
                    .color(review_card_text_color(dark_mode)),
            );
            let deck_name = self
                .decks
                .iter()
                .find(|(id, _)| Some(*id) == self.deck_id)
                .map_or("Selected deck", |(_, name)| name.as_str());
            ui.colored_label(focused_review_secondary_color(dark_mode), deck_name);
        });
        if !self.focused_study {
            return;
        }
        ui.separator();
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.heading(
                egui::RichText::new("Review").color(review_card_text_color(ui.visuals().dark_mode)),
            );
            let due = format!(
                "{} new  ·  {} learning  ·  {} review",
                self.due.0, self.due.1, self.due.2
            );
            live_status(ui, &due);
        });
        let available_height = ui.available_height();
        let card_height = (available_height - 120.0).max(160.0);
        let shown_card = self.review.clone();
        egui::ScrollArea::vertical()
            .id_salt("focused-study-card")
            .max_height(card_height)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_min_height(card_height);
                    ui.add_space((card_height * 0.18).min(85.0));
                    if let Some(card) = &shown_card {
                        let foreground = review_card_text_color(ui.visuals().dark_mode);
                        ui.label(
                            egui::RichText::new(&card.question_html)
                                .size(25.0)
                                .strong()
                                .color(foreground),
                        );
                        if self.revealed {
                            ui.add_space(20.0);
                            ui.separator();
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new(inert_review_answer(&card.answer_html))
                                    .size(24.0)
                                    .color(foreground),
                            );
                        }
                    } else {
                        ui.heading("You're caught up for now.");
                        ui.weak("No cards are currently queued in this deck.");
                    }
                });
            });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            if shown_card.is_some() {
                if self.revealed {
                    ui.horizontal(|ui| {
                        let (width, inset) =
                            rating_row_geometry(ui.available_width(), ui.spacing().item_spacing.x);
                        ui.add_space(inset);
                        for (rating, name) in [(1, "Again"), (2, "Hard"), (3, "Good"), (4, "Easy")]
                        {
                            if ui
                                .add_sized([width, 42.0], egui::Button::new(name))
                                .clicked()
                            {
                                self.grade_revealed(rating);
                            }
                        }
                    });
                } else if ui
                    .add_sized([220.0, 44.0], egui::Button::new("Show answer"))
                    .clicked()
                {
                    self.revealed = true;
                }
            } else if ui.button("Refresh due queue").clicked() {
                self.refresh_review();
            }
            ui.add_space(8.0);
            ui.colored_label(
                focused_review_secondary_color(ui.visuals().dark_mode),
                "Cards are scheduled locally using Anki's review engine.",
            );
            if !self.status.is_empty() {
                ui.colored_label(
                    focused_review_secondary_color(ui.visuals().dark_mode),
                    &self.status,
                );
            }
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Learner workspace · managed Anki collections")
            .default_open(true)
            .show(ui, |ui| {
            ui.label("Choose a Retivus profile. Last used profile is remembered, but never opened automatically.");
            ui.horizontal_wrapped(|ui| {
                let root_label = ui.label("Profile root");
                if ui
                    .add_enabled(
                        self.backend.is_none(),
                        egui::TextEdit::singleline(&mut self.root).desired_width(280.0),
                    )
                    .labelled_by(root_label.id)
                    .changed()
                {
                    self.profile_location_changed();
                }
                let id_label = ui.label("Profile ID");
                if ui
                    .add_enabled(
                        self.backend.is_none(),
                        egui::TextEdit::singleline(&mut self.profile_id).desired_width(160.0),
                    )
                    .labelled_by(id_label.id)
                    .changed()
                {
                    self.profile_location_changed();
                }
            });
            ui.add_enabled_ui(self.backend.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Existing profiles");
                    let response = egui::ComboBox::from_id_salt("retivus-existing-profile")
                        .selected_text(if self.profile_id.is_empty() {
                            "Select a profile"
                        } else {
                            &self.profile_id
                        })
                        .show_ui(ui, |ui| {
                            let choices = Self::existing_profile_ids(Path::new(&self.root));
                            if choices.is_empty() {
                                ui.weak("No profiles in this root");
                            }
                            for id in choices {
                                ui.selectable_value(&mut self.profile_id, id.clone(), id);
                            }
                        })
                        .response
                        .labelled_by(label.id);
                    if response.changed() {
                        self.profile_location_changed();
                    }
                });
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
                if ui
                    .add_enabled(
                        self.backend.is_some(),
                        egui::Button::new("Check local sync blockers (offline)"),
                    )
                    .clicked()
                {
                    self.check_local_sync_readiness();
                }
            });
            egui::CollapsingHeader::new("Offline recovery · private pre-sync ZIP")
                .show(ui, |ui| {
                    ui.weak("Restores review history and media into a brand-new owned profile. Nothing is uploaded or merged. Keep this ZIP private.");
                    ui.add_enabled_ui(self.backend.is_none(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let label = ui.label("Private recovery ZIP");
                            if ui.add(
                                egui::TextEdit::singleline(&mut self.recovery_zip)
                                    .hint_text("/absolute/path/to/private-backup.zip")
                                    .desired_width(340.0),
                            ).labelled_by(label.id).changed() {
                                self.confirm_recovery = false;
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            let label = ui.label("New profile ID");
                            if ui.add(
                                egui::TextEdit::singleline(&mut self.restore_profile_id)
                                    .hint_text("must not already exist")
                                    .desired_width(200.0),
                            ).labelled_by(label.id).changed() {
                                self.confirm_recovery = false;
                            }
                        });
                        ui.checkbox(
                            &mut self.confirm_recovery,
                            "Confirm: restore into a NEW profile; never overwrite an existing one",
                        );
                        if ui
                            .add_enabled(
                                self.confirm_recovery,
                                egui::Button::new("Restore private ZIP as new profile"),
                            )
                            .clicked()
                        {
                            self.restore_private_recovery();
                        }
                    });
                    if self.backend.is_some() {
                        ui.weak("Close the active profile before offline recovery.");
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
                                node.set_role(egui::accesskit::Role::Status);
                                node.set_label(self.source_report.clone());
                                node.set_live(egui::accesskit::Live::Polite);
                                node.set_live_atomic();
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
                if ui.button("Study in focused view").clicked() {
                    self.start_focused_study();
                }
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
                        let safe_answer = inert_review_answer(&card.answer_html);
                        ui.label(egui::RichText::new(safe_answer).monospace());
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
    fn review_answer_fallback_hides_anki_separator_without_executing_html() {
        assert_eq!(
            inert_review_answer("Question<hr id=answer>\nAnswer"),
            "Answer"
        );
        assert_eq!(inert_review_answer("Plain response"), "Plain response");
        assert_eq!(
            inert_review_answer("<script>alert('x')</script>"),
            "<script>alert('x')</script>"
        );
    }

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
    fn profile_picker_lists_real_owned_directories_without_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "test".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        panel.close();
        fs::create_dir(root.path().join("unowned")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.path().join("test"), root.path().join("link")).unwrap();
        assert_eq!(StudyPanel::existing_profile_ids(root.path()), vec!["test"]);
    }

    #[test]
    fn restoring_profile_choice_does_not_open_collection_or_erase_cards() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "test".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let note_id = panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(1, "front", "back")
            .unwrap();
        panel.close();
        let (saved_root, saved_id) = panel.last_successful_profile().unwrap();
        let mut reopened = StudyPanel::default();
        reopened.restore_profile_selection(Some(saved_root.into()), Some(saved_id.into()));
        assert_eq!(reopened.root, saved_root);
        assert_eq!(reopened.profile_id, saved_id);
        assert!(!reopened.is_connected());
        reopened.connect(false);
        assert_eq!(
            reopened
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap(),
            ["front", "back"]
        );
        reopened.close();
    }

    #[test]
    fn correcting_failed_profile_location_clears_stale_error_and_creates_profile() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root
            .path()
            .join("missing-root")
            .to_string_lossy()
            .into_owned();
        panel.profile_id = "admin".into();
        panel.connect(true);
        assert!(panel.backend.is_none());
        assert!(panel.status.contains("missing-root"));

        // A corrected root and profile ID must not retain the previous failure.
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "test".into();
        panel.profile_location_changed();
        assert!(panel.status.is_empty());
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        assert!(panel.status.contains("test"));
        assert!(root.path().join("test/collection.anki2").is_file());
        panel.close();
    }

    #[test]
    fn offline_sync_readiness_in_study_panel_is_read_only_and_not_remote_approval() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.check_local_sync_readiness();
        assert!(panel.status.contains("Open a Retivus-managed"));
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "offline-safety".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        panel.check_local_sync_readiness();
        assert!(
            panel.status.contains("no local sync blockers"),
            "{}",
            panel.status
        );
        assert!(panel.status.contains("Remote conflicts"));
        assert!(
            !root.path().join("offline-safety/collection.mdb").exists(),
            "the offline inspection must not initialize the media index"
        );

        let note_id = panel
            .backend
            .as_mut()
            .unwrap()
            .add_basic_note(1, "private word", "private definition")
            .unwrap();
        let before = panel.backend.as_ref().unwrap().collection_stats().unwrap();
        panel.check_local_sync_readiness();
        assert!(
            panel.status.contains("Offline check refused"),
            "{}",
            panel.status
        );
        assert!(
            panel.status.contains("No network contacted"),
            "{}",
            panel.status
        );
        assert_eq!(
            panel.backend.as_ref().unwrap().collection_stats().unwrap(),
            before
        );
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap()[1],
            "private definition"
        );
        panel.close();
    }

    #[test]
    fn offline_sync_readiness_panel_detects_unindexed_media_without_remote_access() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "offline-media".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);

        let managed = root.path().join("offline-media");
        let media = anki::media::MediaManager::new(
            managed.join("collection.media"),
            managed.join("collection.mdb"),
        )
        .unwrap();
        media
            .add_file("clean.txt", b"existing synchronized content")
            .unwrap();
        drop(media);
        let index = managed.join("collection.mdb");
        let db = rusqlite::Connection::open(&index).unwrap();
        db.execute_batch("UPDATE media SET dirty=0; UPDATE meta SET lastUsn=7;")
            .unwrap();
        drop(db);

        panel.check_local_sync_readiness();
        assert!(
            panel.status.contains("no local sync blockers"),
            "{}",
            panel.status
        );
        assert!(
            panel.status.contains("media edits after inspection"),
            "{}",
            panel.status
        );

        let unindexed = managed.join("collection.media/external.txt");
        std::fs::write(&unindexed, b"manual edit outside Anki").unwrap();
        let before = panel.backend.as_ref().unwrap().collection_stats().unwrap();
        panel.check_local_sync_readiness();
        assert!(
            panel.status.contains("Offline check refused"),
            "{}",
            panel.status
        );
        assert!(
            panel.status.contains("absent from the clean media index"),
            "{}",
            panel.status
        );
        assert!(
            panel.status.contains("No network contacted"),
            "{}",
            panel.status
        );
        assert_eq!(
            panel.backend.as_ref().unwrap().collection_stats().unwrap(),
            before
        );
        assert_eq!(
            std::fs::read(&unindexed).unwrap(),
            b"manual edit outside Anki"
        );
        assert!(!root.path().join("network-recovery.zip").exists());
        panel.close();
    }

    #[test]
    fn offline_private_recovery_requires_consent_and_preserves_original_study_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = dir.path().to_string_lossy().into_owned();
        panel.profile_id = "original-study".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let backend = panel.backend.as_mut().unwrap();
        backend.enable_fsrs().unwrap();
        let note_id = backend
            .add_basic_note(1, "shared?", "private answer")
            .unwrap();
        let card_id = backend.grade_next_card(4).unwrap().unwrap();
        assert_eq!(backend.review_event_count(card_id).unwrap(), 1);
        let archive = dir.path().join("private-pre-sync-recovery.zip");
        backend.create_pre_sync_recovery_bundle(&archive).unwrap();
        panel.close();

        panel.recovery_zip = archive.to_string_lossy().into_owned();
        panel.restore_profile_id = "restored-study".into();
        panel.restore_private_recovery();
        assert!(panel.status.contains("Confirm"), "{}", panel.status);
        assert!(!dir.path().join("restored-study").exists());

        panel.confirm_recovery = true;
        panel.restore_private_recovery();
        assert!(panel.backend.is_some(), "{}", panel.status);
        assert_eq!(panel.profile_id, "restored-study");
        assert!(panel
            .status
            .contains("Original profiles were not replaced."));
        assert!(panel.recovery_zip.is_empty());
        assert!(!panel.confirm_recovery);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .note_fields(note_id)
                .unwrap()[1],
            "private answer"
        );
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            1
        );

        panel.close();
        panel.profile_id = "original-study".into();
        panel.connect(false);
        assert!(panel.backend.is_some(), "{}", panel.status);
        assert_eq!(
            panel
                .backend
                .as_ref()
                .unwrap()
                .review_event_count(card_id)
                .unwrap(),
            1
        );
        panel.close();

        panel.recovery_zip = archive.to_string_lossy().into_owned();
        panel.restore_profile_id = "restored-study".into();
        panel.confirm_recovery = true;
        panel.restore_private_recovery();
        assert!(panel.backend.is_none());
        assert!(panel.status.contains("already exists"), "{}", panel.status);
        assert!(!panel.confirm_recovery);
        assert!(archive.is_file());
        assert!(dir.path().join("original-study/collection.anki2").is_file());
        assert!(dir.path().join("restored-study/collection.anki2").is_file());
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
            .get_by_label("Add selected meaning to Tarium deck")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_label("Add selected meaning to Tarium deck")
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
    fn focused_review_header_and_rating_row_follow_theme_and_centering() {
        assert_eq!(
            focused_review_secondary_color(true),
            egui::Color32::from_gray(190)
        );
        assert_eq!(
            focused_review_secondary_color(false),
            egui::Color32::from_gray(75)
        );
        for (available, gap) in [(940.0, 6.0), (380.0, 6.0), (280.0, 4.0)] {
            let (width, inset) = rating_row_geometry(available, gap);
            let total = width * 4.0 + 3.0 * gap;
            assert!(inset >= 0.0);
            if total <= available {
                assert!((inset * 2.0 + total - available).abs() < 0.01);
            }
        }
        let (width, inset) = rating_row_geometry(940.0, 6.0);
        assert_eq!(width, 105.0);
        assert!(inset > 200.0);
    }

    #[test]
    fn focused_review_card_text_contrasts_with_both_system_themes() {
        assert_eq!(review_card_text_color(true), egui::Color32::WHITE);
        assert_eq!(review_card_text_color(false), egui::Color32::BLACK);
        for (theme, expected) in [
            (egui::Theme::Dark, egui::Color32::WHITE),
            (egui::Theme::Light, egui::Color32::BLACK),
        ] {
            let ctx = egui::Context::default();
            ctx.set_theme(match theme {
                egui::Theme::Dark => egui::ThemePreference::Dark,
                egui::Theme::Light => egui::ThemePreference::Light,
            });
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(review_card_text_color(ui.visuals().dark_mode), expected);
                });
            });
        }
    }

    #[test]
    fn focused_study_uses_own_view_and_records_real_anki_review() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let root = tempfile::tempdir().unwrap();
        let mut harness = Harness::new_eframe(|_| crate::Retivus::default());
        let card_id = {
            let app = harness.state_mut();
            app.study.root = root.path().to_string_lossy().into_owned();
            app.study.profile_id = "focus-review".into();
            app.study.connect(true);
            assert!(app.study.backend.is_some(), "{}", app.study.status);
            app.study
                .backend
                .as_mut()
                .unwrap()
                .add_basic_note(1, "What is 2 + 2?", "Four")
                .unwrap();
            app.study.refresh();
            app.study.refresh_review();
            app.study.review.as_ref().unwrap().card_id
        };
        harness.set_size(egui::vec2(1024.0, 700.0));
        harness.run();
        harness.get_by_label("Study in focused view").scroll_to_me();
        harness.run();
        harness.get_by_label("Study in focused view").click();
        harness.run();
        assert!(harness.state().study.is_focused_study());
        harness.get_by_label("Show answer").click();
        harness.run();
        assert!(harness.state().study.revealed);
        harness.get_by_label("Good").click();
        harness.run();
        assert!(harness.state().study.is_focused_study());
        assert_eq!(harness.state().study.stats.review_events, 1);
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
        harness.get_by_label("Back to decks").click();
        harness.run();
        assert!(!harness.state().study.is_focused_study());
        assert_eq!(harness.state().study.stats.review_events, 1);
        drop(harness);
        let reopened = AnkiBackend::open_managed_profile(root.path(), "focus-review").unwrap();
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
    fn separate_language_providers_create_only_explicit_cards_in_own_decks() {
        use retivus::source_adapter::{SourceInfo, StudyTsvV1};
        use std::io::Cursor;

        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "provider-decks".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);

        for (index, (provider, language, word)) in [
            ("japanese-tarium", "ja", "海"),
            ("mexican-tarium", "es-MX", "mar"),
        ]
        .into_iter()
        .enumerate()
        {
            let adapter = StudyTsvV1::new(SourceInfo {
                provider_id: provider.into(),
                edition: "v1".into(),
                lexical_language: language.into(),
                definition_language: "en".into(),
                license_notice: "Synthetic original fixture".into(),
            })
            .unwrap();
            let path = root.path().join(format!("{provider}.tsv"));
            let data = format!(
                "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n{provider}\t{word}\tnoun\ts1\tsea\n"
            );
            fs::write(&path, &data).unwrap();
            let mut rows = Vec::new();
            adapter
                .visit(&mut BufReader::new(Cursor::new(data)), &mut |row| {
                    rows.push(row)
                })
                .unwrap();
            assert_eq!(panel.stats.notes, index);
            let note_id = panel
                .add_validated_source_to_provider_deck(&adapter, &path, &rows[0], "s1")
                .unwrap();
            assert_eq!(
                panel
                    .backend
                    .as_ref()
                    .unwrap()
                    .note_fields(note_id)
                    .unwrap()[0],
                word
            );
            let expected = format!("Tarium::{language}::{provider}");
            assert!(panel
                .decks
                .iter()
                .any(|(id, name)| Some(*id) == panel.deck_id && name == &expected));
            let decks_before = panel.decks.clone();
            let stats_before = panel.stats;
            let error = panel
                .add_validated_source_to_provider_deck(&adapter, &path, &rows[0], "s1")
                .unwrap_err();
            assert!(
                error.contains("no duplicate card or deck created"),
                "{error}"
            );
            assert_eq!(panel.decks, decks_before);
            assert_eq!(panel.stats, stats_before);
        }
        let decks = panel
            .decks
            .iter()
            .filter(|(_, name)| name.starts_with("Tarium::") && name.matches("::").count() == 2)
            .count();
        assert_eq!(decks, 2, "only the two source-specific leaf decks exist");
        assert_eq!(panel.stats.notes, 2);
        assert_eq!(panel.stats.review_events, 0);
        panel.close();
    }

    #[test]
    fn unsafe_provider_names_cannot_create_confusing_deck_paths() {
        use retivus::source_adapter::SourceInfo;
        let base = SourceInfo {
            provider_id: "other::Default".into(),
            edition: "v1".into(),
            lexical_language: "es-MX".into(),
            definition_language: "en".into(),
            license_notice: "Test fixture".into(),
        };
        assert!(StudyPanel::provider_deck_name(&base).is_err());
        let mut changed = base;
        changed.provider_id = "another-source".into();
        changed.lexical_language = "es-MX".into();
        assert_eq!(
            StudyPanel::provider_deck_name(&changed).unwrap(),
            "Tarium::es-MX::another-source"
        );
    }

    #[test]
    fn unverified_mexilex_jsonl_cannot_touch_catalog_or_create_decks() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel::default();
        panel.root = root.path().to_string_lossy().into_owned();
        panel.profile_id = "checksum-protected".into();
        panel.connect(true);
        assert!(panel.backend.is_some(), "{}", panel.status);
        let source = root.path().join("unverified.jsonl");
        fs::write(&source, b"{\"source\":\"mexilex\"}\n").unwrap();
        let before = panel.decks.clone();
        let err = panel.stage_verified_mexilex_snapshot(&source).unwrap_err();
        assert!(err.contains("checksum differs"), "{err}");
        assert!(!panel.managed_catalog_path().unwrap().exists());
        assert_eq!(panel.decks, before);
        assert_eq!(panel.stats.notes, 0);
        assert_eq!(panel.stats.review_events, 0);
        #[cfg(unix)]
        {
            let symlink = root.path().join("linked-source.jsonl");
            std::os::unix::fs::symlink(&source, &symlink).unwrap();
            assert!(panel.stage_verified_mexilex_snapshot(&symlink).is_err());
            assert!(!panel.managed_catalog_path().unwrap().exists());
        }
        panel.close();
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
