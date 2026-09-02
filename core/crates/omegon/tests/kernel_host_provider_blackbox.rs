use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use omegon_kernel_runtime::{
    BoundedToolCallError, ToolInvocationBudget, execute_bounded_tool_call,
};
use omegon_native_extension_host::{
    ExtensionManifest, ExtensionProcessState, ExtensionSupervisor, LaunchSpec, RpcRequestPolicy,
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

const PROCESS_DEADLINE: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PROMPT: &str = "Return the deterministic kernel fixture response.";

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    request_line: String,
    authorization: Option<String>,
    body: Value,
}

#[derive(Clone, Copy)]
enum ProviderBehavior {
    Complete,
    Continue,
    Stall,
}

struct DeterministicProvider {
    address: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    lease_visible_at_request: Arc<AtomicBool>,
    connection_closed: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<Result<()>>>,
}

impl DeterministicProvider {
    fn start(authority_root: PathBuf) -> Result<Self> {
        Self::start_with_behavior(authority_root, ProviderBehavior::Complete)
    }

    fn start_stalled(authority_root: PathBuf) -> Result<Self> {
        Self::start_with_behavior(authority_root, ProviderBehavior::Stall)
    }

    fn start_continuation(authority_root: PathBuf) -> Result<Self> {
        Self::start_with_behavior(authority_root, ProviderBehavior::Continue)
    }

    fn start_with_behavior(authority_root: PathBuf, behavior: ProviderBehavior) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("bind fixture provider")?;
        listener
            .set_nonblocking(true)
            .context("make fixture provider nonblocking")?;
        let address = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let lease_visible_at_request = Arc::new(AtomicBool::new(false));
        let connection_closed = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_requests = Arc::clone(&requests);
        let thread_lease_visible = Arc::clone(&lease_visible_at_request);
        let thread_connection_closed = Arc::clone(&connection_closed);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_http_request(&mut stream)?;
                        if request
                            .request_line
                            .starts_with("POST /v1/chat/completions ")
                        {
                            thread_lease_visible.store(
                                authority_contains(&authority_root, "route.lease_recorded")?,
                                Ordering::Release,
                            );
                            thread_requests.lock().unwrap().push(request);
                            match behavior {
                                ProviderBehavior::Complete => {
                                    write!(
                                        stream,
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"kernel provider reply\"}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3,\"total_tokens\":10}}}}\n\n"
                                    )?;
                                    stream.flush()?;
                                }
                                ProviderBehavior::Continue => {
                                    write!(
                                        stream,
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"partial kernel reply\"}},\"finish_reason\":\"length\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3,\"total_tokens\":10}}}}\n\n"
                                    )?;
                                    stream.flush()?;
                                }
                                ProviderBehavior::Stall => {
                                    write!(
                                        stream,
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                                    )?;
                                    stream.flush()?;
                                    stream.set_read_timeout(Some(POLL_INTERVAL))?;
                                    let mut byte = [0_u8; 1];
                                    while !thread_stop.load(Ordering::Acquire) {
                                        match stream.read(&mut byte) {
                                            Ok(0) => {
                                                thread_connection_closed
                                                    .store(true, Ordering::Release);
                                                break;
                                            }
                                            Ok(_) => {}
                                            Err(error)
                                                if matches!(
                                                    error.kind(),
                                                    std::io::ErrorKind::WouldBlock
                                                        | std::io::ErrorKind::TimedOut
                                                ) => {}
                                            Err(error) => {
                                                return Err(error)
                                                    .context("observe fixture client settlement");
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            thread_requests.lock().unwrap().push(request);
                            write!(
                                stream,
                                "HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                            )?;
                            stream.flush()?;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(POLL_INTERVAL);
                    }
                    Err(error) => return Err(error).context("accept fixture provider request"),
                }
            }
            Ok(())
        });
        Ok(Self {
            address,
            stop,
            requests,
            lease_visible_at_request,
            connection_closed,
            thread: Some(thread),
        })
    }

    fn shutdown(mut self) -> Result<(Vec<CapturedRequest>, bool, bool)> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow!("fixture provider thread panicked"))??;
        }
        let requests = self.requests.lock().unwrap().clone();
        let lease_visible = self.lease_visible_at_request.load(Ordering::Acquire);
        let connection_closed = self.connection_closed.load(Ordering::Acquire);
        Ok((requests, lease_visible, connection_closed))
    }

    fn wait_for_connection_closed(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.connection_closed.load(Ordering::Acquire) {
                return true;
            }
            thread::sleep(POLL_INTERVAL);
        }
        false
    }

    fn wait_for_request(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !self.requests.lock().unwrap().is_empty() {
                return true;
            }
            thread::sleep(POLL_INTERVAL);
        }
        false
    }
}

impl Drop for DeterministicProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<CapturedRequest> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("bound fixture request read")?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        let read = stream.read(&mut buffer).context("read fixture request")?;
        if read == 0 {
            bail!("fixture request ended before headers completed");
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > 256 * 1024 {
            bail!("fixture request exceeded its bound");
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec())?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while request.len() < header_end + content_length {
        let read = stream
            .read(&mut buffer)
            .context("read fixture request body")?;
        if read == 0 {
            bail!("fixture request ended before its body completed");
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > 256 * 1024 {
            bail!("fixture request exceeded its bound");
        }
    }
    let request_line = headers.lines().next().unwrap_or_default().to_string();
    let authorization = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim().to_string())
    });
    let body = if content_length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&request[header_end..header_end + content_length])?
    };
    Ok(CapturedRequest {
        request_line,
        authorization,
        body,
    })
}

fn authority_contains(root: &Path, event_type: &str) -> Result<bool> {
    if !root.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            if authority_contains(&path, event_type)? {
                return Ok(true);
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".authority.jsonl"))
            && fs::read_to_string(&path)?.contains(&format!(r#""event_type":"{event_type}""#))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn authority_event_types(root: &Path) -> Result<Vec<String>> {
    Ok(authority_events(root)?
        .into_iter()
        .filter_map(|event| event["event_type"].as_str().map(str::to_string))
        .collect())
}

fn authority_events(root: &Path) -> Result<Vec<Value>> {
    let mut events = Vec::new();
    collect_authority_events(root, &mut events)?;
    Ok(events)
}

fn collect_authority_events(root: &Path, events: &mut Vec<Value>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_authority_events(&path, events)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".authority.jsonl"))
        {
            for line in fs::read_to_string(path)?.lines() {
                events.push(serde_json::from_str(line)?);
            }
        }
    }
    Ok(())
}

fn wait_with_deadline(mut child: std::process::Child) -> Result<Output> {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .context("collect kernel host output");
        }
        if Instant::now() >= deadline {
            terminate_process_group(child.id());
            bail!("kernel host exceeded process deadline");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(pid: u32) {
    unsafe {
        kill(-(pid as i32), 9);
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_pid: u32) {}

#[cfg(unix)]
fn interrupt_process_group(pid: u32) {
    unsafe {
        kill(-(pid as i32), 2);
    }
}

#[cfg(not(unix))]
fn interrupt_process_group(_pid: u32) {}

struct KernelFixturePaths {
    workspace: PathBuf,
    omegon_home: PathBuf,
    task: PathBuf,
}

fn write_kernel_fixture(
    root: &Path,
    provider_address: &str,
    timeout_secs: u64,
) -> Result<KernelFixturePaths> {
    write_kernel_fixture_with_bounds(root, provider_address, timeout_secs, 1, None)
}

fn write_kernel_fixture_with_bounds(
    root: &Path,
    provider_address: &str,
    timeout_secs: u64,
    max_turns: u32,
    token_budget: Option<u64>,
) -> Result<KernelFixturePaths> {
    let workspace = root.join("workspace");
    let project_config = workspace.join(".omegon");
    let omegon_home = root.join("omegon-home");
    fs::create_dir_all(&project_config)?;
    fs::create_dir_all(&omegon_home)?;
    fs::write(project_config.join("profile.json"), "{}\n")?;
    fs::write(
        project_config.join("inference.toml"),
        format!(
            r#"schema_version = 1
[[endpoints]]
id = "kernel-fixture"
adapter = "chat-completions"
secret_refs = ["OMEGON_PROJECT_ENDPOINT_6B65726E656C2D66697874757265_TOKEN"]
[endpoints.transport]
kind = "http"
base_url = "{provider_address}/v1"
[[offerings]]
id = "kernel-fixture:bounded"
endpoint = "kernel-fixture"
native_model_id = "fixture-native-v1"
input_modalities = ["text"]
output_modalities = ["text"]
[offerings.capabilities]
tools = false
reasoning = false
"#,
        ),
    )?;
    let task = root.join("task.toml");
    let token_budget = token_budget
        .map(|budget| format!("token_budget = {budget}\n"))
        .unwrap_or_default();
    fs::write(
        &task,
        format!(
            r#"[task]
prompt = "{PROMPT}"
[bounds]
max_turns = {max_turns}
timeout_secs = {timeout_secs}
{token_budget}[agent]
model = "kernel-fixture:bounded"
"#,
        ),
    )?;
    Ok(KernelFixturePaths {
        workspace,
        omegon_home,
        task,
    })
}

fn spawn_kernel_host(root: &Path, fixture: &KernelFixturePaths) -> Result<Child> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_omegon-kernel-host"));
    configure_process_group(&mut command);
    command
        .args([
            "--cwd",
            fixture.workspace.to_str().unwrap(),
            "run",
            fixture.task.to_str().unwrap(),
        ])
        .env("HOME", root)
        .env("OMEGON_HOME", &fixture.omegon_home)
        .env("NO_COLOR", "1")
        .env("OMEGON_NERD_FONT", "1")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env(
            "OMEGON_PROJECT_ENDPOINT_6B65726E656C2D66697874757265_TOKEN",
            "kernel-fixture-secret",
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn reduced kernel host")
}

fn run_kernel_host(root: &Path, fixture: &KernelFixturePaths) -> Result<Output> {
    wait_with_deadline(spawn_kernel_host(root, fixture)?)
}

#[cfg(unix)]
fn write_native_tool_fixture(root: &Path, marker: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let extension_dir = root.join("native-tool-fixture");
    fs::create_dir_all(&extension_dir)?;
    let executable = extension_dir.join("fixture.py");
    fs::write(
        &executable,
        include_str!("fixtures/native_extension_conformance.py"),
    )?;
    let mut permissions = fs::metadata(&executable)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions)?;
    fs::write(
        extension_dir.join("manifest.toml"),
        format!(
            r#"[extension]
name = "native-tool-fixture"
version = "0.1.0"
description = "Deterministic bounded native tool fixture"

[runtime]
type = "native"
binary = "fixture.py"

[runtime.env]
OMEGON_FIXTURE_MODE = "compatible"
OMEGON_FIXTURE_MARKER = "{}"

[startup]
ping_method = "fixture/status"
timeout_ms = 5000
"#,
            marker.display(),
        ),
    )?;
    Ok(extension_dir)
}

#[cfg(unix)]
#[tokio::test]
async fn tool_budget_prevents_native_rpc_owner_entry() -> Result<()> {
    let root = tempfile::tempdir().context("create BND-003 root")?;
    let marker = root.path().join("owner-entered.json");
    let extension_dir = write_native_tool_fixture(root.path(), &marker)?;
    let manifest = ExtensionManifest::from_extension_dir(&extension_dir)?;
    let launch = LaunchSpec {
        manifest,
        extension_dir,
        project_root: Some(root.path().to_path_buf()),
        resolved_config: Map::new(),
        resolved_secrets: Vec::new(),
        source_digest: "fixture:bnd-003".into(),
        notification_tx: None,
        host_request_handler: None,
        readiness_validator: None,
    };
    let (supervisor, handshake) = ExtensionSupervisor::launch(launch).await?;
    assert_eq!(handshake.tools.len(), 1);
    assert_eq!(handshake.tools[0].name, "fixture_echo");

    let mut budget = ToolInvocationBudget::new(Some(1));
    let first = execute_bounded_tool_call(&mut budget, || async {
        supervisor
            .rpc_call_with_cancel(
                "execute_tool",
                json!({"name": "fixture_echo", "arguments": {}}),
                CancellationToken::new(),
                Some(Duration::from_secs(2)),
                RpcRequestPolicy::RejectHostRequests,
                None,
            )
            .await
    })
    .await
    .map_err(|error| anyhow!(error.to_string()))?;
    assert_eq!(first["content"][0]["text"], "ok");
    assert!(marker.is_file(), "admitted call did not enter its owner");
    fs::remove_file(&marker)?;

    let exhausted = execute_bounded_tool_call(&mut budget, || async {
        supervisor
            .rpc_call_with_cancel(
                "execute_tool",
                json!({"name": "fixture_echo", "arguments": {}}),
                CancellationToken::new(),
                Some(Duration::from_secs(2)),
                RpcRequestPolicy::RejectHostRequests,
                None,
            )
            .await
    })
    .await
    .unwrap_err();
    let BoundedToolCallError::BudgetExhausted(exhausted) = exhausted else {
        bail!("second call failed after budget admission: {exhausted}");
    };
    assert_eq!(exhausted.admitted, 1);
    assert_eq!(exhausted.observed, 1);
    assert_eq!(budget.observed(), 1);
    assert!(
        !marker.exists(),
        "exhausted call entered its component owner"
    );
    assert_eq!(supervisor.health().state, ExtensionProcessState::Healthy);

    supervisor.shutdown(Duration::from_millis(500)).await?;
    root.close()
        .context("BND-003 retained an isolated fixture handle")?;
    Ok(())
}

#[test]
fn provider_backed_kernel_turn_uses_admitted_route_and_settles() -> Result<()> {
    let root = tempfile::tempdir().context("create KRN-003 root")?;
    let provider = DeterministicProvider::start(root.path().to_path_buf())?;
    let fixture = write_kernel_fixture(root.path(), &provider.address, 10)?;
    let output = run_kernel_host(root.path(), &fixture)?;
    if !output.status.success() {
        bail!(
            "provider-backed kernel turn failed: {}; stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let result: Value =
        serde_json::from_slice(&output.stdout).context("parse structured result")?;
    assert_eq!(result["status"], "completed");
    assert_eq!(result["turns"], 1);
    assert_eq!(result["total_input_tokens"], 7);
    assert_eq!(result["total_output_tokens"], 3);
    assert_eq!(result["summary"], "kernel provider reply");

    let (requests, lease_visible_at_request, _) = provider.shutdown()?;
    assert_eq!(requests.len(), 1, "kernel must issue exactly one request");
    let request = &requests[0];
    assert!(
        request
            .request_line
            .starts_with("POST /v1/chat/completions ")
    );
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer kernel-fixture-secret")
    );
    assert_eq!(request.body["model"], "fixture-native-v1");
    assert_eq!(request.body["stream"], true);
    assert!(
        request.body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["role"] == "user"
                    && message["content"]
                        .as_str()
                        .is_some_and(|content| content.contains(PROMPT))
            })
    );
    assert!(
        lease_visible_at_request,
        "route lease was not durable before dispatch"
    );

    let event_types = authority_event_types(root.path())?;
    assert_eq!(
        event_types
            .iter()
            .filter(|event| *event == "turn.started")
            .count(),
        1
    );
    assert_eq!(
        event_types
            .iter()
            .filter(|event| *event == "route.lease_recorded")
            .count(),
        1
    );
    assert_eq!(
        event_types
            .iter()
            .filter(|event| *event == "turn.closed")
            .count(),
        1
    );
    let route = event_types
        .iter()
        .position(|event| event == "route.lease_recorded")
        .context("route lease event is absent")?;
    let closed = event_types
        .iter()
        .position(|event| event == "turn.closed")
        .context("turn closure event is absent")?;
    assert!(route < closed, "route lease must precede terminal closure");

    root.close()
        .context("KRN-003 retained an isolated fixture handle")?;
    Ok(())
}

#[test]
fn timed_out_provider_turn_settles_authority_and_transport() -> Result<()> {
    let root = tempfile::tempdir().context("create KRN-004 root")?;
    let provider = DeterministicProvider::start_stalled(root.path().to_path_buf())?;
    let fixture = write_kernel_fixture(root.path(), &provider.address, 1)?;

    let started_at = Instant::now();
    let output = run_kernel_host(root.path(), &fixture)?;
    assert!(
        started_at.elapsed() < Duration::from_secs(5),
        "provider timeout exceeded its bounded settlement window"
    );
    assert_eq!(output.status.code(), Some(3));
    let result: Value = serde_json::from_slice(&output.stdout).context("parse timeout result")?;
    assert_eq!(result["status"], "timeout");
    assert_eq!(result["turns"], 1);
    assert_eq!(result["error"], "provider turn timed out");

    assert!(
        provider.wait_for_connection_closed(),
        "provider connection remained active after timeout"
    );
    let (requests, lease_visible_at_request, connection_closed) = provider.shutdown()?;
    assert_eq!(requests.len(), 1, "kernel must not retry a timed-out turn");
    assert!(
        lease_visible_at_request,
        "route lease was not durable before dispatch"
    );
    assert!(connection_closed, "provider transport did not settle");

    let events = authority_events(root.path())?;
    let closed: Vec<_> = events
        .iter()
        .filter(|event| event["event_type"] == "turn.closed")
        .collect();
    assert_eq!(closed.len(), 1, "timed-out turn must close exactly once");
    assert_eq!(closed[0]["payload"]["outcome"], "timed_out");
    assert_eq!(closed[0]["payload"]["reason_code"], "provider_timeout");
    let route_sequence = events
        .iter()
        .find(|event| event["event_type"] == "route.lease_recorded")
        .and_then(|event| event["sequence"].as_u64())
        .context("route lease event is absent")?;
    let closed_sequence = closed[0]["sequence"]
        .as_u64()
        .context("turn closure sequence is absent")?;
    assert!(route_sequence < closed_sequence);

    root.close()
        .context("KRN-004 retained an isolated fixture handle")?;
    Ok(())
}

#[test]
fn cancelled_provider_turn_settles_authority_and_transport() -> Result<()> {
    let root = tempfile::tempdir().context("create KRN-004 cancellation root")?;
    let provider = DeterministicProvider::start_stalled(root.path().to_path_buf())?;
    let fixture = write_kernel_fixture(root.path(), &provider.address, 10)?;
    let child = spawn_kernel_host(root.path(), &fixture)?;

    assert!(
        provider.wait_for_request(),
        "provider request did not start"
    );
    interrupt_process_group(child.id());
    let output = wait_with_deadline(child)?;
    assert_eq!(output.status.code(), Some(1));
    let result: Value =
        serde_json::from_slice(&output.stdout).context("parse cancellation result")?;
    assert_eq!(result["status"], "error");
    assert_eq!(result["turns"], 1);
    assert_eq!(result["error"], "provider turn cancelled");

    assert!(
        provider.wait_for_connection_closed(),
        "provider connection remained active after cancellation"
    );
    let (requests, lease_visible_at_request, connection_closed) = provider.shutdown()?;
    assert_eq!(requests.len(), 1, "kernel must not retry a cancelled turn");
    assert!(lease_visible_at_request);
    assert!(connection_closed);

    let events = authority_events(root.path())?;
    let closed: Vec<_> = events
        .iter()
        .filter(|event| event["event_type"] == "turn.closed")
        .collect();
    assert_eq!(closed.len(), 1, "cancelled turn must close exactly once");
    assert_eq!(closed[0]["payload"]["outcome"], "cancelled");
    assert_eq!(closed[0]["payload"]["reason_code"], "provider_cancelled");

    root.close()
        .context("KRN-004 cancellation retained an isolated fixture handle")?;
    Ok(())
}

#[test]
fn invalid_task_field_is_rejected_before_authority() -> Result<()> {
    let root = tempfile::tempdir().context("create BND-001 root")?;
    let workspace = root.path().join("workspace");
    let omegon_home = root.path().join("omegon-home");
    fs::create_dir_all(&workspace)?;
    fs::create_dir_all(&omegon_home)?;
    let task = root.path().join("invalid-task.toml");
    fs::write(
        &task,
        format!(
            r#"[task]
prompt = "{PROMPT}"
unadmitted_field = true
[bounds]
max_turns = 1
timeout_secs = 10
[agent]
model = "kernel-fixture:bounded"
"#,
        ),
    )?;
    let fixture = KernelFixturePaths {
        workspace,
        omegon_home,
        task,
    };

    let output = run_kernel_host(root.path(), &fixture)?;
    assert_eq!(output.status.code(), Some(1));
    let result: Value =
        serde_json::from_slice(&output.stdout).context("parse task rejection result")?;
    assert_eq!(result["status"], "error");
    assert_eq!(result["turns"], 0);
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|error| error.contains("unadmitted_field")),
        "task rejection did not identify the invalid field: {result}"
    );
    assert!(
        authority_events(root.path())?.is_empty(),
        "invalid task created runtime authority"
    );

    root.close()
        .context("BND-001 retained an isolated fixture handle")?;
    Ok(())
}

#[test]
fn token_budget_prevents_the_next_provider_request() -> Result<()> {
    for token_budget in [10, 9] {
        let root = tempfile::tempdir().context("create BND-002 root")?;
        let provider = DeterministicProvider::start_continuation(root.path().to_path_buf())?;
        let fixture = write_kernel_fixture_with_bounds(
            root.path(),
            &provider.address,
            10,
            2,
            Some(token_budget),
        )?;

        let output = run_kernel_host(root.path(), &fixture)?;
        assert_eq!(output.status.code(), Some(2));
        let result: Value =
            serde_json::from_slice(&output.stdout).context("parse token exhaustion result")?;
        assert_eq!(result["status"], "exhausted");
        assert_eq!(result["turns"], 1);
        assert_eq!(result["total_input_tokens"], 7);
        assert_eq!(result["total_output_tokens"], 3);
        assert_eq!(result["token_budget"], token_budget);
        assert!(
            result["error"]
                .as_str()
                .is_some_and(|error| error.contains("token budget exhausted"))
        );

        let (requests, lease_visible_at_request, _) = provider.shutdown()?;
        assert_eq!(
            requests.len(),
            1,
            "budget {token_budget} admitted an over-budget request"
        );
        assert!(lease_visible_at_request);

        let events = authority_events(root.path())?;
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event_type"] == "route.lease_recorded")
                .count(),
            1
        );
        let closed: Vec<_> = events
            .iter()
            .filter(|event| event["event_type"] == "turn.closed")
            .collect();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0]["payload"]["outcome"], "failed");
        assert_eq!(
            closed[0]["payload"]["reason_code"],
            "token_budget_exhausted"
        );

        root.close()
            .context("BND-002 retained an isolated fixture handle")?;
    }
    Ok(())
}

#[test]
fn turn_budget_prevents_the_next_provider_request() -> Result<()> {
    let root = tempfile::tempdir().context("create turn-budget root")?;
    let provider = DeterministicProvider::start_continuation(root.path().to_path_buf())?;
    let fixture = write_kernel_fixture_with_bounds(root.path(), &provider.address, 10, 1, None)?;

    let output = run_kernel_host(root.path(), &fixture)?;
    assert_eq!(output.status.code(), Some(2));
    let result: Value =
        serde_json::from_slice(&output.stdout).context("parse turn exhaustion result")?;
    assert_eq!(result["status"], "exhausted");
    assert_eq!(result["turns"], 1);
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|error| error.contains("turn budget exhausted: admitted 1"))
    );

    let (requests, lease_visible_at_request, _) = provider.shutdown()?;
    assert_eq!(requests.len(), 1, "turn budget admitted request two");
    assert!(lease_visible_at_request);
    let events = authority_events(root.path())?;
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event_type"] == "route.lease_recorded")
            .count(),
        1
    );
    let closed: Vec<_> = events
        .iter()
        .filter(|event| event["event_type"] == "turn.closed")
        .collect();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["payload"]["outcome"], "failed");
    assert_eq!(closed[0]["payload"]["reason_code"], "turn_budget_exhausted");

    root.close()
        .context("turn-budget test retained an isolated fixture handle")?;
    Ok(())
}
