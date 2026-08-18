use std::{
    fs::OpenOptions,
    process::Command,
    sync::{Arc, Barrier},
    thread,
};

use omegon_maintenance_contracts::{
    AuditCheckpointV1, AuditRecordV1, AuthorityKey, CommandSemanticsV1, ContributionSelector,
    DenyRecordV1, DenyStateV1, DetachObservation, ErrorV1, FenceV1, FileIdentityV1,
    InstallationStateV1, ListScope, LockMode, MaintenanceResultV1, MutationResultV1, MutationState,
    OwnershipRecordV1, PackageManifestV1, PathIdentityV1, PostStateV1, ProtocolLock,
    ReconciliationDecision, Record, RecordObservation, ResultStatus, SCHEMA_VERSION,
    SessionDenyRecordV1, TransactionState, TransactionStepKind, TransactionStepState,
    TransactionV1, canonical_json, command_fingerprint, derive_key, normalize_workspace_path,
    parse_record, reconcile_detach, reconcile_record, resolve_list_scope, validate_child_name,
};

const ZERO_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn installation() -> InstallationStateV1 {
    let home = PathIdentityV1::unix(b"/tmp/omegon", 7, 11).unwrap();
    InstallationStateV1 {
        schema_version: SCHEMA_VERSION,
        record_kind: "installation_state".into(),
        record_id: derive_key("installation", &[b"11111111-1111-1111-1111-111111111111"]),
        installation_uuid: "11111111-1111-1111-1111-111111111111".into(),
        home,
        next_audit_sequence: 1,
    }
}

#[test]
fn canonical_fixture_is_stable_and_round_trips() {
    let record = installation();
    let encoded = canonical_json(&record).unwrap();
    assert_eq!(
        encoded,
        include_bytes!("fixtures/installation-state-v1.json")
    );
    assert_eq!(
        parse_record::<InstallationStateV1>(&encoded).unwrap(),
        record
    );
}

#[test]
fn every_authority_record_has_a_canonical_fixture() {
    assert_fixture::<DenyRecordV1>(include_bytes!("fixtures/deny-v1.json"));
    assert_fixture::<DenyStateV1>(include_bytes!("fixtures/deny-state-v1.json"));
    assert_fixture::<SessionDenyRecordV1>(include_bytes!("fixtures/session-deny-v1.json"));
    assert_fixture::<OwnershipRecordV1>(include_bytes!("fixtures/ownership-v1.json"));
    assert_fixture::<TransactionV1>(include_bytes!("fixtures/transaction-v1.json"));
    assert_fixture::<FenceV1>(include_bytes!("fixtures/fence-v1.json"));
    assert_fixture::<AuditRecordV1>(include_bytes!("fixtures/audit-v1.json"));
    assert_fixture::<AuditCheckpointV1>(include_bytes!("fixtures/audit-checkpoint-v1.json"));
    assert_fixture::<PackageManifestV1>(include_bytes!("fixtures/package-manifest-v1.json"));
}

#[test]
fn result_wire_fixture_is_bounded_and_stable() {
    for bytes in [
        include_bytes!("fixtures/result-v1.json").as_slice(),
        include_bytes!("fixtures/result-degraded-v1.json").as_slice(),
        include_bytes!("fixtures/result-failure-v1.json").as_slice(),
    ] {
        let result: MaintenanceResultV1 = serde_json::from_slice(bytes).unwrap();
        result.validate().unwrap();
        assert_eq!(canonical_json(&result).unwrap(), bytes);
    }
}

#[test]
fn result_validation_rejects_unstable_codes_and_unsafe_truncation() {
    let mut result: MaintenanceResultV1 =
        serde_json::from_slice(include_bytes!("fixtures/result-v1.json")).unwrap();
    result.errors.push(ErrorV1 {
        code: "unknown_family".into(),
        phase: "admission".into(),
        retry_safe: true,
        message: "refused".into(),
    });
    assert!(result.validate().is_err());

    result.errors[0].code = "root_unsafe".into();
    assert!(result.validate().is_err(), "success cannot contain errors");

    result.errors.clear();
    result.mutations.push(MutationResultV1 {
        domain_key: ZERO_KEY.parse().unwrap(),
        kind: "deny".into(),
        state: MutationState::Unknown,
        retry_safe: false,
    });
    assert!(
        result.validate().is_err(),
        "success cannot contain an unsettled mutation"
    );
    result.mutations.clear();

    result.status = ResultStatus::Degraded;
    result.command = "contribution.list".into();
    result.truncated = true;
    result.next_cursor = Some("next-page".into());
    result.mutations.push(MutationResultV1 {
        domain_key: ZERO_KEY.parse().unwrap(),
        kind: "deny".into(),
        state: MutationState::Applied,
        retry_safe: false,
    });
    assert!(
        result.validate().is_err(),
        "applied mutation cannot be truncated"
    );

    result.mutations.clear();
    result.errors.clear();
    assert!(
        result.validate().is_ok(),
        "paginated diagnostics may truncate"
    );

    result.truncated = false;
    result.next_cursor = None;
    result.composition.excluded_inputs =
        vec!["x".repeat(omegon_maintenance_contracts::MAX_RESULT_BYTES)];
    assert!(
        result.validate().is_err(),
        "result envelope must be bounded"
    );
}

#[test]
fn command_fingerprint_accepts_only_typed_semantics() {
    let mut semantics: CommandSemanticsV1 =
        serde_json::from_slice(include_bytes!("fixtures/command-semantics-v1.json")).unwrap();
    assert_eq!(
        command_fingerprint(&semantics).unwrap().to_string(),
        "91d4b6aca1f2f76e46fb28f32563fc47e110ea01ddf8e51680140cbc25193a49"
    );
    semantics
        .semantic_options
        .insert("request_id".into(), serde_json::json!("forbidden"));
    assert!(command_fingerprint(&semantics).is_err());
}

fn assert_fixture<T>(bytes: &[u8])
where
    T: omegon_maintenance_contracts::Record + for<'de> serde::Deserialize<'de> + serde::Serialize,
{
    let parsed: T = parse_record(bytes).unwrap();
    assert_eq!(canonical_json(&parsed).unwrap(), bytes);
}

#[test]
fn canonical_key_vector_is_stable() {
    let actual = derive_key("path", &[b"unix", b"/tmp/omegon"]);
    assert_eq!(
        actual.to_string(),
        "28b131518772d1d41891ed1d23b793fbc788c897d9aed418b8fb2515937229f7"
    );
    assert_eq!(
        derive_key("installation", &[b"11111111-1111-1111-1111-111111111111"]).to_string(),
        "a025a02cc5ba0d43c08b35ede56fa60321bea3798b577c359d6e9532a3ec707b"
    );
    assert_eq!(
        omegon_maintenance_contracts::session_key(
            "2026-08-17T00-00-00_00000000",
            ZERO_KEY.parse().unwrap()
        )
        .to_string(),
        "145442d96433837fcbd9304d1606736e040c50c3c8323b521f8c853b43e33216"
    );
    assert_eq!(
        derive_key(
            "session-deny",
            &[
                &hex_key("145442d96433837fcbd9304d1606736e040c50c3c8323b521f8c853b43e33216"),
                b"00000000-0000-0000-0000-000000000001"
            ]
        )
        .to_string(),
        "1c2fa0b9f27c6b2f879e32f0c0c47b5e2509f812320464b4b75471553812ed77"
    );
}

#[test]
fn every_authority_key_and_digest_has_an_independent_vector() {
    let zero: AuthorityKey = ZERO_KEY.parse().unwrap();
    let vectors = [
        (
            omegon_maintenance_contracts::workspace_key("unix", b"/tmp/omegon"),
            "2086da2783b16dbb6012a36164b5bdb5ddb168675373cd4f198aa44e6d227782",
        ),
        (
            omegon_maintenance_contracts::scope_key("plugin", "user", zero),
            "9e63c5d56ec80266bdc4a4f76c580149dfac9e3535d30daab1ea0cc93edaf1db",
        ),
        (
            omegon_maintenance_contracts::entry_key("plugin", zero, b"formatter"),
            "298038f9f86ae24a5c4b7e475255c3d660280c97dd4872ee419c6a692123a912",
        ),
        (
            omegon_maintenance_contracts::resource_domain_key(zero),
            "1051c37f5c933d1e5cab029075c9e9e5ae33ef61a096646d1da4006fad3a8ae3",
        ),
        (
            omegon_maintenance_contracts::contribution_domain_key(zero),
            "1dbe2a0fd6873fe7fbfbcafafd493c505ea7a55a659b4a9aedbb62c34d519d30",
        ),
        (
            omegon_maintenance_contracts::session_domain_key(zero),
            "c03f443564f61eb19dd89573a72d880d33afa46fa194ade77f44096e4383602a",
        ),
        (
            omegon_maintenance_contracts::canonical_digest(&installation()).unwrap(),
            "9665a68bbdae3ecf1ed47d22f23626f155a24abc8d6209ef062e6f52ab02a437",
        ),
    ];
    for (actual, expected) in vectors {
        assert_eq!(actual.to_string(), expected);
    }
}

fn hex_key(value: &str) -> [u8; 32] {
    *value.parse::<AuthorityKey>().unwrap().as_bytes()
}

#[test]
fn parser_rejects_corrupt_authority_records() {
    let valid = canonical_json(&installation()).unwrap();

    let duplicate =
        String::from_utf8(valid.clone())
            .unwrap()
            .replacen("{", "{\"schema_version\":1,", 1);
    assert!(matches!(
        parse_record::<InstallationStateV1>(duplicate.as_bytes()),
        Err(omegon_maintenance_contracts::ContractError::DuplicateKey(_))
    ));

    let unknown =
        String::from_utf8(valid.clone())
            .unwrap()
            .replacen("{", "{\"unexpected\":true,", 1);
    assert!(parse_record::<InstallationStateV1>(unknown.as_bytes()).is_err());

    let float = String::from_utf8(valid.clone())
        .unwrap()
        .replace("\"next_audit_sequence\":1", "\"next_audit_sequence\":1.5");
    assert!(parse_record::<InstallationStateV1>(float.as_bytes()).is_err());

    let future = String::from_utf8(valid)
        .unwrap()
        .replace("\"schema_version\":1", "\"schema_version\":2");
    assert!(parse_record::<InstallationStateV1>(future.as_bytes()).is_err());

    assert!(parse_record::<InstallationStateV1>(valid_without_lf()).is_err());
    let mut extra_lf = canonical_json(&installation()).unwrap();
    extra_lf.push(b'\n');
    assert!(parse_record::<InstallationStateV1>(&extra_lf).is_err());

    let whitespace = String::from_utf8(canonical_json(&installation()).unwrap())
        .unwrap()
        .replacen(":", ": ", 1);
    assert!(parse_record::<InstallationStateV1>(whitespace.as_bytes()).is_err());

    let nested_duplicate = String::from_utf8(canonical_json(&installation()).unwrap())
        .unwrap()
        .replace("\"device\":7", "\"device\":7,\"device\":7");
    assert!(parse_record::<InstallationStateV1>(nested_duplicate.as_bytes()).is_err());

    let alternate_escape = String::from_utf8(canonical_json(&installation()).unwrap())
        .unwrap()
        .replace("11111111-1111", "11111111\\u002d1111");
    assert!(parse_record::<InstallationStateV1>(alternate_escape.as_bytes()).is_err());

    let signed_zero = include_str!("fixtures/ownership-v1.json")
        .replace("\"process_group\":42", "\"process_group\":-0");
    assert!(parse_record::<OwnershipRecordV1>(signed_zero.as_bytes()).is_err());

    let oversized = vec![b'x'; omegon_maintenance_contracts::MAX_RECORD_BYTES + 1];
    assert!(parse_record::<InstallationStateV1>(&oversized).is_err());
}

#[test]
fn parser_rejects_semantically_contradictory_records() {
    let bad_session = include_str!("fixtures/session-deny-v1.json").replace(
        "\"session_key\":\"145442d96433837fcbd9304d1606736e040c50c3c8323b521f8c853b43e33216\"",
        &format!("\"session_key\":\"{ZERO_KEY}\""),
    );
    assert!(parse_record::<SessionDenyRecordV1>(bad_session.as_bytes()).is_err());

    let existing = "{\"device\":1,\"inode\":1,\"modified_ns\":1,\"size\":1}";
    let contradictory_step = include_str!("fixtures/transaction-v1.json").replace(
        "\"expected_existing\":null",
        &format!("\"expected_existing\":{existing}"),
    );
    assert!(parse_record::<TransactionV1>(contradictory_step.as_bytes()).is_err());

    let contradictory_state = include_str!("fixtures/transaction-v1.json").replace(
        "\"state\":\"prepared\",\"steps\"",
        "\"state\":\"settled\",\"steps\"",
    );
    assert!(parse_record::<TransactionV1>(contradictory_state.as_bytes()).is_err());

    let bad_timestamp = include_str!("fixtures/deny-v1.json")
        .replace("2026-08-17T00:00:00Z", "2026-02-31T00:00:00Z");
    assert!(parse_record::<DenyRecordV1>(bad_timestamp.as_bytes()).is_err());

    let bad_request = include_str!("fixtures/audit-v1.json")
        .replace("00000000-0000-0000-0000-000000000001", "NOT-A-UUID");
    assert!(parse_record::<AuditRecordV1>(bad_request.as_bytes()).is_err());
}

#[test]
fn transaction_frontier_and_evidence_matrix_is_strict() {
    let base: TransactionV1 = parse_record(include_bytes!("fixtures/transaction-v1.json")).unwrap();

    let mut dispatched = base.clone();
    dispatched.state = TransactionState::StepDispatched;
    dispatched.steps[0].state = TransactionStepState::Dispatched;
    dispatched.validate().unwrap();

    let mut settled = base.clone();
    settled.state = TransactionState::Settled;
    settled.audit_sequence = Some(1);
    settled.steps[0].state = TransactionStepState::Settled;
    settled.steps[0].observed = Some(PostStateV1 {
        source_present: true,
        destination: Some(FileIdentityV1 {
            device: 7,
            inode: 12,
            size: 1,
            modified_ns: 1,
        }),
        destination_content_digest: settled.steps[0].intended_content_digest,
    });
    settled.validate().unwrap();

    let mut step_settled = settled.clone();
    step_settled.state = TransactionState::StepSettled;
    step_settled.audit_sequence = None;
    step_settled.steps.push(base.steps[0].clone());
    step_settled.validate().unwrap();

    let mut impossible_frontier = base.clone();
    impossible_frontier.state = TransactionState::StepDispatched;
    impossible_frontier.steps.push(dispatched.steps[0].clone());
    assert!(impossible_frontier.validate().is_err());

    let mut prepared_with_evidence = base.clone();
    prepared_with_evidence.steps[0].observed = settled.steps[0].observed.clone();
    assert!(prepared_with_evidence.validate().is_err());

    let mut settled_without_evidence = settled.clone();
    settled_without_evidence.steps[0].observed = None;
    assert!(settled_without_evidence.validate().is_err());

    let mut create_over_existing = base.clone();
    create_over_existing.steps[0].expected_absence = false;
    create_over_existing.steps[0].expected_existing = Some(FileIdentityV1 {
        device: 7,
        inode: 12,
        size: 1,
        modified_ns: 1,
    });
    assert!(create_over_existing.validate().is_err());

    let mut replace_over_absence = base.clone();
    replace_over_absence.steps[0].kind = TransactionStepKind::DenyStateReplace;
    assert!(replace_over_absence.validate().is_err());

    let mut detach_with_content = create_over_existing;
    detach_with_content.steps[0].kind = TransactionStepKind::QuarantineDetach;
    assert!(detach_with_content.validate().is_err());

    let mut audited_active = base.clone();
    audited_active.audit_sequence = Some(1);
    assert!(audited_active.validate().is_err());

    let mut time_reversed = base;
    time_reversed.created_at = "2026-08-18T00:00:00Z".into();
    assert!(time_reversed.validate().is_err());
}

fn valid_without_lf() -> &'static [u8] {
    include_bytes!("fixtures/installation-state-v1.json")
        .strip_suffix(b"\n")
        .unwrap()
}

#[test]
fn parser_accepts_reordered_object_keys_only() {
    let reordered = concat!(
        "{\"schema_version\":1,\"record_kind\":\"deny_state\",",
        "\"record_id\":\"c2a87c0027634788428939b4d5170a05bc4e191ca1760a47397da5f4b8d1f521\",",
        "\"scope_key\":\"0000000000000000000000000000000000000000000000000000000000000000\",",
        "\"generation\":0,\"entries\":{}}\n"
    );
    assert!(parse_record::<DenyStateV1>(reordered.as_bytes()).is_ok());
}

#[test]
fn every_fixture_rejects_a_forged_record_id() {
    assert_bad_record_id::<InstallationStateV1>(include_bytes!(
        "fixtures/installation-state-v1.json"
    ));
    assert_bad_record_id::<DenyRecordV1>(include_bytes!("fixtures/deny-v1.json"));
    assert_bad_record_id::<DenyStateV1>(include_bytes!("fixtures/deny-state-v1.json"));
    assert_bad_record_id::<SessionDenyRecordV1>(include_bytes!("fixtures/session-deny-v1.json"));
    assert_bad_record_id::<OwnershipRecordV1>(include_bytes!("fixtures/ownership-v1.json"));
    assert_bad_record_id::<TransactionV1>(include_bytes!("fixtures/transaction-v1.json"));
    assert_bad_record_id::<FenceV1>(include_bytes!("fixtures/fence-v1.json"));
    assert_bad_record_id::<AuditRecordV1>(include_bytes!("fixtures/audit-v1.json"));
    assert_bad_record_id::<AuditCheckpointV1>(include_bytes!("fixtures/audit-checkpoint-v1.json"));
    assert_bad_record_id::<PackageManifestV1>(include_bytes!("fixtures/package-manifest-v1.json"));
}

#[test]
fn corruption_fixture_inventory_is_cross_consumer_data() {
    #[derive(serde::Deserialize)]
    struct CorruptionCase {
        fixture: String,
        field: String,
        replacement: String,
        expected_error: String,
    }
    let cases: Vec<CorruptionCase> =
        serde_json::from_slice(include_bytes!("fixtures/corruption-cases-v1.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        assert!(case.fixture.ends_with("-v1.json"));
        assert_eq!(case.field, "record_id");
        assert_eq!(case.replacement, ZERO_KEY);
        assert_eq!(case.expected_error, "record_invalid_id");
    }
}

fn assert_bad_record_id<T>(fixture: &[u8])
where
    T: omegon_maintenance_contracts::Record + for<'de> serde::Deserialize<'de>,
{
    let mut corrupt = fixture.to_vec();
    let marker = b"\"record_id\":\"";
    let offset = corrupt
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap()
        + marker.len();
    corrupt[offset..offset + 64].fill(b'0');
    if fixture[offset..offset + 64]
        .iter()
        .all(|byte| *byte == b'0')
    {
        corrupt[offset] = b'1';
    }
    assert!(parse_record::<T>(&corrupt).is_err());
}

#[test]
fn authority_keys_require_lowercase_fixed_width_hex() {
    assert!(ZERO_KEY.parse::<AuthorityKey>().is_ok());
    assert!(
        "28B131518772D1D41891ED1D23B793FBC788C897D9AED418B8FB2515937229F7"
            .parse::<AuthorityKey>()
            .is_err()
    );
    assert!("00".parse::<AuthorityKey>().is_err());
    assert!(
        format!("{}g", &ZERO_KEY[..63])
            .parse::<AuthorityKey>()
            .is_err()
    );
}

#[test]
fn selectors_reject_traversal_and_unknown_kinds() {
    assert!("plugin:formatter".parse::<ContributionSelector>().is_ok());
    assert!(
        format!("entry:sha256:{ZERO_KEY}")
            .parse::<ContributionSelector>()
            .is_ok()
    );

    for invalid in [
        "plugin:../escape",
        "plugin:nested/path",
        "plugin:",
        "unknown:item",
        "plugin:\0item",
    ] {
        assert!(
            invalid.parse::<ContributionSelector>().is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn byte_paths_normalize_without_following_links() {
    assert_eq!(
        normalize_workspace_path(b"/tmp/./project/../workspace").unwrap(),
        b"/tmp/workspace"
    );
    assert_eq!(
        normalize_workspace_path(b"/tmp/non-utf8-\xff").unwrap(),
        b"/tmp/non-utf8-\xff"
    );
    assert!(normalize_workspace_path(b"/../../escape").is_err());
    for child in [b"..".as_slice(), b"nested/path", b"C:prefix", b"\0name"] {
        assert!(validate_child_name(child).is_err());
    }
    assert!(validate_child_name(b"backslash\\is-data-on-unix").is_ok());
    assert!(PathIdentityV1::unix(b"/tmp/../escape", 1, 1).is_err());
}

#[test]
fn contribution_list_scope_matrix_is_explicit() {
    assert_eq!(resolve_list_scope(None, false).unwrap(), ListScope::User);
    assert_eq!(
        resolve_list_scope(Some("user"), false).unwrap(),
        ListScope::User
    );
    assert_eq!(
        resolve_list_scope(None, true).unwrap(),
        ListScope::UserAndProject
    );
    assert_eq!(
        resolve_list_scope(Some("project"), true).unwrap(),
        ListScope::Project
    );
    assert!(resolve_list_scope(Some("user"), true).is_err());
    assert!(resolve_list_scope(Some("project"), false).is_err());
}

#[test]
fn result_status_has_stable_exit_codes() {
    assert_eq!(ResultStatus::Success.exit_code(), 0);
    assert_eq!(ResultStatus::Failure.exit_code(), 1);
    assert_eq!(ResultStatus::Degraded.exit_code(), 2);
}

#[test]
fn detach_reconciliation_is_conservative_at_every_crash_point() {
    let cases = [
        (
            DetachObservation {
                dispatched: false,
                source_matches: true,
                destination_matches: false,
                conflicting_state: false,
                observable: true,
            },
            ReconciliationDecision::AbortAndClearFence,
        ),
        (
            DetachObservation {
                dispatched: true,
                source_matches: false,
                destination_matches: true,
                conflicting_state: false,
                observable: true,
            },
            ReconciliationDecision::Settle,
        ),
        (
            DetachObservation {
                dispatched: true,
                source_matches: true,
                destination_matches: false,
                conflicting_state: false,
                observable: true,
            },
            ReconciliationDecision::RetainUnknownFence,
        ),
        (
            DetachObservation {
                dispatched: true,
                source_matches: false,
                destination_matches: false,
                conflicting_state: false,
                observable: false,
            },
            ReconciliationDecision::RetainUnknownFence,
        ),
    ];

    for (observation, expected) in cases {
        assert_eq!(reconcile_detach(observation), expected);
    }
}

#[test]
fn record_reconciliation_never_retries_unknown_dispatch() {
    assert_eq!(
        reconcile_record(RecordObservation::NotDispatched),
        ReconciliationDecision::AbortAndClearFence
    );
    assert_eq!(
        reconcile_record(RecordObservation::IntendedCanonicalBytesPresent),
        ReconciliationDecision::Settle
    );
    for observation in [
        RecordObservation::IntendedTargetAbsentAfterDispatch,
        RecordObservation::ConflictingRecordOrGeneration,
        RecordObservation::Unavailable,
    ] {
        assert_eq!(
            reconcile_record(observation),
            ReconciliationDecision::RetainUnknownFence
        );
    }
}

#[cfg(unix)]
#[test]
fn exclusive_lock_refuses_a_racing_holder() {
    let directory = tempfile::tempdir().unwrap();
    let parent = std::fs::File::open(directory.path()).unwrap();
    let path = directory.path().join("scope.lock");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let mut permissions = file.metadata().unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
    file.set_permissions(permissions).unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let holder_path = path.clone();
    let holder_barrier = Arc::clone(&barrier);
    let holder = thread::spawn(move || {
        let parent = std::fs::File::open(holder_path.parent().unwrap()).unwrap();
        let lock =
            ProtocolLock::acquire_at(&parent, b"scope.lock", LockMode::Exclusive, false, false)
                .unwrap();
        holder_barrier.wait();
        holder_barrier.wait();
        drop(lock);
    });

    barrier.wait();
    assert!(
        ProtocolLock::acquire_at(&parent, b"scope.lock", LockMode::Shared, false, true).is_err()
    );
    assert!(
        ProtocolLock::acquire_at(&parent, b"scope.lock", LockMode::Exclusive, false, true).is_err()
    );
    barrier.wait();
    holder.join().unwrap();
    assert!(
        ProtocolLock::acquire_at(&parent, b"scope.lock", LockMode::Shared, false, true).is_ok()
    );
}

#[cfg(unix)]
#[test]
fn exclusive_lock_refuses_a_racing_process() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("process.lock");
    let parent = std::fs::File::open(directory.path()).unwrap();
    let held = ProtocolLock::acquire_at(&parent, b"process.lock", LockMode::Exclusive, true, false)
        .unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("lock_child_helper")
        .env("OMEGON_MAINTENANCE_LOCK_TEST", &path)
        .status()
        .unwrap();
    drop(held);
    assert!(status.success());
}

#[cfg(unix)]
#[test]
fn lock_child_helper() {
    let Ok(path) = std::env::var("OMEGON_MAINTENANCE_LOCK_TEST") else {
        return;
    };
    let path = std::path::Path::new(&path);
    let parent = std::fs::File::open(path.parent().unwrap()).unwrap();
    assert!(
        ProtocolLock::acquire_at(
            &parent,
            path.file_name().unwrap().as_encoded_bytes(),
            LockMode::Shared,
            false,
            true,
        )
        .is_err(),
        "child unexpectedly acquired a shared lock"
    );
}

#[cfg(unix)]
#[test]
fn lock_rejects_symlinks_and_permissive_modes() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.lock");
    let link = directory.path().join("link.lock");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .unwrap();
    let mut permissions = file.metadata().unwrap().permissions();
    permissions.set_mode(0o644);
    file.set_permissions(permissions).unwrap();
    symlink(&target, &link).unwrap();
    let parent = std::fs::File::open(directory.path()).unwrap();

    assert!(
        ProtocolLock::acquire_at(&parent, b"target.lock", LockMode::Exclusive, false, true)
            .is_err()
    );
    assert!(
        ProtocolLock::acquire_at(&parent, b"link.lock", LockMode::Exclusive, false, true).is_err()
    );
}
