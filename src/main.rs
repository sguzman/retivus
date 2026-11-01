use eframe::egui;
use retivus::{brand, provider};
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    System,
    Light,
    Dark,
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
}
impl Default for Retivus {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            source_path: String::new(),
            import_status: "No source checked".into(),
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
            ui.heading("Validate a local source file");
            ui.label("Mexilex v1 study JSONL · read-only preview, no cards created");
            ui.horizontal(|ui| {
                ui.label("Path");
                ui.text_edit_singleline(&mut self.source_path);
                if ui.button("Validate").clicked() {
                    self.import_status = match File::open(&self.source_path) {
                        Ok(file) => match provider::visit_mexilex(BufReader::new(file), |_| {}) {
                            Ok(count) => {
                                format!("Valid records: {count} · no learner data changed")
                            }
                            Err(error) => format!("Validation failed: {error}"),
                        },
                        Err(error) => format!("Could not open source: {error}"),
                    };
                }
            });
            ui.label(&self.import_status);
        });
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Retivus",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            brand::apply_to_context(&cc.egui_ctx);
            Ok(Box::new(Retivus::restore(cc.storage)))
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

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
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
    fn unknown_or_malformed_preference_follows_system() {
        let mut storage = TestStorage::default();
        for value in ["", "DARK", "broken", "3"] {
            storage.set_string(THEME_STORAGE_KEY, value.into());
            assert_eq!(Retivus::restore(Some(&storage)).theme, Theme::System);
        }
        assert_eq!(Theme::System.preference(), egui::ThemePreference::System);
        assert_eq!(Theme::Light.preference(), egui::ThemePreference::Light);
        assert_eq!(Theme::Dark.preference(), egui::ThemePreference::Dark);
    }
}
