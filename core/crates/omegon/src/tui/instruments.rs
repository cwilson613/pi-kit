//! CIC instrument panel — four simultaneous displays.
//!
//! Ported directly from the fractal_demo example (the operator-tuned reference).
//!
//! ## Four Instruments
//!
//! 1. **Context** (Perlin flow) — context utilization, scale=7.9
//! 2. **Tools** (Lissajous curves) — tool execution activity, curves=3.6
//! 3. **Thinking** (Plasma sine) — inference/thinking state, complexity=2.46
//! 4. **Memory** (CA waterfall) — memory activity with per-mind columns
//!
//! All use unified navy→teal→amber CIE L* perceptual color ramp.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

// ─── Panel ──────────────────────────────────────────────────────────────

/// Instrument panel — four simultaneous displays in a 2×2 grid.
pub struct InstrumentPanel {
    time: f64,
    /// Per-instrument intensity (0.0 = idle, 1.0 = max)
    context_intensity: f64,
    tool_intensity: f64,
    thinking_intensity: f64,
    memory_intensity: f64,
    /// Tool error state → red border
    tool_error: bool,
    /// Waterfall persistent state (one per mind)
    waterfalls: [WaterfallState; 4],
    minds_active: [bool; 4],
    /// Focus mode toggle
    pub focus_mode: bool,
}

impl Default for InstrumentPanel {
    fn default() -> Self {
        Self {
            time: 0.0,
            context_intensity: 0.0,
            tool_intensity: 0.0,
            thinking_intensity: 0.0,
            memory_intensity: 0.0,
            tool_error: false,
            waterfalls: [
                WaterfallState::new(22, 5),
                WaterfallState::new(22, 5),
                WaterfallState::new(22, 5),
                WaterfallState::new(22, 5),
            ],
            minds_active: [true, false, false, false],
            focus_mode: false,
        }
    }
}

impl InstrumentPanel {
    pub fn update_telemetry(
        &mut self,
        context_pct: f32,
        tool_calls: u32,
        thinking_level: &str,
        memory_facts: usize,
        _memory_minds: &[String],
        dt: f64,
    ) {
        self.time += dt;

        // Context: cap at 70% (auto-compaction threshold)
        self.context_intensity = (context_pct as f64 / 70.0).min(1.0);

        // Tools: spike on calls, decay
        if tool_calls > 0 {
            self.tool_intensity = (self.tool_intensity + 0.3).min(1.0);
        } else {
            self.tool_intensity = (self.tool_intensity - dt * 0.5).max(0.0);
        }

        // Thinking: map level name to intensity, quadratic speed ramp
        self.thinking_intensity = match thinking_level {
            "high" => 0.85, "medium" => 0.6, "low" => 0.35, "minimal" => 0.15, _ => 0.0,
        };

        // Memory: based on fact count presence
        let mem_active = memory_facts > 0;
        self.memory_intensity = if mem_active {
            (self.memory_intensity + dt * 0.5).min(0.4)
        } else {
            (self.memory_intensity - dt * 0.3).max(0.0)
        };

        // Tick waterfalls
        let active_count = self.minds_active.iter().filter(|&&a| a).count().max(1);
        let col_w = (22 / active_count).max(2);
        for i in 0..4 {
            if !self.minds_active[i] { continue; }
            let density = 0.008 + self.memory_intensity * 0.25;
            let scroll = 6.0 * (0.5 + self.memory_intensity * 1.5);
            let rule = 204u8; // identity at idle; TODO: per-mind ops
            self.waterfalls[i].ensure_size(col_w, 5);
            self.waterfalls[i].tick(dt, scroll, density, rule, 0.85);
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus_mode = !self.focus_mode;
    }

    pub fn render(&self, area: Rect, frame: &mut Frame) {
        if area.width < 8 || area.height < 4 { return; }

        let rows = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ]).split(area);
        let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
        let bot = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);

        let instruments: [(Rect, &str, f64, bool); 4] = [
            (top[0], "context",  self.context_intensity, false),
            (top[1], "tools",    self.tool_intensity, self.tool_error),
            (bot[0], "thinking", self.thinking_intensity, false),
            (bot[1], "memory",   self.memory_intensity, false),
        ];

        for (idx, (area, label, intensity, is_error)) in instruments.iter().enumerate() {
            let pct = (*intensity * 100.0) as u32;
            let border_color = if *is_error {
                Color::Rgb(224, 72, 72)
            } else {
                Color::Rgb(20, 40, 55)
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    format!(" {} {}% ", label, pct),
                    Style::default().fg(Color::Rgb(64, 88, 112)),
                ));
            let inner = block.inner(*area);
            frame.render_widget(block, *area);

            if inner.width < 2 || inner.height < 1 { continue; }

            match idx {
                0 => render_perlin(self.time, *intensity, inner, frame.buffer_mut()),
                1 => render_lissajous(self.time, *intensity, inner, frame.buffer_mut()),
                2 => render_plasma(self.time, *intensity, inner, frame.buffer_mut()),
                3 => render_waterfall_multi(*intensity, inner, frame.buffer_mut(),
                        &self.waterfalls, &self.minds_active),
                _ => {}
            }
        }
    }
}

// ─── Color ramp (CIE L* perceptual, ported from demo) ──────────────────

fn intensity_color(intensity: f64) -> Color {
    if intensity < 0.005 { return Color::Rgb(0, 1, 3); }
    let linear = intensity.clamp(0.0, 1.0);
    let i = if linear > 0.008856 { linear.cbrt() } else { linear * 7.787 + 16.0 / 116.0 };
    let i = ((i - 0.138) / (1.0 - 0.138)).clamp(0.0, 1.0);

    if i < 0.3 {
        let t = i / 0.3;
        Color::Rgb((1.0 + t * 3.0) as u8, (4.0 + t * 34.0) as u8, (6.0 + t * 30.0) as u8)
    } else if i < 0.5 {
        let t = (i - 0.3) / 0.2;
        Color::Rgb((4.0 + t * 4.0) as u8, (38.0 + t * 10.0) as u8, (36.0 + t * 6.0) as u8)
    } else {
        let t = (i - 0.5) / 0.5;
        Color::Rgb((8.0 + t * 82.0) as u8, (48.0 - t * 2.0) as u8, (42.0 - t * 34.0) as u8)
    }
}

fn bg_color() -> Color { Color::Rgb(0, 1, 3) }

fn pixel_color(value: f64, intensity: f64) -> Color {
    let v = value.clamp(0.0, 1.0);
    if v < 0.01 { return bg_color(); }
    intensity_color(v * intensity)
}

fn pixel_color_floor(value: f64, intensity: f64, floor: f64) -> Color {
    let v = value.clamp(0.0, 1.0);
    if v < 0.01 { return bg_color(); }
    let effective = (v * intensity).max(v * floor);
    intensity_color(effective)
}

fn set_halfblock(buf: &mut Buffer, area: Rect, px: usize, row: usize, top: Color, bot: Color) {
    if let Some(cell) = buf.cell_mut(Position::new(area.x + px as u16, area.y + row as u16)) {
        cell.set_char('▀');
        cell.set_fg(top);
        cell.set_bg(bot);
    }
}

// ─── Perlin flow (context) — ported from demo ──────────────────────────

fn render_perlin(time: f64, intensity: f64, area: Rect, buf: &mut Buffer) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    // Speed increases with intensity (flame effect)
    let speed = 0.3 + intensity * 2.5;
    let t = time * speed;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top = noise_octaves(px as f64 / 7.9, py as f64 / 7.9, t, 2, 4.0);
            let bot = noise_octaves(px as f64 / 7.9, (py + 1) as f64 / 7.9, t, 2, 4.0);
            let tc = pixel_color((top * 0.5 + 0.5) * 1.0, intensity);
            let bc = pixel_color((bot * 0.5 + 0.5) * 1.0, intensity);
            set_halfblock(buf, area, px, row, tc, bc);
        }
    }
}

fn noise_octaves(x: f64, y: f64, z: f64, octaves: usize, lacunarity: f64) -> f64 {
    let mut val = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut total_amp = 0.0;
    for _ in 0..octaves.max(1) {
        val += noise_sample(x * freq, y * freq, z) * amp;
        total_amp += amp;
        amp *= 0.5;
        freq *= lacunarity;
    }
    val / total_amp
}

fn noise_sample(x: f64, y: f64, z: f64) -> f64 {
    let v1 = (x * 1.3 + z).sin() * (y * 0.7 + z * 0.5).cos();
    let v2 = ((x + y) * 0.8 - z * 0.3).sin();
    let v3 = (x * 2.1 - z * 0.7).cos() * (y * 1.5 + z * 0.4).sin();
    (v1 + v2 + v3) / 3.0
}

// ─── Plasma sine (thinking) — ported from demo ─────────────────────────

fn render_plasma(time: f64, intensity: f64, area: Rect, buf: &mut Buffer) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    // Quadratic speed: slow ignition, then accelerates
    let speed = 0.2 + intensity * intensity * 2.0;
    let t = time * speed;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top = plasma_sample(px as f64, py as f64, t);
            let bot = plasma_sample(px as f64, (py + 1) as f64, t);
            let tc = pixel_color((top * 0.5 + 0.5) * 1.0, intensity);
            let bc = pixel_color((bot * 0.5 + 0.5) * 1.0, intensity);
            set_halfblock(buf, area, px, row, tc, bc);
        }
    }
}

fn plasma_sample(x: f64, y: f64, t: f64) -> f64 {
    let c = 2.46;
    let d = 0.68;
    let v1 = (x / (6.0 / c) + t).sin();
    let v2 = ((y / (4.0 / c) + t * 0.7).sin() + (x / (8.0 / c)).cos()).sin();
    let v3 = ((x * x + y * y).sqrt() * d / (6.0 / c) - t * 1.3).sin();
    let v4 = (x / (3.0 / c) - t * 0.5).cos() * (y / (5.0 / c) + t * 0.9).sin();
    (v1 + v2 + v3 + v4) / 4.0
}

// ─── Lissajous curves (tools) — ported from demo ───────────────────────

fn render_lissajous(time: f64, intensity: f64, area: Rect, buf: &mut Buffer) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    let mut grid = vec![0u32; w * h];

    let nc = 3; // curves=3.6 rounded
    let pts = 500usize;
    let speed = match () {
        _ if intensity > 0.8 => 1.2,  // cleave
        _ if intensity > 0.5 => 0.8,  // burst
        _ if intensity > 0.2 => 0.5,  // single
        _ => 0.3,                      // idle
    };
    let t = time * speed;

    for curve in 0..nc {
        let fx = 1.9 + curve as f64 * 3.0 / nc as f64;
        let fy = 1.9 + 1.0 + curve as f64 * (3.0 * 0.8) / nc as f64;
        let phase = t * (1.0 + curve as f64 * 0.03);
        for i in 0..pts {
            let tt = i as f64 / pts as f64 * std::f64::consts::TAU;
            let x = (fx * tt + phase).sin();
            let y = (fy * tt + phase * 0.3).cos();
            let gx = ((x * 0.5 + 0.5) * w as f64) as usize;
            let gy = ((y * 0.5 + 0.5) * h as f64) as usize;
            if gx < w && gy < h { grid[gy * w + gx] += 1; }
        }
    }

    let max_hits = (*grid.iter().max().unwrap_or(&1)).max(1) as f64;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top_v = (grid[py * w + px] as f64 / max_hits).min(1.0);
            let bot_v = if py + 1 < h { (grid[(py + 1) * w + px] as f64 / max_hits).min(1.0) } else { 0.0 };
            let tc = pixel_color_floor(top_v, intensity, 0.25);
            let bc = pixel_color_floor(bot_v, intensity, 0.25);
            set_halfblock(buf, area, px, row, tc, bc);
        }
    }
}

// ─── CA waterfall (memory) — ported from demo ──────────────────────────

const NOISE_CHARS: &[char] = &[
    '▏', '▎', '▍', '░',
    '▌', '▐', '▒', '┤', '├', '│', '─',
    '▊', '▋', '▓', '╱', '╲', '┼', '╪', '╫',
    '█', '╬', '■', '◆',
];

/// Persistent waterfall state per mind.
pub struct WaterfallState {
    grid: Vec<f64>,
    width: usize,
    height: usize,
    scroll_accum: f64,
    rng: u64,
}

impl WaterfallState {
    fn new(w: usize, h: usize) -> Self {
        Self { grid: vec![0.0; w * h], width: w, height: h, scroll_accum: 0.0, rng: 0xdeadbeef }
    }

    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn tick(&mut self, dt: f64, scroll_rate: f64, density: f64, rule: u8, fade: f64) {
        self.scroll_accum += dt * scroll_rate;
        while self.scroll_accum >= 1.0 {
            self.scroll_accum -= 1.0;
            let w = self.width;
            let h = self.height;
            for y in 0..(h - 1) {
                for x in 0..w {
                    self.grid[y * w + x] = self.grid[(y + 1) * w + x] * fade;
                }
            }
            let prev_row = h - 2;
            let new_row = h - 1;
            for x in 0..w {
                let left = if x > 0 { (self.grid[prev_row * w + x - 1] > 0.3) as u8 } else { 0 };
                let center = (self.grid[prev_row * w + x] > 0.3) as u8;
                let right = if x + 1 < w { (self.grid[prev_row * w + x + 1] > 0.3) as u8 } else { 0 };
                let neighborhood = (left << 2) | (center << 1) | right;
                let alive = (rule >> neighborhood) & 1 == 1;
                let random_birth = (self.next_rand() % 1000) < (density * 1000.0) as u64;
                self.grid[new_row * w + x] = if alive || random_birth { 1.0 } else { 0.0 };
            }
        }
    }

    fn ensure_size(&mut self, w: usize, h: usize) {
        if self.width != w || self.height != h {
            self.grid = vec![0.0; w * h];
            self.width = w;
            self.height = h;
        }
    }
}

fn render_waterfall_multi(
    intensity: f64, area: Rect, buf: &mut Buffer,
    waterfalls: &[WaterfallState; 4], minds_active: &[bool; 4],
) {
    let active_indices: Vec<usize> = minds_active.iter().enumerate()
        .filter(|(_, a)| **a).map(|(i, _)| i).collect();
    let n = active_indices.len();
    if n == 0 { return; }

    let total_w = area.width as usize;
    let gap = if n > 1 { 1 } else { 0 };
    let total_gaps = if n > 1 { n - 1 } else { 0 };
    let usable = total_w.saturating_sub(total_gaps);
    let col_w = usable / n;

    for (seg_idx, &mind_idx) in active_indices.iter().enumerate() {
        let x_offset = seg_idx * (col_w + gap);
        let seg_area = Rect {
            x: area.x + x_offset as u16,
            y: area.y,
            width: (col_w as u16).min(area.width.saturating_sub(x_offset as u16)),
            height: area.height,
        };
        render_waterfall(intensity, seg_area, buf, &waterfalls[mind_idx]);

        if seg_idx < n - 1 && gap > 0 {
            let sep_x = area.x + (x_offset + col_w) as u16;
            for y in area.y..area.bottom() {
                if let Some(cell) = buf.cell_mut(Position::new(sep_x, y)) {
                    cell.set_char('│');
                    cell.set_fg(Color::Rgb(20, 40, 55));
                    cell.set_bg(bg_color());
                }
            }
        }
    }
}

fn render_waterfall(intensity: f64, area: Rect, buf: &mut Buffer, wf: &WaterfallState) {
    for cy in 0..area.height as usize {
        for cx in 0..area.width as usize {
            let val = if cx < wf.width && cy < wf.height {
                wf.grid[cy * wf.width + cx]
            } else { 0.0 };

            if val < 0.05 {
                if let Some(cell) = buf.cell_mut(Position::new(area.x + cx as u16, area.y + cy as u16)) {
                    cell.set_char(' ');
                    cell.set_fg(bg_color());
                    cell.set_bg(bg_color());
                }
                continue;
            }

            let hash = ((cx * 7 + cy * 13 + (val * 100.0) as usize) * 31) % NOISE_CHARS.len();
            let tier = ((val * (NOISE_CHARS.len() - 1) as f64) as usize).min(NOISE_CHARS.len() - 1);
            let idx = (tier / 2 + hash / 2).min(NOISE_CHARS.len() - 1);
            let ch = NOISE_CHARS[idx];

            let effective = (val * intensity).max(val * 0.08);
            let color = intensity_color(effective);

            if let Some(cell) = buf.cell_mut(Position::new(area.x + cx as u16, area.y + cy as u16)) {
                cell.set_char(ch);
                cell.set_fg(color);
                cell.set_bg(bg_color());
            }
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensity_color_floor_is_bg() {
        assert!(matches!(intensity_color(0.0), Color::Rgb(0, 1, 3)));
    }

    #[test]
    fn intensity_color_ramp_progresses() {
        if let Color::Rgb(r, g, b) = intensity_color(0.1) {
            assert!(r < 20 && g < 50 && b < 50, "0.1 dark: ({r},{g},{b})");
        }
        if let Color::Rgb(r, g, b) = intensity_color(1.0) {
            assert!(r > 40 && b < r, "1.0 amber: ({r},{g},{b})");
        }
    }

    #[test]
    fn panel_renders_without_panic() {
        let panel = InstrumentPanel::default();
        let area = Rect::new(0, 0, 96, 12);
        let backend = ratatui::backend::TestBackend::new(96, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| panel.render(area, f)).unwrap();
    }

    #[test]
    fn waterfall_scrolls() {
        let mut wf = WaterfallState::new(10, 5);
        wf.tick(0.5, 10.0, 0.1, 30, 0.85);
        let has_content = wf.grid.iter().any(|&v| v > 0.0);
        assert!(has_content, "waterfall should have content after tick");
    }
}
