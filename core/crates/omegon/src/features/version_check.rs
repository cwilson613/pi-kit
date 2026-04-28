//! version_check — Polls GitHub for new Omegon releases.
//!
//! Checks on session start. If a newer version exists, surfaces a TUI
//! notification via BusRequest::Notify. Respects OMEGON_SKIP_VERSION_CHECK
//! and OMEGON_OFFLINE env vars.

use async_trait::async_trait;
use omegon_traits::{BusEvent, BusRequest, Feature, NotifyLevel};
use std::sync::{Arc, Mutex};

const REPO_OWNER: &str = "styrene-lab";
const REPO_NAME: &str = "omegon";
const FETCH_TIMEOUT_SECS: u64 = 10;

pub struct VersionCheck {
    current_version: String,
    checked: bool,
    /// Result slot: the spawned async task writes the update message here.
    /// The next on_event call picks it up and returns a BusRequest::Notify.
    pending_notification: Arc<Mutex<Option<String>>>,
}

impl VersionCheck {
    pub fn new(current_version: impl Into<String>) -> Self {
        Self {
            current_version: current_version.into(),
            checked: false,
            pending_notification: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Feature for VersionCheck {
    fn name(&self) -> &str {
        "version-check"
    }

    fn on_event(&mut self, event: &BusEvent) -> Vec<BusRequest> {
        // Check for pending notification from the async task.
        // This fires on any subsequent event after the task completes.
        if let Ok(mut slot) = self.pending_notification.lock()
            && let Some(msg) = slot.take() {
                return vec![BusRequest::Notify {
                    message: msg,
                    level: NotifyLevel::Info,
                }];
            }

        if let BusEvent::SessionStart { .. } = event {
            if self.checked {
                return vec![];
            }
            self.checked = true;

            if std::env::var("OMEGON_SKIP_VERSION_CHECK").is_ok()
                || std::env::var("OMEGON_OFFLINE").is_ok()
            {
                return vec![];
            }

            let current = self.current_version.clone();
            let slot = self.pending_notification.clone();
            crate::task_spawn::spawn_best_effort("version-check", async move {
                match fetch_latest().await {
                    Some(latest) if is_newer(&latest, &current) => {
                        let msg = format!(
                            "Update available: v{current} → v{latest}. Run /update to install."
                        );
                        tracing::info!(current = %current, latest = %latest, "{msg}");
                        if let Ok(mut s) = slot.lock() {
                            *s = Some(msg);
                        }
                    }
                    _ => {
                        tracing::debug!("Version check: up to date");
                    }
                }
            });
        }
        vec![]
    }
}

async fn fetch_latest() -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .user_agent("omegon-version-check")
        .build()
        .ok()?;

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body["tag_name"]
        .as_str()
        .map(|s| s.strip_prefix('v').unwrap_or(s).to_string())
}

/// Compare dotted version strings. Returns true if `latest` > `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    let len = l.len().max(c.len());
    for i in 0..len {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.13.0", "0.12.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.12.0", "0.12.0"));
        assert!(!is_newer("0.11.0", "0.12.0"));
        assert!(is_newer("0.12.1", "0.12.0"));
    }

    #[test]
    fn version_with_prefix() {
        // The fetch strips 'v' prefix before comparison
        assert!(is_newer("0.13.0", "0.12.0"));
    }

    #[tokio::test]
    async fn checked_flag_prevents_duplicate_check() {
        let mut vc = VersionCheck::new("0.12.0");
        // First session start sets checked=true
        let _ = vc.on_event(&BusEvent::SessionStart {
            cwd: "/tmp".into(),
            session_id: "test-1".into(),
        });
        assert!(vc.checked, "should be marked as checked after first event");

        // Second session start returns empty (already checked)
        let requests = vc.on_event(&BusEvent::SessionStart {
            cwd: "/tmp".into(),
            session_id: "test-2".into(),
        });
        assert!(requests.is_empty(), "should skip duplicate check");
    }

    #[test]
    fn pending_notification_drains_on_next_event() {
        let mut vc = VersionCheck::new("0.12.0");
        // Simulate the async task writing a result
        *vc.pending_notification.lock().unwrap() =
            Some("Update available: v0.12.0 → v0.13.0. Run /update to install.".to_string());

        // Next event should drain the slot and return a Notify request
        let requests = vc.on_event(&BusEvent::TurnEnd {
            turn: 1,
            model: None,
            provider: None,
            estimated_tokens: 0,
            context_window: 200_000,
            context_composition: Default::default(),
            actual_input_tokens: 0,
            actual_output_tokens: 0,
            cache_read_tokens: 0,
            provider_telemetry: None,
            dominant_phase: None,
            drift_kind: None,
            progress_signal: omegon_traits::ProgressSignal::None,
        });
        assert_eq!(requests.len(), 1);
        assert!(
            matches!(&requests[0], BusRequest::Notify { message, .. } if message.contains("v0.13.0")),
            "should notify about available update"
        );

        // Slot is now empty — next event returns nothing
        let requests = vc.on_event(&BusEvent::TurnEnd {
            turn: 2,
            model: None,
            provider: None,
            estimated_tokens: 0,
            context_window: 200_000,
            context_composition: Default::default(),
            actual_input_tokens: 0,
            actual_output_tokens: 0,
            cache_read_tokens: 0,
            provider_telemetry: None,
            dominant_phase: None,
            drift_kind: None,
            progress_signal: omegon_traits::ProgressSignal::None,
        });
        assert!(
            requests.is_empty(),
            "slot should be drained after first read"
        );
    }
}
