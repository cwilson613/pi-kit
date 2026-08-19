//! Extension lifecycle management — install, list, remove, update, enable, disable.
//!
//! Extensions are native binaries or OCI containers installed into
//! `~/.omegon/extensions/<name>/`.  Each extension must have a
//! `manifest.toml` at the root.
//!
//! ## Install
//!
//! ```sh
//! omegon extension install https://github.com/user/my-extension
//! omegon extension install ./local/path/to/extension
//! omegon extension install https://example.com/my-extension-v1.0-aarch64-apple-darwin.tar.gz
//! ```
//!
//! Git URIs are cloned. Local paths are copied into the admitted extension root.
//! Tarball URLs (.tar.gz) are downloaded and extracted — no build step required.
//!
//! ## List
//!
//! ```sh
//! omegon extension list
//! ```
//!
//! ## Remove
//!
//! ```sh
//! omegon extension remove my-extension
//! ```
//!
//! ## Update
//!
//! ```sh
//! omegon extension update [name]
//! ```
//!
//! ## Enable / Disable
//!
//! ```sh
//! omegon extension enable my-extension
//! omegon extension disable my-extension
//! ```

use std::path::{Path, PathBuf};

use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::state::ExtensionState;

/// Scaffold a new extension project.
pub fn init(name: &str) -> anyhow::Result<()> {
    // Validate name
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        anyhow::bail!("Extension name must be lowercase alphanumeric + hyphens (got '{name}')");
    }

    let dir = Path::new(name);
    if dir.exists() {
        anyhow::bail!("Directory '{name}' already exists");
    }

    std::fs::create_dir_all(dir.join("src"))?;

    // manifest.toml
    std::fs::write(
        dir.join("manifest.toml"),
        format!(
            r#"[extension]
name = "{name}"
version = "0.1.0"
description = "TODO: describe your extension"

[runtime]
type = "native"
binary = "target/release/{name}"

[startup]
ping_method = "get_tools"
timeout_ms = 5000

# [secrets]
# required = []
# optional = []

# [widgets.my-widget]
# label = "My Widget"
# kind = "stateful"
# renderer = "table"
"#
        ),
    )?;

    // Cargo.toml
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
omegon-extension = {{ git = "https://github.com/styrene-lab/omegon" }}
serde_json = "1"
tokio = {{ version = "1", features = ["rt", "macros", "io-util"] }}
async-trait = "0.1"
"#
        ),
    )?;

    // src/main.rs
    let struct_name = name
        .split('-')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>();

    std::fs::write(
        dir.join("src").join("main.rs"),
        format!(
            r#"use omegon_extension::{{Extension, serve}};
use serde_json::{{json, Value}};
use async_trait::async_trait;

#[derive(Default)]
struct {struct_name};

#[async_trait]
impl Extension for {struct_name} {{
    fn name(&self) -> &str {{
        "{name}"
    }}

    fn version(&self) -> &str {{
        env!("CARGO_PKG_VERSION")
    }}

    async fn handle_rpc(
        &self,
        method: &str,
        params: Value,
    ) -> omegon_extension::Result<Value> {{
        match method {{
            "get_tools" => Ok(json!([
                {{
                    "name": "hello",
                    "label": "Hello",
                    "description": "A greeting tool - replace this with your own",
                    "parameters": {{
                        "type": "object",
                        "properties": {{
                            "name": {{"type": "string", "description": "Who to greet"}}
                        }},
                        "required": ["name"]
                    }}
                }}
            ])),
            "execute_tool" => {{
                let tool_name = params.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if tool_name != "hello" {{
                    return Err(omegon_extension::Error::method_not_found(tool_name));
                }}
                let args = params.get("args").cloned().unwrap_or_default();
                let who = args.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("World");
                Ok(json!({{
                    "content": [{{
                        "type": "text",
                        "text": format!("Hello, {{}}!", who)
                    }}]
                }}))
            }}
            _ => Err(omegon_extension::Error::method_not_found(method)),
        }}
    }}
}}

#[tokio::main]
async fn main() {{
    serve({struct_name}::default()).await.unwrap();
}}
"#
        ),
    )?;

    println!("Created extension '{name}' in ./{name}/");
    println!();
    println!("  Next steps:");
    println!("    cd {name}");
    println!("    cargo build --release");
    println!("    omegon extension install .");
    println!();
    println!("  Then restart omegon — your extension will be loaded automatically.");

    Ok(())
}

/// Install an extension from a git URI or local path.
pub fn install(uri: &str) -> anyhow::Result<()> {
    let extensions_dir = extensions_dir()?;

    let local_path = Path::new(uri);

    if local_path.exists() && local_path.join("manifest.toml").exists() {
        install_local(local_path)
    } else if uri.ends_with(".tar.gz") || uri.ends_with(".tgz") {
        install_tarball(&extensions_dir, uri)
    } else if uri.contains("://") || uri.contains("git@") || uri.ends_with(".git") {
        install_git(&extensions_dir, uri)
    } else {
        anyhow::bail!(
            "'{uri}' is not a valid extension source.\n\
             Expected: a git URL, a tarball URL (.tar.gz), or a local directory containing manifest.toml"
        );
    }
}

/// Render all installed extensions as terminal-friendly text.
pub fn list_summary() -> anyhow::Result<String> {
    let extensions_dir = extensions_dir()?;

    if !extensions_dir.exists() {
        return Ok(
            "No extensions installed.\n  Install with: omegon extension install <git-url-or-path>"
                .into(),
        );
    }

    let entries: Vec<_> = std::fs::read_dir(&extensions_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() || e.path().is_symlink())
        .collect();

    if entries.is_empty() {
        return Ok("No extensions installed.".into());
    }

    let mut lines = vec![
        format!(
            "{:<20} {:<10} {:<10} {:<12} DESCRIPTION",
            "NAME", "VERSION", "RUNTIME", "STATUS"
        ),
        "─".repeat(80),
    ];

    for entry in &entries {
        let dir = entry.path();
        let resolved = if dir.is_symlink() {
            std::fs::read_link(&dir).unwrap_or(dir.clone())
        } else {
            dir.clone()
        };

        let manifest_path = resolved.join("manifest.toml");
        if !manifest_path.exists() {
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            lines.push(format!(
                "{:<20} {:<10} {:<10} {:<12} (no manifest.toml)",
                name, "?", "?", "?"
            ));
            continue;
        }

        match load_extension_summary(&resolved) {
            Ok(info) => {
                let symlink_marker = if dir.is_symlink() { " →" } else { "" };
                lines.push(format!(
                    "{:<20} {:<10} {:<10} {:<12} {}{}",
                    info.name,
                    info.version,
                    info.runtime,
                    info.status,
                    info.description,
                    symlink_marker
                ));
            }
            Err(e) => {
                let name = dir.file_name().unwrap_or_default().to_string_lossy();
                lines.push(format!(
                    "{:<20} {:<10} {:<10} {:<12} (error: {e})",
                    name, "?", "?", "?"
                ));
            }
        }
    }

    let symlinks = entries.iter().filter(|e| e.path().is_symlink()).count();
    if symlinks > 0 {
        lines.push("\n  → = symlinked (development mode)".into());
    }

    Ok(lines.join("\n"))
}

/// List all installed extensions.
pub fn list() -> anyhow::Result<()> {
    println!("{}", list_summary()?);
    Ok(())
}

/// Remove an installed extension by name.
pub fn remove(name: &str) -> anyhow::Result<()> {
    validate_name(name)?;
    let mutation = extension_mutation_directory(false)?
        .ok_or_else(|| anyhow::anyhow!("Extension '{name}' not found"))?;
    if !mutation.remove_entry(name.as_bytes())? {
        anyhow::bail!("Extension '{name}' not found");
    }
    println!("Removed extension: {name}");
    Ok(())
}

/// Update an extension (or all git-backed extensions).
pub fn update(name: Option<&str>) -> anyhow::Result<()> {
    let explicit_name = name.is_some();
    let Some(mutation) = extension_mutation_directory(false)? else {
        if let Some(name) = name {
            anyhow::bail!("Extension '{name}' not found");
        }
        println!("No extensions installed.");
        return Ok(());
    };
    let names = if let Some(name) = name {
        validate_name(name)?;
        vec![name.as_bytes().to_vec()]
    } else {
        let mut names = mutation.entry_names(10_000)?;
        names.sort();
        names
    };
    if names.is_empty() {
        println!("No updatable extensions.");
        return Ok(());
    }
    let mut updates = Vec::new();
    for raw_name in names {
        if crate::contribution_loading::is_internal_contribution_entry(&raw_name) {
            continue;
        }
        let Ok(name) = std::str::from_utf8(&raw_name) else {
            continue;
        };
        let discovered = (|| -> anyhow::Result<Option<PendingExtensionUpdate>> {
            let directory = mutation
                .open_directory(&raw_name)?
                .ok_or_else(|| anyhow::anyhow!("not a canonical extension directory"))?;
            let Some(source) = extension_update_source(&mutation, &raw_name)? else {
                return Ok(None);
            };
            let manifest = crate::contribution_loading::read_file_at(
                &directory,
                b"manifest.toml",
                1024 * 1024,
            )?
            .ok_or_else(|| anyhow::anyhow!("installed extension has no manifest"))?;
            let manifest: ExtensionManifest = toml::from_str(std::str::from_utf8(&manifest)?)?;
            Ok(Some(PendingExtensionUpdate {
                name: name.to_string(),
                extension_name: manifest.extension.name,
                source,
                identity: omegon_maintenance_contracts::path_identity(&directory)?,
            }))
        })();
        match discovered {
            Ok(Some(update)) => updates.push(update),
            Ok(None) => println!("  {name}: skipped (no git source metadata)"),
            Err(error) if explicit_name => return Err(error),
            Err(error) => eprintln!("  {name}: discovery failed: {error:#}"),
        }
    }
    drop(mutation);

    for pending in updates {
        let result = update_one(&pending);
        if let Err(error) = result {
            if explicit_name {
                return Err(error);
            }
            eprintln!("  {}: update failed: {error:#}", pending.name);
            continue;
        }
        println!("  {}: updated", pending.name);
    }
    Ok(())
}

fn update_one(pending: &PendingExtensionUpdate) -> anyhow::Result<()> {
    let prepared = prepare_git_source(&pending.source)?;
    if prepared.manifest.extension.name != pending.extension_name {
        anyhow::bail!(
            "updated extension identity changed from '{}' to '{}'",
            pending.extension_name,
            prepared.manifest.extension.name
        );
    }
    let mutation = extension_mutation_directory(false)?
        .ok_or_else(|| anyhow::anyhow!("Extension '{}' not found", pending.name))?;
    let directory = mutation
        .open_directory(pending.name.as_bytes())?
        .ok_or_else(|| anyhow::anyhow!("Extension '{}' not found", pending.name))?;
    if omegon_maintenance_contracts::path_identity(&directory)? != pending.identity {
        anyhow::bail!("extension identity changed while preparing update");
    }
    let config =
        crate::contribution_loading::read_file_at(&directory, b"config.toml", 1024 * 1024)?;
    let (_, state) = mutation.read_file_in_directory(
        pending.name.as_bytes(),
        b".omegon",
        b"state.toml",
        1024 * 1024,
    )?;
    import_extension_source(
        &mutation,
        &prepared.source,
        &prepared.manifest_bytes,
        Some(&pending.name),
        true,
        Some(&prepared.source_metadata()?),
        Some(&pending.identity),
        config.as_deref(),
        state.as_deref(),
    )?;
    Ok(())
}

struct PendingExtensionUpdate {
    name: String,
    extension_name: String,
    source: GitSource,
    identity: omegon_maintenance_contracts::PathIdentityV1,
}

#[derive(Clone)]
struct GitSource {
    uri: String,
    reference: Option<String>,
}

fn extension_update_source(
    mutation: &crate::contribution_loading::GuardedContributionMutationDirectory,
    raw_name: &[u8],
) -> anyhow::Result<Option<GitSource>> {
    let (_, metadata) = mutation.read_file_in_directory(
        raw_name,
        b".omegon",
        b"install-source.toml",
        1024 * 1024,
    )?;
    if let Some(metadata) = metadata {
        let table: toml::Table = toml::from_str(std::str::from_utf8(&metadata)?)?;
        return Ok(table
            .get("source")
            .and_then(toml::Value::as_str)
            .map(|source| GitSource {
                uri: source.to_string(),
                reference: table
                    .get("reference")
                    .and_then(toml::Value::as_str)
                    .map(str::to_string),
            }));
    }
    let (_, config) = mutation.read_file_in_directory(raw_name, b".git", b"config", 1024 * 1024)?;
    let Some(config) = config else {
        return Ok(None);
    };
    let Some(source) = git_origin_from_config(std::str::from_utf8(&config)?) else {
        return Ok(None);
    };
    let (_, head) = mutation.read_file_in_directory(raw_name, b".git", b"HEAD", 1024)?;
    let reference = head
        .as_deref()
        .and_then(|head| git_reference_from_head(std::str::from_utf8(head).ok()?));
    Ok(Some(GitSource {
        uri: source,
        reference,
    }))
}

fn git_origin_from_config(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line == r#"[remote "origin"]"#;
        } else if in_origin
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "url"
        {
            let value = value.trim();
            return (!value.is_empty()).then(|| value.to_string());
        }
    }
    None
}

fn git_reference_from_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: refs/heads/") {
        return (!reference.is_empty()).then(|| reference.to_string());
    }
    (!head.is_empty()).then(|| head.to_string())
}

/// Enable a disabled extension.
pub fn enable(name: &str) -> anyhow::Result<()> {
    let state = mutate_extension_state(name, |state| {
        if state.enabled {
            return false;
        }
        state.mark_enabled();
        true
    })?;

    if !state {
        println!("Extension '{name}' is already enabled.");
        return Ok(());
    }
    println!("Enabled extension '{name}'.");
    Ok(())
}

/// Disable an extension (prevents spawning on next startup).
pub fn disable(name: &str) -> anyhow::Result<()> {
    let state = mutate_extension_state(name, |state| {
        if !state.enabled {
            return false;
        }
        state.mark_disabled();
        true
    })?;

    if !state {
        println!("Extension '{name}' is already disabled.");
        return Ok(());
    }
    println!("Disabled extension '{name}'.");
    Ok(())
}

pub(crate) fn set_config(name: &str, key: &str, value: &str) -> anyhow::Result<()> {
    validate_name(name)?;
    let mutation = extension_mutation_directory(false)?
        .ok_or_else(|| anyhow::anyhow!("Extension '{name}' not found"))?;
    let directory = mutation
        .open_directory(name.as_bytes())?
        .ok_or_else(|| anyhow::anyhow!("Extension '{name}' not found"))?;
    let identity = omegon_maintenance_contracts::path_identity(&directory)?;
    let manifest =
        crate::contribution_loading::read_file_at(&directory, b"manifest.toml", 1024 * 1024)?
            .ok_or_else(|| anyhow::anyhow!("extension '{name}' has no manifest"))?;
    let manifest: ExtensionManifest = toml::from_str(std::str::from_utf8(&manifest)?)?;
    if !manifest.config.is_empty() {
        let field = manifest.config.get(key).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown config key '{key}' for extension '{name}'. Declared keys: {:?}",
                manifest.config.keys().collect::<Vec<_>>()
            )
        })?;
        crate::extensions::config_store::validate_field(field, value)?;
    }
    let mut table =
        match crate::contribution_loading::read_file_at(&directory, b"config.toml", 1024 * 1024)? {
            Some(bytes) => toml::from_str(std::str::from_utf8(&bytes)?)?,
            None => toml::Table::new(),
        };
    table.insert(key.to_string(), toml::Value::String(value.to_string()));
    let content = toml::to_string_pretty(&table)?;
    mutation.write_file_in_existing_directory(
        name.as_bytes(),
        b"config.toml",
        content.as_bytes(),
        &identity,
    )
}

fn mutate_extension_state(
    name: &str,
    mutate: impl FnOnce(&mut ExtensionState) -> bool,
) -> anyhow::Result<bool> {
    validate_name(name)?;
    let mutation = extension_mutation_directory(false)?
        .ok_or_else(|| anyhow::anyhow!("Extension '{name}' not found"))?;
    let (identity, state) =
        mutation.read_file_in_directory(name.as_bytes(), b".omegon", b"state.toml", 1024 * 1024)?;
    let mut state = match state {
        Some(bytes) => toml::from_str(std::str::from_utf8(&bytes)?)?,
        None => ExtensionState::new(),
    };
    if !mutate(&mut state) {
        return Ok(false);
    }
    let content = toml::to_string_pretty(&state)?;
    mutation.write_file_in_directory(
        name.as_bytes(),
        b".omegon",
        b"state.toml",
        content.as_bytes(),
        &identity,
    )?;
    Ok(true)
}

fn extension_mutation_directory(
    create: bool,
) -> anyhow::Result<Option<crate::contribution_loading::GuardedContributionMutationDirectory>> {
    let home = crate::paths::omegon_home()?;
    if create {
        return Ok(Some(
            crate::contribution_loading::GuardedContributionMutationDirectory::open_or_create(
                &home,
                &[b"extensions"],
                &home,
                omegon_maintenance_contracts::ContributionKind::Extension,
                "user",
            )?,
        ));
    }
    crate::contribution_loading::GuardedContributionMutationDirectory::open_existing(
        &home,
        &[b"extensions"],
        &home,
        omegon_maintenance_contracts::ContributionKind::Extension,
        "user",
    )
}

pub(crate) fn extensions_dir() -> anyhow::Result<PathBuf> {
    let base = crate::paths::omegon_home()?;
    Ok(base.join("extensions"))
}

/// Validate that an extension name is safe for use as a directory component.
/// Rejects path traversal attempts and any non-filesystem-safe characters.
fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("extension name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") || name.contains('\0') {
        anyhow::bail!(
            "invalid extension name '{name}': must not contain '/', '\\', '..', or null bytes"
        );
    }
    // Reject absolute paths on Windows (e.g. "C:")
    if name.contains(':') {
        anyhow::bail!("invalid extension name '{name}': must not contain ':'");
    }
    Ok(())
}

fn install_local(local_path: &Path) -> anyhow::Result<()> {
    let source = std::fs::File::open(local_path)?;
    let manifest_bytes =
        crate::contribution_loading::read_file_at(&source, b"manifest.toml", 1024 * 1024)?
            .ok_or_else(|| anyhow::anyhow!("local extension has no manifest.toml"))?;
    let manifest: ExtensionManifest = toml::from_str(std::str::from_utf8(&manifest_bytes)?)?;
    let name = &manifest.extension.name;
    validate_name(name)?;

    // Verify binary exists for native extensions
    if manifest.is_native() {
        match manifest.native_binary_path(local_path) {
            Ok(_) => {}
            Err(_) => {
                println!(
                    "Warning: native binary not found. Build with `cargo build --release` before running."
                );
            }
        }
    }

    let binary_path = match &manifest.runtime {
        crate::extensions::manifest::RuntimeConfig::Native { binary, .. } => {
            Some(Path::new(binary.as_str()))
        }
        crate::extensions::manifest::RuntimeConfig::Oci { .. } => None,
    };
    let mutation = extension_mutation_directory(true)?.expect("create returns a mutation root");
    mutation.import_extension_directory(
        name.as_bytes(),
        &source,
        binary_path,
        &manifest_bytes,
        false,
    )?;

    println!("Installed local extension '{}'", name);

    print_secrets_hint(&manifest);

    Ok(())
}

fn install_git(extensions_dir: &Path, uri: &str) -> anyhow::Result<()> {
    let _ = extensions_dir;
    let inferred_name = infer_extension_name(uri)?;
    let prepared = prepare_git_source(&GitSource {
        uri: uri.to_string(),
        reference: None,
    })?;
    let source = &prepared.source;
    let manifest = &prepared.manifest;
    if manifest.extension.name != inferred_name {
        println!(
            "Note: inferred name '{}' but manifest declares '{}'.",
            inferred_name, manifest.extension.name
        );
    }

    publish_extension_source(
        source,
        &prepared.manifest_bytes,
        false,
        Some(&prepared.source_metadata()?),
    )?;

    println!(
        "Installed extension '{}' from {uri}",
        manifest.extension.name
    );
    print_secrets_hint(manifest);

    Ok(())
}

/// Install a pre-built extension from a tarball URL or local .tar.gz file.
///
/// The tarball must contain a `manifest.toml` and (for native extensions) the
/// pre-built binary.  No build step is performed — this is the path for users
/// without a Rust toolchain.
pub(crate) fn install_tarball(_extensions_dir: &Path, uri: &str) -> anyhow::Result<()> {
    const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
    let cleanup = create_prepared_directory("omegon-ext-install")?;
    let tmp = &cleanup.0;
    let archive_path = tmp.join("extension.tar.gz");

    if uri.starts_with("http://") || uri.starts_with("https://") {
        println!("Downloading {uri}...");
        let mut response = reqwest::blocking::get(uri)?.error_for_status()?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_ARCHIVE_BYTES)
        {
            anyhow::bail!("extension archive exceeds the download size limit");
        }
        let mut archive = std::fs::File::create(&archive_path)?;
        let copied = std::io::copy(
            &mut std::io::Read::take(&mut response, MAX_ARCHIVE_BYTES + 1),
            &mut archive,
        )?;
        if copied > MAX_ARCHIVE_BYTES {
            anyhow::bail!("extension archive exceeds the download size limit");
        }
    } else {
        let local = Path::new(uri);
        let mut local = std::fs::File::open(local)
            .map_err(|error| anyhow::anyhow!("could not open tarball {uri}: {error}"))?;
        let metadata = local.metadata()?;
        if !metadata.is_file() {
            anyhow::bail!("tarball source is not a regular file: {uri}");
        }
        if metadata.len() > MAX_ARCHIVE_BYTES {
            anyhow::bail!("extension archive exceeds the download size limit");
        }
        let mut archive = std::fs::File::create(&archive_path)?;
        let copied = std::io::copy(
            &mut std::io::Read::take(&mut local, MAX_ARCHIVE_BYTES + 1),
            &mut archive,
        )?;
        if copied > MAX_ARCHIVE_BYTES {
            anyhow::bail!("extension archive exceeds the download size limit");
        }
    }

    let extract_dir = tmp.join("extracted");
    std::fs::create_dir_all(&extract_dir)?;
    extract_extension_archive(&archive_path, &extract_dir)?;

    // Find manifest.toml — may be at root or one level deep
    let manifest_path = if extract_dir.join("manifest.toml").exists() {
        extract_dir.join("manifest.toml")
    } else {
        // Check one level deep (tarball may have a top-level directory)
        let mut found = None;
        for entry in std::fs::read_dir(&extract_dir)? {
            let entry = entry?;
            if entry.path().is_dir() && entry.path().join("manifest.toml").exists() {
                found = Some(entry.path().join("manifest.toml"));
                break;
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("tarball does not contain manifest.toml"))?
    };

    let ext_root = manifest_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid manifest path"))?;
    let manifest = ExtensionManifest::from_file(&manifest_path)?;
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let name = &manifest.extension.name;
    validate_name(name)?;
    publish_extension_source(ext_root, &manifest_bytes, false, None)?;

    // Make native binary executable
    if manifest.is_native() {
        if manifest.native_binary_path(ext_root).is_ok() {
            println!(
                "Installed extension '{}' v{} (pre-built binary)",
                name, manifest.extension.version
            );
        } else {
            println!(
                "Installed extension '{}' v{} (warning: native binary not found in tarball)",
                name, manifest.extension.version
            );
        }
    } else {
        println!(
            "Installed extension '{}' v{}",
            name, manifest.extension.version
        );
    }

    print_secrets_hint(&manifest);

    Ok(())
}

fn publish_extension_source(
    source_path: &Path,
    manifest_bytes: &[u8],
    overwrite: bool,
    install_source: Option<&[u8]>,
) -> anyhow::Result<()> {
    let mutation = extension_mutation_directory(true)?.expect("create returns a mutation root");
    import_extension_source(
        &mutation,
        source_path,
        manifest_bytes,
        None,
        overwrite,
        install_source,
        None,
        None,
        None,
    )
    .map(|_| ())
}

#[allow(clippy::too_many_arguments)]
fn import_extension_source(
    mutation: &crate::contribution_loading::GuardedContributionMutationDirectory,
    source_path: &Path,
    expected_manifest_bytes: &[u8],
    target_name: Option<&str>,
    overwrite: bool,
    install_source: Option<&[u8]>,
    expected_existing: Option<&omegon_maintenance_contracts::PathIdentityV1>,
    config: Option<&[u8]>,
    state: Option<&[u8]>,
) -> anyhow::Result<ExtensionManifest> {
    let source = std::fs::File::open(source_path)?;
    let manifest_bytes =
        crate::contribution_loading::read_file_at(&source, b"manifest.toml", 1024 * 1024)?
            .ok_or_else(|| anyhow::anyhow!("extension source has no manifest.toml"))?;
    if manifest_bytes != expected_manifest_bytes {
        anyhow::bail!("extension manifest changed after candidate preparation");
    }
    let manifest: ExtensionManifest = toml::from_str(std::str::from_utf8(&manifest_bytes)?)?;
    validate_name(&manifest.extension.name)?;
    let target_name = target_name.unwrap_or(&manifest.extension.name);
    validate_name(target_name)?;
    let binary_path = match &manifest.runtime {
        crate::extensions::manifest::RuntimeConfig::Native { binary, .. } => {
            Some(Path::new(binary.as_str()))
        }
        crate::extensions::manifest::RuntimeConfig::Oci { .. } => None,
    };
    if !overwrite
        && mutation
            .entry_names(10_000)?
            .iter()
            .any(|name| name == target_name.as_bytes())
    {
        anyhow::bail!(
            "Extension '{}' is already installed",
            manifest.extension.name
        );
    }
    mutation.import_extension_directory_with_state(
        target_name.as_bytes(),
        &source,
        binary_path,
        &manifest_bytes,
        overwrite,
        expected_existing,
        install_source,
        config,
        state,
    )?;
    Ok(manifest)
}

struct PreparedGitSource {
    _directory: PreparedDirectory,
    source: PathBuf,
    manifest: ExtensionManifest,
    manifest_bytes: Vec<u8>,
    update_source: GitSource,
}

impl PreparedGitSource {
    fn source_metadata(&self) -> anyhow::Result<Vec<u8>> {
        let mut table = toml::Table::new();
        table.insert(
            "source".to_string(),
            toml::Value::String(self.update_source.uri.clone()),
        );
        if let Some(reference) = &self.update_source.reference {
            table.insert(
                "reference".to_string(),
                toml::Value::String(reference.clone()),
            );
        }
        Ok(toml::to_string_pretty(&table)?.into_bytes())
    }
}

struct PreparedDirectory(PathBuf);

impl Drop for PreparedDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            tracing::warn!(path = %self.0.display(), %error, "could not remove extension preparation directory");
        }
    }
}

fn create_prepared_directory(prefix: &str) -> anyhow::Result<PreparedDirectory> {
    let root = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&root)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir(&root)?;
    Ok(PreparedDirectory(root))
}

fn extract_extension_archive(archive_path: &Path, destination: &Path) -> anyhow::Result<()> {
    const MAX_ENTRIES: usize = 10_000;
    const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
    let archive = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut archive = tar::Archive::new(decoder);
    let mut paths = std::collections::HashSet::new();
    let mut entries = 0_usize;
    let mut extracted_bytes = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        entries += 1;
        if entries > MAX_ENTRIES {
            anyhow::bail!("extension archive exceeds the entry limit");
        }
        let path = entry.path()?.into_owned();
        let components = path.components().collect::<Vec<_>>();
        if components.is_empty()
            || components.len() > 32
            || components
                .iter()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            anyhow::bail!("extension archive contains an unsafe path");
        }
        if !paths.insert(path.clone()) {
            anyhow::bail!("extension archive contains duplicate paths");
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_dir() || entry_type.is_file()) {
            anyhow::bail!("extension archive contains links or special entries");
        }
        if entry_type.is_file() {
            extracted_bytes = extracted_bytes
                .checked_add(entry.header().size()?)
                .ok_or_else(|| anyhow::anyhow!("extension archive size overflow"))?;
            if extracted_bytes > MAX_EXTRACTED_BYTES {
                anyhow::bail!("extension archive exceeds the extracted size limit");
            }
        }
        if !entry.unpack_in(destination)? {
            anyhow::bail!("extension archive entry escapes the extraction directory");
        }
    }
    Ok(())
}

fn prepare_git_source(update_source: &GitSource) -> anyhow::Result<PreparedGitSource> {
    let uri = &update_source.uri;
    let directory = create_prepared_directory("omegon-extension")?;
    let root = directory.0.clone();
    let source = root.join("source");
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(uri)
        .arg(&source)
        .status()?;
    if !status.success() {
        anyhow::bail!(
            "git clone failed for {uri}\n  \
             Check: URL is correct, you have network access, and git credentials are configured."
        );
    }
    if let Some(reference) = &update_source.reference {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["checkout", reference])
            .status()?;
        if !status.success() {
            anyhow::bail!("git checkout failed for reference '{reference}' from {uri}");
        }
    }
    let manifest_path = source.join("manifest.toml");
    if !manifest_path.exists() {
        anyhow::bail!("cloned repository does not contain manifest.toml");
    }
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let manifest: ExtensionManifest = toml::from_str(std::str::from_utf8(&manifest_bytes)?)?;
    validate_name(&manifest.extension.name)?;
    if manifest.is_native() && source.join("Cargo.toml").exists() {
        println!("Building extension '{}'...", manifest.extension.name);
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(&source)
            .status()?;
        if !status.success() {
            anyhow::bail!(
                "cargo build --release failed (exit {}) for extension '{}'",
                status.code().unwrap_or(-1),
                manifest.extension.name
            );
        }
        println!("Build succeeded.");
    } else if manifest.is_native() && manifest.native_binary_path(&source).is_err() {
        println!("Warning: native binary not found. Build manually before installing.");
    }
    Ok(PreparedGitSource {
        _directory: directory,
        source,
        manifest,
        manifest_bytes,
        update_source: GitSource {
            uri: uri.clone(),
            reference: git_reference(&root.join("source"))?,
        },
    })
}

fn git_reference(repository: &Path) -> anyhow::Result<Option<String>> {
    let branch = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()?;
    if branch.status.success() {
        let branch = String::from_utf8(branch.stdout)?.trim().to_string();
        return Ok((!branch.is_empty()).then_some(branch));
    }
    let revision = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !revision.status.success() {
        return Ok(None);
    }
    let revision = String::from_utf8(revision.stdout)?.trim().to_string();
    Ok((!revision.is_empty()).then_some(revision))
}

fn infer_extension_name(uri: &str) -> anyhow::Result<String> {
    let stripped = uri.trim_end_matches('/').trim_end_matches(".git");
    let name = stripped
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .or_else(|| stripped.rsplit_once(':').map(|(_, tail)| tail))
        .ok_or_else(|| anyhow::anyhow!("could not infer extension name from URI: {uri}"))?;

    if name.is_empty() {
        anyhow::bail!("could not infer extension name from URI: {uri}");
    }

    Ok(name.to_string())
}

fn print_secrets_hint(manifest: &ExtensionManifest) {
    let all_secrets: Vec<&String> = manifest
        .secrets
        .required
        .iter()
        .chain(manifest.secrets.optional.iter())
        .collect();

    if all_secrets.is_empty() {
        return;
    }

    println!();
    if !manifest.secrets.required.is_empty() {
        println!("Required secrets:");
        for s in &manifest.secrets.required {
            println!("  omegon secret set {s} <value>");
        }
    }
    if !manifest.secrets.optional.is_empty() {
        println!("Optional secrets (for additional connectors):");
        for s in &manifest.secrets.optional {
            println!("  omegon secret set {s} <value>");
        }
    }
}

struct ExtensionSummary {
    name: String,
    version: String,
    runtime: String,
    status: String,
    description: String,
}

fn load_extension_summary(dir: &Path) -> anyhow::Result<ExtensionSummary> {
    let manifest = ExtensionManifest::from_extension_dir(dir)?;
    let state = ExtensionState::load(dir)?;

    let runtime = if manifest.is_native() {
        "native"
    } else {
        "oci"
    };

    Ok(ExtensionSummary {
        name: manifest.extension.name,
        version: manifest.extension.version,
        runtime: runtime.to_string(),
        status: state.status_text(),
        description: manifest.extension.description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard(Option<std::ffi::OsString>);

    impl EnvGuard {
        fn isolate(home: &Path) -> Self {
            let previous = std::env::var_os("OMEGON_HOME");
            // SAFETY: guarded extension tests hold the shared environment lock.
            unsafe { std::env::set_var("OMEGON_HOME", home) };
            Self(previous)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: guarded extension tests hold the shared environment lock.
            unsafe {
                if let Some(previous) = self.0.take() {
                    std::env::set_var("OMEGON_HOME", previous);
                } else {
                    std::env::remove_var("OMEGON_HOME");
                }
            }
        }
    }

    #[test]
    fn infer_extension_name_from_https() {
        let name = infer_extension_name("https://github.com/styrene-lab/vox.git").unwrap();
        assert_eq!(name, "vox");
    }

    #[test]
    fn infer_extension_name_from_ssh() {
        let name = infer_extension_name("git@github.com:styrene-lab/vox.git").unwrap();
        assert_eq!(name, "vox");
    }

    #[test]
    fn infer_extension_name_from_local() {
        let name = infer_extension_name("./extensions/vox").unwrap();
        assert_eq!(name, "vox");
    }

    #[test]
    fn install_rejects_invalid_uri() {
        let err = install("not-a-uri").unwrap_err();
        assert!(err.to_string().contains("not a valid extension source"));
    }

    #[test]
    fn list_summary_handles_missing_dir() {
        let summary = list_summary().unwrap();
        // Either reports extensions or says none installed
        assert!(summary.contains("extension") || summary.contains("DESCRIPTION"));
    }

    #[test]
    fn remove_rejects_path_traversal() {
        let err = remove("../../.ssh").unwrap_err();
        assert!(err.to_string().contains("must not contain"));
    }

    #[test]
    fn remove_rejects_slash_in_name() {
        let err = remove("foo/bar").unwrap_err();
        assert!(err.to_string().contains("must not contain"));
    }

    #[test]
    fn validate_name_rejects_empty() {
        let err = validate_name("").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn validate_name_accepts_normal_names() {
        validate_name("vox").unwrap();
        validate_name("scribe-rpc").unwrap();
        validate_name("my_extension.v2").unwrap();
    }

    #[test]
    fn enable_disable_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let ext = tmp.path().join("test-ext");
        std::fs::create_dir_all(ext.join(".omegon")).unwrap();
        std::fs::write(
            ext.join("manifest.toml"),
            r#"
[extension]
name = "test-ext"
version = "0.1.0"
description = "Test"

[runtime]
type = "native"
binary = "bin/test"
"#,
        )
        .unwrap();

        // Start enabled (default)
        let state = ExtensionState::load(&ext).unwrap();
        assert!(state.enabled);

        // Disable
        let mut state = ExtensionState::load(&ext).unwrap();
        state.mark_disabled();
        state.save(&ext).unwrap();

        let state = ExtensionState::load(&ext).unwrap();
        assert!(!state.enabled);
        assert_eq!(state.status_text(), "disabled");

        // Re-enable
        let mut state = ExtensionState::load(&ext).unwrap();
        state.mark_enabled();
        state.save(&ext).unwrap();

        let state = ExtensionState::load(&ext).unwrap();
        assert!(state.enabled);
        assert_eq!(state.status_text(), "enabled");
    }

    #[cfg(unix)]
    #[test]
    fn guarded_extension_state_config_and_remove_roundtrip() {
        let _lock = crate::test_support::env::lock();
        let home = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        let ext = home.path().join("extensions/test-ext");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(
            ext.join("manifest.toml"),
            r#"
[extension]
name = "test-ext"
version = "0.1.0"
description = "Test"

[runtime]
type = "native"
binary = "bin/test"

[config.mode]
type = "string"
label = "Mode"
description = "Execution mode"
default = "safe"
"#,
        )
        .unwrap();

        disable("test-ext").unwrap();
        assert!(!ExtensionState::load(&ext).unwrap().enabled);
        enable("test-ext").unwrap();
        assert!(ExtensionState::load(&ext).unwrap().enabled);
        set_config("test-ext", "mode", "fast").unwrap();
        assert_eq!(
            crate::extensions::config_store::read_config(&ext)
                .unwrap()
                .get("mode")
                .map(String::as_str),
            Some("fast")
        );
        remove("test-ext").unwrap();
        assert!(!ext.exists());
    }

    #[cfg(unix)]
    #[test]
    fn guarded_remove_unlinks_legacy_extension_symlink_only() {
        use std::os::unix::fs::symlink;

        let _lock = crate::test_support::env::lock();
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        std::fs::create_dir_all(home.path().join("extensions")).unwrap();
        let link = home.path().join("extensions/linked");
        symlink(external.path(), &link).unwrap();

        remove("linked").unwrap();

        assert!(!link.exists());
        assert!(external.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn install_local_copies_extension_into_guarded_root() {
        let _lock = crate::test_support::env::lock();
        let tmp = tempfile::tempdir().unwrap();
        let ext = tmp.path().join("test-ext");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(
            ext.join("manifest.toml"),
            r#"
[extension]
name = "test-ext"
version = "0.1.0"
description = "Test extension"

[runtime]
type = "native"
binary = "target/release/test-ext"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(ext.join("target/release")).unwrap();
        std::fs::create_dir_all(ext.join("target/debug/incremental")).unwrap();
        std::fs::write(ext.join("target/release/test-ext"), "release-binary").unwrap();
        std::fs::write(ext.join("target/debug/incremental/junk"), "junk").unwrap();

        let home = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        install_local(&ext).unwrap();

        let installed = home.path().join("extensions/test-ext");
        assert!(installed.is_dir());
        assert!(!installed.is_symlink());
        assert_eq!(
            std::fs::read_to_string(installed.join("target/release/test-ext")).unwrap(),
            "release-binary"
        );
        assert!(!installed.join("target/debug").exists());
        std::fs::write(ext.join("source-only"), "changed").unwrap();
        assert!(!installed.join("source-only").exists());
    }

    #[cfg(unix)]
    #[test]
    fn install_tarball_from_local_file() {
        let _lock = crate::test_support::env::lock();
        let tmp = tempfile::tempdir().unwrap();

        // Build a tarball containing manifest.toml + a fake binary
        let staging = tmp.path().join("my-ext");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("manifest.toml"),
            r#"
[extension]
name = "my-ext"
version = "1.0.0"
description = "Test tarball extension"

[runtime]
type = "native"
binary = "my-ext"
"#,
        )
        .unwrap();
        // Write a fake binary (just needs to exist)
        std::fs::write(staging.join("my-ext"), "#!/bin/sh\necho ok").unwrap();

        // Create .tar.gz
        let tarball = tmp.path().join("my-ext.tar.gz");
        let status = std::process::Command::new("tar")
            .args(["czf"])
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("my-ext")
            .status()
            .unwrap();
        assert!(status.success(), "tar should succeed");

        // Install into a fresh extensions dir
        let home = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        let ext_dir = home.path().join("extensions");
        install_tarball(&ext_dir, tarball.to_str().unwrap()).unwrap();

        let installed = ext_dir.join("my-ext");
        assert!(installed.exists(), "extension dir should exist");
        assert!(
            installed.join("manifest.toml").exists(),
            "manifest should exist"
        );
        assert!(installed.join("my-ext").exists(), "binary should exist");

        // Verify it's a real copy, not a symlink
        assert!(!installed.is_symlink(), "should not be a symlink");
    }

    #[test]
    fn install_tarball_rejects_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();

        // Build a tarball with no manifest.toml
        let staging = tmp.path().join("bad-ext");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("README.md"), "no manifest here").unwrap();

        let tarball = tmp.path().join("bad-ext.tar.gz");
        let status = std::process::Command::new("tar")
            .args(["czf"])
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("bad-ext")
            .status()
            .unwrap();
        assert!(status.success());

        let ext_dir = tempfile::tempdir().unwrap();
        let err = install_tarball(ext_dir.path(), tarball.to_str().unwrap()).unwrap_err();
        assert!(
            err.to_string().contains("manifest.toml"),
            "should mention missing manifest: {}",
            err
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_tarball_rejects_duplicate() {
        let _lock = crate::test_support::env::lock();
        let tmp = tempfile::tempdir().unwrap();

        let staging = tmp.path().join("dup-ext");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("manifest.toml"),
            r#"
[extension]
name = "dup-ext"
version = "1.0.0"
description = "Duplicate test"

[runtime]
type = "native"
binary = "dup-ext"
"#,
        )
        .unwrap();
        std::fs::write(staging.join("dup-ext"), "fake").unwrap();

        let tarball = tmp.path().join("dup-ext.tar.gz");
        std::process::Command::new("tar")
            .args(["czf"])
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("dup-ext")
            .status()
            .unwrap();

        let home = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        let ext_dir = home.path().join("extensions");

        // First install succeeds
        install_tarball(&ext_dir, tarball.to_str().unwrap()).unwrap();

        // Second install fails with "already installed"
        let err = install_tarball(&ext_dir, tarball.to_str().unwrap()).unwrap_err();
        assert!(
            err.to_string().contains("already installed"),
            "should reject duplicate: {}",
            err
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_tarball_rejects_links() {
        let _lock = crate::test_support::env::lock();
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("linked-ext");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("manifest.toml"),
            r#"[extension]
name = "linked-ext"
version = "1.0.0"
description = "Link fixture"

[runtime]
type = "oci"
image = "example.invalid/linked-ext:1.0.0"
"#,
        )
        .unwrap();
        std::os::unix::fs::symlink("manifest.toml", staging.join("linked-manifest")).unwrap();
        let tarball = tmp.path().join("linked-ext.tar.gz");
        let status = std::process::Command::new("tar")
            .args(["czf"])
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("linked-ext")
            .status()
            .unwrap();
        assert!(status.success());
        let home = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());

        let error = install_tarball(&home.path().join("extensions"), tarball.to_str().unwrap())
            .unwrap_err();
        assert!(error.to_string().contains("links or special entries"));
    }

    #[cfg(unix)]
    #[test]
    fn git_extension_update_replaces_complete_guarded_bundle() {
        let _lock = crate::test_support::env::lock();
        let home = tempfile::tempdir().unwrap();
        let repository = tempfile::tempdir().unwrap();
        let _env = EnvGuard::isolate(home.path());
        run_git(repository.path(), &["init"]);
        write_git_extension_manifest(repository.path(), "1.0.0");
        run_git(repository.path(), &["add", "."]);
        run_git(
            repository.path(),
            &[
                "-c",
                "user.name=Omegon Test",
                "-c",
                "user.email=omegon@example.invalid",
                "commit",
                "-m",
                "initial",
            ],
        );

        install_git(
            &home.path().join("extensions"),
            repository.path().to_str().unwrap(),
        )
        .unwrap();
        let installed = home.path().join("extensions/git-fixture");
        assert!(!installed.join(".git").exists());
        assert!(installed.join(".omegon/install-source.toml").is_file());
        disable("git-fixture").unwrap();
        set_config("git-fixture", "endpoint", "operator-value").unwrap();

        write_git_extension_manifest(repository.path(), "2.0.0");
        run_git(repository.path(), &["add", "manifest.toml"]);
        run_git(
            repository.path(),
            &[
                "-c",
                "user.name=Omegon Test",
                "-c",
                "user.email=omegon@example.invalid",
                "commit",
                "-m",
                "update",
            ],
        );
        update(Some("git-fixture")).unwrap();

        let manifest = ExtensionManifest::from_extension_dir(&installed).unwrap();
        assert_eq!(manifest.extension.version, "2.0.0");
        assert!(installed.join(".omegon/install-source.toml").is_file());
        assert!(!ExtensionState::load(&installed).unwrap().enabled);
        assert!(
            std::fs::read_to_string(installed.join("config.toml"))
                .unwrap()
                .contains("operator-value")
        );
    }

    #[cfg(unix)]
    fn write_git_extension_manifest(directory: &Path, version: &str) {
        std::fs::write(
            directory.join("manifest.toml"),
            format!(
                r#"[extension]
name = "git-fixture"
version = "{version}"
description = "Git fixture"

[runtime]
type = "oci"
image = "example.invalid/git-fixture:{version}"
"#
            ),
        )
        .unwrap();
    }

    #[cfg(unix)]
    fn run_git(directory: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn install_routes_tarball_url() {
        // Verify the install() dispatcher recognizes .tar.gz URLs
        // (will fail on network, but should not fall through to git or "invalid source")
        let err = install("https://example.com/ext-1.0.tar.gz").unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("not a valid extension source"),
            "should route to tarball path, not reject: {}",
            msg
        );
    }
}
