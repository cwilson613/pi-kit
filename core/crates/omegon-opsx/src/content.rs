//! Renderer-neutral OpenSpec content models and Markdown parsers.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub given: String,
    pub when: String,
    pub then: String,
    pub and_clauses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub title: String,
    pub description: String,
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFile {
    pub domain: String,
    pub file_path: PathBuf,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGroup {
    pub title: String,
    pub specs: Vec<String>,
    pub tasks: Vec<TaskLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLine {
    pub id: String,
    pub stable_id: Option<String>,
    pub description: String,
    pub done: bool,
}

pub fn parse_task_groups(path: &Path) -> Vec<TaskGroup> {
    fs::read_to_string(path)
        .map(|content| parse_task_groups_content(&content))
        .unwrap_or_default()
}

pub fn parse_task_groups_content(content: &str) -> Vec<TaskGroup> {
    let mut groups = Vec::new();
    let mut current: Option<TaskGroup> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("## ") {
            if let Some(group) = current.take() {
                groups.push(group);
            }
            current = Some(TaskGroup {
                title: title.trim().to_string(),
                specs: vec![],
                tasks: vec![],
            });
            continue;
        }
        if let Some(specs) = trimmed
            .strip_prefix("<!-- specs:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            if let Some(group) = current.as_mut() {
                group.specs = specs
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
            }
            continue;
        }
        let done = if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            Some(true)
        } else if trimmed.starts_with("- [ ]") {
            Some(false)
        } else {
            None
        };
        if let Some(done) = done {
            let description = trimmed
                .strip_prefix("- [x]")
                .or_else(|| trimmed.strip_prefix("- [X]"))
                .or_else(|| trimmed.strip_prefix("- [ ]"))
                .unwrap_or(trimmed)
                .trim();
            let stable_id = parse_task_stable_id_marker(description);
            let description = strip_task_stable_id_marker(description);
            let id = description
                .split_whitespace()
                .next()
                .filter(|token| token.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
                .map(|token| token.trim_end_matches('.').to_string())
                .unwrap_or_else(|| description.to_ascii_lowercase().replace(' ', "-"));
            let group = current.get_or_insert_with(|| TaskGroup {
                title: "Tasks".into(),
                specs: vec![],
                tasks: vec![],
            });
            group.tasks.push(TaskLine {
                id,
                stable_id,
                description,
                done,
            });
        }
    }
    if let Some(group) = current {
        groups.push(group);
    }
    groups
}

pub fn parse_specs_dir(specs_dir: &Path) -> Vec<SpecFile> {
    let mut specs = Vec::new();
    collect_specs(specs_dir, specs_dir, &mut specs);
    specs
}

fn collect_specs(root: &Path, directory: &Path, specs: &mut Vec<SpecFile>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_specs(root, &path, specs);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let domain = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            specs.push(SpecFile {
                requirements: parse_spec_content_with_domain(&domain, &content),
                domain,
                file_path: path,
            });
        }
    }
}

pub fn parse_spec_content(content: &str) -> Vec<Requirement> {
    parse_spec_content_with_domain("", content)
}

pub fn parse_spec_content_with_domain(domain: &str, content: &str) -> Vec<Requirement> {
    let mut requirements = Vec::new();
    let mut current_req: Option<(String, String, Vec<Scenario>)> = None;
    let mut current_scenario: Option<ScenarioBuilder> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("### ") && !trimmed.starts_with("#### ") {
            let requirement_title = current_req
                .as_ref()
                .map(|requirement| requirement.0.clone())
                .unwrap_or_default();
            flush_scenario(
                &mut current_scenario,
                current_req.as_mut().map(|r| &mut r.2),
                domain,
                &requirement_title,
            );
            if let Some((title, description, scenarios)) = current_req.take() {
                requirements.push(Requirement {
                    title,
                    description: description.trim().into(),
                    scenarios,
                });
            }
            let title = trimmed[4..]
                .strip_prefix("Requirement:")
                .unwrap_or(&trimmed[4..])
                .trim();
            current_req = Some((title.into(), String::new(), vec![]));
        } else if let Some(after) = trimmed.strip_prefix("#### ") {
            let requirement_title = current_req
                .as_ref()
                .map(|requirement| requirement.0.clone())
                .unwrap_or_default();
            flush_scenario(
                &mut current_scenario,
                current_req.as_mut().map(|r| &mut r.2),
                domain,
                &requirement_title,
            );
            current_scenario = Some(ScenarioBuilder {
                id: None,
                title: after
                    .strip_prefix("Scenario:")
                    .unwrap_or(after)
                    .trim()
                    .into(),
                given: String::new(),
                when_: String::new(),
                then_: String::new(),
                and_clauses: vec![],
            });
        } else if let Some(builder) = current_scenario.as_mut() {
            if let Some(id) = trimmed
                .strip_prefix("<!-- id:")
                .and_then(|rest| rest.strip_suffix("-->"))
            {
                builder.id = Some(id.trim().into());
            } else if let Some(rest) = trimmed.strip_prefix("Given ") {
                builder.given = rest.into();
            } else if let Some(rest) = trimmed.strip_prefix("When ") {
                builder.when_ = rest.into();
            } else if let Some(rest) = trimmed.strip_prefix("Then ") {
                builder.then_ = rest.into();
            } else if let Some(rest) = trimmed.strip_prefix("And ") {
                builder.and_clauses.push(rest.into());
            }
        } else if let Some(req) = current_req.as_mut()
            && !trimmed.is_empty()
        {
            req.1.push_str(trimmed);
            req.1.push('\n');
        }
    }
    let title = current_req
        .as_ref()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    flush_scenario(
        &mut current_scenario,
        current_req.as_mut().map(|r| &mut r.2),
        domain,
        &title,
    );
    if let Some((title, description, scenarios)) = current_req {
        requirements.push(Requirement {
            title,
            description: description.trim().into(),
            scenarios,
        });
    }
    requirements
}

struct ScenarioBuilder {
    id: Option<String>,
    title: String,
    given: String,
    when_: String,
    then_: String,
    and_clauses: Vec<String>,
}
fn flush_scenario(
    builder: &mut Option<ScenarioBuilder>,
    target: Option<&mut Vec<Scenario>>,
    domain: &str,
    requirement: &str,
) {
    if let Some(builder) = builder.take()
        && (!builder.given.is_empty() || !builder.when_.is_empty() || !builder.then_.is_empty())
        && let Some(target) = target
    {
        target.push(Scenario {
            id: builder
                .id
                .unwrap_or_else(|| stable_scenario_id(domain, requirement, &builder.title)),
            title: builder.title,
            given: builder.given,
            when: builder.when_,
            then: builder.then_,
            and_clauses: builder.and_clauses,
        });
    }
}

fn stable_scenario_id(domain: &str, requirement: &str, scenario: &str) -> String {
    format!("{}/{}/{}", slug(domain), slug(requirement), slug(scenario))
}
fn slug(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
pub fn parse_task_stable_id_marker(description: &str) -> Option<String> {
    let rest = &description[description.find("<!-- task-id:")? + 13..];
    let end = rest.find("-->")?;
    let id = rest[..end].trim();
    (!id.is_empty()).then(|| id.into())
}
fn strip_task_stable_id_marker(description: &str) -> String {
    let Some(start) = description.find("<!-- task-id:") else {
        return description.trim().into();
    };
    let rest = &description[start + 13..];
    let Some(end) = rest.find("-->") else {
        return description.trim().into();
    };
    format!(
        "{}{}",
        description[..start].trim_end(),
        rest[end + 3..].trim_start()
    )
    .trim()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_scenarios_with_stable_ids() {
        let requirements = parse_spec_content_with_domain(
            "auth",
            "### Requirement: Login\n#### Scenario: Works\nGiven user\nWhen login\nThen success\n",
        );
        assert_eq!(requirements[0].scenarios[0].id, "auth/login/works");
    }
    #[test]
    fn parses_task_groups_and_markers() {
        let groups = parse_task_groups_content(
            "## 1. Core\n<!-- specs: auth/login -->\n- [x] 1.1 Done <!-- task-id: core.done -->\n",
        );
        assert_eq!(groups[0].tasks[0].stable_id.as_deref(), Some("core.done"));
        assert!(groups[0].tasks[0].done);
    }
}
