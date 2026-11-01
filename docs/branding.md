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
- Typography follows the usual compact, legible approach: prefer local **Krypton Nerd Font** if available in a later dedicated font-loading feature; fallback to native defaults without bundling or sharing font files. Do not falsely claim the font is already loaded.
- This brand does not copy Anki logos or visual assets. Content/provider-specific source palettes must not override Retivus's identity.

## Branding acceptance

Color tokens are committed and tested in code. The application now saves the theme value in eframe native persistence storage and restores it on startup; tests cover every preference, unknown-value fallback, and System-mode selection. Final M5.1 verification still includes explicit light/dark rendering and operating-system appearance switching QA.
