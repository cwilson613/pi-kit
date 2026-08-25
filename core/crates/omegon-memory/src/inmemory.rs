//! In-memory MemoryBackend — HashMap-based, no persistence.
//! Used for unit tests and ephemeral sessions.

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::backend::*;
use crate::hash;
use crate::types::*;
use crate::util::{gen_id, now_iso};
use crate::vectors;

#[derive(Clone)]
struct EmbeddingEntry {
    fact_id: String,
    model_name: String,
    embedding: Vec<f32>,
    inserted_at: String,
}

#[derive(Clone)]
struct State {
    facts: HashMap<String, Fact>,
    edges: Vec<Edge>,
    episodes: Vec<Episode>,
    embeddings: Vec<EmbeddingEntry>,
    version_clock: u64,
    operation_receipts: HashMap<String, (String, MemoryMutationEffect)>,
}

pub struct InMemoryBackend {
    state: Mutex<State>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                facts: HashMap::new(),
                edges: Vec::new(),
                episodes: Vec::new(),
                embeddings: Vec::new(),
                version_clock: 0,
                operation_receipts: HashMap::new(),
            }),
        }
    }

    fn check_fact_precondition(state: &State, fact: &FactPrecondition) -> Result<()> {
        let existing = state
            .facts
            .get(&fact.id)
            .ok_or_else(|| MemoryError::FactNotFound(fact.id.clone()))?;
        if existing.version != fact.expected_version {
            return Err(MemoryError::FactVersionConflict {
                id: fact.id.clone(),
                expected: fact.expected_version,
                actual: existing.version,
            });
        }
        Ok(())
    }

    fn next_version(state: &mut State) -> Result<u64> {
        let version = state.version_clock.checked_add(1).ok_or_else(|| {
            MemoryError::InvalidMutation("Lamport version space is exhausted".into())
        })?;
        persisted_lamport_version(version)?;
        state.version_clock = version;
        Ok(version)
    }

    fn apply_to_state(state: &mut State, mutation: MemoryMutation) -> Result<MemoryMutationEffect> {
        match mutation {
            MemoryMutation::ImportJsonl { jsonl } => {
                let stats = Self::import_jsonl_to_state(state, &jsonl)?;
                Ok(jsonl_import_effect(stats))
            }
            MemoryMutation::StoreFact { request } => {
                let content_hash = hash::content_hash(&request.content);
                let existing_id = state
                    .facts
                    .iter()
                    .find(|(_, fact)| {
                        fact.mind == request.mind
                            && fact.content_hash.as_deref() == Some(content_hash.as_str())
                            && fact.status == FactStatus::Active
                    })
                    .map(|(id, _)| id.clone());
                let version = Self::next_version(state)?;
                if let Some(fact_id) = existing_id {
                    let fact = state.facts.get_mut(&fact_id).ok_or_else(|| {
                        MemoryError::Storage(anyhow::anyhow!("deduplicated fact disappeared"))
                    })?;
                    fact.reinforcement_count += 1;
                    fact.last_reinforced = now_iso();
                    fact.version = version;
                    return Ok(MemoryMutationEffect::FactStored {
                        fact_id,
                        version,
                        action: StoreAction::Reinforced,
                    });
                }

                let fact_id = gen_id();
                let timestamp = now_iso();
                state.facts.insert(
                    fact_id.clone(),
                    Fact {
                        id: fact_id.clone(),
                        mind: request.mind,
                        content: request.content,
                        section: request.section,
                        status: FactStatus::Active,
                        confidence: 1.0,
                        reinforcement_count: 1,
                        decay_rate: 0.05,
                        decay_profile: request.decay_profile,
                        last_reinforced: timestamp.clone(),
                        created_at: timestamp,
                        version,
                        superseded_by: None,
                        source: request.source,
                        content_hash: Some(content_hash),
                        last_accessed: None,
                        created_session: None,
                        superseded_at: None,
                        archived_at: None,
                        jj_change_id: None,
                        persona_id: None,
                        layer: "project".into(),
                        tags: vec![],
                    },
                );
                Ok(MemoryMutationEffect::FactStored {
                    fact_id,
                    version,
                    action: StoreAction::Stored,
                })
            }
            MemoryMutation::ReinforceFact { fact } => {
                Self::check_fact_precondition(state, &fact)?;
                let version = Self::next_version(state)?;
                let existing = state.facts.get_mut(&fact.id).ok_or_else(|| {
                    MemoryError::Storage(anyhow::anyhow!("validated fact disappeared"))
                })?;
                if existing.status != FactStatus::Active {
                    return Err(MemoryError::FactNotFound(fact.id));
                }
                existing.reinforcement_count += 1;
                existing.last_reinforced = now_iso();
                existing.version = version;
                Ok(MemoryMutationEffect::FactReinforced {
                    fact_id: existing.id.clone(),
                    version,
                    reinforcement_count: existing.reinforcement_count,
                })
            }
            MemoryMutation::ReinforceFactOnce { fact_id } => {
                let version = Self::next_version(state)?;
                let existing = state
                    .facts
                    .get_mut(&fact_id)
                    .ok_or_else(|| MemoryError::FactNotFound(fact_id.clone()))?;
                if existing.status != FactStatus::Active {
                    return Err(MemoryError::FactNotFound(fact_id));
                }
                existing.reinforcement_count += 1;
                existing.last_reinforced = now_iso();
                existing.version = version;
                Ok(MemoryMutationEffect::FactReinforced {
                    fact_id: existing.id.clone(),
                    version,
                    reinforcement_count: existing.reinforcement_count,
                })
            }
            MemoryMutation::TransitionFacts { facts, status } => {
                if !matches!(status, FactStatus::Dormant | FactStatus::Archived) {
                    return Err(MemoryError::InvalidMutation(
                        "transition target must be dormant or archived".into(),
                    ));
                }
                validate_unique_fact_preconditions(&facts)?;
                for fact in &facts {
                    Self::check_fact_precondition(state, fact)?;
                }
                let mut transitioned = Vec::new();
                for fact in facts {
                    let is_active = state
                        .facts
                        .get(&fact.id)
                        .is_some_and(|existing| existing.status == FactStatus::Active);
                    if !is_active {
                        continue;
                    }
                    let version = Self::next_version(state)?;
                    let existing = state.facts.get_mut(&fact.id).ok_or_else(|| {
                        MemoryError::Storage(anyhow::anyhow!("validated fact disappeared"))
                    })?;
                    existing.status = status.clone();
                    existing.version = version;
                    transitioned.push(FactPrecondition {
                        id: fact.id,
                        expected_version: version,
                    });
                }
                Ok(MemoryMutationEffect::FactsTransitioned {
                    facts: transitioned,
                    status,
                })
            }
            MemoryMutation::SupersedeFact { fact, replacement } => {
                Self::check_fact_precondition(state, &fact)?;
                if state
                    .facts
                    .get(&fact.id)
                    .is_none_or(|existing| existing.status != FactStatus::Active)
                {
                    return Err(MemoryError::FactNotFound(fact.id));
                }
                let original_version = Self::next_version(state)?;
                let original = state.facts.get_mut(&fact.id).ok_or_else(|| {
                    MemoryError::Storage(anyhow::anyhow!("validated fact disappeared"))
                })?;
                original.status = FactStatus::Superseded;
                original.version = original_version;

                let replacement_version = Self::next_version(state)?;
                let replacement_id = gen_id();
                let timestamp = now_iso();
                state.facts.insert(
                    replacement_id.clone(),
                    Fact {
                        id: replacement_id.clone(),
                        mind: replacement.mind,
                        content_hash: Some(hash::content_hash(&replacement.content)),
                        content: replacement.content,
                        section: replacement.section,
                        status: FactStatus::Active,
                        confidence: 1.0,
                        reinforcement_count: 1,
                        decay_rate: 0.05,
                        decay_profile: replacement.decay_profile,
                        last_reinforced: timestamp.clone(),
                        created_at: timestamp,
                        version: replacement_version,
                        superseded_by: Some(fact.id.clone()),
                        source: replacement.source,
                        last_accessed: None,
                        created_session: None,
                        superseded_at: None,
                        archived_at: None,
                        jj_change_id: None,
                        persona_id: None,
                        layer: "project".into(),
                        tags: vec![],
                    },
                );
                Ok(MemoryMutationEffect::FactSuperseded {
                    original: FactPrecondition {
                        id: fact.id,
                        expected_version: original_version,
                    },
                    replacement: FactPrecondition {
                        id: replacement_id,
                        expected_version: replacement_version,
                    },
                })
            }
            MemoryMutation::SupersedeFactWithExisting { fact, replacement } => {
                if fact.id == replacement.id {
                    return Err(MemoryError::InvalidMutation(
                        "a fact cannot supersede itself".into(),
                    ));
                }
                Self::check_fact_precondition(state, &fact)?;
                Self::check_fact_precondition(state, &replacement)?;
                let original = state.facts.get(&fact.id).unwrap();
                let existing = state.facts.get(&replacement.id).unwrap();
                if original.status != FactStatus::Active
                    || existing.status != FactStatus::Active
                    || original.mind != existing.mind
                {
                    return Err(MemoryError::InvalidMutation(
                        "supersession requires distinct active facts in the same mind".into(),
                    ));
                }
                let original_version = Self::next_version(state)?;
                let original = state.facts.get_mut(&fact.id).unwrap();
                original.status = FactStatus::Superseded;
                original.version = original_version;
                let replacement_version = Self::next_version(state)?;
                let existing = state.facts.get_mut(&replacement.id).unwrap();
                existing.superseded_by = Some(fact.id.clone());
                existing.version = replacement_version;
                Ok(MemoryMutationEffect::FactSuperseded {
                    original: FactPrecondition {
                        id: fact.id,
                        expected_version: original_version,
                    },
                    replacement: FactPrecondition {
                        id: replacement.id,
                        expected_version: replacement_version,
                    },
                })
            }
            MemoryMutation::StoreEmbedding {
                fact,
                model_name,
                embedding,
            } => {
                Self::check_fact_precondition(state, &fact)?;
                if let Some(existing) = state
                    .embeddings
                    .iter()
                    .find(|entry| entry.model_name == model_name)
                    && existing.embedding.len() != embedding.len()
                {
                    return Err(MemoryError::EmbeddingDimensionMismatch {
                        expected: existing.embedding.len() as u32,
                        got: embedding.len() as u32,
                        stored_model: model_name,
                    });
                }
                let dims = embedding.len() as u32;
                state.embeddings.retain(|entry| entry.fact_id != fact.id);
                state.embeddings.push(EmbeddingEntry {
                    fact_id: fact.id.clone(),
                    model_name: model_name.clone(),
                    embedding,
                    inserted_at: now_iso(),
                });
                Ok(MemoryMutationEffect::EmbeddingStored {
                    fact_id: fact.id,
                    model_name,
                    dims,
                })
            }
            MemoryMutation::CreateEdge { request } => {
                if !state.facts.contains_key(&request.source_id) {
                    return Err(MemoryError::FactNotFound(request.source_id));
                }
                if !state.facts.contains_key(&request.target_id) {
                    return Err(MemoryError::FactNotFound(request.target_id));
                }
                let edge_id = gen_id();
                state.edges.push(Edge {
                    id: edge_id.clone(),
                    source_id: request.source_id,
                    target_id: request.target_id,
                    relation: request.relation,
                    description: request.description,
                    confidence: 1.0,
                    created_at: now_iso(),
                });
                Ok(MemoryMutationEffect::EdgeCreated { edge_id })
            }
            MemoryMutation::StoreEpisode { request } => {
                let episode_id = gen_id();
                let timestamp = now_iso();
                state.episodes.push(Episode {
                    id: episode_id.clone(),
                    mind: request.mind,
                    date: request.date.unwrap_or_else(|| timestamp[..10].to_string()),
                    title: request.title,
                    narrative: request.narrative,
                    created_at: timestamp,
                    affected_nodes: request.affected_nodes,
                    affected_changes: request.affected_changes,
                    files_changed: request.files_changed,
                    tags: request.tags,
                    tool_calls_count: request.tool_calls_count,
                    jj_change_id: None,
                });
                Ok(MemoryMutationEffect::EpisodeStored { episode_id })
            }
        }
    }

    fn import_jsonl_to_state(state: &mut State, jsonl: &str) -> Result<ImportStats> {
        let mut stats = ImportStats::default();
        for line in jsonl.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<JsonlRecord>(trimmed) {
                Ok(JsonlRecord::Fact(jf)) => {
                    persisted_lamport_version(jf.version)?;
                    let content_hash = jf
                        .content_hash
                        .clone()
                        .unwrap_or_else(|| hash::content_hash(&jf.content));
                    if let Some(existing) = state.facts.get(&jf.id) {
                        if jf.version > existing.version {
                            let mut updated = existing.clone();
                            updated.content = jf.content;
                            updated.section = jf.section;
                            updated.mind = jf.mind;
                            updated.status = jf.status;
                            updated.source = jf.source;
                            updated.content_hash = Some(content_hash);
                            updated.superseded_by = jf.supersedes;
                            updated.decay_profile = jf.decay_profile;
                            updated.persona_id = jf.persona_id;
                            updated.layer = jf.layer;
                            updated.tags = jf.tags;
                            updated.version = jf.version;
                            state.version_clock = state.version_clock.max(jf.version);
                            state.facts.insert(jf.id, updated);
                            stats.reinforced += 1;
                        } else {
                            stats.skipped += 1;
                        }
                    } else {
                        state.version_clock = state.version_clock.max(jf.version);
                        let fact = Fact {
                            id: jf.id.clone(),
                            mind: jf.mind,
                            content: jf.content,
                            section: jf.section,
                            status: jf.status,
                            confidence: 1.0,
                            reinforcement_count: 1,
                            decay_rate: 0.05,
                            decay_profile: jf.decay_profile,
                            last_reinforced: jf.created_at.clone(),
                            created_at: jf.created_at,
                            version: jf.version,
                            superseded_by: jf.supersedes,
                            source: jf.source,
                            content_hash: Some(content_hash),
                            last_accessed: None,
                            created_session: None,
                            superseded_at: None,
                            archived_at: None,
                            jj_change_id: None,
                            persona_id: jf.persona_id,
                            layer: jf.layer,
                            tags: jf.tags,
                        };
                        state.facts.insert(jf.id, fact);
                        stats.imported += 1;
                    }
                }
                Ok(JsonlRecord::Episode(ep)) => {
                    if state.episodes.iter().any(|existing| existing.id == ep.id) {
                        stats.skipped += 1;
                    } else {
                        state.episodes.push(ep);
                        stats.imported += 1;
                    }
                }
                Ok(JsonlRecord::Edge(edge)) => {
                    if state.edges.iter().any(|existing| existing.id == edge.id) {
                        stats.skipped += 1;
                    } else if !state.facts.contains_key(&edge.source_id) {
                        return Err(MemoryError::FactNotFound(edge.source_id));
                    } else if !state.facts.contains_key(&edge.target_id) {
                        return Err(MemoryError::FactNotFound(edge.target_id));
                    } else {
                        state.edges.push(edge);
                        stats.imported += 1;
                    }
                }
                Ok(JsonlRecord::Mind(_)) => stats.skipped += 1,
                Err(_) => stats.errors += 1,
            }
        }
        Ok(stats)
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryBackend for InMemoryBackend {
    async fn apply_mutation(
        &self,
        operation_id: &str,
        mutation: MemoryMutation,
    ) -> Result<MemoryMutationOutcome> {
        if operation_id.trim().is_empty() {
            return Err(MemoryError::InvalidMutation(
                "operation identity must not be empty".into(),
            ));
        }
        let payload_hash = mutation_payload_hash(&mutation)?;
        let mut state = self.state.lock().unwrap();
        if let Some((recorded_hash, effect)) = state.operation_receipts.get(operation_id) {
            if recorded_hash != &payload_hash {
                return Err(MemoryError::OperationConflict(operation_id.into()));
            }
            return Ok(MemoryMutationOutcome {
                effect: effect.clone(),
                replayed: true,
            });
        }

        let mut staged = state.clone();
        let effect = Self::apply_to_state(&mut staged, mutation)?;
        staged
            .operation_receipts
            .insert(operation_id.into(), (payload_hash, effect.clone()));
        *state = staged;
        Ok(MemoryMutationOutcome {
            effect,
            replayed: false,
        })
    }

    async fn store_fact(&self, req: StoreFact) -> Result<StoreResult> {
        let mut s = self.state.lock().unwrap();
        let ch = hash::content_hash(&req.content);

        // Check for dedup by content hash within same mind — find ID first, then mutate
        let existing_id = s
            .facts
            .iter()
            .find(|(_, f)| {
                f.mind == req.mind
                    && f.content_hash.as_deref() == Some(ch.as_str())
                    && f.status == FactStatus::Active
            })
            .map(|(id, _)| id.clone());

        if let Some(id) = existing_id {
            let vc = Self::next_version(&mut s)?;
            let ts = now_iso();
            let existing = s.facts.get_mut(&id).unwrap();
            existing.reinforcement_count += 1;
            existing.last_reinforced = ts;
            existing.version = vc;
            return Ok(StoreResult {
                fact: existing.clone(),
                action: StoreAction::Reinforced,
            });
        }

        let version = Self::next_version(&mut s)?;
        let fact = Fact {
            id: gen_id(),
            mind: req.mind,
            content: req.content,
            section: req.section,
            status: FactStatus::Active,
            confidence: 1.0,
            reinforcement_count: 1,
            decay_rate: 0.05,
            decay_profile: req.decay_profile,
            last_reinforced: now_iso(),
            created_at: now_iso(),
            version,
            superseded_by: None,
            source: req.source,
            content_hash: Some(ch),
            last_accessed: None,
            created_session: None,
            superseded_at: None,
            archived_at: None,
            jj_change_id: None,
            persona_id: None,
            layer: "project".into(),
            tags: vec![],
        };
        s.facts.insert(fact.id.clone(), fact.clone());
        Ok(StoreResult {
            fact,
            action: StoreAction::Stored,
        })
    }

    async fn get_fact(&self, id: &str) -> Result<Option<Fact>> {
        let s = self.state.lock().unwrap();
        Ok(s.facts
            .get(id)
            .filter(|f| f.status == FactStatus::Active)
            .cloned())
    }

    async fn list_facts(&self, mind: &str, filter: FactFilter) -> Result<Vec<Fact>> {
        let s = self.state.lock().unwrap();
        let status = filter.status.unwrap_or(FactStatus::Active);
        let mut facts: Vec<Fact> = s
            .facts
            .values()
            .filter(|f| {
                f.mind == mind
                    && f.status == status
                    && filter.section.as_ref().is_none_or(|sec| &f.section == sec)
            })
            .cloned()
            .collect();
        facts.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(facts)
    }

    async fn reinforce_fact(&self, id: &str) -> Result<Fact> {
        let mut s = self.state.lock().unwrap();
        if s.facts
            .get(id)
            .is_none_or(|fact| fact.status != FactStatus::Active)
        {
            return Err(MemoryError::FactNotFound(id.into()));
        }
        let vc = Self::next_version(&mut s)?;
        let ts = now_iso();
        let fact = s.facts.get_mut(id).unwrap();
        fact.reinforcement_count += 1;
        fact.last_reinforced = ts;
        fact.version = vc;
        Ok(fact.clone())
    }

    async fn dormancy_facts(&self, ids: &[&str]) -> Result<usize> {
        let mut state = self.state.lock().unwrap();
        let mut transitioned = 0;
        for id in ids {
            let is_active = state
                .facts
                .get(*id)
                .is_some_and(|fact| fact.status == FactStatus::Active);
            if is_active {
                let version = Self::next_version(&mut state)?;
                let fact = state.facts.get_mut(*id).ok_or_else(|| {
                    MemoryError::Storage(anyhow::anyhow!("active fact disappeared"))
                })?;
                fact.status = FactStatus::Dormant;
                fact.version = version;
                transitioned += 1;
            }
        }
        Ok(transitioned)
    }

    async fn archive_facts(&self, ids: &[&str]) -> Result<usize> {
        let mut s = self.state.lock().unwrap();
        let mut count = 0;
        for id in ids {
            // Check if active first, then update
            let is_active = s
                .facts
                .get(*id)
                .is_some_and(|f| f.status == FactStatus::Active);
            if is_active {
                let vc = Self::next_version(&mut s)?;
                let fact = s.facts.get_mut(*id).unwrap();
                fact.status = FactStatus::Archived;
                fact.version = vc;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn supersede_fact(&self, id: &str, replacement: StoreFact) -> Result<Fact> {
        let mut s = self.state.lock().unwrap();

        if s.facts
            .get(id)
            .is_none_or(|fact| fact.status != FactStatus::Active)
        {
            return Err(MemoryError::FactNotFound(id.into()));
        }

        // Version the original before the replacement, matching SQLite.
        let original_version = Self::next_version(&mut s)?;
        let original = s
            .facts
            .get_mut(id)
            .ok_or_else(|| MemoryError::Storage(anyhow::anyhow!("validated fact disappeared")))?;
        original.status = FactStatus::Superseded;
        original.version = original_version;

        let replacement_version = Self::next_version(&mut s)?;
        let new_id = gen_id();
        let ch = hash::content_hash(&replacement.content);
        let new_fact = Fact {
            id: new_id.clone(),
            mind: replacement.mind,
            content: replacement.content,
            section: replacement.section,
            status: FactStatus::Active,
            confidence: 1.0,
            reinforcement_count: 1,
            decay_rate: 0.05,
            decay_profile: replacement.decay_profile,
            last_reinforced: now_iso(),
            created_at: now_iso(),
            version: replacement_version,
            superseded_by: Some(id.to_string()), // "I supersede old_id"
            source: replacement.source,
            content_hash: Some(ch),
            last_accessed: None,
            created_session: None,
            superseded_at: None,
            archived_at: None,
            jj_change_id: None,
            persona_id: None,
            layer: "project".into(),
            tags: vec![],
        };

        s.facts.insert(new_id, new_fact.clone());
        Ok(new_fact)
    }

    async fn superseding_fact(&self, old_id: &str) -> Result<Option<Fact>> {
        let state = self.state.lock().unwrap();
        let Some(original) = state.facts.get(old_id) else {
            return Ok(None);
        };
        if original.status != FactStatus::Superseded {
            return Ok(None);
        }
        let mut predecessor = old_id;
        let mut visited = HashSet::new();
        visited.insert(old_id);
        while let Some(replacement) = state
            .facts
            .values()
            .filter(|fact| fact.superseded_by.as_deref() == Some(predecessor))
            .max_by(|left, right| {
                left.version
                    .cmp(&right.version)
                    .then_with(|| left.id.cmp(&right.id))
            })
        {
            if !visited.insert(replacement.id.as_str()) {
                return Err(MemoryError::Storage(anyhow::anyhow!(
                    "supersession cycle detected"
                )));
            }
            if replacement.status == FactStatus::Active {
                return Ok(Some(replacement.clone()));
            }
            if replacement.status != FactStatus::Superseded {
                break;
            }
            predecessor = &replacement.id;
        }

        let Some(source) = original
            .source
            .as_deref()
            .filter(|source| source.starts_with("codex-vault:"))
        else {
            return Ok(None);
        };
        let Some(latest) = state
            .facts
            .values()
            .filter(|fact| fact.source.as_deref() == Some(source))
            .max_by(|left, right| {
                left.version
                    .cmp(&right.version)
                    .then_with(|| left.id.cmp(&right.id))
            })
        else {
            return Ok(None);
        };
        if latest.status == FactStatus::Active {
            return Ok(Some(latest.clone()));
        }
        if latest.status != FactStatus::Superseded {
            return Ok(None);
        }
        Ok(state
            .facts
            .values()
            .filter(|fact| {
                fact.status == FactStatus::Active
                    && fact.superseded_by.as_deref() == Some(latest.id.as_str())
            })
            .max_by(|left, right| {
                left.version
                    .cmp(&right.version)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .cloned())
    }

    async fn fts_search(&self, mind: &str, query: &str, k: usize) -> Result<Vec<ScoredFact>> {
        let s = self.state.lock().unwrap();
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<ScoredFact> = s
            .facts
            .values()
            .filter(|f| f.mind == mind && f.status == FactStatus::Active)
            .filter_map(|f| {
                let content_lower = f.content.to_lowercase();
                let matches = terms.iter().filter(|t| content_lower.contains(**t)).count();
                if matches == 0 {
                    return None;
                }
                let relevance = matches as f64 / terms.len().max(1) as f64;
                let score = crate::decay::ambient_score(relevance, f)?;
                Some(ScoredFact {
                    fact: f.clone(),
                    similarity: relevance,
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.fact.id.cmp(&b.fact.id))
        });
        results.truncate(k);
        Ok(results)
    }

    async fn vector_search(
        &self,
        mind: &str,
        embedding: &[f32],
        k: usize,
        min_similarity: f32,
    ) -> Result<Vec<ScoredFact>> {
        let s = self.state.lock().unwrap();

        // Find embeddings for this mind
        let mind_embeddings: Vec<&EmbeddingEntry> = s
            .embeddings
            .iter()
            .filter(|e| {
                s.facts
                    .get(&e.fact_id)
                    .is_some_and(|f| f.mind == mind && f.status == FactStatus::Active)
            })
            .collect();

        if mind_embeddings.is_empty() {
            return Err(MemoryError::NoEmbeddings);
        }

        // Check dimension
        let expected_dims = mind_embeddings[0].embedding.len() as u32;
        let got_dims = embedding.len() as u32;
        if expected_dims != got_dims {
            return Err(MemoryError::EmbeddingDimensionMismatch {
                expected: expected_dims,
                got: got_dims,
                stored_model: mind_embeddings[0].model_name.clone(),
            });
        }

        let mut results: Vec<ScoredFact> = mind_embeddings
            .iter()
            .filter_map(|e| {
                let sim = vectors::cosine_similarity(&e.embedding, embedding);
                if sim < min_similarity {
                    return None;
                }
                let fact = s.facts.get(&e.fact_id)?.clone();
                let score = crate::decay::ambient_score(sim as f64, &fact)?;
                Some(ScoredFact {
                    fact,
                    similarity: sim as f64,
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.fact.id.cmp(&b.fact.id))
        });
        results.truncate(k);
        Ok(results)
    }

    async fn store_embedding(
        &self,
        fact_id: &str,
        model_name: &str,
        embedding: &[f32],
    ) -> Result<()> {
        validate_embedding(embedding)?;
        let mut s = self.state.lock().unwrap();
        if !s.facts.contains_key(fact_id) {
            return Err(MemoryError::FactNotFound(fact_id.into()));
        }
        if let Some(existing) = s
            .embeddings
            .iter()
            .find(|entry| entry.model_name == model_name)
            && existing.embedding.len() != embedding.len()
        {
            return Err(MemoryError::EmbeddingDimensionMismatch {
                expected: existing.embedding.len() as u32,
                got: embedding.len() as u32,
                stored_model: model_name.into(),
            });
        }
        // Remove existing embedding for this fact
        s.embeddings.retain(|e| e.fact_id != fact_id);
        s.embeddings.push(EmbeddingEntry {
            fact_id: fact_id.into(),
            model_name: model_name.into(),
            embedding: embedding.to_vec(),
            inserted_at: now_iso(),
        });
        Ok(())
    }

    async fn embedding_metadata(&self, mind: &str) -> Result<Option<EmbeddingMetadata>> {
        let s = self.state.lock().unwrap();
        let entry = s
            .embeddings
            .iter()
            .find(|e| s.facts.get(&e.fact_id).is_some_and(|f| f.mind == mind));
        Ok(entry.map(|e| EmbeddingMetadata {
            model_name: e.model_name.clone(),
            dims: e.embedding.len() as u32,
            inserted_at: e.inserted_at.clone(),
        }))
    }

    async fn create_edge(&self, req: CreateEdge) -> Result<Edge> {
        let mut s = self.state.lock().unwrap();
        if !s.facts.contains_key(&req.source_id) {
            return Err(MemoryError::FactNotFound(req.source_id));
        }
        if !s.facts.contains_key(&req.target_id) {
            return Err(MemoryError::FactNotFound(req.target_id));
        }
        let edge = Edge {
            id: gen_id(),
            source_id: req.source_id,
            target_id: req.target_id,
            relation: req.relation,
            description: req.description,
            confidence: 1.0,
            created_at: now_iso(),
        };
        s.edges.push(edge.clone());
        Ok(edge)
    }

    async fn get_edges(&self, mind: &str, fact_id: &str) -> Result<Vec<Edge>> {
        let s = self.state.lock().unwrap();
        if s.facts.get(fact_id).is_none_or(|fact| fact.mind != mind) {
            return Ok(Vec::new());
        }
        Ok(s.edges
            .iter()
            .filter(|e| e.source_id == fact_id || e.target_id == fact_id)
            .cloned()
            .collect())
    }

    async fn store_episode(&self, req: StoreEpisode) -> Result<Episode> {
        let mut s = self.state.lock().unwrap();
        let episode = Episode {
            id: gen_id(),
            mind: req.mind,
            date: req.date.unwrap_or_else(|| now_iso()[..10].to_string()),
            title: req.title,
            narrative: req.narrative,
            created_at: now_iso(),
            affected_nodes: req.affected_nodes,
            affected_changes: req.affected_changes,
            files_changed: req.files_changed,
            tags: req.tags,
            tool_calls_count: req.tool_calls_count,
            jj_change_id: None,
        };
        s.episodes.push(episode.clone());
        Ok(episode)
    }

    async fn list_episodes(&self, mind: &str, k: usize) -> Result<Vec<Episode>> {
        let s = self.state.lock().unwrap();
        let mut eps: Vec<Episode> = s
            .episodes
            .iter()
            .filter(|e| e.mind == mind)
            .cloned()
            .collect();
        eps.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| b.created_at.cmp(&a.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        eps.truncate(k);
        Ok(eps)
    }

    async fn search_episodes(&self, mind: &str, query: &str, k: usize) -> Result<Vec<Episode>> {
        let s = self.state.lock().unwrap();
        let query_lower = query.to_lowercase();
        let mut results: Vec<Episode> = s
            .episodes
            .iter()
            .filter(|e| e.mind == mind && e.narrative.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        results.truncate(k);
        Ok(results)
    }

    async fn export_jsonl(&self, mind: &str) -> Result<String> {
        let s = self.state.lock().unwrap();
        let mut lines = Vec::new();

        // Facts (sorted by id for determinism)
        let mut facts: Vec<&Fact> = s
            .facts
            .values()
            .filter(|f| f.mind == mind && f.status == FactStatus::Active)
            .collect();
        facts.sort_by(|a, b| a.id.cmp(&b.id));
        for fact in facts {
            let record = JsonlRecord::Fact(JsonlFact {
                id: fact.id.clone(),
                mind: fact.mind.clone(),
                content: fact.content.clone(),
                section: fact.section.clone(),
                status: fact.status.clone(),
                created_at: fact.created_at.clone(),
                source: fact.source.clone(),
                content_hash: fact.content_hash.clone(),
                supersedes: fact.superseded_by.clone(),
                version: fact.version,
                decay_profile: fact.decay_profile.clone(),
                persona_id: fact.persona_id.clone(),
                layer: fact.layer.clone(),
                tags: fact.tags.clone(),
            });
            lines.push(serde_json::to_string(&record).unwrap());
        }

        // Edges
        let mut edges: Vec<&Edge> = s
            .edges
            .iter()
            .filter(|e| s.facts.get(&e.source_id).is_some_and(|f| f.mind == mind))
            .collect();
        edges.sort_by(|a, b| a.id.cmp(&b.id));
        for edge in edges {
            lines.push(serde_json::to_string(&JsonlRecord::Edge(edge.clone())).unwrap());
        }

        // Episodes
        let mut eps: Vec<&Episode> = s.episodes.iter().filter(|e| e.mind == mind).collect();
        eps.sort_by(|a, b| a.id.cmp(&b.id));
        for ep in eps {
            lines.push(serde_json::to_string(&JsonlRecord::Episode(ep.clone())).unwrap());
        }

        Ok(lines.join("\n"))
    }

    async fn import_jsonl(&self, jsonl: &str) -> Result<ImportStats> {
        let mut state = self.state.lock().unwrap();
        let mut staged = state.clone();
        let stats = Self::import_jsonl_to_state(&mut staged, jsonl)?;
        *state = staged;
        Ok(stats)
    }

    async fn stats(&self, mind: &str) -> Result<MemoryStats> {
        let s = self.state.lock().unwrap();
        let mind_facts: Vec<&Fact> = s.facts.values().filter(|f| f.mind == mind).collect();
        let active = mind_facts
            .iter()
            .filter(|f| f.status == FactStatus::Active)
            .count();
        let archived = mind_facts
            .iter()
            .filter(|f| f.status == FactStatus::Archived)
            .count();
        let superseded = mind_facts
            .iter()
            .filter(|f| f.status == FactStatus::Superseded)
            .count();
        let with_vectors = s
            .embeddings
            .iter()
            .filter(|e| s.facts.get(&e.fact_id).is_some_and(|f| f.mind == mind))
            .count();
        let meta = s
            .embeddings
            .iter()
            .find(|e| s.facts.get(&e.fact_id).is_some_and(|f| f.mind == mind));
        let episodes = s.episodes.iter().filter(|e| e.mind == mind).count();
        let edges = s
            .edges
            .iter()
            .filter(|e| s.facts.get(&e.source_id).is_some_and(|f| f.mind == mind))
            .count();
        let version_hwm = s
            .facts
            .values()
            .filter(|f| f.mind == mind)
            .map(|f| f.version)
            .max()
            .unwrap_or(0);

        Ok(MemoryStats {
            total_facts: mind_facts.len(),
            active_facts: active,
            archived_facts: archived,
            superseded_facts: superseded,
            facts_with_vectors: with_vectors,
            embedding_model: meta.map(|e| e.model_name.clone()),
            embedding_dims: meta.map(|e| e.embedding.len() as u32),
            episodes,
            edges,
            version_hwm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::run_backend_tests;

    #[tokio::test]
    async fn inmemory_backend_passes_all_tests() {
        let backend = InMemoryBackend::new();
        run_backend_tests(&backend).await;
    }
}
