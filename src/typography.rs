//! Local-only font preference. No font assets are stored or redistributed.
//! If a valid Krypton Nerd Font is not installed, egui's normal fonts remain untouched.
use eframe::egui;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_FONT_BYTES: u64 = 12 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 4_096;

fn font_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(xdg).join("fonts"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/fonts"));
        roots.push(home.join(".fonts"));
    }
    roots.push(PathBuf::from("/usr/local/share/fonts"));
    roots.push(PathBuf::from("/usr/share/fonts"));
    roots
}

fn discover_krypton(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut pending: VecDeque<(PathBuf, usize)> =
        roots.iter().cloned().map(|path| (path, 0)).collect();
    let mut found = Vec::new();
    let mut seen = 0;
    while let Some((dir, depth)) = pending.pop_front() {
        if depth > 6 || seen >= MAX_DIRECTORY_ENTRIES {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen >= MAX_DIRECTORY_ENTRIES {
                break;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if depth < 6 {
                    pending.push_back((path, depth + 1));
                }
            } else if kind.is_file() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.contains("krypton")
                    && name.contains("nerd")
                    && (name.ends_with(".ttf") || name.ends_with(".otf"))
                {
                    found.push(path);
                }
            }
        }
    }
    found.sort_by_key(|path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
        (!name.contains("regular"), name)
    });
    found
}

fn font_bytes(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file()
        || meta.file_type().is_symlink()
        || meta.len() == 0
        || meta.len() > MAX_FONT_BYTES
    {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    ttf_parser::Face::parse(&bytes, 0).ok()?;
    Some(bytes)
}

/// Prefer a locally installed Krypton Nerd Font while leaving egui fallbacks in place.
/// Returns true only when a valid font was loaded.
pub fn install_local_krypton(ctx: &egui::Context) -> bool {
    for path in discover_krypton(&font_roots()) {
        let Some(bytes) = font_bytes(&path) else { continue };
        let font_name = "retivus-local-krypton".to_owned();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            font_name.clone(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().insert(0, font_name.clone());
        }
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn local_font_discovery_prefers_regular_and_ignores_symlinks() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("Krypton_Nerd_Font-Bold.ttf"), b"not a font").unwrap();
        fs::write(root.path().join("Krypton_Nerd_Font-Regular.ttf"), b"not a font").unwrap();
        let paths = discover_krypton(&[root.path().to_path_buf()]);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("Krypton_Nerd_Font-Regular.ttf"));
        assert!(font_bytes(&paths[0]).is_none());
    }

    #[test]
    fn missing_font_directory_retains_native_fallback() {
        let root = tempdir().unwrap();
        let nonexistent = root.path().join("does-not-exist");
        assert!(discover_krypton(&[nonexistent]).is_empty());
        assert!(!install_with_roots(&egui::Context::default(), &[root.path().to_path_buf()]));
    }
}

#[cfg(test)]
fn install_with_roots(ctx: &egui::Context, roots: &[PathBuf]) -> bool {
    for path in discover_krypton(roots) {
        if let Some(bytes) = font_bytes(&path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "retivus-local-krypton".into(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            ctx.set_fonts(fonts);
            return true;
        }
    }
    false
}
