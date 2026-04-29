//! Knowledge scanner — markdown heading-hierarchy + JSON/JSONL chunking.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KnowledgeChunk {
    pub path: PathBuf,
    pub heading: String,
    pub start_line: usize,
    pub end_line: usize,
    pub tags: Vec<String>,
    pub text: String,
}

pub struct KnowledgeDirs {
    pub patterns: Vec<String>,
}

impl Default for KnowledgeDirs {
    fn default() -> Self {
        Self {
            patterns: vec![
                "docs/*.md".into(),
                "openspec/**/*.md".into(),
                "openspec/**/*.json".into(),
                ".omegon/*.json".into(),
                ".omegon/*.md".into(),
                "ai/memory/facts.jsonl".into(),
            ],
        }
    }
}

pub struct KnowledgeScanner;

impl KnowledgeScanner {
    pub fn scan_markdown(path: &Path, content: &str) -> Vec<KnowledgeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let (tags, body_start) = extract_frontmatter_tags(content);
        let mut heading_positions: Vec<(usize, String)> = Vec::new();

        for (i, line) in lines.iter().enumerate().skip(body_start) {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("## ") {
                heading_positions.push((i, rest.trim().to_string()));
            } else if let Some(rest) = t.strip_prefix("### ") {
                heading_positions.push((i, rest.trim().to_string()));
            }
        }

        // Insert # title as first chunk if before first ##
        for (i, line) in lines.iter().enumerate().skip(body_start) {
            let t = line.trim();
            if t.starts_with("# ") && !t.starts_with("## ") {
                let first_sub = heading_positions
                    .first()
                    .map(|(l, _)| *l)
                    .unwrap_or(lines.len());
                if i < first_sub {
                    heading_positions.insert(0, (i, t[2..].trim().to_string()));
                }
                break;
            }
        }

        if heading_positions.is_empty() {
            let text = lines[body_start..].join("\n");
            if text.trim().is_empty() {
                return vec![];
            }
            let heading = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document")
                .to_string();
            return vec![KnowledgeChunk {
                path: path.to_path_buf(),
                heading,
                start_line: body_start + 1,
                end_line: lines.len(),
                tags,
                text,
            }];
        }

        let mut chunks = Vec::new();
        for (i, (start, heading)) in heading_positions.iter().enumerate() {
            let end = if i + 1 < heading_positions.len() {
                heading_positions[i + 1].0.saturating_sub(1)
            } else {
                lines.len().saturating_sub(1)
            };
            let text = lines[*start..=end.min(lines.len().saturating_sub(1))].join("\n");
            if text.trim().is_empty() {
                continue;
            }
            chunks.push(KnowledgeChunk {
                path: path.to_path_buf(),
                heading: heading.clone(),
                start_line: start + 1,
                end_line: end + 1,
                tags: tags.clone(),
                text,
            });
        }
        chunks
    }

    pub fn scan_json(path: &Path, content: &str) -> Vec<KnowledgeChunk> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            return vec![];
        };
        let heading = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("json")
            .to_string();
        match value {
            serde_json::Value::Array(arr) => arr
                .into_iter()
                .enumerate()
                .map(|(i, v)| KnowledgeChunk {
                    path: path.to_path_buf(),
                    heading: format!("{} item {}", heading, i),
                    start_line: i + 1,
                    end_line: i + 1,
                    tags: vec![],
                    text: serde_json::to_string_pretty(&v).unwrap_or_default(),
                })
                .collect(),
            other => vec![KnowledgeChunk {
                path: path.to_path_buf(),
                heading,
                start_line: 1,
                end_line: 1,
                tags: vec![],
                text: serde_json::to_string_pretty(&other).unwrap_or_default(),
            }],
        }
    }

    pub fn scan_jsonl(path: &Path, content: &str) -> Vec<KnowledgeChunk> {
        let base = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("jsonl")
            .to_string();
        content
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, line)| {
                let heading = if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    v.get("section")
                        .or_else(|| v.get("heading"))
                        .or_else(|| v.get("title"))
                        .and_then(|s| s.as_str())
                        .map(|s| format!("{}: {}", base, s))
                        .unwrap_or_else(|| format!("{} line {}", base, i + 1))
                } else {
                    format!("{} line {}", base, i + 1)
                };
                KnowledgeChunk {
                    path: path.to_path_buf(),
                    heading,
                    start_line: i + 1,
                    end_line: i + 1,
                    tags: vec![],
                    text: line.to_string(),
                }
            })
            .collect()
    }

    pub fn scan_file(path: &Path, content: &str) -> Vec<KnowledgeChunk> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "md" => Self::scan_markdown(path, content),
            "jsonl" => Self::scan_jsonl(path, content),
            "json" => Self::scan_json(path, content),
            _ => vec![],
        }
    }
}

fn extract_frontmatter_tags(content: &str) -> (Vec<String>, usize) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (vec![], 0);
    }
    let close = lines
        .iter()
        .skip(1)
        .position(|l| l.trim() == "---")
        .map(|i| i + 1);
    let Some(close_idx) = close else {
        return (vec![], 0);
    };
    let mut tags = Vec::new();
    let mut in_tags = false;
    for line in &lines[1..close_idx] {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("tags:") {
            in_tags = true;
            let rest = rest.trim().trim_start_matches('[').trim_end_matches(']');
            for tag in rest.split(',') {
                let t = tag.trim().trim_matches('"').trim_matches('\'');
                if !t.is_empty() {
                    tags.push(t.to_string());
                }
            }
        } else if in_tags && trimmed.starts_with("- ") {
            let t = trimmed[2..].trim().trim_matches('"').trim_matches('\'');
            if !t.is_empty() {
                tags.push(t.to_string());
            }
        } else if !trimmed.starts_with(' ') && !trimmed.starts_with('-') {
            in_tags = false;
        }
    }
    (tags, close_idx + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_markdown_headings() {
        let md = "# Title\n\n## Section A\n\nContent A.\n\n## Section B\n\nContent B.\n";
        let chunks = KnowledgeScanner::scan_markdown(Path::new("doc.md"), md);
        let headings: Vec<&str> = chunks.iter().map(|c| c.heading.as_str()).collect();
        assert!(headings.contains(&"Section A"), "{:?}", headings);
        assert!(headings.contains(&"Section B"), "{:?}", headings);
    }

    #[test]
    fn scan_markdown_frontmatter_tags() {
        let md =
            "---\nid: foo\ntags: [architecture, rust]\n---\n\n# Title\n\n## Section\n\nText.\n";
        let chunks = KnowledgeScanner::scan_markdown(Path::new("doc.md"), md);
        assert!(!chunks.is_empty());
        assert!(
            chunks[0].tags.contains(&"architecture".to_string()),
            "{:?}",
            chunks[0].tags
        );
    }

    #[test]
    fn scan_markdown_no_headings() {
        let md = "Just some text.";
        let chunks = KnowledgeScanner::scan_markdown(Path::new("note.md"), md);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn scan_json_array() {
        let chunks = KnowledgeScanner::scan_json(Path::new("a.json"), r#"[{"x":1},{"x":2}]"#);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn scan_jsonl() {
        let jsonl = "{\"section\":\"Architecture\",\"content\":\"x\"}\n{\"section\":\"Decisions\",\"content\":\"y\"}\n";
        let chunks = KnowledgeScanner::scan_jsonl(Path::new("facts.jsonl"), jsonl);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].heading.contains("Architecture"));
    }

    #[test]
    fn scan_file_dispatch() {
        let md = KnowledgeScanner::scan_file(Path::new("d.md"), "# T\n\n## S\n\nC");
        assert!(!md.is_empty());
        let txt = KnowledgeScanner::scan_file(Path::new("d.txt"), "hello");
        assert!(txt.is_empty());
    }
}
