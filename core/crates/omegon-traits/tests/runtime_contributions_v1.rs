use omegon_traits::{
    RUNTIME_CONTRIBUTION_SCHEMA_VERSION, RuntimeCompositionGeneration,
    RuntimeContributionDeclaration, RuntimeContributionId, RuntimeDiagnosticCode,
    RuntimeProtocolRange,
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
fn generation_v1_fixture_round_trips() {
    assert_fixture_round_trip::<RuntimeCompositionGeneration>(include_str!(
        "fixtures/runtime-composition-generation-v1.json"
    ));
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
