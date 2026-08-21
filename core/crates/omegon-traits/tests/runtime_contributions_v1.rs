use omegon_traits::{
    RUNTIME_CONTRIBUTION_SCHEMA_VERSION, RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
    RuntimeCompositionGeneration, RuntimeConfinementRequest, RuntimeContributionDeclaration,
    RuntimeContributionId, RuntimeContributionLifecycleRecord, RuntimeDiagnosticCode,
    RuntimeDynamicContributionPreflight, RuntimeEffect, RuntimeOwnedResourceRecord,
    RuntimeProtocolRange, RuntimeTrustAdmission, RuntimeTrustAdmissionEvidence,
    RuntimeTrustedCodeAuthority,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

fn assert_fixture_round_trip<T>(raw: &str)
where
    T: DeserializeOwned + serde::Serialize,
{
    let expected: Value = serde_json::from_str(raw).expect("fixture is valid JSON");
    let decoded: T = serde_json::from_value(expected.clone()).expect("fixture matches contract");
    let encoded = serde_json::to_value(decoded).expect("contract serializes");
    assert_eq!(encoded, expected);
}

#[test]
fn declaration_v1_fixture_round_trips() {
    let raw = include_str!("fixtures/runtime-contribution-declaration-v1.json");
    assert_fixture_round_trip::<RuntimeContributionDeclaration>(raw);
    let declaration: RuntimeContributionDeclaration = serde_json::from_str(raw).unwrap();
    assert_eq!(
        declaration.schema_version,
        RUNTIME_CONTRIBUTION_SCHEMA_VERSION
    );
    declaration.validate().unwrap();
}

#[test]
fn execution_policy_defaults_are_conservative_and_validate_fail_closed() {
    let raw = include_str!("fixtures/runtime-contribution-declaration-v1.json")
        .replace("\n        \"principals\": [\"model\"],", "")
        .replace("\n        \"parallelism\": \"parallel_safe\",", "");
    let declaration: RuntimeContributionDeclaration = serde_json::from_str(&raw).unwrap();
    let execution = &declaration.capabilities[0].execution;
    assert!(execution.principals.is_empty());
    assert_eq!(
        execution.parallelism,
        omegon_traits::RuntimeParallelism::Serial
    );
    assert!(declaration.validate().is_err());
}

#[test]
fn mutating_execution_requires_a_durable_fence_identity() {
    let mut declaration: RuntimeContributionDeclaration = serde_json::from_str(include_str!(
        "fixtures/runtime-contribution-declaration-v1.json"
    ))
    .unwrap();
    let capability = &mut declaration.capabilities[0];
    capability.effects.push(RuntimeEffect::FilesystemWrite);
    capability.execution.transaction =
        omegon_traits::RuntimeTransactionBehavior::IndependentMutation;
    assert_eq!(
        declaration.validate().unwrap_err(),
        "mutating execution must declare a mutation fence"
    );

    declaration.capabilities[0].execution.mutation_fence =
        Some(Box::new(omegon_traits::RuntimeMutationFence {
            domain: omegon_traits::RuntimeMutationDomainId::new("workspace:runtime").unwrap(),
            key: omegon_traits::RuntimeMutationFenceKey::new("capability:read").unwrap(),
        }));
    declaration.validate().unwrap();

    declaration.capabilities[0]
        .effects
        .retain(|effect| *effect != RuntimeEffect::FilesystemWrite);
    declaration.capabilities[0].execution.transaction =
        omegon_traits::RuntimeTransactionBehavior::None;
    assert_eq!(
        declaration.validate().unwrap_err(),
        "non-mutating execution cannot declare a mutation fence"
    );
}

#[test]
fn generation_v1_fixture_round_trips() {
    assert_fixture_round_trip::<RuntimeCompositionGeneration>(include_str!(
        "fixtures/runtime-composition-generation-v1.json"
    ));
}

#[test]
fn effect_evidence_v1_fixture_round_trips() {
    assert_fixture_round_trip::<Vec<omegon_traits::RuntimeEffectEvidence>>(include_str!(
        "fixtures/runtime-effect-evidence-v1.json"
    ));
}

#[test]
fn internal_bindings_and_command_aliases_have_stable_wire_shapes() {
    assert_eq!(
        serde_json::to_string(&omegon_traits::RuntimeInvocationKind::Internal).unwrap(),
        "\"internal\""
    );
    let alias = omegon_traits::CommandAlias {
        alias: "subagent".into(),
        canonical: "delegate".into(),
    };
    assert_eq!(
        serde_json::to_value(alias).unwrap(),
        serde_json::json!({"alias": "subagent", "canonical": "delegate"})
    );
}

#[test]
fn diagnostics_v1_fixture_has_explicit_stable_order() {
    let raw = include_str!("fixtures/runtime-contribution-diagnostics-v1.json");
    assert_fixture_round_trip::<Vec<omegon_traits::RuntimeContributionDiagnostic>>(raw);
    let diagnostics: Vec<omegon_traits::RuntimeContributionDiagnostic> =
        serde_json::from_str(raw).unwrap();
    let mut ordered = diagnostics.clone();
    ordered.sort_by_key(|diagnostic| diagnostic.stable_order_key());
    assert_eq!(diagnostics, ordered);
}

#[test]
fn validated_identifiers_and_protocol_ranges_fail_closed() {
    assert!(RuntimeContributionId::new("feature:core-tools").is_ok());
    assert!(RuntimeContributionId::new("core-tools").is_err());
    assert!(RuntimeDiagnosticCode::new("graph:duplicate_owner").is_ok());
    assert!(RuntimeProtocolRange::new(2, 1).is_err());
    assert!(serde_json::from_str::<RuntimeProtocolRange>(r#"{"minimum":2,"maximum":1}"#).is_err());

    let invalid_id = serde_json::from_str::<RuntimeContributionId>(r#""../escape""#);
    assert!(invalid_id.is_err());

    let mut declaration: RuntimeContributionDeclaration = serde_json::from_str(include_str!(
        "fixtures/runtime-contribution-declaration-v1.json"
    ))
    .unwrap();
    declaration.protocol.minimum = declaration.protocol.maximum + 1;
    assert!(declaration.validate().is_err());

    let unsupported_schema = include_str!("fixtures/runtime-composition-generation-v1.json")
        .replacen("\"schema_version\": 1", "\"schema_version\": 2", 1);
    assert!(serde_json::from_str::<RuntimeCompositionGeneration>(&unsupported_schema).is_err());
}

#[test]
fn trust_request_is_not_serialized_as_an_admission_grant() {
    let raw = include_str!("fixtures/runtime-contribution-declaration-v1.json");
    let value: Value = serde_json::from_str(raw).unwrap();
    assert!(value.get("requested_trust").is_some());
    assert!(value.get("trust_granted").is_none());
    assert!(value.get("admitted").is_none());
}

#[test]
fn dynamic_preflight_v1_fixture_round_trips() {
    let raw = include_str!("fixtures/runtime-dynamic-preflight-v1.json");
    assert_fixture_round_trip::<RuntimeDynamicContributionPreflight>(raw);
    let preflight: RuntimeDynamicContributionPreflight = serde_json::from_str(raw).unwrap();
    assert_eq!(
        preflight.schema_version,
        RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION
    );
    preflight.validate().unwrap();
}

#[test]
fn trust_admission_is_source_bound_and_manifest_requests_do_not_grant_it() {
    let raw = include_str!("fixtures/runtime-dynamic-preflight-v1.json");
    let preflight: RuntimeDynamicContributionPreflight = serde_json::from_str(raw).unwrap();
    let admission = RuntimeTrustAdmission {
        schema_version: RUNTIME_DYNAMIC_PREFLIGHT_SCHEMA_VERSION,
        contribution_id: preflight.id.clone(),
        source_digest: preflight.source_digest.clone(),
        evidence: RuntimeTrustAdmissionEvidence::TrustedCode {
            authority: RuntimeTrustedCodeAuthority::OperatorPolicy,
            policy_id: "operator:extensions-v1".into(),
        },
    };
    admission.validate_for(&preflight).unwrap();

    let mut wrong_source = admission;
    wrong_source.source_digest = "sha256:different".into();
    assert!(wrong_source.validate_for(&preflight).is_err());

    let value: Value = serde_json::from_str(raw).unwrap();
    assert!(value.get("evidence").is_none());
    assert!(value.get("admission").is_none());
}

#[test]
fn confinement_evidence_fails_closed_without_a_complete_brokered_boundary() {
    let incomplete = RuntimeTrustAdmissionEvidence::VerifiedConfinement {
        boundary: RuntimeConfinementRequest::Oci,
        verifier: "host:oci-v1".into(),
        profile: "isolated-probe-v1".into(),
        prevented_effects: vec![RuntimeEffect::NetworkAccess],
        brokered_effects_only: true,
    };
    assert!(incomplete.validate().is_err());

    let complete = RuntimeTrustAdmissionEvidence::VerifiedConfinement {
        boundary: RuntimeConfinementRequest::Oci,
        verifier: "host:oci-v1".into(),
        profile: "isolated-probe-v1".into(),
        prevented_effects: vec![
            RuntimeEffect::FilesystemRead,
            RuntimeEffect::ProcessSpawn,
            RuntimeEffect::NetworkAccess,
            RuntimeEffect::SecretDelivery,
        ],
        brokered_effects_only: true,
    };
    complete.validate().unwrap();
}

#[test]
fn lifecycle_and_owned_resource_v1_fixture_round_trips() {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct Fixture {
        lifecycle: RuntimeContributionLifecycleRecord,
        resource: RuntimeOwnedResourceRecord,
    }

    let raw = include_str!("fixtures/runtime-lifecycle-records-v1.json");
    assert_fixture_round_trip::<Fixture>(raw);
    let fixture: Fixture = serde_json::from_str(raw).unwrap();
    fixture.lifecycle.validate().unwrap();
    fixture.resource.validate().unwrap();
}

#[test]
fn lifecycle_records_reject_unbounded_reasons_and_false_cleanup_claims() {
    let raw = include_str!("fixtures/runtime-lifecycle-records-v1.json");
    let value: Value = serde_json::from_str(raw).unwrap();
    let mut lifecycle: RuntimeContributionLifecycleRecord =
        serde_json::from_value(value["lifecycle"].clone()).unwrap();
    lifecycle.reason = Some("x".repeat(513));
    assert!(lifecycle.validate().is_err());

    lifecycle.reason = Some("cleanup could not be verified".into());
    lifecycle.cleanup_state = omegon_traits::RuntimeCleanupState::Unverified;
    assert!(lifecycle.validate().is_err());

    let mut resource: RuntimeOwnedResourceRecord =
        serde_json::from_value(value["resource"].clone()).unwrap();
    resource.kind = omegon_traits::RuntimeOwnedResourceKind::RemoteService;
    assert!(resource.validate().is_err());
}
