//! Custom Pkl module reader for the `omegon://` URI scheme.
//!
//! Resolves two namespaces:
//!
//! - `omegon://schema/<name>` — schema modules embedded at compile time
//!   (e.g. `AgentManifest.pkl`, `TriggerConfig.pkl`).
//! - `omegon://catalog/<id>/agent.pkl` — installed catalog agent bundles
//!   read from `$OMEGON_HOME/catalog/<id>/agent.pkl`.
//!
//! Register via [`omegon_eval_options`] when calling
//! `rpkl::from_config_with_options`.
//!
//! # Module reference format
//!
//! Schema modules:
//! ```pkl
//! amends "omegon://schema/AgentManifest.pkl"
//! ```
//!
//! Cross-agent inheritance (user overlay extending an installed base):
//! ```pkl
//! amends "omegon://catalog/styrene.bd-agent/agent.pkl"
//! ```

use std::{collections::HashMap, path::PathBuf};

use rpkl::api::evaluator::EvaluatorOptions;
use rpkl::api::reader::{PathElements, PklModuleReader};

// ── Embedded schema files ────────────────────────────────────────────────────

static SCHEMAS: &[(&str, &str)] = &[
    (
        "AgentManifest.pkl",
        include_str!("../../../../pkl/AgentManifest.pkl"),
    ),
    (
        "TriggerConfig.pkl",
        include_str!("../../../../pkl/TriggerConfig.pkl"),
    ),
    (
        "PluginManifest.pkl",
        include_str!("../../../../pkl/PluginManifest.pkl"),
    ),
    (
        "ExtensionManifest.pkl",
        include_str!("../../../../pkl/ExtensionManifest.pkl"),
    ),
    (
        "McpConfig.pkl",
        include_str!("../../../../pkl/McpConfig.pkl"),
    ),
    (
        "SkillManifest.pkl",
        include_str!("../../../../pkl/SkillManifest.pkl"),
    ),
    ("Profile.pkl", include_str!("../../../../pkl/Profile.pkl")),
    ("TaskSpec.pkl", include_str!("../../../../pkl/TaskSpec.pkl")),
    (
        "RouteMatrix.pkl",
        include_str!("../../../../pkl/RouteMatrix.pkl"),
    ),
    (
        "CodexIntegration.pkl",
        include_str!("../../../../pkl/CodexIntegration.pkl"),
    ),
];

// ── Reader ───────────────────────────────────────────────────────────────────

pub struct OmegonModuleReader {
    omegon_home: PathBuf,
    catalog_snapshots: Option<HashMap<String, PathBuf>>,
}

impl OmegonModuleReader {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            omegon_home: crate::paths::omegon_home()?,
            catalog_snapshots: None,
        })
    }

    fn with_catalog_snapshots(catalog_snapshots: HashMap<String, PathBuf>) -> Self {
        Self {
            omegon_home: PathBuf::new(),
            catalog_snapshots: Some(catalog_snapshots),
        }
    }

    fn catalog_path(&self, rest: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let (bundle, relative) = rest
            .split_once('/')
            .ok_or_else(|| format!("catalog URI requires a bundle and file: {rest}"))?;
        let root = match &self.catalog_snapshots {
            Some(snapshots) => snapshots
                .get(bundle)
                .ok_or_else(|| format!("catalog bundle is not admitted: {bundle}"))?
                .clone(),
            None => self.omegon_home.join("catalog").join(bundle),
        };
        let relative = std::path::Path::new(relative);
        if relative.components().next().is_none()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(format!("catalog URI contains an unsafe path: {rest}").into());
        }
        Ok(root.join(relative))
    }
}

impl PklModuleReader for OmegonModuleReader {
    fn scheme(&self) -> &str {
        "omegon"
    }

    fn has_hierarchical_uris(&self) -> bool {
        true
    }

    fn is_local(&self) -> bool {
        true
    }

    fn read(&self, uri: &str) -> Result<String, Box<dyn std::error::Error>> {
        let path = uri
            .strip_prefix("omegon://")
            .ok_or_else(|| format!("unexpected URI scheme: {uri}"))?;

        let (namespace, rest) = path
            .split_once('/')
            .ok_or_else(|| format!("omegon:// URI missing namespace segment: {uri}"))?;

        match namespace {
            "schema" => SCHEMAS
                .iter()
                .find(|(name, _)| *name == rest)
                .map(|(_, content)| content.to_string())
                .ok_or_else(|| format!("unknown schema module: {rest}").into()),
            "catalog" => {
                let file_path = self.catalog_path(rest)?;
                let metadata = std::fs::metadata(&file_path)?;
                if !metadata.is_file() || metadata.len() > 1024 * 1024 {
                    return Err(
                        format!("catalog module is not a bounded regular file: {rest}").into(),
                    );
                }
                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    format!("failed to read catalog module {}: {e}", file_path.display())
                })?;
                validate_catalog_pkl_module(&content)?;
                Ok(content)
            }
            other => Err(format!("unknown omegon:// namespace '{other}' in URI: {uri}").into()),
        }
    }

    fn list(&self, uri: &str) -> Result<Vec<PathElements>, Box<dyn std::error::Error>> {
        let path = uri
            .strip_prefix("omegon://")
            .ok_or_else(|| format!("unexpected URI scheme: {uri}"))?;

        let path = path.trim_end_matches('/');

        // Split on first '/' to get namespace + optional sub-path.
        let (namespace, sub) = match path.split_once('/') {
            Some((ns, rest)) => (ns, Some(rest.trim_end_matches('/'))),
            None => (path, None),
        };

        match namespace {
            "schema" => {
                // Sub-path listing within schema is not meaningful; return all entries.
                Ok(SCHEMAS
                    .iter()
                    .map(|(name, _)| PathElements::new(*name, false))
                    .collect())
            }
            "catalog" => match sub {
                None => {
                    if let Some(snapshots) = &self.catalog_snapshots {
                        Ok(snapshots
                            .keys()
                            .map(|name| PathElements::new(name.clone(), true))
                            .collect())
                    } else {
                        let catalog_dir = self.omegon_home.join("catalog");
                        let entries = std::fs::read_dir(&catalog_dir)
                            .map_err(|e| format!("cannot list catalog: {e}"))?;
                        Ok(entries
                            .flatten()
                            .filter(|e| e.path().is_dir())
                            .map(|e| {
                                PathElements::new(
                                    e.file_name().to_string_lossy().into_owned(),
                                    true,
                                )
                            })
                            .collect())
                    }
                }
                Some(agent_id) => {
                    let bundle_dir = match &self.catalog_snapshots {
                        Some(snapshots) => snapshots
                            .get(agent_id)
                            .ok_or_else(|| format!("catalog bundle is not admitted: {agent_id}"))?
                            .clone(),
                        None => self.omegon_home.join("catalog").join(agent_id),
                    };
                    let entries = std::fs::read_dir(&bundle_dir)
                        .map_err(|e| format!("cannot list catalog bundle {agent_id}: {e}"))?;
                    Ok(entries
                        .flatten()
                        .map(|e| {
                            let is_dir = e.path().is_dir();
                            PathElements::new(e.file_name().to_string_lossy().into_owned(), is_dir)
                        })
                        .collect())
                }
            },
            other => Err(format!("cannot list omegon:// namespace '{other}'").into()),
        }
    }
}

// ── Evaluator options factory ─────────────────────────────────────────────────

/// Build `EvaluatorOptions` with the `omegon://` module reader registered.
///
/// Falls back to default options (no custom reader) if `omegon_home` cannot
/// be determined — the evaluator will still work for files that don't use
/// `omegon://` URIs.
pub fn omegon_eval_options() -> EvaluatorOptions {
    match OmegonModuleReader::new() {
        Ok(reader) => EvaluatorOptions::new().add_client_module_readers(reader),
        Err(e) => {
            tracing::warn!("could not initialize omegon:// pkl module reader: {e}");
            EvaluatorOptions::default()
        }
    }
}

pub(crate) fn omegon_eval_options_for_catalog(
    snapshots: HashMap<String, PathBuf>,
) -> EvaluatorOptions {
    EvaluatorOptions::new()
        .add_client_module_readers(OmegonModuleReader::with_catalog_snapshots(snapshots))
        .timeout_seconds(10)
}

pub(crate) fn validate_catalog_pkl_module(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    for line in content.lines().map(str::trim) {
        if (line.starts_with("amends ") || line.starts_with("extends "))
            && !line
                .split_once('"')
                .and_then(|(_, rest)| rest.split_once('"'))
                .is_some_and(|(uri, _)| uri.starts_with("omegon://"))
        {
            return Err("catalog Pkl inheritance must use an omegon:// module".into());
        }
        if !line.starts_with("//")
            && (line.contains("import(")
                || line.contains("import*(")
                || line.contains("read(")
                || line.contains("read*(")
                || line.contains("read?("))
        {
            return Err("catalog Pkl cannot import modules or read external resources".into());
        }
    }
    for string in content.split('"').skip(1).step_by(2) {
        let drive_absolute = string.as_bytes().get(1) == Some(&b':');
        if string.starts_with('/')
            || string.starts_with('\\')
            || string.starts_with("file:")
            || string.split(['/', '\\']).any(|part| part == "..")
            || drive_absolute
        {
            return Err(
                "catalog Pkl contains a filesystem module reference outside its bundle".into(),
            );
        }
    }
    Ok(())
}
