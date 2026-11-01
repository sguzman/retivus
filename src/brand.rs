//! The Retivus project identity, expressed in perceptual OKLCH / OKLab coordinates.
//! Hue is kept at 350° across modes while lightness adapts for contrast.
//! Conversion to sRGB uses standard Oklab matrices; see docs/branding.md.
use eframe::egui::{self, Color32, Stroke, Theme};

/// Canonical identity: oklch(69% 0.16 350), approximately #E26DA9.
pub const IDENTITY: Color32 = Color32::from_rgb(0xE2, 0x6D, 0xA9);
/// Light theme: oklch(48% 0.16 350), approximately #9B2B6A.
pub const LIGHT_ACCENT: Color32 = Color32::from_rgb(0x9B, 0x2B, 0x6A);
/// Dark theme: oklch(77% 0.14 350), approximately #F68DC1.
pub const DARK_ACCENT: Color32 = Color32::from_rgb(0xF6, 0x8D, 0xC1);

pub fn accent(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => DARK_ACCENT,
        Theme::Light => LIGHT_ACCENT,
    }
}

pub fn apply_to_context(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        let accent = accent(theme);
        let foreground = match theme {
            Theme::Light => Color32::WHITE,
            Theme::Dark => Color32::BLACK,
        };
        ctx.style_mut_of(theme, |style| {
            style.visuals.hyperlink_color = accent;
            style.visuals.selection.bg_fill = accent;
            style.visuals.selection.stroke = Stroke::new(1.0, foreground);
            style.visuals.widgets.hovered.bg_stroke.color = accent;
            style.visuals.widgets.active.bg_fill = accent;
            style.visuals.widgets.active.fg_stroke.color = foreground;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_are_distinct_across_themes() {
        assert_eq!(IDENTITY, Color32::from_rgb(226, 109, 169));
        assert_eq!(accent(Theme::Light), LIGHT_ACCENT);
        assert_eq!(accent(Theme::Dark), DARK_ACCENT);
        assert_ne!(LIGHT_ACCENT, DARK_ACCENT);
    }

    #[test]
    fn both_egui_styles_receive_accents_without_overriding_system_preference() {
        let ctx = egui::Context::default();
        ctx.set_theme(egui::ThemePreference::System);
        apply_to_context(&ctx);
        assert_eq!(
            ctx.style_of(Theme::Light).visuals.selection.bg_fill,
            LIGHT_ACCENT
        );
        assert_eq!(
            ctx.style_of(Theme::Dark).visuals.selection.bg_fill,
            DARK_ACCENT
        );
    }
}
