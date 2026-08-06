//! Opt-in live TUI performance tracing.

use serde::Serialize;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ENABLED_ENV: &str = "OMEGON_TUI_TRACE";
const PATH_ENV: &str = "OMEGON_TUI_TRACE_PATH";
const WINDOW_ENV: &str = "OMEGON_TUI_TRACE_WINDOW_SECS";

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DrawPhaseTimings {
    pub(crate) preparation: Duration,
    pub(crate) background_fill: Duration,
    pub(crate) conversation_projection: Duration,
    pub(crate) conversation_render: Duration,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Distribution {
    samples: u64,
    total: u64,
    mean: u64,
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildIdentity {
    package_version: &'static str,
    git_sha: &'static str,
    git_describe: &'static str,
    build_date: &'static str,
    build_profile: &'static str,
    manifest_dir: &'static str,
    executable: String,
    process_id: u32,
    working_directory: String,
}

impl BuildIdentity {
    fn current() -> Self {
        Self {
            package_version: env!("CARGO_PKG_VERSION"),
            git_sha: env!("OMEGON_GIT_SHA"),
            git_describe: env!("OMEGON_GIT_DESCRIBE"),
            build_date: env!("OMEGON_BUILD_DATE"),
            build_profile: option_env!("PROFILE").unwrap_or("unknown"),
            manifest_dir: env!("CARGO_MANIFEST_DIR"),
            executable: std::env::current_exe()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            process_id: std::process::id(),
            working_directory: std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceWindow {
    schema_version: u32,
    kind: &'static str,
    generated_at_unix_ms: u128,
    window_ms: u128,
    build: BuildIdentity,
    agent_events: u64,
    agent_drain_passes: u64,
    agent_budget_hits: u64,
    agent_events_per_drain: Distribution,
    operator_inputs: u64,
    input_batches: u64,
    inputs_per_batch: Distribution,
    frames: u64,
    urgent_frames: u64,
    background_frames: u64,
    dirty_passes_without_draw: u64,
    draw_us: Distribution,
    draw_callback_us: Distribution,
    backend_us: Distribution,
    preparation_us: Distribution,
    background_fill_us: Distribution,
    conversation_projection_us: Distribution,
    conversation_render_us: Distribution,
    remaining_render_us: Distribution,
    input_to_frame_us: Distribution,
    conversation_segments: Distribution,
    conversation_scroll_offset: Distribution,
    streaming_frames: u64,
    detached_frames: u64,
    slow_frames_over_16ms: u64,
    slow_frames_over_33ms: u64,
    slow_frames_over_100ms: u64,
}

pub(crate) struct TuiRuntimeTrace {
    path: PathBuf,
    window: Duration,
    started: Instant,
    build: BuildIdentity,
    agent_events: u64,
    agent_drain_passes: u64,
    agent_budget_hits: u64,
    agent_events_per_drain: Vec<u64>,
    operator_inputs: u64,
    input_batches: u64,
    inputs_per_batch: Vec<u64>,
    frames: u64,
    urgent_frames: u64,
    background_frames: u64,
    dirty_passes_without_draw: u64,
    draw_us: Vec<u64>,
    draw_callback_us: Vec<u64>,
    backend_us: Vec<u64>,
    preparation_us: Vec<u64>,
    background_fill_us: Vec<u64>,
    conversation_projection_us: Vec<u64>,
    conversation_render_us: Vec<u64>,
    remaining_render_us: Vec<u64>,
    input_to_frame_us: Vec<u64>,
    conversation_segments: Vec<u64>,
    conversation_scroll_offset: Vec<u64>,
    streaming_frames: u64,
    detached_frames: u64,
    slow_frames_over_16ms: u64,
    slow_frames_over_33ms: u64,
    slow_frames_over_100ms: u64,
    pending_input: Option<Instant>,
}

impl TuiRuntimeTrace {
    pub(crate) fn from_env() -> Option<Self> {
        let enabled = std::env::var(ENABLED_ENV).ok()?;
        if !matches!(
            enabled.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ) {
            return None;
        }
        let path = std::env::var_os(PATH_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".omegon/debug/tui-runtime.jsonl"));
        let window = std::env::var(WINDOW_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(5));
        Some(Self {
            path,
            window,
            started: Instant::now(),
            build: BuildIdentity::current(),
            agent_events: 0,
            agent_drain_passes: 0,
            agent_budget_hits: 0,
            agent_events_per_drain: Vec::new(),
            operator_inputs: 0,
            input_batches: 0,
            inputs_per_batch: Vec::new(),
            frames: 0,
            urgent_frames: 0,
            background_frames: 0,
            dirty_passes_without_draw: 0,
            draw_us: Vec::new(),
            draw_callback_us: Vec::new(),
            backend_us: Vec::new(),
            preparation_us: Vec::new(),
            background_fill_us: Vec::new(),
            conversation_projection_us: Vec::new(),
            conversation_render_us: Vec::new(),
            remaining_render_us: Vec::new(),
            input_to_frame_us: Vec::new(),
            conversation_segments: Vec::new(),
            conversation_scroll_offset: Vec::new(),
            streaming_frames: 0,
            detached_frames: 0,
            slow_frames_over_16ms: 0,
            slow_frames_over_33ms: 0,
            slow_frames_over_100ms: 0,
            pending_input: None,
        })
    }

    pub(crate) fn record_input(&mut self, count: u64, now: Instant) {
        self.operator_inputs += count;
        self.input_batches += 1;
        self.inputs_per_batch.push(count);
        self.pending_input.get_or_insert(now);
    }

    pub(crate) fn record_agent_drain(&mut self, events: usize, hit_budget: bool) {
        self.agent_events += events as u64;
        self.agent_drain_passes += 1;
        self.agent_events_per_drain.push(events as u64);
        self.agent_budget_hits += u64::from(hit_budget);
    }

    pub(crate) fn record_dirty_without_draw(&mut self) {
        self.dirty_passes_without_draw += 1;
    }

    pub(crate) fn record_draw(
        &mut self,
        urgent: bool,
        elapsed: Duration,
        callback_elapsed: Duration,
        phases: DrawPhaseTimings,
        now: Instant,
        conversation_segments: usize,
        scroll_offset: u16,
        streaming: bool,
        detached: bool,
    ) {
        self.frames += 1;
        self.urgent_frames += u64::from(urgent);
        self.background_frames += u64::from(!urgent);
        let draw_us = elapsed.as_micros() as u64;
        self.draw_us.push(draw_us);
        self.draw_callback_us.push(micros(callback_elapsed));
        self.backend_us
            .push(micros(unmeasured(elapsed, callback_elapsed)));
        self.preparation_us.push(micros(phases.preparation));
        self.background_fill_us.push(micros(phases.background_fill));
        self.conversation_projection_us
            .push(micros(phases.conversation_projection));
        self.conversation_render_us
            .push(micros(phases.conversation_render));
        let measured_phases = phases
            .preparation
            .saturating_add(phases.background_fill)
            .saturating_add(phases.conversation_projection)
            .saturating_add(phases.conversation_render);
        self.remaining_render_us
            .push(micros(unmeasured(callback_elapsed, measured_phases)));
        self.slow_frames_over_16ms += u64::from(draw_us > 16_000);
        self.slow_frames_over_33ms += u64::from(draw_us > 33_000);
        self.slow_frames_over_100ms += u64::from(draw_us > 100_000);
        self.conversation_segments
            .push(conversation_segments as u64);
        self.conversation_scroll_offset.push(scroll_offset as u64);
        self.streaming_frames += u64::from(streaming);
        self.detached_frames += u64::from(detached);
        if let Some(input_at) = self.pending_input.take() {
            self.input_to_frame_us
                .push(now.saturating_duration_since(input_at).as_micros() as u64);
        }
    }

    pub(crate) fn flush_if_due(&mut self, now: Instant) {
        if now.duration_since(self.started) < self.window {
            return;
        }
        let record = TraceWindow {
            schema_version: 3,
            kind: "tuiPerformanceTraceWindow",
            generated_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            window_ms: now.duration_since(self.started).as_millis(),
            build: self.build.clone(),
            agent_events: self.agent_events,
            agent_drain_passes: self.agent_drain_passes,
            agent_budget_hits: self.agent_budget_hits,
            agent_events_per_drain: distribution(std::mem::take(&mut self.agent_events_per_drain)),
            operator_inputs: self.operator_inputs,
            input_batches: self.input_batches,
            inputs_per_batch: distribution(std::mem::take(&mut self.inputs_per_batch)),
            frames: self.frames,
            urgent_frames: self.urgent_frames,
            background_frames: self.background_frames,
            dirty_passes_without_draw: self.dirty_passes_without_draw,
            draw_us: distribution(std::mem::take(&mut self.draw_us)),
            draw_callback_us: distribution(std::mem::take(&mut self.draw_callback_us)),
            backend_us: distribution(std::mem::take(&mut self.backend_us)),
            preparation_us: distribution(std::mem::take(&mut self.preparation_us)),
            background_fill_us: distribution(std::mem::take(&mut self.background_fill_us)),
            conversation_projection_us: distribution(std::mem::take(
                &mut self.conversation_projection_us,
            )),
            conversation_render_us: distribution(std::mem::take(&mut self.conversation_render_us)),
            remaining_render_us: distribution(std::mem::take(&mut self.remaining_render_us)),
            input_to_frame_us: distribution(std::mem::take(&mut self.input_to_frame_us)),
            conversation_segments: distribution(std::mem::take(&mut self.conversation_segments)),
            conversation_scroll_offset: distribution(std::mem::take(
                &mut self.conversation_scroll_offset,
            )),
            streaming_frames: self.streaming_frames,
            detached_frames: self.detached_frames,
            slow_frames_over_16ms: self.slow_frames_over_16ms,
            slow_frames_over_33ms: self.slow_frames_over_33ms,
            slow_frames_over_100ms: self.slow_frames_over_100ms,
        };
        if let Some(parent) = self.path.parent() {
            let _ = create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            && let Ok(line) = serde_json::to_string(&record)
        {
            let _ = writeln!(file, "{line}");
        }
        self.reset_window(now);
    }

    fn reset_window(&mut self, now: Instant) {
        self.started = now;
        self.agent_events = 0;
        self.agent_drain_passes = 0;
        self.agent_budget_hits = 0;
        self.operator_inputs = 0;
        self.input_batches = 0;
        self.frames = 0;
        self.urgent_frames = 0;
        self.background_frames = 0;
        self.dirty_passes_without_draw = 0;
        self.streaming_frames = 0;
        self.detached_frames = 0;
        self.slow_frames_over_16ms = 0;
        self.slow_frames_over_33ms = 0;
        self.slow_frames_over_100ms = 0;
    }
}

fn unmeasured(total: Duration, measured: Duration) -> Duration {
    total.checked_sub(measured).unwrap_or_default()
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}

fn distribution(mut samples: Vec<u64>) -> Distribution {
    if samples.is_empty() {
        return Distribution::default();
    }
    samples.sort_unstable();
    let total = samples.iter().sum::<u64>();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).ceil() as usize];
    Distribution {
        samples: samples.len() as u64,
        total,
        mean: total / samples.len() as u64,
        p50: at(0.50),
        p95: at(0.95),
        p99: at(0.99),
        max: *samples.last().unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasured_reports_only_time_outside_measured_work() {
        assert_eq!(
            unmeasured(Duration::from_micros(90), Duration::from_micros(35)),
            Duration::from_micros(55)
        );
    }

    #[test]
    fn unmeasured_clamps_overlapping_measurements_to_zero() {
        assert_eq!(
            unmeasured(Duration::from_micros(20), Duration::from_micros(25)),
            Duration::ZERO
        );
    }

    #[test]
    fn distribution_preserves_sample_count_total_and_tail() {
        let summary = distribution(vec![10, 40, 20, 30]);
        assert_eq!(summary.samples, 4);
        assert_eq!(summary.total, 100);
        assert_eq!(summary.mean, 25);
        assert_eq!(summary.p50, 30);
        assert_eq!(summary.p95, 40);
        assert_eq!(summary.max, 40);
    }
}
