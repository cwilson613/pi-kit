#![cfg(unix)]

use std::{fs, process::Command};

use omegon_maintenance_contracts::{
    InstallationStateV1, MaintenanceStateV1, canonical_json, open_secure_root, parse_record,
    path_identity,
};
use serde_json::Value;

struct Fixture {
    root: tempfile::TempDir,
    home: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Fixture {
    fn mismatched() -> Self {
        let fixture = Self::bound();
        fixture.renumber();
        fixture
    }

    fn bound() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let config = root.path().join("config");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&config).unwrap();
        let descriptor = open_secure_root(&home).unwrap();
        let state = MaintenanceStateV1::bootstrap(
            &descriptor,
            path_identity(&descriptor).unwrap(),
            "11111111-1111-4111-8111-111111111111",
            false,
        )
        .unwrap();
        drop(state);
        Self { root, home, config }
    }

    fn renumber(&self) {
        // Model an installation created before stable continuity enrollment.
        let binding = self.home.join("maintain/v1/home-continuity.json");
        if binding.exists() {
            fs::remove_file(binding).unwrap();
        }
        let mut installation: InstallationStateV1 = parse_record(&self.state_bytes()).unwrap();
        installation.home.device ^= 2;
        fs::write(
            self.home.join("maintain/v1/state.json"),
            canonical_json(&installation).unwrap(),
        )
        .unwrap();
    }

    fn state_bytes(&self) -> Vec<u8> {
        fs::read(self.home.join("maintain/v1/state.json")).unwrap()
    }

    fn run(&self, args: &[&str]) -> (i32, Value) {
        let binary = env!("CARGO_BIN_EXE_omegon-maintain");
        let output = Command::new(binary)
            .args(["--json", "--home"])
            .arg(&self.home)
            .arg("--config-home")
            .arg(&self.config)
            .args(args)
            .current_dir(self.root.path())
            .env_clear()
            .env("HOME", self.root.path())
            .output()
            .unwrap();
        let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{error}: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output.status.code().unwrap_or(-1), value)
    }
}

fn has_diagnostic(value: &Value, code: &str) -> bool {
    value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == code)
}

#[test]
fn home_inspection_reports_mismatch_without_changing_authority() {
    let fixture = Fixture::mismatched();
    let before = fixture.state_bytes();
    let (_, result) = fixture.run(&["home", "inspect"]);
    assert_eq!(result["command"], "home.inspect", "{result}");
    assert_eq!(result["status"], "degraded", "{result}");
    assert!(
        has_diagnostic(&result, "home_identity_mismatch"),
        "{result}"
    );
    assert_eq!(fixture.state_bytes(), before);
}

#[test]
fn home_recovery_requires_a_bounded_deadline() {
    let fixture = Fixture::mismatched();
    let before = fixture.state_bytes();
    let (code, result) = fixture.run(&["home", "recover"]);
    assert_ne!(code, 0);
    assert_eq!(result["errors"][0]["code"], "deadline_required", "{result}");
    assert_eq!(fixture.state_bytes(), before);
}

#[test]
fn home_recovery_dry_run_does_not_rebind() {
    let fixture = Fixture::mismatched();
    let before = fixture.state_bytes();
    let (code, result) = fixture.run(&["home", "recover", "--dry-run", "--deadline", "10s"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(fixture.state_bytes(), before);
    assert!(
        MaintenanceStateV1::bootstrap(
            &open_secure_root(&fixture.home).unwrap(),
            path_identity(&open_secure_root(&fixture.home).unwrap()).unwrap(),
            "22222222-2222-4222-8222-222222222222",
            true,
        )
        .is_err()
    );
}

#[test]
fn home_recovery_repairs_admission_and_replays_without_new_identity() {
    let fixture = Fixture::mismatched();
    let before: InstallationStateV1 = parse_record(&fixture.state_bytes()).unwrap();
    let args = [
        "home",
        "recover",
        "--deadline",
        "10s",
        "--request-id",
        "33333333-3333-4333-8333-333333333333",
    ];
    let (code, result) = fixture.run(&args);
    assert_eq!(code, 0, "{result}");
    let descriptor = open_secure_root(&fixture.home).unwrap();
    let state = MaintenanceStateV1::bootstrap(
        &descriptor,
        path_identity(&descriptor).unwrap(),
        "44444444-4444-4444-8444-444444444444",
        true,
    )
    .unwrap();
    assert_eq!(
        state.installation.installation_uuid,
        before.installation_uuid
    );
    assert_eq!(state.installation.record_id, before.record_id);
    drop(state);
    let settled = fixture.state_bytes();
    let (code, replay) = fixture.run(&args);
    assert_eq!(code, 0, "{replay}");
    assert_eq!(fixture.state_bytes(), settled);
    let (_, inspection) = fixture.run(&["home", "inspect"]);
    assert!(
        has_diagnostic(&inspection, "home_identity_ready"),
        "{inspection}"
    );
    fs::create_dir_all(fixture.home.join("plugins/later")).unwrap();
    let (code, later) = fixture.run(&[
        "contribution",
        "disable",
        "plugin:later",
        "--scope",
        "user",
        "--deadline",
        "10s",
    ]);
    assert_eq!(code, 0, "{later}");
    let after_later_mutation = fixture.state_bytes();
    let (code, replay) = fixture.run(&args);
    assert_eq!(code, 0, "{replay}");
    assert_eq!(fixture.state_bytes(), after_later_mutation);
}

#[test]
fn home_recovery_keeps_disabled_plugins_disabled_and_existing_audit_valid() {
    let fixture = Fixture::bound();
    fs::create_dir_all(fixture.home.join("plugins/formatter")).unwrap();
    let (code, disabled) = fixture.run(&[
        "contribution",
        "disable",
        "plugin:formatter",
        "--scope",
        "user",
        "--deadline",
        "10s",
    ]);
    assert_eq!(code, 0, "{disabled}");
    let scope = fs::read_dir(fixture.home.join("maintain/v1/deny"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let deny_before = fs::read(scope.join("state.json")).unwrap();
    fixture.renumber();
    let (code, recovered) = fixture.run(&["home", "recover", "--deadline", "10s"]);
    assert_eq!(code, 0, "{recovered}");
    assert_eq!(fs::read(scope.join("state.json")).unwrap(), deny_before);
    let descriptor = open_secure_root(&fixture.home).unwrap();
    let state = MaintenanceStateV1::bootstrap(
        &descriptor,
        path_identity(&descriptor).unwrap(),
        "55555555-5555-4555-8555-555555555555",
        true,
    )
    .unwrap();
    let plugin_root = open_secure_root(&fixture.home.join("plugins")).unwrap();
    let guard = state
        .admit_contribution_scope(
            omegon_maintenance_contracts::ContributionKind::Plugin,
            "user",
            &path_identity(&plugin_root).unwrap(),
            "66666666-6666-4666-8666-666666666666",
            true,
        )
        .unwrap();
    assert!(!guard.allows(b"formatter").unwrap());
    drop(guard);
    drop(state);
    let (code, verified) = fixture.run(&["audit", "verify"]);
    assert_eq!(code, 0, "{verified}");
}
