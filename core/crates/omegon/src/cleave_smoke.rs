use crate::cleave;
use crate::Cli;
use anyhow::Context;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Deterministic cleave smoke scenarios.
///
/// Each scenario injects a child outcome via `OMEGON_CLEAVE_SMOKE_CHILD_MODE`,
/// runs the orchestrator against a disposable git repo, and asserts the
/// resulting status summary and merge-result line are correct.
///
/// Adding a new regression:
/// 1. Add a `SmokeScenario` entry below.
/// 2. Describe `child_mode` — either an existing mode or add a new one to
///    `maybe_run_injected_cleave_smoke_child()` in `main.rs`.
/// 3. Set the expected `status_line` and `merge_line` substrings.
///
/// Child modes (from `maybe_run_injected_cleave_smoke_child`):
///   upstream-exhausted  — exits 2 (provider exhaustion)
///   fail                — exits 1 (logic failure)
///   success-noop        — exits 0, no file writes
///   success-dirty       — exits 0, writes OMEGON_CLEAVE_SMOKE_WRITE_FILE
struct SmokeScenario {
    name: &'static str,
    child_mode: &'static str,
    write_file: Option<&'static str>,
    expect_exit_ok: bool,
    expect_status_line: &'static str,
    expect_merge_line: &'static str,
}

pub async fn run(cli: &Cli) -> anyhow::Result<()> {
    let scenarios = vec![
        SmokeScenario {
            name: "upstream_exhausted_no_changes",
            child_mode: "upstream-exhausted",
            write_file: None,
            expect_exit_ok: false,
            expect_status_line: "0 completed, 0 failed, 1 upstream exhausted, 0 unfinished",
            expect_merge_line: "upstream exhausted (no repo changes to merge)",
        },
        SmokeScenario {
            name: "failed_no_changes",
            child_mode: "fail",
            write_file: None,
            expect_exit_ok: false,
            expect_status_line: "0 completed, 1 failed, 0 upstream exhausted, 0 unfinished",
            expect_merge_line: "failed (no repo changes to merge)",
        },
        SmokeScenario {
            name: "completed_no_changes",
            child_mode: "success-noop",
            write_file: None,
            expect_exit_ok: true,
            expect_status_line: "1 completed, 0 failed, 0 upstream exhausted, 0 unfinished",
            expect_merge_line: "completed (no changes)",
        },
        SmokeScenario {
            name: "completed_with_merge",
            child_mode: "success-dirty",
            write_file: Some("README.md"),
            expect_exit_ok: true,
            expect_status_line: "1 completed, 0 failed, 0 upstream exhausted, 0 unfinished",
            expect_merge_line: "merged",
        },
    ];

    eprintln!("omegon {} — cleave smoke test mode", env!("CARGO_PKG_VERSION"));
    eprintln!("Running {} deterministic cleave smoke scenario(s)...", scenarios.len());

    let mut failed = 0usize;
    for scenario in &scenarios {
        eprint!("  {:<32} ", scenario.name);
        match run_scenario(cli, scenario).await {
            Ok(()) => eprintln!("✓ pass"),
            Err(e) => {
                failed += 1;
                eprintln!("✗ FAIL: {e:#}");
            }
        }
    }

    if failed > 0 {
        anyhow::bail!("cleave smoke suite failed: {failed} scenario(s) failed");
    }

    eprintln!("✓ all deterministic cleave smoke scenarios passed");
    Ok(())
}

async fn run_scenario(cli: &Cli, scenario: &SmokeScenario) -> anyhow::Result<()> {
    let temp_dir = std::env::temp_dir()
        .join(format!("omegon-cleave-smoke-{}-{}", std::process::id(), scenario.name));
    let repo = temp_dir.join("repo");
    let workspace = temp_dir.join("workspace");
    std::fs::create_dir_all(&workspace)?;
    init_repo(&repo)?;

    // temp_dir is PID-namespaced; best-effort cleanup on success
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    let _cleanup = Cleanup(temp_dir.clone());

    let plan_json = r#"{
  "children": [
    {
      "label": "smoke-child",
      "description": "Deterministic cleave smoke child.",
      "scope": ["README.md"],
      "depends_on": []
    }
  ],
  "rationale": "deterministic cleave smoke"
}
"#;
    let plan: cleave::CleavePlan = serde_json::from_str(plan_json)?;
    let injected_env = scenario_env(scenario);
    let config = cleave::orchestrator::CleaveConfig {
        agent_binary: std::env::current_exe()?,
        bridge_path: PathBuf::new(),
        node: String::new(),
        model: cli.model.clone(),
        max_parallel: 1,
        timeout_secs: 30,
        idle_timeout_secs: 10,
        max_turns: 2,
        inventory: None,
        inherited_env: vec![],
        injected_env,
        progress_sink: cleave::progress::stdout_progress_sink(),
    };

    let result = cleave::run_cleave(
        &plan,
        "deterministic cleave smoke",
        &repo,
        &workspace,
        &config,
        CancellationToken::new(),
    )
    .await?;

    let (completed, failed, upstream_exhausted, unfinished) =
        crate::summarize_cleave_child_statuses(&result.state.children);
    let status_line = format!(
        "{completed} completed, {failed} failed, {upstream_exhausted} upstream exhausted, {unfinished} unfinished"
    );
    if !status_line.contains(scenario.expect_status_line) {
        anyhow::bail!(
            "status summary mismatch: expected {:?}, got {:?}",
            scenario.expect_status_line,
            status_line
        );
    }

    let merge_line = crate::format_cleave_merge_result(
        result.state.children.first(),
        "smoke-child",
        &result.merge_results[0].1,
    );
    if !merge_line.contains(scenario.expect_merge_line) {
        anyhow::bail!(
            "merge line mismatch: expected {:?}, got {:?}",
            scenario.expect_merge_line,
            merge_line
        );
    }

    let terminal_ok = failed == 0 && upstream_exhausted == 0 && unfinished == 0;
    if terminal_ok != scenario.expect_exit_ok {
        anyhow::bail!(
            "terminal success mismatch: expected {}, got status summary {:?}",
            scenario.expect_exit_ok,
            status_line
        );
    }

    Ok(())
}

fn scenario_env(scenario: &SmokeScenario) -> Vec<(String, String)> {
    let mut vars = vec![(
        "OMEGON_CLEAVE_SMOKE_CHILD_MODE".to_string(),
        scenario.child_mode.to_string(),
    )];
    if let Some(path) = scenario.write_file {
        vars.push((
            "OMEGON_CLEAVE_SMOKE_WRITE_FILE".to_string(),
            path.to_string(),
        ));
    }
    vars
}

fn init_repo(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)?;
    run_git(path, ["init", "-q"])?;
    run_git(path, ["config", "user.email", "smoke@example.com"])?;
    run_git(path, ["config", "user.name", "Smoke Test"])?;
    std::fs::write(path.join("README.md"), "hello smoke\n")?;
    run_git(path, ["add", "README.md"])?;
    run_git(path, ["commit", "-qm", "init"])?;
    Ok(())
}

fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> anyhow::Result<()> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to run git {:?}", args))?;
    if !status.success() {
        anyhow::bail!("git {:?} failed with status {status}", args);
    }
    Ok(())
}
