//! Persistent operator preferences for model-menu curation.
//!
//! Favorites are concrete provider route IDs. They are global navigation state,
//! not project routing policy, so they live under the user configuration dir.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelPreferences {
    #[serde(default)]
    pub customized: bool,
    #[serde(default)]
    pub favorites: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMenuPreferences {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderModelPreferences>,
}

const fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for ModelMenuPreferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            providers: BTreeMap::new(),
        }
    }
}

impl ModelMenuPreferences {
    pub fn load_default() -> Self {
        Self::load(&default_path()).unwrap_or_default()
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save_default(&self) -> anyhow::Result<()> {
        self.save(&default_path())
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temporary)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn favorites_for(&self, provider_id: &str) -> Option<&BTreeSet<String>> {
        self.providers
            .get(provider_id)
            .filter(|preferences| preferences.customized)
            .map(|preferences| &preferences.favorites)
    }

    pub fn toggle(&mut self, route_id: &str) -> anyhow::Result<bool> {
        let (provider_id, _) = route_id
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("model route must use provider:model-id form"))?;
        let preferences = self.providers.entry(provider_id.to_string()).or_default();
        preferences.customized = true;
        let favorite = if preferences.favorites.remove(route_id) {
            false
        } else {
            preferences.favorites.insert(route_id.to_string());
            true
        };
        Ok(favorite)
    }
}

pub fn default_path() -> PathBuf {
    crate::paths::user_config_dir().join("model-preferences.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_is_provider_scoped_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model-preferences.json");
        let mut preferences = ModelMenuPreferences::default();
        assert!(preferences.toggle("ollama-cloud:qwen3.5:397b").unwrap());
        assert!(!preferences.toggle("ollama-cloud:qwen3.5:397b").unwrap());
        assert!(preferences.toggle("moonshot:kimi-k3").unwrap());
        preferences.save(&path).unwrap();

        let loaded = ModelMenuPreferences::load(&path).unwrap();
        assert_eq!(loaded, preferences);
        assert!(loaded.favorites_for("ollama-cloud").unwrap().is_empty());
        assert!(
            loaded
                .favorites_for("moonshot")
                .unwrap()
                .contains("moonshot:kimi-k3")
        );
    }

    #[test]
    fn invalid_route_is_rejected_without_mutation() {
        let mut preferences = ModelMenuPreferences::default();
        assert!(preferences.toggle("missing-provider-prefix").is_err());
        assert!(preferences.providers.is_empty());
    }
}
