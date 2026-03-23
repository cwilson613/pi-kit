//! Lifecycle ContextProvider — injects design-tree and openspec context
//! into the system prompt based on signals.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use omegon_traits::{ContextInjection, ContextProvider, ContextSignals};

use super::design;
use super::spec;
use super::types::*;

/// A node that was previously parseable but is now broken.
/// Kept visible so the operator can trace the breakage.
#[derive(Debug, Clone)]
pub struct DegradedNode {
    /// The node ID as it was last known.
    pub id: String,
    /// The title from the last successful parse.
    pub title: String,
    /// The status from the last successful parse.
    pub last_status: NodeStatus,
    /// Path to the file that's still on disk but no longer parses.
    pub file_path: PathBuf,
    /// Why the node degraded.
    pub reason: DegradedReason,
}

/// Why a previously-valid node became unparseable.
#[derive(Debug, Clone, Copy)]
pub enum DegradedReason {
    /// File exists but frontmatter is missing or unparseable.
    ParseFailed,
    /// File exists but the `id` field was removed from frontmatter.
    MissingId,
}

impl std::fmt::Display for DegradedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseFailed => write!(f, "frontmatter parse failed"),
            Self::MissingId => write!(f, "id field missing"),
        }
    }
}

/// Provides lifecycle context (design nodes + openspec changes) to the agent.
pub struct LifecycleContextProvider {
    /// All design nodes, keyed by id.
    nodes: HashMap<String, DesignNode>,
    /// Parsed sections cache (lazy-loaded).
    sections_cache: HashMap<String, DocumentSections>,
    /// Active openspec changes.
    changes: Vec<ChangeInfo>,
    /// Currently focused node id (if any).
    focused_node: Option<String>,
    /// The repo root for re-scanning.
    repo_path: PathBuf,
    /// Nodes that were valid on a previous scan but broke on rescan.
    degraded: Vec<DegradedNode>,
}

impl LifecycleContextProvider {
    /// Initialize by scanning docs/ and openspec/ directories.
    pub fn new(repo_path: &Path) -> Self {
        let docs_dir = repo_path.join("docs");
        let nodes = design::scan_design_docs(&docs_dir);
        let changes = spec::list_changes(repo_path);

        tracing::info!(
            nodes = nodes.len(),
            changes = changes.len(),
            "Lifecycle context initialized"
        );

        Self {
            nodes,
            sections_cache: HashMap::new(),
            changes,
            focused_node: None,
            repo_path: repo_path.to_path_buf(),
            degraded: Vec::new(),
        }
    }

    /// Set the focused design node.
    pub fn set_focus(&mut self, node_id: Option<String>) {
        self.focused_node = node_id;
    }

    /// Get the focused node ID (if any).
    pub fn focused_node_id(&self) -> Option<&str> {
        self.focused_node.as_deref()
    }

    /// Get a design node by id.
    pub fn get_node(&self, id: &str) -> Option<&DesignNode> {
        self.nodes.get(id)
    }

    /// Get all design nodes.
    pub fn all_nodes(&self) -> &HashMap<String, DesignNode> {
        &self.nodes
    }

    /// Get all active changes.
    pub fn changes(&self) -> &[ChangeInfo] {
        &self.changes
    }

    /// Get degraded nodes (previously valid, now broken).
    pub fn degraded_nodes(&self) -> &[DegradedNode] {
        &self.degraded
    }

    /// Refresh by re-scanning (call after mutations from TS side).
    /// Detects nodes that disappeared between scans — if the file still
    /// exists on disk but no longer parses, the node is marked degraded
    /// so the operator can trace the breakage.
    pub fn refresh(&mut self) {
        let docs_dir = self.repo_path.join("docs");
        let scan = design::scan_design_docs_full(&docs_dir);
        let new_nodes = scan.nodes;

        // Build set of paths that had frontmatter but failed to produce nodes
        let failure_paths: std::collections::HashSet<_> =
            scan.parse_failures.iter().collect();

        // Detect degraded: nodes present before but missing now
        let mut degraded = Vec::new();
        for (id, old_node) in &self.nodes {
            if !new_nodes.contains_key(id) && old_node.file_path.exists() {
                // Determine reason from scan results — no re-read needed
                let reason = if failure_paths.contains(&old_node.file_path) {
                    DegradedReason::MissingId
                } else {
                    DegradedReason::ParseFailed
                };

                degraded.push(DegradedNode {
                    id: id.clone(),
                    title: old_node.title.clone(),
                    last_status: old_node.status,
                    file_path: old_node.file_path.clone(),
                    reason,
                });

                tracing::warn!(
                    node_id = %id,
                    file = %old_node.file_path.display(),
                    reason = %reason,
                    "design node degraded — file exists but no longer parses"
                );
            }
        }

        // Nodes that reappear after being degraded → un-degrade them
        self.degraded.retain(|d| !new_nodes.contains_key(&d.id));
        // Add newly degraded nodes
        for d in degraded {
            if !self.degraded.iter().any(|existing| existing.id == d.id) {
                self.degraded.push(d);
            }
        }
        // Clear degraded nodes whose files were deleted (genuinely removed, not broken)
        self.degraded.retain(|d| d.file_path.exists());

        self.nodes = new_nodes;
        self.changes = spec::list_changes(&self.repo_path);
        self.sections_cache.clear();
    }

    fn get_sections(&mut self, node_id: &str) -> Option<&DocumentSections> {
        if !self.sections_cache.contains_key(node_id)
            && let Some(node) = self.nodes.get(node_id)
                && let Some(sections) = design::read_node_sections(node) {
                    self.sections_cache.insert(node_id.to_string(), sections);
                }
        self.sections_cache.get(node_id)
    }
}

impl ContextProvider for LifecycleContextProvider {
    fn provide_context(&self, _signals: &ContextSignals<'_>) -> Option<ContextInjection> {
        let mut parts = Vec::new();

        // 1. Focused design node context
        if let Some(ref node_id) = self.focused_node
            && let Some(node) = self.nodes.get(node_id) {
                // Read sections (can't use get_sections due to &self)
                if let Some(sections) = design::read_node_sections(node) {
                    let injection = design::build_context_injection(node, &sections);
                    if !injection.is_empty() {
                        parts.push(injection);
                    }
                }
            }

        // 2. Active openspec changes (if any are implementing/verifying)
        let active: Vec<_> = self.changes.iter()
            .filter(|c| matches!(c.stage, ChangeStage::Implementing | ChangeStage::Verifying))
            .collect();
        if !active.is_empty() {
            let injection = spec::build_context_injection(&active.iter().copied().cloned().collect::<Vec<_>>());
            if !injection.is_empty() {
                parts.push(injection);
            }
        }

        if parts.is_empty() {
            return None;
        }

        Some(ContextInjection {
            source: "lifecycle".into(),
            content: parts.join("\n\n"),
            priority: 150, // Between base prompt (200) and memory facts
            ttl_turns: 3,  // Refresh every few turns
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_returns_none_when_empty() {
        let provider = LifecycleContextProvider {
            nodes: HashMap::new(),
            sections_cache: HashMap::new(),
            changes: vec![],
            focused_node: None,
            repo_path: PathBuf::from("/nonexistent"),
            degraded: Vec::new(),
        };

        let signals = ContextSignals {
            user_prompt: "hello",
            recent_tools: &[],
            recent_files: &[],
            lifecycle_phase: &omegon_traits::LifecyclePhase::Idle,
            turn_number: 1,
            context_budget_tokens: 4000,
        };

        assert!(provider.provide_context(&signals).is_none());
    }

    #[test]
    fn provider_injects_focused_node() {
        let mut nodes = HashMap::new();
        let tmp = std::env::temp_dir().join("omegon-lifecycle-test");
        let _ = std::fs::create_dir_all(&tmp);
        let doc_path = tmp.join("test.md");
        std::fs::write(&doc_path, "---\nid: test\ntitle: Test\nstatus: decided\n---\n\n# Test\n\n## Overview\n\nTest overview.\n\n## Decisions\n\n### Use X\n\n**Status:** decided\n\n**Rationale:** Because Y.\n").unwrap();

        let fm = design::parse_frontmatter(&std::fs::read_to_string(&doc_path).unwrap()).unwrap();
        let node = design::node_from_frontmatter(&fm, doc_path).unwrap();
        nodes.insert("test".to_string(), node);

        let provider = LifecycleContextProvider {
            nodes,
            sections_cache: HashMap::new(),
            changes: vec![],
            focused_node: Some("test".to_string()),
            repo_path: tmp.clone(),
            degraded: Vec::new(),
        };

        let signals = ContextSignals {
            user_prompt: "hello",
            recent_tools: &[],
            recent_files: &[],
            lifecycle_phase: &omegon_traits::LifecyclePhase::Idle,
            turn_number: 1,
            context_budget_tokens: 4000,
        };

        let injection = provider.provide_context(&signals).unwrap();
        assert!(injection.content.contains("● test — Test"));
        assert!(injection.content.contains("Test overview"));
        assert!(injection.content.contains("Use X"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn provider_injects_active_changes() {
        let provider = LifecycleContextProvider {
            nodes: HashMap::new(),
            sections_cache: HashMap::new(),
            changes: vec![ChangeInfo {
                name: "my-change".into(),
                path: PathBuf::new(),
                stage: ChangeStage::Implementing,
                has_proposal: true,
                has_design: true,
                has_specs: true,
                has_tasks: true,
                total_tasks: 8,
                done_tasks: 5,
                specs: vec![],
            }],
            focused_node: None,
            repo_path: PathBuf::from("/nonexistent"),
            degraded: Vec::new(),
        };

        let signals = ContextSignals {
            user_prompt: "hello",
            recent_tools: &[],
            recent_files: &[],
            lifecycle_phase: &omegon_traits::LifecyclePhase::Idle,
            turn_number: 1,
            context_budget_tokens: 4000,
        };

        let injection = provider.provide_context(&signals).unwrap();
        assert!(injection.content.contains("my-change"));
        assert!(injection.content.contains("5/8"));
    }

    #[test]
    fn refresh_detects_degraded_nodes() {
        // Create a temp dir with a valid design doc
        let tmp = std::env::temp_dir().join("omegon-degraded-test");
        let docs_dir = tmp.join("docs");
        let _ = std::fs::create_dir_all(&docs_dir);
        let doc_path = docs_dir.join("test-node.md");
        std::fs::write(
            &doc_path,
            "---\nid: test-node\ntitle: Test Node\nstatus: exploring\n---\n\n# Test\n",
        ).unwrap();

        // Initial scan should find the node
        let mut provider = LifecycleContextProvider::new(&tmp);
        assert!(provider.all_nodes().contains_key("test-node"));
        assert!(provider.degraded_nodes().is_empty());

        // Break the frontmatter (remove id)
        std::fs::write(&doc_path, "---\ntitle: Broken\nstatus: exploring\n---\n\n# Broken\n").unwrap();

        // Refresh should detect the node as degraded
        provider.refresh();
        assert!(!provider.all_nodes().contains_key("test-node"));
        assert_eq!(provider.degraded_nodes().len(), 1);
        assert_eq!(provider.degraded_nodes()[0].id, "test-node");
        assert!(matches!(provider.degraded_nodes()[0].reason, DegradedReason::MissingId));

        // Fix the file — node should un-degrade
        std::fs::write(
            &doc_path,
            "---\nid: test-node\ntitle: Fixed Node\nstatus: decided\n---\n\n# Fixed\n",
        ).unwrap();
        provider.refresh();
        assert!(provider.all_nodes().contains_key("test-node"));
        assert!(provider.degraded_nodes().is_empty());

        // Delete the file — should not appear as degraded (genuinely removed)
        std::fs::remove_file(&doc_path).unwrap();
        provider.refresh();
        assert!(!provider.all_nodes().contains_key("test-node"));
        assert!(provider.degraded_nodes().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
