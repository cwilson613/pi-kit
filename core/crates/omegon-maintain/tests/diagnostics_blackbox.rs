use std::{ffi::OsStr, fs, path::Path, process::Command};

use omegon_maintenance_contracts::{
    ArtifactIdentityV1, AuditCheckpointV1, AuditRecordV1, AuthorityKey, CleanupCapability,
    FenceState, FenceV1, InstallationStateV1, LifecycleBoundary, OwnershipRecordV1, ResultStatus,
    SCHEMA_VERSION, TransactionState, TransactionV1, canonical_digest, canonical_json, derive_key,
    normalize_workspace_path, parse_record, record_identity_at, workspace_key,
};
use serde_json::Value;

const EXPECTED_EXCLUSIONS: &[&str] = &[
    "default_loop",
    "extension_runtime",
    "lifecycle",
    "mcp",
    "memory",
    "mutable_packs",
    "orchestration",
    "project_config",
    "project_contributions",
    "provider_clients",
    "tui",
];

fn binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_omegon-maintain")
        .map(Into::into)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path.join("omegon-maintain")
        })
}

#[cfg(unix)]
fn reset_audit(home: &Path) {
    let root = home.join("maintain/v1");
    let segment = root.join("audit/segments/1.jsonl");
    if segment.exists() {
        fs::remove_file(segment).unwrap();
    }
    let installation_path = root.join("state.json");
    let mut installation: InstallationStateV1 =
        parse_record(&fs::read(&installation_path).unwrap()).unwrap();
    installation.next_audit_sequence = 1;
    fs::write(&installation_path, canonical_json(&installation).unwrap()).unwrap();
    let zero = AuthorityKey::from_bytes([0; 32]);
    let checkpoint = AuditCheckpointV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "audit_checkpoint".into(),
        record_id: derive_key(
            "audit-checkpoint",
            &[
                installation.installation_uuid.as_bytes(),
                &0_u64.to_be_bytes(),
                zero.as_bytes(),
            ],
        ),
        installation_uuid: installation.installation_uuid,
        last_sequence: 0,
        last_digest: zero,
    };
    fs::write(
        root.join("audit/checkpoint.json"),
        canonical_json(&checkpoint).unwrap(),
    )
    .unwrap();
}

#[cfg(unix)]
fn write_audit_records(home: &Path, count: u64) {
    use std::{io::Write, os::unix::fs::PermissionsExt};

    let root = home.join("maintain/v1");
    let segments = root.join("audit/segments");
    for entry in fs::read_dir(&segments).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let installation_path = root.join("state.json");
    let mut installation: InstallationStateV1 =
        parse_record(&fs::read(&installation_path).unwrap()).unwrap();
    let mut previous = None;
    let mut writer = None;
    for sequence in 1..=count {
        if (sequence - 1) % 100_000 == 0 {
            let path = segments.join(format!("{sequence}.jsonl"));
            writer = Some(std::io::BufWriter::new(fs::File::create(&path).unwrap()));
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let record = AuditRecordV1 {
            schema_version: SCHEMA_VERSION,
            record_kind: "audit".into(),
            record_id: derive_key(
                "audit",
                &[
                    installation.installation_uuid.as_bytes(),
                    &sequence.to_be_bytes(),
                ],
            ),
            installation_uuid: installation.installation_uuid.clone(),
            sequence,
            previous_digest: previous,
            request_id: "44444444-4444-4444-4444-444444444444".into(),
            command: "test.audit".into(),
            outcome: ResultStatus::Success,
        };
        writer
            .as_mut()
            .unwrap()
            .write_all(&canonical_json(&record).unwrap())
            .unwrap();
        previous = Some(canonical_digest(&record).unwrap());
    }
    writer.unwrap().flush().unwrap();
    let last_digest = previous.unwrap_or_else(|| AuthorityKey::from_bytes([0; 32]));
    installation.next_audit_sequence = count + 1;
    fs::write(&installation_path, canonical_json(&installation).unwrap()).unwrap();
    let checkpoint = AuditCheckpointV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "audit_checkpoint".into(),
        record_id: derive_key(
            "audit-checkpoint",
            &[
                installation.installation_uuid.as_bytes(),
                &count.to_be_bytes(),
                last_digest.as_bytes(),
            ],
        ),
        installation_uuid: installation.installation_uuid,
        last_sequence: count,
        last_digest,
    };
    fs::write(
        root.join("audit/checkpoint.json"),
        canonical_json(&checkpoint).unwrap(),
    )
    .unwrap();
}

fn run_json<I, S>(args: I, cwd: &Path) -> (i32, Value, String)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(binary())
        .args(args)
        .current_dir(cwd)
        .env_remove("OMEGON_HOME")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("GEMINI_API_KEY")
        .env_remove("OLLAMA_HOST")
        .env_remove("TERM")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid JSON ({error}): {stdout}\nstderr: {stderr}"));
    (output.status.code().unwrap_or(-1), value, stderr)
}

#[test]
fn identity_starts_without_normal_runtime_inputs() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&workspace).unwrap();

    let sentinel = root.path().join("normal-runtime-started");
    let sentinel_command = format!("touch {}", sentinel.display());
    let poisoned_inputs = [
        (
            workspace.join(".omegon/config.toml"),
            "not = [valid".to_owned(),
        ),
        (
            workspace.join(".omegon/plugins/poison/plugin.toml"),
            format!("startup = {sentinel_command:?}"),
        ),
        (
            home.join("extensions/poison/manifest.toml"),
            format!("command = {sentinel_command:?}"),
        ),
        (
            config.join("omegon/mcp.toml"),
            format!("command = {sentinel_command:?}"),
        ),
        (
            home.join("skills/poison/SKILL.md"),
            format!("---\nname: poison\nhook: {sentinel_command}\n---\n"),
        ),
        (home.join("memory.db"), "not a database".to_owned()),
        (
            workspace.join("openspec/changes/poison/tasks.md"),
            "not lifecycle state".to_owned(),
        ),
        (
            workspace.join(".omegon/workbench.json"),
            "not orchestration state".to_owned(),
        ),
    ];
    for (path, contents) in &poisoned_inputs {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    for command in [vec!["identity"], vec!["composition", "inspect"]] {
        let mut args = vec![
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
        ];
        args.extend(command);
        let (code, output, stderr) = run_json(args, root.path());
        assert_eq!(code, 0, "{stderr}");
        assert_eq!(output["status"], "success");
        assert_eq!(output["composition"]["profile"], "maintenance");
        assert_eq!(
            output["composition"]["excluded_inputs"],
            serde_json::json!(EXPECTED_EXCLUSIONS)
        );
    }

    assert!(!sentinel.exists(), "normal runtime command was executed");
    for (path, contents) in poisoned_inputs {
        assert_eq!(fs::read_to_string(path).unwrap(), contents);
    }
}

#[cfg(unix)]
#[test]
fn contribution_list_is_inert_and_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::write(root.path().join("secret"), "DO_NOT_READ_THIS_VALUE").unwrap();
    symlink(root.path().join("secret"), home.join("plugins/linked")).unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "contribution",
            "list",
            "--scope",
            "user",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    let encoded = serde_json::to_string(&output).unwrap();
    assert!(encoded.contains("plugin:formatter"));
    assert!(encoded.contains("plugin:linked"));
    assert!(!encoded.contains("DO_NOT_READ_THIS_VALUE"));
}

#[test]
fn session_inspect_validates_pair_and_workspace() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.meta.json")),
        serde_json::to_vec(&serde_json::json!({
            "session_id": id,
            "cwd": workspace.to_str().unwrap()
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        sessions.join(format!("{id}.json")),
        br#"{"schema_version":1}"#,
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "inspect",
            id,
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| { diagnostic["code"] == "session_pair_valid" })
    );
}

#[test]
fn session_inspect_prefers_catalog_and_ignores_stale_compatibility_files() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    let catalog = serde_json::to_vec(&serde_json::json!({
        "catalog_schema_version": 1,
        "session_id": id,
        "workspace_identity": workspace.join(".").to_str().unwrap(),
        "metadata_revision": 1,
        "friendly_name": null,
        "description": null,
        "created_at": "2026-08-17T00:00:00Z",
        "turns": 1,
        "tool_calls": 0,
        "last_prompt_snippet": null,
        "lineage": "full",
        "availability": "exact",
        "semantic_frontier": {
            "stream_id": "11111111-1111-4111-8111-111111111111",
            "sequence": 1,
            "event_id": "22222222-2222-4222-8222-222222222222"
        },
        "source_selection": "semantic_authority_plus_host_stores"
    }))
    .unwrap();
    fs::write(sessions.join(format!("{id}.catalog.v1.json")), &catalog).unwrap();
    fs::write(sessions.join(format!("{id}.json")), b"stale and malformed").unwrap();
    fs::write(
        sessions.join(format!("{id}.meta.json")),
        b"stale and malformed",
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "inspect",
            id,
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}: {output}");
    let diagnostics = output["diagnostics"].as_array().unwrap();
    let valid = diagnostics
        .iter()
        .find(|entry| entry["code"] == "session_semantic_valid")
        .unwrap();
    let evidence: Value = serde_json::from_str(valid["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(evidence["catalog_schema_version"], 1);
    assert_eq!(evidence["catalog_size"], catalog.len());
    assert!(evidence["catalog_digest"].is_string());
    assert_eq!(evidence["lineage"], "full");
    assert_eq!(evidence["availability"], "exact");
    assert!(diagnostics.iter().all(|entry| {
        entry["code"] != "session_pair_invalid" && entry["code"] != "session_semantic_invalid"
    }));
}

#[test]
fn session_inspect_rejects_partial_semantic_catalog() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.catalog.v1.json")),
        serde_json::to_vec(&serde_json::json!({
            "catalog_schema_version": 1,
            "session_id": id,
            "workspace_identity": workspace.to_str().unwrap(),
            "metadata_revision": 1,
            "lineage": "full",
            "availability": "exact",
            "source_selection": "semantic_authority_plus_host_stores"
        }))
        .unwrap(),
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "inspect",
            id,
        ],
        root.path(),
    );
    assert_eq!(code, 1, "{stderr}: {output}");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "session_semantic_invalid")
    );
    assert!(
        output["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "session_not_unique")
    );
}

#[test]
fn session_list_accepts_catalog_only_session() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.catalog.v1.json")),
        serde_json::to_vec(&serde_json::json!({
            "catalog_schema_version": 1,
            "session_id": id,
            "workspace_identity": workspace.to_str().unwrap(),
            "metadata_revision": 1,
            "friendly_name": null,
            "description": null,
            "created_at": "2026-08-17T00:00:00Z",
            "turns": 1,
            "tool_calls": 0,
            "last_prompt_snippet": null,
            "lineage": "mixed",
            "availability": "exact_suffix",
            "semantic_frontier": {
                "stream_id": "11111111-1111-4111-8111-111111111111",
                "sequence": 1,
                "event_id": "22222222-2222-4222-8222-222222222222"
            },
            "source_selection": "semantic_authority_plus_host_stores"
        }))
        .unwrap(),
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}: {output}");
    assert_eq!(output["status"], "success");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "session_semantic_valid")
    );
}

#[test]
fn session_list_ignores_semantic_authority_sidecars() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.meta.json")),
        serde_json::to_vec(&serde_json::json!({
            "session_id": id,
            "cwd": workspace.to_str().unwrap()
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        sessions.join(format!("{id}.json")),
        br#"{"schema_version":1}"#,
    )
    .unwrap();
    fs::write(
        sessions.join(format!("{id}.authority.jsonl")),
        b"authority record bytes are not inspected",
    )
    .unwrap();
    fs::write(
        sessions.join(format!("{id}.authority.snapshot.json")),
        b"authority cache bytes are not inspected",
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["status"], "success");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| { entry["code"] != "session_pair_invalid" })
    );
}

#[test]
fn resource_list_labels_legacy_records_unverifiable() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let runtime = workspace.join(".omegon/runtime/tui-42");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    fs::write(runtime.join("workspace.json"), b"{}").unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "resource",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| { diagnostic["code"] == "resource_legacy_unverifiable" })
    );
}

#[cfg(unix)]
#[test]
fn root_aliases_and_final_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let linked = root.path().join("linked-home");
    fs::create_dir_all(&home).unwrap();
    symlink(&home, &linked).unwrap();

    let (alias_code, alias_output, _) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            home.to_str().unwrap(),
            "doctor",
        ],
        root.path(),
    );
    assert_eq!(alias_code, 1);
    assert_eq!(alias_output["errors"][0]["code"], "root_alias_rejected");

    let (symlink_code, symlink_output, _) = run_json(
        [
            "--json",
            "--home",
            linked.to_str().unwrap(),
            "--config-home",
            home.to_str().unwrap(),
            "doctor",
        ],
        root.path(),
    );
    assert_eq!(symlink_code, 1);
    assert_eq!(symlink_output["errors"][0]["code"], "root_home_invalid");
}

#[test]
fn identity_does_not_require_filesystem_roots() {
    let root = tempfile::tempdir().unwrap();
    let absent_home = root.path().join("absent-home");
    let absent_config = root.path().join("absent-config");

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            absent_home.to_str().unwrap(),
            "--config-home",
            absent_config.to_str().unwrap(),
            "identity",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["status"], "success");
    assert_eq!(output["diagnostics"][0]["code"], "record_artifact_identity");
}

#[test]
fn malformed_session_does_not_count_as_an_exact_match() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.meta.json")),
        br#"{"session_id":"different","cwd":"/wrong"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join(format!("{id}.json")),
        br#"{"schema_version":1}"#,
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "inspect",
            id,
        ],
        root.path(),
    );
    assert_eq!(code, 1, "{stderr}");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["code"] == "session_pair_invalid"
                    && serde_json::from_str::<Value>(item["evidence"].as_str().unwrap()).unwrap()
                        ["quarantine_available"]
                        == false
            })
    );
    assert!(
        output["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "session_not_unique")
    );
}

#[test]
fn mutation_requires_explicit_deadline_before_root_admission() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent");

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            absent.to_str().unwrap(),
            "contribution",
            "disable",
            "plugin:formatter",
            "--scope",
            "user",
        ],
        root.path(),
    );
    assert_eq!(code, 1, "{stderr}");
    assert_eq!(output["errors"][0]["code"], "deadline_required");
}

#[test]
fn contribution_inventory_with_multiple_scopes_has_valid_ordered_output() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(home.join("catalog/user-entry")).unwrap();
    fs::create_dir_all(home.join("plugins/z-user")).unwrap();
    fs::create_dir_all(workspace.join(".omegon/plugins/project-entry")).unwrap();
    fs::create_dir_all(&config).unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "contribution",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["status"], "success");
    let messages: Vec<&str> = output["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["message"].as_str().unwrap())
        .collect();
    assert_eq!(messages.len(), 3);
    assert!(messages[0].starts_with("catalog:user-entry "));
    assert!(messages[1].starts_with("plugin:z-user "));
    assert!(messages[2].starts_with("plugin:project-entry "));
}

#[test]
fn list_commands_reject_legacy_numeric_cursors() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&workspace).unwrap();

    for (command, cursor) in [
        (["contribution", "list"], "list-v1:contribution.list:0"),
        (["session", "list"], "list-v1:session.list:0"),
        (["resource", "list"], "list-v1:resource.list:0"),
    ] {
        let mut args = vec![
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
        ];
        args.extend(command);
        args.extend(["--cursor", cursor]);
        let (code, output, stderr) = run_json(args, root.path());
        assert_eq!(code, 1, "{stderr}: {output}");
        assert_eq!(output["errors"][0]["code"], "cli_cursor_invalid");
    }
}

#[cfg(unix)]
#[test]
fn contribution_list_emits_largest_bounded_prefix_and_resumes() {
    use std::os::unix::fs::symlink;

    const ENTRY_COUNT: usize = 5_200;
    const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let plugins = home.join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    fs::create_dir_all(&config).unwrap();
    let link_target = "x".repeat(768);
    for index in 0..ENTRY_COUNT {
        symlink(&link_target, plugins.join(format!("item-{index:05}"))).unwrap();
    }

    let common = [
        "--json",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "contribution",
        "list",
        "--scope",
        "user",
    ];
    let mut cursor = None;
    let mut selectors = Vec::new();
    let mut first_page = None;
    for _ in 0..4 {
        let mut args = common.to_vec();
        if let Some(value) = cursor.as_deref() {
            args.extend(["--cursor", value]);
        }
        let (code, output, stderr) = run_json(args, root.path());
        assert!(
            serde_json::to_vec(&output).unwrap().len() <= OUTPUT_LIMIT,
            "oversized output: {output}"
        );
        for diagnostic in output["diagnostics"].as_array().unwrap() {
            let evidence: Value =
                serde_json::from_str(diagnostic["evidence"].as_str().unwrap()).unwrap();
            selectors.push(evidence["selector"].as_str().unwrap().to_owned());
        }
        if output["truncated"] == false {
            assert_eq!(code, 0, "{stderr}: {output}");
            cursor = None;
            break;
        }
        assert_eq!(code, 2, "{stderr}: {output}");
        assert_eq!(output["status"], "degraded");
        cursor = Some(output["next_cursor"].as_str().unwrap().to_owned());
        if first_page.is_none() {
            let workspace = root.path().join("workspace");
            fs::create_dir_all(&workspace).unwrap();
            let forged = [
                "--json",
                "--home",
                home.to_str().unwrap(),
                "--config-home",
                config.to_str().unwrap(),
                "--workspace",
                workspace.to_str().unwrap(),
                "contribution",
                "list",
                "--scope",
                "project",
                "--cursor",
                cursor.as_deref().unwrap(),
            ];
            let (forged_code, forged_output, forged_stderr) = run_json(forged, root.path());
            assert_eq!(forged_code, 1, "{forged_stderr}: {forged_output}");
            assert_eq!(forged_output["errors"][0]["code"], "cli_cursor_invalid");

            // This sorts ahead of the boundary. A numeric offset would skip an old item.
            symlink(&link_target, plugins.join("item-00000a")).unwrap();
            first_page = Some(output);
        }
    }
    assert!(cursor.is_none(), "pagination did not terminate");

    let expected: Vec<_> = (0..ENTRY_COUNT)
        .map(|index| format!("plugin:item-{index:05}"))
        .collect();
    assert_eq!(selectors, expected);

    let mut overfull = first_page.expect("inventory must exceed one page");
    let next_cursor = overfull["next_cursor"].as_str().unwrap().to_owned();
    let mut next_args = common.to_vec();
    next_args.extend(["--cursor", &next_cursor]);
    let (_, next_page, _) = run_json(next_args, root.path());
    overfull["diagnostics"]
        .as_array_mut()
        .unwrap()
        .push(next_page["diagnostics"][0].clone());
    assert!(serde_json::to_vec(&overfull).unwrap().len() > OUTPUT_LIMIT);

    let boundary_selector: Value = serde_json::from_str(
        overfull["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .nth(1)
            .unwrap()["evidence"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let boundary_name = boundary_selector["selector"]
        .as_str()
        .unwrap()
        .strip_prefix("plugin:")
        .unwrap();
    fs::remove_file(plugins.join(boundary_name)).unwrap();
    let mut removed_args = common.to_vec();
    removed_args.extend(["--cursor", &next_cursor]);
    let (removed_code, removed_output, removed_stderr) = run_json(removed_args, root.path());
    assert_eq!(removed_code, 1, "{removed_stderr}: {removed_output}");
    assert_eq!(removed_output["errors"][0]["code"], "cli_cursor_invalid");
}

#[test]
fn session_list_filters_other_workspaces_without_degrading() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let other_workspace = root.path().join("other-workspace");
    let sessions = config.join("sessions");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&other_workspace).unwrap();
    for (slug, id, cwd) in [
        ("one", "2026-08-17T00-00-00_00000000", workspace.as_path()),
        (
            "two",
            "2026-08-17T00-00-00_00000000",
            other_workspace.as_path(),
        ),
    ] {
        let directory = sessions.join(slug);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("{id}.meta.json")),
            serde_json::to_vec(&serde_json::json!({
                "session_id": id,
                "cwd": cwd.to_str().unwrap()
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join(format!("{id}.json")),
            br#"{"schema_version":1}"#,
        )
        .unwrap();
    }

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["status"], "success");
    assert_eq!(output["diagnostics"].as_array().unwrap().len(), 1);
    assert!(output.to_string().contains("00000000"));

    let (inspect_code, inspect_output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "session",
            "inspect",
            "2026-08-17T00-00-00_00000000",
        ],
        root.path(),
    );
    assert_eq!(inspect_code, 0, "{stderr}");
    assert_eq!(inspect_output["diagnostics"].as_array().unwrap().len(), 1);
}

#[test]
fn snapshot_without_metadata_is_reported_as_malformed() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    fs::write(
        sessions.join(format!("{id}.json")),
        br#"{"schema_version":1}"#,
    )
    .unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "session",
            "list",
        ],
        root.path(),
    );
    assert_eq!(code, 2, "{stderr}");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "session_pair_invalid")
    );
}

#[test]
fn project_mutation_admission_requires_workspace_before_deferred_refusal() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--deadline",
            "5s",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "contribution",
            "disable",
            "plugin:formatter",
            "--scope",
            "project",
        ],
        root.path(),
    );
    assert_eq!(code, 1, "{stderr}");
    assert!(
        output["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "root_workspace_required")
    );
}

#[test]
fn resource_list_requires_record_identity_to_match_runtime_and_workspace() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let runtime_id = "runtime-good";
    let runtime = workspace.join(".omegon/runtime").join(runtime_id);
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    let workspace_bytes = normalize_workspace_path(workspace.to_str().unwrap().as_bytes()).unwrap();
    let workspace_key = workspace_key("unix", &workspace_bytes);
    let generation_id = "generation-good";
    let record = OwnershipRecordV1 {
        schema_version: 1,
        record_kind: "ownership".into(),
        record_id: derive_key(
            "ownership",
            &[
                workspace_key.as_bytes(),
                runtime_id.as_bytes(),
                generation_id.as_bytes(),
            ],
        ),
        runtime_id: runtime_id.into(),
        generation_id: generation_id.into(),
        workspace_key,
        boot_id: "linux:00000000-0000-0000-0000-000000000001".into(),
        pid: 42,
        process_group: Some(42),
        process_start_token: "linux:42".into(),
        lifecycle_boundary: LifecycleBoundary::OwnedProcessTree,
        cleanup_capability: CleanupCapability::Strict,
        writer: ArtifactIdentityV1 {
            version: "0.29.0-dev".into(),
            commit: "commit-good".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            digest: AuthorityKey::from_bytes([0; 32]),
        },
        heartbeat_utc: "2026-08-17T00:00:00Z".into(),
        heartbeat_monotonic_ticks: 42,
        expires_after_seconds: 300,
    };
    fs::write(
        runtime.join("ownership-v1.json"),
        canonical_json(&record).unwrap(),
    )
    .unwrap();

    let args = [
        "--json",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "resource",
        "list",
    ];
    let (valid_code, valid_output, stderr) = run_json(args, root.path());
    assert_eq!(valid_code, 0, "{stderr}");
    assert_eq!(
        valid_output["diagnostics"][0]["code"],
        "resource_ownership_v1"
    );

    let mismatched = workspace.join(".omegon/runtime/runtime-mismatch");
    fs::create_dir_all(&mismatched).unwrap();
    fs::write(
        mismatched.join("ownership-v1.json"),
        canonical_json(&record).unwrap(),
    )
    .unwrap();
    let (mismatch_code, mismatch_output, stderr) = run_json(args, root.path());
    assert_eq!(mismatch_code, 2, "{stderr}");
    assert!(
        mismatch_output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "resource_record_invalid")
    );
}

#[test]
fn contribution_disable_settles_deny_transaction_and_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let plugin = home.join("plugins/formatter");
    fs::create_dir_all(&plugin).unwrap();
    fs::create_dir_all(&config).unwrap();
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "contribution",
        "disable",
        "plugin:formatter",
        "--scope",
        "user",
    ];

    let (code, output, stderr) = run_json(args, root.path());
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert!(plugin.exists());
    let deny_root = home.join("maintain/v1/deny");
    let scope = fs::read_dir(&deny_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let first: Value =
        serde_json::from_slice(&fs::read(scope.join("state.json")).unwrap()).unwrap();
    assert_eq!(first["generation"], 1);
    assert_eq!(first["entries"].as_object().unwrap().len(), 1);
    assert!(
        fs::read_dir(home.join("maintain/v1/fences"))
            .unwrap()
            .next()
            .is_none()
    );
    assert_eq!(
        fs::read_dir(home.join("maintain/v1/transactions"))
            .unwrap()
            .count(),
        1
    );

    let (second_code, second_output, stderr) = run_json(args, root.path());
    assert_eq!(second_code, 0, "{stderr}");
    assert_eq!(second_output["mutations"][0]["state"], "settled");
    let second: Value =
        serde_json::from_slice(&fs::read(scope.join("state.json")).unwrap()).unwrap();
    assert_eq!(second["generation"], 1);
    assert_eq!(
        fs::read_to_string(home.join("maintain/v1/audit/segments/1.jsonl"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    fs::create_dir_all(home.join("plugins/linter")).unwrap();
    let reused_request = second_output["request_id"].as_str().unwrap();
    let (conflict_code, conflict_output, _) = run_json(
        [
            "--json",
            "--deadline",
            "5s",
            "--request-id",
            reused_request,
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "contribution",
            "disable",
            "plugin:linter",
            "--scope",
            "user",
        ],
        root.path(),
    );
    assert_ne!(conflict_code, 0);
    assert_eq!(conflict_output["errors"][0]["code"], "transaction_refused");
    let after_conflict: Value =
        serde_json::from_slice(&fs::read(scope.join("state.json")).unwrap()).unwrap();
    assert_eq!(after_conflict["generation"], 1);
    assert_eq!(after_conflict["entries"].as_object().unwrap().len(), 1);
}

#[test]
fn contribution_disable_dry_run_creates_no_deny_or_transaction() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--dry-run",
            "--deadline",
            "5s",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "contribution",
            "disable",
            "plugin:formatter",
            "--scope",
            "user",
        ],
        root.path(),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(output["mutations"][0]["state"], "planned");
    assert!(
        fs::read_dir(home.join("maintain/v1/deny"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        fs::read_dir(home.join("maintain/v1/transactions"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn contribution_quarantine_renames_real_entry_and_unlinks_only_symlink() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let real = home.join("plugins/formatter");
    let linked = home.join("plugins/linked");
    let target = root.path().join("outside-target");
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::write(&target, b"preserve me").unwrap();
    symlink(&target, &linked).unwrap();

    for selector in ["plugin:formatter", "plugin:linked"] {
        let (code, output, stderr) = run_json(
            [
                "--json",
                "--deadline",
                "5s",
                "--home",
                home.to_str().unwrap(),
                "--config-home",
                config.to_str().unwrap(),
                "contribution",
                "quarantine",
                selector,
                "--scope",
                "user",
            ],
            root.path(),
        );
        assert_eq!(code, 0, "{stderr}: {output}");
        assert_eq!(output["mutations"][0]["state"], "settled");
    }
    assert!(!real.exists());
    assert!(!linked.exists());
    assert_eq!(fs::read(&target).unwrap(), b"preserve me");
    let quarantine = home.join("plugins/.omegon-maintain-quarantine");
    assert_eq!(fs::read_dir(quarantine).unwrap().count(), 1);
    let deny_scope = fs::read_dir(home.join("maintain/v1/deny"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let deny: Value =
        serde_json::from_slice(&fs::read(deny_scope.join("state.json")).unwrap()).unwrap();
    assert_eq!(deny["generation"], 2);
    assert_eq!(deny["entries"].as_object().unwrap().len(), 2);
}

#[cfg(unix)]
#[test]
fn session_quarantine_creates_resume_deny_and_preserves_pair_bytes() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    let request_id = "33333333-3333-3333-3333-333333333333";
    let metadata = serde_json::to_vec(&serde_json::json!({
        "session_id": id,
        "cwd": workspace.to_str().unwrap()
    }))
    .unwrap();
    let snapshot = br#"{"schema_version":1,"messages":[]}"#.to_vec();
    let metadata_path = sessions.join(format!("{id}.meta.json"));
    let snapshot_path = sessions.join(format!("{id}.json"));
    fs::write(&metadata_path, &metadata).unwrap();
    fs::write(&snapshot_path, &snapshot).unwrap();
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--request-id",
        request_id,
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "session",
        "quarantine",
        id,
    ];

    let (code, output, stderr) = run_json(args, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert_eq!(fs::read(&metadata_path).unwrap(), metadata);
    assert_eq!(fs::read(&snapshot_path).unwrap(), snapshot);
    assert_eq!(
        fs::read_dir(home.join("maintain/v1/session-deny"))
            .unwrap()
            .count(),
        1
    );

    let (second_code, second_output, stderr) = run_json(args, root.path());
    assert_eq!(second_code, 0, "{stderr}: {second_output}");
    assert_eq!(second_output["mutations"][0]["state"], "settled");
    assert_eq!(
        fs::read_dir(home.join("maintain/v1/session-deny"))
            .unwrap()
            .count(),
        1
    );

    let transaction_path = home
        .join("maintain/v1/transactions")
        .join(format!("{request_id}.json"));
    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    transaction.state = TransactionState::StepDispatched;
    transaction.audit_sequence = None;
    transaction.steps[0].state = omegon_maintenance_contracts::TransactionStepState::Dispatched;
    transaction.steps[0].observed = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    let fence_path = home
        .join("maintain/v1/fences")
        .join(format!("{}.json", transaction.domain_key));
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();
    fs::remove_file(&metadata_path).unwrap();
    fs::remove_file(&snapshot_path).unwrap();

    let (recovery_code, recovery_output, stderr) = run_json(args, root.path());
    assert_eq!(recovery_code, 0, "{stderr}: {recovery_output}");
    assert_eq!(recovery_output["mutations"][0]["state"], "settled");
    assert!(!fence_path.exists());
}

#[cfg(unix)]
#[test]
fn session_quarantine_prefers_catalog_and_preserves_catalog_bytes() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let sessions = config.join("sessions/legacy-slug");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let id = "2026-08-17T00-00-00_00000000";
    let catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "catalog_schema_version": 1,
        "session_id": id,
        "workspace_identity": workspace.to_str().unwrap(),
        "metadata_revision": 1,
        "friendly_name": null,
        "description": null,
        "created_at": "2026-08-17T00:00:00Z",
        "turns": 1,
        "tool_calls": 0,
        "last_prompt_snippet": null,
        "lineage": "full",
        "availability": "exact",
        "semantic_frontier": {
            "stream_id": "11111111-1111-4111-8111-111111111111",
            "sequence": 1,
            "event_id": "22222222-2222-4222-8222-222222222222"
        },
        "source_selection": "semantic_authority_plus_host_stores"
    }))
    .unwrap();
    let catalog_path = sessions.join(format!("{id}.catalog.v1.json"));
    fs::write(&catalog_path, &catalog).unwrap();
    fs::write(sessions.join(format!("{id}.json")), b"stale snapshot").unwrap();
    fs::write(sessions.join(format!("{id}.meta.json")), b"stale metadata").unwrap();
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--request-id",
        "44444444-4444-4444-4444-444444444444",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "session",
        "quarantine",
        id,
    ];

    let (code, output, stderr) = run_json(args, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert_eq!(fs::read(&catalog_path).unwrap(), catalog);
    assert_eq!(
        fs::read_dir(home.join("maintain/v1/session-deny"))
            .unwrap()
            .count(),
        1
    );

    let (second_code, second_output, stderr) = run_json(args, root.path());
    assert_eq!(second_code, 0, "{stderr}: {second_output}");
    assert_eq!(second_output["mutations"][0]["state"], "settled");
    assert_eq!(fs::read(&catalog_path).unwrap(), catalog);
}

#[test]
fn audit_inspect_and_verify_detect_structural_corruption() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let common = [
        "--json",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
    ];
    let mut disable = common.to_vec();
    disable.extend([
        "--deadline",
        "5s",
        "contribution",
        "disable",
        "plugin:formatter",
        "--scope",
        "user",
    ]);
    assert_eq!(run_json(disable, root.path()).0, 0);

    let mut inspect = common.to_vec();
    inspect.extend(["audit", "inspect"]);
    let (inspect_code, inspect_output, stderr) = run_json(inspect, root.path());
    assert_eq!(inspect_code, 0, "{stderr}");
    assert_eq!(inspect_output["diagnostics"][0]["code"], "audit_record");

    let mut verify = common.to_vec();
    verify.extend(["audit", "verify"]);
    let (verify_code, verify_output, stderr) = run_json(verify.clone(), root.path());
    assert_eq!(verify_code, 0, "{stderr}");
    assert_eq!(verify_output["diagnostics"][0]["code"], "audit_chain_valid");

    let segment = home.join("maintain/v1/audit/segments/1.jsonl");
    let mut bytes = fs::read(&segment).unwrap();
    bytes[0] = b'[';
    fs::write(segment, bytes).unwrap();
    let (corrupt_code, corrupt_output, stderr) = run_json(verify, root.path());
    assert_eq!(corrupt_code, 1, "{stderr}");
    assert_eq!(corrupt_output["errors"][0]["code"], "audit_invalid");
}

#[cfg(unix)]
#[test]
fn audit_rotation_verifies_and_paginates_across_segment_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let common = [
        "--json",
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
    ];
    let mut disable = common.to_vec();
    disable.extend([
        "--deadline",
        "5s",
        "contribution",
        "disable",
        "plugin:formatter",
        "--scope",
        "user",
    ]);
    assert_eq!(run_json(disable, root.path()).0, 0);
    write_audit_records(&home, 100_001);

    let mut verify = common.to_vec();
    verify.extend(["audit", "verify"]);
    let (code, output, stderr) = run_json(&verify, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    let evidence: Value =
        serde_json::from_str(output["diagnostics"][0]["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(evidence["first_sequence"], 100_001);
    assert_eq!(evidence["records_verified"], 1);
    let verify_cursor = evidence["continuation_cursor"].as_str().unwrap();

    verify.extend(["--cursor", verify_cursor]);
    let (code, output, stderr) = run_json(&verify, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    let evidence: Value =
        serde_json::from_str(output["diagnostics"][0]["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(evidence["first_sequence"], 1);
    assert_eq!(evidence["records_verified"], 100_000);
    assert!(evidence["continuation_cursor"].is_null());

    let first_segment = home.join("maintain/v1/audit/segments/1.jsonl");
    let original = fs::read(&first_segment).unwrap();
    let final_line_start = original[..original.len() - 1]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |offset| offset + 1);
    let mut replacement: AuditRecordV1 = parse_record(&original[final_line_start..]).unwrap();
    replacement.command = "forged.audit".into();
    let replacement_bytes = canonical_json(&replacement).unwrap();
    let replacement_digest = canonical_digest(&replacement).unwrap();
    let mut disconnected = original[..final_line_start].to_vec();
    disconnected.extend_from_slice(&replacement_bytes);
    fs::write(&first_segment, disconnected).unwrap();
    let cursor_fields: Vec<_> = verify_cursor.split(':').collect();
    let forged_cursor = format!(
        "{}:{}:{}:{}:{}:{}",
        cursor_fields[0],
        cursor_fields[1],
        cursor_fields[2],
        cursor_fields[3],
        replacement_digest,
        cursor_fields[5]
    );
    let mut forged_verify = common.to_vec();
    forged_verify.extend(["audit", "verify", "--cursor", &forged_cursor]);
    let (code, output, stderr) = run_json(forged_verify, root.path());
    assert_eq!(code, 1, "{stderr}: {output}");
    assert_eq!(output["errors"][0]["code"], "audit_invalid");
    fs::write(&first_segment, &original).unwrap();

    let mut inspect = common.to_vec();
    inspect.extend(["audit", "inspect"]);
    let (code, output, stderr) = run_json(&inspect, root.path());
    assert_eq!(code, 2, "{stderr}: {output}");
    assert_eq!(output["diagnostics"].as_array().unwrap().len(), 1);
    let evidence: Value =
        serde_json::from_str(output["diagnostics"][0]["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(evidence["sequence"], 100_001);
    let inspect_cursor = output["next_cursor"].as_str().unwrap();

    inspect.extend(["--cursor", inspect_cursor]);
    let (code, output, stderr) = run_json(inspect, root.path());
    assert_eq!(code, 2, "{stderr}: {output}");
    assert_eq!(output["diagnostics"].as_array().unwrap().len(), 1_000);
    let newest: Value =
        serde_json::from_str(output["diagnostics"][0]["evidence"].as_str().unwrap()).unwrap();
    let oldest: Value =
        serde_json::from_str(output["diagnostics"][999]["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(newest["sequence"], 100_000);
    assert_eq!(oldest["sequence"], 99_001);

    let mut bytes = fs::read(&first_segment).unwrap();
    let command = b"\"command\":\"test.audit\"";
    let offset = bytes
        .windows(command.len())
        .rposition(|window| window == command)
        .unwrap();
    bytes[offset + b"\"command\":\"".len()] = b'x';
    fs::write(first_segment, bytes).unwrap();
    let mut corrupt_verify = common.to_vec();
    corrupt_verify.extend(["audit", "verify", "--cursor", verify_cursor]);
    let (code, output, stderr) = run_json(corrupt_verify, root.path());
    assert_eq!(code, 1, "{stderr}: {output}");
    assert_eq!(output["errors"][0]["code"], "audit_invalid");
}

#[cfg(unix)]
#[test]
fn resource_prune_removes_only_expired_different_boot_record() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    let runtime_id = "runtime-stale";
    let runtime = workspace.join(".omegon/runtime").join(runtime_id);
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    fs::write(runtime.join("sibling.txt"), b"preserve").unwrap();
    let workspace_bytes = normalize_workspace_path(workspace.to_str().unwrap().as_bytes()).unwrap();
    let workspace_key = workspace_key("unix", &workspace_bytes);
    let generation_id = "generation-stale";
    #[cfg(target_os = "macos")]
    let (boot_id, process_start_token, writer_target) =
        ("macos:1:1", "macos:1:1", "aarch64-apple-darwin");
    #[cfg(target_os = "linux")]
    let (boot_id, process_start_token, writer_target) = (
        "linux:00000000-0000-0000-0000-000000000001",
        "linux:42",
        "x86_64-unknown-linux-gnu",
    );
    let record = OwnershipRecordV1 {
        schema_version: 1,
        record_kind: "ownership".into(),
        record_id: derive_key(
            "ownership",
            &[
                workspace_key.as_bytes(),
                runtime_id.as_bytes(),
                generation_id.as_bytes(),
            ],
        ),
        runtime_id: runtime_id.into(),
        generation_id: generation_id.into(),
        workspace_key,
        boot_id: boot_id.into(),
        pid: 42,
        process_group: Some(42),
        process_start_token: process_start_token.into(),
        lifecycle_boundary: LifecycleBoundary::OwnedProcessTree,
        cleanup_capability: CleanupCapability::Strict,
        writer: ArtifactIdentityV1 {
            version: "0.29.0-dev".into(),
            commit: "commit-stale".into(),
            target: writer_target.into(),
            digest: AuthorityKey::from_bytes([0; 32]),
        },
        heartbeat_utc: "2020-01-01T00:00:00Z".into(),
        heartbeat_monotonic_ticks: 1,
        expires_after_seconds: 300,
    };
    let ownership = runtime.join("ownership-v1.json");
    fs::write(&ownership, canonical_json(&record).unwrap()).unwrap();
    let mut permissions = fs::metadata(&ownership).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&ownership, permissions).unwrap();
    let foreign_runtime_id = "runtime-foreign";
    let foreign_runtime = workspace.join(".omegon/runtime").join(foreign_runtime_id);
    fs::create_dir_all(&foreign_runtime).unwrap();
    let mut foreign = record.clone();
    foreign.runtime_id = foreign_runtime_id.into();
    foreign.record_id = derive_key(
        "ownership",
        &[
            workspace_key.as_bytes(),
            foreign_runtime_id.as_bytes(),
            generation_id.as_bytes(),
        ],
    );
    #[cfg(target_os = "macos")]
    {
        foreign.boot_id = "linux:00000000-0000-0000-0000-000000000001".into();
        foreign.process_start_token = "linux:42".into();
        foreign.writer.target = "x86_64-unknown-linux-gnu".into();
    }
    #[cfg(target_os = "linux")]
    {
        foreign.boot_id = "macos:1:1".into();
        foreign.process_start_token = "macos:1:1".into();
        foreign.writer.target = "aarch64-apple-darwin".into();
    }
    let foreign_ownership = foreign_runtime.join("ownership-v1.json");
    fs::write(&foreign_ownership, canonical_json(&foreign).unwrap()).unwrap();
    let mut permissions = fs::metadata(&foreign_ownership).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&foreign_ownership, permissions).unwrap();

    let (code, output, stderr) = run_json(
        [
            "--json",
            "--deadline",
            "5s",
            "--home",
            home.to_str().unwrap(),
            "--config-home",
            config.to_str().unwrap(),
            "--workspace",
            workspace.to_str().unwrap(),
            "resource",
            "prune-stale",
        ],
        root.path(),
    );
    assert_ne!(code, 0, "{stderr}: {output}");
    assert_eq!(output["status"], "degraded");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert!(!ownership.exists());
    assert!(foreign_ownership.exists());
    assert_eq!(fs::read(runtime.join("sibling.txt")).unwrap(), b"preserve");
    assert!(runtime.exists());
}

#[cfg(unix)]
#[test]
fn resource_restart_aborts_one_prepared_frontier_without_pruning() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&config).unwrap();
    let workspace_bytes = normalize_workspace_path(workspace.to_str().unwrap().as_bytes()).unwrap();
    let workspace_key = workspace_key("unix", &workspace_bytes);
    #[cfg(target_os = "macos")]
    let (boot_id, process_start_token, writer_target) =
        ("macos:1:1", "macos:1:1", "aarch64-apple-darwin");
    #[cfg(target_os = "linux")]
    let (boot_id, process_start_token, writer_target) = (
        "linux:00000000-0000-0000-0000-000000000001",
        "linux:42",
        "x86_64-unknown-linux-gnu",
    );
    let make_record = |runtime_id: &str| OwnershipRecordV1 {
        schema_version: 1,
        record_kind: "ownership".into(),
        record_id: derive_key(
            "ownership",
            &[
                workspace_key.as_bytes(),
                runtime_id.as_bytes(),
                b"generation-stale",
            ],
        ),
        runtime_id: runtime_id.into(),
        generation_id: "generation-stale".into(),
        workspace_key,
        boot_id: boot_id.into(),
        pid: 42,
        process_group: Some(42),
        process_start_token: process_start_token.into(),
        lifecycle_boundary: LifecycleBoundary::OwnedProcessTree,
        cleanup_capability: CleanupCapability::Strict,
        writer: ArtifactIdentityV1 {
            version: "0.29.0-dev".into(),
            commit: "commit-stale".into(),
            target: writer_target.into(),
            digest: AuthorityKey::from_bytes([0; 32]),
        },
        heartbeat_utc: "2020-01-01T00:00:00Z".into(),
        heartbeat_monotonic_ticks: 1,
        expires_after_seconds: 300,
    };
    let runtime_ids = ["runtime-a", "runtime-b", "runtime-c"];
    for runtime_id in runtime_ids {
        let runtime = workspace.join(".omegon/runtime").join(runtime_id);
        fs::create_dir_all(&runtime).unwrap();
        let ownership = runtime.join("ownership-v1.json");
        fs::write(
            &ownership,
            canonical_json(&make_record(runtime_id)).unwrap(),
        )
        .unwrap();
        let mut permissions = fs::metadata(&ownership).unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&ownership, permissions).unwrap();
    }
    let request_id = "44444444-4444-4444-4444-444444444444";
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--request-id",
        request_id,
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "resource",
        "prune-stale",
    ];
    assert_eq!(run_json(args, root.path()).0, 0);
    reset_audit(&home);

    let transaction_path = home
        .join("maintain/v1/transactions")
        .join(format!("{request_id}.json"));
    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    assert_eq!(transaction.steps.len(), 3);
    for (index, runtime_id) in runtime_ids.iter().enumerate().skip(1) {
        let runtime = workspace.join(".omegon/runtime").join(runtime_id);
        let ownership = runtime.join("ownership-v1.json");
        fs::write(
            &ownership,
            canonical_json(&make_record(runtime_id)).unwrap(),
        )
        .unwrap();
        let mut permissions = fs::metadata(&ownership).unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&ownership, permissions).unwrap();
        let directory = std::fs::File::open(&runtime).unwrap();
        transaction.steps[index].expected_existing = Some(
            record_identity_at(&directory, b"ownership-v1.json")
                .unwrap()
                .unwrap(),
        );
        transaction.steps[index].state =
            omegon_maintenance_contracts::TransactionStepState::Prepared;
        transaction.steps[index].observed = None;
    }
    transaction.state = TransactionState::StepSettled;
    transaction.audit_sequence = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    let fence_path = home
        .join("maintain/v1/fences")
        .join(format!("{}.json", transaction.domain_key));
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();

    let (code, output, stderr) = run_json(args, root.path());
    assert_ne!(code, 0, "{stderr}: {output}");
    assert_eq!(output["status"], "degraded");
    assert!(
        workspace
            .join(".omegon/runtime/runtime-b/ownership-v1.json")
            .exists()
    );
    assert!(
        workspace
            .join(".omegon/runtime/runtime-c/ownership-v1.json")
            .exists()
    );
    let transaction: TransactionV1 = parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    assert_eq!(transaction.state, TransactionState::Aborted);
    assert_eq!(
        transaction.steps[1].state,
        omegon_maintenance_contracts::TransactionStepState::Aborted
    );
    assert_eq!(
        transaction.steps[2].state,
        omegon_maintenance_contracts::TransactionStepState::Prepared
    );
    assert!(!fence_path.exists());
}

#[cfg(unix)]
#[test]
fn same_request_reconciles_audited_target_settlement_without_repeating_mutation() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let request_id = "11111111-1111-1111-1111-111111111111";
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--request-id",
        request_id,
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "contribution",
        "disable",
        "plugin:formatter",
        "--scope",
        "user",
    ];
    assert_eq!(run_json(args, root.path()).0, 0);
    let transaction_path = home
        .join("maintain/v1/transactions")
        .join(format!("{request_id}.json"));
    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    transaction.state = TransactionState::TargetsSettled;
    transaction.audit_sequence = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let mut permissions = fs::metadata(&transaction_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&transaction_path, permissions).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    let fence_path = home
        .join("maintain/v1/fences")
        .join(format!("{}.json", transaction.domain_key));
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();

    let (code, output, stderr) = run_json(args, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "transaction_reconciled")
    );
    assert!(!fence_path.exists());
    assert_eq!(
        fs::read_to_string(home.join("maintain/v1/audit/segments/1.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    transaction.state = TransactionState::StepDispatched;
    transaction.audit_sequence = None;
    transaction.steps[0].state = omegon_maintenance_contracts::TransactionStepState::Dispatched;
    transaction.steps[0].observed = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();
    let (dispatched_code, dispatched_output, stderr) = run_json(args, root.path());
    assert_eq!(dispatched_code, 0, "{stderr}: {dispatched_output}");
    assert_eq!(dispatched_output["mutations"][0]["state"], "settled");
    assert_eq!(
        fs::read_to_string(home.join("maintain/v1/audit/segments/1.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    transaction.state = TransactionState::StepDispatched;
    transaction.audit_sequence = None;
    transaction.steps[0].state = omegon_maintenance_contracts::TransactionStepState::Dispatched;
    transaction.steps[0].observed = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let (missing_fence_code, missing_fence_output, _) = run_json(args, root.path());
    assert_ne!(missing_fence_code, 0);
    assert_eq!(missing_fence_output["status"], "degraded");
    let persisted: TransactionV1 = parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    assert_eq!(persisted.state, TransactionState::StepDispatched);
}

#[cfg(unix)]
#[test]
fn same_request_reconciles_dispatched_quarantine_from_destination_identity() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = root.path().join("config");
    fs::create_dir_all(home.join("plugins/formatter")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let request_id = "22222222-2222-2222-2222-222222222222";
    let args = [
        "--json",
        "--deadline",
        "5s",
        "--request-id",
        request_id,
        "--home",
        home.to_str().unwrap(),
        "--config-home",
        config.to_str().unwrap(),
        "contribution",
        "quarantine",
        "plugin:formatter",
        "--scope",
        "user",
    ];
    assert_eq!(run_json(args, root.path()).0, 0);
    let transaction_path = home
        .join("maintain/v1/transactions")
        .join(format!("{request_id}.json"));
    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    assert_eq!(transaction.steps.len(), 2);
    transaction.state = TransactionState::StepDispatched;
    transaction.audit_sequence = None;
    transaction.steps[1].state = omegon_maintenance_contracts::TransactionStepState::Dispatched;
    transaction.steps[1].observed = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let mut permissions = fs::metadata(&transaction_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&transaction_path, permissions).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    let fence_path = home
        .join("maintain/v1/fences")
        .join(format!("{}.json", transaction.domain_key));
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();

    let (code, output, stderr) = run_json(args, root.path());
    assert_eq!(code, 0, "{stderr}: {output}");
    assert_eq!(output["mutations"][0]["state"], "settled");
    assert!(!fence_path.exists());
    assert_eq!(
        fs::read_dir(home.join("plugins/.omegon-maintain-quarantine"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(home.join("maintain/v1/audit/segments/1.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    reset_audit(&home);

    let quarantine_directory = home.join("plugins/.omegon-maintain-quarantine");
    let quarantined = fs::read_dir(&quarantine_directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::rename(&quarantined, home.join("plugins/formatter")).unwrap();
    let mut transaction: TransactionV1 =
        parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    transaction.state = TransactionState::StepSettled;
    transaction.audit_sequence = None;
    transaction.steps[1].state = omegon_maintenance_contracts::TransactionStepState::Prepared;
    transaction.steps[1].observed = None;
    fs::write(&transaction_path, canonical_json(&transaction).unwrap()).unwrap();
    let fence = FenceV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "fence".into(),
        record_id: derive_key(
            "fence",
            &[
                transaction.domain_key.as_bytes(),
                transaction.record_id.as_bytes(),
            ],
        ),
        domain_key: transaction.domain_key,
        transaction_record_id: transaction.record_id,
        state: FenceState::Active,
    };
    fs::write(&fence_path, canonical_json(&fence).unwrap()).unwrap();
    let mut permissions = fs::metadata(&fence_path).unwrap().permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&fence_path, permissions).unwrap();

    let (code, output, stderr) = run_json(args, root.path());
    assert_ne!(code, 0, "{stderr}: {output}");
    assert_eq!(output["status"], "degraded");
    assert!(home.join("plugins/formatter").exists());
    assert_eq!(fs::read_dir(&quarantine_directory).unwrap().count(), 0);
    assert!(!fence_path.exists());
    let transaction: TransactionV1 = parse_record(&fs::read(&transaction_path).unwrap()).unwrap();
    assert_eq!(transaction.state, TransactionState::Aborted);
    assert_eq!(
        transaction.steps[1].state,
        omegon_maintenance_contracts::TransactionStepState::Aborted
    );
}
