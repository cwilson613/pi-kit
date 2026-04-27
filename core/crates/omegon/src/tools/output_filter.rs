//! Tool output filtering — command-aware compression of noisy output.
//!
//! Domain-specific patterns strip noise (progress bars, repeated compile
//! lines, download status) while preserving actionable content (errors,
//! warnings, summaries). Called from the bash tool and available to any
//! tool that produces verbose output.
//!
//! Inspired by PandaFilter's handler patterns, implemented natively.

/// Minimum lines before compression is attempted.
const MIN_LINES: usize = 10;

/// Filter tool output based on the command that produced it.
/// Returns the filtered output, or the original if no patterns matched.
pub(crate) fn filter_tool_output(command: &str, output: &str) -> String {
    let cmd = command.trim_start();
    let lines: Vec<&str> = output.lines().collect();

    if lines.len() < MIN_LINES {
        return output.to_string();
    }

    // Try command-specific filters first
    if let Some(result) = filter_cargo_build(cmd, &lines) {
        return result;
    }
    if let Some(result) = filter_cargo_test(cmd, &lines) {
        return result;
    }
    if let Some(result) = filter_npm_install(cmd, &lines) {
        return result;
    }
    if let Some(result) = filter_pip_install(cmd, &lines) {
        return result;
    }
    if let Some(result) = filter_progress_bars(&lines) {
        return result;
    }

    output.to_string()
}

// ── Cargo build/check/clippy ───────────────────────────────────────────

fn filter_cargo_build(cmd: &str, lines: &[&str]) -> Option<String> {
    let is_cargo = cmd.starts_with("cargo build")
        || cmd.starts_with("cargo check")
        || cmd.starts_with("cargo clippy");
    // Also match RUSTFLAGS=... cargo ... (env prefix before cargo)
    let is_rustflags_cargo = cmd.starts_with("RUSTFLAGS") && cmd.contains("cargo");

    if !is_cargo && !is_rustflags_cargo {
        return None;
    }

    let compiling: Vec<&&str> = lines.iter().filter(|l| l.contains("Compiling ")).collect();
    let important: Vec<&str> = lines
        .iter()
        .filter(|l| {
            !l.contains("Compiling ")
                && !l.contains("Downloading ")
                && !l.contains("Downloaded ")
                && !l.trim().is_empty()
        })
        .copied()
        .collect();

    if compiling.len() > 3 && important.len() < lines.len() {
        let mut result = Vec::new();
        if !compiling.is_empty() {
            result.push(format!("[compiled {} crates]", compiling.len()));
        }
        result.extend(important.iter().map(|s| s.to_string()));
        Some(result.join("\n"))
    } else {
        None
    }
}

// ── Cargo test ─────────────────────────────────────────────────────────

fn filter_cargo_test(cmd: &str, lines: &[&str]) -> Option<String> {
    if !cmd.starts_with("cargo test") {
        return None;
    }

    let mut summary_lines = Vec::new();
    let mut test_count = 0usize;
    let mut pass_count = 0usize;
    let mut fail_lines = Vec::new();
    let mut in_summary = false;

    for line in lines {
        if line.starts_with("test ") && line.contains(" ... ") {
            test_count += 1;
            if line.contains("... ok") {
                pass_count += 1;
            } else {
                fail_lines.push(line.to_string());
            }
        } else if line.starts_with("test result:")
            || line.starts_with("failures:")
            || line.contains("FAILED")
            || line.contains("running ")
        {
            in_summary = true;
            summary_lines.push(line.to_string());
        } else if in_summary || line.starts_with("error") || line.starts_with("warning") {
            summary_lines.push(line.to_string());
        }
    }

    if test_count > 5 {
        let mut result = Vec::new();
        if pass_count > 0 {
            result.push(format!("[{pass_count} tests passed]"));
        }
        result.extend(fail_lines);
        result.extend(summary_lines);
        Some(result.join("\n"))
    } else {
        None
    }
}

// ── npm/yarn/pnpm install ──────────────────────────────────────────────

fn filter_npm_install(cmd: &str, lines: &[&str]) -> Option<String> {
    let is_npm = cmd.starts_with("npm install")
        || cmd.starts_with("npm i")
        || cmd.starts_with("yarn install")
        || cmd.starts_with("yarn add")
        || cmd.starts_with("pnpm install")
        || cmd.starts_with("pnpm add");

    if !is_npm {
        return None;
    }

    let important: Vec<&str> = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("npm ")
                && !t.starts_with("⸩")
                && !t.contains("idealTree")
                && !t.contains("reify")
                && !t.contains("timing ")
                && !t.contains("http fetch")
                && (t.contains("added")
                    || t.contains("removed")
                    || t.contains("packages")
                    || t.contains("audited")
                    || t.contains("WARN")
                    || t.contains("ERR!")
                    || t.contains("vulnerabilit")
                    || t.contains("up to date")
                    || t.contains("peer dep"))
        })
        .copied()
        .collect();

    if important.len() < lines.len() / 2 {
        Some(important.join("\n"))
    } else {
        None
    }
}

// ── pip install ────────────────────────────────────────────────────────

fn filter_pip_install(cmd: &str, lines: &[&str]) -> Option<String> {
    if !cmd.starts_with("pip install") && !cmd.starts_with("pip3 install") {
        return None;
    }

    let important: Vec<&str> = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("Downloading ")
                && !t.starts_with("  Downloading")
                && !t.contains("━")
                && !t.contains("██")
                && !t.contains("Collecting ")
                && !t.starts_with("  Using cached")
        })
        .copied()
        .collect();

    if important.len() < lines.len() / 2 {
        Some(important.join("\n"))
    } else {
        None
    }
}

// ── Generic progress bar stripping ─────────────────────────────────────

const PROGRESS_CHARS: &[char] = &['█', '▓', '▒', '░', '━', '─', '⣿', '⠿', '⠋', '⠙', '⠹', '⠸'];

fn filter_progress_bars(lines: &[&str]) -> Option<String> {
    let progress_count = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            t.len() > 5
                && (PROGRESS_CHARS.iter().any(|c| t.contains(*c))
                    || (t.contains('%') && t.contains('/'))
                    || t.starts_with('\r'))
        })
        .count();

    if progress_count <= lines.len() / 3 {
        return None;
    }

    let filtered: Vec<&str> = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !PROGRESS_CHARS.iter().any(|c| t.contains(*c))
                && !(t.contains('%') && t.contains('/') && t.len() < 80)
                && !t.starts_with('\r')
        })
        .copied()
        .collect();

    if filtered.len() < lines.len() {
        let stripped = lines.len() - filtered.len();
        let mut out: Vec<String> = filtered.iter().map(|s| s.to_string()).collect();
        out.push(String::new());
        out.push(format!("[{stripped} progress lines stripped]"));
        Some(out.join("\n"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_build_collapses_compiling() {
        let output = (0..20)
            .map(|i| format!("   Compiling crate-{i} v0.1.0"))
            .chain(std::iter::once("    Finished `dev` profile in 4.2s".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        let filtered = filter_tool_output("cargo build", &output);
        assert!(filtered.contains("[compiled 20 crates]"));
        assert!(filtered.contains("Finished"));
        assert!(filtered.lines().count() < 5);
    }

    #[test]
    fn cargo_test_keeps_failures() {
        let mut lines = vec!["running 15 tests".to_string()];
        for i in 0..12 {
            lines.push(format!("test test_{i} ... ok"));
        }
        lines.push("test test_bad ... FAILED".to_string());
        lines.push("test test_also_bad ... FAILED".to_string());
        lines.push("test result: FAILED. 12 passed; 2 failed; 0 ignored".to_string());
        let output = lines.join("\n");
        let filtered = filter_tool_output("cargo test", &output);
        assert!(filtered.contains("12 tests passed"));
        assert!(filtered.contains("test_bad"));
        assert!(filtered.contains("FAILED"));
    }

    #[test]
    fn short_output_unchanged() {
        let output = "line1\nline2\nline3";
        assert_eq!(filter_tool_output("ls -la", output), output);
    }

    #[test]
    fn rustflags_cargo_is_caught() {
        let output = (0..15)
            .map(|i| format!("   Compiling crate-{i} v0.1.0"))
            .chain(std::iter::once("    Finished `dev` profile in 2s".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        let filtered = filter_tool_output("RUSTFLAGS=\"-D warnings\" cargo check -p omegon", &output);
        assert!(filtered.contains("[compiled 15 crates]"));
    }
}
