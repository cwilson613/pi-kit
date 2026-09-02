use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

mod invocation;

pub use invocation::{
    BoundedToolCallError, InvocationLeaseState, InvocationLeaseStateMachine,
    InvocationLeaseTransitionError, ToolBudgetExhausted, ToolInvocationBudget,
    execute_bounded_tool_call,
};

const TASK_BYTES_LIMIT: u64 = 64 * 1024;
const MANIFEST_BYTES_LIMIT: u64 = 256 * 1024;
const RESPONSE_BYTES_LIMIT: usize = 1024 * 1024;
const SSE_EVENT_BYTES_LIMIT: usize = 64 * 1024;
const SSE_EVENT_COUNT_LIMIT: usize = 1024;
const SUMMARY_BYTES_LIMIT: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("task admission failed: {0}")]
    TaskAdmission(String),
    #[error("project route admission failed: {0}")]
    RouteAdmission(String),
    #[error("project endpoint credential is unavailable: {0}")]
    SecretResolution(String),
    #[error("authority persistence failed: {0}")]
    Authority(#[source] io::Error),
    #[error("provider transport failed: {0}")]
    Transport(String),
    #[error("provider turn timed out")]
    TimedOut,
    #[error("provider turn cancelled")]
    Cancelled,
    #[error("provider turn failed and terminal authority closure failed: {0}")]
    Closure(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Revoked,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStarted {
    pub turn_id: Uuid,
    pub prompt_id: Uuid,
    pub runtime_generation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteLeaseRecorded {
    pub lease_id: Uuid,
    pub request_id: Uuid,
    pub turn_id: Uuid,
    pub selected_provider_id: String,
    pub selected_model_id: String,
    pub serving_provider_id: String,
    pub serving_model_id: String,
    pub schema_dialect: String,
    pub credential_source_class: String,
    pub fallback_reason: Option<String>,
    pub contribution_generation_id: String,
    pub route_policy: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnClosed {
    pub turn_id: Uuid,
    pub outcome: TurnOutcome,
    pub reason_code: String,
    pub recovery_rule_version: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    pub status: String,
    pub turns: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
    pub duration_secs: f64,
    pub summary: String,
    pub error: Option<String>,
}

impl RunResult {
    pub fn exit_code(&self) -> i32 {
        match self.status.as_str() {
            "completed" => 0,
            "error" => 1,
            "exhausted" => 2,
            "timeout" => 3,
            _ => 1,
        }
    }

    fn error(
        turns: u32,
        started_at: Instant,
        error: &RuntimeError,
        token_budget: Option<u64>,
    ) -> Self {
        let status = if matches!(error, RuntimeError::TimedOut) {
            "timeout"
        } else {
            "error"
        };
        Self {
            status: status.into(),
            turns,
            total_input_tokens: 0,
            total_output_tokens: 0,
            token_budget,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            duration_secs: started_at.elapsed().as_secs_f64(),
            summary: String::new(),
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskFile {
    task: TaskSection,
    bounds: BoundsSection,
    agent: AgentSection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskSection {
    prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundsSection {
    max_turns: u32,
    timeout_secs: u64,
    #[serde(default)]
    token_budget: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentSection {
    model: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InferenceManifest {
    schema_version: u32,
    #[serde(default)]
    endpoints: Vec<EndpointRecord>,
    #[serde(default)]
    offerings: Vec<OfferingRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointRecord {
    id: String,
    adapter: String,
    transport: TransportRecord,
    secret_refs: Vec<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum TransportRecord {
    Http { base_url: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OfferingRecord {
    id: String,
    endpoint: String,
    native_model_id: String,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    #[serde(default)]
    capabilities: OfferingCapabilities,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct OfferingCapabilities {
    #[serde(default)]
    tools: bool,
    #[serde(default)]
    reasoning: bool,
}

#[derive(Debug)]
struct AdmittedRoute {
    offering_id: String,
    endpoint_id: String,
    native_model_id: String,
    base_url: Url,
    secret: String,
}

#[derive(Debug, Default, Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    delta: ChatDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatDelta {
    content: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct ProviderResult {
    summary: String,
    input_tokens: u64,
    output_tokens: u64,
    finish_reason: String,
}

#[derive(Debug, PartialEq, Eq)]
enum StartedTurnResult {
    Completed {
        provider: ProviderResult,
        requests: u32,
    },
    Exhausted {
        provider: ProviderResult,
        requests: u32,
        error: String,
    },
}

#[derive(Serialize)]
struct AuthorityEnvelope<'a, T> {
    envelope_version: u16,
    event_id: Uuid,
    session_id: Uuid,
    stream_id: Uuid,
    sequence: u64,
    event_type: &'a str,
    event_version: u16,
    command_id: Uuid,
    command_fingerprint: &'a str,
    causation_event_id: Option<Uuid>,
    recorded_at: String,
    payload: &'a T,
}

struct AuthorityWriter {
    file: File,
    session_id: Uuid,
    stream_id: Uuid,
    sequence: u64,
    command_id: Uuid,
    command_fingerprint: String,
}

pub async fn run_task(workspace: &Path, task_path: &Path) -> Result<RunResult, RuntimeError> {
    let started_at = Instant::now();
    let (task, route, fingerprint) = match admit_run(workspace, task_path) {
        Ok(admitted) => admitted,
        Err(error)
            if matches!(
                error,
                RuntimeError::TaskAdmission(_)
                    | RuntimeError::RouteAdmission(_)
                    | RuntimeError::SecretResolution(_)
            ) =>
        {
            return Ok(RunResult::error(0, started_at, &error, None));
        }
        Err(error) => return Err(error),
    };

    let mut authority = AuthorityWriter::create(&fingerprint)?;
    let prompt_id = Uuid::new_v4();
    let turn_id = Uuid::new_v4();
    authority.append(
        "session.created",
        &json!({
            "workspace_identity": sha256_hex(workspace.as_os_str().as_encoded_bytes()),
            "created_by": {"principal": "kernel-host", "ingress": "bounded-task"},
            "runtime_generation_id": "omegon-kernel-runtime-v1"
        }),
    )?;
    authority.append(
        "prompt.admitted",
        &json!({
            "submission_id": Uuid::new_v4(),
            "prompt_id": prompt_id,
            "principal": "kernel-host",
            "ingress": "bounded-task",
            "queue_mode": "immediate",
            "content": {"text": task.task.prompt, "attachments": []},
            "metadata": {"task_path": task_path.to_string_lossy()}
        }),
    )?;
    authority.append(
        "turn.started",
        &TurnStarted {
            turn_id,
            prompt_id,
            runtime_generation_id: "omegon-kernel-runtime-v1".into(),
        },
    )?;

    let token_budget = task.bounds.token_budget;
    let started = run_started_turn(
        &mut authority,
        turn_id,
        &route,
        &task.task.prompt,
        task.bounds.max_turns,
        token_budget,
        Duration::from_secs(task.bounds.timeout_secs),
    )
    .await;
    match started {
        Ok(StartedTurnResult::Completed { provider, requests }) => Ok(RunResult {
            status: "completed".into(),
            turns: requests,
            total_input_tokens: provider.input_tokens,
            total_output_tokens: provider.output_tokens,
            token_budget,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            duration_secs: started_at.elapsed().as_secs_f64(),
            summary: provider.summary,
            error: None,
        }),
        Ok(StartedTurnResult::Exhausted {
            provider,
            requests,
            error,
        }) => Ok(RunResult {
            status: "exhausted".into(),
            turns: requests,
            total_input_tokens: provider.input_tokens,
            total_output_tokens: provider.output_tokens,
            token_budget,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            duration_secs: started_at.elapsed().as_secs_f64(),
            summary: provider.summary,
            error: Some(error),
        }),
        Err(error @ RuntimeError::Closure(_)) => Err(error),
        Err(error) => Ok(RunResult::error(1, started_at, &error, token_budget)),
    }
}

fn admit_run(
    workspace: &Path,
    task_path: &Path,
) -> Result<(TaskFile, AdmittedRoute, String), RuntimeError> {
    let task_bytes =
        read_bounded(task_path, TASK_BYTES_LIMIT, "task").map_err(RuntimeError::TaskAdmission)?;
    let task: TaskFile = toml::from_str(
        std::str::from_utf8(&task_bytes)
            .map_err(|_| RuntimeError::TaskAdmission("task is not UTF-8".into()))?,
    )
    .map_err(|error| RuntimeError::TaskAdmission(format!("invalid TOML: {error}")))?;
    admit_task(&task)?;

    let manifest_path = workspace.join(".omegon/inference.toml");
    let manifest_bytes = read_bounded(
        &manifest_path,
        MANIFEST_BYTES_LIMIT,
        "project inference manifest",
    )
    .map_err(RuntimeError::RouteAdmission)?;
    let manifest: InferenceManifest = toml::from_str(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| RuntimeError::RouteAdmission("manifest is not UTF-8".into()))?,
    )
    .map_err(|error| RuntimeError::RouteAdmission(format!("invalid manifest TOML: {error}")))?;
    let route = admit_route(manifest, &task.agent.model)?;
    let fingerprint = sha256_hex(&task_bytes);
    Ok((task, route, fingerprint))
}

async fn run_started_turn(
    authority: &mut AuthorityWriter,
    turn_id: Uuid,
    route: &AdmittedRoute,
    prompt: &str,
    max_turns: u32,
    token_budget: Option<u64>,
    timeout: Duration,
) -> Result<StartedTurnResult, RuntimeError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut aggregate = ProviderResult {
        summary: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        finish_reason: String::new(),
    };
    let mut next_input_tokens = 0;

    let mut request_index = 0;
    loop {
        if turn_budget_prevents_request(request_index, max_turns) {
            authority
                .append(
                    "turn.closed",
                    &TurnClosed {
                        turn_id,
                        outcome: TurnOutcome::Failed,
                        reason_code: "turn_budget_exhausted".into(),
                        recovery_rule_version: None,
                    },
                )
                .map_err(|error| {
                    RuntimeError::Closure(format!(
                        "turn exhaustion terminal authority closure failed: {error}"
                    ))
                })?;
            return Ok(StartedTurnResult::Exhausted {
                provider: aggregate,
                requests: request_index,
                error: format!("turn budget exhausted: admitted {max_turns}"),
            });
        }
        if deadline_prevents_request(tokio::time::Instant::now(), deadline) {
            return fail_and_close(
                authority,
                turn_id,
                RuntimeError::TimedOut,
                TurnOutcome::TimedOut,
                "provider_timeout",
            );
        }
        let observed_tokens = aggregate
            .input_tokens
            .saturating_add(aggregate.output_tokens);
        if request_index > 0
            && let Some(admitted) = token_budget
            && token_budget_prevents_request(observed_tokens, next_input_tokens, admitted)
        {
            authority
                .append(
                    "turn.closed",
                    &TurnClosed {
                        turn_id,
                        outcome: TurnOutcome::Failed,
                        reason_code: "token_budget_exhausted".into(),
                        recovery_rule_version: None,
                    },
                )
                .map_err(|error| {
                    RuntimeError::Closure(format!(
                        "token exhaustion terminal authority closure failed: {error}"
                    ))
                })?;
            return Ok(StartedTurnResult::Exhausted {
                provider: aggregate,
                requests: request_index,
                error: format!(
                    "token budget exhausted: observed {observed_tokens} tokens; admitted {admitted}"
                ),
            });
        }

        let request_id = Uuid::new_v4();
        let lease_id = Uuid::new_v4();
        let pre_dispatch = (|| {
            authority.append(
                "route.lease_recorded",
                &RouteLeaseRecorded {
                    lease_id,
                    request_id,
                    turn_id,
                    selected_provider_id: route.endpoint_id.clone(),
                    selected_model_id: route.offering_id.clone(),
                    serving_provider_id: route.endpoint_id.clone(),
                    serving_model_id: route.native_model_id.clone(),
                    schema_dialect: "open_ai".into(),
                    credential_source_class: "project_endpoint_environment".into(),
                    fallback_reason: None,
                    contribution_generation_id: "project-inference-manifest-v1".into(),
                    route_policy: "exact_offering".into(),
                },
            )?;
            authority.append(
                "route.endpoint_provenance_recorded",
                &json!({
                    "lease_id": lease_id,
                    "endpoint_id": route.endpoint_id,
                    "adapter_id": "chat-completions",
                    "inventory_generation": 1
                }),
            )
        })();
        if let Err(error) = pre_dispatch {
            return fail_and_close(
                authority,
                turn_id,
                RuntimeError::Authority(error),
                TurnOutcome::Failed,
                "authority_pre_dispatch_failed",
            );
        }

        let provider = tokio::select! {
            dispatched = tokio::time::timeout_at(deadline, dispatch(route, prompt)) => match dispatched {
                Ok(Ok(provider)) => provider,
                Ok(Err(error)) => return fail_and_close(
                    authority,
                    turn_id,
                    error,
                    TurnOutcome::Failed,
                    "provider_transport_failed",
                ),
                Err(_) => return fail_and_close(
                    authority,
                    turn_id,
                    RuntimeError::TimedOut,
                    TurnOutcome::TimedOut,
                    "provider_timeout",
                ),
            },
            cancellation = tokio::signal::ctrl_c() => match cancellation {
                Ok(()) => return fail_and_close(
                    authority,
                    turn_id,
                    RuntimeError::Cancelled,
                    TurnOutcome::Cancelled,
                    "provider_cancelled",
                ),
                Err(error) => return fail_and_close(
                    authority,
                    turn_id,
                    RuntimeError::Transport(format!("cancellation handler failed: {error}")),
                    TurnOutcome::Failed,
                    "cancellation_handler_failed",
                ),
            },
        };
        aggregate.input_tokens = aggregate.input_tokens.saturating_add(provider.input_tokens);
        aggregate.output_tokens = aggregate
            .output_tokens
            .saturating_add(provider.output_tokens);
        if aggregate
            .summary
            .len()
            .saturating_add(provider.summary.len())
            > SUMMARY_BYTES_LIMIT
        {
            return fail_and_close(
                authority,
                turn_id,
                RuntimeError::Transport("cumulative summary byte bound exceeded".into()),
                TurnOutcome::Failed,
                "provider_transport_failed",
            );
        }
        next_input_tokens = provider.input_tokens;
        aggregate.summary.push_str(&provider.summary);
        aggregate.finish_reason = provider.finish_reason;

        if aggregate.finish_reason != "length" {
            authority
                .append(
                    "turn.closed",
                    &TurnClosed {
                        turn_id,
                        outcome: TurnOutcome::Completed,
                        reason_code: "completed".into(),
                        recovery_rule_version: None,
                    },
                )
                .map_err(|error| {
                    RuntimeError::Closure(format!(
                        "completed turn terminal authority closure failed: {error}"
                    ))
                })?;
            return Ok(StartedTurnResult::Completed {
                provider: aggregate,
                requests: request_index + 1,
            });
        }
        request_index += 1;
    }
}

fn turn_budget_prevents_request(observed_requests: u32, admitted: u32) -> bool {
    observed_requests >= admitted
}

fn deadline_prevents_request(now: tokio::time::Instant, deadline: tokio::time::Instant) -> bool {
    now >= deadline
}

fn token_budget_prevents_request(
    observed_tokens: u64,
    next_input_tokens: u64,
    admitted: u64,
) -> bool {
    observed_tokens >= admitted || observed_tokens.saturating_add(next_input_tokens) > admitted
}

fn fail_and_close<T>(
    authority: &mut AuthorityWriter,
    turn_id: Uuid,
    error: RuntimeError,
    outcome: TurnOutcome,
    reason_code: &str,
) -> Result<T, RuntimeError> {
    match authority.append(
        "turn.closed",
        &TurnClosed {
            turn_id,
            outcome,
            reason_code: reason_code.into(),
            recovery_rule_version: None,
        },
    ) {
        Ok(()) => Err(error),
        Err(close) => Err(RuntimeError::Closure(format!("{error}; {close}"))),
    }
}

async fn dispatch(route: &AdmittedRoute, prompt: &str) -> Result<ProviderResult, RuntimeError> {
    let endpoint = chat_completions_url(&route.base_url);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| RuntimeError::Transport(error.to_string()))?;
    let response = client
        .post(endpoint)
        .bearer_auth(&route.secret)
        .json(&json!({
            "model": route.native_model_id,
            "messages": [{"role": "user", "content": prompt}],
            "stream": true
        }))
        .send()
        .await
        .map_err(|error| RuntimeError::Transport(error.to_string()))?;
    let status = response.status();
    let is_sse = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream"));
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| RuntimeError::Transport(error.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_BYTES_LIMIT {
            return Err(RuntimeError::Transport(
                "response byte bound exceeded".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        return Err(RuntimeError::Transport(format!(
            "provider returned HTTP {status}"
        )));
    }
    if !is_sse {
        return Err(RuntimeError::Transport(
            "provider response is not text/event-stream".into(),
        ));
    }
    parse_sse(&bytes)
}

fn chat_completions_url(base_url: &Url) -> Url {
    let mut endpoint = base_url.clone();
    endpoint.set_path(&format!(
        "{}/chat/completions",
        base_url.path().trim_end_matches('/')
    ));
    endpoint
}

fn parse_sse(bytes: &[u8]) -> Result<ProviderResult, RuntimeError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RuntimeError::Transport("SSE response is not UTF-8".into()))?;
    let normalized = text.replace("\r\n", "\n");
    let mut summary = String::new();
    let mut usage = ChatUsage::default();
    let mut finish_reason = None;
    let mut count = 0_usize;
    for event in normalized.split("\n\n") {
        if event.trim().is_empty() {
            continue;
        }
        count += 1;
        if count > SSE_EVENT_COUNT_LIMIT || event.len() > SSE_EVENT_BYTES_LIMIT {
            return Err(RuntimeError::Transport("SSE event bound exceeded".into()));
        }
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            continue;
        }
        let chunk: ChatChunk = serde_json::from_str(&data)
            .map_err(|error| RuntimeError::Transport(format!("invalid SSE data: {error}")))?;
        if let Some(value) = chunk.usage {
            usage = value;
        }
        for choice in chunk.choices {
            if let Some(content) = choice.delta.content {
                if summary.len().saturating_add(content.len()) > SUMMARY_BYTES_LIMIT {
                    return Err(RuntimeError::Transport(
                        "summary byte bound exceeded".into(),
                    ));
                }
                summary.push_str(&content);
            }
            if let Some(reason) = choice.finish_reason.filter(|reason| !reason.is_empty()) {
                finish_reason = Some(reason);
            }
        }
    }
    let finish_reason = finish_reason.ok_or_else(|| {
        RuntimeError::Transport("SSE response lacks a terminal finish_reason".into())
    })?;
    Ok(ProviderResult {
        summary,
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        finish_reason,
    })
}

fn admit_task(task: &TaskFile) -> Result<(), RuntimeError> {
    if task.task.prompt.trim().is_empty() {
        return Err(RuntimeError::TaskAdmission(
            "prompt must not be empty".into(),
        ));
    }
    if task.bounds.max_turns == 0 || task.bounds.max_turns > 30 {
        return Err(RuntimeError::TaskAdmission(
            "max_turns must be between 1 and 30".into(),
        ));
    }
    if task.bounds.timeout_secs == 0 || task.bounds.timeout_secs > 3600 {
        return Err(RuntimeError::TaskAdmission(
            "timeout_secs must be between 1 and 3600".into(),
        ));
    }
    if task.bounds.token_budget == Some(0) {
        return Err(RuntimeError::TaskAdmission(
            "token_budget must be greater than zero".into(),
        ));
    }
    if task.agent.model.trim().is_empty() {
        return Err(RuntimeError::TaskAdmission(
            "model must not be empty".into(),
        ));
    }
    Ok(())
}

fn admit_route(
    manifest: InferenceManifest,
    requested_offering: &str,
) -> Result<AdmittedRoute, RuntimeError> {
    if manifest.schema_version != 1 {
        return Err(RuntimeError::RouteAdmission(format!(
            "unsupported schema version {}",
            manifest.schema_version
        )));
    }
    reject_duplicate_ids(&manifest.endpoints, |item| &item.id, "endpoint")?;
    reject_duplicate_ids(&manifest.offerings, |item| &item.id, "offering")?;
    let offering = manifest
        .offerings
        .iter()
        .find(|item| item.id == requested_offering)
        .ok_or_else(|| {
            RuntimeError::RouteAdmission("exact requested offering does not exist".into())
        })?;
    if !offering.enabled {
        return Err(RuntimeError::RouteAdmission("offering is disabled".into()));
    }
    if offering.native_model_id.trim().is_empty() {
        return Err(RuntimeError::RouteAdmission(
            "native model ID is empty".into(),
        ));
    }
    if offering.input_modalities.as_slice() != ["text"]
        || offering.output_modalities.as_slice() != ["text"]
    {
        return Err(RuntimeError::RouteAdmission(
            "offering must have only text input and output".into(),
        ));
    }
    if offering.capabilities.tools || offering.capabilities.reasoning {
        return Err(RuntimeError::RouteAdmission(
            "tools and reasoning must be disabled".into(),
        ));
    }
    let endpoint = manifest
        .endpoints
        .iter()
        .find(|item| item.id == offering.endpoint)
        .ok_or_else(|| RuntimeError::RouteAdmission("offering endpoint does not exist".into()))?;
    if !endpoint.enabled {
        return Err(RuntimeError::RouteAdmission("endpoint is disabled".into()));
    }
    if endpoint.adapter != "chat-completions" {
        return Err(RuntimeError::RouteAdmission(
            "endpoint adapter must be chat-completions".into(),
        ));
    }
    let TransportRecord::Http { base_url } = &endpoint.transport;
    let base_url = validate_base_url(base_url)?;
    let expected_secret = endpoint_secret_name(&endpoint.id)?;
    if endpoint.secret_refs.as_slice() != [expected_secret.as_str()] {
        return Err(RuntimeError::RouteAdmission(format!(
            "endpoint must declare exactly secret_ref {expected_secret}"
        )));
    }
    let secret = env::var(&expected_secret)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RuntimeError::SecretResolution(expected_secret.clone()))?;
    Ok(AdmittedRoute {
        offering_id: offering.id.clone(),
        endpoint_id: endpoint.id.clone(),
        native_model_id: offering.native_model_id.clone(),
        base_url,
        secret,
    })
}

fn reject_duplicate_ids<T>(
    records: &[T],
    id: impl Fn(&T) -> &str,
    kind: &str,
) -> Result<(), RuntimeError> {
    let mut seen = HashSet::new();
    if records.iter().any(|record| !seen.insert(id(record))) {
        return Err(RuntimeError::RouteAdmission(format!("duplicate {kind} ID")));
    }
    Ok(())
}

fn validate_base_url(value: &str) -> Result<Url, RuntimeError> {
    let url = Url::parse(value)
        .map_err(|_| RuntimeError::RouteAdmission("endpoint base URL is invalid".into()))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(RuntimeError::RouteAdmission(
            "endpoint URL must not contain credentials, query, or fragment".into(),
        ));
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(&url) => {}
        "http" => {
            return Err(RuntimeError::RouteAdmission(
                "plaintext HTTP is allowed only on loopback".into(),
            ));
        }
        _ => {
            return Err(RuntimeError::RouteAdmission(
                "endpoint URL must use HTTP transport".into(),
            ));
        }
    }
    Ok(url)
}

fn is_loopback_host(url: &Url) -> bool {
    match url.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
        None => false,
    }
}

pub fn endpoint_secret_name(endpoint_id: &str) -> Result<String, RuntimeError> {
    if endpoint_id.is_empty() {
        return Err(RuntimeError::RouteAdmission(
            "endpoint ID must not be empty".into(),
        ));
    }
    let mut encoded = String::with_capacity(endpoint_id.len() * 2);
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in endpoint_id.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(format!("OMEGON_PROJECT_ENDPOINT_{encoded}_TOKEN"))
}

fn read_bounded(path: &Path, limit: u64, kind: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("cannot read {kind}: {error}"))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{kind} is not a bounded regular file"));
    }
    let bytes = fs::read(path).map_err(|error| format!("cannot read {kind}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{kind} exceeded its byte bound while reading"));
    }
    Ok(bytes)
}

fn enabled_by_default() -> bool {
    true
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl AuthorityWriter {
    fn create(command_fingerprint: &str) -> Result<Self, RuntimeError> {
        let home = match env::var_os("OMEGON_HOME") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".omegon"))
                .ok_or_else(|| {
                    RuntimeError::Authority(io::Error::new(
                        io::ErrorKind::NotFound,
                        "OMEGON_HOME and HOME are unset",
                    ))
                })?,
        };
        Self::create_in(&home, command_fingerprint)
    }

    fn create_in(home: &Path, command_fingerprint: &str) -> Result<Self, RuntimeError> {
        fs::create_dir_all(home).map_err(RuntimeError::Authority)?;
        let session_id = Uuid::new_v4();
        let path = home.join(format!("{session_id}.authority.jsonl"));
        let file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .map_err(RuntimeError::Authority)?;
        File::open(home)
            .and_then(|directory| directory.sync_all())
            .map_err(RuntimeError::Authority)?;
        Ok(Self {
            file,
            session_id,
            stream_id: Uuid::new_v4(),
            sequence: 0,
            command_id: Uuid::new_v4(),
            command_fingerprint: command_fingerprint.into(),
        })
    }

    fn append<T: Serialize>(&mut self, event_type: &str, payload: &T) -> Result<(), io::Error> {
        let sequence = self.sequence + 1;
        let envelope = AuthorityEnvelope {
            envelope_version: 1,
            event_id: Uuid::new_v4(),
            session_id: self.session_id,
            stream_id: self.stream_id,
            sequence,
            event_type,
            event_version: 1,
            command_id: self.command_id,
            command_fingerprint: &self.command_fingerprint,
            causation_event_id: None,
            recorded_at: Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            payload,
        };
        serde_json::to_writer(&mut self.file, &envelope).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.sync_all()?;
        self.sequence = sequence;
        Ok(())
    }
}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> Self {
        Self::Authority(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_name_uses_uppercase_utf8_hex() {
        assert_eq!(
            endpoint_secret_name("kernel-fixture").unwrap(),
            "OMEGON_PROJECT_ENDPOINT_6B65726E656C2D66697874757265_TOKEN"
        );
        assert_eq!(
            endpoint_secret_name("mødel").unwrap(),
            "OMEGON_PROJECT_ENDPOINT_6DC3B864656C_TOKEN"
        );
    }

    #[test]
    fn route_rejects_non_loopback_plaintext_and_url_secrets() {
        assert!(validate_base_url("http://example.com/v1").is_err());
        assert!(validate_base_url("https://user:secret@example.com/v1").is_err());
        assert!(validate_base_url("http://127.0.0.1:1234/v1").is_ok());
    }

    #[test]
    fn chat_completion_url_preserves_base_path() {
        let endpoint = chat_completions_url(&Url::parse("https://example.com/api/v1").unwrap());

        assert_eq!(
            endpoint.as_str(),
            "https://example.com/api/v1/chat/completions"
        );
    }

    #[test]
    fn parses_bounded_chat_completion_sse() {
        let result = parse_sse(
            br#"data: {"choices":[{"delta":{"content":"kernel "},"finish_reason":null}]}

data: {"choices":[{"delta":{"content":"reply"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":3}}

data: [DONE]

"#,
        )
        .unwrap();
        assert_eq!(
            result,
            ProviderResult {
                summary: "kernel reply".into(),
                input_tokens: 7,
                output_tokens: 3,
                finish_reason: "stop".into(),
            }
        );
    }

    #[test]
    fn authority_records_exactly_one_terminal_closure() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = AuthorityWriter::create_in(root.path(), &sha256_hex(b"command")).unwrap();
        let turn_id = Uuid::new_v4();
        writer
            .append(
                "turn.started",
                &TurnStarted {
                    turn_id,
                    prompt_id: Uuid::new_v4(),
                    runtime_generation_id: "test".into(),
                },
            )
            .unwrap();
        writer
            .append(
                "turn.closed",
                &TurnClosed {
                    turn_id,
                    outcome: TurnOutcome::Failed,
                    reason_code: "test".into(),
                    recovery_rule_version: None,
                },
            )
            .unwrap();
        drop(writer);
        let path = fs::read_dir(root.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content.matches(r#""event_type":"turn.closed""#).count(), 1);
    }

    #[test]
    fn bounded_statuses_have_stable_exit_codes() {
        let mut result = RunResult {
            status: "completed".into(),
            turns: 1,
            total_input_tokens: 0,
            total_output_tokens: 0,
            token_budget: None,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            duration_secs: 0.0,
            summary: String::new(),
            error: None,
        };
        for (status, exit_code) in [
            ("completed", 0),
            ("error", 1),
            ("exhausted", 2),
            ("timeout", 3),
        ] {
            result.status = status.into();
            assert_eq!(result.exit_code(), exit_code);
        }
    }

    #[test]
    fn token_budget_checks_the_next_request_prospectively() {
        assert!(!token_budget_prevents_request(10, 7, 18));
        assert!(!token_budget_prevents_request(10, 7, 17));
        assert!(token_budget_prevents_request(10, 7, 16));
        assert!(token_budget_prevents_request(10, 7, 10));
    }

    #[test]
    fn turn_budget_checks_below_exact_and_one_above_before_dispatch() {
        assert!(!turn_budget_prevents_request(1, 2));
        assert!(turn_budget_prevents_request(2, 2));
        assert!(turn_budget_prevents_request(3, 2));
    }

    #[test]
    fn deadline_checks_below_exact_and_one_above_before_dispatch() {
        let deadline = tokio::time::Instant::now();
        assert!(!deadline_prevents_request(
            deadline - Duration::from_nanos(1),
            deadline
        ));
        assert!(deadline_prevents_request(deadline, deadline));
        assert!(deadline_prevents_request(
            deadline + Duration::from_nanos(1),
            deadline
        ));
    }
}
