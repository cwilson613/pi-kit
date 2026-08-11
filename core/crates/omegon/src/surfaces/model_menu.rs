//! Renderer-neutral model-menu curation over the complete model catalog.

use crate::model_catalog::{ModelCatalog, ModelInfo};
use crate::model_preferences::ModelMenuPreferences;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMenuProjection {
    pub current_route: String,
    pub favorite_groups: Vec<ModelProviderGroupProjection>,
    pub providers: Vec<ModelProviderSummaryProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProviderSummaryProjection {
    pub provider_id: String,
    pub display_name: String,
    pub model_count: usize,
    pub favorite_count: usize,
    pub freshness: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProviderGroupProjection {
    pub provider_id: String,
    pub display_name: String,
    pub models: Vec<ModelRouteProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRouteProjection {
    pub route_id: String,
    pub provider_id: String,
    pub display_name: String,
    pub description: String,
    pub context_input: usize,
    pub capabilities: Vec<String>,
    pub favorite: bool,
    pub seeded: bool,
    pub current: bool,
    pub selectable: bool,
}

pub fn provider_id_from_route(route_id: &str) -> &str {
    route_id
        .split_once(':')
        .map_or(route_id, |(provider, _)| provider)
}

pub fn provider_seeds(provider_id: &str) -> &'static [&'static str] {
    match provider_id {
        "anthropic" => &[
            "claude-fable-5",
            "claude-sonnet-5",
            "claude-haiku-4-5-20251001",
        ],
        "openai" => &["gpt-5.6", "gpt-5-mini", "gpt-4.1"],
        "openai-codex" => &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"],
        "github-copilot" => &["gpt-5.6-sol", "claude-fable-5", "gpt-5.4-mini"],
        "ollama-cloud" => &["gpt-oss:120b", "qwen3.5:397b", "kimi-k3"],
        "moonshot" => &["kimi-k3", "kimi-k2.7-code", "kimi-k2.6"],
        "openrouter" => &[
            "anthropic/claude-sonnet-4-7",
            "deepseek/deepseek-chat",
            "minimax/minimax-m2.7",
        ],
        "google" | "gemini-openai" => &[
            "gemini-3.1-pro-preview",
            "gemini-2.5-flash",
            "gemini-2.0-flash-lite",
        ],
        "mistral" => &["mistral-large-latest", "mistral-small-latest"],
        "groq" => &["llama-3.3-70b-versatile"],
        "xai" => &["grok-4-0709", "grok-3"],
        _ => &[],
    }
}

pub fn project_model_menu(
    catalog: &ModelCatalog,
    preferences: &ModelMenuPreferences,
    current_route: &str,
) -> ModelMenuProjection {
    let inventories = provider_inventories(catalog, preferences, current_route);
    let favorite_groups = inventories
        .iter()
        .filter_map(|(provider_id, group)| {
            let explicit = preferences.favorites_for(provider_id);
            let available_ids: BTreeSet<&str> = group
                .models
                .iter()
                .map(|model| model.route_id.as_str())
                .collect();
            let mut selected: BTreeSet<String> = match explicit {
                Some(favorites) => favorites.clone(),
                None => provider_seeds(provider_id)
                    .iter()
                    .map(|id| format!("{provider_id}:{id}"))
                    .filter(|route| available_ids.contains(route.as_str()))
                    .collect(),
            };
            if explicit.is_none() && selected.len() < 3 {
                for model in &group.models {
                    selected.insert(model.route_id.clone());
                    if selected.len() == 3 {
                        break;
                    }
                }
            }
            if group.models.iter().any(|model| model.current) {
                selected.insert(current_route.to_string());
            }
            let models: Vec<_> = group
                .models
                .iter()
                .filter(|model| selected.contains(&model.route_id))
                .cloned()
                .collect();
            (!models.is_empty()).then(|| ModelProviderGroupProjection {
                provider_id: provider_id.clone(),
                display_name: group.display_name.clone(),
                models,
            })
        })
        .collect();

    let providers = inventories
        .into_iter()
        .map(|(provider_id, group)| ModelProviderSummaryProjection {
            favorite_count: group.models.iter().filter(|model| model.favorite).count(),
            model_count: group.models.len(),
            freshness: catalog.freshness.get(&group.display_name).cloned(),
            provider_id,
            display_name: group.display_name,
        })
        .collect();

    ModelMenuProjection {
        current_route: current_route.to_string(),
        favorite_groups,
        providers,
    }
}

pub fn project_provider_inventory(
    catalog: &ModelCatalog,
    preferences: &ModelMenuPreferences,
    current_route: &str,
    provider_id: &str,
) -> Option<ModelProviderGroupProjection> {
    provider_inventories(catalog, preferences, current_route).remove(provider_id)
}

fn provider_inventories(
    catalog: &ModelCatalog,
    preferences: &ModelMenuPreferences,
    current_route: &str,
) -> BTreeMap<String, ModelProviderGroupProjection> {
    let mut groups = BTreeMap::new();
    for (display_name, models) in &catalog.providers {
        let Some(first) = models.first() else {
            continue;
        };
        let provider_id = provider_id_from_route(&first.id).to_string();
        let explicit = preferences.favorites_for(&provider_id);
        let seeds: BTreeSet<String> = provider_seeds(&provider_id)
            .iter()
            .map(|id| format!("{provider_id}:{id}"))
            .collect();
        let mut projected: Vec<_> = models
            .iter()
            .map(|model| project_route(model, &provider_id, explicit, &seeds, current_route))
            .collect();
        projected.sort_by(|left, right| {
            right
                .current
                .cmp(&left.current)
                .then_with(|| left.display_name.cmp(&right.display_name))
                .then_with(|| left.route_id.cmp(&right.route_id))
        });
        groups.insert(
            provider_id.clone(),
            ModelProviderGroupProjection {
                provider_id,
                display_name: display_name.clone(),
                models: projected,
            },
        );
    }
    groups
}

fn project_route(
    model: &ModelInfo,
    provider_id: &str,
    explicit: Option<&BTreeSet<String>>,
    seeds: &BTreeSet<String>,
    current_route: &str,
) -> ModelRouteProjection {
    ModelRouteProjection {
        route_id: model.id.clone(),
        provider_id: provider_id.to_string(),
        display_name: model.name.clone(),
        description: model.description.clone(),
        context_input: model.context_input,
        capabilities: model
            .capabilities
            .iter()
            .map(|cap| cap.as_str().to_string())
            .collect(),
        favorite: explicit.is_some_and(|favorites| favorites.contains(&model.id)),
        seeded: explicit.is_none() && seeds.contains(&model.id),
        current: model.id == current_route,
        selectable: model.available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_inventory::ModelAdmissionStatus;
    use crate::model_catalog::Capability;

    fn model(id: &str, name: &str, provider: &str) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: name.into(),
            provider: provider.into(),
            context_input: 128_000,
            context_output: 16_000,
            capabilities: vec![Capability::Coding],
            description: "test".into(),
            available: true,
            admission: ModelAdmissionStatus::Curated,
            context_pricing_notice: None,
            conceptual_model_id: None,
            producer: None,
            execution_class: None,
        }
    }

    #[test]
    fn shortlist_preserves_full_inventory_and_uses_seeds() {
        let catalog = ModelCatalog {
            providers: BTreeMap::from([(
                "Ollama Cloud".into(),
                vec![
                    model("ollama-cloud:gpt-oss:120b", "GPT OSS", "Ollama Cloud"),
                    model("ollama-cloud:qwen3.5:397b", "Qwen", "Ollama Cloud"),
                    model("ollama-cloud:kimi-k3", "Kimi", "Ollama Cloud"),
                    model("ollama-cloud:deepseek-v4-pro", "DeepSeek", "Ollama Cloud"),
                ],
            )]),
            freshness: BTreeMap::new(),
        };
        let preferences = ModelMenuPreferences::default();
        let projection = project_model_menu(&catalog, &preferences, "ollama-cloud:gpt-oss:120b");
        assert_eq!(projection.providers[0].model_count, 4);
        assert_eq!(projection.favorite_groups[0].models.len(), 3);
        let full = project_provider_inventory(&catalog, &preferences, "", "ollama-cloud").unwrap();
        assert_eq!(full.models.len(), 4);
    }

    #[test]
    fn explicit_favorites_replace_seeds() {
        let catalog = ModelCatalog {
            providers: BTreeMap::from([(
                "Ollama Cloud".into(),
                vec![
                    model("ollama-cloud:gpt-oss:120b", "GPT OSS", "Ollama Cloud"),
                    model("ollama-cloud:deepseek-v4-pro", "DeepSeek", "Ollama Cloud"),
                ],
            )]),
            freshness: BTreeMap::new(),
        };
        let mut preferences = ModelMenuPreferences::default();
        preferences.toggle("ollama-cloud:deepseek-v4-pro").unwrap();
        let projection = project_model_menu(&catalog, &preferences, "");
        assert_eq!(projection.favorite_groups[0].models.len(), 1);
        assert_eq!(
            projection.favorite_groups[0].models[0].route_id,
            "ollama-cloud:deepseek-v4-pro"
        );
    }
}
