use retivus::provider;
use eframe::egui;
use std::fs::File;
use std::io::BufReader;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Theme {
    System,
    Light,
    Dark,
}
impl Theme {
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
impl eframe::App for Retivus {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Switching back to System must restore live operating-system theme tracking.
        ctx.set_theme(match self.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Retivus");
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
        Box::new(|_cc| Ok(Box::new(Retivus::default()))),
    )
}
