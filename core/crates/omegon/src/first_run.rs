//! First-run setup — interactive system sweep and configuration.
//!
//! Runs once on first launch (before the TUI) when no `~/.omegon/profile.json`
//! exists. Detects existing development tools on the system, offers sensible
//! defaults, and writes the initial profile so subsequent launches are instant.

use crate::settings::{self, PosturePreset, Profile};
use std::io::Write;
use std::path::Path;

/// Check whether first-run setup should run.
/// Returns true if no global profile exists (fresh install).
pub fn should_run(cwd: &Path) -> bool {
    // Skip for child processes
    if std::env::var("OMEGON_CHILD").is_ok() {
        return false;
    }
    // Skip if --fresh or other flags suggest non-interactive intent
    if std::env::args().any(|a| a == "--prompt" || a == "--prompt-file") {
        return false;
    }
    // First run = no global profile
    let has_global = dirs::home_dir()
        .map(|h| h.join(".omegon/profile.json").exists())
        .unwrap_or(false);
    // Also check project-level
    let has_project = crate::setup::find_project_root(cwd)
        .join(".omegon/profile.json")
        .exists();
    !has_global && !has_project
}

/// Run the interactive first-run setup. Prints to stderr (TUI hasn't taken over yet).
pub fn run_interactive(_cwd: &Path, shared_settings: &settings::SharedSettings) {
    let mut out = std::io::stderr();

    // Header
    let _ = writeln!(out, "\n\x1b[1m  Welcome to Omegon\x1b[0m\n");
    let _ = writeln!(out, "  First time here — let me set things up.\n");

    // ─── System sweep ──────────────────────────────────────────
    let sources = crate::migrate::detect_sources();
    let found: Vec<_> = sources.iter().filter(|(_, _, present)| *present).collect();

    if !found.is_empty() {
        let _ = writeln!(out, "  Found existing tools:");
        for (_, name, _) in &found {
            let _ = writeln!(out, "    \x1b[32m✓\x1b[0m {name}");
        }
        let _ = writeln!(out);
    }

    // ─── Detect Ollama ─────────────────────────────────────────
    let has_ollama = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        std::time::Duration::from_millis(200),
    )
    .is_ok();
    if has_ollama {
        let _ = writeln!(out, "    \x1b[32m✓\x1b[0m Ollama (local inference)");
        let _ = writeln!(out);
    }

    // ─── Recommend a starting posture ──────────────────────────
    // Map detected tools to a recommendation
    let has_ide_tool = found
        .iter()
        .any(|(id, _, _)| matches!(*id, "cursor" | "copilot" | "continue" | "windsurf"));
    let has_cli_tool = found
        .iter()
        .any(|(id, _, _)| matches!(*id, "claude-code" | "aider" | "codex"));

    let recommended = if has_cli_tool {
        // Coming from CLI coding agents — they know the terminal loop
        PosturePreset::Fabricator
    } else if has_ide_tool {
        // Coming from IDE extensions — start lean
        PosturePreset::Fabricator
    } else if has_ollama {
        // Has local models — Architect can delegate to them
        PosturePreset::Architect
    } else {
        // Fresh start — Fabricator is the safe middle ground
        PosturePreset::Fabricator
    };

    let _ = writeln!(out, "  How would you like to work?\n");

    let options: [(PosturePreset, &str, &str); 4] = [
        (
            PosturePreset::Fabricator,
            "Fabricator",
            "balanced coding agent — direct execution, delegates larger tasks",
        ),
        (
            PosturePreset::Architect,
            "Architect",
            "orchestrator — plans, delegates to local models, reviews results",
        ),
        (
            PosturePreset::Explorator,
            "Explorator",
            "lean terminal loop — fast, minimal, like a simple coding CLI",
        ),
        (
            PosturePreset::Devastator,
            "Devastator",
            "maximum force — deep reasoning, large context, full power",
        ),
    ];

    for (i, (preset, name, desc)) in options.iter().enumerate() {
        let marker = if *preset == recommended {
            " (recommended)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "    \x1b[1m[{}]\x1b[0m {} — {}{marker}",
            i + 1,
            name,
            desc
        );
    }
    let _ = writeln!(out);

    let _ = write!(out, "  Choice [1]: ");
    let _ = out.flush();

    let choice = read_choice();

    let posture = match choice.trim() {
        "2" => options[1].0,
        "3" => options[2].0,
        "4" => options[3].0,
        _ => options[0].0, // default: option 1 (Fabricator)
    };

    let posture_name = match posture {
        PosturePreset::Explorator => "explorator",
        PosturePreset::Fabricator => "fabricator",
        PosturePreset::Architect => "architect",
        PosturePreset::Devastator => "devastator",
    };

    // ─── Apply and persist ─────────────────────────────────────
    if let Ok(mut s) = shared_settings.lock() {
        s.set_posture(posture);
    }

    // Write the global profile
    let profile = Profile {
        default_posture: Some(posture_name.to_string()),
        ..Profile::default()
    };
    if let Err(e) = profile.save_global() {
        tracing::warn!("failed to save global profile: {e}");
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  \x1b[32m✓\x1b[0m Starting in \x1b[1m{posture_name}\x1b[0m mode."
    );
    let _ = writeln!(
        out,
        "    Change anytime: \x1b[2m--architect, --fabricator, --explorator, --devastator\x1b[0m"
    );
    let _ = writeln!(
        out,
        "    Or set in \x1b[2m~/.omegon/profile.json\x1b[0m → \x1b[2m\"defaultPosture\": \"...\"\x1b[0m"
    );

    // ─── Auth check ────────────────────────────────────────────
    let has_provider = std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("OPENROUTER_API_KEY").is_ok()
        || crate::auth::any_oauth_token_exists();
    if !has_provider {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  \x1b[33m⚠\x1b[0m  No LLM provider detected. You'll need one before your first session."
        );
        let _ = writeln!(
            out,
            "    \x1b[2mRun:\x1b[0m  omegon auth login anthropic   \x1b[2m(OAuth — recommended)\x1b[0m"
        );
        let _ = writeln!(
            out,
            "    \x1b[2m  or:\x1b[0m  export ANTHROPIC_API_KEY=sk-ant-..."
        );
    }

    // ─── Auto-migrate if sources detected ──────────────────────
    if !found.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  \x1b[2mTip: run \x1b[0momegon migrate auto\x1b[2m to import settings from your existing tools.\x1b[0m"
        );
    }

    let _ = writeln!(out);
}

fn read_choice() -> String {
    let mut input = String::new();
    // Read from stdin (terminal is still in normal mode)
    let _ = std::io::stdin().read_line(&mut input);
    input
}
