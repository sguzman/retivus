use eframe::egui;
use retivus::{brand, source_adapter, typography};
use std::fs::File;
use std::io::BufReader;

#[cfg(feature = "anki-backend")]
mod study_ui;
#[cfg(target_os = "linux")]
mod linux_theme;

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
const HIGH_CONTRAST_STORAGE_KEY: &str = "retivus.high-contrast.v1";
const REDUCED_MOTION_STORAGE_KEY: &str = "retivus.reduced-motion.v1";

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
    #[cfg(target_os = "linux")]
    linux_theme: linux_theme::DesktopTheme,
    ui_scale: f32,
    pending_scale_commit: bool,
    density: Density,
    high_contrast: bool,
    reduced_motion: bool,
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
            #[cfg(target_os = "linux")]
            linux_theme: linux_theme::DesktopTheme::default(),
            ui_scale: 1.0,
            pending_scale_commit: false,
            density: Density::Comfortable,
            high_contrast: false,
            reduced_motion: false,
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

    // Applying zoom while dragging this slider changes its screen coordinates
    // beneath the pointer. Keep the chosen value visible, but resize the UI
    // only after the pointer is released (keyboard changes apply immediately).
    fn effective_theme(&self) -> egui::ThemePreference {
        #[cfg(target_os = "linux")]
        if self.theme == Theme::System {
            if let Some(theme) = self.linux_theme.current() {
                return match theme {
                    egui::Theme::Light => egui::ThemePreference::Light,
                    egui::Theme::Dark => egui::ThemePreference::Dark,
                };
            }
        }
        self.theme.preference()
    }

    fn apply_pending_ui_scale(&mut self, ctx: &egui::Context) {
        if self.pending_scale_commit && !ctx.input(|input| input.pointer.any_down()) {
            ctx.set_zoom_factor(self.ui_scale);
            self.pending_scale_commit = false;
        }
    }

    fn restore(storage: Option<&dyn eframe::Storage>) -> Self {
        let theme = storage.and_then(|value| value.get_string(THEME_STORAGE_KEY));
        let scale = storage.and_then(|value| value.get_string(SCALE_STORAGE_KEY));
        let density = storage.and_then(|value| value.get_string(DENSITY_STORAGE_KEY));
        let high_contrast = storage.and_then(|value| value.get_string(HIGH_CONTRAST_STORAGE_KEY));
        let reduced_motion = storage.and_then(|value| value.get_string(REDUCED_MOTION_STORAGE_KEY));
        Self {
            theme: Theme::from_stored(theme.as_deref()),
            ui_scale: parse_scale(scale.as_deref()),
            density: Density::from_stored(density.as_deref()),
            high_contrast: high_contrast.as_deref() == Some("true"),
            reduced_motion: reduced_motion.as_deref() == Some("true"),
            ..Self::default()
        }
    }
}

fn apply_accessibility_style(
    ctx: &egui::Context,
    density: Density,
    high_contrast: bool,
    reduced_motion: bool,
) {
    let dark = ctx.theme() == egui::Theme::Dark;
    ctx.style_mut(|style| {
        style.spacing.item_spacing = density.spacing();
        style.animation_time = if reduced_motion {
            0.0
        } else {
            egui::Style::default().animation_time
        };
        style.scroll_animation = if reduced_motion {
            egui::style::ScrollAnimation::none()
        } else {
            egui::style::ScrollAnimation::default()
        };
        // Reset every changed field when contrast is disabled. That also
        // preserves live System theme switching without stale overrides.
        let defaults = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        style.visuals.override_text_color = None;
        style.visuals.weak_text_color = defaults.weak_text_color;
        style.visuals.panel_fill = defaults.panel_fill;
        style.visuals.window_fill = defaults.window_fill;
        style.visuals.extreme_bg_color = defaults.extreme_bg_color;
        if high_contrast {
            let foreground = if dark {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            };
            let background = if dark {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            };
            style.visuals.override_text_color = Some(foreground);
            style.visuals.weak_text_color = Some(foreground);
            style.visuals.panel_fill = background;
            style.visuals.window_fill = background;
            style.visuals.extreme_bg_color = background;
        }
    });
}

impl eframe::App for Retivus {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(THEME_STORAGE_KEY, self.theme.stored().into());
        storage.set_string(SCALE_STORAGE_KEY, self.ui_scale.to_string());
        storage.set_string(DENSITY_STORAGE_KEY, self.density.stored().into());
        storage.set_string(HIGH_CONTRAST_STORAGE_KEY, self.high_contrast.to_string());
        storage.set_string(REDUCED_MOTION_STORAGE_KEY, self.reduced_motion.to_string());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Switching back to System must restore live operating-system theme tracking.
        ctx.set_theme(self.effective_theme());
        apply_accessibility_style(ctx, self.density, self.high_contrast, self.reduced_motion);
        let mut narrow_toolbar = false;
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            // The headless Sway renderer reserves but doesn't paint a wrapped
            // second row of this top panel at 720px. Keep the panel to one
            // row, and render narrow accessibility controls below in the
            // central panel, where their actual pixels are visible.
            narrow_toolbar = ui.available_width() < 860.0;
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
                    self.pending_scale_commit = true;
                }
                if !narrow_toolbar {
                    ui.checkbox(&mut self.high_contrast, "High contrast")
                        .on_hover_text(
                            "Use high-contrast text and backgrounds in either color scheme.",
                        );
                    ui.checkbox(&mut self.reduced_motion, "Reduce motion")
                        .on_hover_text("Disable standard widget transitions in Retivus.");
                }
            });
        });
        self.apply_pending_ui_scale(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            if narrow_toolbar {
                // Separate from the TopBottomPanel clipping scope. This is
                // always visible above the scrollable learner workspace.
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.high_contrast, "High contrast")
                        .on_hover_text(
                            "Use high-contrast text and backgrounds in either color scheme.",
                        );
                    ui.checkbox(&mut self.reduced_motion, "Reduce motion")
                        .on_hover_text("Disable standard widget transitions in Retivus.");
                });
                ui.separator();
            }
            ui.heading("Learn from any source.");
            ui.label("Anki ecosystem compatibility and versioned external data providers.");
            ui.separator();
            #[cfg(not(feature = "anki-backend"))]
            ui.weak("Source preview only. For local decks and studying, relaunch with --features anki-backend --bin retivus.");
            ui.weak("Keyboard: Tab and Shift+Tab move between controls; Enter or Space activates the focused control.");
            ui.label("First data source: Mexilex · es-MX → en");
            #[cfg(not(feature = "anki-backend"))]
            ui.label("The Anki Rust engine can run locally; no AnkiWeb account or synchronization is needed.");
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
                // Keep the live-region node identity stable when its text changes,
                // so AT-SPI consumers never chase a newly allocated label node.
                let import_status = ui
                    .push_id("import-status-live-region", |ui| ui.label(&self.import_status))
                    .inner;
                ui.ctx().accesskit_node_builder(import_status.id, |node| {
                    // A live property on a generic label did not trigger a
                    // spoken announcement in real Orca/AT-SPI testing. Expose
                    // this changing result as a semantic, atomic status node.
                    node.set_role(egui::accesskit::Role::Status);
                    // Egui labels expose their text as a value; AT-SPI live
                    // announcements are triggered by accessible NAME changes.
                    node.set_label(self.import_status.clone());
                    node.set_live(egui::accesskit::Live::Polite);
                    node.set_live_atomic();
                });
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
        eframe::NativeOptions {
            // The initial eframe X11 viewport otherwise stays unmapped under
            // clean minimal desktops until an unrelated MapNotify event.
            // Reveal it explicitly so startup never depends on a compositor
            // or an external window-mapping command.
            viewport: egui::ViewportBuilder::default().with_visible(true),
            ..Default::default()
        },
        Box::new(|cc| {
            brand::apply_to_context(&cc.egui_ctx);
            let mut app = Retivus::restore(cc.storage);
            #[cfg(target_os = "linux")]
            app.linux_theme.start(cc.egui_ctx.clone());
            app.krypton_font_loaded = typography::install_local_krypton(&cc.egui_ctx);
            #[cfg(all(debug_assertions, feature = "anki-backend"))]
            if std::env::var("RETIVUS_VISUAL_QA").ok().as_deref() == Some("isolated-2026") {
                let scene = std::env::var("RETIVUS_QA_SCENE")
                    .expect("RETIVUS_QA_SCENE must identify review or conflict");
                app.study
                    .seed_visual_qa(&scene)
                    .expect("Disposable native visual QA fixture must initialize");
                app.theme = if scene == "review" {
                    Theme::Light
                } else {
                    Theme::Dark
                };
                app.high_contrast = true;
                app.reduced_motion = true;
                eprintln!("Retivus disposable visual QA scene initialized: {scene}");
            }
            cc.egui_ctx.set_zoom_factor(app.ui_scale);
            // eframe's native GL backend creates the root window hidden until
            // its first painted frame, overriding NativeOptions.visible.
            // Under minimal X11 sessions, no initial repaint may arrive:
            // explicitly queue a visibility command and first redraw.
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
            cc.egui_ctx.request_repaint();
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

    #[cfg(target_os = "linux")]
    #[test]
    fn system_mode_prefers_desktop_portal_and_explicit_modes_override_it() {
        let mut app = Retivus::default();
        assert_eq!(app.effective_theme(), egui::ThemePreference::System);
        app.linux_theme.set_for_test(egui::Theme::Light);
        assert_eq!(app.effective_theme(), egui::ThemePreference::Light);
        app.linux_theme.set_for_test(egui::Theme::Dark);
        assert_eq!(app.effective_theme(), egui::ThemePreference::Dark);
        app.theme = Theme::Light;
        assert_eq!(app.effective_theme(), egui::ThemePreference::Light);
        app.theme = Theme::Dark;
        assert_eq!(app.effective_theme(), egui::ThemePreference::Dark);
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
    fn dragging_scale_does_not_move_ui_until_pointer_release() {
        let ctx = egui::Context::default();
        let mut app = Retivus {
            ui_scale: 1.4,
            pending_scale_commit: true,
            ..Retivus::default()
        };
        let pos = egui::pos2(30.0, 10.0);
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(pos), pointer(true)],
                ..egui::RawInput::default()
            },
            |ctx| {
                assert!(ctx.input(|input| input.pointer.any_down()));
                app.apply_pending_ui_scale(ctx);
                assert!(app.pending_scale_commit);
                assert_eq!(ctx.zoom_factor(), 1.0);
            },
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![pointer(false)],
                ..egui::RawInput::default()
            },
            |ctx| {
                assert!(!ctx.input(|input| input.pointer.any_down()));
                app.apply_pending_ui_scale(ctx);
                assert!(!app.pending_scale_commit);
            },
        );
        // egui applies zoom at the start of the next pass.
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!((ctx.zoom_factor() - 1.4).abs() < 0.001);
        });
    }

    #[test]
    fn high_contrast_and_reduced_motion_persist_and_reset() {
        let mut storage = TestStorage::default();
        let mut app = Retivus {
            high_contrast: true,
            reduced_motion: true,
            ..Retivus::default()
        };
        eframe::App::save(&mut app, &mut storage);
        let restored = Retivus::restore(Some(&storage));
        assert!(restored.high_contrast);
        assert!(restored.reduced_motion);
        assert!(!Retivus::default().high_contrast);
        assert!(!Retivus::default().reduced_motion);

        let ctx = egui::Context::default();
        for (theme, fg, bg) in [
            (
                egui::Theme::Dark,
                egui::Color32::WHITE,
                egui::Color32::BLACK,
            ),
            (
                egui::Theme::Light,
                egui::Color32::BLACK,
                egui::Color32::WHITE,
            ),
        ] {
            ctx.set_theme(if theme == egui::Theme::Dark {
                egui::ThemePreference::Dark
            } else {
                egui::ThemePreference::Light
            });
            apply_accessibility_style(&ctx, Density::Comfortable, true, true);
            let style = ctx.style();
            assert_eq!(style.visuals.override_text_color, Some(fg));
            assert_eq!(style.visuals.panel_fill, bg);
            assert_eq!(style.animation_time, 0.0);
            assert_eq!(style.scroll_animation, egui::style::ScrollAnimation::none());
            apply_accessibility_style(&ctx, Density::Comfortable, false, false);
            let reset = ctx.style();
            assert_eq!(reset.visuals.override_text_color, None);
            assert_eq!(reset.animation_time, egui::Style::default().animation_time);
            assert_eq!(
                reset.scroll_animation,
                egui::style::ScrollAnimation::default()
            );
            assert_eq!(
                reset.visuals.panel_fill,
                if theme == egui::Theme::Dark {
                    egui::Visuals::dark().panel_fill
                } else {
                    egui::Visuals::light().panel_fill
                }
            );
        }
        eframe::Storage::set_string(&mut storage, HIGH_CONTRAST_STORAGE_KEY, "invalid".into());
        eframe::Storage::set_string(&mut storage, REDUCED_MOTION_STORAGE_KEY, "invalid".into());
        let invalid = Retivus::restore(Some(&storage));
        assert!(!invalid.high_contrast);
        assert!(!invalid.reduced_motion);
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

        // The full learner UI may place validation outside the viewport.
        // Scroll to the real button before the mouse activation.
        harness.get_by_label("Validate source").scroll_to_me();
        harness.run();
        harness.get_by_label("Validate source").click();
        harness.run();
        assert_eq!(harness.state().preview.len(), 1);
        assert!(harness.state().import_status.contains("Valid records: 1"));
        let status_name = harness.state().import_status.clone();
        // Actual AccessKit role+name, not only the visual label text:
        // AT-SPI requires this changing name for a live announcement.
        let _ = harness.get_by_role_and_label(egui::accesskit::Role::Status, &status_name);
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
    fn accessibility_toolbar_settings_remain_visible_and_usable_on_narrow_viewports() {
        use egui_kittest::{kittest::Queryable as _, Harness};

        let mut harness = Harness::new_eframe(|_| Retivus::default());
        for viewport in [egui::vec2(720.0, 650.0), egui::vec2(1080.0, 800.0)] {
            harness.set_size(viewport);
            harness.state_mut().high_contrast = false;
            harness.state_mut().reduced_motion = false;
            harness.run();

            for label in ["High contrast", "Reduce motion"] {
                let widget = harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, label);
                let rect = widget.rect();
                assert!(
                    rect.min.x >= -1.0
                        && rect.max.x <= viewport.x + 1.0
                        && rect.min.y >= -1.0
                        && rect.max.y <= viewport.y.min(95.0),
                    "{label} must fit within the visible toolbar at {viewport:?}: {rect:?}"
                );
            }

            harness.get_by_label("High contrast").click();
            harness.run();
            assert!(
                harness.state().high_contrast,
                "narrow-window contrast control must be clickable"
            );
            harness.get_by_label("Reduce motion").click();
            harness.run();
            assert!(
                harness.state().reduced_motion,
                "narrow-window motion control must be clickable"
            );
        }
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
            harness.state_mut().high_contrast = pixels_per_point >= 1.0;
            harness.state_mut().reduced_motion = pixels_per_point >= 1.0;
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
