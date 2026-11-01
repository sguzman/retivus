//! Explicit native learner-workspace operations behind the Anki feature gate.
//! No profile opens, is created, or is migrated until a user chooses an action.
use eframe::egui;
use retivus::anki_backend::{AnkiBackend, CollectionStats, NoteRow, ReviewPreview};
use retivus::source_adapter::{SourceAdapter, SourceRecord};
use retivus::source_store::{SourceCardCandidate, SourceCatalog};
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

impl StudyPanel {
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
        let profile_root = self
            .active_root
            .as_ref()
            .ok_or("learner profile root is not pinned")?;
        let profile_id = self
            .active_profile_id
            .as_deref()
            .ok_or("learner profile identity is not pinned")?;
        let profile_dir = profile_root.join(profile_id);
        let directory = fs::symlink_metadata(&profile_dir).map_err(|e| e.to_string())?;
        if !directory.is_dir() || directory.file_type().is_symlink() {
            return Err("learner profile must be a real Retivus-managed directory".into());
        }
        let catalog_file = profile_dir.join("source-catalog.sqlite3");
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
                self.status = format!("Deleted note {note_id}; Undo last Anki edit can restore it.");
                self.selected_note = None;
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
        self.confirm_delete = false;
        // Never display editor contents, search terms, or file paths from
        // a previous profile after the learner switches collections.
        self.front.clear();
        self.back.clear();
        self.query.clear();
        self.package.clear();
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
                ui.label("Profile root");
                ui.add_enabled(
                    self.backend.is_none(),
                    egui::TextEdit::singleline(&mut self.root).desired_width(280.0),
                );
                ui.label("Profile ID");
                ui.add_enabled(
                    self.backend.is_none(),
                    egui::TextEdit::singleline(&mut self.profile_id).desired_width(160.0),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Source snapshot ID");
                ui.add(egui::TextEdit::singleline(&mut self.snapshot_id).hint_text("blank = source edition").desired_width(240.0));
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
                    egui::ComboBox::from_id_salt("retivus-active-deck")
                        .selected_text(self.decks.iter().find(|(id, _)| Some(*id) == self.deck_id)
                            .map_or("Select deck", |(_, name)| name.as_str()))
                        .show_ui(ui, |ui| {
                            for (id, name) in &self.decks {
                                ui.selectable_value(&mut self.deck_id, Some(*id), name);
                            }
                        });
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
                    ui.label("Search notes");
                    ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Anki search syntax").desired_width(280.0));
                    if ui.button("Search / refresh").clicked() { self.refresh(); }
                    ui.weak("Up to 50 matching notes; queries use Anki's own search parser.");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Browse filter");
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
                        });
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
                            self.confirm_delete = false;
                            self.front = note.fields.first().cloned().unwrap_or_default();
                            self.back = note.fields.get(1).cloned().unwrap_or_default();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("New basic note").clicked() {
                        self.selected_note = None;
                        self.confirm_delete = false;
                        self.front.clear();
                        self.back.clear();
                    }
                    if ui.button("Undo last Anki edit").clicked() {
                        self.undo_last_edit();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Front");
                    ui.add(egui::TextEdit::singleline(&mut self.front).desired_width(220.0));
                    ui.label("Back");
                    ui.add(egui::TextEdit::singleline(&mut self.back).desired_width(220.0));
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
                ui.heading("Profile statistics");
                ui.label(format!(
                    "{} notes · {} cards · {} recorded reviews (all decks)",
                    self.stats.notes, self.stats.cards, self.stats.review_events
                ));
                ui.heading("Study · Anki scheduler");
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "Due: {} new · {} learning · {} review",
                        self.due.0, self.due.1, self.due.2
                    ));
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
                ui.horizontal_wrapped(|ui| {
                    ui.label("Absolute .apkg path");
                    ui.add(egui::TextEdit::singleline(&mut self.package).desired_width(300.0));
                    if ui.button("Import package").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.import_apkg(Path::new(&self.package)) {
                                Ok(()) => { self.status = "Package imported into this profile.".into(); self.refresh(); self.refresh_review(); }
                                Err(error) => self.status = format!("Import refused: {error}"),
                            }
                        }
                    }
                    if ui.button("Export new package").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.export_apkg_to(Path::new(&self.package)) {
                                Ok(count) => self.status = format!("Exported {count} notes and media."),
                                Err(error) => self.status = format!("Export refused: {error}"),
                            }
                        }
                    }
                });
                ui.weak("Imports never open personal Anki profiles. Exports refuse existing files.");
            }
            if !self.status.is_empty() { ui.label(&self.status); }
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
            panel.backend.as_ref().unwrap().note_fields(note_id).unwrap(),
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
        let deck = panel.backend.as_mut().unwrap().create_deck("Spanish").unwrap();
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
            panel.backend.as_ref().unwrap().note_fields(note_id).unwrap(),
            ["árbol", "tree"]
        );
        panel.confirm_delete = true;
        panel.delete_selected_note();
        assert!(panel.status.contains("Deleted note"), "{}", panel.status);
        assert!(!panel.confirm_delete);
        assert!(panel.selected_note.is_none());
        assert!(panel.backend.as_ref().unwrap().note_fields(note_id).is_err());
        panel.undo_last_edit();
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(note_id).unwrap(),
            ["árbol", "tree"]
        );
        panel.close();
        panel.connect(false);
        assert_eq!(
            panel.backend.as_ref().unwrap().note_fields(note_id).unwrap(),
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
