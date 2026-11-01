//! Explicit native learner-workspace operations behind the Anki feature gate.
//! No profile opens, is created, or is migrated until a user chooses an action.
use eframe::egui;
use retivus::anki_backend::{AnkiBackend, NoteRow};
use std::path::Path;

#[derive(Default)]
pub struct StudyPanel {
    root: String,
    profile_id: String,
    package: String,
    backend: Option<AnkiBackend>,
    decks: Vec<(i64, String)>,
    deck_id: Option<i64>,
    new_deck: String,
    front: String,
    back: String,
    query: String,
    notes: Vec<NoteRow>,
    selected_note: Option<i64>,
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
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Learner workspace · managed Anki collections", |ui| {
            ui.label("Choose an existing root directory and a Retivus profile ID. No ordinary Anki profile is opened automatically.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Profile root");
                ui.add(egui::TextEdit::singleline(&mut self.root).desired_width(280.0));
                ui.label("Profile ID");
                ui.add(egui::TextEdit::singleline(&mut self.profile_id).desired_width(160.0));
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
                                Ok(()) => { self.status = "Last edit undone.".into(); self.refresh(); }
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
                                Ok(()) => { self.status = "Note updated; undo is available.".into(); self.refresh(); }
                                Err(error) => self.status = format!("Note not edited: {error}"),
                            }
                        }
                    }
                } else if ui.button("Add basic note to selected deck").clicked() {
                    match (self.backend.as_mut(), self.deck_id) {
                        (Some(backend), Some(deck_id)) => match backend.add_basic_note(deck_id, &self.front, &self.back) {
                            Ok(_) => { self.status = "Note added to selected deck.".into(); self.refresh(); }
                            Err(error) => self.status = format!("Note not added: {error}"),
                        },
                        _ => self.status = "Select a deck first.".into(),
                    }
                }
                ui.separator();
                ui.heading("Anki package");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Absolute .apkg path");
                    ui.add(egui::TextEdit::singleline(&mut self.package).desired_width(300.0));
                    if ui.button("Import package").clicked() {
                        if let Some(backend) = self.backend.as_mut() {
                            match backend.import_apkg(Path::new(&self.package)) {
                                Ok(()) => { self.status = "Package imported into this profile.".into(); self.refresh(); }
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
    fn learner_workspace_never_opens_user_profiles_on_startup() {
        let state = StudyPanel::default();
        assert!(state.backend.is_none());
        assert!(state.root.is_empty());
        assert!(state.profile_id.is_empty());
    }
}
