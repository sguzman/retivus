use eframe::egui;
use retivus::{brand, source_adapter, typography};
use std::fs::File;
use std::io::BufReader;

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

const THEME_STORAGE_KEY: &str = "retivus.theme.v1";
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
    source_path: String,
    import_status: String,
    source_format: SourceFormat,
    source_id: String,
    source_edition: String,
    source_language: String,
    definition_language: String,
    license_notice: String,
    preview: Vec<source_adapter::SourceRecord>,
    krypton_font_loaded: bool,
}
impl Default for Retivus {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            source_path: String::new(),
            import_status: "No source checked".into(),
            source_format: SourceFormat::Mexilex,
            source_id: String::new(),
            source_edition: String::new(),
            source_language: String::new(),
            definition_language: String::new(),
            license_notice: String::new(),
            preview: Vec::new(),
            krypton_font_loaded: false,
        }
    }
}
impl Retivus {
    fn restore(storage: Option<&dyn eframe::Storage>) -> Self {
        let value = storage.and_then(|storage| storage.get_string(THEME_STORAGE_KEY));
        Self {
            theme: Theme::from_stored(value.as_deref()),
            ..Self::default()
        }
    }
}

impl eframe::App for Retivus {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(THEME_STORAGE_KEY, self.theme.stored().into());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Switching back to System must restore live operating-system theme tracking.
        ctx.set_theme(self.theme.preference());
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Retivus")
                        .heading()
                        .color(brand::accent(ctx.theme())),
                );
                ui.separator();
                if self.krypton_font_loaded {
                    ui.weak("Krypton Nerd Font");
                }
                ui.label("Appearance");
                egui::ComboBox::from_id_salt("appearance")
                    .selected_text(self.theme.name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.theme, Theme::System, "System");
                        ui.selectable_value(&mut self.theme, Theme::Light, "Light");
                        ui.selectable_value(&mut self.theme, Theme::Dark, "Dark");
                    });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Learn from any source.");
            ui.label("Anki ecosystem compatibility and versioned external data providers.");
            ui.separator();
            ui.weak("Engineering foundation: collection, import and review features are not implemented yet.");
            ui.label("First data source: Mexilex · es-MX → en");
            ui.label("Planned backend: Anki Rust engine feasibility gate (not yet integrated).");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Validate a local source file");
                ui.weak("Read-only validation · never creates cards or changes review history");
                ui.horizontal(|ui| {
                    ui.label("Format");
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
                        });
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
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut self.source_path).desired_width(300.0));
                });
                if ui.button("Validate source").clicked() {
                    self.preview.clear();
                    let adapter: Result<Box<dyn source_adapter::SourceAdapter>, String> =
                        match self.source_format {
                            SourceFormat::Mexilex => {
                                Ok(Box::new(source_adapter::MexilexV1::default()))
                            }
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
                        };
                    self.import_status = match adapter {
                        Err(error) => format!("Invalid source descriptor: {error}"),
                        Ok(adapter) => match File::open(&self.source_path) {
                            Err(error) => format!("Could not open source: {error}"),
                            Ok(file) => {
                                let mut reader = BufReader::new(file);
                                match retivus::preview::validate_preview(adapter.as_ref(), &mut reader) {
                                    Ok(result) => {
                                        self.preview = result.rows;
                                        format!("Valid records: {} · no learner data changed", result.validated_count)
                                    }
                                    Err(error) => format!("Validation failed: {error}"),
                                }
                            }
                        },
                    };
                }
                ui.label(&self.import_status);
                if !self.preview.is_empty() {
                    ui.separator();
                    ui.heading("Vocabulary preview");
                    ui.weak("First 12 validated records · read-only · no cards created");
                    for record in &self.preview {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&record.word);
                                ui.weak(&record.part_of_speech);
                            });
                            for sense in record.senses.iter().take(3) {
                                ui.label(&sense.definition);
                            }
                            if record.senses.len() > 3 {
                                ui.weak(format!("{} additional meanings", record.senses.len() - 3));
                            }
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
