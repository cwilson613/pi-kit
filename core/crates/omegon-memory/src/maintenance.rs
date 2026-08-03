use serde::{Deserialize, Serialize};

use crate::backend::{MemoryBackend, Result};
use crate::decay::{AMBIENT_CONFIDENCE_FLOOR, effective_confidence};
use crate::types::{Fact, FactFilter, FactStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DormancyCandidate {
    pub fact_id: String,
    pub mind: String,
    pub effective_confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DormancyPlan {
    pub inspected_active: usize,
    pub candidates: Vec<DormancyCandidate>,
}

impl DormancyPlan {
    pub fn candidate_ids(&self) -> Vec<&str> {
        self.candidates
            .iter()
            .map(|candidate| candidate.fact_id.as_str())
            .collect()
    }
}

pub async fn plan_dormancy(backend: &dyn MemoryBackend, minds: &[String]) -> Result<DormancyPlan> {
    let mut facts = Vec::new();
    for mind in minds {
        facts.extend(
            backend
                .list_facts(
                    mind,
                    FactFilter {
                        status: Some(FactStatus::Active),
                        ..FactFilter::default()
                    },
                )
                .await?,
        );
    }
    plan_dormancy_for_facts(facts)
}

pub fn plan_dormancy_for_facts(facts: Vec<Fact>) -> Result<DormancyPlan> {
    let inspected_active = facts.len();
    let mut candidates = Vec::new();
    for fact in facts {
        if fact.status != FactStatus::Active {
            continue;
        }
        let confidence = effective_confidence(&fact);
        if confidence < AMBIENT_CONFIDENCE_FLOOR {
            candidates.push(DormancyCandidate {
                fact_id: fact.id,
                mind: fact.mind,
                effective_confidence: confidence,
                reason: format!(
                    "effective confidence {confidence:.6} is below ambient floor {AMBIENT_CONFIDENCE_FLOOR:.6}"
                ),
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.effective_confidence
            .total_cmp(&right.effective_confidence)
            .then_with(|| left.mind.cmp(&right.mind))
            .then_with(|| left.fact_id.cmp(&right.fact_id))
    });
    Ok(DormancyPlan {
        inspected_active,
        candidates,
    })
}

pub async fn apply_dormancy_plan(
    backend: &dyn MemoryBackend,
    plan: &DormancyPlan,
) -> Result<usize> {
    backend.dormancy_facts(&plan.candidate_ids()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inmemory::InMemoryBackend;
    use crate::types::{DecayProfileName, Section, StoreFact};

    async fn store(backend: &InMemoryBackend, content: &str) -> String {
        backend
            .store_fact(StoreFact {
                mind: "project".into(),
                section: Section::Architecture,
                content: content.into(),
                source: Some("test".into()),
                decay_profile: DecayProfileName::Standard,
            })
            .await
            .unwrap()
            .fact
            .id
    }

    #[tokio::test]
    async fn dry_run_is_deterministic_and_does_not_mutate() {
        let backend = InMemoryBackend::new();
        let stale = store(&backend, "stale").await;
        store(&backend, "healthy").await;
        let mut facts = backend
            .list_facts("project", FactFilter::default())
            .await
            .unwrap();
        facts
            .iter_mut()
            .find(|fact| fact.id == stale)
            .unwrap()
            .confidence = 0.01;

        let first = plan_dormancy_for_facts(facts.clone()).unwrap();
        let second = plan_dormancy_for_facts(facts).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.inspected_active, 2);
        assert_eq!(first.candidate_ids(), vec![stale.as_str()]);
        assert_eq!(
            backend
                .list_facts(
                    "project",
                    FactFilter {
                        status: Some(FactStatus::Active),
                        ..FactFilter::default()
                    }
                )
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn apply_only_transitions_planned_active_facts() {
        let backend = InMemoryBackend::new();
        let stale = store(&backend, "stale").await;
        let mut facts = backend
            .list_facts("project", FactFilter::default())
            .await
            .unwrap();
        facts[0].confidence = 0.01;
        let plan = plan_dormancy_for_facts(facts).unwrap();
        assert_eq!(apply_dormancy_plan(&backend, &plan).await.unwrap(), 1);
        assert_eq!(apply_dormancy_plan(&backend, &plan).await.unwrap(), 0);
        let dormant = backend
            .list_facts(
                "project",
                FactFilter {
                    status: Some(FactStatus::Dormant),
                    ..FactFilter::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(dormant[0].id, stale);
    }
}
