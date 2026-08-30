use crate::settings::Profile;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const CHILD_COMPONENT_DENIES_ENV: &str = "OMEGON_CHILD_COMPONENT_DENIES_V1";
const USER_COMPONENT_POLICY_FILE: &str = "component-policy.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentCatalogEntry {
    pub id: &'static str,
    pub enabled_by_default: bool,
    pub disableable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentCatalog {
    entries: Vec<ComponentCatalogEntry>,
}

impl ComponentCatalog {
    pub fn product_v1() -> Self {
        Self {
            entries: vec![
                ComponentCatalogEntry {
                    id: "core:codescan",
                    enabled_by_default: true,
                    disableable: true,
                },
                ComponentCatalogEntry {
                    id: "core:constitutional-kernel",
                    enabled_by_default: true,
                    disableable: false,
                },
                ComponentCatalogEntry {
                    id: "core:host-effects",
                    enabled_by_default: true,
                    disableable: false,
                },
                ComponentCatalogEntry {
                    id: "core:maintenance-recovery",
                    enabled_by_default: true,
                    disableable: false,
                },
            ],
        }
    }

    fn validate_selector(
        &self,
        selector: &str,
        source: &str,
        path: &str,
        allow_wildcard: bool,
    ) -> anyhow::Result<()> {
        if selector == "core:*" {
            if allow_wildcard {
                return Ok(());
            }
            anyhow::bail!("{source}: {path}: wildcard component selector is not permitted here");
        }
        let Some(name) = selector.strip_prefix("core:") else {
            anyhow::bail!("{source}: {path}: malformed component selector `{selector}`");
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            anyhow::bail!("{source}: {path}: malformed component selector `{selector}`");
        }
        let Some(entry) = self.entries.iter().find(|entry| entry.id == selector) else {
            anyhow::bail!("{source}: {path}: unknown component `{selector}`");
        };
        if !entry.disableable {
            anyhow::bail!("{source}: {path}: component `{selector}` is not disableable");
        }
        Ok(())
    }

    pub fn validate_profile_selector(&self, selector: &str, source: &str) -> anyhow::Result<()> {
        self.validate_selector(selector, source, "components", true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ComponentSwitch {
    pub enabled: bool,
}

pub fn parse_profile_json(
    source: &Path,
    json: &str,
    catalog: &ComponentCatalog,
) -> anyhow::Result<Profile> {
    let source = source.display().to_string();
    validate_profile_component_shape(&source, json)?;
    let profile = serde_json::from_str::<Profile>(json)
        .with_context(|| format!("{source}: invalid profile components object"))?;
    validate_component_map(&profile.components, catalog, &source, "components", true)?;
    Ok(profile)
}

fn validate_profile_component_shape(source: &str, json: &str) -> anyhow::Result<()> {
    let document = serde_json::from_str::<serde_json::Value>(json)
        .with_context(|| format!("{source}: invalid profile JSON"))?;
    let Some(components) = document.get("components") else {
        return Ok(());
    };
    let components = components
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("{source}: components: expected object"))?;
    for (selector, value) in components {
        let path = format!("components.{selector}");
        let setting = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("{source}: {path}: expected object"))?;
        for field in setting.keys() {
            if field != "enabled" {
                anyhow::bail!("{source}: {path}.{field}: unknown field");
            }
        }
        match setting.get("enabled") {
            Some(serde_json::Value::Bool(_)) => {}
            Some(_) => anyhow::bail!("{source}: {path}.enabled: expected boolean"),
            None => anyhow::bail!("{source}: {path}.enabled: missing field"),
        }
    }
    Ok(())
}

fn validate_component_map(
    components: &BTreeMap<String, ComponentSwitch>,
    catalog: &ComponentCatalog,
    source: &str,
    path: &str,
    allow_wildcard: bool,
) -> anyhow::Result<()> {
    for selector in components.keys() {
        catalog.validate_selector(
            selector,
            source,
            &format!("{path}.{selector}"),
            allow_wildcard,
        )?;
    }
    Ok(())
}

pub fn user_component_policy_path(omegon_home: &Path) -> PathBuf {
    // OMEGON_HOME is already the Omegon state root (normally ~/.omegon).
    omegon_home.join(USER_COMPONENT_POLICY_FILE)
}

pub fn load_user_component_policy(
    omegon_home: &Path,
    catalog: &ComponentCatalog,
) -> anyhow::Result<Option<UserComponentPolicy>> {
    let path = user_component_policy_path(omegon_home);
    let json = match std::fs::read_to_string(&path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    UserComponentPolicy::parse(&path, &json, catalog).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserComponentPolicyDocument {
    schema_version: u32,
    components: BTreeMap<String, ComponentSwitch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserComponentPolicy {
    source: PathBuf,
    components: BTreeMap<String, ComponentSwitch>,
}

impl UserComponentPolicy {
    pub fn parse(source: &Path, json: &str, catalog: &ComponentCatalog) -> anyhow::Result<Self> {
        let display = source.display().to_string();
        let document = serde_json::from_str::<UserComponentPolicyDocument>(json)
            .with_context(|| format!("{display}: invalid user-local component policy"))?;
        if document.schema_version != 1 {
            anyhow::bail!(
                "{display}: schemaVersion: unsupported component policy version {}",
                document.schema_version
            );
        }
        validate_component_map(&document.components, catalog, &display, "components", true)?;
        if let Some((selector, _)) = document
            .components
            .iter()
            .find(|(_, setting)| setting.enabled)
        {
            anyhow::bail!(
                "{display}: components.{selector}.enabled: user-local policy is deny-only"
            );
        }
        Ok(Self {
            source: source.to_path_buf(),
            components: document.components,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChildComponentDenyDocument {
    schema_version: u32,
    denied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChildComponentDeny {
    denied: Vec<String>,
}

impl ChildComponentDeny {
    pub fn parse_env(json: &str, catalog: &ComponentCatalog) -> anyhow::Result<Self> {
        let document = serde_json::from_str::<ChildComponentDenyDocument>(json)
            .with_context(|| format!("{CHILD_COMPONENT_DENIES_ENV}: invalid child deny policy"))?;
        if document.schema_version != 1 {
            anyhow::bail!(
                "{CHILD_COMPONENT_DENIES_ENV}: schemaVersion: unsupported child deny version {}",
                document.schema_version
            );
        }
        for selector in &document.denied {
            catalog.validate_selector(selector, CHILD_COMPONENT_DENIES_ENV, "denied", false)?;
        }
        Ok(Self {
            denied: document.denied,
        })
    }

    pub fn to_env_json(&self) -> String {
        serde_json::to_string(&ChildComponentDenyDocument {
            schema_version: 1,
            denied: self.denied.clone(),
        })
        .expect("child component deny document is serializable")
    }

    pub fn from_env(catalog: &ComponentCatalog) -> anyhow::Result<Option<Self>> {
        match std::env::var(CHILD_COMPONENT_DENIES_ENV) {
            Ok(json) => Self::parse_env(&json, catalog).map(Some),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(error).context(CHILD_COMPONENT_DENIES_ENV),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePolicySource {
    pub profile: String,
    pub path: String,
}

impl ProfilePolicySource {
    pub fn new(profile: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ComponentPolicySource {
    CompositionDefault,
    SelectedProfile { profile: String, path: String },
    UserLocal { path: PathBuf },
    ChildPropagation { env: &'static str },
    DeprecatedExtensionField { profile: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentPolicyEvidence {
    pub enabled: bool,
    pub source: ComponentPolicySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentPolicyDecision {
    pub component_id: String,
    pub enabled: bool,
    pub evidence: Vec<ComponentPolicyEvidence>,
    pub determining_source: ComponentPolicySource,
}

impl From<&ComponentPolicyDecision> for crate::surfaces::component::ComponentPolicySnapshot {
    fn from(decision: &ComponentPolicyDecision) -> Self {
        use crate::surfaces::component::ComponentDeterminingSourceProjection as Projection;
        let determining_source = match &decision.determining_source {
            ComponentPolicySource::CompositionDefault => Projection::CompositionDefault,
            ComponentPolicySource::SelectedProfile { profile, path } => {
                Projection::SelectedProfile {
                    profile: profile.clone(),
                    path: path.clone(),
                }
            }
            ComponentPolicySource::UserLocal { path } => {
                Projection::UserLocal { path: path.clone() }
            }
            ComponentPolicySource::ChildPropagation { env } => Projection::ChildPropagation {
                env: (*env).to_string(),
            },
            ComponentPolicySource::DeprecatedExtensionField { profile, path } => {
                Projection::DeprecatedExtensionField {
                    profile: profile.clone(),
                    path: path.clone(),
                }
            }
        };
        Self {
            component_id: decision.component_id.clone(),
            enabled: decision.enabled,
            determining_source,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResolvedComponentPolicy {
    components: BTreeMap<String, ComponentPolicyDecision>,
}

impl ResolvedComponentPolicy {
    pub fn component(&self, id: &str) -> Option<&ComponentPolicyDecision> {
        self.components.get(id)
    }

    pub fn decisions(&self) -> impl Iterator<Item = &ComponentPolicyDecision> {
        self.components.values()
    }

    pub fn denied_ids(&self) -> Vec<String> {
        self.components
            .values()
            .filter(|decision| !decision.enabled)
            .map(|decision| decision.component_id.clone())
            .collect()
    }

    pub fn child_deny(&self) -> ChildComponentDeny {
        ChildComponentDeny {
            denied: self.denied_ids(),
        }
    }
}

pub fn resolve_product_boot_policy(
    cwd: &Path,
    omegon_home: &Path,
) -> anyhow::Result<ResolvedComponentPolicy> {
    let catalog = ComponentCatalog::product_v1();
    let loaded = Profile::load_with_source(cwd);
    let profile_source = match &loaded.source {
        crate::settings::ProfileSource::Project(path)
        | crate::settings::ProfileSource::User(path) => ProfilePolicySource::new(
            loaded
                .profile
                .name
                .clone()
                .unwrap_or_else(|| loaded.source.label().to_string()),
            path.display().to_string(),
        ),
        crate::settings::ProfileSource::BuiltInDefault => {
            ProfilePolicySource::new("built-in-default", "built-in")
        }
    };
    let user = load_user_component_policy(omegon_home, &catalog)?;
    let child = ChildComponentDeny::from_env(&catalog)?;
    Ok(resolve_component_policy(
        &catalog,
        &loaded.profile,
        profile_source,
        user.as_ref(),
        child.as_ref(),
    ))
}

pub fn resolve_component_policy(
    catalog: &ComponentCatalog,
    profile: &Profile,
    profile_source: ProfilePolicySource,
    user: Option<&UserComponentPolicy>,
    child: Option<&ChildComponentDeny>,
) -> ResolvedComponentPolicy {
    let mut resolved = ResolvedComponentPolicy::default();
    for entry in catalog.entries.iter().filter(|entry| entry.disableable) {
        let mut evidence = vec![ComponentPolicyEvidence {
            enabled: entry.enabled_by_default,
            source: ComponentPolicySource::CompositionDefault,
        }];
        for selector in ["core:*", entry.id] {
            if let Some(setting) = profile.components.get(selector) {
                evidence.push(ComponentPolicyEvidence {
                    enabled: setting.enabled,
                    source: ComponentPolicySource::SelectedProfile {
                        profile: profile_source.profile.clone(),
                        path: profile_source.path.clone(),
                    },
                });
            }
        }
        if entry.id == "core:codescan"
            && profile
                .extensions
                .disabled
                .iter()
                .any(|name| name.eq_ignore_ascii_case("omegon-codescan"))
        {
            evidence.push(ComponentPolicyEvidence {
                enabled: false,
                source: ComponentPolicySource::DeprecatedExtensionField {
                    profile: profile_source.profile.clone(),
                    path: format!("{}.extensions.disabled", profile_source.path),
                },
            });
        }
        if let Some(user) = user {
            for selector in ["core:*", entry.id] {
                if let Some(setting) = user.components.get(selector) {
                    evidence.push(ComponentPolicyEvidence {
                        enabled: setting.enabled,
                        source: ComponentPolicySource::UserLocal {
                            path: user.source.clone(),
                        },
                    });
                }
            }
        }
        if child.is_some_and(|policy| policy.denied.iter().any(|id| id == entry.id)) {
            evidence.push(ComponentPolicyEvidence {
                enabled: false,
                source: ComponentPolicySource::ChildPropagation {
                    env: CHILD_COMPONENT_DENIES_ENV,
                },
            });
        }
        let enabled = evidence.iter().all(|item| item.enabled);
        let determining_source = if enabled {
            evidence
                .last()
                .expect("composition default evidence")
                .source
                .clone()
        } else {
            evidence
                .iter()
                .rev()
                .find(|item| !item.enabled)
                .expect("disabled decision has deny evidence")
                .source
                .clone()
        };
        resolved.components.insert(
            entry.id.to_string(),
            ComponentPolicyDecision {
                component_id: entry.id.to_string(),
                enabled,
                evidence,
                determining_source,
            },
        );
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Profile, ProfileExtensions};
    use std::path::Path;

    fn catalog() -> ComponentCatalog {
        ComponentCatalog::product_v1()
    }

    #[test]
    fn component_policy_profile_shape_is_strict() {
        let profile = parse_profile_json(
            Path::new("/repo/.omegon/profile.json"),
            r#"{"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .expect("valid component policy");
        assert!(!profile.components["core:codescan"].enabled);

        for invalid in [
            r#"{"components":{"core:codescan":{"enabled":false,"enabeld":false}}}"#,
            r#"{"components":{"core:codescan":{"enabled":"false"}}}"#,
            r#"{"components":[]}"#,
        ] {
            let error =
                parse_profile_json(Path::new("/repo/.omegon/profile.json"), invalid, &catalog())
                    .expect_err("invalid new object must be rejected");
            assert!(error.to_string().contains("/repo/.omegon/profile.json"));
            assert!(error.to_string().contains("components"));
        }
        let typo = parse_profile_json(
            Path::new("/repo/.omegon/profile.json"),
            r#"{"components":{"core:codescan":{"enabeld":false}}}"#,
            &catalog(),
        )
        .unwrap_err()
        .to_string();
        assert!(typo.contains("components.core:codescan.enabeld"), "{typo}");
    }

    #[test]
    fn component_catalog_rejects_bad_unknown_and_non_disableable_selectors() {
        let cases = [
            ("codescan", "malformed component selector"),
            ("core:**", "malformed component selector"),
            ("core:codesan", "unknown component"),
            ("core:constitutional-kernel", "not disableable"),
        ];
        for (selector, expected) in cases {
            let json = format!(r#"{{"components":{{"{selector}":{{"enabled":false}}}}}}"#);
            let error = parse_profile_json(
                Path::new("/repo/.omegon/profiles/compliance.json"),
                &json,
                &catalog(),
            )
            .expect_err("invalid selector must fail profile validation");
            let rendered = error.to_string();
            assert!(rendered.contains(expected), "{rendered}");
            assert!(rendered.contains("components"), "{rendered}");
            assert!(rendered.contains("compliance.json"), "{rendered}");
        }
    }

    #[test]
    fn resolver_applies_defaults_profile_and_monotonic_denies_with_provenance() {
        let profile = parse_profile_json(
            Path::new("/repo/.omegon/profile.json"),
            r#"{"components":{"core:codescan":{"enabled":true}}}"#,
            &catalog(),
        )
        .unwrap();
        let user = UserComponentPolicy::parse(
            Path::new("/home/me/.omegon/component-policy.json"),
            r#"{"schemaVersion":1,"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .unwrap();
        let child = ChildComponentDeny::parse_env(
            r#"{"schemaVersion":1,"denied":["core:codescan"]}"#,
            &catalog(),
        )
        .unwrap();

        let resolved = resolve_component_policy(
            &catalog(),
            &profile,
            ProfilePolicySource::new("selected-profile", "/repo/.omegon/profile.json"),
            Some(&user),
            Some(&child),
        );
        let codescan = resolved.component("core:codescan").unwrap();
        assert!(!codescan.enabled);
        assert!(
            codescan
                .evidence
                .iter()
                .any(|item| matches!(item.source, ComponentPolicySource::SelectedProfile { .. }))
        );
        assert!(
            codescan
                .evidence
                .iter()
                .any(|item| matches!(item.source, ComponentPolicySource::UserLocal { .. }))
        );
        assert!(matches!(
            codescan.determining_source,
            ComponentPolicySource::ChildPropagation { .. }
        ));
    }

    #[test]
    fn determining_provenance_is_exact_for_profile_user_and_child_denies() {
        let profile_path = "/repo/.omegon/profiles/compliance.json";
        let profile = parse_profile_json(
            Path::new(profile_path),
            r#"{"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .unwrap();
        let profile_only = resolve_component_policy(
            &catalog(),
            &profile,
            ProfilePolicySource::new("compliance", profile_path),
            None,
            None,
        );
        assert_eq!(
            profile_only
                .component("core:codescan")
                .unwrap()
                .determining_source,
            ComponentPolicySource::SelectedProfile {
                profile: "compliance".into(),
                path: profile_path.into(),
            }
        );

        let user_path = Path::new("/home/operator/.omegon/component-policy.json");
        let user = UserComponentPolicy::parse(
            user_path,
            r#"{"schemaVersion":1,"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .unwrap();
        let user_denied = resolve_component_policy(
            &catalog(),
            &Profile::default(),
            ProfilePolicySource::new("default", "built-in"),
            Some(&user),
            None,
        );
        assert_eq!(
            user_denied
                .component("core:codescan")
                .unwrap()
                .determining_source,
            ComponentPolicySource::UserLocal {
                path: user_path.into(),
            }
        );

        let child = ChildComponentDeny::parse_env(
            r#"{"schemaVersion":1,"denied":["core:codescan"]}"#,
            &catalog(),
        )
        .unwrap();
        let child_denied = resolve_component_policy(
            &catalog(),
            &Profile::default(),
            ProfilePolicySource::new("default", "built-in"),
            None,
            Some(&child),
        );
        assert_eq!(
            child_denied
                .component("core:codescan")
                .unwrap()
                .determining_source,
            ComponentPolicySource::ChildPropagation {
                env: CHILD_COMPONENT_DENIES_ENV,
            }
        );
    }

    #[test]
    fn semantic_component_projection_distinguishes_policy_and_runtime_states() {
        use crate::surfaces::component::{
            ComponentDeterminingSourceProjection, ComponentPackageProjection,
            ComponentProcessProvenance, ComponentRuntimeEvidence, ComponentState,
            ComponentStatusProjection,
        };

        let enabled = resolve_component_policy(
            &catalog(),
            &Profile::default(),
            ProfilePolicySource::new("default", "built-in"),
            None,
            None,
        );
        let package = ComponentPackageProjection {
            identity: "omegon-codescan".into(),
            source: Some("/opt/omegon/share/omegon/extensions/omegon-codescan".into()),
            present: true,
        };
        let decision = enabled.component("core:codescan").unwrap();
        for (runtime, expected) in [
            (
                ComponentRuntimeEvidence::NotObserved,
                ComponentState::Packaged,
            ),
            (ComponentRuntimeEvidence::Absent, ComponentState::Absent),
            (
                ComponentRuntimeEvidence::Incompatible,
                ComponentState::Incompatible,
            ),
            (ComponentRuntimeEvidence::Failed, ComponentState::Failed),
            (
                ComponentRuntimeEvidence::Quarantined,
                ComponentState::Quarantined,
            ),
        ] {
            let status =
                ComponentStatusProjection::new(&decision.into(), Some(package.clone()), runtime);
            assert_eq!(status.state, expected);
            assert!(status.process.is_none());
        }

        let process = ComponentProcessProvenance {
            identity: "omegon-codescan".into(),
            source_digest: "sha256:abc".into(),
            pid: Some(42),
        };
        let healthy = ComponentStatusProjection::new(
            &decision.into(),
            Some(package.clone()),
            ComponentRuntimeEvidence::Healthy(process.clone()),
        );
        assert_eq!(healthy.state, ComponentState::Healthy);
        assert_eq!(healthy.process, Some(process));

        let denied_profile = parse_profile_json(
            Path::new("/repo/.omegon/profiles/compliance.json"),
            r#"{"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .unwrap();
        let denied = resolve_component_policy(
            &catalog(),
            &denied_profile,
            ProfilePolicySource::new("compliance", "/repo/.omegon/profiles/compliance.json"),
            None,
            None,
        );
        let status = ComponentStatusProjection::new(
            &denied.component("core:codescan").unwrap().into(),
            Some(package),
            ComponentRuntimeEvidence::NotObserved,
        );
        assert_eq!(status.state, ComponentState::DisabledByProfile);
        assert!(status.restart_bound);
        assert!(matches!(
            status.determining_source,
            ComponentDeterminingSourceProjection::SelectedProfile { ref profile, ref path }
                if profile == "compliance" && path.ends_with("compliance.json")
        ));
    }

    #[test]
    fn wildcard_and_composition_default_are_resolved_deny_first() {
        let exact_deny = parse_profile_json(
            Path::new("profile.json"),
            r#"{"components":{"core:codescan":{"enabled":false}}}"#,
            &catalog(),
        )
        .unwrap();
        let exact = resolve_component_policy(
            &catalog(),
            &exact_deny,
            ProfilePolicySource::new("selected-profile", "profile.json"),
            None,
            None,
        );
        assert!(matches!(
            exact.component("core:codescan").unwrap().determining_source,
            ComponentPolicySource::SelectedProfile { .. }
        ));

        let profile = parse_profile_json(
            Path::new("profile.json"),
            r#"{"components":{"core:*":{"enabled":false},"core:codescan":{"enabled":true}}}"#,
            &catalog(),
        )
        .unwrap();
        let resolved = resolve_component_policy(
            &catalog(),
            &profile,
            ProfilePolicySource::new("selected-profile", "profile.json"),
            None,
            None,
        );
        assert!(!resolved.component("core:codescan").unwrap().enabled);

        let default = resolve_component_policy(
            &catalog(),
            &Profile::default(),
            ProfilePolicySource::new("built-in-default", "built-in"),
            None,
            None,
        );
        assert!(default.component("core:codescan").unwrap().enabled);
    }

    #[test]
    fn user_local_policy_is_versioned_deny_only_and_has_a_stable_home_path() {
        assert_eq!(
            user_component_policy_path(Path::new("/srv/omegon")),
            Path::new("/srv/omegon/component-policy.json")
        );
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            user_component_policy_path(home.path()),
            r#"{"schemaVersion":1,"components":{"core:codescan":{"enabled":false}}}"#,
        )
        .unwrap();
        assert!(
            load_user_component_policy(home.path(), &catalog())
                .unwrap()
                .is_some()
        );
        for invalid in [
            r#"{"schemaVersion":2,"components":{}}"#,
            r#"{"schemaVersion":1,"extra":true,"components":{}}"#,
            r#"{"schemaVersion":1,"components":{"core:codescan":{"enabled":true}}}"#,
        ] {
            let error = UserComponentPolicy::parse(
                Path::new("/srv/omegon/component-policy.json"),
                invalid,
                &catalog(),
            )
            .expect_err("local policy must be strict and deny-only");
            assert!(error.to_string().contains("component-policy.json"));
        }
    }

    #[test]
    fn child_deny_contract_is_versioned_strict_and_exact() {
        assert_eq!(
            CHILD_COMPONENT_DENIES_ENV,
            "OMEGON_CHILD_COMPONENT_DENIES_V1"
        );
        for invalid in [
            r#"{"schemaVersion":2,"denied":[]}"#,
            r#"{"schemaVersion":1,"denied":["core:*"]}"#,
            r#"{"schemaVersion":1,"denied":["core:codesan"]}"#,
            r#"{"schemaVersion":1,"denied":[],"extra":true}"#,
        ] {
            assert!(ChildComponentDeny::parse_env(invalid, &catalog()).is_err());
        }
    }

    #[test]
    fn legacy_codescan_deny_resolves_with_deprecation_and_save_migrates_only_it() {
        let profile = Profile {
            extensions: ProfileExtensions {
                enabled: vec!["scribe".into()],
                disabled: vec!["omegon-codescan".into(), "vox".into()],
            },
            ..Profile::default()
        };
        let resolved = resolve_component_policy(
            &catalog(),
            &profile,
            ProfilePolicySource::new("selected-profile", "/repo/.omegon/profile.json"),
            None,
            None,
        );
        let decision = resolved.component("core:codescan").unwrap();
        assert!(!decision.enabled);
        assert!(decision.evidence.iter().any(|item| matches!(
            item.source,
            ComponentPolicySource::DeprecatedExtensionField { .. }
        )));

        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        profile.save(temp.path()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(
            &std::fs::read(temp.path().join(".omegon/profile.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["components"]["core:codescan"]["enabled"], false);
        assert_eq!(
            saved["extensions"]["enabled"],
            serde_json::json!(["scribe"])
        );
        assert_eq!(saved["extensions"]["disabled"], serde_json::json!(["vox"]));
    }

    #[test]
    fn product_boot_policy_loads_all_sources_and_is_generation_immutable() {
        let _lock = crate::test_support::env::lock();
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join(".omegon")).unwrap();
        std::fs::write(
            project.path().join(".omegon/profile.json"),
            r#"{"components":{"core:codescan":{"enabled":true}}}"#,
        )
        .unwrap();
        std::fs::write(
            home.path().join("component-policy.json"),
            r#"{"schemaVersion":1,"components":{"core:codescan":{"enabled":false}}}"#,
        )
        .unwrap();
        unsafe {
            std::env::set_var(
                CHILD_COMPONENT_DENIES_ENV,
                r#"{"schemaVersion":1,"denied":["core:codescan"]}"#,
            );
        }

        let first = resolve_product_boot_policy(project.path(), home.path()).unwrap();
        let decision = first.component("core:codescan").unwrap();
        assert!(!decision.enabled);
        assert!(decision.evidence.iter().any(|evidence| matches!(
            evidence.source,
            ComponentPolicySource::SelectedProfile { .. }
        )));
        assert!(
            decision
                .evidence
                .iter()
                .any(|evidence| matches!(evidence.source, ComponentPolicySource::UserLocal { .. }))
        );
        assert!(matches!(
            decision.determining_source,
            ComponentPolicySource::ChildPropagation { .. }
        ));

        std::fs::remove_file(home.path().join("component-policy.json")).unwrap();
        unsafe { std::env::remove_var(CHILD_COMPONENT_DENIES_ENV) };
        let second = resolve_product_boot_policy(project.path(), home.path()).unwrap();
        assert!(!first.component("core:codescan").unwrap().enabled);
        assert!(second.component("core:codescan").unwrap().enabled);
    }
}
