//! codebase_search and codebase_index tools backed by omegon-codescan.

use async_trait::async_trait;
use omegon_codescan::SearchScope;
use omegon_traits::{ContentBlock, ToolDefinition, ToolProvider, ToolResult};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

use crate::codescan_service::{
    CodescanBinding, CodescanRequest, CodescanResponse, unavailable_code,
};

pub struct CodescanProvider {
    repo_path: PathBuf,
    codescan: CodescanBinding,
}

impl CodescanProvider {
    pub(crate) fn new(repo_path: PathBuf, codescan: CodescanBinding) -> Self {
        Self {
            repo_path,
            codescan,
        }
    }

    fn resolve_within(&self, args: &Value) -> anyhow::Result<Option<PathBuf>> {
        let Some(raw) = args["within"].as_str() else {
            return Ok(None);
        };
        let rel = Path::new(raw);
        if raw.trim().is_empty()
            || rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("within must be a non-empty repo-relative path inside the repository");
        }
        let root = self
            .repo_path
            .canonicalize()
            .unwrap_or_else(|_| self.repo_path.clone());
        let candidate = root.join(rel);
        let resolved = candidate.canonicalize().unwrap_or(candidate);
        if !resolved.starts_with(&root) {
            anyhow::bail!("within must resolve inside the repository");
        }
        Ok(Some(rel.to_path_buf()))
    }

    fn unavailable_result(&self, code: &str) -> ToolResult {
        ToolResult {
            content: vec![ContentBlock::Text {
                text: "Codescan is unavailable for this workspace.".into(),
            }],
            details: json!({
                "available": false,
                "code": code,
                "service": "service:codescan",
                "root": self.repo_path.display().to_string(),
            }),
        }
    }

    async fn execute_search(
        &self,
        args: &Value,
        cancel: CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("query required"))?;
        let scope_str = args["scope"].as_str().unwrap_or("all");
        let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;
        let within = self.resolve_within(args)?;
        if cancel.is_cancelled() {
            anyhow::bail!("codebase search cancelled");
        }
        let tag_filter: Vec<String> = args["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let scope = SearchScope::parse(scope_str);

        let Some(handle) = self.codescan.handle() else {
            return Ok(self.unavailable_result("service:unavailable"));
        };
        let response = handle
            .invoke(CodescanRequest::Search {
                query: query.into(),
                scope,
                max_results,
                tags: tag_filter,
                within: within.clone(),
                cancellation: cancel,
            })
            .await;
        let (results, indexed_code_chunks, indexed_knowledge_chunks) = match response {
            Ok(CodescanResponse::Search {
                results,
                indexed_code_chunks,
                indexed_knowledge_chunks,
            }) => (results, indexed_code_chunks, indexed_knowledge_chunks),
            Ok(_) => anyhow::bail!("codescan returned an unexpected search response"),
            Err(
                error @ (omegon_traits::ManagedServiceCallError::GenerationDraining
                | omegon_traits::ManagedServiceCallError::GenerationDegraded
                | omegon_traits::ManagedServiceCallError::GenerationRetired),
            ) => {
                return Ok(self.unavailable_result(unavailable_code(&error)));
            }
            Err(omegon_traits::ManagedServiceCallError::Operation(error)) => {
                anyhow::bail!(error)
            }
            Err(error) => anyhow::bail!("codescan search failed: {}", unavailable_code(&error)),
        };

        if results.is_empty() {
            return Ok(ToolResult {
                content: vec![ContentBlock::Text {
                    text: format!(
                        "No results for `{}` (scope: {}, within: {}).",
                        query,
                        scope_str,
                        within
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| ".".into())
                    ),
                }],
                details: json!({"results": [], "query": query, "scope": scope_str, "within": within.as_ref().map(|p| p.display().to_string()), "root": self.repo_path.display().to_string()}),
            });
        }

        // ── Build TUI-safe result list ─────────────────────────────────────
        // Markdown tables looked nice in ideal conditions, but rich previews can
        // still shred row structure in the terminal renderer. Emit a compact,
        // line-oriented format instead: header + one block per result. The
        // structured JSON details remain the authoritative machine-readable form.
        let mut table = format!(
            "## codebase_search: `{}`\n\n**{} result(s)** (scope: `{}`)\n\n",
            query,
            results.len(),
            scope_str
        );

        for r in &results {
            let preview_raw: String = r
                .preview
                .chars()
                .take(300)
                .collect::<String>()
                .replace('\r', "");
            // Indent each line so the preview renders as a recognisable
            // code block under the bullet header, preserving line structure
            // instead of flattening everything into a middot-separated blob.
            let preview_lines: String = preview_raw
                .lines()
                .take(8)
                .map(|l| format!("    {l}"))
                .collect::<Vec<_>>()
                .join("\n");
            table.push_str(&format!(
                "- `{}`:{}-{} · {} · score {:.2}\n{}\n\n",
                r.file,
                r.start_line,
                r.end_line,
                r.chunk_type.as_str(),
                r.score,
                preview_lines
            ));
        }
        table.push_str("*Use `read` with offset/limit for full chunk content.*");

        Ok(ToolResult {
            content: vec![ContentBlock::Text { text: table }],
            details: json!({
                "query": query,
                "scope": scope_str,
                "within": within.as_ref().map(|p| p.display().to_string()),
                "root": self.repo_path.display().to_string(),
                "indexed_code_chunks": indexed_code_chunks,
                "indexed_knowledge_chunks": indexed_knowledge_chunks,
                "results": results.iter().map(|r| json!({
                    "file": r.file,
                    "start_line": r.start_line,
                    "end_line": r.end_line,
                    "chunk_type": r.chunk_type.as_str(),
                    "score": r.score,
                    "label": r.label,
                    "preview": r.preview.chars().take(400).collect::<String>(),
                })).collect::<Vec<_>>(),
            }),
        })
    }

    async fn execute_index(
        &self,
        args: &Value,
        cancel: CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        let invalidate = args["invalidate"].as_bool().unwrap_or(false);
        let Some(handle) = self.codescan.handle() else {
            return Ok(self.unavailable_result("service:unavailable"));
        };
        let stats = match handle
            .invoke(CodescanRequest::Index {
                invalidate,
                cancellation: cancel,
            })
            .await
        {
            Ok(CodescanResponse::Index(stats)) => stats,
            Ok(_) => anyhow::bail!("codescan returned an unexpected index response"),
            Err(
                error @ (omegon_traits::ManagedServiceCallError::GenerationDraining
                | omegon_traits::ManagedServiceCallError::GenerationDegraded
                | omegon_traits::ManagedServiceCallError::GenerationRetired),
            ) => {
                return Ok(self.unavailable_result(unavailable_code(&error)));
            }
            Err(omegon_traits::ManagedServiceCallError::Operation(error)) => {
                anyhow::bail!(error)
            }
            Err(error) => anyhow::bail!("codescan index failed: {}", unavailable_code(&error)),
        };
        let text = format!(
            "## codebase_index\n\n**Status:** {}\n\n\
            | Metric | Count |\n|--------|-------|\n\
            | Code files scanned | {} |\n\
            | Knowledge files scanned | {} |\n\
            | Code chunks indexed | {} |\n\
            | Knowledge chunks indexed | {} |\n\
            | Duration | {}ms |\n",
            if invalidate {
                "Full reindex"
            } else {
                "Incremental"
            },
            stats.code_files,
            stats.knowledge_files,
            stats.code_chunks,
            stats.knowledge_chunks,
            stats.duration_ms,
        );
        Ok(ToolResult {
            content: vec![ContentBlock::Text { text }],
            details: json!({
                "code_files": stats.code_files,
                "knowledge_files": stats.knowledge_files,
                "code_chunks": stats.code_chunks,
                "knowledge_chunks": stats.knowledge_chunks,
                "duration_ms": stats.duration_ms,
            }),
        })
    }
}

#[async_trait]
impl ToolProvider for CodescanProvider {
    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: crate::tool_registry::codescan::CODEBASE_SEARCH.into(),
                label: "codebase_search".into(),
                description: "Search the codebase by concept across code files (functions, structs, classes) and knowledge files (design docs, OpenSpec, memory facts). BM25 ranked. scope: all|code|knowledge. Returns file location, score, and 300-char preview per result.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query — concept, function name, design topic, etc." },
                        "scope": { "type": "string", "enum": ["all", "code", "knowledge"], "description": "Search scope (default: all)" },
                        "max_results": { "type": "number", "description": "Max results (default 10)" },
                        "tags": { "type": "array", "items": {"type": "string"}, "description": "Filter knowledge chunks by frontmatter tags" },
                        "within": { "type": "string", "description": "Repo-relative path prefix to limit returned code and knowledge results. Must stay inside the repository." }
                    },
                    "required": ["query"]
                }),
                capabilities: vec![
                    omegon_traits::ToolCapability::RepoInspection,
                    omegon_traits::ToolCapability::BroadRepoInspection,
                ],
            },
            ToolDefinition {
                name: crate::tool_registry::codescan::CODEBASE_INDEX.into(),
                label: "codebase_index".into(),
                description: "Rebuild the codebase search index. invalidate=true drops the cache and forces a full reindex; default is incremental (skips unchanged files).".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "invalidate": { "type": "boolean", "description": "Drop cache and full reindex (default: false)" }
                    }
                }),
                capabilities: vec![
                    omegon_traits::ToolCapability::RepoInspection,
                    omegon_traits::ToolCapability::BroadRepoInspection,
                ],
            },
        ]
    }

    async fn execute(
        &self,
        tool_name: &str,
        _call_id: &str,
        args: Value,
        cancel: CancellationToken,
    ) -> anyhow::Result<ToolResult> {
        match tool_name {
            crate::tool_registry::codescan::CODEBASE_SEARCH => {
                self.execute_search(&args, cancel).await
            }
            crate::tool_registry::codescan::CODEBASE_INDEX => {
                self.execute_index(&args, cancel).await
            }
            _ => anyhow::bail!("Unknown codescan tool: {tool_name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn managed_provider(path: PathBuf) -> (crate::bus::EventBus, CodescanProvider) {
        let binding = CodescanBinding::default();
        let mut bus = crate::bus::EventBus::new();
        bus.register(Box::new(crate::codescan_service::CodescanFeature::new(
            path.clone(),
            binding.clone(),
        )));
        bus.stage_managed_generation(
            "codescan",
            crate::codescan_service::start_candidate(path.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        bus.try_finalize_managed().await.unwrap();
        binding.capture(&bus).unwrap();
        (bus, CodescanProvider::new(path, binding))
    }

    #[tokio::test]
    async fn tool_definitions_have_correct_names() {
        let dir = tempfile::tempdir().unwrap();
        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let tools = p.tools();
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"codebase_search"));
        assert!(names.contains(&"codebase_index"));
    }

    #[tokio::test]
    async fn execute_index_returns_stats() {
        let dir = tempfile::tempdir().unwrap();
        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let result = p
            .execute("codebase_index", "tc", json!({}), CancellationToken::new())
            .await
            .unwrap();
        let text = match &result.content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => panic!(),
        };
        assert!(text.contains("codebase_index"), "{text}");
    }

    #[tokio::test]
    async fn execute_search_empty_returns_no_results() {
        let dir = tempfile::tempdir().unwrap();
        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let result = p
            .execute(
                "codebase_search",
                "tc",
                json!({"query": "zzz_not_found_12345"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = match &result.content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => panic!(),
        };
        assert!(text.contains("No results"), "{text}");
    }

    #[tokio::test]
    async fn execute_search_within_filters_results() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("alpha")).unwrap();
        std::fs::create_dir_all(dir.path().join("beta")).unwrap();
        std::fs::write(
            dir.path().join("alpha/Needle.java"),
            "public class Needle { public void alphaNeedle() {} }",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("beta/Needle.java"),
            "public class Needle { public void betaNeedle() {} }",
        )
        .unwrap();

        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let result = p
            .execute(
                "codebase_search",
                "tc",
                json!({"query": "Needle", "within": "alpha", "max_results": 10}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let files = result.details["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["file"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(!files.is_empty(), "expected scoped results: {result:?}");
        assert!(files.iter().all(|f| f.starts_with("alpha/")), "{files:?}");
        assert_eq!(result.details["within"], "alpha");
    }

    #[tokio::test]
    async fn execute_search_rejects_within_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let err = p
            .execute(
                "codebase_search",
                "tc",
                json!({"query": "x", "within": "../outside"}),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("within must"), "{err}");
    }

    #[tokio::test]
    async fn execute_search_respects_pre_cancelled_token() {
        let dir = tempfile::tempdir().unwrap();
        let (_bus, p) = managed_provider(dir.path().to_path_buf()).await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = p
            .execute("codebase_search", "tc", json!({"query": "x"}), cancel)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cancelled"), "{err}");
    }

    #[tokio::test]
    async fn unavailable_service_keeps_tools_declared_and_returns_typed_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let p = CodescanProvider::new(dir.path().to_path_buf(), CodescanBinding::default());
        assert_eq!(p.tools().len(), 2);

        let result = p
            .execute(
                "codebase_search",
                "tc",
                json!({"query": "anything"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.details["available"], false);
        assert_eq!(result.details["code"], "service:unavailable");
    }
}
