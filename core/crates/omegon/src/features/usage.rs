//! Usage advisory feature — exposes /usage from existing provider telemetry.
//!
//! Uses only the already-captured `ProviderTelemetrySnapshot` carried on
//! `BusEvent::TurnEnd`. No new upstream calls are made.

use async_trait::async_trait;
use omegon_traits::{
    BusEvent, BusRequest, CommandDefinition, CommandResult, Feature, ProviderTelemetrySnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum UsageHeadroomState {
    Unknown,
    Healthy,
    Elevated,
    Constrained,
    Exhausted,
}

impl UsageHeadroomState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Elevated => "elevated",
            Self::Constrained => "constrained",
            Self::Exhausted => "exhausted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageAuthorityLink {
    label: &'static str,
    url: &'static str,
}

#[derive(Debug, Clone, Default)]
struct LatestUsageSnapshot {
    model: Option<String>,
    provider: Option<String>,
    telemetry: Option<ProviderTelemetrySnapshot>,
}

pub struct UsageFeature {
    latest: LatestUsageSnapshot,
}

impl UsageFeature {
    pub fn new() -> Self {
        Self {
            latest: LatestUsageSnapshot::default(),
        }
    }

    fn format_usage_report(&self) -> String {
        let provider = self.latest.provider.as_deref().unwrap_or("unknown");
        let model = self.latest.model.as_deref().unwrap_or("unknown");
        let telemetry = self.latest.telemetry.as_ref();
        let headroom = derive_headroom_state(telemetry);
        let rationale = derive_rationale(telemetry, &headroom);
        let authority = authoritative_links(provider);

        let mut lines = vec![
            "Usage".to_string(),
            String::new(),
            "Current route".to_string(),
            format!("- provider: {provider}"),
            format!("- model: {model}"),
            String::new(),
            "Raw upstream telemetry".to_string(),
        ];

        match telemetry {
            Some(t) => {
                let raw_lines = format_raw_telemetry_lines(t);
                if raw_lines.is_empty() {
                    lines.push("- none exposed in current session".to_string());
                } else {
                    lines.extend(raw_lines.into_iter().map(|line| format!("- {line}")));
                }
            }
            None => lines.push("- none captured yet in this session".to_string()),
        }

        lines.push(String::new());
        lines.push("Derived advisory".to_string());
        lines.push(format!("- headroom: {}", headroom.as_str()));
        lines.push(format!("- rationale: {rationale}"));

        if !authority.is_empty() {
            lines.push(String::new());
            lines.push("Authority".to_string());
            for link in authority {
                lines.push(format!("- {}: {}", link.label, link.url));
            }
        }

        lines.join("\n")
    }
}

#[async_trait]
impl Feature for UsageFeature {
    fn name(&self) -> &str {
        "usage"
    }

    fn commands(&self) -> Vec<CommandDefinition> {
        vec![CommandDefinition {
            name: "usage".into(),
            description: "Show current provider usage telemetry and advisory".into(),
            subcommands: vec![],
        }]
    }

    fn handle_command(&mut self, name: &str, _args: &str) -> CommandResult {
        if name != "usage" {
            return CommandResult::NotHandled;
        }
        CommandResult::Display(self.format_usage_report())
    }

    fn on_event(&mut self, event: &BusEvent) -> Vec<BusRequest> {
        if let BusEvent::TurnEnd {
            model,
            provider,
            provider_telemetry,
            ..
        } = event
        {
            self.latest.model = model.clone();
            self.latest.provider = provider.clone();
            self.latest.telemetry = provider_telemetry.clone();
        }
        vec![]
    }
}

fn classify_pct(pct: f32) -> UsageHeadroomState {
    if pct >= 98.0 {
        UsageHeadroomState::Exhausted
    } else if pct >= 90.0 {
        UsageHeadroomState::Constrained
    } else if pct >= 70.0 {
        UsageHeadroomState::Elevated
    } else {
        UsageHeadroomState::Healthy
    }
}

fn derive_headroom_state(telemetry: Option<&ProviderTelemetrySnapshot>) -> UsageHeadroomState {
    let Some(t) = telemetry else {
        return UsageHeadroomState::Unknown;
    };

    match t.provider.as_str() {
        "anthropic" => t
            .unified_5h_utilization_pct
            .map(classify_pct)
            .or_else(|| t.unified_7d_utilization_pct.map(classify_pct))
            .unwrap_or(UsageHeadroomState::Unknown),
        "openai-codex" => t
            .codex_primary_pct
            .map(|pct| classify_pct(pct as f32))
            .unwrap_or(UsageHeadroomState::Unknown),
        _ => {
            if let Some(secs) = t.retry_after_secs
                && secs > 0
                && (t.requests_remaining == Some(0) || t.tokens_remaining == Some(0))
            {
                return UsageHeadroomState::Exhausted;
            }
            if let Some(req) = t.requests_remaining
                && req == 0
            {
                return UsageHeadroomState::Exhausted;
            }
            if let Some(tok) = t.tokens_remaining
                && tok == 0
            {
                return UsageHeadroomState::Exhausted;
            }
            if t.retry_after_secs.is_some() {
                return UsageHeadroomState::Constrained;
            }
            if t.requests_remaining.is_some() || t.tokens_remaining.is_some() {
                return UsageHeadroomState::Elevated;
            }
            UsageHeadroomState::Unknown
        }
    }
}

fn derive_rationale(
    telemetry: Option<&ProviderTelemetrySnapshot>,
    headroom: &UsageHeadroomState,
) -> String {
    let Some(t) = telemetry else {
        return "no provider telemetry has been captured yet in this session".to_string();
    };

    match t.provider.as_str() {
        "anthropic" => match (t.unified_5h_utilization_pct, t.unified_7d_utilization_pct) {
            (Some(short), Some(long)) => format!(
                "derived from Anthropic upstream utilization headers: 5h {:.0}% and 7d {:.0}%",
                short, long
            ),
            (Some(short), None) => format!(
                "derived from Anthropic upstream 5h utilization header: {:.0}%",
                short
            ),
            (None, Some(long)) => format!(
                "derived from Anthropic upstream 7d utilization header: {:.0}%",
                long
            ),
            _ => "Anthropic provider selected, but no utilization headers were captured".to_string(),
        },
        "openai-codex" => match t.codex_primary_pct {
            Some(pct) => format!(
                "derived from Codex primary limit pressure header: {}%{}",
                pct,
                t.codex_primary_reset_secs
                    .map(|secs| format!(", reset in {}", format_duration_compact(secs)))
                    .unwrap_or_default()
            ),
            None => "Codex provider selected, but no primary limit header was captured".to_string(),
        },
        _ => {
            let mut parts = Vec::new();
            if let Some(req) = t.requests_remaining {
                parts.push(format!("requests remaining {req}"));
            }
            if let Some(tok) = t.tokens_remaining {
                parts.push(format!("tokens remaining {}", format_compact_tokens(tok)));
            }
            if let Some(secs) = t.retry_after_secs {
                parts.push(format!("retry-after {}", format_duration_compact(secs)));
            }
            if parts.is_empty() {
                format!("no recognized quota headers captured; advisory remains {}", headroom.as_str())
            } else {
                format!(
                    "best-effort advisory from generic quota headers: {}",
                    parts.join(", ")
                )
            }
        }
    }
}

fn format_raw_telemetry_lines(t: &ProviderTelemetrySnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    match t.provider.as_str() {
        "anthropic" => {
            if let Some(pct) = t.unified_5h_utilization_pct {
                lines.push(format!("5h utilization: {:.0}%", pct));
            }
            if let Some(pct) = t.unified_7d_utilization_pct {
                lines.push(format!("7d utilization: {:.0}%", pct));
            }
            if let Some(secs) = t.retry_after_secs {
                lines.push(format!("retry after: {}", format_duration_compact(secs)));
            }
        }
        "openai-codex" => {
            if let Some(name) = &t.codex_limit_name {
                lines.push(format!("model limit: {name}"));
            }
            if let Some(active) = &t.codex_active_limit {
                lines.push(format!("active limit: {active}"));
            }
            if let Some(pct) = t.codex_primary_pct {
                lines.push(format!("primary utilization: {pct}%"));
            }
            if let Some(secs) = t.codex_primary_reset_secs {
                lines.push(format!("primary reset: {}", format_duration_compact(secs)));
            }
            if let Some(secs) = t.codex_secondary_reset_secs {
                lines.push(format!("secondary reset: {}", format_duration_compact(secs)));
            }
            if let Some(unlimited) = t.codex_credits_unlimited {
                lines.push(format!(
                    "credits: {}",
                    if unlimited { "unlimited" } else { "metered" }
                ));
            }
        }
        _ => {
            if let Some(req) = t.requests_remaining {
                lines.push(format!("requests remaining: {req}"));
            }
            if let Some(tok) = t.tokens_remaining {
                lines.push(format!("tokens remaining: {}", format_compact_tokens(tok)));
            }
            if let Some(secs) = t.retry_after_secs {
                lines.push(format!("retry after: {}", format_duration_compact(secs)));
            }
        }
    }
    lines
}

fn authoritative_links(provider: &str) -> Vec<UsageAuthorityLink> {
    match provider {
        "anthropic" => vec![UsageAuthorityLink {
            label: "Anthropic rate limits",
            url: "https://platform.claude.com/docs/en/api/rate-limits",
        }],
        "openai" => vec![UsageAuthorityLink {
            label: "OpenAI API rate limits",
            url: "https://developers.openai.com/api/docs/guides/rate-limits",
        }],
        "openai-codex" => vec![
            UsageAuthorityLink {
                label: "OpenAI help",
                url: "https://help.openai.com/en/?q=rate+limit",
            },
            UsageAuthorityLink {
                label: "OpenAI API rate limits",
                url: "https://developers.openai.com/api/docs/guides/rate-limits",
            },
        ],
        _ => Vec::new(),
    }
}

fn format_duration_compact(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    let rem_mins = mins % 60;
    if hours < 24 {
        if rem_mins > 0 {
            return format!("{hours}h{rem_mins:02}m");
        }
        return format!("{hours}h");
    }
    let days = hours / 24;
    let rem_hours = hours % 24;
    if rem_hours > 0 {
        format!("{days}d{rem_hours}h")
    } else {
        format!("{days}d")
    }
}

fn format_compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.0}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_command_formats_anthropic_with_authority_link() {
        let mut feature = UsageFeature::new();
        feature.on_event(&BusEvent::TurnEnd {
            turn: 1,
            model: Some("anthropic:claude-sonnet-4-6".into()),
            provider: Some("anthropic".into()),
            estimated_tokens: 0,
            context_window: 0,
            context_composition: Default::default(),
            actual_input_tokens: 0,
            actual_output_tokens: 0,
            cache_read_tokens: 0,
            provider_telemetry: Some(ProviderTelemetrySnapshot {
                provider: "anthropic".into(),
                source: "response_headers".into(),
                unified_5h_utilization_pct: Some(42.0),
                unified_7d_utilization_pct: Some(64.0),
                ..Default::default()
            }),
        });

        let CommandResult::Display(text) = feature.handle_command("usage", "") else {
            panic!("expected display result");
        };
        assert!(text.contains("5h utilization: 42%"), "got: {text}");
        assert!(text.contains("7d utilization: 64%"), "got: {text}");
        assert!(text.contains("headroom: healthy"), "got: {text}");
        assert!(
            text.contains("https://platform.claude.com/docs/en/api/rate-limits"),
            "got: {text}"
        );
    }

    #[test]
    fn usage_command_formats_codex_with_help_links() {
        let mut feature = UsageFeature::new();
        feature.on_event(&BusEvent::TurnEnd {
            turn: 1,
            model: Some("openai-codex:gpt-5.4".into()),
            provider: Some("openai-codex".into()),
            estimated_tokens: 0,
            context_window: 0,
            context_composition: Default::default(),
            actual_input_tokens: 0,
            actual_output_tokens: 0,
            cache_read_tokens: 0,
            provider_telemetry: Some(ProviderTelemetrySnapshot {
                provider: "openai-codex".into(),
                source: "response_headers".into(),
                codex_active_limit: Some("codex".into()),
                codex_primary_pct: Some(99),
                codex_primary_reset_secs: Some(13648),
                codex_limit_name: Some("GPT-5.3-Codex-Spark".into()),
                ..Default::default()
            }),
        });

        let CommandResult::Display(text) = feature.handle_command("usage", "") else {
            panic!("expected display result");
        };
        assert!(text.contains("primary utilization: 99%"), "got: {text}");
        assert!(text.contains("headroom: exhausted"), "got: {text}");
        assert!(text.contains("OpenAI help"), "got: {text}");
        assert!(text.contains("developers.openai.com/api/docs/guides/rate-limits"), "got: {text}");
    }

    #[test]
    fn usage_command_handles_missing_telemetry() {
        let feature = UsageFeature::new();
        let CommandResult::Display(text) = ({
            let mut feature = feature;
            feature.handle_command("usage", "")
        }) else {
            panic!("expected display result");
        };
        assert!(text.contains("none captured yet in this session"), "got: {text}");
        assert!(text.contains("headroom: unknown"), "got: {text}");
    }
}
