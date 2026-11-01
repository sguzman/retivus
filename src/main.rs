mod provider;
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Theme { System, Light, Dark }
impl Theme {
    fn name(self) -> &'static str { match self { Self::System => "System", Self::Light => "Light", Self::Dark => "Dark" } }
}
struct Retivus { theme: Theme }
impl Default for Retivus {
    fn default() -> Self { Self { theme: Theme::System } }
}
impl eframe::App for Retivus {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.theme {
            Theme::System => ctx.set_visuals(egui::Visuals::default()),
            Theme::Light => ctx.set_visuals(egui::Visuals::light()),
            Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
        }
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
