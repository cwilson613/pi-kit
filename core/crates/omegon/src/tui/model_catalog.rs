//! Unified model catalog — cloud and local inference models with metadata.
//!
//! This is the single source of truth for all available models across providers.
//! It supports:
//! - Dynamic discovery (Ollama, OpenRouter live queries)
//! - Static fallback (hardcoded model lists for known providers)
//! - Symmetric representation: cloud and local models are peers
//! - Context limits, capability tags, cost tier, hardware requirements

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TokenPricing {
    /// USD per 1M input/prompt tokens.
    pub input_per_million_usd: f64,
    /// USD per 1M output/completion tokens.
    pub output_per_million_usd: f64,
}

impl TokenPricing {
    pub const fn new(input_per_million_usd: f64, output_per_million_usd: f64) -> Self {
        Self {
            input_per_million_usd,
            output_per_million_usd,
        }
    }

    pub fn estimate_cost_usd(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        (input_tokens as f64 / 1_000_000.0) * self.input_per_million_usd
            + (output_tokens as f64 / 1_000_000.0) * self.output_per_million_usd
    }
}

/// A model's availability tier and cost characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CostTier {
    /// Free tier (no charge, rate-limited)
    Free,
    /// Pay-per-token, low cost (<$1/M input tokens)
    CheapAPI,
    /// Pay-per-token, standard cost ($1-10/M)
    StandardAPI,
    /// Premium models ($10+/M)
    Premium,
    /// Local inference (no API cost)
    Local,
}

impl CostTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::CheapAPI => "cheap",
            Self::StandardAPI => "standard",
            Self::Premium => "premium",
            Self::Local => "local",
        }
    }
}

/// Capability tags for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Reasoning,    // Extended thinking, CoT-optimized
    Coding,       // Good at code generation
    Fast,         // Low latency responses
    Vision,       // Can process images
    Instruction,  // Instruction-following optimized
    Multilingual, // Strong across languages
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reasoning => "reasoning",
            Self::Coding => "coding",
            Self::Fast => "fast",
            Self::Vision => "vision",
            Self::Instruction => "instruction",
            Self::Multilingual => "multilingual",
        }
    }
}

/// A single model's metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// `provider:model-id` (e.g. `openrouter:qwen/qwen-qwq-32b`)
    pub id: String,
    /// Human-readable name (e.g. "Qwen QwQ 32B")
    pub name: String,
    /// Provider name (e.g. "OpenRouter", "Ollama", "Anthropic")
    pub provider: String,
    /// Max input tokens
    pub context_input: usize,
    /// Max output tokens
    pub context_output: usize,
    /// Cost tier
    pub cost_tier: CostTier,
    /// Explicit token pricing when known. This is the authoritative source for
    /// footer/session cost calculations; `cost_tier` is only a coarse UX bucket.
    pub pricing: Option<TokenPricing>,
    /// Capability tags
    pub capabilities: Vec<Capability>,
    /// Brief description
    pub description: String,
    /// Whether it's available (authenticated, installed, etc.)
    pub available: bool,
}

impl ModelInfo {
    /// Format context window as "200k in / 8k out"
    pub fn context_str(&self) -> String {
        format!(
            "{}k in / {}k out",
            self.context_input / 1000,
            self.context_output / 1000
        )
    }

    /// Format capabilities as comma-separated tags
    pub fn capability_str(&self) -> String {
        self.capabilities
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Model catalog — grouped by provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalog {
    /// Models keyed by provider, then by model ID
    pub providers: BTreeMap<String, Vec<ModelInfo>>,
}

impl ModelCatalog {
    pub fn pricing_for_model(model_id: &str) -> Option<TokenPricing> {
        // Local models are always free
        if model_id.starts_with("ollama:") {
            return Some(TokenPricing::new(0.0, 0.0));
        }
        // Look up in the centralized model registry
        let reg = crate::model_registry::ModelRegistry::global();
        reg.pricing(model_id)
            .map(|p| TokenPricing::new(p.input, p.output))
    }

    pub fn find_by_id(&self, model_id: &str) -> Option<&ModelInfo> {
        self.providers
            .values()
            .flat_map(|models| models.iter())
            .find(|model| model.id == model_id)
    }

    /// Discover the live model catalog.
    ///
    /// - Ollama section is populated by running `ollama list` — only models
    ///   actually installed on this machine appear.
    /// - Cloud provider sections are included only when a valid API key or
    ///   OAuth token can be resolved for that provider.
    pub fn discover() -> Self {
        let mut cat = Self::cloud_only();

        // Populate Ollama from live `ollama list`
        let ollama_models = Self::query_ollama();
        if !ollama_models.is_empty() {
            cat.providers.insert("Ollama".to_string(), ollama_models);
        }

        cat
    }

    /// Query `ollama list` and parse installed models into ModelInfo entries.
    fn query_ollama() -> Vec<ModelInfo> {
        let output = std::process::Command::new("ollama").arg("list").output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return vec![],
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut models = Vec::new();
        for line in stdout.lines().skip(1) {
            // Format: "NAME   ID   SIZE   MODIFIED"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            let full_name = parts[0]; // e.g. "glm-4.7-flash:latest"
            // Title-case display name from the name portion (before the colon tag)
            let raw = full_name.split(':').next().unwrap_or(full_name);
            let display_name = raw
                .replace('-', " ")
                .split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let size_str = parts.get(2).copied().unwrap_or("");
            let description = if size_str.is_empty() {
                format!("Ollama: {full_name}")
            } else {
                format!("Ollama: {full_name} ({size_str})")
            };
            models.push(ModelInfo {
                id: format!("ollama:{full_name}"),
                name: display_name,
                provider: "Ollama".to_string(),
                context_input: 128_000,
                context_output: 32_768,
                cost_tier: CostTier::Local,
                pricing: Some(TokenPricing::new(0.0, 0.0)),
                capabilities: vec![Capability::Instruction, Capability::Coding],
                description,
                available: true,
            });
        }
        models
    }

    /// Cloud-only catalog — includes a provider section only when an API key
    /// is present for it.  Call `discover()` for the full live catalog.
    pub fn cloud_only() -> Self {
        let mut providers = BTreeMap::new();

        fn has_key(provider: &str) -> bool {
            crate::providers::resolve_api_key_sync(provider).is_some()
        }

        // ─── Cloud Providers (from model registry, auth-gated) ────────
        let reg = crate::model_registry::ModelRegistry::global();
        let provider_display: &[(&str, &str)] = &[
            ("anthropic", "Anthropic"),
            ("openai", "OpenAI"),
            ("openai-codex", "OpenAI Codex"),
            ("ollama-cloud", "Ollama Cloud"),
            ("groq", "Groq"),
            ("xai", "xAI"),
            ("mistral", "Mistral"),
            ("google", "Google Gemini"),
            ("openrouter", "OpenRouter"),
        ];

        for &(provider_id, display_name) in provider_display {
            if !has_key(provider_id) {
                continue;
            }
            let models: Vec<ModelInfo> = reg
                .models_for_provider(provider_id)
                .into_iter()
                .map(|m| {
                    let cost_tier = match m.cost_tier.as_str() {
                        "premium" => CostTier::Premium,
                        "standard" => CostTier::StandardAPI,
                        "cheap" => CostTier::CheapAPI,
                        "free" => CostTier::Free,
                        "local" => CostTier::Local,
                        _ => CostTier::StandardAPI,
                    };
                    let capabilities = m
                        .capabilities
                        .iter()
                        .filter_map(|c| match c.as_str() {
                            "reasoning" => Some(Capability::Reasoning),
                            "coding" => Some(Capability::Coding),
                            "vision" => Some(Capability::Vision),
                            "fast" => Some(Capability::Fast),
                            "instruction" => Some(Capability::Instruction),
                            "multilingual" => Some(Capability::Multilingual),
                            _ => None,
                        })
                        .collect();
                    let pricing = m.pricing.map(|p| TokenPricing::new(p.input, p.output));
                    ModelInfo {
                        id: format!("{}:{}", provider_id, m.id),
                        name: m.name.clone(),
                        provider: display_name.to_string(),
                        context_input: m.context_input,
                        context_output: m.context_output,
                        cost_tier,
                        pricing,
                        capabilities,
                        description: m.description.clone(),
                        available: true,
                    }
                })
                .collect();
            if !models.is_empty() {
                providers.insert(display_name.to_string(), models);
            }
        }

        // Google Antigravity (show as unavailable if no API key)
        if has_key("google-antigravity") && !has_key("google") {
            let models: Vec<ModelInfo> = reg
                .models_for_provider("google")
                .into_iter()
                .map(|m| ModelInfo {
                    id: format!("google-antigravity:{}", m.id),
                    name: m.name.clone(),
                    provider: "Google Antigravity".to_string(),
                    context_input: m.context_input,
                    context_output: m.context_output,
                    cost_tier: CostTier::Free,
                    pricing: Some(TokenPricing::new(0.0, 0.0)),
                    capabilities: m
                        .capabilities
                        .iter()
                        .filter_map(|c| match c.as_str() {
                            "reasoning" => Some(Capability::Reasoning),
                            "coding" => Some(Capability::Coding),
                            "vision" => Some(Capability::Vision),
                            "fast" => Some(Capability::Fast),
                            "instruction" => Some(Capability::Instruction),
                            "multilingual" => Some(Capability::Multilingual),
                            _ => None,
                        })
                        .collect(),
                    description: format!("{} via Antigravity subscription", m.name),
                    available: false,
                })
                .collect();
            if !models.is_empty() {
                providers.insert(
                    "Google Antigravity (use GOOGLE_API_KEY instead)".to_string(),
                    models,
                );
            }
        }

        ModelCatalog { providers }
    }

    // ── Legacy hardcoded blocks removed — all model data now comes from
    // data/model-registry.json via crate::model_registry::ModelRegistry.
    // To add a model, edit the JSON file. Zero Rust changes required. ──

    #[allow(dead_code)]
    fn _removed_hardcoded_blocks() {
        // This marker exists so git blame shows when the migration happened.
        // The following providers were migrated:
        // OpenRouter, Anthropic, OpenAI, Ollama Cloud, Groq, xAI, Mistral,
        // Google Gemini, Google Antigravity, OpenAI Codex
        unreachable!();
    }

    // NOTE: ~500 lines of hardcoded ModelInfo structs were here.
    // They have been replaced by the registry-driven loop above.

    /// Alias for `cloud_only()`.
    pub fn new() -> Self {
        Self::cloud_only()
    }

    /// Get all models, flattened and optionally filtered by provider.
    pub fn all_models(&self) -> Vec<&ModelInfo> {
        self.providers
            .values()
            .flat_map(|models| models.iter())
            .collect()
    }

    /// Filter models by search term (name or description).
    pub fn search(&self, query: &str) -> Vec<&ModelInfo> {
        let q = query.to_lowercase();
        self.all_models()
            .into_iter()
            .filter(|m| {
                m.name.to_lowercase().contains(&q)
                    || m.id.to_lowercase().contains(&q)
                    || m.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Filter by capability.
    pub fn by_capability(&self, cap: Capability) -> Vec<&ModelInfo> {
        self.all_models()
            .into_iter()
            .filter(|m| m.capabilities.contains(&cap))
            .collect()
    }

    /// Filter by provider name.
    pub fn by_provider(&self, provider: &str) -> Vec<&ModelInfo> {
        self.providers
            .get(provider)
            .map(|models| models.iter().collect())
            .unwrap_or_default()
    }

    /// Get models available for immediate use (authenticated, installed).
    pub fn available(&self) -> Vec<&ModelInfo> {
        self.all_models()
            .into_iter()
            .filter(|m| m.available)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_cloud_providers() {
        // `new()` / `cloud_only()` returns cloud providers gated by key availability.
        // Ollama is not present here — it only appears via `discover()` at runtime.
        let cat = ModelCatalog::new();
        // At minimum the catalog is buildable and the map is populated (may be empty
        // in CI where no API keys are configured — that is correct behavior).
        let _ = cat.providers; // structural smoke test
    }

    #[test]
    fn discover_smoke() {
        // discover() runs `ollama list`; may return empty in CI — just must not panic.
        let cat = ModelCatalog::discover();
        let _ = cat.providers;
    }

    fn fixture_catalog() -> ModelCatalog {
        let mut providers = std::collections::BTreeMap::new();
        providers.insert(
            "OpenRouter".to_string(),
            vec![ModelInfo {
                id: "openrouter:qwen/qwen-qwq-32b".to_string(),
                name: "Qwen QwQ 32B".to_string(),
                provider: "OpenRouter".to_string(),
                context_input: 32768,
                context_output: 8192,
                cost_tier: CostTier::CheapAPI,
                pricing: Some(TokenPricing::new(0.20, 0.20)),
                capabilities: vec![Capability::Reasoning, Capability::Coding],
                description: "Qwen reasoning model".to_string(),
                available: true,
            }],
        );
        providers.insert(
            "Anthropic".to_string(),
            vec![ModelInfo {
                id: "anthropic:claude-sonnet-4-6".to_string(),
                name: "Claude Sonnet 4.6".to_string(),
                provider: "Anthropic".to_string(),
                context_input: 1_000_000,
                context_output: 65536,
                cost_tier: CostTier::StandardAPI,
                pricing: Some(TokenPricing::new(3.0, 15.0)),
                capabilities: vec![
                    Capability::Reasoning,
                    Capability::Coding,
                    Capability::Vision,
                ],
                description: "Claude Sonnet 4.6".to_string(),
                available: true,
            }],
        );
        ModelCatalog { providers }
    }

    #[test]
    fn search_finds_qwen() {
        let cat = fixture_catalog();
        let results = cat.search("qwen");
        assert!(!results.is_empty());
        assert!(results.iter().any(|m| m.name.contains("Qwen")));
    }

    #[test]
    fn by_capability_finds_reasoning() {
        let cat = fixture_catalog();
        let results = cat.by_capability(Capability::Reasoning);
        assert!(!results.is_empty());
    }

    #[test]
    fn context_str_formats_correctly() {
        let model = ModelInfo {
            id: "test:model".to_string(),
            name: "Test".to_string(),
            provider: "Test".to_string(),
            context_input: 128000,
            context_output: 8192,
            cost_tier: CostTier::Local,
            pricing: Some(TokenPricing::new(0.0, 0.0)),
            capabilities: vec![],
            description: "test".to_string(),
            available: true,
        };
        assert_eq!(model.context_str(), "128k in / 8k out");
    }

    #[test]
    fn pricing_estimates_cost() {
        let pricing = TokenPricing::new(3.0, 15.0);
        let usd = pricing.estimate_cost_usd(100_000, 20_000);
        assert!((usd - 0.6).abs() < 0.000_001, "got {usd}");
    }

    #[test]
    fn find_by_id_returns_model() {
        let cat = fixture_catalog();
        let model = cat.find_by_id("anthropic:claude-sonnet-4-6");
        assert!(model.is_some());
    }

    #[test]
    fn pricing_for_model_is_not_auth_gated() {
        let pricing = ModelCatalog::pricing_for_model("openai:gpt-5.4");
        assert_eq!(pricing, Some(TokenPricing::new(2.5, 15.0)));
    }

    #[test]
    fn pricing_for_ollama_cloud_is_defined() {
        let pricing = ModelCatalog::pricing_for_model("ollama-cloud:gpt-oss:120b-cloud");
        assert_eq!(pricing, Some(TokenPricing::new(0.0, 0.0)));
    }
}
