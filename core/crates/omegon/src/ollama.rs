//! OllamaManager — structured access to a local Ollama server.
//!
//! Provides model listing, running-model queries, reachability checks,
//! and hardware profile estimation.

use anyhow::Result;
use serde::Deserialize;

/// Manages interaction with a local Ollama server.
#[derive(Debug, Clone)]
pub struct OllamaManager {
    host: String,
    client: reqwest::Client,
}

/// A model available in Ollama.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub digest: String,
}

/// A model currently loaded in VRAM.
#[derive(Debug, Clone, Deserialize)]
pub struct RunningModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
}

/// System hardware profile for model sizing recommendations.
#[derive(Debug, Clone)]
pub struct HardwareProfile {
    pub total_memory_bytes: u64,
    pub estimated_vram_bytes: u64,
    pub recommended_max_params: &'static str,
}

impl OllamaManager {
    /// Create a new OllamaManager, reading OLLAMA_HOST or defaulting to localhost:11434.
    pub fn new() -> Self {
        let host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(300))
            .build()
            .unwrap_or_default();
        Self { host, client }
    }

    /// Check if Ollama is reachable (200ms timeout).
    pub async fn is_reachable(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.host))
            .timeout(std::time::Duration::from_millis(200))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List all available models.
    pub async fn list_models(&self) -> Result<Vec<OllamaModel>> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.host))
            .send()
            .await?;
        let body: TagsResponse = resp.json().await?;
        Ok(body.models)
    }

    /// List models currently loaded in VRAM.
    pub async fn list_running(&self) -> Result<Vec<RunningModel>> {
        let resp = self
            .client
            .get(format!("{}/api/ps", self.host))
            .send()
            .await?;
        let body: PsResponse = resp.json().await?;
        Ok(body.models)
    }

    /// Estimate hardware profile for model sizing.
    pub fn hardware_profile() -> HardwareProfile {
        let total = sysinfo::System::new_all().total_memory();
        // On Apple Silicon, VRAM ≈ total (unified memory)
        let is_apple_silicon = cfg!(target_arch = "aarch64") && cfg!(target_os = "macos");
        let vram = if is_apple_silicon { total } else { total / 4 };
        let recommended = match vram {
            0..=8_000_000_000 => "7B",
            8_000_000_001..=16_000_000_000 => "14B",
            16_000_000_001..=32_000_000_000 => "32B",
            32_000_000_001..=64_000_000_000 => "70B",
            _ => "100B+",
        };
        HardwareProfile {
            total_memory_bytes: total,
            estimated_vram_bytes: vram,
            recommended_max_params: recommended,
        }
    }
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct PsResponse {
    #[serde(default)]
    models: Vec<RunningModel>,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tags_response() {
        let json = r#"{"models":[{"name":"qwen3:32b","size":19000000000,"digest":"abc123"}]}"#;
        let resp: TagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].name, "qwen3:32b");
        assert_eq!(resp.models[0].size, 19000000000);
    }

    #[test]
    fn test_parse_ps_response() {
        let json =
            r#"{"models":[{"name":"qwen3:32b","size":19000000000,"size_vram":19000000000}]}"#;
        let resp: PsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 1);
        assert_eq!(resp.models[0].name, "qwen3:32b");
        assert_eq!(resp.models[0].size_vram, 19000000000);
    }

    #[test]
    fn test_parse_empty_responses() {
        let json = r#"{"models":[]}"#;
        let tags: TagsResponse = serde_json::from_str(json).unwrap();
        assert!(tags.models.is_empty());
        let ps: PsResponse = serde_json::from_str(json).unwrap();
        assert!(ps.models.is_empty());
    }

    #[test]
    fn test_hardware_profile_nonzero() {
        let profile = OllamaManager::hardware_profile();
        assert!(profile.total_memory_bytes > 0);
        assert!(profile.estimated_vram_bytes > 0);
        assert!(!profile.recommended_max_params.is_empty());
    }

    #[test]
    fn test_ollama_manager_new() {
        let mgr = OllamaManager::new();
        // Should have a valid host
        assert!(mgr.host.starts_with("http"));
    }
}
