//! Explicit native learner-workspace operations behind the Anki feature gate.
//! No profile opens, is created, or is migrated until a user chooses an action.
use eframe::egui;
use retivus::anki_backend::{AnkiBackend, NoteRow, ReviewPreview};
use retivus::source_adapter::{SourceAdapter, SourceRecord};
use retivus::source_store::{SourceCardCandidate, SourceCatalog};
use std::fs;
use std::io::BufReader;
use std::path::Path;

#[derive(Default)]
pub struct StudyPanel {
    root: String,
    profile_id: String,
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
    selected_note: Option<i64>,
    review: Option<ReviewPreview>,
    revealed: bool,
    due: (usize, usize, usize),
    status: String,
}

impl StudyPanel {
    fn connect(&mut self, create: bool) {
        if self.backend.is_some() {
            self.status = "Close the active profile before switching profiles.".into();
            return;
        }
        let root = Path::new(&self.root);
        let result = if create {
            AnkiBackend::create_managed_profile(root, &self.profile_id)
        } else {
            AnkiBackend::open_managed_profile(root, &self.profile_id)
        };
        match result {
            Ok(backend) => {
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
        match (backend.list_decks(), backend.browse_notes(&self.query, 50)) {
            (Ok(decks), Ok(notes)) => {
                self.decks = decks;
                if !self.decks.iter().any(|(id, _)| Some(*id) == self.deck_id) {
                    self.deck_id = self.decks.first().map(|(id, _)| *id);
                }
                self.notes = notes;
            }
            (Err(error), _) | (_, Err(error)) => {
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
        match (backend.due_counts(), backend.next_review_preview()) {
            (Ok(due), Ok(review)) => {
                self.due = due;
                self.review = review;
            }
            (Err(error), _) | (_, Err(error)) => {
                self.review = None;
                self.status = format!("Could not read Anki queue: {error}");
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
        let profile_root = fs::canonicalize(&self.root).map_err(|e| e.to_string())?;
        let profile_dir = profile_root.join(&self.profile_id);
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
                return Err("Mexilex source snapshot is not in the verified release catalog".into());
            }
            let file = fs::symlink_metadata(source_path).map_err(|e| e.to_string())?;
            if !file.is_file() || file.file_type().is_symlink() {
                return Err("source must be a regular file, not a symlink".into());
            }
            let input = fs::File::open(source_path).map_err(|e| e.to_string())?;
            catalog.import_snapshot(&snapshot, adapter, &mut BufReader::new(input))?;
        }
        let actual = catalog.select_sense(
            &origin.provider_id,
            &snapshot,
            &record.entry_id,
            sense_id,
        )?;
        if actual != expected {
            return Err("source selection differs from the immutable imported snapshot".into());
        }
        let note_id = self
            .backend
            .as_mut()
            .ok_or("learner profile closed during import")?
            .add_source_candidate(deck_id, &actual)
            .map_err(|e| e.to_string())?;
        self.status = format!("Created note {note_id} from an exact source sense; provenance saved.");
        self.refresh();
        self.refresh_review();
        Ok(note_id)
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

    fn close(&mut self) {
        if let Some(backend) = self.backend.take() {
            self.status = match backend.close() {
                Ok(()) => "Closed the learner profile safely.".into(),
                Err(error) => format!("Profile close reported an error: {error}"),
            };
        }
        self.decks.clear();
        self.notes.clear();
        self.deck_id = None;
        self.selected_note = None;
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
                ui.add(egui::TextEdit::singleline(&mut self.root).desired_width(280.0));
                ui.label("Profile ID");
                ui.add(egui::TextEdit::singleline(&mut self.profile_id).desired_width(160.0));
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
                    egui::ComboBox::from_id_salt("retivus-active-deck")
                        .selected_text(self.decks.iter().find(|(id, _)| Some(*id) == self.deck_id)
                            .map_or("Select deck", |(_, name)| name.as_str()))
                        .show_ui(ui, |ui| {
                            for (id, name) in &self.decks {
                                ui.selectable_value(&mut self.deck_id, Some(*id), name);
                            }
                        });
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
                egui::ScrollArea::vertical().max_height(135.0).show(ui, |ui| {
                    for note in self.notes.clone() {
                        let summary = note.fields.first().map_or("(empty note)", String::as_str);
                        let label = format!("{} · {}", note.id, summary);
                        if ui.selectable_label(self.selected_note == Some(note.id), label).clicked() {
                            self.selected_note = Some(note.id);
                            self.front = note.fields.first().cloned().unwrap_or_default();
                            self.back = note.fields.get(1).cloned().unwrap_or_default();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("New basic note").clicked() {
                        self.selected_note = None;
                        self.front.clear();
                        self.back.clear();
                    }
                    if ui.button("Undo last Anki edit").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.undo_last() {
                                Ok(()) => { self.status = "Last edit undone.".into(); self.refresh(); self.refresh_review(); }
                                Err(error) => self.status = format!("Cannot undo: {error}"),
                            }
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Front");
                    ui.add(egui::TextEdit::singleline(&mut self.front).desired_width(220.0));
                    ui.label("Back");
                    ui.add(egui::TextEdit::singleline(&mut self.back).desired_width(220.0));
                });
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
    fn stateful_native_study_flow_requires_reveal_and_preserves_review_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut panel = StudyPanel {
            root: dir.path().to_string_lossy().into_owned(),
            profile_id: "fixture-learner".into(),
            ..StudyPanel::default()
        };
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
            .visit(&mut BufReader::new(fs::File::open(&source_file).unwrap()), &mut |row| {
                preview.push(row);
            })
            .unwrap();
        let mut panel = StudyPanel {
            root: dir.path().to_string_lossy().into_owned(),
            profile_id: "source-learner".into(),
            ..StudyPanel::default()
        };
        panel.connect(true);
        let deck = panel.backend.as_mut().unwrap().create_deck("Japanese").unwrap();
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
        let second_tags = panel.backend.as_ref().unwrap().note_tags(second).unwrap();
        assert_ne!(first_tags, second_tags, "homographs retain distinct source identities");
        assert_eq!(panel.backend.as_ref().unwrap().note_fields(first).unwrap(), ["橋", "bridge"]);
        assert_eq!(panel.backend.as_ref().unwrap().note_fields(second).unwrap(), ["橋", "connect"]);
        panel.close();
        panel.connect(false);
        assert_eq!(panel.backend.as_ref().unwrap().note_fields(second).unwrap(), ["橋", "connect"]);
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
