//! Startup and replay splash orchestration for the native TUI.
//!
//! Terminal ownership remains in `run_tui`. This module owns probe lifecycle,
//! animation timing, dismissal policy, capability classification, and preserving
//! agent events received while startup presentation is active.

use super::*;
use std::time::Instant;

const PROBE_LABELS: [&str; 9] = [
    "cloud",
    "local",
    "hardware",
    "memory",
    "tools",
    "design",
    "secrets",
    "container",
    "mcp",
];
const STARTUP_DEADLINE: Duration = Duration::from_secs(3);
const AUTO_DISMISS_HOLD: u32 = splash::HOLD_FRAMES + 6;
const REPLAY_AUTO_DISMISS_FRAME: u32 = splash::TOTAL_FRAMES + splash::HOLD_FRAMES + 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplashMode {
    Startup,
    Replay,
}

#[derive(Debug, Default)]
struct StartupSplashOutcome {
    probes: Vec<crate::startup::ProbeResult>,
    deferred_events: Vec<AgentEvent>,
    timed_out: bool,
}

fn should_dismiss_for_input(event: &Event, ready: bool, elapsed: Duration) -> bool {
    matches!(event, Event::Key(_) | Event::Mouse(_))
        && (ready || elapsed >= Duration::from_millis(300))
}

fn should_auto_dismiss(mode: SplashMode, splash: &splash::SplashScreen) -> bool {
    match mode {
        SplashMode::Startup => splash.ready_to_dismiss() && splash.hold_count >= AUTO_DISMISS_HOLD,
        SplashMode::Replay => splash.frame >= REPLAY_AUTO_DISMISS_FRAME,
    }
}

fn drain_probe_results(
    splash: &mut splash::SplashScreen,
    rx: &std::sync::mpsc::Receiver<crate::startup::ProbeResult>,
    results: &mut Vec<crate::startup::ProbeResult>,
) {
    while let Ok(result) = rx.try_recv() {
        splash.receive_probe(result.clone());
        results.push(result);
    }
}

fn drain_startup_agent_events(
    app: &mut App,
    events_rx: &mut broadcast::Receiver<AgentEvent>,
    deferred: &mut Vec<AgentEvent>,
) {
    while let Ok(event) = events_rx.try_recv() {
        match event {
            AgentEvent::HarnessStatusChanged { status_json } => {
                if let Ok(status) =
                    serde_json::from_value::<crate::status::HarnessStatus>(status_json)
                {
                    app.footer_data.update_harness(status);
                }
            }
            event => deferred.push(event),
        }
    }
}

fn spawn_probe_worker(cwd: String) -> std::sync::mpsc::Receiver<crate::startup::ProbeResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => runtime.block_on(crate::startup::run_probes(tx, cwd)),
            Err(error) => {
                tracing::warn!(%error, "startup probe runtime unavailable");
            }
        }
    });
    rx
}

async fn run_splash<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut events_rx: Option<&mut broadcast::Receiver<AgentEvent>>,
    mode: SplashMode,
    cwd: Option<String>,
) -> io::Result<StartupSplashOutcome>
where
    B::Error: Into<io::Error> + Send + Sync + 'static,
{
    let size = terminal.size().map_err(Into::into)?;
    let Some(mut splash) = splash::SplashScreen::new(size.width, size.height) else {
        return Ok(StartupSplashOutcome::default());
    };

    let probe_rx = if mode == SplashMode::Startup {
        for label in PROBE_LABELS {
            splash.set_load_state(label, splash::LoadState::Active);
        }
        cwd.map(spawn_probe_worker)
    } else {
        splash.force_done();
        None
    };

    let started = Instant::now();
    let mut outcome = StartupSplashOutcome::default();

    loop {
        let theme = &app.theme;
        terminal
            .draw(|frame| splash.draw(frame, theme.as_ref()))
            .map_err(Into::into)?;
        if splash.is_dissolved() {
            break;
        }

        let interval = splash::SplashScreen::frame_interval();
        if event::poll(interval)? {
            let input = event::read()?;
            if should_dismiss_for_input(&input, splash.ready_to_dismiss(), started.elapsed()) {
                splash.dismiss();
            }
        }

        splash.tick();
        if let Some(rx) = probe_rx.as_ref() {
            drain_probe_results(&mut splash, rx, &mut outcome.probes);
        }
        if let Some(rx) = events_rx.as_deref_mut() {
            drain_startup_agent_events(app, rx, &mut outcome.deferred_events);
        }

        if mode == SplashMode::Startup
            && started.elapsed() >= STARTUP_DEADLINE
            && !splash.ready_to_dismiss()
        {
            outcome.timed_out = true;
            splash.fail_unfinished("startup probe timed out");
            splash.dismiss();
        } else if should_auto_dismiss(mode, &splash) {
            splash.dismiss();
        }
    }

    if let Some(rx) = probe_rx.as_ref() {
        drain_probe_results(&mut splash, rx, &mut outcome.probes);
    }
    if let Some(rx) = events_rx {
        drain_startup_agent_events(app, rx, &mut outcome.deferred_events);
    }
    Ok(outcome)
}

pub(super) async fn run_startup_splash<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    events_rx: &mut broadcast::Receiver<AgentEvent>,
    cwd: String,
) -> io::Result<()>
where
    B::Error: Into<io::Error> + Send + Sync + 'static,
{
    let outcome = run_splash(
        terminal,
        app,
        Some(events_rx),
        SplashMode::Startup,
        Some(cwd),
    )
    .await?;
    app.capability_grade = Some(crate::startup::classify_tier(&outcome.probes));
    for event in outcome.deferred_events {
        app.handle_agent_event(event);
    }
    if outcome.timed_out {
        tracing::warn!(
            completed = outcome.probes.len(),
            expected = PROBE_LABELS.len(),
            "startup capability inspection timed out"
        );
    }
    Ok(())
}

pub(super) async fn run_replay_splash<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()>
where
    B::Error: Into<io::Error> + Send + Sync + 'static,
{
    run_splash(terminal, app, None, SplashMode::Replay, None)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_input_waits_for_initial_animation_window() {
        let key = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!should_dismiss_for_input(
            &key,
            false,
            Duration::from_millis(299)
        ));
        assert!(should_dismiss_for_input(
            &key,
            false,
            Duration::from_millis(300)
        ));
    }

    #[test]
    fn startup_input_accepts_mouse_consistently_with_replay() {
        let mouse = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        assert!(should_dismiss_for_input(&mouse, true, Duration::ZERO));
    }

    #[test]
    fn unrelated_terminal_events_do_not_dismiss() {
        assert!(!should_dismiss_for_input(
            &Event::Resize(80, 24),
            true,
            Duration::from_secs(1)
        ));
    }
}
