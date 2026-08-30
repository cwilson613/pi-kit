use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SUPPORTED_SDK_CONTRACT_VERSION: &str = "0.25";
pub const MIN_COMPATIBLE_SDK_CONTRACT_VERSION: &str = "0.24";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkCompatibilityStatus {
    Supported,
    MissingLegacy,
    Malformed,
    OlderCompatible,
    OlderUnsupported,
    NewerUnsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkCompatibilitySeverity {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkCompatibilityDiagnostic {
    pub status: SdkCompatibilityStatus,
    pub advertised_version: Option<String>,
    pub supported_version: String,
    pub severity: SdkCompatibilitySeverity,
    pub message: String,
}

impl SdkCompatibilityDiagnostic {
    pub fn is_blocking(&self) -> bool {
        self.severity == SdkCompatibilitySeverity::Error
    }
}

pub fn classify_initialize_metadata(metadata: Option<&Value>) -> SdkCompatibilityDiagnostic {
    classify_sdk_version(
        metadata
            .and_then(|value| value.get("extension_info"))
            .and_then(|info| info.get("sdk_version"))
            .and_then(Value::as_str),
    )
}

pub fn classify_sdk_version(version: Option<&str>) -> SdkCompatibilityDiagnostic {
    let supported = SUPPORTED_SDK_CONTRACT_VERSION.to_string();
    let Some(raw) = version.map(str::trim).filter(|version| !version.is_empty()) else {
        return SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::MissingLegacy,
            advertised_version: None,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Warning,
            message: "extension did not advertise an SDK contract version; treating as legacy"
                .to_string(),
        };
    };
    let advertised = Some(raw.to_string());
    let Some(observed) = parse_contract_version(raw) else {
        return SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::Malformed,
            advertised_version: advertised,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Error,
            message: format!("extension advertised malformed SDK contract version '{raw}'"),
        };
    };
    let expected = parse_contract_version(SUPPORTED_SDK_CONTRACT_VERSION)
        .expect("supported SDK contract version must be valid");
    let minimum = parse_contract_version(MIN_COMPATIBLE_SDK_CONTRACT_VERSION)
        .expect("minimum SDK contract version must be valid");
    match observed.cmp(&expected) {
        std::cmp::Ordering::Equal => SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::Supported,
            advertised_version: advertised,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Ok,
            message: format!("extension SDK contract {raw} is supported"),
        },
        std::cmp::Ordering::Less if observed >= minimum => SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::OlderCompatible,
            advertised_version: advertised,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Warning,
            message: format!(
                "extension SDK contract {raw} is older than supported contract {SUPPORTED_SDK_CONTRACT_VERSION} but remains within the compatibility window"
            ),
        },
        std::cmp::Ordering::Less => SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::OlderUnsupported,
            advertised_version: advertised,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Error,
            message: format!(
                "extension SDK contract {raw} is older than minimum compatible contract {MIN_COMPATIBLE_SDK_CONTRACT_VERSION}"
            ),
        },
        std::cmp::Ordering::Greater => SdkCompatibilityDiagnostic {
            status: SdkCompatibilityStatus::NewerUnsupported,
            advertised_version: advertised,
            supported_version: supported,
            severity: SdkCompatibilitySeverity::Error,
            message: format!(
                "extension SDK contract {raw} is newer than supported contract {SUPPORTED_SDK_CONTRACT_VERSION}"
            ),
        },
    }
}

fn parse_contract_version(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    if let Some(patch) = parts.next() {
        patch.parse::<u64>().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_window_is_preserved() {
        assert_eq!(
            classify_sdk_version(Some("0.25")).status,
            SdkCompatibilityStatus::Supported
        );
        assert_eq!(
            classify_sdk_version(Some("0.24")).status,
            SdkCompatibilityStatus::OlderCompatible
        );
        assert!(classify_sdk_version(Some("0.26")).is_blocking());
        assert!(!classify_sdk_version(None).is_blocking());
    }
}
