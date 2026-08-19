//! Agent catalog — discovers and lists available agent bundles.
//!
//! Agent bundles live in `$OMEGON_HOME/catalog/` as directories containing
//! an `agent.pkl` or `agent.toml` manifest. The catalog provides discovery
//! and resolution for the `--agent` CLI flag and Auspex spawn contracts.
//!
//! # Installation
//!
//! `cmd_install(offline)` fetches the upstream armory registry and downloads
//! each agent's files. When `offline = true` (or the network is unreachable),
//! it falls back to the copies embedded in the binary at compile time.

use std::{
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::agent_manifest::{self, ResolvedManifest};

const MAX_CATALOG_ENTRIES: usize = 10_000;

struct AdmittedCatalogGeneration {
    _admission: crate::contribution_loading::GuardedContributionDirectory,
    bundles: Vec<AdmittedCatalogBundle>,
}

struct AdmittedCatalogBundle {
    raw_name: Vec<u8>,
    snapshot: crate::contribution_loading::ContributionSnapshot,
}

pub struct CatalogListing {
    entries: Vec<CatalogEntry>,
    _generation: Option<AdmittedCatalogGeneration>,
}

impl Deref for CatalogListing {
    type Target = [CatalogEntry];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

pub struct AdmittedResolvedManifest {
    resolved: ResolvedManifest,
    display_bundle_dir: PathBuf,
    _generation: Option<AdmittedCatalogGeneration>,
    _direct_snapshot: Option<crate::contribution_loading::ContributionSnapshot>,
}

pub(crate) struct AdmittedCatalogManifests {
    manifests: Vec<ResolvedManifest>,
    _generation: Option<AdmittedCatalogGeneration>,
}

impl Deref for AdmittedCatalogManifests {
    type Target = [ResolvedManifest];

    fn deref(&self) -> &Self::Target {
        &self.manifests
    }
}

impl Deref for AdmittedResolvedManifest {
    type Target = ResolvedManifest;

    fn deref(&self) -> &Self::Target {
        &self.resolved
    }
}

impl AdmittedResolvedManifest {
    pub(crate) fn resolved(&self) -> &ResolvedManifest {
        &self.resolved
    }

    pub(crate) fn display_bundle_dir(&self) -> &Path {
        &self.display_bundle_dir
    }
}

/// Base URL for the upstream armory catalog.
const ARMORY_BASE: &str = "https://raw.githubusercontent.com/styrene-lab/omegon-armory/main";

/// Parsed entry from `catalog-registry.toml`.
/// Only `files` is consumed; remaining fields are defined in the registry for
/// documentation and future use (toml deserialization ignores unknown fields by default).
#[derive(serde::Deserialize)]
struct ArmoryEntry {
    files: Vec<String>,
}

/// A catalog agent bundle with all files embedded at compile time.
struct BundledAgent {
    id: &'static str,
    /// TOML manifest — always present; used as fallback when pkl binary unavailable.
    agent_toml: &'static str,
    /// Pkl manifest — present for agents that have an agent.pkl.
    /// Enables `amends "omegon://catalog/<id>/agent.pkl"` inheritance for user overlays.
    agent_pkl: Option<&'static str>,
    persona_md: &'static str,
    mind_facts: Option<&'static str>,
}

const BUNDLED: &[BundledAgent] = &[
    BundledAgent {
        id: "styrene.bd-agent",
        agent_toml: include_str!("../../../../catalog/styrene.bd-agent/agent.toml"),
        agent_pkl: Some(include_str!(
            "../../../../catalog/styrene.bd-agent/agent.pkl"
        )),
        persona_md: include_str!("../../../../catalog/styrene.bd-agent/PERSONA.md"),
        mind_facts: Some(include_str!(
            "../../../../catalog/styrene.bd-agent/mind/facts.jsonl"
        )),
    },
    BundledAgent {
        id: "styrene.coding-agent",
        agent_toml: include_str!("../../../../catalog/styrene.coding-agent/agent.toml"),
        agent_pkl: None,
        persona_md: include_str!("../../../../catalog/styrene.coding-agent/PERSONA.md"),
        mind_facts: Some(include_str!(
            "../../../../catalog/styrene.coding-agent/mind/facts.jsonl"
        )),
    },
    BundledAgent {
        id: "styrene.community-agent",
        agent_toml: include_str!("../../../../catalog/styrene.community-agent/agent.toml"),
        agent_pkl: None,
        persona_md: include_str!("../../../../catalog/styrene.community-agent/PERSONA.md"),
        mind_facts: Some(include_str!(
            "../../../../catalog/styrene.community-agent/mind/facts.jsonl"
        )),
    },
    BundledAgent {
        id: "styrene.discord-agent",
        agent_toml: include_str!("../../../../catalog/styrene.discord-agent/agent.toml"),
        agent_pkl: Some(include_str!(
            "../../../../catalog/styrene.discord-agent/agent.pkl"
        )),
        persona_md: include_str!("../../../../catalog/styrene.discord-agent/PERSONA.md"),
        mind_facts: None,
    },
    BundledAgent {
        id: "styrene.infra-engineer",
        agent_toml: include_str!("../../../../catalog/styrene.infra-engineer/agent.toml"),
        agent_pkl: None,
        persona_md: include_str!("../../../../catalog/styrene.infra-engineer/PERSONA.md"),
        mind_facts: Some(include_str!(
            "../../../../catalog/styrene.infra-engineer/mind/facts.jsonl"
        )),
    },
    BundledAgent {
        id: "styrene.slack-agent",
        agent_toml: include_str!("../../../../catalog/styrene.slack-agent/agent.toml"),
        agent_pkl: Some(include_str!(
            "../../../../catalog/styrene.slack-agent/agent.pkl"
        )),
        persona_md: include_str!("../../../../catalog/styrene.slack-agent/PERSONA.md"),
        mind_facts: None,
    },
];

pub fn remove(omegon_home: &Path, selector: &str) -> anyhow::Result<()> {
    omegon_maintenance_contracts::validate_child_name(selector.as_bytes())?;
    let mutation =
        crate::contribution_loading::GuardedContributionMutationDirectory::open_existing(
            omegon_home,
            &[b"catalog"],
            omegon_home,
            omegon_maintenance_contracts::ContributionKind::Catalog,
            "user",
        )?
        .ok_or_else(|| anyhow::anyhow!("catalog agent '{selector}' not found"))?;
    let mut names = mutation.entry_names(MAX_CATALOG_ENTRIES)?;
    names.sort();
    for raw_name in &names {
        if crate::contribution_loading::is_internal_contribution_entry(raw_name) {
            continue;
        }
        let raw_matches = raw_name == selector.as_bytes();
        if raw_matches {
            mutation.remove_entry(raw_name)?;
            return Ok(());
        }
        let manifest_matches = if let Some(directory) = mutation.open_directory(raw_name)? {
            crate::contribution_loading::read_file_at(&directory, b"agent.toml", 1024 * 1024)?
                .and_then(|bytes| {
                    std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|text| toml::from_str::<agent_manifest::AgentManifest>(text).ok())
                })
                .is_some_and(|manifest| manifest.agent.id == selector)
        } else {
            false
        };
        if manifest_matches {
            mutation.remove_entry(raw_name)?;
            return Ok(());
        }
    }
    anyhow::bail!("catalog agent '{selector}' not found")
}

/// List bundled agents and their installation status.
pub fn cmd_list() -> anyhow::Result<()> {
    let home = crate::paths::omegon_home()?;
    let cat_dir = home.join("catalog");
    let installed_catalog = list(&home)?;
    println!("Bundled agents ({})\n", BUNDLED.len());
    for bundle in BUNDLED {
        let installed = installed_catalog.iter().any(|entry| entry.id == bundle.id);
        let marker = if installed { "✓" } else { "○" };
        let (name, domain) = extract_agent_meta(bundle.agent_toml);
        println!("  {marker} {id:<30}  {name}  [{domain}]", id = bundle.id);
    }
    println!("\nInstall path: {}", cat_dir.display());
    Ok(())
}

/// Install agents to `~/.omegon/catalog/`.
///
/// Fetches from the upstream armory unless `offline` is `true` or the network
/// is unavailable, in which case it falls back to the copies embedded in the
/// binary at compile time.
pub async fn cmd_install(offline: bool) -> anyhow::Result<()> {
    let home = crate::paths::omegon_home()?;

    if !offline {
        match install_from_upstream(&home).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("  ! upstream fetch failed ({e}), falling back to bundled");
            }
        }
    }

    install_from_bundled(&home)
}

fn print_install_summary(installed: usize, updated: usize, cat_dir: &Path) {
    println!(
        "\n{installed} agent(s) installed, {updated} updated → {}",
        cat_dir.display()
    );
    println!("Agents are active immediately in new sessions.");
}

/// Download all agents listed in the armory `catalog-registry.toml`.
async fn install_from_upstream(omegon_home: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let registry_url = format!("{ARMORY_BASE}/catalog-registry.toml");
    let registry_bytes = response_bytes_bounded(
        client.get(&registry_url).send().await?.error_for_status()?,
        1024 * 1024,
        "catalog registry",
    )
    .await?;
    let registry_text = std::str::from_utf8(&registry_bytes)?;

    let registry: std::collections::HashMap<String, ArmoryEntry> = toml::from_str(registry_text)?;
    if registry.len() > MAX_CATALOG_ENTRIES {
        anyhow::bail!("catalog registry exceeds the entry limit");
    }

    // Sort for stable output order.
    let mut ids: Vec<&String> = registry.keys().collect();
    ids.sort();

    let prepared = PreparedCatalogDirectory::new()?;
    let mut candidates = Vec::new();
    let mut total_files = 0_usize;
    let mut total_bytes = 0_usize;

    for id in ids {
        omegon_maintenance_contracts::validate_child_name(id.as_bytes())?;
        let entry = &registry[id];
        let bundle_dir = prepared.0.join(id);
        std::fs::create_dir_all(&bundle_dir)?;

        if entry.files.len() > MAX_CATALOG_ENTRIES {
            anyhow::bail!("catalog bundle '{id}' exceeds the file-count limit");
        }
        total_files = total_files
            .checked_add(entry.files.len())
            .ok_or_else(|| anyhow::anyhow!("catalog file count overflow"))?;
        if total_files > 100_000 {
            anyhow::bail!("catalog installation exceeds the total file-count limit");
        }
        let mut paths = std::collections::HashSet::new();
        let mut aggregate_bytes = 0_usize;
        for file in &entry.files {
            validate_catalog_relative_path(file)?;
            if !paths.insert(file) {
                anyhow::bail!("catalog bundle '{id}' contains duplicate file paths");
            }
            let url = format!("{ARMORY_BASE}/catalog/{id}/{file}");
            let bytes = response_bytes_bounded(
                client.get(&url).send().await?.error_for_status()?,
                16 * 1024 * 1024,
                file,
            )
            .await?;
            aggregate_bytes = aggregate_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| anyhow::anyhow!("catalog bundle size overflow"))?;
            if aggregate_bytes > 128 * 1024 * 1024 {
                anyhow::bail!("catalog bundle '{id}' exceeds the aggregate size limit");
            }
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| anyhow::anyhow!("catalog installation size overflow"))?;
            if total_bytes > 512 * 1024 * 1024 {
                anyhow::bail!("catalog installation exceeds the total size limit");
            }
            let dest = bundle_dir.join(file);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, &bytes)?;
        }
        candidates.push((id.to_string(), bundle_dir));
    }
    publish_catalog_candidates(omegon_home, &candidates)
}

/// Write the compile-time bundled agents to disk.
fn install_from_bundled(omegon_home: &Path) -> anyhow::Result<()> {
    let prepared = PreparedCatalogDirectory::new()?;
    let mut candidates = Vec::new();
    for bundle in BUNDLED {
        omegon_maintenance_contracts::validate_child_name(bundle.id.as_bytes())?;
        let bundle_dir = prepared.0.join(bundle.id);
        std::fs::create_dir_all(&bundle_dir)?;

        let toml_path = bundle_dir.join("agent.toml");
        std::fs::write(&toml_path, bundle.agent_toml)?;
        if let Some(pkl) = bundle.agent_pkl {
            std::fs::write(bundle_dir.join("agent.pkl"), pkl)?;
        }
        std::fs::write(bundle_dir.join("PERSONA.md"), bundle.persona_md)?;
        if let Some(facts) = bundle.mind_facts {
            let mind_dir = bundle_dir.join("mind");
            std::fs::create_dir_all(&mind_dir)?;
            std::fs::write(mind_dir.join("facts.jsonl"), facts)?;
        }

        candidates.push((bundle.id.to_string(), bundle_dir));
    }
    publish_catalog_candidates(omegon_home, &candidates)
}

fn publish_catalog_candidates(
    omegon_home: &Path,
    candidates: &[(String, PathBuf)],
) -> anyhow::Result<()> {
    let mutation =
        crate::contribution_loading::GuardedContributionMutationDirectory::open_or_create(
            omegon_home,
            &[b"catalog"],
            omegon_home,
            omegon_maintenance_contracts::ContributionKind::Catalog,
            "user",
        )?;
    let existing = mutation.entry_names(MAX_CATALOG_ENTRIES)?;
    let new_entries = candidates
        .iter()
        .filter(|(id, _)| !existing.iter().any(|name| name == id.as_bytes()))
        .count();
    if existing.len().saturating_add(new_entries) > MAX_CATALOG_ENTRIES {
        anyhow::bail!("catalog installation would exceed the entry limit");
    }
    let mut installed = 0;
    let mut updated = 0;
    for (id, path) in candidates {
        let source = std::fs::File::open(path)?;
        let replacing = existing.iter().any(|name| name == id.as_bytes());
        mutation.import_directory(id.as_bytes(), &source, true)?;
        if replacing {
            println!("  ↑ {id}  (updated)");
            updated += 1;
        } else {
            println!("  + {id}");
            installed += 1;
        }
    }
    print_install_summary(installed, updated, &omegon_home.join("catalog"));
    Ok(())
}

async fn response_bytes_bounded(
    response: reqwest::Response,
    limit: usize,
    label: &str,
) -> anyhow::Result<Vec<u8>> {
    use futures_util::StreamExt;

    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        anyhow::bail!("{label} exceeds the size limit");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            anyhow::bail!("{label} exceeds the size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_catalog_relative_path(path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    if path.components().next().is_none()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        anyhow::bail!("catalog registry contains an unsafe file path: {path:?}");
    }
    Ok(())
}

struct PreparedCatalogDirectory(PathBuf);

impl PreparedCatalogDirectory {
    fn new() -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!("omegon-catalog-{}", uuid::Uuid::new_v4()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for PreparedCatalogDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            tracing::warn!(path = %self.0.display(), %error, "could not remove catalog preparation directory");
        }
    }
}

/// Parse name and domain from an embedded agent.toml string.
fn extract_agent_meta(toml_src: &str) -> (String, String) {
    #[derive(serde::Deserialize)]
    struct AgentSection {
        name: Option<String>,
        domain: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Outer {
        agent: Option<AgentSection>,
    }
    let parsed: Outer = toml::from_str(toml_src).unwrap_or(Outer { agent: None });
    let section = parsed.agent.unwrap_or(AgentSection {
        name: None,
        domain: None,
    });
    (
        section.name.unwrap_or_default(),
        section.domain.unwrap_or_default(),
    )
}

/// Summary of an available agent in the catalog.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain: String,
    pub bundle_dir: PathBuf,
}

/// Discover all admitted agent bundles in the catalog directory.
pub fn list(omegon_home: &Path) -> anyhow::Result<CatalogListing> {
    let Some(generation) = admit_catalog(omegon_home)? else {
        return Ok(CatalogListing {
            entries: Vec::new(),
            _generation: None,
        });
    };
    let snapshots = catalog_snapshot_map(&generation)?;
    let mut catalog = Vec::new();
    for bundle in &generation.bundles {
        match agent_manifest::load_with_catalog_snapshots(bundle.snapshot.path(), snapshots.clone())
        {
            Ok(resolved) => {
                let m = &resolved.manifest;
                catalog.push(CatalogEntry {
                    id: m.agent.id.clone(),
                    name: m.agent.name.clone(),
                    version: m.agent.version.clone(),
                    description: m.agent.description.clone(),
                    domain: m.agent.domain.clone(),
                    bundle_dir: catalog_display_path(omegon_home, &bundle.raw_name),
                });
            }
            Err(e) => {
                tracing::warn!(
                    path = %catalog_display_path(omegon_home, &bundle.raw_name).display(),
                    error = %e,
                    "skipping invalid agent bundle"
                );
            }
        }
    }

    catalog.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(CatalogListing {
        entries: catalog,
        _generation: Some(generation),
    })
}

pub(crate) fn resolved_manifests(omegon_home: &Path) -> anyhow::Result<AdmittedCatalogManifests> {
    let Some(generation) = admit_catalog(omegon_home)? else {
        return Ok(AdmittedCatalogManifests {
            manifests: Vec::new(),
            _generation: None,
        });
    };
    let snapshots = catalog_snapshot_map(&generation)?;
    let mut manifests = Vec::new();
    for bundle in &generation.bundles {
        match agent_manifest::load_with_catalog_snapshots(bundle.snapshot.path(), snapshots.clone())
        {
            Ok(mut resolved) => {
                resolved.bundle_dir = catalog_display_path(omegon_home, &bundle.raw_name);
                manifests.push(resolved);
            }
            Err(error) => tracing::warn!(
                path = %catalog_display_path(omegon_home, &bundle.raw_name).display(),
                %error,
                "skipping invalid admitted catalog bundle"
            ),
        }
    }
    manifests.sort_by(|left, right| left.manifest.agent.id.cmp(&right.manifest.agent.id));
    Ok(AdmittedCatalogManifests {
        manifests,
        _generation: Some(generation),
    })
}

/// Resolve an agent by ID from the catalog. Searches `$OMEGON_HOME/catalog/`
/// and also accepts a direct path to a bundle directory.
pub fn resolve(omegon_home: &Path, agent_id: &str) -> anyhow::Result<AdmittedResolvedManifest> {
    // First, check if agent_id is a direct path
    let as_path = Path::new(agent_id);
    if as_path.is_dir() {
        let canonical_catalog = omegon_home.join("catalog").canonicalize().ok();
        let canonical_path = as_path.canonicalize()?;
        if canonical_catalog
            .as_ref()
            .is_some_and(|catalog| canonical_path.starts_with(catalog))
        {
            let raw_name = canonical_path
                .strip_prefix(canonical_catalog.as_ref().expect("checked"))?
                .components()
                .next()
                .ok_or_else(|| anyhow::anyhow!("catalog path does not name a bundle"))?
                .as_os_str()
                .to_string_lossy()
                .into_owned();
            return resolve_catalog_entry(omegon_home, &raw_name, true);
        }
        let source = std::fs::File::open(&canonical_path)?;
        let snapshot = crate::contribution_loading::snapshot_contribution_directory(&source)?;
        let resolved =
            agent_manifest::load_with_catalog_snapshots(snapshot.path(), HashMap::new())?;
        return Ok(AdmittedResolvedManifest {
            resolved,
            display_bundle_dir: canonical_path,
            _generation: None,
            _direct_snapshot: Some(snapshot),
        });
    }
    resolve_catalog_entry(omegon_home, agent_id, false)
}

fn resolve_catalog_entry(
    omegon_home: &Path,
    selector: &str,
    directory_only: bool,
) -> anyhow::Result<AdmittedResolvedManifest> {
    let generation = admit_catalog(omegon_home)?.ok_or_else(|| {
        anyhow::anyhow!(
            "catalog directory not found: {}",
            omegon_home.join("catalog").display()
        )
    })?;
    let snapshots = catalog_snapshot_map(&generation)?;
    for bundle in &generation.bundles {
        let raw_name = std::str::from_utf8(&bundle.raw_name).ok();
        if raw_name == Some(selector) {
            let resolved = agent_manifest::load_with_catalog_snapshots(
                bundle.snapshot.path(),
                snapshots.clone(),
            )?;
            return Ok(AdmittedResolvedManifest {
                resolved,
                display_bundle_dir: catalog_display_path(omegon_home, &bundle.raw_name),
                _generation: Some(generation),
                _direct_snapshot: None,
            });
        }
    }
    if !directory_only {
        for bundle in &generation.bundles {
            if let Ok(resolved) = agent_manifest::load_with_catalog_snapshots(
                bundle.snapshot.path(),
                snapshots.clone(),
            ) && resolved.manifest.agent.id == selector
            {
                return Ok(AdmittedResolvedManifest {
                    resolved,
                    display_bundle_dir: catalog_display_path(omegon_home, &bundle.raw_name),
                    _generation: Some(generation),
                    _direct_snapshot: None,
                });
            }
        }
    }
    anyhow::bail!(
        "agent '{selector}' not found in catalog at {}",
        omegon_home.join("catalog").display()
    )
}

#[cfg(unix)]
fn admit_catalog(omegon_home: &Path) -> anyhow::Result<Option<AdmittedCatalogGeneration>> {
    let Some(admission) = crate::contribution_loading::GuardedContributionDirectory::open(
        omegon_home,
        &[b"catalog"],
        omegon_home,
        omegon_maintenance_contracts::ContributionKind::Catalog,
        "user",
    )?
    else {
        return Ok(None);
    };
    let mut names = admission.entry_names(MAX_CATALOG_ENTRIES)?;
    names.sort();
    let mut bundles = Vec::new();
    for raw_name in names {
        if crate::contribution_loading::is_internal_contribution_entry(&raw_name)
            || !admission.allows(&raw_name)?
        {
            continue;
        }
        let Some(directory) = admission.open_child_directory(&raw_name)? else {
            continue;
        };
        bundles.push(AdmittedCatalogBundle {
            raw_name,
            snapshot: crate::contribution_loading::snapshot_contribution_directory(&directory)?,
        });
    }
    Ok(Some(AdmittedCatalogGeneration {
        _admission: admission,
        bundles,
    }))
}

#[cfg(not(unix))]
fn admit_catalog(_omegon_home: &Path) -> anyhow::Result<Option<AdmittedCatalogGeneration>> {
    anyhow::bail!("guarded catalog discovery requires Unix")
}

fn catalog_snapshot_map(
    generation: &AdmittedCatalogGeneration,
) -> anyhow::Result<HashMap<String, PathBuf>> {
    generation
        .bundles
        .iter()
        .map(|bundle| {
            Ok((
                std::str::from_utf8(&bundle.raw_name)?.to_string(),
                bundle.snapshot.path().to_path_buf(),
            ))
        })
        .collect()
}

#[cfg(unix)]
fn catalog_display_path(omegon_home: &Path, raw_name: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    omegon_home
        .join("catalog")
        .join(std::ffi::OsStr::from_bytes(raw_name))
}

#[cfg(not(unix))]
fn catalog_display_path(omegon_home: &Path, raw_name: &[u8]) -> PathBuf {
    omegon_home
        .join("catalog")
        .join(String::from_utf8_lossy(raw_name).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_empty_catalog() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("catalog")).unwrap();
        let entries = list(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn list_discovers_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("catalog/test-agent");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("agent.toml"),
            r#"
[agent]
id = "test.agent"
name = "Test"
version = "1.0.0"
domain = "chat"
"#,
        )
        .unwrap();

        let entries = list(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "test.agent");
        assert_eq!(entries[0].domain, "chat");
    }

    #[test]
    fn resolve_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("catalog/my-agent");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("agent.toml"),
            r#"
[agent]
id = "org.my-agent"
name = "My Agent"
version = "1.0.0"
domain = "coding"
"#,
        )
        .unwrap();

        let resolved = resolve(dir.path(), "org.my-agent").unwrap();
        assert_eq!(resolved.manifest.agent.id, "org.my-agent");
    }

    #[test]
    fn resolve_by_dir_name() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("catalog/my-agent");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("agent.toml"),
            r#"
[agent]
id = "org.my-agent"
name = "My Agent"
version = "1.0.0"
domain = "coding"
"#,
        )
        .unwrap();

        let resolved = resolve(dir.path(), "my-agent").unwrap();
        assert_eq!(resolved.manifest.agent.id, "org.my-agent");
    }

    #[test]
    fn resolve_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("catalog")).unwrap();
        assert!(resolve(dir.path(), "nonexistent").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_catalog_excludes_denied_bundle() {
        let home = tempfile::tempdir().unwrap();
        write_test_bundle(home.path(), "denied", "test.denied");
        write_test_bundle(home.path(), "allowed", "test.allowed");
        deny_catalog_entry(home.path(), b"denied");

        let listing = list(home.path()).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].id, "test.allowed");
        assert!(resolve(home.path(), "denied").is_err());
        assert!(resolve(home.path(), "test.denied").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_catalog_fails_closed_on_malformed_deny_state() {
        let home = tempfile::tempdir().unwrap();
        write_test_bundle(home.path(), "agent", "test.agent");
        let authority = initialize_catalog_scope(home.path());
        std::fs::write(
            home.path()
                .join("maintain/v1/deny")
                .join(authority.to_hex())
                .join("state.json"),
            "{not-json",
        )
        .unwrap();
        assert!(list(home.path()).is_err());
        assert!(resolve(home.path(), "agent").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_catalog_holds_lock_through_listing_publication() {
        use omegon_maintenance_contracts::{LockMode, MaintenanceStateV1, ProtocolLock};

        let home_path = tempfile::tempdir().unwrap();
        write_test_bundle(home_path.path(), "allowed", "test.allowed");
        let listing = list(home_path.path()).unwrap();
        let authority = listing._generation.as_ref().unwrap()._admission.scope_key();
        let home = omegon_maintenance_contracts::open_secure_root(home_path.path()).unwrap();
        let state = MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let lock_name = format!("contribution-{authority}.lock");
        assert!(
            ProtocolLock::acquire_at(
                &state.locks,
                lock_name.as_bytes(),
                LockMode::Exclusive,
                false,
                true,
            )
            .is_err()
        );
        drop(listing);
        assert!(
            ProtocolLock::acquire_at(
                &state.locks,
                lock_name.as_bytes(),
                LockMode::Exclusive,
                false,
                true,
            )
            .is_ok()
        );
    }

    #[cfg(unix)]
    #[test]
    fn guarded_catalog_install_replaces_complete_bundle_and_remove_is_confined() {
        let home = tempfile::tempdir().unwrap();
        write_test_bundle(home.path(), "test-agent", "test.agent");
        std::fs::write(home.path().join("catalog/test-agent/stale.txt"), "stale").unwrap();
        let prepared = PreparedCatalogDirectory::new().unwrap();
        let candidate = prepared.0.join("test-agent");
        std::fs::create_dir_all(&candidate).unwrap();
        std::fs::write(
            candidate.join("agent.toml"),
            test_manifest("test.agent", "2.0.0"),
        )
        .unwrap();

        publish_catalog_candidates(home.path(), &[("test-agent".to_string(), candidate)]).unwrap();
        assert!(!home.path().join("catalog/test-agent/stale.txt").exists());
        assert!(
            std::fs::read_to_string(home.path().join("catalog/test-agent/agent.toml"))
                .unwrap()
                .contains("2.0.0")
        );
        remove(home.path(), "test.agent").unwrap();
        assert!(!home.path().join("catalog/test-agent").exists());
    }

    #[cfg(unix)]
    fn write_test_bundle(home: &Path, directory: &str, id: &str) {
        let bundle = home.join("catalog").join(directory);
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("agent.toml"), test_manifest(id, "1.0.0")).unwrap();
    }

    #[cfg(unix)]
    fn test_manifest(id: &str, version: &str) -> String {
        format!(
            "[agent]\nid = \"{id}\"\nname = \"Test\"\nversion = \"{version}\"\ndomain = \"test\"\n"
        )
    }

    #[cfg(unix)]
    fn initialize_catalog_scope(home_path: &Path) -> omegon_maintenance_contracts::AuthorityKey {
        let home = omegon_maintenance_contracts::open_secure_root(home_path).unwrap();
        let state = omegon_maintenance_contracts::MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let catalog = std::fs::File::open(home_path.join("catalog")).unwrap();
        state
            .admit_contribution_scope(
                omegon_maintenance_contracts::ContributionKind::Catalog,
                "user",
                &omegon_maintenance_contracts::path_identity(&catalog).unwrap(),
                "initialize-test",
                false,
            )
            .unwrap()
            .scope_key
    }

    #[cfg(unix)]
    fn deny_catalog_entry(home_path: &Path, raw_name: &[u8]) {
        use omegon_maintenance_contracts::{
            AuthorityKey, ContributionKind, DenyRecordV1, DenyState, DenyStateV1, SCHEMA_VERSION,
            derive_key, entry_key, open_secure_dir_at, replace_record_at,
        };
        use sha2::{Digest, Sha256};

        let authority = initialize_catalog_scope(home_path);
        let home = omegon_maintenance_contracts::open_secure_root(home_path).unwrap();
        let state = omegon_maintenance_contracts::MaintenanceStateV1::bootstrap(
            &home,
            omegon_maintenance_contracts::path_identity(&home).unwrap(),
            "11111111-1111-1111-1111-111111111111",
            false,
        )
        .unwrap();
        let deny_directory = open_secure_dir_at(&state.deny, authority.to_hex().as_bytes())
            .unwrap()
            .unwrap();
        let kind = ContributionKind::Catalog;
        let entry = entry_key(kind.as_str(), authority, raw_name);
        let request_id = "00000000-0000-0000-0000-000000000001";
        let record = DenyRecordV1 {
            schema_version: SCHEMA_VERSION,
            record_kind: "deny".into(),
            record_id: derive_key(
                "deny",
                &[
                    authority.as_bytes(),
                    entry.as_bytes(),
                    request_id.as_bytes(),
                ],
            ),
            scope_key: authority,
            contribution_kind: kind,
            entry_key: entry,
            raw_name_digest: AuthorityKey::from_bytes(Sha256::digest(raw_name).into()),
            generation: 1,
            state: DenyState::Denied,
            request_id: request_id.into(),
            created_at: "2026-08-19T00:00:00Z".into(),
        };
        let deny = DenyStateV1 {
            schema_version: SCHEMA_VERSION,
            record_kind: "deny_state".into(),
            record_id: derive_key("deny-state", &[authority.as_bytes(), &1_u64.to_be_bytes()]),
            scope_key: authority,
            generation: 1,
            entries: [(entry.to_hex(), record)].into(),
        };
        replace_record_at(&deny_directory, b"state.json", &deny, "deny-test").unwrap();
    }
}
