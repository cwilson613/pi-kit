//! Extension mind persistence — in-memory fact storage with JSONL disk backing.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use chrono::Utc;

// Import types from omegon-extension SDK
use omegon_extension::{Fact, Episode, MindMetadata};

/// In-memory extension mind with persistent backing.
/// Manages facts and episodes, persists to JSONL files.
pub struct ExtensionMind {
    /// Extension name
    name: String,
    /// Mind directory: ~/.omegon/extensions/{name}/mind/
    mind_dir: PathBuf,
    /// All facts (keyed by id)
    facts: HashMap<String, Fact>,
    /// All episodes (keyed by id)
    episodes: HashMap<String, Episode>,
    /// Metadata
    metadata: MindMetadata,
}

impl ExtensionMind {
    /// Load mind from disk, or create new if doesn't exist.
    pub async fn load(
        name: String,
        ext_dir: &Path,
        extension_version: String,
        sdk_version: String,
    ) -> Result<Self> {
        let mind_dir = ext_dir.join("mind");

        // Try loading existing mind
        if mind_dir.exists() {
            return Self::load_from_disk(&mind_dir, name, ext_dir);
        }

        // Create new mind
        Ok(Self {
            name: name.clone(),
            mind_dir,
            facts: HashMap::new(),
            episodes: HashMap::new(),
            metadata: MindMetadata::new(name, extension_version, sdk_version),
        })
    }

    /// Load mind from disk (facts.jsonl, episodes.jsonl, metadata.json).
    fn load_from_disk(mind_dir: &Path, name: String, _ext_dir: &Path) -> Result<Self> {
        // Load metadata
        let metadata_path = mind_dir.join("metadata.json");
        let metadata: MindMetadata = if metadata_path.exists() {
            let content = std::fs::read_to_string(&metadata_path)?;
            serde_json::from_str(&content)?
        } else {
            MindMetadata::new(name.clone(), "0.0.0".to_string(), "0.15.0".to_string())
        };

        // Load facts from facts.jsonl
        let mut facts = HashMap::new();
        let facts_path = mind_dir.join("facts.jsonl");
        if facts_path.exists() {
            let content = std::fs::read_to_string(&facts_path)?;
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Fact>(line) {
                    Ok(fact) => {
                        facts.insert(fact.id.clone(), fact);
                    }
                    Err(e) => {
                        tracing::warn!("corrupted fact in {}: {}", facts_path.display(), e);
                        continue;
                    }
                }
            }
        }

        // Load episodes from episodes.jsonl
        let mut episodes = HashMap::new();
        let episodes_path = mind_dir.join("episodes.jsonl");
        if episodes_path.exists() {
            let content = std::fs::read_to_string(&episodes_path)?;
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Episode>(line) {
                    Ok(episode) => {
                        episodes.insert(episode.id.clone(), episode);
                    }
                    Err(e) => {
                        tracing::warn!("corrupted episode in {}: {}", episodes_path.display(), e);
                        continue;
                    }
                }
            }
        }

        Ok(Self {
            name,
            mind_dir: mind_dir.to_path_buf(),
            facts,
            episodes,
            metadata,
        })
    }

    /// Search facts by query using simple text matching.
    /// TODO: Replace with BM25 when integrated.
    pub fn search(&self, query: &str) -> Vec<Fact> {
        if query.is_empty() {
            return self.facts.values().cloned().collect();
        }

        let query_lower = query.to_lowercase();
        let mut results: Vec<_> = self
            .facts
            .values()
            .filter(|fact| {
                fact.content.to_lowercase().contains(&query_lower)
                    || fact.section.to_lowercase().contains(&query_lower)
                    || fact.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        // Sort by reinforcement count (most useful first)
        results.sort_by_key(|f| std::cmp::Reverse(f.reinforced));
        results
    }

    /// Add a new fact to the mind.
    pub fn add_fact(&mut self, mut fact: Fact) -> String {
        // Ensure ID is unique
        let mut id = fact.id.clone();
        let mut counter = 1;
        while self.facts.contains_key(&id) {
            id = format!("{}-{}", fact.id, counter);
            counter += 1;
        }
        fact.id = id.clone();

        self.facts.insert(id.clone(), fact);
        self.metadata.total_facts = self.facts.len();
        id
    }

    /// Update an existing fact.
    pub fn update_fact(&mut self, id: String, mut fact: Fact) -> Result<()> {
        if !self.facts.contains_key(&id) {
            return Err(anyhow!("fact not found: {}", id));
        }

        fact.id = id.clone();
        self.facts.insert(id, fact);
        Ok(())
    }

    /// Reinforce a fact (increment usefulness counter).
    pub fn reinforce_fact(&mut self, id: &str) -> Result<()> {
        let fact = self
            .facts
            .get_mut(id)
            .ok_or_else(|| anyhow!("fact not found: {}", id))?;

        fact.reinforce();
        Ok(())
    }

    /// Delete a fact from the mind.
    pub fn delete_fact(&mut self, id: &str) -> Result<()> {
        self.facts
            .remove(id)
            .ok_or_else(|| anyhow!("fact not found: {}", id))?;
        self.metadata.total_facts = self.facts.len();
        Ok(())
    }

    /// Add an episode (grouping of facts).
    pub fn add_episode(&mut self, mut episode: Episode) -> String {
        let mut id = episode.id.clone();
        let mut counter = 1;
        while self.episodes.contains_key(&id) {
            id = format!("{}-{}", episode.id, counter);
            counter += 1;
        }
        episode.id = id.clone();

        self.episodes.insert(id.clone(), episode);
        self.metadata.total_episodes = self.episodes.len();
        id
    }

    /// Persist mind to disk (facts.jsonl, episodes.jsonl, metadata.json).
    pub async fn save(&mut self) -> Result<()> {
        // Create mind directory
        std::fs::create_dir_all(&self.mind_dir)?;

        // Write facts.jsonl
        let facts_path = self.mind_dir.join("facts.jsonl");
        let facts_content = self
            .facts
            .values()
            .map(|f| serde_json::to_string(f).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");

        if !facts_content.is_empty() {
            std::fs::write(&facts_path, format!("{}\n", facts_content))?;
        }

        // Write episodes.jsonl
        let episodes_path = self.mind_dir.join("episodes.jsonl");
        let episodes_content = self
            .episodes
            .values()
            .map(|e| serde_json::to_string(e).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");

        if !episodes_content.is_empty() {
            std::fs::write(&episodes_path, format!("{}\n", episodes_content))?;
        }

        // Update metadata
        self.metadata.last_updated = Utc::now().to_rfc3339();
        self.metadata.total_facts = self.facts.len();
        self.metadata.total_episodes = self.episodes.len();

        // Write metadata.json
        let metadata_path = self.mind_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&self.metadata)?;
        std::fs::write(&metadata_path, metadata_json)?;

        tracing::debug!(
            name = %self.name,
            facts = self.facts.len(),
            episodes = self.episodes.len(),
            "mind persisted to disk"
        );

        Ok(())
    }

    /// Get mind statistics.
    pub fn stats(&self) -> MindStats {
        MindStats {
            fact_count: self.facts.len(),
            episode_count: self.episodes.len(),
            total_reinforced: self.facts.values().map(|f| f.reinforced as usize).sum(),
        }
    }

    /// Get fact count.
    pub fn fact_count(&self) -> usize {
        self.facts.len()
    }

    /// Get all facts as vector.
    pub fn facts(&self) -> Vec<Fact> {
        self.facts.values().cloned().collect()
    }

    /// Get all episodes as vector.
    pub fn episodes(&self) -> Vec<Episode> {
        self.episodes.values().cloned().collect()
    }
}

/// Mind statistics.
#[derive(Debug, Clone)]
pub struct MindStats {
    pub fact_count: usize,
    pub episode_count: usize,
    pub total_reinforced: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mind_creation() {
        let mind = ExtensionMind {
            name: "test-ext".to_string(),
            mind_dir: PathBuf::from("/tmp/test-mind"),
            facts: HashMap::new(),
            episodes: HashMap::new(),
            metadata: MindMetadata::new("test-ext".to_string(), "0.1.0".to_string(), "0.15.0".to_string()),
        };

        assert_eq!(mind.fact_count(), 0);
        assert_eq!(mind.stats().fact_count, 0);
    }

    #[tokio::test]
    async fn test_add_fact() {
        let mut mind = ExtensionMind {
            name: "test-ext".to_string(),
            mind_dir: PathBuf::from("/tmp/test-mind"),
            facts: HashMap::new(),
            episodes: HashMap::new(),
            metadata: MindMetadata::new("test-ext".to_string(), "0.1.0".to_string(), "0.15.0".to_string()),
        };

        let fact = Fact::new(
            "fact-001".to_string(),
            "Test".to_string(),
            "Test content".to_string(),
            vec!["test".to_string()],
            0.9,
        );

        let id = mind.add_fact(fact);
        assert_eq!(mind.fact_count(), 1);
        assert!(mind.facts.contains_key(&id));
    }

    #[tokio::test]
    async fn test_search() {
        let mut mind = ExtensionMind {
            name: "test-ext".to_string(),
            mind_dir: PathBuf::from("/tmp/test-mind"),
            facts: HashMap::new(),
            episodes: HashMap::new(),
            metadata: MindMetadata::new("test-ext".to_string(), "0.1.0".to_string(), "0.15.0".to_string()),
        };

        mind.add_fact(Fact::new(
            "fact-001".to_string(),
            "Architecture".to_string(),
            "Uses async patterns".to_string(),
            vec!["patterns".to_string()],
            0.9,
        ));

        mind.add_fact(Fact::new(
            "fact-002".to_string(),
            "Communication".to_string(),
            "Prefers async over sync".to_string(),
            vec!["communication".to_string()],
            0.85,
        ));

        let results = mind.search("async");
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_reinforce_fact() {
        let mut mind = ExtensionMind {
            name: "test-ext".to_string(),
            mind_dir: PathBuf::from("/tmp/test-mind"),
            facts: HashMap::new(),
            episodes: HashMap::new(),
            metadata: MindMetadata::new("test-ext".to_string(), "0.1.0".to_string(), "0.15.0".to_string()),
        };

        let id = mind.add_fact(Fact::new(
            "fact-001".to_string(),
            "Test".to_string(),
            "Test".to_string(),
            vec![],
            0.9,
        ));

        mind.reinforce_fact(&id).unwrap();
        assert_eq!(mind.facts[&id].reinforced, 1);

        mind.reinforce_fact(&id).unwrap();
        assert_eq!(mind.facts[&id].reinforced, 2);
    }
}
