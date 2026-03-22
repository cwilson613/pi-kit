//! Fractal status surface — a living visualization of harness state.
//!
//! Renders a Mandelbrot/Julia fractal in the dashboard sidebar where
//! visual properties encode multi-dimensional harness telemetry:
//!
//! - **Zoom depth** → context utilization (deeper = fuller)
//! - **Color palette** → cognitive mode (idle=ocean, coding=amber, design=violet)
//! - **Animation speed** → agent activity (fast during tool calls, slow during thinking)
//! - **Fractal type** → persona (Mandelbrot default, Julia parameterized by persona ID)
//! - **Iteration depth** → thinking level (off=50, low=100, medium=200, high=500)
//!
//! Uses half-block characters (▀) for 2x vertical resolution.
//! True color preferred (COLORTERM check), 256-color fallback.

use ratatui::prelude::*;
use ratatui::buffer::Buffer;

/// A fractal viewport driven by harness telemetry.
pub struct FractalWidget {
    /// Viewport center in the complex plane.
    pub center: (f64, f64),
    /// Zoom level (1.0 = full view, higher = deeper).
    pub zoom: f64,
    /// Maximum iterations (controls detail + maps to thinking level).
    pub max_iter: u32,
    /// Color palette index (maps to cognitive mode).
    pub palette: Palette,
    /// If Some, render Julia set with this c parameter. If None, Mandelbrot.
    pub julia_c: Option<(f64, f64)>,
    /// Animation time — drives slow drift of center coordinates.
    pub time: f64,
}

/// Color palette — each maps to a cognitive mode.
#[derive(Debug, Clone, Copy, Default)]
pub enum Palette {
    /// Deep blue → teal → white. Idle / waiting.
    #[default]
    Ocean,
    /// Amber → gold → white. Coding / execution.
    Amber,
    /// Violet → cyan → white. Design / exploration.
    Violet,
    /// Split complementary. Cleave / parallel work.
    Split,
    /// Desaturated, low contrast. Error / degraded.
    Muted,
}

impl Default for FractalWidget {
    fn default() -> Self {
        Self {
            // Start zoomed into the Seahorse Valley — organic tendrils,
            // not the iconic beetle silhouette
            center: (-0.745, 0.186),
            zoom: 40.0,
            max_iter: 100,
            palette: Palette::Ocean,
            julia_c: None,
            time: 0.0,
        }
    }
}

impl FractalWidget {
    /// Update from harness telemetry. Call once per tick.
    pub fn update_from_status(
        &mut self,
        context_pct: f32,
        thinking_level: &str,
        is_agent_active: bool,
        persona_id: Option<&str>,
        is_cleave_active: bool,
        dt: f64,
    ) {
        // Zoom tracks context utilization — deeper into the edge as context fills
        self.zoom = 40.0 + (context_pct as f64 / 100.0) * 200.0;

        // Iteration depth tracks thinking level
        self.max_iter = match thinking_level {
            "off" | "Off" => 50,
            "low" | "Low" | "minimal" | "Minimal" => 100,
            "medium" | "Medium" => 200,
            "high" | "High" => 500,
            _ => 100,
        };

        // Palette tracks cognitive mode
        self.palette = if is_cleave_active {
            Palette::Split
        } else if is_agent_active {
            Palette::Amber
        } else {
            Palette::Ocean
        };

        // Persona → Julia set with unique c parameter
        self.julia_c = persona_id.map(|id| {
            let hash = simple_hash(id);
            let real = (hash & 0xFFFF) as f64 / 65536.0 * 1.2 - 0.6;
            let imag = ((hash >> 16) & 0xFFFF) as f64 / 65536.0 * 1.2 - 0.6;
            (real, imag)
        });

        // Drift around the Seahorse Valley edge — visible but not frantic
        let speed = if is_agent_active { 0.15 } else { 0.03 };
        self.time += dt;
        let drift_radius = 0.003 / (self.zoom / 40.0); // tighter orbit at higher zoom
        self.center.0 = -0.745 + (self.time * speed).sin() * drift_radius;
        self.center.1 = 0.186 + (self.time * speed * 0.7).cos() * drift_radius * 1.3;
    }

    /// Render the fractal into a ratatui Buffer area using braille characters
    /// with an Ω-shaped mask and pulsing border glow.
    ///
    /// Braille gives 2×4 dots per cell = 4× the resolution of half-blocks.
    /// Each cell gets one fg color (from the fractal) against the bg.
    /// The Ω mask controls which dots are lit within each cell.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 2 {
            return;
        }

        let cell_w = area.width as usize;
        let cell_h = area.height as usize;
        // Braille: each cell is 2 dots wide × 4 dots tall
        let dot_w = cell_w * 2;
        let dot_h = cell_h * 4;

        let aspect = dot_w as f64 / dot_h as f64;
        let view_h = 2.5 / self.zoom;
        let view_w = view_h * aspect;

        // Generate the Ω mask at braille resolution
        let mask = omega_mask(dot_w, dot_h);

        // Border glow
        let glow_phase = (self.time * 0.8).sin() * 0.5 + 0.5;
        let glow_color = self.glow_color(glow_phase);

        let bg = Color::Rgb(6, 10, 18); // surface_bg

        for cy in 0..cell_h {
            for cx in 0..cell_w {
                // Each braille cell covers a 2×4 dot region
                let dx = cx * 2;
                let dy = cy * 4;

                // Determine which dots are lit and the dominant color
                let mut braille_bits: u8 = 0;
                let mut total_r: u32 = 0;
                let mut total_g: u32 = 0;
                let mut total_b: u32 = 0;
                let mut color_count: u32 = 0;
                let mut has_border = false;

                // Braille dot positions within the 2×4 grid:
                // Col 0: bits 0,1,2,6 (top to bottom)
                // Col 1: bits 3,4,5,7
                let dot_bits = [
                    [0u8, 1, 2, 6], // column 0, rows 0-3
                    [3, 4, 5, 7],   // column 1, rows 0-3
                ];

                for col in 0..2 {
                    for row in 0..4 {
                        let px = dx + col;
                        let py = dy + row;
                        if px >= dot_w || py >= dot_h { continue; }

                        let mv = mask_value(&mask, px, py, dot_w);
                        match mv {
                            MaskVal::Inside => {
                                braille_bits |= 1 << dot_bits[col][row];
                                let iter = self.compute_pixel(px, py, dot_w, dot_h, view_w, view_h);
                                if let Color::Rgb(r, g, b) = self.iter_to_color(iter) {
                                    total_r += r as u32;
                                    total_g += g as u32;
                                    total_b += b as u32;
                                    color_count += 1;
                                }
                            }
                            MaskVal::Border => {
                                braille_bits |= 1 << dot_bits[col][row];
                                has_border = true;
                            }
                            MaskVal::Outside => {}
                        }
                    }
                }

                let fg = if has_border && color_count == 0 {
                    glow_color
                } else if color_count > 0 {
                    // Average color of the fractal pixels in this cell
                    let r = (total_r / color_count) as u8;
                    let g = (total_g / color_count) as u8;
                    let b = (total_b / color_count) as u8;
                    // Blend in border glow if present
                    if has_border {
                        if let (Color::Rgb(gr, gg, gb), Color::Rgb(fr, fg, fb)) = (glow_color, Color::Rgb(r, g, b)) {
                            Color::Rgb(
                                ((fr as u16 + gr as u16) / 2) as u8,
                                ((fg as u16 + gg as u16) / 2) as u8,
                                ((fb as u16 + gb as u16) / 2) as u8,
                            )
                        } else { Color::Rgb(r, g, b) }
                    } else {
                        Color::Rgb(r, g, b)
                    }
                } else {
                    bg // no dots lit
                };

                // Braille Unicode block starts at U+2800
                let braille_char = char::from_u32(0x2800 + braille_bits as u32).unwrap_or(' ');

                if let Some(cell) = buf.cell_mut(Position::new(
                    area.x + cx as u16,
                    area.y + cy as u16,
                )) {
                    cell.set_char(braille_char);
                    cell.set_fg(fg);
                    cell.set_bg(bg);
                }
            }
        }
    }

    /// Border glow color — subdued, palette-matched pulse.
    fn glow_color(&self, phase: f64) -> Color {
        let intensity = 20.0 + phase * 25.0; // 20..45 brightness
        match self.palette {
            Palette::Ocean => Color::Rgb(
                (intensity * 0.2) as u8,
                (intensity * 0.6) as u8,
                (intensity * 0.9) as u8,
            ),
            Palette::Amber => Color::Rgb(
                (intensity * 0.9) as u8,
                (intensity * 0.5) as u8,
                (intensity * 0.1) as u8,
            ),
            Palette::Violet => Color::Rgb(
                (intensity * 0.6) as u8,
                (intensity * 0.2) as u8,
                (intensity * 0.9) as u8,
            ),
            Palette::Split => Color::Rgb(
                (intensity * 0.7) as u8,
                (intensity * 0.4) as u8,
                (intensity * 0.7) as u8,
            ),
            Palette::Muted => Color::Rgb(
                (intensity * 0.4) as u8,
                (intensity * 0.4) as u8,
                (intensity * 0.45) as u8,
            ),
        }
    }

    /// Compute iteration count for a single pixel.
    fn compute_pixel(&self, px: usize, py: usize, w: usize, h: usize, vw: f64, vh: f64) -> u32 {
        let re = self.center.0 + (px as f64 / w as f64 - 0.5) * vw;
        let im = self.center.1 + (py as f64 / h as f64 - 0.5) * vh;

        match self.julia_c {
            None => mandelbrot(re, im, self.max_iter),
            Some((cr, ci)) => julia(re, im, cr, ci, self.max_iter),
        }
    }

    /// Map iteration count to a color using the active palette.
    ///
    /// All palettes are subdued — the fractal is ambient, not a spotlight.
    /// Peak brightness ~60-80 so it glows against the dark Alpharius bg
    /// without competing with the text content below.
    fn iter_to_color(&self, iter: u32) -> Color {
        if iter >= self.max_iter {
            // Inside the set = surface bg (blends with dashboard)
            return Color::Rgb(6, 10, 18);
        }

        // Smooth coloring using log2 for gradual gradients instead of banding
        let t = (iter as f64 / self.max_iter as f64).sqrt(); // sqrt for gentler ramp

        match self.palette {
            Palette::Ocean => {
                // Deep teal → dark cyan. Alpharius ocean tones.
                let r = (t * 12.0) as u8;
                let g = (t * 36.0 + t * t * 20.0) as u8;
                let b = (t * 50.0 + t * t * 30.0) as u8;
                Color::Rgb(r, g, b)
            }
            Palette::Amber => {
                // Warm ember glow — working, thinking
                let r = (t * 65.0 + t * t * 20.0) as u8;
                let g = (t * 30.0 + t * t * 10.0) as u8;
                let b = (t * 8.0) as u8;
                Color::Rgb(r, g, b)
            }
            Palette::Violet => {
                // Cool violet — design mode
                let r = (t * 35.0 + t * t * 15.0) as u8;
                let g = (t * 12.0) as u8;
                let b = (t * 55.0 + t * t * 25.0) as u8;
                Color::Rgb(r, g, b)
            }
            Palette::Split => {
                // Alternating warm/cool bands — cleave parallel work
                if iter % 2 == 0 {
                    let r = (t * 50.0) as u8;
                    let g = (t * 25.0) as u8;
                    let b = (t * 8.0) as u8;
                    Color::Rgb(r, g, b)
                } else {
                    let r = (t * 8.0) as u8;
                    let g = (t * 30.0) as u8;
                    let b = (t * 50.0) as u8;
                    Color::Rgb(r, g, b)
                }
            }
            Palette::Muted => {
                // Near-monochrome — error/degraded state
                let v = (t * 30.0) as u8;
                Color::Rgb(v, v, v.saturating_add(5))
            }
        }
    }
}

/// Mandelbrot iteration: z = z² + c, where c = (re, im).
fn mandelbrot(cr: f64, ci: f64, max_iter: u32) -> u32 {
    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    for i in 0..max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return i;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    max_iter
}

/// Julia set iteration: z = z² + c, where c is fixed and z₀ = (re, im).
fn julia(zr0: f64, zi0: f64, cr: f64, ci: f64, max_iter: u32) -> u32 {
    let mut zr = zr0;
    let mut zi = zi0;
    for i in 0..max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return i;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    max_iter
}

/// Simple hash for persona ID → Julia c parameter.
fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

// ─── Ω mask ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum MaskVal {
    Inside,  // fractal shows through
    Border,  // glow edge
    Outside, // background
}

/// Generate an Ω (omega) shaped mask at the given pixel resolution.
/// The Ω is a circle open at the bottom with two feet.
fn omega_mask(w: usize, h: usize) -> Vec<MaskVal> {
    let mut mask = vec![MaskVal::Outside; w * h];
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0 - 1.0; // shift up slightly to make room for feet
    let radius = (w.min(h) as f64 / 2.0) - 2.0; // leave margin for border
    let border_width = 1.2;

    for py in 0..h {
        for px in 0..w {
            let dx = px as f64 - cx;
            let dy = py as f64 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Angle from center (0 = right, π/2 = down)
            let angle = dy.atan2(dx);

            // The Ω shape: a circle with a gap at the bottom (~30° each side)
            let gap_angle = 0.45; // radians from straight down
            let in_gap = angle > std::f64::consts::FRAC_PI_2 - gap_angle
                && angle < std::f64::consts::FRAC_PI_2 + gap_angle;

            // Feet: two small rectangles at the bottom of the gap
            let foot_y = cy + radius * 0.85;
            let foot_width = radius * 0.22;
            let left_foot_x = cx - radius * gap_angle.sin() - foot_width * 0.5;
            let right_foot_x = cx + radius * gap_angle.sin() - foot_width * 0.5;
            let is_foot = py as f64 >= foot_y && py as f64 <= foot_y + 2.5
                && ((px as f64 >= left_foot_x && px as f64 <= left_foot_x + foot_width * 2.0)
                    || (px as f64 >= right_foot_x && px as f64 <= right_foot_x + foot_width * 2.0));

            let idx = py * w + px;
            if is_foot {
                // Check if this foot pixel is on the edge
                let is_foot_edge = py as f64 <= foot_y + border_width
                    || py as f64 >= foot_y + 2.5 - border_width
                    || px as f64 <= left_foot_x + border_width
                    || px as f64 >= left_foot_x + foot_width * 2.0 - border_width
                    || px as f64 <= right_foot_x + border_width
                    || px as f64 >= right_foot_x + foot_width * 2.0 - border_width;
                mask[idx] = if is_foot_edge { MaskVal::Border } else { MaskVal::Inside };
            } else if !in_gap && dist <= radius {
                // Inside the circle (excluding gap)
                if dist >= radius - border_width {
                    mask[idx] = MaskVal::Border; // outer edge
                } else {
                    mask[idx] = MaskVal::Inside;
                }
            }
        }
    }
    mask
}

fn mask_value(mask: &[MaskVal], x: usize, y: usize, w: usize) -> MaskVal {
    let idx = y * w + x;
    if idx < mask.len() { mask[idx] } else { MaskVal::Outside }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandelbrot_origin_is_inside() {
        assert_eq!(mandelbrot(0.0, 0.0, 100), 100);
    }

    #[test]
    fn mandelbrot_outside_escapes() {
        assert!(mandelbrot(2.0, 2.0, 100) < 10);
    }

    #[test]
    fn julia_basic() {
        // Julia set for c = -0.7 + 0.27i — known to produce a connected set
        assert!(julia(0.0, 0.0, -0.7, 0.27, 100) > 50);
    }

    #[test]
    fn simple_hash_deterministic() {
        let h1 = simple_hash("systems-engineer");
        let h2 = simple_hash("systems-engineer");
        assert_eq!(h1, h2);
        assert_ne!(simple_hash("tutor"), simple_hash("systems-engineer"));
    }

    #[test]
    fn palette_colors_in_range() {
        let widget = FractalWidget::default();
        for i in 0..100 {
            let color = widget.iter_to_color(i);
            if let Color::Rgb(r, g, b) = color {
                // Just verify no panic — RGB values are always valid u8
                let _ = (r, g, b);
            }
        }
    }

    #[test]
    fn render_to_buffer() {
        let widget = FractalWidget::default();
        let area = Rect::new(0, 0, 20, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Verify cells were written (not all spaces)
        let mut non_space = 0;
        for y in 0..area.height {
            for x in 0..area.width {
                let sym = buf.cell(Position::new(x, y)).unwrap().symbol().chars().next().unwrap_or(' ');
                // Braille block is U+2800..U+28FF
                if ('\u{2800}'..='\u{28FF}').contains(&sym) {
                    non_space += 1;
                }
            }
        }
        assert!(non_space > 0, "should render braille characters");
    }

    #[test]
    fn render_tiny_area_does_not_panic() {
        let widget = FractalWidget::default();
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf); // should not panic even if too small
    }

    #[test]
    fn update_from_status_changes_state() {
        let mut widget = FractalWidget::default();
        widget.update_from_status(50.0, "high", true, Some("test-persona"), false, 0.016);
        assert!(widget.zoom > 1.0, "zoom should increase with context usage");
        assert_eq!(widget.max_iter, 500, "high thinking = 500 iterations");
        assert!(matches!(widget.palette, Palette::Amber), "active agent = amber");
        assert!(widget.julia_c.is_some(), "persona should enable Julia set");
    }

    #[test]
    fn update_cleave_activates_split_palette() {
        let mut widget = FractalWidget::default();
        widget.update_from_status(30.0, "medium", true, None, true, 0.016);
        assert!(matches!(widget.palette, Palette::Split));
    }
}
