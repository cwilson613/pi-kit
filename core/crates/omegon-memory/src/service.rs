//! Memory Mind service policy layer.
//!
//! This module holds reusable memory-domain behavior that should not depend on
//! Omegon's harness/tool adapter. Keep provider calls, ToolResult formatting,
//! and context-injection TTL policy outside this layer.

use std::sync::Arc;

use crate::{MemoryBackend, ScoredFact};

/// Reusable semantic-memory service over a [`MemoryBackend`].
pub struct MemoryMindService {
    backend: Arc<dyn MemoryBackend>,
    mind: String,
}

impl MemoryMindService {
    pub fn new(backend: Arc<dyn MemoryBackend>, mind: impl Into<String>) -> Self {
        Self {
            backend,
            mind: mind.into(),
        }
    }

    /// 1-hop edge expansion for recall results.
    ///
    /// For each seed fact, fetch edges, load neighbor facts, and score each
    /// neighbor as `parent_score × edge.confidence × 0.5`. Seed facts are not
    /// duplicated. The result is sorted by derived score and truncated to
    /// `limit`.
    pub async fn expand_edges(&self, results: Vec<ScoredFact>, limit: usize) -> Vec<ScoredFact> {
        expand_edges(self.backend.as_ref(), &self.mind, results, limit).await
    }
}

pub async fn expand_edges(
    backend: &dyn MemoryBackend,
    mind: &str,
    results: Vec<ScoredFact>,
    limit: usize,
) -> Vec<ScoredFact> {
    expand_edges_cancellable(backend, mind, results.clone(), limit, &|| false)
        .await
        .unwrap_or(results)
}

pub async fn expand_edges_cancellable(
    backend: &dyn MemoryBackend,
    mind: &str,
    mut results: Vec<ScoredFact>,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Option<Vec<ScoredFact>> {
    use std::collections::{BTreeMap, HashSet};

    const MAX_SEEDS: usize = 1_000;
    const MAX_EDGES_PER_SEED: usize = 64;
    const MAX_NEIGHBOR_LOADS: usize = 4_096;

    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.fact.id.cmp(&right.fact.id))
    });
    let seeds: HashSet<String> = results
        .iter()
        .map(|result| result.fact.id.clone())
        .collect();
    let mut candidates = BTreeMap::<String, f64>::new();
    for result in results.iter().take(MAX_SEEDS) {
        if cancelled() {
            return None;
        }
        let mut edges = match backend.get_edges(mind, &result.fact.id).await {
            Ok(edges) => edges,
            Err(e) => {
                tracing::debug!(fact_id = %result.fact.id, error = %e, "edge lookup failed");
                continue;
            }
        };
        edges.sort_by(|left, right| {
            right
                .confidence
                .partial_cmp(&left.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        for edge in edges.into_iter().take(MAX_EDGES_PER_SEED) {
            let neighbor_id = if edge.source_id == result.fact.id {
                edge.target_id
            } else {
                edge.source_id
            };
            if seeds.contains(&neighbor_id) {
                continue;
            }
            let score = result.score * edge.confidence * 0.5;
            candidates
                .entry(neighbor_id)
                .and_modify(|existing| *existing = existing.max(score))
                .or_insert(score);
        }
    }
    for (neighbor_id, score) in candidates.into_iter().take(MAX_NEIGHBOR_LOADS) {
        if cancelled() {
            return None;
        }
        if let Ok(Some(fact)) = backend.get_fact(&neighbor_id).await {
            results.push(ScoredFact {
                similarity: score,
                score,
                fact,
            });
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.fact.id.cmp(&b.fact.id))
    });
    results.truncate(limit);
    Some(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateEdge, DecayProfileName, InMemoryBackend, Section, StoreFact};

    async fn store(backend: &Arc<dyn MemoryBackend>, mind: &str, content: &str) -> ScoredFact {
        let result = backend
            .store_fact(StoreFact {
                mind: mind.to_string(),
                content: content.to_string(),
                section: Section::Architecture,
                source: Some("test".into()),
                decay_profile: DecayProfileName::Standard,
            })
            .await
            .unwrap();
        ScoredFact {
            similarity: 1.0,
            score: 1.0,
            fact: result.fact,
        }
    }

    #[tokio::test]
    async fn edge_expansion_adds_scored_neighbors_without_duplicates() {
        let backend: Arc<dyn MemoryBackend> = Arc::new(InMemoryBackend::new());
        let a = store(&backend, "default", "Fact A about routing boundaries").await;
        let b = store(&backend, "default", "Fact B about adapter boundaries").await;

        backend
            .create_edge(CreateEdge {
                source_id: a.fact.id.clone(),
                target_id: b.fact.id.clone(),
                relation: "related".into(),
                description: None,
            })
            .await
            .unwrap();

        let expanded = expand_edges(backend.as_ref(), "default", vec![a.clone()], 10).await;
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].fact.id, a.fact.id);
        assert_eq!(expanded[1].fact.id, b.fact.id);
        assert!((expanded[1].score - 0.5).abs() < f64::EPSILON);

        let expanded_again = expand_edges(backend.as_ref(), "default", expanded, 10).await;
        let b_count = expanded_again
            .iter()
            .filter(|fact| fact.fact.id == b.fact.id)
            .count();
        assert_eq!(b_count, 1);
    }

    #[tokio::test]
    async fn edge_expansion_respects_limit() {
        let backend: Arc<dyn MemoryBackend> = Arc::new(InMemoryBackend::new());
        let a = store(&backend, "default", "Fact A about context").await;
        let b = store(&backend, "default", "Fact B about context").await;

        backend
            .create_edge(CreateEdge {
                source_id: a.fact.id.clone(),
                target_id: b.fact.id.clone(),
                relation: "related".into(),
                description: None,
            })
            .await
            .unwrap();

        let expanded = expand_edges(backend.as_ref(), "default", vec![a], 1).await;
        assert_eq!(expanded.len(), 1);
    }

    #[tokio::test]
    async fn edge_expansion_is_cancellable_and_ties_are_fact_id_ordered() {
        let backend: Arc<dyn MemoryBackend> = Arc::new(InMemoryBackend::new());
        let seed = store(&backend, "default", "seed").await;
        let left = store(&backend, "default", "left").await;
        let right = store(&backend, "default", "right").await;
        for neighbor in [&right, &left] {
            backend
                .create_edge(CreateEdge {
                    source_id: seed.fact.id.clone(),
                    target_id: neighbor.fact.id.clone(),
                    relation: "related".into(),
                    description: None,
                })
                .await
                .unwrap();
        }
        assert!(
            expand_edges_cancellable(backend.as_ref(), "default", vec![seed.clone()], 10, &|| {
                true
            })
            .await
            .is_none()
        );
        let expanded = expand_edges(backend.as_ref(), "default", vec![seed], 10).await;
        let actual = expanded[1..]
            .iter()
            .map(|result| result.fact.id.as_str())
            .collect::<Vec<_>>();
        let mut expected = vec![left.fact.id.as_str(), right.fact.id.as_str()];
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }
}
