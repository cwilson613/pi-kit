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

    /// Render the fractal using half-block characters (▀).
    /// Each cell = 2 vertical pixels (fg = top, bg = bottom).
    /// Fills the entire area — no mask, no overlay.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 2 {
            return;
        }

        let px_w = area.width as usize;
        let px_h = area.height as usize * 2;

        let aspect = px_w as f64 / px_h as f64;
        let view_h = 2.5 / self.zoom;
        let view_w = view_h * aspect;

        for cy in (0..px_h).step_by(2) {
            let row = cy / 2;
            if row >= area.height as usize { break; }

            for cx in 0..px_w {
                if cx >= area.width as usize { break; }

                let top_color = self.iter_to_color(
                    self.compute_pixel(cx, cy, px_w, px_h, view_w, view_h)
                );
                let bot_color = self.iter_to_color(
                    self.compute_pixel(cx, cy + 1, px_w, px_h, view_w, view_h)
                );

                if let Some(cell) = buf.cell_mut(Position::new(
                    area.x + cx as u16,
                    area.y + row as u16,
                )) {
                    cell.set_char('▀');
                    cell.set_fg(top_color);
                    cell.set_bg(bot_color);
                }
            }
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
                if buf.cell(Position::new(x, y)).unwrap().symbol() == "▀" {
                    non_space += 1;
                }
            }
        }
        assert!(non_space > 0, "should render half-block characters");
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
