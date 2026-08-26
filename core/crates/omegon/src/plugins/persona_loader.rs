//! Persona and tone loader — scan installed plugins and build
//! `LoadedPersona` / `LoadedTone` instances for the AugmentRegistry.

use std::{fs::File, path::Path};

use super::armory::{ArmoryManifest, PluginType};
use super::registry::{LoadedPersona, LoadedTone, MindFact, ToneIntensity};

/// A discovered persona or tone available for activation.
#[derive(Debug, Clone)]
pub struct AvailablePlugin {
    pub id: String,
    pub name: String,
    pub plugin_type: PluginType,
    pub description: String,
    pub path: std::path::PathBuf,
    loaded: AvailablePluginContent,
}

#[derive(Debug, Clone)]
enum AvailablePluginContent {
    Persona(LoadedPersona),
    Tone(LoadedTone),
}

impl AvailablePlugin {
    pub fn persona(&self) -> Option<&LoadedPersona> {
        match &self.loaded {
            AvailablePluginContent::Persona(persona) => Some(persona),
            AvailablePluginContent::Tone(_) => None,
        }
    }

    pub fn tone(&self) -> Option<&LoadedTone> {
        match &self.loaded {
            AvailablePluginContent::Persona(_) => None,
            AvailablePluginContent::Tone(tone) => Some(tone),
        }
    }
}

const MAX_PLUGIN_ENTRIES: usize = 10_000;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_DIRECTIVE_BYTES: usize = 4 * 1024 * 1024;
const MAX_MIND_BYTES: usize = 4 * 1024 * 1024;
const MAX_EXEMPLARS: usize = 256;
const MAX_EXEMPLAR_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_EXEMPLAR_BYTES: usize = 16 * 1024 * 1024;

/// Build an immutable persona/tone catalog from admitted canonical plugin scopes.
/// Admission locks remain held until `publish` returns.
pub fn with_available<R>(
    cwd: &Path,
    publish: impl FnOnce(&[AvailablePlugin], &[AvailablePlugin]) -> R,
) -> R {
    let mut personas = Vec::new();
    let mut tones = Vec::new();
    let scopes = match crate::paths::omegon_home() {
        Ok(home) => super::open_guarded_plugin_scopes(cwd, &home),
        Err(error) => {
            tracing::warn!(error = %error, "persona/tone discovery failed closed");
            Vec::new()
        }
    };

    for scope in &scopes {
        let result = load_scope(scope);
        match result {
            Ok((mut scope_personas, mut scope_tones)) => {
                personas.append(&mut scope_personas);
                tones.append(&mut scope_tones);
            }
            Err(error) => {
                tracing::warn!(scope = scope.scope, error = %error, "persona/tone scope failed closed");
            }
        }
    }

    if let Some(pack) = crate::content_pack::boot_pack() {
        match load_pack_plugins(&pack) {
            Ok((pack_personas, pack_tones)) => {
                let existing = personas
                    .iter()
                    .chain(tones.iter())
                    .map(|plugin| plugin.id.clone())
                    .collect::<std::collections::HashSet<_>>();
                personas.extend(
                    pack_personas
                        .into_iter()
                        .filter(|plugin| !existing.contains(&plugin.id)),
                );
                tones.extend(
                    pack_tones
                        .into_iter()
                        .filter(|plugin| !existing.contains(&plugin.id)),
                );
            }
            Err(error) => tracing::warn!(error = %error, "persona/tone pack content unavailable"),
        }
    }

    remove_duplicate_ids(&mut personas, &mut tones);

    publish(&personas, &tones)
}

fn load_pack_plugins(
    pack: &crate::content_pack::ContentPack,
) -> anyhow::Result<(Vec<AvailablePlugin>, Vec<AvailablePlugin>)> {
    let mut personas = Vec::new();
    let mut tones = Vec::new();
    for (kind, subtree, output) in [
        ("persona", "personas", &mut personas),
        ("tone", "tones", &mut tones),
    ] {
        let snapshot = pack.materialize_kind(kind)?;
        let root = snapshot.path().join(subtree);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let directory = File::open(entry.path())?;
            let Some(manifest) = crate::contribution_loading::read_file_at(
                &directory,
                b"plugin.toml",
                MAX_MANIFEST_BYTES,
            )?
            else {
                continue;
            };
            let manifest = ArmoryManifest::parse(&String::from_utf8(manifest)?)?;
            if let Some(plugin) = load_available_plugin(
                &directory,
                manifest,
                pack.root.join(subtree).join(entry.file_name()),
            )? {
                output.push(plugin);
            }
        }
    }
    Ok((personas, tones))
}

fn remove_duplicate_ids(personas: &mut Vec<AvailablePlugin>, tones: &mut Vec<AvailablePlugin>) {
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for plugin in personas.iter().chain(tones.iter()) {
        *counts.entry(plugin.id.clone()).or_default() += 1;
    }
    let duplicates = counts
        .into_iter()
        .filter_map(|(id, count)| (count > 1).then_some(id))
        .collect::<std::collections::HashSet<_>>();
    for id in &duplicates {
        tracing::warn!(
            plugin_id = id,
            "excluding ambiguous duplicate persona/tone ID"
        );
    }
    personas.retain(|plugin| !duplicates.contains(&plugin.id));
    tones.retain(|plugin| !duplicates.contains(&plugin.id));
}

fn load_scope(
    scope: &super::GuardedPluginScope,
) -> anyhow::Result<(Vec<AvailablePlugin>, Vec<AvailablePlugin>)> {
    let mut personas = Vec::new();
    let mut tones = Vec::new();
    let mut entries = scope.admission.entry_names(MAX_PLUGIN_ENTRIES)?;
    entries.sort();

    for raw_name in entries {
        if crate::contribution_loading::is_internal_contribution_entry(&raw_name) {
            continue;
        }
        if !scope.admission.allows(&raw_name)? {
            continue;
        }
        let Ok(name) = std::str::from_utf8(&raw_name) else {
            continue;
        };
        let Some(directory) = scope.admission.open_child_directory(&raw_name)? else {
            continue;
        };
        let Some(manifest_bytes) = crate::contribution_loading::read_file_at(
            &directory,
            b"plugin.toml",
            MAX_MANIFEST_BYTES,
        )?
        else {
            continue;
        };
        let available = match String::from_utf8(manifest_bytes)
            .map_err(anyhow::Error::from)
            .and_then(|content| ArmoryManifest::parse(&content).map_err(anyhow::Error::from))
            .and_then(|manifest| {
                load_available_plugin(&directory, manifest, scope.display_root.join(name))
            }) {
            Ok(Some(available)) => available,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(path = %scope.display_root.join(name).display(), error = %error, "skipping invalid persona/tone plugin");
                continue;
            }
        };
        match available.plugin_type {
            PluginType::Persona => personas.push(available),
            PluginType::Tone => tones.push(available),
            _ => unreachable!("catalog loader only returns persona/tone plugins"),
        }
    }

    Ok((personas, tones))
}

fn load_available_plugin(
    directory: &File,
    manifest: ArmoryManifest,
    path: std::path::PathBuf,
) -> anyhow::Result<Option<AvailablePlugin>> {
    let id = manifest.plugin.id.clone();
    let name = manifest.plugin.name.clone();
    let plugin_type = manifest.plugin.plugin_type;
    let description = manifest.plugin.description.clone();
    let loaded = match plugin_type {
        PluginType::Persona => {
            AvailablePluginContent::Persona(load_persona_from_manifest(directory, manifest)?)
        }
        PluginType::Tone => {
            AvailablePluginContent::Tone(load_tone_from_manifest(directory, manifest)?)
        }
        _ => return Ok(None),
    };
    Ok(Some(AvailablePlugin {
        id,
        name,
        plugin_type,
        description,
        path,
        loaded,
    }))
}

fn load_persona_from_manifest(
    plugin_dir: &File,
    manifest: ArmoryManifest,
) -> anyhow::Result<LoadedPersona> {
    if manifest.plugin.plugin_type != PluginType::Persona {
        anyhow::bail!(
            "plugin '{}' is not a persona (type: {})",
            manifest.plugin.name,
            manifest.plugin.plugin_type
        );
    }

    let persona_config = manifest.persona.ok_or_else(|| {
        anyhow::anyhow!(
            "persona plugin '{}' has no [persona] section",
            manifest.plugin.name
        )
    })?;

    let directive = if let Some(ref identity) = persona_config.identity {
        read_required_text(plugin_dir, &identity.directive, MAX_DIRECTIVE_BYTES)?
    } else {
        String::new()
    };

    let mind_facts = if let Some(ref mind) = persona_config.mind {
        if let Some(ref seed_path) = mind.seed_facts {
            match read_relative_file(plugin_dir, seed_path, MAX_MIND_BYTES)? {
                Some(bytes) => parse_mind_facts(&String::from_utf8(bytes)?),
                None => vec![],
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let activated_skills = persona_config
        .skills
        .as_ref()
        .map(|s| s.activate.clone())
        .unwrap_or_default();

    let disabled_tools = persona_config
        .tools
        .as_ref()
        .map(|t| t.disable.clone())
        .unwrap_or_default();

    let badge = persona_config.style.as_ref().and_then(|s| s.badge.clone());

    Ok(LoadedPersona {
        id: manifest.plugin.id,
        name: manifest.plugin.name,
        directive,
        mind_facts,
        activated_skills,
        disabled_tools,
        badge,
    })
}

fn load_tone_from_manifest(
    plugin_dir: &File,
    manifest: ArmoryManifest,
) -> anyhow::Result<LoadedTone> {
    if manifest.plugin.plugin_type != PluginType::Tone {
        anyhow::bail!(
            "plugin '{}' is not a tone (type: {})",
            manifest.plugin.name,
            manifest.plugin.plugin_type
        );
    }

    let tone_config = manifest.tone.ok_or_else(|| {
        anyhow::anyhow!(
            "tone plugin '{}' has no [tone] section",
            manifest.plugin.name
        )
    })?;

    let directive = read_required_text(plugin_dir, &tone_config.directive, MAX_DIRECTIVE_BYTES)?;

    let exemplars = if let Some(ref exemplar_dir) = tone_config.exemplars {
        load_exemplars(plugin_dir, exemplar_dir)?
    } else {
        vec![]
    };

    let intensity = tone_config
        .intensity
        .map(|i| ToneIntensity {
            design: i.design,
            coding: i.coding,
        })
        .unwrap_or_default();

    Ok(LoadedTone {
        id: manifest.plugin.id,
        name: manifest.plugin.name,
        directive,
        exemplars,
        intensity,
    })
}

fn parse_mind_facts(content: &str) -> Vec<MindFact> {
    let mut facts = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match serde_json::from_str::<MindFact>(line) {
            Ok(fact) => facts.push(fact),
            Err(e) => tracing::warn!(line = line, error = %e, "skipping invalid mind fact"),
        }
    }
    facts
}

fn load_exemplars(plugin_dir: &File, path: &str) -> anyhow::Result<Vec<String>> {
    let Some(dir) = open_relative_directory(plugin_dir, path)? else {
        return Ok(Vec::new());
    };
    let mut exemplars = Vec::new();
    let mut total_bytes = 0_usize;
    let mut entries = crate::contribution_loading::read_directory_names(&dir, MAX_EXEMPLARS)?;
    entries.retain(|name| name.ends_with(b".md"));
    entries.sort();
    for raw_name in entries {
        let Some(bytes) =
            crate::contribution_loading::read_file_at(&dir, &raw_name, MAX_EXEMPLAR_BYTES)?
        else {
            continue;
        };
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("tone exemplar size overflow"))?;
        if total_bytes > MAX_TOTAL_EXEMPLAR_BYTES {
            anyhow::bail!("tone exemplars exceed the total size limit");
        }
        exemplars.push(String::from_utf8(bytes)?);
    }
    Ok(exemplars)
}

fn read_required_text(parent: &File, path: &str, limit: usize) -> anyhow::Result<String> {
    let bytes = read_relative_file(parent, path, limit)?
        .ok_or_else(|| anyhow::anyhow!("cannot read {path}: file not found"))?;
    String::from_utf8(bytes).map_err(Into::into)
}

fn read_relative_file(parent: &File, path: &str, limit: usize) -> anyhow::Result<Option<Vec<u8>>> {
    let components = relative_components(path)?;
    let (name, parents) = components
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("contribution path is empty"))?;
    let mut directory = parent.try_clone()?;
    for component in parents {
        let Some(next) = crate::contribution_loading::open_child_directory(&directory, component)?
        else {
            return Ok(None);
        };
        directory = next;
    }
    crate::contribution_loading::read_file_at(&directory, name, limit)
}

fn open_relative_directory(parent: &File, path: &str) -> anyhow::Result<Option<File>> {
    let mut directory = parent.try_clone()?;
    for component in relative_components(path)? {
        let Some(next) = crate::contribution_loading::open_child_directory(&directory, &component)?
        else {
            return Ok(None);
        };
        directory = next;
    }
    Ok(Some(directory))
}

fn relative_components(path: &str) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut components = Vec::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::Normal(name) => {
                let bytes = name.as_encoded_bytes();
                omegon_maintenance_contracts::validate_child_name(bytes)?;
                components.push(bytes.to_vec());
            }
            _ => anyhow::bail!("contribution path must be relative and confined"),
        }
    }
    if components.is_empty() {
        anyhow::bail!("contribution path is empty");
    }
    Ok(components)
}

#[derive(Default)]
pub(crate) struct PersonaUpdate<'a> {
    pub(crate) directive: Option<&'a str>,
    pub(crate) name: Option<&'a str>,
    pub(crate) description: Option<&'a str>,
    pub(crate) badge: Option<&'a str>,
    pub(crate) disabled_tools: Option<Vec<String>>,
    pub(crate) activated_skills: Option<Vec<String>>,
}

pub(crate) fn create_user_persona(
    cwd: &Path,
    slug: &str,
    name: &str,
    description: &str,
    badge: Option<&str>,
    disabled_tools: &[String],
    directive: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let home = crate::paths::omegon_home()?;
    let slug = validated_slug(slug)?;
    let id = format!("user.{slug}");
    let manifest = persona_manifest(&id, name, description, badge, disabled_tools)?;
    let mut scopes = open_persona_mutation_scopes(cwd, &home, true)?;
    if !find_persona_matches(&scopes, &id)?.is_empty() {
        anyhow::bail!("persona ID '{id}' already exists");
    }
    let directory = &mut scopes
        .iter_mut()
        .find(|scope| scope.scope == "user")
        .ok_or_else(|| anyhow::anyhow!("user plugin scope is unavailable"))?
        .directory;
    directory.write_files_directory(
        slug.as_bytes(),
        &[
            (b"plugin.toml".as_slice(), manifest.as_bytes(), 0o600),
            (b"PERSONA.md".as_slice(), directive.as_bytes(), 0o600),
        ],
        false,
    )?;
    Ok(home.join("plugins").join(slug))
}

pub(crate) fn delete_persona(cwd: &Path, id: &str) -> anyhow::Result<std::path::PathBuf> {
    let home = crate::paths::omegon_home()?;
    let target = resolve_persona_for_mutation(cwd, &home, id)?;
    let existing = target
        .directory
        .open_directory(&target.raw_name)?
        .ok_or_else(|| anyhow::anyhow!("persona '{id}' disappeared"))?;
    verify_persona_identity(&existing, id)?;
    if !target.directory.remove_directory(&target.raw_name)? {
        anyhow::bail!("persona '{id}' disappeared");
    }
    Ok(target.path)
}

pub(crate) fn update_persona(
    cwd: &Path,
    id: &str,
    update: PersonaUpdate<'_>,
) -> anyhow::Result<std::path::PathBuf> {
    let home = crate::paths::omegon_home()?;
    let target = resolve_persona_for_mutation(cwd, &home, id)?;
    let existing = target
        .directory
        .open_directory(&target.raw_name)?
        .ok_or_else(|| anyhow::anyhow!("persona '{id}' disappeared"))?;
    let manifest_bytes =
        crate::contribution_loading::read_file_at(&existing, b"plugin.toml", MAX_MANIFEST_BYTES)?
            .ok_or_else(|| anyhow::anyhow!("persona manifest disappeared"))?;
    let manifest_text = String::from_utf8(manifest_bytes)?;
    let parsed = ArmoryManifest::parse(&manifest_text)?;
    if parsed.plugin.plugin_type != PluginType::Persona || parsed.plugin.id != id {
        anyhow::bail!("persona identity changed during mutation");
    }
    let directive_path = parsed
        .persona
        .as_ref()
        .and_then(|persona| persona.identity.as_ref())
        .map(|identity| identity.directive.clone());
    let snapshot = crate::contribution_loading::snapshot_contribution_directory(&existing)?;
    let mut manifest: toml::Table = toml::from_str(&manifest_text)?;
    if let Some(name) = update.name
        && let Some(plugin) = manifest
            .get_mut("plugin")
            .and_then(toml::Value::as_table_mut)
    {
        plugin.insert("name".into(), name.into());
    }
    if let Some(description) = update.description
        && let Some(plugin) = manifest
            .get_mut("plugin")
            .and_then(toml::Value::as_table_mut)
    {
        plugin.insert("description".into(), description.into());
    }
    if let Some(badge) = update.badge {
        let style = persona_subtable(&mut manifest, "style");
        style.insert("badge".into(), badge.into());
    }
    if let Some(tools) = update.disabled_tools {
        let tools_table = persona_subtable(&mut manifest, "tools");
        tools_table.insert(
            "disable".into(),
            toml::Value::Array(tools.into_iter().map(toml::Value::String).collect()),
        );
    }
    if let Some(skills) = update.activated_skills {
        let skills_table = persona_subtable(&mut manifest, "skills");
        skills_table.insert(
            "activate".into(),
            toml::Value::Array(skills.into_iter().map(toml::Value::String).collect()),
        );
    }
    if let Some(directive) = update.directive {
        let directive_path =
            directive_path.ok_or_else(|| anyhow::anyhow!("persona has no directive path"))?;
        let path = snapshot_path(snapshot.path(), &directive_path)?;
        std::fs::write(path, directive)?;
    }
    std::fs::write(
        snapshot.path().join("plugin.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    let source = File::open(snapshot.path())?;
    target
        .directory
        .replace_from_snapshot(&target.raw_name, &source)?;
    Ok(target.path)
}

struct PersonaMutationTarget {
    directory: crate::contribution_loading::GuardedContributionMutationDirectory,
    raw_name: Vec<u8>,
    path: std::path::PathBuf,
}

struct PersonaMutationScope {
    scope: &'static str,
    display_root: std::path::PathBuf,
    directory: crate::contribution_loading::GuardedContributionMutationDirectory,
}

fn resolve_persona_for_mutation(
    cwd: &Path,
    home: &Path,
    id: &str,
) -> anyhow::Result<PersonaMutationTarget> {
    let mut scopes = open_persona_mutation_scopes(cwd, home, false)?;
    let matches = find_persona_matches(&scopes, id)?;
    let [(scope_index, raw_name)] = matches.as_slice() else {
        return if matches.is_empty() {
            Err(anyhow::anyhow!("persona '{id}' not found"))
        } else {
            Err(anyhow::anyhow!("persona ID '{id}' is ambiguous"))
        };
    };
    let scope = scopes.swap_remove(*scope_index);
    let path = display_child_path(&scope.display_root, raw_name);
    Ok(PersonaMutationTarget {
        directory: scope.directory,
        raw_name: raw_name.clone(),
        path,
    })
}

fn open_persona_mutation_scopes(
    cwd: &Path,
    home: &Path,
    create_user: bool,
) -> anyhow::Result<Vec<PersonaMutationScope>> {
    let mut scopes = Vec::new();
    let user = if create_user {
        Some(
            crate::contribution_loading::GuardedContributionMutationDirectory::open_or_create(
                home,
                &[b"plugins"],
                home,
                omegon_maintenance_contracts::ContributionKind::Plugin,
                "user",
            )?,
        )
    } else {
        crate::contribution_loading::GuardedContributionMutationDirectory::open_existing(
            home,
            &[b"plugins"],
            home,
            omegon_maintenance_contracts::ContributionKind::Plugin,
            "user",
        )?
    };
    if let Some(directory) = user {
        scopes.push(PersonaMutationScope {
            scope: "user",
            display_root: home.join("plugins"),
            directory,
        });
    }
    let project_root = crate::setup::find_project_root(cwd);
    if let Some(directory) =
        crate::contribution_loading::GuardedContributionMutationDirectory::open_existing(
            &project_root,
            &[b".omegon", b"plugins"],
            home,
            omegon_maintenance_contracts::ContributionKind::Plugin,
            "project",
        )?
    {
        scopes.push(PersonaMutationScope {
            scope: "project",
            display_root: project_root.join(".omegon/plugins"),
            directory,
        });
    }
    Ok(scopes)
}

fn find_persona_matches(
    scopes: &[PersonaMutationScope],
    id: &str,
) -> anyhow::Result<Vec<(usize, Vec<u8>)>> {
    let mut matches = Vec::new();
    for (scope_index, scope) in scopes.iter().enumerate() {
        let mut entries = scope.directory.entry_names(MAX_PLUGIN_ENTRIES)?;
        entries.sort();
        for raw_name in entries {
            if crate::contribution_loading::is_internal_contribution_entry(&raw_name) {
                continue;
            }
            let Some(directory) = scope.directory.open_directory(&raw_name)? else {
                continue;
            };
            let Some(bytes) = crate::contribution_loading::read_file_at(
                &directory,
                b"plugin.toml",
                MAX_MANIFEST_BYTES,
            )?
            else {
                continue;
            };
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };
            let Ok(manifest) = ArmoryManifest::parse(&content) else {
                continue;
            };
            if matches!(
                manifest.plugin.plugin_type,
                PluginType::Persona | PluginType::Tone
            ) && manifest.plugin.id == id
            {
                matches.push((scope_index, raw_name));
            }
        }
    }
    Ok(matches)
}

#[cfg(unix)]
fn display_child_path(root: &Path, raw_name: &[u8]) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    root.join(std::ffi::OsStr::from_bytes(raw_name))
}

#[cfg(not(unix))]
fn display_child_path(root: &Path, raw_name: &[u8]) -> std::path::PathBuf {
    root.join(String::from_utf8_lossy(raw_name).as_ref())
}

fn verify_persona_identity(directory: &File, id: &str) -> anyhow::Result<()> {
    let bytes =
        crate::contribution_loading::read_file_at(directory, b"plugin.toml", MAX_MANIFEST_BYTES)?
            .ok_or_else(|| anyhow::anyhow!("persona manifest disappeared"))?;
    let manifest = ArmoryManifest::parse(&String::from_utf8(bytes)?)?;
    if manifest.plugin.plugin_type != PluginType::Persona || manifest.plugin.id != id {
        anyhow::bail!("persona identity changed during mutation");
    }
    Ok(())
}

fn validated_slug(slug: &str) -> anyhow::Result<&str> {
    if slug.is_empty()
        || slug.len() > 128
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        anyhow::bail!("invalid persona slug");
    }
    omegon_maintenance_contracts::validate_child_name(slug.as_bytes())?;
    Ok(slug)
}

fn persona_manifest(
    id: &str,
    name: &str,
    description: &str,
    badge: Option<&str>,
    disabled_tools: &[String],
) -> anyhow::Result<String> {
    let mut root = toml::Table::new();
    let mut plugin = toml::Table::new();
    plugin.insert("type".into(), "persona".into());
    plugin.insert("id".into(), id.into());
    plugin.insert("name".into(), name.into());
    plugin.insert("version".into(), "1.0.0".into());
    plugin.insert("description".into(), description.into());
    root.insert("plugin".into(), toml::Value::Table(plugin));
    let mut persona = toml::Table::new();
    let mut identity = toml::Table::new();
    identity.insert("directive".into(), "PERSONA.md".into());
    persona.insert("identity".into(), toml::Value::Table(identity));
    if !disabled_tools.is_empty() {
        let mut tools = toml::Table::new();
        tools.insert(
            "disable".into(),
            toml::Value::Array(
                disabled_tools
                    .iter()
                    .cloned()
                    .map(toml::Value::String)
                    .collect(),
            ),
        );
        persona.insert("tools".into(), toml::Value::Table(tools));
    }
    if let Some(badge) = badge {
        let mut style = toml::Table::new();
        style.insert("badge".into(), badge.into());
        persona.insert("style".into(), toml::Value::Table(style));
    }
    root.insert("persona".into(), toml::Value::Table(persona));
    toml::to_string_pretty(&root).map_err(Into::into)
}

fn persona_subtable<'a>(manifest: &'a mut toml::Table, name: &str) -> &'a mut toml::Table {
    manifest
        .entry("persona")
        .or_insert(toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .expect("persona table was parsed as a table")
        .entry(name)
        .or_insert(toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .expect("persona child table was parsed as a table")
}

fn snapshot_path(root: &Path, relative: &str) -> anyhow::Result<std::path::PathBuf> {
    let mut path = root.to_path_buf();
    for component in relative_components(relative)? {
        path.push(std::str::from_utf8(&component)?);
    }
    Ok(path)
}

#[cfg(test)]
pub(crate) fn load_persona(plugin_dir: &Path) -> anyhow::Result<LoadedPersona> {
    let directory = File::open(plugin_dir)?;
    let content =
        crate::contribution_loading::read_file_at(&directory, b"plugin.toml", MAX_MANIFEST_BYTES)?
            .ok_or_else(|| anyhow::anyhow!("plugin manifest not found"))?;
    load_persona_from_manifest(
        &directory,
        ArmoryManifest::parse(&String::from_utf8(content)?)?,
    )
}

#[cfg(test)]
fn load_tone(plugin_dir: &Path) -> anyhow::Result<LoadedTone> {
    let directory = File::open(plugin_dir)?;
    let content =
        crate::contribution_loading::read_file_at(&directory, b"plugin.toml", MAX_MANIFEST_BYTES)?
            .ok_or_else(|| anyhow::anyhow!("plugin manifest not found"))?;
    load_tone_from_manifest(
        &directory,
        ArmoryManifest::parse(&String::from_utf8(content)?)?,
    )
}

#[cfg(test)]
fn load_mind_facts(path: &Path) -> anyhow::Result<Vec<MindFact>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(parse_mind_facts(&std::fs::read_to_string(path)?))
}

/// Generate the system prompt for the persona builder conversation.
/// The agent guides the operator through creating a new persona interactively.
pub fn persona_builder_prompt() -> String {
    let home = crate::paths::omegon_home()
        .map(|h| h.join("plugins").display().to_string())
        .unwrap_or_else(|_| "~/.omegon/plugins".to_string());
    format!(
        r#"You are helping the operator create a new Omegon persona. A persona is a behavioral directive that shapes how the agent thinks, communicates, and approaches tasks.

Guide the operator through these questions conversationally. Be concise — one question at a time.

1. **What should this persona do?** Get a clear description of the persona's role, expertise, and communication style.
2. **What should it be called?** Suggest a short name based on their description. Names become kebab-case slugs.
3. **Badge emoji?** What single emoji should represent this persona in the TUI? (e.g., a security persona might use a shield)
4. **Any tools to disable?** Some personas should NOT have access to certain tools (e.g., a read-only analyst shouldn't use `write` or `bash`). Ask if any tools should be disabled.
5. **Skills to activate?** Should this persona automatically activate any installed skills? (e.g., a Rust persona might activate the "rust" skill)

After gathering answers, create the persona by:

1. Create the directory:
   mkdir -p {home}/<slug>/

2. Write `plugin.toml` with this structure:
   ```toml
   [plugin]
   type = "persona"
   id = "user.<slug>"
   name = "<display name>"
   version = "1.0.0"
   description = "<one-line description>"

   [persona.identity]
   directive = "PERSONA.md"

   [persona.skills]
   activate = ["<skill1>", "<skill2>"]  # omit if empty

   [persona.tools]
   disable = ["<tool1>"]  # omit if empty

   [persona.style]
   badge = "<emoji>"
   ```

3. Write `PERSONA.md` with the behavioral directive — this is the core of the persona.
   Write it in second person ("You are...", "You always...", "You never...").
   Be specific and actionable, not vague.

After writing both files, confirm the persona ID and tell the operator it will be available immediately via `/persona <name>`.

Do NOT ask all questions at once. Start with question 1 only."#,
        home = home,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_persona_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("PERSONA.md"), "You are a test persona.\n").unwrap();

        let mind_dir = dir.path().join("mind");
        std::fs::create_dir_all(&mind_dir).unwrap();
        std::fs::write(
            mind_dir.join("facts.jsonl"),
            r#"{"section":"Architecture","content":"test fact","confidence":1.0}
{"section":"Decisions","content":"another fact","confidence":0.9,"tags":["test"]}
"#,
        )
        .unwrap();

        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "persona"
id = "dev.test.tester"
name = "Test Persona"
version = "1.0.0"
description = "A test"

[persona.identity]
directive = "PERSONA.md"

[persona.mind]
seed_facts = "mind/facts.jsonl"

[persona.skills]
activate = ["rust", "testing"]

[persona.style]
badge = "🧪"
"#,
        )
        .unwrap();

        let persona = load_persona(dir.path()).unwrap();
        assert_eq!(persona.id, "dev.test.tester");
        assert_eq!(persona.name, "Test Persona");
        assert!(persona.directive.contains("test persona"));
        assert_eq!(persona.mind_facts.len(), 2);
        assert_eq!(persona.mind_facts[1].tags, vec!["test"]);
        assert_eq!(persona.activated_skills, vec!["rust", "testing"]);
        assert_eq!(persona.badge, Some("🧪".into()));
    }

    #[test]
    fn load_tone_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("TONE.md"), "Speak concisely.\n").unwrap();

        let exemplar_dir = dir.path().join("exemplars");
        std::fs::create_dir_all(&exemplar_dir).unwrap();
        std::fs::write(exemplar_dir.join("01-brevity.md"), "Short and sharp.\n").unwrap();
        std::fs::write(exemplar_dir.join("02-clarity.md"), "Clear, not clever.\n").unwrap();

        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "tone"
id = "dev.test.concise"
name = "Concise"
version = "1.0.0"
description = "Brevity tone"

[tone]
directive = "TONE.md"
exemplars = "exemplars"

[tone.intensity]
design = "full"
coding = "muted"
"#,
        )
        .unwrap();

        let tone = load_tone(dir.path()).unwrap();
        assert_eq!(tone.id, "dev.test.concise");
        assert_eq!(tone.name, "Concise");
        assert!(tone.directive.contains("concisely"));
        assert_eq!(tone.exemplars.len(), 2);
        assert_eq!(tone.intensity.design, "full");
        assert_eq!(tone.intensity.coding, "muted");
    }

    #[test]
    fn load_persona_wrong_type() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
[plugin]
type = "tone"
id = "dev.test.not-persona"
name = "Not A Persona"
version = "1.0.0"
description = "wrong type"

[tone]
directive = "TONE.md"
"#,
        )
        .unwrap();

        let result = load_persona(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a persona"));
    }

    #[test]
    fn load_mind_facts_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("facts.jsonl"), "").unwrap();
        let facts = load_mind_facts(&dir.path().join("facts.jsonl")).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn load_mind_facts_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("facts.jsonl"),
            "# This is a comment\n{\"section\":\"Architecture\",\"content\":\"real fact\",\"confidence\":1.0}\n\n"
        ).unwrap();
        let facts = load_mind_facts(&dir.path().join("facts.jsonl")).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "real fact");
    }

    #[test]
    fn load_mind_facts_missing_file() {
        let facts = load_mind_facts(Path::new("/nonexistent/facts.jsonl")).unwrap();
        assert!(facts.is_empty());
    }
}
