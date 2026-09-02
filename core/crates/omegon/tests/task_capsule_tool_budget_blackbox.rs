use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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

use anyhow::{Context, Result, bail};
use serde_json::Value;

const PROCESS_DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

struct FixtureProvider {
    address: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    thread: Option<thread::JoinHandle<Result<()>>>,
}

impl FixtureProvider {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_requests = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let body = read_request(&mut stream)?;
                        let request_index = {
                            let mut requests = thread_requests.lock().unwrap();
                            let index = requests.len();
                            requests.push(body);
                            index
                        };
                        let event = if request_index == 0 {
                            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"bounded-call-1","type":"function","function":{"name":"fixture_echo","arguments":"{}"}},{"index":1,"id":"bounded-call-2","type":"function","function":{"name":"fixture_echo","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}"#
                        } else {
                            r#"{"choices":[{"index":0,"delta":{"content":"bounded task settled"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":3,"total_tokens":12}}"#
                        };
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {event}\n\ndata: [DONE]\n\n"
                        )?;
                        stream.flush()?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(POLL_INTERVAL);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        });
        Ok(Self {
            address,
            stop,
            requests,
            thread: Some(thread),
        })
    }

    fn shutdown(mut self) -> Result<Vec<Value>> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("provider fixture panicked"))??;
        }
        Ok(self.requests.lock().unwrap().clone())
    }
}

impl Drop for FixtureProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Result<Value> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("provider request ended before headers completed");
        }
        request.extend_from_slice(&buffer[..read]);
    };
    let headers = std::str::from_utf8(&request[..header_end])?;
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
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("provider request ended before body completed");
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Ok(serde_json::from_slice(
        &request[header_end..header_end + content_length],
    )?)
}

fn write_extension(home: &Path, marker: &Path, counter: &Path) -> Result<()> {
    let extension = home.join("extensions/native-tool-fixture");
    fs::create_dir_all(&extension)?;
    let executable = extension.join("fixture.py");
    fs::write(
        &executable,
        include_str!("fixtures/native_extension_conformance.py"),
    )?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&executable)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions)?;
    }
    fs::write(
        extension.join("manifest.toml"),
        format!(
            r#"[extension]
name = "native-tool-fixture"
version = "0.1.0"
description = "Task-capsule tool-budget fixture"

[runtime]
type = "native"
binary = "fixture.py"

[runtime.env]
OMEGON_FIXTURE_MODE = "compatible"
OMEGON_FIXTURE_MARKER = "{}"
OMEGON_FIXTURE_COUNTER = "{}"

[startup]
ping_method = "fixture/status"
timeout_ms = 5000
"#,
            marker.display(),
            counter.display(),
        ),
    )?;
    Ok(())
}

fn write_task_fixture(
    root: &Path,
    provider: &FixtureProvider,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let workspace = root.join("workspace");
    let project = workspace.join(".omegon");
    let home = root.join("omegon-home");
    let marker = root.join("owner.json");
    let counter = root.join("owner-count.txt");
    fs::create_dir_all(&project)?;
    fs::create_dir_all(&home)?;
    fs::write(
        project.join("profile.json"),
        r#"{"permissions":{"trustedContributionCode":["extension:native-tool-fixture"]}}"#,
    )?;
    fs::write(
        project.join("inference.toml"),
        format!(
            r#"schema_version = 1
[[endpoints]]
id = "task-fixture"
adapter = "chat-completions"
secret_refs = ["OMEGON_PROJECT_ENDPOINT_7461736B2D66697874757265_TOKEN"]
enabled = true
[endpoints.transport]
kind = "http"
base_url = "{}/v1"
[[offerings]]
id = "task-fixture:bounded"
endpoint = "task-fixture"
native_model_id = "fixture-native-v1"
input_modalities = ["text"]
output_modalities = ["text"]
enabled = true
[offerings.capabilities]
tools = true
reasoning = true
"#,
            provider.address,
        ),
    )?;
    write_extension(&home, &marker, &counter)?;
    let task = root.join("task.toml");
    fs::write(
        &task,
        r#"[task]
prompt = "Invoke the fixture tool twice."

[bounds]
max_turns = 3
timeout_secs = 20
tool_budget = 1

[agent]
model = "task-fixture:bounded"
"#,
    )?;
    Ok((workspace, home, task))
}

fn run_with_deadline(mut child: Child) -> Result<Output> {
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .context("collect task-capsule output");
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            bail!("task-capsule tool-budget fixture exceeded its deadline");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn authority_text(root: &Path) -> Result<String> {
    if !root.exists() {
        return Ok(String::new());
    }
    let mut text = String::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            text.push_str(&authority_text(&path)?);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".authority.jsonl"))
        {
            text.push_str(&fs::read_to_string(path)?);
        }
    }
    Ok(text)
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    (unsafe { libc::kill(pid as i32, 0) == 0 })
        || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
#[test]
fn manifest_tool_budget_prevents_second_native_owner_entry() -> Result<()> {
    let root = tempfile::tempdir()?;
    let provider = FixtureProvider::start()?;
    let (workspace, home, task) = write_task_fixture(root.path(), &provider)?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_omegon"));
    command.process_group(0);
    let output = run_with_deadline(
        command
            .args([
                "--cwd",
                workspace.to_str().unwrap(),
                "--dangerously-bypass-permissions",
                "--fresh",
                "run",
                task.to_str().unwrap(),
            ])
            .env("HOME", root.path())
            .env("OMEGON_HOME", &home)
            .env("NO_COLOR", "1")
            .env("OMEGON_NERD_FONT", "1")
            .env("RUST_LOG", "error")
            .env(
                "OMEGON_PROJECT_ENDPOINT_7461736B2D66697874757265_TOKEN",
                "fixture-secret",
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    )?;
    let requests = provider.shutdown()?;
    if output.status.code() != Some(2) {
        bail!(
            "task capsule did not exhaust: {}; stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["status"], "exhausted");
    assert_eq!(result["tool_budget"], 1);
    assert_eq!(result["observed_tool_calls"], 1);
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|error| error.contains("observed 1; admitted 1"))
    );
    assert_eq!(
        fs::read_to_string(root.path().join("owner-count.txt"))?
            .lines()
            .count(),
        1
    );
    let owner: Value = serde_json::from_slice(&fs::read(root.path().join("owner.json"))?)?;
    let owner_pid = owner["pid"].as_u64().context("fixture owner pid")? as u32;
    assert!(
        !process_exists(owner_pid),
        "native owner survived task settlement"
    );
    let authority = authority_text(root.path())?;
    assert_eq!(
        authority.matches("\"event_type\":\"turn.closed\"").count(),
        1,
        "bounded authority did not close exactly once: {authority}"
    );
    assert!(
        authority.contains("tool_budget_exhausted"),
        "bounded authority did not record tool exhaustion: {authority}"
    );
    assert!(
        requests.len() >= 2,
        "provider did not receive the settlement turn"
    );
    assert!(requests[0]["tools"].as_array().is_some_and(|tools| {
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "fixture_echo")
    }));
    assert!(
        requests[1]["messages"].as_array().is_some_and(|messages| {
            messages.iter().any(|message| {
                message["role"] == "tool"
                    && message["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("tool budget exhausted"))
            })
        }),
        "settlement request did not contain typed tool exhaustion"
    );
    Ok(())
}
