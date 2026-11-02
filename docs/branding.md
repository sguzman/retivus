# Retivus visual identity

## Project color

![Retivus color swatch](../assets/retivus-color.svg)

**Berry-rose**, selected as a perceptually distinct project identity from the periwinkle/blue, sea-teal, and rust/oxide palettes already in the Rust project family.

| Token | OKLCH coordinate | sRGB fallback | Use |
| --- | --- | --- | --- |
| Identity | `oklch(69% 0.16 350)` | `#E26DA9` | Repo branding and project identifier |
| Light-mode accent | `oklch(48% 0.16 350)` | `#9B2B6A` | Interactive active/selection accents on light surfaces |
| Dark-mode accent | `oklch(77% 0.14 350)` | `#F68DC1` | Interactive active/selection accents on dark surfaces |

**Coordinates:** CSS OKLCH / perceptual Oklab cylindrical coordinates: lightness L, chroma C, hue H. sRGB values are rounded to 8-bit channels using the Oklab-to-linear-sRGB matrices and standard sRGB transfer curve. These are deliberately **two foreground-capable theme variants**, not a demand to apply the same lightness to both contrast regimes.

## UI contract

- The single identity hue remains 350°; only lightness/chroma adapt between light and dark modes.
- **System is the default** and follows the operating-system day/night setting; the two other explicit modes override the preference.
- Shared code lives in `src/brand.rs` and is used by the live egui application. Never scatter independent hex literals across screens.
- Selected text uses contrast-aware strokes (dark foreground over the light pink selection background; white over deep rose on light themes), while the rest of the interface retains native egui text legibility.
- Typography prefers an already-installed **Krypton Nerd Font** and optionally appends a local **Noto Sans CJK** font for Japanese glyph fallback. Native defaults remain available, and no font files are bundled or redistributed. The toolbar reports Krypton only if it actually loads.
- This brand does not copy Anki logos or visual assets. Content/provider-specific source palettes must not override Retivus's identity.

## Branding acceptance

Color tokens are committed and tested in code. The application now saves the theme value in eframe native persistence storage and restores it on startup; tests cover every preference, unknown-value fallback, and System-mode selection. The M5.1 completion criteria are now verified by cross-instance preference tests, rendering paths for all three modes, simulated OS theme input transitions in System mode, and the native GUI smoke CI. Full desktop-size visual QA remains M5.6.

## Local typography implementation (M5.2 groundwork)

Retivus now searches the user's local and system font directories for an installed **Krypton Nerd Font** at launch. It prefers a regular style, rejects symlinks, oversized files and invalid TrueType/OpenType bytes, and registers a valid local font at highest egui priority while retaining normal font fallbacks. No font assets are shipped, copied to the repository or supplied to other users. The toolbar confirms Krypton only when successfully installed. If absent, the normal default egui font remains. M5.2 was already accepted on its separate keyboard/sizing QA evidence. The newer optional Noto CJK fallback is additional multilingual QA hardening; real populated-screen glyph inspection and M6.4 acceptance remain separate and pending.
