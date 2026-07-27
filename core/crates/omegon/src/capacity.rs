//! Read-only account-capacity probes and normalized hard-bucket observations.
//!
//! Probes never submit inference. Codex uses its authenticated app-server
//! protocol; Claude uses the OAuth usage endpoint when an Omegon-managed OAuth
//! credential is available.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CAPACITY_TTL: Duration = Duration::from_secs(60);
static CAPACITY_CACHE: OnceLock<tokio::sync::Mutex<HashMap<String, CapacityObservation>>> =
    OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityObservation {
    pub provider: String,
    pub plan: Option<String>,
    pub source: String,
    pub observed_at: u64,
    pub windows: Vec<CapacityWindow>,
    pub credits: Option<CapacityCredits>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f32,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityCredits {
    pub enabled: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

impl CapacityObservation {
    pub fn is_stale(&self, ttl: Duration) -> bool {
        now_epoch().saturating_sub(self.observed_at) > ttl.as_secs()
    }
}

pub async fn observe(provider: &str, force: bool) -> CapacityObservation {
    let cache = CAPACITY_CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().await;
    if !force
        && let Some(observation) = cache.get(provider)
        && !observation.is_stale(CAPACITY_TTL)
    {
        return observation.clone();
    }
    // Holding the provider cache lock intentionally coalesces concurrent refreshes.
    let observation = probe(provider).await;
    if observation.error.is_none() || !cache.contains_key(provider) {
        cache.insert(provider.to_string(), observation.clone());
        observation
    } else {
        let mut stale = cache[provider].clone();
        stale.error = observation.error;
        stale
    }
}

pub async fn probe(provider: &str) -> CapacityObservation {
    match provider {
        "openai-codex" => probe_codex().await,
        "anthropic" => probe_claude().await,
        other => unavailable(other, "no account-capacity adapter is installed"),
    }
}

async fn probe_codex() -> CapacityObservation {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::Command;

    let mut child = match Command::new("codex")
        .args(["app-server", "--stdio"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return unavailable(
                "openai-codex",
                &format!("codex app-server unavailable: {error}"),
            );
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        return unavailable("openai-codex", "codex app-server stdin unavailable");
    };
    let Some(stdout) = child.stdout.take() else {
        return unavailable("openai-codex", "codex app-server stdout unavailable");
    };
    let request = concat!(
        "{\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"omegon\",\"title\":\"Omegon\",\"version\":\"0.29.0\"}}}\n",
        "{\"method\":\"initialized\",\"params\":{}}\n",
        "{\"id\":2,\"method\":\"account/rateLimits/read\",\"params\":null}\n"
    );
    if let Err(error) = stdin.write_all(request.as_bytes()).await {
        return unavailable("openai-codex", &format!("capacity request failed: {error}"));
    }
    let mut lines = BufReader::new(stdout).lines();
    let response = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(line) = lines.next_line().await? {
            let value: serde_json::Value = serde_json::from_str(&line)?;
            if value.get("id").and_then(|id| id.as_u64()) == Some(2) {
                return Ok::<_, anyhow::Error>(value);
            }
        }
        anyhow::bail!("codex app-server closed before capacity response")
    })
    .await;
    let _ = child.kill().await;
    let value = match response {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => return unavailable("openai-codex", &error.to_string()),
        Err(_) => return unavailable("openai-codex", "capacity request timed out"),
    };
    parse_codex(&value)
}

fn parse_codex(value: &serde_json::Value) -> CapacityObservation {
    let result = &value["result"];
    let mut windows = Vec::new();
    let buckets = result["rateLimitsByLimitId"].as_object();
    if let Some(buckets) = buckets {
        for (id, bucket) in buckets {
            for (suffix, window) in [
                ("primary", &bucket["primary"]),
                ("secondary", &bucket["secondary"]),
            ] {
                if let Some(used) = window["usedPercent"].as_f64() {
                    let name = bucket["limitName"].as_str().unwrap_or(id);
                    windows.push(CapacityWindow {
                        id: format!("{id}:{suffix}"),
                        label: if suffix == "primary" {
                            name.to_string()
                        } else {
                            format!("{name} secondary")
                        },
                        used_percent: used as f32,
                        window_minutes: window["windowDurationMins"].as_u64(),
                        resets_at: window["resetsAt"].as_u64(),
                    });
                }
            }
        }
    }
    let legacy = &result["rateLimits"];
    let credits = legacy["credits"]
        .as_object()
        .map(|credits| CapacityCredits {
            enabled: credits
                .get("hasCredits")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            unlimited: credits
                .get("unlimited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            balance: credits
                .get("balance")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        });
    CapacityObservation {
        provider: "openai-codex".into(),
        plan: legacy["planType"].as_str().map(str::to_string),
        source: "codex app-server account/rateLimits/read".into(),
        observed_at: now_epoch(),
        windows,
        credits,
        error: value.get("error").map(ToString::to_string),
    }
}

async fn probe_claude() -> CapacityObservation {
    let Some((token, oauth)) = crate::auth::resolve_with_refresh("anthropic").await else {
        return unavailable("anthropic", "Anthropic credential unavailable");
    };
    if !oauth {
        return unavailable(
            "anthropic",
            "Claude subscription usage requires OAuth authentication",
        );
    }
    let response = match reqwest::Client::new()
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(token)
        .header("anthropic-version", "2023-06-01")
        .timeout(Duration::from_secs(15))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return unavailable(
                "anthropic",
                &format!("Claude usage request failed: {error}"),
            );
        }
    };
    if !response.status().is_success() {
        return unavailable(
            "anthropic",
            &format!("Claude usage endpoint returned {}", response.status()),
        );
    }
    match response.json::<serde_json::Value>().await {
        Ok(value) => parse_claude(&value),
        Err(error) => unavailable(
            "anthropic",
            &format!("Claude usage response was invalid: {error}"),
        ),
    }
}

fn parse_claude(value: &serde_json::Value) -> CapacityObservation {
    let mut windows = Vec::new();
    for (id, label) in [
        ("five_hour", "Current session"),
        ("seven_day", "Current week"),
        ("seven_day_sonnet", "Sonnet weekly"),
        ("seven_day_opus", "Opus weekly"),
        ("seven_day_overage_included", "Included overage weekly"),
    ] {
        let bucket = &value[id];
        if let Some(used) = bucket["utilization"].as_f64() {
            windows.push(CapacityWindow {
                id: id.into(),
                label: label.into(),
                used_percent: used as f32,
                window_minutes: None,
                resets_at: bucket["resets_at"]
                    .as_u64()
                    .or_else(|| bucket["resets_at"].as_str().and_then(parse_rfc3339_epoch)),
            });
        }
    }
    CapacityObservation {
        provider: "anthropic".into(),
        plan: value["subscription_type"].as_str().map(str::to_string),
        source: "Claude OAuth GET /api/oauth/usage".into(),
        observed_at: now_epoch(),
        windows,
        credits: value
            .get("extra_usage")
            .and_then(|v| v.as_object())
            .map(|extra| CapacityCredits {
                enabled: extra
                    .get("is_enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                unlimited: false,
                balance: extra.get("monthly_limit").map(ToString::to_string),
            }),
        error: None,
    }
}

fn parse_rfc3339_epoch(value: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

fn unavailable(provider: &str, error: &str) -> CapacityObservation {
    CapacityObservation {
        provider: provider.into(),
        plan: None,
        source: "account capacity probe".into(),
        observed_at: now_epoch(),
        windows: Vec::new(),
        credits: None,
        error: Some(error.into()),
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_multi_bucket_response() {
        let value = serde_json::json!({"id":2,"result":{"rateLimits":{"planType":"pro","credits":{"hasCredits":false,"unlimited":false,"balance":"0"}},"rateLimitsByLimitId":{"codex":{"limitName":null,"primary":{"usedPercent":17,"windowDurationMins":10080,"resetsAt":123}},"spark":{"limitName":"Spark","primary":{"usedPercent":0,"windowDurationMins":10080,"resetsAt":456}}}}});
        let observation = parse_codex(&value);
        assert_eq!(observation.plan.as_deref(), Some("pro"));
        assert_eq!(observation.windows.len(), 2);
        assert_eq!(observation.windows[0].used_percent, 17.0);
    }

    #[test]
    fn parses_claude_usage_windows() {
        let value = serde_json::json!({"five_hour":{"utilization":12.5,"resets_at":"2026-08-01T12:00:00Z"},"seven_day":{"utilization":60.0,"resets_at":123}});
        let observation = parse_claude(&value);
        assert_eq!(observation.windows.len(), 2);
        assert_eq!(observation.windows[0].label, "Current session");
    }
}
