//! TUI Theme — Alpharius color system for ratatui.
//!
//! Loads from `themes/alpharius.json` when available, falls back to
//! compiled-in defaults. The JSON file is the source of truth — it
//! defines `vars` (base tokens) and `colors` (semantic mappings).
//!
//! To add a new theme, implement the Theme trait with different values.

use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

/// Semantic color slots for the TUI.
pub trait Theme: Send + Sync {
    // ─── Core palette ───────────────────────────────────────────────
    fn bg(&self) -> Color;
    fn card_bg(&self) -> Color;
    fn surface_bg(&self) -> Color;
    fn border(&self) -> Color;
    fn border_dim(&self) -> Color;

    // ─── Text ───────────────────────────────────────────────────────
    fn fg(&self) -> Color;
    fn muted(&self) -> Color;
    fn dim(&self) -> Color;

    // ─── Brand ──────────────────────────────────────────────────────
    fn accent(&self) -> Color;
    fn accent_muted(&self) -> Color;
    fn accent_bright(&self) -> Color;

    // ─── Signal ─────────────────────────────────────────────────────
    fn success(&self) -> Color;
    fn error(&self) -> Color;
    fn warning(&self) -> Color;
    fn caution(&self) -> Color;

    // ─── Extended (semantic tool/diff colors) ───────────────────────
    fn footer_bg(&self) -> Color { Color::Rgb(1, 3, 6) }
    fn user_msg_bg(&self) -> Color { self.card_bg() }
    fn tool_success_bg(&self) -> Color { self.card_bg() }
    fn tool_error_bg(&self) -> Color { Color::Rgb(30, 8, 16) }
    fn diff_added(&self) -> Color { self.success() }
    fn diff_removed(&self) -> Color { self.error() }
    fn diff_added_bg(&self) -> Color { Color::Rgb(4, 22, 12) }
    fn diff_removed_bg(&self) -> Color { Color::Rgb(22, 4, 4) }

    // ─── Derived styles ─────────────────────────────────────────────

    fn style_fg(&self) -> Style {
        Style::default().fg(self.fg())
    }
    fn style_muted(&self) -> Style {
        Style::default().fg(self.muted())
    }
    fn style_dim(&self) -> Style {
        Style::default().fg(self.dim())
    }
    fn style_accent(&self) -> Style {
        Style::default().fg(self.accent())
    }
    fn style_accent_bold(&self) -> Style {
        Style::default().fg(self.accent()).add_modifier(Modifier::BOLD)
    }
    fn style_success(&self) -> Style {
        Style::default().fg(self.success())
    }
    fn style_error(&self) -> Style {
        Style::default().fg(self.error())
    }
    fn style_warning(&self) -> Style {
        Style::default().fg(self.warning())
    }
    fn style_heading(&self) -> Style {
        Style::default().fg(self.accent_bright()).add_modifier(Modifier::BOLD)
    }
    fn style_user_input(&self) -> Style {
        Style::default().fg(self.fg()).add_modifier(Modifier::BOLD)
    }
    fn style_footer_bg(&self) -> Style {
        Style::default().bg(self.footer_bg())
    }
    fn style_border(&self) -> Style {
        Style::default().fg(self.border())
    }
    fn style_border_dim(&self) -> Style {
        Style::default().fg(self.border_dim())
    }
}

/// Parse a hex color string (#RRGGBB or RRGGBB) to a ratatui Color.
fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Resolve a color value — either a hex string or a reference to a var.
fn resolve_color(value: &str, vars: &HashMap<String, String>) -> Option<Color> {
    if value.starts_with('#') {
        parse_hex(value)
    } else {
        // It's a var reference
        vars.get(value).and_then(|hex| parse_hex(hex))
    }
}

/// Theme loaded from alpharius.json — parameterized, not hardcoded.
pub struct JsonTheme {
    vars: HashMap<String, Color>,
}

impl JsonTheme {
    /// Load from a JSON theme file. Returns None if loading fails.
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let vars_obj = json.get("vars")?.as_object()?;
        let mut raw_vars: HashMap<String, String> = HashMap::new();
        for (key, val) in vars_obj {
            if let Some(s) = val.as_str() {
                raw_vars.insert(key.clone(), s.to_string());
            }
        }

        // Resolve colors from the "colors" section (which references vars)
        let mut resolved: HashMap<String, Color> = HashMap::new();

        // First, resolve all vars directly
        for (key, hex) in &raw_vars {
            if let Some(color) = parse_hex(hex) {
                resolved.insert(key.clone(), color);
            }
        }

        // Then resolve semantic colors
        if let Some(colors_obj) = json.get("colors").and_then(|c| c.as_object()) {
            for (key, val) in colors_obj {
                if let Some(s) = val.as_str()
                    && let Some(color) = resolve_color(s, &raw_vars)
                {
                    resolved.insert(key.clone(), color);
                }
            }
        }

        // Also resolve export colors
        if let Some(export_obj) = json.get("export").and_then(|e| e.as_object()) {
            for (key, val) in export_obj {
                if let Some(s) = val.as_str()
                    && let Some(color) = parse_hex(s)
                {
                    resolved.insert(format!("export_{key}"), color);
                }
            }
        }

        Some(Self { vars: resolved })
    }

    fn get(&self, key: &str) -> Color {
        self.vars.get(key).copied().unwrap_or(Color::Reset)
    }
}

impl Theme for JsonTheme {
    fn bg(&self) -> Color { self.get("bg") }
    fn card_bg(&self) -> Color { self.get("cardBg") }
    fn surface_bg(&self) -> Color { self.get("surfaceBg") }
    fn border(&self) -> Color { self.get("borderColor") }
    fn border_dim(&self) -> Color { self.get("borderDim") }

    fn fg(&self) -> Color { self.get("fg") }
    fn muted(&self) -> Color { self.get("mutedFg") }
    fn dim(&self) -> Color { self.get("dimFg") }

    fn accent(&self) -> Color { self.get("primary") }
    fn accent_muted(&self) -> Color { self.get("primaryMuted") }
    fn accent_bright(&self) -> Color { self.get("primaryBright") }

    fn success(&self) -> Color { self.get("green") }
    fn error(&self) -> Color { self.get("red") }
    fn warning(&self) -> Color { self.get("orange") }
    fn caution(&self) -> Color { self.get("yellow") }

    fn footer_bg(&self) -> Color {
        self.vars.get("footerBg").copied().unwrap_or(Color::Rgb(1, 3, 6))
    }
    fn user_msg_bg(&self) -> Color { self.get("userMsgBg") }
    fn tool_success_bg(&self) -> Color {
        self.vars.get("toolSuccessBg").copied().unwrap_or_else(|| self.card_bg())
    }
    fn tool_error_bg(&self) -> Color { self.get("toolErrorBg") }
    fn diff_added(&self) -> Color { self.get("toolDiffAdded") }
    fn diff_removed(&self) -> Color { self.get("toolDiffRemoved") }
    fn diff_added_bg(&self) -> Color {
        self.vars.get("toolDiffAddedBg").copied().unwrap_or(Color::Rgb(4, 22, 12))
    }
    fn diff_removed_bg(&self) -> Color {
        self.vars.get("toolDiffRemovedBg").copied().unwrap_or(Color::Rgb(22, 4, 4))
    }
}

/// Hardcoded fallback — used when alpharius.json is not found.
pub struct Alpharius;

impl Theme for Alpharius {
    fn bg(&self) -> Color { Color::Rgb(0, 1, 3) }
    fn card_bg(&self) -> Color { Color::Rgb(2, 6, 12) }
    fn surface_bg(&self) -> Color { Color::Rgb(0, 1, 3) }
    fn border(&self) -> Color { Color::Rgb(48, 112, 140) }
    fn border_dim(&self) -> Color { Color::Rgb(32, 72, 96) }

    fn fg(&self) -> Color { Color::Rgb(196, 216, 228) }
    fn muted(&self) -> Color { Color::Rgb(96, 120, 136) }
    fn dim(&self) -> Color { Color::Rgb(64, 88, 112) }

    fn accent(&self) -> Color { Color::Rgb(42, 180, 200) }
    fn accent_muted(&self) -> Color { Color::Rgb(26, 136, 152) }
    fn accent_bright(&self) -> Color { Color::Rgb(110, 202, 216) }

    fn success(&self) -> Color { Color::Rgb(26, 184, 120) }
    fn error(&self) -> Color { Color::Rgb(224, 72, 72) }
    fn warning(&self) -> Color { Color::Rgb(200, 100, 24) }
    fn caution(&self) -> Color { Color::Rgb(120, 184, 32) }
}

/// Load the theme — try alpharius.json first, fall back to hardcoded.
pub fn default_theme() -> Box<dyn Theme> {
    // Search for alpharius.json relative to cwd
    let search_paths = [
        std::path::PathBuf::from("themes/alpharius.json"),
        std::path::PathBuf::from("../themes/alpharius.json"),
    ];

    // Also check relative to the project root via .git
    let mut project_root = std::env::current_dir().unwrap_or_default();
    for _ in 0..5 {
        if project_root.join(".git").exists() || project_root.join("themes/alpharius.json").exists() {
            let theme_path = project_root.join("themes/alpharius.json");
            if let Some(theme) = JsonTheme::load(&theme_path) {
                tracing::info!(path = %theme_path.display(), "loaded theme from JSON");
                return Box::new(theme);
            }
            break;
        }
        if !project_root.pop() { break; }
    }

    for path in &search_paths {
        if let Some(theme) = JsonTheme::load(path) {
            tracing::info!(path = %path.display(), "loaded theme from JSON");
            return Box::new(theme);
        }
    }

    tracing::debug!("using hardcoded Alpharius theme (alpharius.json not found)");
    Box::new(Alpharius)
}

/// Load theme with calibration applied.
pub fn calibrated_theme(cal: &crate::settings::CalibrationParams) -> Box<dyn Theme> {
    let base = default_theme();
    if cal.is_identity() {
        return base;
    }
    Box::new(CalibratedTheme::new(base, *cal))
}

// ─── HSL ↔ RGB conversion ──────────────────────────────────────────

/// Convert RGB (0–255 each) to HSL (h: 0–360, s: 0–1, l: 0–1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l); // achromatic
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < 1e-6 {
        let mut h = (g - b) / d;
        if g < b { h += 6.0; }
        h
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h * 60.0, s, l)
}

/// Convert HSL (h: 0–360, s: 0–1, l: 0–1) to RGB (0–255 each).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < 1e-6 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h = h / 360.0;

    let hue_to_rgb = |p: f32, q: f32, mut t: f32| -> f32 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };

    let r = (hue_to_rgb(p, q, h + 1.0 / 3.0) * 255.0).round() as u8;
    let g = (hue_to_rgb(p, q, h) * 255.0).round() as u8;
    let b = (hue_to_rgb(p, q, h - 1.0 / 3.0) * 255.0).round() as u8;
    (r, g, b)
}

/// Apply calibration transforms to a single color.
fn calibrate_color(
    color: Color,
    cal: &crate::settings::CalibrationParams,
) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let (mut h, mut s, mut l) = rgb_to_hsl(r, g, b);
            // Hue shift
            h = (h + cal.hue_shift).rem_euclid(360.0);
            // Saturation scale
            s = (s * cal.saturation).clamp(0.0, 1.0);
            // Gamma (lightness curve)
            l = l.powf(1.0 / cal.gamma).clamp(0.0, 1.0);
            let (r, g, b) = hsl_to_rgb(h, s, l);
            Color::Rgb(r, g, b)
        }
        other => other, // non-RGB colors pass through
    }
}

// ─── Calibrated theme wrapper ───────────────────────────────────────

/// Pre-computed calibrated colors. All HSL transforms are done once at
/// construction time — no per-frame recalculation.
struct CalibratedTheme {
    c_bg: Color,
    c_card_bg: Color,
    c_surface_bg: Color,
    c_border: Color,
    c_border_dim: Color,
    c_fg: Color,
    c_muted: Color,
    c_dim: Color,
    c_accent: Color,
    c_accent_muted: Color,
    c_accent_bright: Color,
    c_success: Color,
    c_error: Color,
    c_warning: Color,
    c_caution: Color,
    c_footer_bg: Color,
    c_user_msg_bg: Color,
    c_tool_success_bg: Color,
    c_tool_error_bg: Color,
    c_diff_added: Color,
    c_diff_removed: Color,
    c_diff_added_bg: Color,
    c_diff_removed_bg: Color,
}

impl CalibratedTheme {
    fn new(base: Box<dyn Theme>, cal: crate::settings::CalibrationParams) -> Self {
        let c = |color: Color| calibrate_color(color, &cal);
        Self {
            c_bg: c(base.bg()),
            c_card_bg: c(base.card_bg()),
            c_surface_bg: c(base.surface_bg()),
            c_border: c(base.border()),
            c_border_dim: c(base.border_dim()),
            c_fg: c(base.fg()),
            c_muted: c(base.muted()),
            c_dim: c(base.dim()),
            c_accent: c(base.accent()),
            c_accent_muted: c(base.accent_muted()),
            c_accent_bright: c(base.accent_bright()),
            c_success: c(base.success()),
            c_error: c(base.error()),
            c_warning: c(base.warning()),
            c_caution: c(base.caution()),
            c_footer_bg: c(base.footer_bg()),
            c_user_msg_bg: c(base.user_msg_bg()),
            c_tool_success_bg: c(base.tool_success_bg()),
            c_tool_error_bg: c(base.tool_error_bg()),
            c_diff_added: c(base.diff_added()),
            c_diff_removed: c(base.diff_removed()),
            c_diff_added_bg: c(base.diff_added_bg()),
            c_diff_removed_bg: c(base.diff_removed_bg()),
        }
    }
}

impl Theme for CalibratedTheme {
    fn bg(&self) -> Color { self.c_bg }
    fn card_bg(&self) -> Color { self.c_card_bg }
    fn surface_bg(&self) -> Color { self.c_surface_bg }
    fn border(&self) -> Color { self.c_border }
    fn border_dim(&self) -> Color { self.c_border_dim }
    fn fg(&self) -> Color { self.c_fg }
    fn muted(&self) -> Color { self.c_muted }
    fn dim(&self) -> Color { self.c_dim }
    fn accent(&self) -> Color { self.c_accent }
    fn accent_muted(&self) -> Color { self.c_accent_muted }
    fn accent_bright(&self) -> Color { self.c_accent_bright }
    fn success(&self) -> Color { self.c_success }
    fn error(&self) -> Color { self.c_error }
    fn warning(&self) -> Color { self.c_warning }
    fn caution(&self) -> Color { self.c_caution }
    fn footer_bg(&self) -> Color { self.c_footer_bg }
    fn user_msg_bg(&self) -> Color { self.c_user_msg_bg }
    fn tool_success_bg(&self) -> Color { self.c_tool_success_bg }
    fn tool_error_bg(&self) -> Color { self.c_tool_error_bg }
    fn diff_added(&self) -> Color { self.c_diff_added }
    fn diff_removed(&self) -> Color { self.c_diff_removed }
    fn diff_added_bg(&self) -> Color { self.c_diff_added_bg }
    fn diff_removed_bg(&self) -> Color { self.c_diff_removed_bg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_works() {
        assert_eq!(parse_hex("#2ab4c8"), Some(Color::Rgb(42, 180, 200)));
        assert_eq!(parse_hex("06080e"), Some(Color::Rgb(6, 8, 14)));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn alpharius_fallback_colors_are_distinct() {
        let t = Alpharius;
        assert_ne!(t.bg(), t.fg());
        assert_ne!(t.accent(), t.success());
        assert_ne!(t.error(), t.warning());
        assert_ne!(t.card_bg(), t.surface_bg());
    }

    #[test]
    fn derived_styles_have_correct_color() {
        let t = Alpharius;
        assert_eq!(t.style_accent().fg, Some(t.accent()));
    }

    #[test]
    fn hsl_rgb_round_trip() {
        // Test a few known colors
        let cases: Vec<(u8, u8, u8)> = vec![
            (255, 0, 0),     // pure red
            (0, 255, 0),     // pure green
            (0, 0, 255),     // pure blue
            (42, 180, 200),  // alpharius accent
            (0, 1, 3),       // alpharius bg (near black)
            (196, 216, 228), // alpharius fg
            (128, 128, 128), // gray
            (0, 0, 0),       // black
            (255, 255, 255), // white
        ];
        for (r, g, b) in cases {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r2, g2, b2) = hsl_to_rgb(h, s, l);
            assert!(
                (r as i16 - r2 as i16).unsigned_abs() <= 1
                    && (g as i16 - g2 as i16).unsigned_abs() <= 1
                    && (b as i16 - b2 as i16).unsigned_abs() <= 1,
                "round trip failed for ({r},{g},{b}) → HSL({h},{s},{l}) → ({r2},{g2},{b2})"
            );
        }
    }

    #[test]
    fn identity_calibration_preserves_colors() {
        let cal = crate::settings::CalibrationParams::default();
        assert!(cal.is_identity());
        let color = Color::Rgb(42, 180, 200);
        let result = calibrate_color(color, &cal);
        assert_eq!(result, color, "identity calibration should not change color");
    }

    #[test]
    fn hue_shift_rotates_color() {
        let cal = crate::settings::CalibrationParams {
            gamma: 1.0,
            saturation: 1.0,
            hue_shift: 180.0, // opposite side of color wheel
        };
        let color = Color::Rgb(255, 0, 0); // red
        let result = calibrate_color(color, &cal);
        // Red shifted 180° → cyan
        if let Color::Rgb(r, g, b) = result {
            assert!(r < 10, "red channel should be low after 180° shift: {r}");
            assert!(g > 200, "green should be high: {g}");
            assert!(b > 200, "blue should be high: {b}");
        } else {
            panic!("expected Rgb color");
        }
    }

    #[test]
    fn saturation_zero_produces_gray() {
        let cal = crate::settings::CalibrationParams {
            gamma: 1.0,
            saturation: 0.0, // fully desaturated
            hue_shift: 0.0,
        };
        let color = Color::Rgb(42, 180, 200); // teal
        let result = calibrate_color(color, &cal);
        if let Color::Rgb(r, g, b) = result {
            // All channels should be equal (gray)
            assert!(
                (r as i16 - g as i16).unsigned_abs() <= 1
                    && (g as i16 - b as i16).unsigned_abs() <= 1,
                "should be gray: ({r},{g},{b})"
            );
        }
    }

    #[test]
    fn gamma_brightens() {
        let cal = crate::settings::CalibrationParams {
            gamma: 2.0, // brighter
            saturation: 1.0,
            hue_shift: 0.0,
        };
        let color = Color::Rgb(42, 180, 200);
        let result = calibrate_color(color, &cal);
        if let Color::Rgb(r, _g, _b) = result {
            assert!(r > 42, "gamma 2.0 should brighten: original r=42, got {r}");
        }
    }

    #[test]
    fn non_rgb_colors_pass_through() {
        let cal = crate::settings::CalibrationParams {
            gamma: 2.0,
            saturation: 0.5,
            hue_shift: 90.0,
        };
        assert_eq!(calibrate_color(Color::Reset, &cal), Color::Reset);
        assert_eq!(calibrate_color(Color::Red, &cal), Color::Red);
    }

    #[test]
    fn json_theme_loads_from_file() {
        // Resolve relative to the crate manifest, not cwd
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../themes/alpharius.json");
        if path.exists() {
            let theme = JsonTheme::load(&path).expect("should load alpharius.json");
            assert_ne!(theme.bg(), Color::Reset, "bg should be loaded");
            assert_ne!(theme.accent(), Color::Reset, "accent should be loaded");
            // Verify known values from the file
            assert_eq!(theme.accent(), Color::Rgb(42, 180, 200), "primary should be #2ab4c8");
        }
    }
}
