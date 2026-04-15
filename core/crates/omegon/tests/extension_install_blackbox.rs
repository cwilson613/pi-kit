//! Blackbox integration tests for extension installation.
//!
//! Local fixture tests run unconditionally — they exercise install, list,
//! enable, disable, and remove using an in-repo fixture extension.
//!
//! GitHub clone tests (vox, scribe-rpc) require network access and are
//! gated behind OMEGON_RUN_EXTENSION_INSTALL_TESTS=1.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────

fn resolve_omegon_binary() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_omegon") {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe().context("current_exe")?;
    let deps_dir = current
        .parent()
        .context("integration test executable has no parent")?;
    let debug_dir = deps_dir.parent().context("deps dir has no parent")?;
    let candidate = debug_dir.join(if cfg!(windows) {
        "omegon.exe"
    } else {
        "omegon"
    });
    if candidate.is_file() {
        return Ok(candidate);
    }
    anyhow::bail!(
        "unable to locate omegon binary: CARGO_BIN_EXE_omegon unset and {} missing",
        candidate.display()
    )
}

fn omegon(bin: &Path, home: &TempDir, args: &[&str]) -> Result<std::process::Output> {
    Command::new(bin)
        .args(args)
        .env("OMEGON_HOME", home.path())
        .env("OMEGON_NO_KEYRING", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("failed to run omegon {}", args.join(" ")))
}

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock-extension")
}

fn github_tests_enabled() -> bool {
    std::env::var("OMEGON_RUN_EXTENSION_INSTALL_TESTS").is_ok()
}

// ── Local fixture tests (always run in CI) ───────────────────────────────

#[test]
fn install_local_extension() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");
    let fixture = fixture_path();

    let out = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("install");
    assert!(
        out.status.success(),
        "install failed: stdout={} stderr={}",
        stdout(&out),
        stderr(&out),
    );
    let text = stdout(&out);
    assert!(
        text.contains("mock-extension"),
        "expected extension name in output, got: {text}"
    );

    // Verify symlink was created (local install uses symlink)
    let ext_dir = home.path().join("extensions/mock-extension");
    assert!(ext_dir.exists(), "extension dir should exist");
    assert!(
        ext_dir.is_symlink(),
        "local install should create a symlink"
    );
}

#[test]
fn list_shows_installed_extension() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");
    let fixture = fixture_path();

    // Install first
    let inst = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("install");
    assert!(inst.status.success());

    // List
    let out = omegon(&bin, &home, &["extension", "list"]).expect("list");
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("mock-extension"),
        "expected mock-extension in list, got: {text}"
    );
    assert!(
        text.contains("0.1.0"),
        "expected version in list, got: {text}"
    );
    assert!(
        text.contains("native"),
        "expected runtime type in list, got: {text}"
    );
}

#[test]
fn disable_and_enable_extension() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");
    let fixture = fixture_path();

    // Install
    let inst = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("install");
    assert!(inst.status.success());

    // Disable
    let dis = omegon(&bin, &home, &["extension", "disable", "mock-extension"])
        .expect("disable");
    assert!(dis.status.success());
    let dis_text = stdout(&dis);
    assert!(
        dis_text.contains("Disabled"),
        "expected disabled confirmation, got: {dis_text}"
    );

    // List should show disabled status
    let list = omegon(&bin, &home, &["extension", "list"]).expect("list");
    let list_text = stdout(&list);
    assert!(
        list_text.contains("disabled"),
        "expected disabled status in list, got: {list_text}"
    );

    // Enable
    let en = omegon(&bin, &home, &["extension", "enable", "mock-extension"])
        .expect("enable");
    assert!(en.status.success());
    let en_text = stdout(&en);
    assert!(
        en_text.contains("Enabled"),
        "expected enabled confirmation, got: {en_text}"
    );
}

#[test]
fn remove_installed_extension() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");
    let fixture = fixture_path();

    // Install
    let inst = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("install");
    assert!(inst.status.success());

    // Remove
    let rm = omegon(&bin, &home, &["extension", "remove", "mock-extension"]).expect("remove");
    assert!(rm.status.success());
    assert!(
        !home.path().join("extensions/mock-extension").exists(),
        "extension dir should be gone after remove"
    );

    // List should show empty
    let list = omegon(&bin, &home, &["extension", "list"]).expect("list");
    let text = stdout(&list);
    assert!(
        text.contains("No extensions installed"),
        "expected empty after remove, got: {text}"
    );
}

#[test]
fn install_duplicate_fails() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");
    let fixture = fixture_path();

    let inst1 = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("first install");
    assert!(inst1.status.success());

    let inst2 = omegon(&bin, &home, &["extension", "install", fixture.to_str().unwrap()])
        .expect("second install");
    assert!(!inst2.status.success(), "duplicate install should fail");
    let combined = format!("{}{}", stdout(&inst2), stderr(&inst2));
    assert!(
        combined.contains("already"),
        "expected 'already installed' error, got: {combined}"
    );
}

#[test]
fn list_empty_home() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");

    let out = omegon(&bin, &home, &["extension", "list"]).expect("list");
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(
        text.contains("No extensions installed"),
        "expected empty list, got: {text}"
    );
}

#[test]
fn remove_nonexistent_fails() {
    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");

    let out = omegon(&bin, &home, &["extension", "remove", "ghost"]).expect("remove");
    assert!(!out.status.success());
    let combined = format!("{}{}", stdout(&out), stderr(&out));
    assert!(
        combined.contains("not found"),
        "expected 'not found' error, got: {combined}"
    );
}

// ── GitHub clone tests (gated: OMEGON_RUN_EXTENSION_INSTALL_TESTS=1) ──

#[test]
fn github_install_vox() {
    if !github_tests_enabled() {
        eprintln!("skipping: set OMEGON_RUN_EXTENSION_INSTALL_TESTS=1 to run");
        return;
    }

    let bin = resolve_omegon_binary().expect("omegon binary");
    let home = tempfile::tempdir().expect("tempdir");

    let out = omegon(
        &bin,
        &home,
        &["extension", "install", "https://github.com/styrene-lab/vox.git"],
    )
    .expect("install");
    assert!(
        out.status.success(),
        "vox install failed: stdout={} stderr={}",
        stdout(&out),
        stderr(&out),
    );

    // Verify in list
    let list = omegon(&bin, &home, &["extension", "list"]).expect("list");
    assert!(stdout(&list).contains("vox"));

    // Verify manifest
    let manifest = home.path().join("extensions/vox/manifest.toml");
    assert!(manifest.exists(), "vox manifest.toml not found");
    let content = std::fs::read_to_string(&manifest).expect("read");
    assert!(content.contains("[extension]"));

    // Clean up
    let rm = omegon(&bin, &home, &["extension", "remove", "vox"]).expect("remove");
    assert!(rm.status.success());
    assert!(!home.path().join("extensions/vox").exists());
}
