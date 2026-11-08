//! Read Linux desktop appearance without blocking egui's rendering thread.
//! Winit frequently reports no system theme on Wayland. Use the standard XDG
//! Settings portal, with GNOME's GSettings key as a fallback when unavailable.
use eframe::egui::{Context, Theme};
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

const UNKNOWN: u8 = 0;
const LIGHT: u8 = 1;
const DARK: u8 = 2;

#[derive(Clone, Default)]
pub(super) struct DesktopTheme {
    current: Arc<AtomicU8>,
}

impl DesktopTheme {
    pub(super) fn current(&self) -> Option<Theme> {
        match self.current.load(Ordering::Relaxed) {
            LIGHT => Some(Theme::Light),
            DARK => Some(Theme::Dark),
            _ => None,
        }
    }

    pub(super) fn start(&self, ctx: Context) {
        let current = Arc::downgrade(&self.current);
        std::thread::spawn(move || loop {
            let Some(shared) = current.upgrade() else {
                return;
            };
            let detected = match read_desktop_theme() {
                Some(Theme::Light) => LIGHT,
                Some(Theme::Dark) => DARK,
                None => UNKNOWN,
            };
            if shared.swap(detected, Ordering::Relaxed) != detected {
                ctx.request_repaint();
            }
            drop(shared);
            std::thread::sleep(Duration::from_secs(3));
        });
    }

    #[cfg(test)]
    pub(super) fn set_for_test(&self, theme: Theme) {
        self.current.store(
            match theme {
                Theme::Light => LIGHT,
                Theme::Dark => DARK,
            },
            Ordering::Relaxed,
        );
    }
}

/// The portal's uint32 color-scheme: 1 = dark, 2 = light, 0 = no preference.
/// Treat no preference as light, as desktop apps normally do.
fn parse_portal_theme(output: &str) -> Option<Theme> {
    let suffix = output.split_once("uint32 ")?.1;
    let digits: String = suffix.chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u32>().ok()? {
        1 => Some(Theme::Dark),
        0 | 2 => Some(Theme::Light),
        _ => None,
    }
}

fn parse_gsettings_theme(output: &str) -> Option<Theme> {
    match output.trim().trim_matches('\'') {
        "prefer-dark" => Some(Theme::Dark),
        "prefer-light" | "default" => Some(Theme::Light),
        _ => None,
    }
}

fn read_desktop_theme() -> Option<Theme> {
    let portal = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--timeout=1",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.ReadOne",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output();
    if let Ok(output) = portal {
        if output.status.success() {
            if let Some(theme) = parse_portal_theme(&String::from_utf8_lossy(&output.stdout)) {
                return Some(theme);
            }
        }
    }
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_gsettings_theme(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_values_map_to_correct_dark_and_light_modes() {
        assert_eq!(parse_portal_theme("(<uint32 1>,)"), Some(Theme::Dark));
        assert_eq!(parse_portal_theme("(<uint32 2>,)"), Some(Theme::Light));
        assert_eq!(parse_portal_theme("(<uint32 0>,)"), Some(Theme::Light));
        assert_eq!(parse_portal_theme("(<uint32 99>,)"), None);
        assert_eq!(parse_portal_theme("not a portal answer"), None);
    }

    #[test]
    fn gsettings_value_fallback_matches_desktop_preference() {
        assert_eq!(parse_gsettings_theme("'prefer-dark'"), Some(Theme::Dark));
        assert_eq!(parse_gsettings_theme("'prefer-light'"), Some(Theme::Light));
        assert_eq!(parse_gsettings_theme("'default'"), Some(Theme::Light));
        assert_eq!(parse_gsettings_theme("'unknown'"), None);
    }
}
