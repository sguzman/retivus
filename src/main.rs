use eframe::egui;
use retivus::{brand, source_adapter, typography};
use std::fs::File;
use std::io::BufReader;

#[cfg(feature = "anki-backend")]
mod study_ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    System,
    Light,
    Dark,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFormat {
    Mexilex,
    GenericTsv,
}

/// A read-only selection from the validated preview, not yet a learner card.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedSense {
    provider_id: String,
    edition: String,
    entry_id: String,
    sense_id: String,
    word: String,
    definition: String,
}

impl SelectedSense {
    fn from_record(record: &source_adapter::SourceRecord, sense_id: &str) -> Option<Self> {
        let sense = record
            .senses
            .iter()
            .find(|item| item.sense_id == sense_id)?;
        Some(Self {
            provider_id: record.provenance.provider_id.clone(),
            edition: record.provenance.edition.clone(),
            entry_id: record.entry_id.clone(),
            sense_id: sense.sense_id.clone(),
            word: record.word.clone(),
            definition: sense.definition.clone(),
        })
    }
}

const THEME_STORAGE_KEY: &str = "retivus.theme.v1";
const SCALE_STORAGE_KEY: &str = "retivus.ui-scale.v1";
const DENSITY_STORAGE_KEY: &str = "retivus.density.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Density {
    Compact,
    Comfortable,
}

impl Density {
    fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("compact") => Self::Compact,
            _ => Self::Comfortable,
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Comfortable => "Comfortable",
        }
    }

    fn spacing(self) -> egui::Vec2 {
        match self {
            Self::Compact => egui::vec2(6.0, 3.0),
            Self::Comfortable => egui::vec2(9.0, 7.0),
        }
    }
}

fn parse_scale(value: Option<&str>) -> f32 {
    value
        .and_then(|raw| raw.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.80, 1.50))
        .unwrap_or(1.0)
}

impl Theme {
    fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}
struct Retivus {
    theme: Theme,
    ui_scale: f32,
    density: Density,
    source_path: String,
    import_status: String,
    source_format: SourceFormat,
    source_id: String,
    source_edition: String,
    source_language: String,
    definition_language: String,
    license_notice: String,
    preview: Vec<source_adapter::SourceRecord>,
    selected_sense: Option<SelectedSense>,
    krypton_font_loaded: bool,
    #[cfg(feature = "anki-backend")]
    study: study_ui::StudyPanel,
}
impl Default for Retivus {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            ui_scale: 1.0,
            density: Density::Comfortable,
            source_path: String::new(),
            import_status: "No source checked".into(),
            source_format: SourceFormat::Mexilex,
            source_id: String::new(),
            source_edition: String::new(),
            source_language: String::new(),
            definition_language: String::new(),
            license_notice: String::new(),
            preview: Vec::new(),
            selected_sense: None,
            krypton_font_loaded: false,
            #[cfg(feature = "anki-backend")]
            study: study_ui::StudyPanel::default(),
        }
    }
}
impl Retivus {
    fn source_adapter(&self) -> Result<Box<dyn source_adapter::SourceAdapter>, String> {
        match self.source_format {
            SourceFormat::Mexilex => Ok(Box::new(source_adapter::MexilexV1::default())),
            SourceFormat::GenericTsv => {
                source_adapter::StudyTsvV1::new(source_adapter::SourceInfo {
                    provider_id: self.source_id.clone(),
                    edition: self.source_edition.clone(),
                    lexical_language: self.source_language.clone(),
                    definition_language: self.definition_language.clone(),
                    license_notice: self.license_notice.clone(),
                })
                .map(|adapter| Box::new(adapter) as Box<dyn source_adapter::SourceAdapter>)
            }
        }
    }

    /// Single validation path shared by the button and real-profile regression.
    /// A failed replacement source must never leave an earlier choice selected.
    fn validate_source_file(&mut self) {
        self.preview.clear();
        self.selected_sense = None;
        self.import_status = match self.source_adapter() {
            Err(error) => format!("Invalid source descriptor: {error}"),
            Ok(adapter) => match File::open(&self.source_path) {
                Err(error) => format!("Could not open source: {error}"),
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    match retivus::preview::validate_preview(adapter.as_ref(), &mut reader) {
                        Ok(result) => {
                            self.preview = result.rows;
                            format!(
                                "Valid records: {} · no learner data changed",
                                result.validated_count
                            )
                        }
                        Err(error) => format!("Validation failed: {error}"),
                    }
                }
            },
        };
    }

    #[cfg(feature = "anki-backend")]
    fn stage_validated_source_to_study(&mut self) {
        let result = (|| {
            if self.source_format != SourceFormat::GenericTsv {
                return Err("Mexilex requires checksum-verified release ingestion".into());
            }
            if self.preview.is_empty() {
                return Err("Validate a nonempty source before staging it".into());
            }
            let adapter = self.source_adapter()?;
            self.study
                .stage_generic_snapshot(adapter.as_ref(), std::path::Path::new(&self.source_path))
        })();
        self.import_status = match result {
            Ok(count) => {
                format!("Staged {count} source entries; no cards or review data changed.")
            }
            Err(error) => format!("Source staging refused: {error}"),
        };
    }

    #[cfg(feature = "anki-backend")]
    fn add_selected_sense_to_study(&mut self) {
        let result = (|| {
            let chosen = self
                .selected_sense
                .as_ref()
                .ok_or("select a validated source sense first")?;
            let record = self
                .preview
                .iter()
                .find(|record| {
                    record.provenance.provider_id == chosen.provider_id
                        && record.provenance.edition == chosen.edition
                        && record.entry_id == chosen.entry_id
                        && record.word == chosen.word
                        && record.senses.iter().any(|sense| {
                            sense.sense_id == chosen.sense_id
                                && sense.definition == chosen.definition
                        })
                })
                .ok_or("selected source sense is not in validated preview")?;
            let adapter = self.source_adapter()?;
            self.study.add_validated_source_choice(
                adapter.as_ref(),
                std::path::Path::new(&self.source_path),
                record,
                &chosen.sense_id,
            )
        })();
        self.import_status = match result {
            Ok(note) => format!("Created study note {note} from selected meaning."),
            Err(error) => format!("Not added to learner profile: {error}"),
        };
    }

    fn restore(storage: Option<&dyn eframe::Storage>) -> Self {
        let theme = storage.and_then(|value| value.get_string(THEME_STORAGE_KEY));
        let scale = storage.and_then(|value| value.get_string(SCALE_STORAGE_KEY));
        let density = storage.and_then(|value| value.get_string(DENSITY_STORAGE_KEY));
        Self {
            theme: Theme::from_stored(theme.as_deref()),
            ui_scale: parse_scale(scale.as_deref()),
            density: Density::from_stored(density.as_deref()),
            ..Self::default()
        }
    }
}

impl eframe::App for Retivus {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(THEME_STORAGE_KEY, self.theme.stored().into());
        storage.set_string(SCALE_STORAGE_KEY, self.ui_scale.to_string());
        storage.set_string(DENSITY_STORAGE_KEY, self.density.stored().into());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Switching back to System must restore live operating-system theme tracking.
        ctx.set_theme(self.theme.preference());
        ctx.style_mut(|style| style.spacing.item_spacing = self.density.spacing());
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Retivus")
                        .heading()
                        .color(brand::accent(ctx.theme())),
                );
                ui.separator();
                if self.krypton_font_loaded {
                    ui.weak("Krypton Nerd Font");
                }
                let appearance_label = ui.label("Appearance");
                egui::ComboBox::from_id_salt("appearance")
                    .selected_text(self.theme.name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.theme, Theme::System, "System");
                        ui.selectable_value(&mut self.theme, Theme::Light, "Light");
                        ui.selectable_value(&mut self.theme, Theme::Dark, "Dark");
                    })
                    .response
                    .labelled_by(appearance_label.id);
                ui.separator();
                let density_label = ui.label("Density");
                egui::ComboBox::from_id_salt("density")
                    .selected_text(self.density.name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.density, Density::Compact, "Compact");
                        ui.selectable_value(&mut self.density, Density::Comfortable, "Comfortable");
                    })
                    .response
                    .labelled_by(density_label.id);
                ui.separator();
                if ui
                    .add(egui::Slider::new(&mut self.ui_scale, 0.80..=1.50).text("Scale"))
                    .changed()
                {
                    ctx.set_zoom_factor(self.ui_scale);
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Learn from any source.");
            ui.label("Anki ecosystem compatibility and versioned external data providers.");
            ui.separator();
            #[cfg(not(feature = "anki-backend"))]
            ui.weak("Preview mode: validated sources only. Study, editing and review controls are not available in this window yet.");
            ui.weak("Keyboard: Tab and Shift+Tab move between controls; Enter or Space activates the focused control.");
            ui.label("First data source: Mexilex · es-MX → en");
            #[cfg(not(feature = "anki-backend"))]
            ui.label("Anki Rust backend: separately feature-gated and under integration; this preview never opens learner collections.");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                #[cfg(feature = "anki-backend")]
                {
                    self.study.ui(ui);
                    ui.separator();
                }
                ui.heading("Validate a local source file");
                ui.weak("Read-only validation · never creates cards or changes review history");
                ui.horizontal(|ui| {
                    let format_label = ui.label("Format");
                    egui::ComboBox::from_id_salt("source-format")
                        .selected_text(match self.source_format {
                            SourceFormat::Mexilex => "Mexilex study JSONL v1",
                            SourceFormat::GenericTsv => "Generic study TSV v1",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.source_format,
                                SourceFormat::Mexilex,
                                "Mexilex study JSONL v1",
                            );
                            ui.selectable_value(
                                &mut self.source_format,
                                SourceFormat::GenericTsv,
                                "Generic study TSV v1",
                            );
                        })
                        .response
                        .labelled_by(format_label.id);
                });
                if self.source_format == SourceFormat::GenericTsv {
                    ui.weak("Supply source identifiers and licensing metadata for this import.");
                    for (label, value) in [
                        ("Provider ID", &mut self.source_id),
                        ("Edition / snapshot", &mut self.source_edition),
                        ("Source language", &mut self.source_language),
                        ("Meaning language", &mut self.definition_language),
                        ("License notice", &mut self.license_notice),
                    ] {
                        ui.horizontal(|ui| {
                            let description = ui.label(label);
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .desired_width(ui.available_width().min(260.0)),
                            )
                            .labelled_by(description.id);
                        });
                    }
                }
                ui.horizontal(|ui| {
                    let label = ui.label("File");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.source_path)
                            .desired_width(ui.available_width().min(300.0)),
                    )
                    .labelled_by(label.id);
                });
                if ui.button("Validate source").clicked() {
                    self.validate_source_file();
                }
                #[cfg(feature = "anki-backend")]
                if self.source_format == SourceFormat::GenericTsv
                    && ui
                        .add_enabled(
                            self.study.is_connected() && !self.preview.is_empty(),
                            egui::Button::new("Stage complete source snapshot (no cards)"),
                        )
                        .clicked()
                {
                    self.stage_validated_source_to_study();
                }
                ui.label(&self.import_status);
                if let Some(chosen) = self.selected_sense.clone() {
                    ui.group(|ui| {
                        ui.heading("Selected source meaning");
                        ui.strong(format!("{} → {}", chosen.word, chosen.definition));
                        ui.weak(format!(
                            "Provider {} · edition {} · entry {} · sense {}",
                            chosen.provider_id, chosen.edition, chosen.entry_id, chosen.sense_id
                        ));
                        #[cfg(not(feature = "anki-backend"))]
                        ui.label("Preview only. Enable the Anki backend to create study notes from validated source senses.");
                        #[cfg(feature = "anki-backend")]
                        ui.label("Add this sense only to an explicitly opened Retivus learner profile and selected deck.");
                        #[cfg(feature = "anki-backend")]
                        if ui.button("Add selected meaning to learner deck").clicked() {
                            self.add_selected_sense_to_study();
                        }
                    });
                }
                if !self.preview.is_empty() {
                    ui.separator();
                    ui.heading("Vocabulary preview");
                    ui.weak("First 12 validated records · select one specific sense to review or explicitly add");
                    for record in &self.preview {
                        ui.push_id((&record.provenance.provider_id, &record.entry_id), |ui| {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&record.word);
                                    ui.weak(&record.part_of_speech);
                                });
                                for sense in record.senses.iter().take(3) {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(&sense.definition);
                                        if ui.small_button("Select sense").clicked() {
                                            self.selected_sense =
                                                SelectedSense::from_record(record, &sense.sense_id);
                                        }
                                    });
                                }
                                if record.senses.len() > 3 {
                                    ui.collapsing(
                                        format!("{} more meanings", record.senses.len() - 3),
                                        |ui| {
                                            for sense in record.senses.iter().skip(3) {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label(&sense.definition);
                                                    if ui.small_button("Select sense").clicked() {
                                                        self.selected_sense =
                                                            SelectedSense::from_record(
                                                                record, &sense.sense_id,
                                                            );
                                                    }
                                                });
                                            }
                                        },
                                    );
                                }
                            });
                        });
                    }
                }
            });
        });
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Retivus",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            brand::apply_to_context(&cc.egui_ctx);
            let mut app = Retivus::restore(cc.storage);
            app.krypton_font_loaded = typography::install_local_krypton(&cc.egui_ctx);
            cc.egui_ctx.set_zoom_factor(app.ui_scale);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct TestStorage(HashMap<String, String>);

    impl eframe::Storage for TestStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn theme_persists_across_new_app_instances() {
        let mut storage = TestStorage::default();
        assert_eq!(Retivus::restore(Some(&storage)).theme, Theme::System);
        for theme in [Theme::Light, Theme::Dark, Theme::System] {
            let mut first = Retivus {
                theme,
                ..Retivus::default()
            };
            eframe::App::save(&mut first, &mut storage);
            assert_eq!(Retivus::restore(Some(&storage)).theme, theme);
        }
    }

    #[test]
    fn theme_modes_render_and_system_tracks_operating_system_input() {
        let ctx = egui::Context::default();
        brand::apply_to_context(&ctx);
        for choice in [Theme::System, Theme::Light, Theme::Dark] {
            ctx.set_theme(choice.preference());
            for system_theme in [egui::Theme::Light, egui::Theme::Dark] {
                let output = ctx.run(
                    egui::RawInput {
                        system_theme: Some(system_theme),
                        ..egui::RawInput::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.heading("Retivus");
                            ui.label("Review");
                        });
                    },
                );
                assert!(!output.shapes.is_empty());
                let expected = match choice {
                    Theme::System => system_theme,
                    Theme::Light => egui::Theme::Light,
                    Theme::Dark => egui::Theme::Dark,
                };
                assert_eq!(ctx.theme(), expected);
            }
        }
    }

    #[test]
    fn scale_and_density_preferences_round_trip_with_invalid_value_fallback() {
        let mut storage = TestStorage::default();
        let mut state = Retivus {
            ui_scale: 1.25,
            density: Density::Compact,
            ..Retivus::default()
        };
        eframe::App::save(&mut state, &mut storage);
        let restored = Retivus::restore(Some(&storage));
        assert_eq!(restored.ui_scale, 1.25);
        assert_eq!(restored.density, Density::Compact);
        assert_eq!(restored.density.spacing(), egui::vec2(6.0, 3.0));

        for invalid in ["NaN", "inf", "nonsense", ""] {
            eframe::Storage::set_string(&mut storage, SCALE_STORAGE_KEY, invalid.into());
            assert_eq!(Retivus::restore(Some(&storage)).ui_scale, 1.0);
        }
        eframe::Storage::set_string(&mut storage, SCALE_STORAGE_KEY, "9".into());
        assert_eq!(Retivus::restore(Some(&storage)).ui_scale, 1.5);
        eframe::Storage::set_string(&mut storage, SCALE_STORAGE_KEY, "-8".into());
        assert_eq!(Retivus::restore(Some(&storage)).ui_scale, 0.8);
        eframe::Storage::set_string(&mut storage, DENSITY_STORAGE_KEY, "invalid".into());
        assert_eq!(
            Retivus::restore(Some(&storage)).density,
            Density::Comfortable
        );
    }

    #[test]
    fn source_preview_selects_exact_homograph_and_rejects_absent_sense() {
        use retivus::source_adapter::{SourceInfo, StudyTsvV1};
        use std::io::Cursor;

        let adapter = StudyTsvV1::new(SourceInfo {
            provider_id: "fixture".into(),
            edition: "v1".into(),
            lexical_language: "ja".into(),
            definition_language: "es".into(),
            license_notice: "Original test fixture".into(),
        })
        .unwrap();
        let input = include_str!("../tests/fixtures/independent-ja-es.tsv");
        let preview =
            retivus::preview::validate_preview(&adapter, &mut Cursor::new(input)).unwrap();
        let life = SelectedSense::from_record(&preview.rows[0], "s1").unwrap();
        let live = SelectedSense::from_record(&preview.rows[1], "s1").unwrap();
        assert_eq!(life.word, live.word);
        assert_ne!(life.entry_id, live.entry_id);
        assert_eq!(life.definition, "vida");
        assert_eq!(live.definition, "vivir");
        assert!(SelectedSense::from_record(&preview.rows[0], "invalid").is_none());
        assert!(Retivus::default().selected_sense.is_none());
    }

    #[test]
    fn native_gui_keyboard_and_mouse_select_a_validated_source_sense() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("words.tsv");
        std::fs::write(
            &file,
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\t橋\tnoun\ts1\tbridge\n",
        )
        .unwrap();

        let mut harness = Harness::new_eframe(|_| Retivus::default());
        {
            let app = harness.state_mut();
            app.source_format = SourceFormat::GenericTsv;
            app.source_id = "ui-kittest-fixture".into();
            app.source_edition = "v1".into();
            app.source_language = "ja".into();
            app.definition_language = "en".into();
            app.license_notice = "Synthetic original fixture".into();
        }
        harness.run();

        let file_field = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "File");
        file_field.focus();
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
        harness.run();
        assert!(
            harness
                .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
                .is_focused(),
            "Shift+Tab must restore keyboard focus to the source-file input"
        );
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
            .type_text(&file.to_string_lossy());
        harness.run();
        assert_eq!(harness.state().source_path, file.to_string_lossy().as_ref());

        harness.get_by_label("Validate source").click();
        harness.run();
        assert_eq!(harness.state().preview.len(), 1);
        assert!(harness.state().import_status.contains("Valid records: 1"));
        harness.get_by_label("Select sense").scroll_to_me();
        harness.run();
        harness.get_by_label("Select sense").click();
        harness.run();
        assert_eq!(
            harness.state().selected_sense.as_ref().unwrap().definition,
            "bridge"
        );
        harness.set_size(egui::vec2(720.0, 650.0));
        harness.run();
        assert!(harness.state().selected_sense.is_some());
    }

    #[test]
    fn accessibility_focus_survives_scaling_density_theme_and_resizing() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let mut harness = Harness::new_eframe(|_| Retivus::default());
        for (window, pixels_per_point, theme, density) in [
            (
                egui::vec2(680.0, 560.0),
                1.0,
                Theme::Light,
                Density::Comfortable,
            ),
            (egui::vec2(720.0, 650.0), 1.5, Theme::Dark, Density::Compact),
            (
                egui::vec2(1100.0, 760.0),
                0.8,
                Theme::System,
                Density::Comfortable,
            ),
        ] {
            harness.set_size(window);
            harness.set_pixels_per_point(pixels_per_point);
            harness.state_mut().theme = theme;
            harness.state_mut().density = density;
            harness.run();

            harness
                .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
                .scroll_to_me();
            harness.run();
            harness
                .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
                .focus();
            harness.run();
            assert!(
                harness
                    .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
                    .is_focused(),
                "File input lost keyboard focus at {window:?}, scale={pixels_per_point}"
            );
            harness.get_by_label("Validate source").scroll_to_me();
            harness.run();
            assert!(
                harness
                    .get_by_role_and_label(egui::accesskit::Role::TextInput, "File")
                    .is_focused(),
                "Scrolling must not silently move keyboard focus"
            );

            // Exercise keyboard activation after a scale/theme/density switch,
            // not just accessibility-tree focus inspection. No user data exists.
            harness.get_by_label("Validate source").focus();
            harness.run();
            assert!(
                harness.get_by_label("Validate source").is_focused(),
                "Validate source button was not keyboard focusable"
            );
            harness.key_press(egui::Key::Enter);
            harness.run();
            assert!(
                harness.state().import_status.starts_with("Could not open source:"),
                "Enter did not activate the focused validation button at {window:?}, scale={pixels_per_point}"
            );
            assert!(harness.state().preview.is_empty());
        }
    }

    #[test]
    fn unknown_or_malformed_preference_follows_system() {
        let mut storage = TestStorage::default();
        for value in ["", "DARK", "broken", "3"] {
            eframe::Storage::set_string(&mut storage, THEME_STORAGE_KEY, value.into());
            assert_eq!(Retivus::restore(Some(&storage)).theme, Theme::System);
        }
        assert_eq!(Theme::System.preference(), egui::ThemePreference::System);
        assert_eq!(Theme::Light.preference(), egui::ThemePreference::Light);
        assert_eq!(Theme::Dark.preference(), egui::ThemePreference::Dark);
    }
}
