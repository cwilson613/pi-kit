//! Fractal rendering demo — interactive parameter tuning.
//! Run: cargo run -p omegon --example fractal_demo
//!
//! Controls:
//!   Tab       — cycle algorithm (Perlin / Plasma / Attractor / Lissajous)
//!   ↑/↓       — select parameter
//!   ←/→       — adjust value (hold Shift for fine, Ctrl for coarse)
//!   1/2/3     — color scheme (Ocean / Amber / Violet)
//!   q         — quit

use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

fn main() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let start = Instant::now();
    let mut state = DemoState::default();

    loop {
        let t = start.elapsed().as_secs_f64();

        terminal.draw(|f| {
            let area = f.area();
            let bg = Color::Rgb(6, 10, 18);
            let fg = Color::Rgb(196, 216, 228);
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    let cell = &mut f.buffer_mut()[(x, y)];
                    cell.set_bg(bg);
                    cell.set_fg(fg);
                }
            }

            let chunks = Layout::vertical([
                Constraint::Length(1),  // title
                Constraint::Min(8),    // render + params
                Constraint::Length(2), // controls
            ]).split(area);

            // Title
            let scheme_name = match state.scheme { 0 => "Ocean", 1 => "Amber", _ => "Violet" };
            let title = format!(" {} · {} · t={:.1}s", state.algo_name(), scheme_name, t);
            f.render_widget(
                Paragraph::new(title).style(Style::default().fg(Color::Rgb(42, 180, 200)).add_modifier(Modifier::BOLD)),
                chunks[0],
            );

            // Main area: render + params side by side
            let cols = Layout::horizontal([
                Constraint::Length(38), // 36 + border
                Constraint::Min(30),   // params
            ]).split(chunks[1]);

            // Fractal render area (36×8)
            let render_area = Rect {
                x: cols[0].x + 1,
                y: cols[0].y + 1,
                width: 36.min(cols[0].width.saturating_sub(2)),
                height: 8.min(cols[0].height.saturating_sub(2)),
            };
            let border = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(32, 72, 96)))
                .title(format!(" {}×{} ", render_area.width, render_area.height));
            f.render_widget(border, cols[0]);
            state.render(t, render_area, f.buffer_mut());

            // Also render wider below if space
            if cols[0].height > 12 {
                let wide_area = Rect {
                    x: cols[0].x + 1,
                    y: cols[0].y + 11,
                    width: (area.width - 4).min(80),
                    height: 8.min(cols[0].height.saturating_sub(14)),
                };
                if wide_area.height >= 4 {
                    let wide_border = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(32, 72, 96)))
                        .title(format!(" {}×{} wide ", wide_area.width, wide_area.height));
                    let wb = Rect { x: wide_area.x - 1, y: wide_area.y - 1, width: wide_area.width + 2, height: wide_area.height + 2 };
                    f.render_widget(wide_border, wb);
                    state.render(t, wide_area, f.buffer_mut());
                }
            }

            // Parameter sliders
            let params = state.params();
            let mut lines: Vec<Line<'_>> = vec![
                Line::from(Span::styled(" Parameters", Style::default().fg(Color::Rgb(42, 180, 200)).add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];
            for (i, (name, val, min, max)) in params.iter().enumerate() {
                let selected = i == state.selected_param;
                let pct = (val - min) / (max - min);
                let bar_w = 16;
                let filled = (pct * bar_w as f64) as usize;
                let bar: String = "█".repeat(filled) + &"░".repeat(bar_w - filled);

                let cursor = if selected { "▸ " } else { "  " };
                let name_style = if selected {
                    Style::default().fg(Color::Rgb(42, 180, 200)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(96, 120, 136))
                };
                let val_style = Style::default().fg(Color::Rgb(196, 216, 228));
                let bar_style = if selected {
                    Style::default().fg(Color::Rgb(42, 180, 200))
                } else {
                    Style::default().fg(Color::Rgb(48, 72, 96))
                };

                lines.push(Line::from(vec![
                    Span::styled(cursor, name_style),
                    Span::styled(format!("{:<12}", name), name_style),
                    Span::styled(format!("{:>6.2} ", val), val_style),
                    Span::styled(bar, bar_style),
                ]));
            }
            f.render_widget(Paragraph::new(lines), cols[1]);

            // Controls
            let controls = " Tab=algo  ↑↓=select  ←→=adjust (Shift=fine)  1/2/3=color  q=quit";
            f.render_widget(
                Paragraph::new(controls).style(Style::default().fg(Color::Rgb(64, 88, 112))),
                chunks[2],
            );
        })?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                let fine = modifiers.contains(KeyModifiers::SHIFT);
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab => state.next_algo(),
                    KeyCode::BackTab => state.prev_algo(),
                    KeyCode::Up => state.prev_param(),
                    KeyCode::Down => state.next_param(),
                    KeyCode::Left => state.adjust(-1.0, fine),
                    KeyCode::Right => state.adjust(1.0, fine),
                    KeyCode::Char('1') => state.scheme = 0,
                    KeyCode::Char('2') => state.scheme = 1,
                    KeyCode::Char('3') => state.scheme = 2,
                    _ => {}
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

// ─── State ──────────────────────────────────────────────────────────────────

struct DemoState {
    algo: usize,
    scheme: usize,
    selected_param: usize,
    // Perlin params
    perlin_scale: f64,
    perlin_speed: f64,
    perlin_octaves: f64,
    perlin_lacunarity: f64,
    perlin_amplitude: f64,
    // Plasma params
    plasma_complexity: f64,
    plasma_speed: f64,
    plasma_num_waves: f64,
    plasma_distortion: f64,
    plasma_amplitude: f64,
    // Attractor params
    attr_iterations: f64,
    attr_evolve_speed: f64,
    attr_a: f64,
    attr_b: f64,
    attr_spread: f64,
    attr_gamma: f64,
    // Lissajous params
    liss_num_curves: f64,
    liss_speed: f64,
    liss_freq_base: f64,
    liss_freq_spread: f64,
    liss_amplitude: f64,
    liss_points: f64,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            algo: 0, scheme: 0, selected_param: 0,
            perlin_scale: 8.0, perlin_speed: 0.5, perlin_octaves: 1.0,
            perlin_lacunarity: 2.0, perlin_amplitude: 0.7,
            plasma_complexity: 1.0, plasma_speed: 0.3, plasma_num_waves: 3.0,
            plasma_distortion: 0.5, plasma_amplitude: 0.7,
            attr_iterations: 8000.0, attr_evolve_speed: 0.02, attr_a: -1.4,
            attr_b: 1.6, attr_spread: 6.0, attr_gamma: 0.5,
            liss_num_curves: 3.0, liss_speed: 0.3, liss_freq_base: 3.0,
            liss_freq_spread: 0.7, liss_amplitude: 0.45, liss_points: 2000.0,
        }
    }
}

impl DemoState {
    fn algo_name(&self) -> &str {
        match self.algo {
            0 => "Perlin Flow (idle)",
            1 => "Plasma Sine (thinking)",
            2 => "Clifford Attractor (working)",
            3 => "Lissajous (cleave)",
            _ => "?",
        }
    }

    fn next_algo(&mut self) { self.algo = (self.algo + 1) % 4; self.selected_param = 0; }
    fn prev_algo(&mut self) { self.algo = (self.algo + 3) % 4; self.selected_param = 0; }
    fn next_param(&mut self) {
        let n = self.params().len();
        self.selected_param = (self.selected_param + 1) % n;
    }
    fn prev_param(&mut self) {
        let n = self.params().len();
        self.selected_param = (self.selected_param + n - 1) % n;
    }

    // Returns (name, value, min, max) for current algo
    fn params(&self) -> Vec<(&str, f64, f64, f64)> {
        match self.algo {
            0 => vec![
                ("scale", self.perlin_scale, 1.0, 20.0),
                ("speed", self.perlin_speed, 0.05, 2.0),
                ("octaves", self.perlin_octaves, 1.0, 4.0),
                ("lacunarity", self.perlin_lacunarity, 1.0, 4.0),
                ("amplitude", self.perlin_amplitude, 0.1, 1.0),
            ],
            1 => vec![
                ("complexity", self.plasma_complexity, 0.3, 3.0),
                ("speed", self.plasma_speed, 0.05, 1.5),
                ("waves", self.plasma_num_waves, 2.0, 6.0),
                ("distortion", self.plasma_distortion, 0.0, 1.5),
                ("amplitude", self.plasma_amplitude, 0.1, 1.0),
            ],
            2 => vec![
                ("iterations", self.attr_iterations, 1000.0, 32000.0),
                ("evolve", self.attr_evolve_speed, 0.002, 0.1),
                ("a", self.attr_a, -2.0, 0.0),
                ("b", self.attr_b, 0.5, 2.5),
                ("spread", self.attr_spread, 3.0, 8.0),
                ("gamma", self.attr_gamma, 0.2, 1.0),
            ],
            3 => vec![
                ("curves", self.liss_num_curves, 1.0, 8.0),
                ("speed", self.liss_speed, 0.05, 1.0),
                ("freq_base", self.liss_freq_base, 1.0, 7.0),
                ("freq_spread", self.liss_freq_spread, 0.1, 3.0),
                ("amplitude", self.liss_amplitude, 0.15, 0.5),
                ("points", self.liss_points, 500.0, 8000.0),
            ],
            _ => vec![],
        }
    }

    fn adjust(&mut self, dir: f64, fine: bool) {
        let params = self.params();
        if self.selected_param >= params.len() { return; }
        let (_, val, min, max) = params[self.selected_param];
        let range = max - min;
        let step = if fine { range * 0.01 } else { range * 0.05 };
        let new_val = (val + dir * step).clamp(min, max);

        match self.algo {
            0 => match self.selected_param {
                0 => self.perlin_scale = new_val,
                1 => self.perlin_speed = new_val,
                2 => self.perlin_octaves = new_val,
                3 => self.perlin_lacunarity = new_val,
                4 => self.perlin_amplitude = new_val,
                _ => {}
            },
            1 => match self.selected_param {
                0 => self.plasma_complexity = new_val,
                1 => self.plasma_speed = new_val,
                2 => self.plasma_num_waves = new_val,
                3 => self.plasma_distortion = new_val,
                4 => self.plasma_amplitude = new_val,
                _ => {}
            },
            2 => match self.selected_param {
                0 => self.attr_iterations = new_val,
                1 => self.attr_evolve_speed = new_val,
                2 => self.attr_a = new_val,
                3 => self.attr_b = new_val,
                4 => self.attr_spread = new_val,
                5 => self.attr_gamma = new_val,
                _ => {}
            },
            3 => match self.selected_param {
                0 => self.liss_num_curves = new_val,
                1 => self.liss_speed = new_val,
                2 => self.liss_freq_base = new_val,
                3 => self.liss_freq_spread = new_val,
                4 => self.liss_amplitude = new_val,
                5 => self.liss_points = new_val,
                _ => {}
            },
            _ => {}
        }
    }

    fn color_scheme(&self) -> ColorScheme {
        match self.scheme {
            0 => ColorScheme::Ocean,
            1 => ColorScheme::Amber,
            _ => ColorScheme::Violet,
        }
    }

    fn render(&self, t: f64, area: Rect, buf: &mut Buffer) {
        let cs = self.color_scheme();
        match self.algo {
            0 => render_perlin(t, area, buf, cs, self),
            1 => render_plasma(t, area, buf, cs, self),
            2 => render_attractor(t, area, buf, cs, self),
            3 => render_lissajous(t, area, buf, cs, self),
            _ => {}
        }
    }
}

// ─── Color ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum ColorScheme { Ocean, Amber, Violet }

fn scheme_color(scheme: ColorScheme, t: f64, amplitude: f64) -> Color {
    let t = (t * amplitude).sqrt().clamp(0.0, 1.0);
    match scheme {
        ColorScheme::Ocean => Color::Rgb(
            (t * 12.0) as u8,
            (t * 36.0 + t * t * 20.0) as u8,
            (t * 50.0 + t * t * 30.0) as u8,
        ),
        ColorScheme::Amber => Color::Rgb(
            (t * 65.0 + t * t * 20.0) as u8,
            (t * 30.0 + t * t * 10.0) as u8,
            (t * 8.0) as u8,
        ),
        ColorScheme::Violet => Color::Rgb(
            (t * 35.0 + t * t * 15.0) as u8,
            (t * 12.0) as u8,
            (t * 55.0 + t * t * 25.0) as u8,
        ),
    }
}

fn bg_color() -> Color { Color::Rgb(6, 10, 18) }

// ─── Perlin ─────────────────────────────────────────────────────────────────

fn render_perlin(time: f64, area: Rect, buf: &mut Buffer, cs: ColorScheme, s: &DemoState) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top = noise_octaves(px as f64 / s.perlin_scale, py as f64 / s.perlin_scale,
                                     time * s.perlin_speed, s.perlin_octaves as usize, s.perlin_lacunarity);
            let bot = noise_octaves(px as f64 / s.perlin_scale, (py+1) as f64 / s.perlin_scale,
                                     time * s.perlin_speed, s.perlin_octaves as usize, s.perlin_lacunarity);
            let tc = scheme_color(cs, (top * 0.5 + 0.5).clamp(0.0, 1.0), s.perlin_amplitude);
            let bc = scheme_color(cs, (bot * 0.5 + 0.5).clamp(0.0, 1.0), s.perlin_amplitude);
            if let Some(cell) = buf.cell_mut(Position::new(area.x + px as u16, area.y + row as u16)) {
                cell.set_char('▀');
                cell.set_fg(tc);
                cell.set_bg(bc);
            }
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

// ─── Plasma ─────────────────────────────────────────────────────────────────

fn render_plasma(time: f64, area: Rect, buf: &mut Buffer, cs: ColorScheme, s: &DemoState) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    let num_waves = s.plasma_num_waves as usize;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top = plasma_sample(px as f64, py as f64, time, s, num_waves);
            let bot = plasma_sample(px as f64, (py+1) as f64, time, s, num_waves);
            let tc = scheme_color(cs, (top * 0.5 + 0.5).clamp(0.0, 1.0), s.plasma_amplitude);
            let bc = scheme_color(cs, (bot * 0.5 + 0.5).clamp(0.0, 1.0), s.plasma_amplitude);
            if let Some(cell) = buf.cell_mut(Position::new(area.x + px as u16, area.y + row as u16)) {
                cell.set_char('▀');
                cell.set_fg(tc);
                cell.set_bg(bc);
            }
        }
    }
}

fn plasma_sample(x: f64, y: f64, t: f64, s: &DemoState, waves: usize) -> f64 {
    let c = s.plasma_complexity;
    let sp = t * s.plasma_speed;
    let mut v = (x / (6.0 / c) + sp).sin();
    if waves >= 2 { v += ((y / (4.0 / c) + sp * 0.7).sin() + (x / (8.0 / c)).cos()).sin(); }
    if waves >= 3 { v += ((x * x + y * y).sqrt() * s.plasma_distortion / (6.0 / c) - sp * 1.3).sin(); }
    if waves >= 4 { v += (x / (3.0 / c) - sp * 0.5).cos() * (y / (5.0 / c) + sp * 0.9).sin(); }
    if waves >= 5 { v += ((x - y) / (4.0 / c) + sp * 0.3).sin(); }
    v / waves as f64
}

// ─── Attractor ──────────────────────────────────────────────────────────────

fn render_attractor(time: f64, area: Rect, buf: &mut Buffer, cs: ColorScheme, s: &DemoState) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    let mut grid = vec![0u32; w * h];

    let a = s.attr_a + (time * s.attr_evolve_speed).sin() * 0.3;
    let b = s.attr_b + (time * s.attr_evolve_speed * 0.75).cos() * 0.2;
    let c = 1.0 + (time * s.attr_evolve_speed * 1.25).sin() * 0.2;
    let d = 0.7 + (time * s.attr_evolve_speed * 1.5).cos() * 0.1;

    let iters = s.attr_iterations as usize;
    let spread = s.attr_spread;
    let mut x = 0.1_f64;
    let mut y = 0.1_f64;
    for _ in 0..iters {
        let nx = (a * y).sin() + c * (a * x).cos();
        let ny = (b * x).sin() + d * (b * y).cos();
        x = nx; y = ny;
        let gx = ((x + spread / 2.0) / spread * w as f64) as usize;
        let gy = ((y + spread / 2.0) / spread * h as f64) as usize;
        if gx < w && gy < h { grid[gy * w + gx] += 1; }
    }

    let max_hits = (*grid.iter().max().unwrap_or(&1)).max(1) as f64;
    for py in (0..h).step_by(2) {
        let row = py / 2;
        if row >= area.height as usize { break; }
        for px in 0..w {
            if px >= area.width as usize { break; }
            let top_v = (grid[py * w + px] as f64 / max_hits).powf(s.attr_gamma);
            let bot_v = if py+1 < h { (grid[(py+1) * w + px] as f64 / max_hits).powf(s.attr_gamma) } else { 0.0 };
            let tc = if top_v < 0.005 { bg_color() } else { scheme_color(cs, top_v, 1.0) };
            let bc = if bot_v < 0.005 { bg_color() } else { scheme_color(cs, bot_v, 1.0) };
            if let Some(cell) = buf.cell_mut(Position::new(area.x + px as u16, area.y + row as u16)) {
                cell.set_char('▀');
                cell.set_fg(tc);
                cell.set_bg(bc);
            }
        }
    }
}

// ─── Lissajous ──────────────────────────────────────────────────────────────

fn render_lissajous(time: f64, area: Rect, buf: &mut Buffer, cs: ColorScheme, s: &DemoState) {
    let w = area.width as usize;
    let h = area.height as usize * 2;
    let mut grid = vec![0u32; w * h];
    let nc = s.liss_num_curves as usize;
    let pts = s.liss_points as usize;

    for curve in 0..nc {
        let fx = s.liss_freq_base + curve as f64 * s.liss_freq_spread;
        let fy = s.liss_freq_base + 1.0 + curve as f64 * (s.liss_freq_spread * 0.8);
        let phase = time * (s.liss_speed + curve as f64 * 0.03);
        for i in 0..pts {
            let t = i as f64 / pts as f64 * std::f64::consts::TAU;
            let x = (fx * t + phase).sin();
            let y = (fy * t + phase * 0.3).cos();
            let gx = ((x * s.liss_amplitude + 0.5) * w as f64) as usize;
            let gy = ((y * s.liss_amplitude + 0.5) * h as f64) as usize;
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
            let bot_v = if py+1 < h { (grid[(py+1) * w + px] as f64 / max_hits).min(1.0) } else { 0.0 };
            let tc = if top_v < 0.01 { bg_color() } else { scheme_color(cs, top_v, 1.0) };
            let bc = if bot_v < 0.01 { bg_color() } else { scheme_color(cs, bot_v, 1.0) };
            if let Some(cell) = buf.cell_mut(Position::new(area.x + px as u16, area.y + row as u16)) {
                cell.set_char('▀');
                cell.set_fg(tc);
                cell.set_bg(bc);
            }
        }
    }
}
