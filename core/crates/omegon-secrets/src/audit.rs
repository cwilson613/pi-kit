//! Audit log — append-only record of guard decisions.

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::guards::GuardDecision;

pub struct AuditLog {
    path: PathBuf,
}

#[derive(Serialize)]
struct AuditEntry {
    timestamp: String,
    tool: String,
    decision: String,
    reason: String,
    path: String,
}

impl AuditLog {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join("secrets-audit.jsonl"),
        }
    }

    /// Log a guard decision. Failures are silently ignored (audit is best-effort).
    pub fn log_guard(&self, tool_name: &str, _args: &Value, decision: &GuardDecision) {
        let (decision_str, reason, path) = match decision {
            GuardDecision::Block { reason, path } => ("block", reason.as_str(), path.as_str()),
            GuardDecision::Warn { reason, path } => ("warn", reason.as_str(), path.as_str()),
        };

        let entry = AuditEntry {
            timestamp: Utc::now().to_rfc3339(),
            tool: tool_name.to_string(),
            decision: decision_str.to_string(),
            reason: reason.to_string(),
            path: path.to_string(),
        };

        if let Ok(line) = serde_json::to_string(&entry) {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                let _ = writeln!(file, "{line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_guard_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let audit = AuditLog::new(dir.path());

        let decision = GuardDecision::Block {
            reason: "sensitive path".into(),
            path: "/etc/passwd".into(),
        };
        audit.log_guard(
            "bash",
            &serde_json::json!({"command": "cat /etc/passwd"}),
            &decision,
        );

        let content = std::fs::read_to_string(dir.path().join("secrets-audit.jsonl")).unwrap();
        assert!(
            content.contains("\"tool\":\"bash\""),
            "should log tool name: {content}"
        );
        assert!(
            content.contains("\"decision\":\"block\""),
            "should log decision: {content}"
        );
        assert!(
            content.contains("/etc/passwd"),
            "should log path: {content}"
        );
    }

    #[test]
    fn log_guard_warn_entry() {
        let dir = tempfile::tempdir().unwrap();
        let audit = AuditLog::new(dir.path());

        let decision = GuardDecision::Warn {
            reason: "external path".into(),
            path: "/opt/data".into(),
        };
        audit.log_guard("read", &serde_json::json!({}), &decision);

        let content = std::fs::read_to_string(dir.path().join("secrets-audit.jsonl")).unwrap();
        assert!(content.contains("\"decision\":\"warn\""));
    }

    #[test]
    fn log_guard_appends_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        let audit = AuditLog::new(dir.path());

        for i in 0..3 {
            let decision = GuardDecision::Block {
                reason: format!("reason {i}"),
                path: format!("/path/{i}"),
            };
            audit.log_guard("bash", &serde_json::json!({}), &decision);
        }

        let content = std::fs::read_to_string(dir.path().join("secrets-audit.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3, "should have 3 entries");
    }
}
