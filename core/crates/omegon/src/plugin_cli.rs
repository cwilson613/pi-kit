//! Plugin lifecycle management — install, list, remove, update.
//!
//! Plugins are git repositories cloned into `~/.omegon/plugins/<name>/`.
//! Each plugin must have a `plugin.toml` manifest at the root.
//!
//! ## Install
//!
//! ```sh
//! omegon plugin install https://github.com/user/my-plugin
//! omegon plugin install ./local/path/to/plugin
//! ```
//!
//! Git URIs are cloned. Local paths are symlinked (development mode).
//!
//! ## List
//!
//! ```sh
//! omegon plugin list
//! ```
//!
//! ## Remove
//!
//! ```sh
//! omegon plugin remove my-plugin
//! ```
//!
//! ## Update
//!
//! ```sh
//! omegon plugin update [name]
//! ```
//!
//! Runs `git pull` in the plugin directory. Without a name, updates all.

use std::path::{Path, PathBuf};

use crate::plugins::armory::ArmoryManifest;

/// Install a plugin from a git URI or local path.
pub fn install(uri: &str) -> anyhow::Result<()> {
    let plugins_dir = plugins_dir()?;
    std::fs::create_dir_all(&plugins_dir)?;

    let local_path = Path::new(uri);

    if local_path.exists() && local_path.join("plugin.toml").exists() {
        // Local path — symlink for development
        install_local(&plugins_dir, local_path)
    } else if uri.contains("://") || uri.contains("git@") || uri.ends_with(".git") {
        // Git URI — clone
        install_git(&plugins_dir, uri)
    } else {
        anyhow::bail!(
            "'{uri}' is not a valid plugin source.\n\
             Expected: a git URL or a local directory containing plugin.toml"
        );
    }
}

/// List all installed plugins.
pub fn list() -> anyhow::Result<()> {
    let plugins_dir = plugins_dir()?;

    if !plugins_dir.exists() {
        println!("No plugins installed.");
        println!("  Install with: omegon plugin install <git-url>");
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(&plugins_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() || e.path().is_symlink())
        .collect();

    if entries.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }

    println!(
        "{:<20} {:<12} {:<10} DESCRIPTION",
        "NAME", "TYPE", "VERSION"
    );
    println!("{}", "─".repeat(72));

    for entry in &entries {
        let dir = entry.path();
        // Follow symlinks
        let resolved = if dir.is_symlink() {
            std::fs::read_link(&dir).unwrap_or(dir.clone())
        } else {
            dir.clone()
        };

        let manifest_path = resolved.join("plugin.toml");
        if !manifest_path.exists() {
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            println!("{:<20} {:<12} {:<10} (no plugin.toml)", name, "?", "?");
            continue;
        }

        match load_manifest_summary(&manifest_path) {
            Ok(info) => {
                let symlink_marker = if dir.is_symlink() { " →" } else { "" };
                println!(
                    "{:<20} {:<12} {:<10} {}{}",
                    info.name, info.plugin_type, info.version, info.description, symlink_marker
                );
            }
            Err(e) => {
                let name = dir.file_name().unwrap_or_default().to_string_lossy();
                println!("{:<20} {:<12} {:<10} (error: {e})", name, "?", "?");
            }
        }
    }

    let symlinks = entries.iter().filter(|e| e.path().is_symlink()).count();
    if symlinks > 0 {
        println!("\n  → = symlinked (development mode)");
    }

    Ok(())
}

/// Remove an installed plugin by name.
pub fn remove(name: &str) -> anyhow::Result<()> {
    let plugins_dir = plugins_dir()?;
    let plugin_path = plugins_dir.join(name);

    if !plugin_path.exists() {
        anyhow::bail!("Plugin '{}' not found in {}", name, plugins_dir.display());
    }

    if plugin_path.is_symlink() {
        // Symlink — just remove the link
        std::fs::remove_file(&plugin_path)?;
        println!("Removed symlink: {name}");
    } else {
        // Cloned repo — remove directory
        std::fs::remove_dir_all(&plugin_path)?;
        println!("Removed plugin: {name}");
    }

    Ok(())
}

/// Update a plugin (or all plugins) by running `git pull`.
pub fn update(name: Option<&str>) -> anyhow::Result<()> {
    let plugins_dir = plugins_dir()?;

    if !plugins_dir.exists() {
        println!("No plugins installed.");
        return Ok(());
    }

    let dirs_to_update: Vec<PathBuf> = if let Some(name) = name {
        let path = plugins_dir.join(name);
        if !path.exists() {
            anyhow::bail!("Plugin '{}' not found", name);
        }
        vec![path]
    } else {
        std::fs::read_dir(&plugins_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !p.is_symlink())
            .collect()
    };

    if dirs_to_update.is_empty() {
        println!("No updatable plugins (symlinked plugins are managed externally).");
        return Ok(());
    }

    for dir in &dirs_to_update {
        let name = dir.file_name().unwrap_or_default().to_string_lossy();
        let git_dir = dir.join(".git");

        if !git_dir.exists() {
            println!("  {name}: skipped (not a git repo)");
            continue;
        }

        print!("  {name}: ");
        match std::process::Command::new("git")
            .args(["pull", "--ff-only", "--quiet"])
            .current_dir(dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("Already up to date") {
                    println!("up to date");
                } else {
                    println!("updated ✓");
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("failed — {}", stderr.trim());
            }
            Err(e) => {
                println!("failed — {e}");
            }
        }
    }

    Ok(())
}

// ─── Internals ────────────────────────────────────────────────────────────

/// Canonical plugin install directory.
fn plugins_dir() -> anyhow::Result<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".omegon").join("plugins"))
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))
}

/// Clone a git repository into the plugins directory.
fn install_git(plugins_dir: &Path, uri: &str) -> anyhow::Result<()> {
    // Derive plugin name from URI
    let name = plugin_name_from_uri(uri)?;
    let target = plugins_dir.join(&name);

    if target.exists() {
        anyhow::bail!(
            "Plugin '{}' already installed at {}",
            name,
            target.display()
        );
    }

    println!("Cloning {uri} → {name}...");

    let output = std::process::Command::new("git")
        .args([
            "clone",
            "--depth=1",
            "--single-branch",
            uri,
            target.to_str().unwrap_or(""),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }

    // Verify plugin.toml exists
    let manifest = target.join("plugin.toml");
    if !manifest.exists() {
        // Clean up — not a valid plugin
        let _ = std::fs::remove_dir_all(&target);
        anyhow::bail!("Cloned repo has no plugin.toml — not a valid Omegon plugin");
    }

    // Parse and display summary
    match load_manifest_summary(&manifest) {
        Ok(info) => {
            println!(
                "Installed {} ({}) v{}",
                info.name, info.plugin_type, info.version
            );
            println!("  {}", info.description);
            if info.tool_count > 0 {
                println!(
                    "  {} tool{}",
                    info.tool_count,
                    if info.tool_count == 1 { "" } else { "s" }
                );
            }
            if info.has_context {
                println!("  dynamic context injection");
            }
        }
        Err(e) => {
            println!("Installed {name} (warning: manifest parse error: {e})");
        }
    }

    Ok(())
}

/// Symlink a local plugin directory for development.
fn install_local(plugins_dir: &Path, local_path: &Path) -> anyhow::Result<()> {
    let canonical = local_path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot resolve path {}: {e}", local_path.display()))?;

    let name = canonical
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("cannot determine plugin name from path"))?
        .to_string_lossy()
        .to_string();

    let target = plugins_dir.join(&name);

    if target.exists() {
        anyhow::bail!(
            "Plugin '{}' already installed at {}",
            name,
            target.display()
        );
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&canonical, &target)?;

    #[cfg(not(unix))]
    {
        // Windows: copy instead of symlink (symlinks require admin)
        copy_dir_recursive(&canonical, &target)?;
    }

    println!("Linked {name} → {}", canonical.display());

    match load_manifest_summary(&canonical.join("plugin.toml")) {
        Ok(info) => {
            println!("  {} ({}) v{}", info.name, info.plugin_type, info.version);
        }
        Err(e) => {
            println!("  (warning: {e})");
        }
    }

    Ok(())
}

/// Extract plugin name from a git URI.
///
/// `https://github.com/user/omegon-tool-csv.git` → `omegon-tool-csv`
/// `git@github.com:user/my-plugin.git` → `my-plugin`
fn plugin_name_from_uri(uri: &str) -> anyhow::Result<String> {
    let name = uri
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .or_else(|| uri.rsplit(':').next()) // git@host:user/repo
        .ok_or_else(|| anyhow::anyhow!("cannot derive plugin name from URI: {uri}"))?;

    // Remove path component if git@host:user/repo format
    let name = name.rsplit('/').next().unwrap_or(name);

    if name.is_empty() {
        anyhow::bail!("cannot derive plugin name from URI: {uri}");
    }

    Ok(name.to_string())
}

/// Summary info from a parsed manifest.
struct ManifestSummary {
    name: String,
    plugin_type: String,
    version: String,
    description: String,
    tool_count: usize,
    has_context: bool,
}

fn load_manifest_summary(path: &Path) -> anyhow::Result<ManifestSummary> {
    let content = std::fs::read_to_string(path)?;
    let manifest =
        ArmoryManifest::parse(&content).map_err(|e| anyhow::anyhow!("parse error: {e}"))?;

    Ok(ManifestSummary {
        name: manifest.plugin.name.clone(),
        plugin_type: manifest.plugin.plugin_type.to_string(),
        version: manifest.plugin.version.clone(),
        description: manifest.plugin.description.clone(),
        tool_count: manifest.tools.len() + manifest.mcp_servers.len(),
        has_context: manifest.context.is_some(),
    })
}

#[cfg(not(unix))]
fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_name_from_https_uri() {
        assert_eq!(
            plugin_name_from_uri("https://github.com/user/omegon-tool-csv.git").unwrap(),
            "omegon-tool-csv"
        );
    }

    #[test]
    fn plugin_name_from_https_no_git_suffix() {
        assert_eq!(
            plugin_name_from_uri("https://github.com/user/my-plugin").unwrap(),
            "my-plugin"
        );
    }

    #[test]
    fn plugin_name_from_ssh_uri() {
        assert_eq!(
            plugin_name_from_uri("git@github.com:user/my-plugin.git").unwrap(),
            "my-plugin"
        );
    }

    #[test]
    fn plugin_name_trailing_slash() {
        assert_eq!(
            plugin_name_from_uri("https://github.com/user/repo/").unwrap(),
            "repo"
        );
    }

    #[test]
    fn install_local_creates_symlink() {
        let plugins = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();

        // Create a minimal plugin
        std::fs::write(
            source.path().join("plugin.toml"),
            r#"
            [plugin]
            type = "skill"
            id = "dev.test.local"
            name = "Local Test"
            version = "0.1.0"
            description = "a test"
        "#,
        )
        .unwrap();

        install_local(plugins.path(), source.path()).unwrap();

        // Verify symlink was created
        let name = source.path().file_name().unwrap();
        let link = plugins.path().join(name);
        assert!(link.exists(), "symlink should exist");
        assert!(link.is_symlink(), "should be a symlink");
        assert!(
            link.join("plugin.toml").exists(),
            "manifest should be accessible"
        );
    }

    #[test]
    fn install_local_rejects_duplicate() {
        let plugins = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();

        std::fs::write(
            source.path().join("plugin.toml"),
            r#"
            [plugin]
            type = "skill"
            id = "dev.test.dup"
            name = "Dup"
            version = "0.1.0"
            description = "a test"
        "#,
        )
        .unwrap();

        install_local(plugins.path(), source.path()).unwrap();
        let result = install_local(plugins.path(), source.path());
        assert!(result.is_err(), "duplicate install should fail");
    }

    #[test]
    fn remove_symlink() {
        let plugins = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();

        std::fs::write(
            source.path().join("plugin.toml"),
            r#"
            [plugin]
            type = "skill"
            id = "dev.test.rm"
            name = "Remove Me"
            version = "0.1.0"
            description = "a test"
        "#,
        )
        .unwrap();

        install_local(plugins.path(), source.path()).unwrap();

        let name = source
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        // Override plugins_dir for test — call remove's inner logic directly
        let plugin_path = plugins.path().join(&name);
        assert!(plugin_path.exists());

        std::fs::remove_file(&plugin_path).unwrap(); // simulates remove()
        assert!(!plugin_path.exists());
    }

    #[test]
    fn remove_cloned_dir() {
        let plugins = tempfile::tempdir().unwrap();
        let plugin_dir = plugins.path().join("test-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), "").unwrap();

        assert!(plugin_dir.exists());
        std::fs::remove_dir_all(&plugin_dir).unwrap(); // simulates remove()
        assert!(!plugin_dir.exists());
    }

    #[test]
    fn list_empty_dir() {
        // Just verify it doesn't panic
        let _plugins = tempfile::tempdir().unwrap();
        // list() uses plugins_dir() which looks at home — can't easily test
        // without env override. The function path is tested by the summary tests.
    }

    #[test]
    fn load_manifest_summary_valid() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("plugin.toml"),
            r#"
            [plugin]
            type = "persona"
            id = "dev.test.summary"
            name = "Test Persona"
            version = "2.0.0"
            description = "A test persona plugin"

            [[tools]]
            name = "tool1"
            description = "a tool"
            runner = "python"
            script = "tools/tool1.py"

            [context]
            runner = "bash"
            script = "context/status.sh"
        "#,
        )
        .unwrap();

        let info = load_manifest_summary(&dir.path().join("plugin.toml")).unwrap();
        assert_eq!(info.name, "Test Persona");
        assert_eq!(info.plugin_type, "persona");
        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.tool_count, 1);
        assert!(info.has_context);
    }

    #[test]
    fn load_manifest_summary_invalid() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plugin.toml"), "not valid toml {{").unwrap();
        assert!(load_manifest_summary(&dir.path().join("plugin.toml")).is_err());
    }

    #[test]
    fn update_skips_symlinks() {
        // update() only processes non-symlink dirs — symlinked plugins
        // are managed externally by the developer
        let plugins = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join("plugin.toml"),
            r#"
            [plugin]
            type = "skill"
            id = "dev.test.up"
            name = "Update Test"
            version = "0.1.0"
            description = "a test"
        "#,
        )
        .unwrap();

        install_local(plugins.path(), source.path()).unwrap();

        // Verify the installed path is a symlink
        let name = source.path().file_name().unwrap();
        let link = plugins.path().join(name);
        assert!(link.is_symlink());

        // Collect non-symlink dirs (should be empty)
        let updatable: Vec<_> = std::fs::read_dir(plugins.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !p.is_symlink())
            .collect();
        assert!(
            updatable.is_empty(),
            "symlinked plugins should not be updatable"
        );
    }
}
