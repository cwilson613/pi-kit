#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tempfile::TempDir;

const STARTUP_DEADLINE: Duration = Duration::from_secs(60);
const EXIT_DEADLINE: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

struct HoldingProvider {
    address: String,
    stop: Arc<AtomicBool>,
    request_started: Arc<AtomicBool>,
    accept_thread: Option<thread::JoinHandle<()>>,
}

impl HoldingProvider {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("bind fixture provider")?;
        listener
            .set_nonblocking(true)
            .context("make fixture provider nonblocking")?;
        let address = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let request_started = Arc::new(AtomicBool::new(false));
        let thread_request_started = Arc::clone(&request_started);
        let accept_thread = thread::spawn(move || {
            let mut connections = Vec::new();
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let connection_stop = Arc::clone(&thread_stop);
                        let connection_request_started = Arc::clone(&thread_request_started);
                        connections.push(thread::spawn(move || {
                            let _ = serve_provider_connection(
                                stream,
                                &connection_stop,
                                &connection_request_started,
                            );
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(POLL_INTERVAL);
                    }
                    Err(_) => break,
                }
            }
            for connection in connections {
                let _ = connection.join();
            }
        });
        Ok(Self {
            address,
            stop,
            request_started,
            accept_thread: Some(accept_thread),
        })
    }
}

impl Drop for HoldingProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_provider_connection(
    mut stream: TcpStream,
    stop: &AtomicBool,
    request_started: &AtomicBool,
) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("bound fixture request read")?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).context("read fixture request")?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > 256 * 1024 {
            bail!("fixture provider request exceeded its bound");
        }
    }
    let request_line = String::from_utf8_lossy(&request);
    if request_line.starts_with("GET /api/tags ") {
        write_json_response(
            &mut stream,
            r#"{"models":[{"name":"gpt-5.4","model":"gpt-5.4"}]}"#,
        )?;
    } else if request_line.starts_with("GET /v1/models ") {
        write_json_response(
            &mut stream,
            r#"{"data":[{"id":"gpt-5.4","object":"model"}]}"#,
        )?;
    } else if request_line.starts_with("POST /v1/chat/completions ") {
        request_started.store(true, Ordering::Release);
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        )?;
        stream.flush()?;
        while !stop.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
        }
    } else {
        write_json_response(&mut stream, "{}")?;
    }
    Ok(())
}

fn write_json_response(stream: &mut TcpStream, body: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    stream.flush()?;
    Ok(())
}

struct PtyOmegon {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Option<Box<dyn MasterPty + Send>>,
    process_group: libc::pid_t,
    root: TempDir,
    log: PathBuf,
}

impl PtyOmegon {
    fn spawn(initial_prompt: Option<&str>, provider: &HoldingProvider) -> Result<Self> {
        let root = tempfile::tempdir().context("create isolated acceptance root")?;
        let workspace = root.path().join("workspace");
        let project_config = workspace.join(".omegon");
        fs::create_dir_all(&project_config).context("create isolated project config")?;
        fs::write(project_config.join("profile.json"), "{}\n")
            .context("disable first-run interaction")?;
        fs::write(
            project_config.join("inference.toml"),
            format!(
                r#"schema_version = 1
[[endpoints]]
id = "acceptance"
adapter = "chat-completions"
secret_refs = ["OMEGON_PROJECT_ENDPOINT_616363657074616E6365_TOKEN"]
[endpoints.transport]
kind = "http"
base_url = "{}/v1"
[[offerings]]
id = "openai:gpt-5.4"
endpoint = "acceptance"
native_model_id = "gpt-5.4"
input_modalities = ["text"]
output_modalities = ["text"]
[offerings.capabilities]
tools = true
reasoning = true
"#,
                provider.address
            ),
        )
        .context("write isolated inference route")?;
        let omegon_home = root.path().join("omegon-home");
        fs::create_dir_all(&omegon_home).context("create isolated Omegon home")?;
        let log = root.path().join("omegon.log");

        let pty = native_pty_system().openpty(PtySize {
            rows: 32,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(resolve_omegon_binary()?);
        command.args([
            "--cwd",
            workspace.to_str().context("workspace path is not UTF-8")?,
            "--model",
            "openai:gpt-5.4",
            "--no-splash",
            "--fresh",
            "--log-level",
            "debug",
            "--log-file",
            log.to_str().context("log path is not UTF-8")?,
        ]);
        if let Some(prompt) = initial_prompt {
            command.args(["--initial-prompt", prompt]);
        }
        command.env("HOME", root.path());
        command.env("OMEGON_HOME", &omegon_home);
        command.env("OMEGON_CHILD", "1");
        command.env("OPENAI_API_KEY", "acceptance-local-only");
        command.env(
            "OMEGON_PROJECT_ENDPOINT_616363657074616E6365_TOKEN",
            "acceptance-local-only",
        );
        command.env("OMEGON_NERD_FONT", "1");
        command.env("NO_COLOR", "1");

        let child = pty
            .slave
            .spawn_command(command)
            .context("spawn Omegon on controlling PTY")?;
        drop(pty.slave);
        let pid = child.process_id().context("PTY child has no process ID")? as libc::pid_t;
        let process_group = pty
            .master
            .process_group_leader()
            .context("PTY has no foreground process group")?;
        if process_group != pid {
            bail!(
                "PTY child did not receive a dedicated process group: pid={pid}, pgid={process_group}"
            );
        }
        let master_fd = pty
            .master
            .as_raw_fd()
            .context("PTY master has no Unix fd")?;
        let flags = unsafe { libc::fcntl(master_fd, libc::F_GETFL) };
        if flags == -1
            || unsafe { libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err(std::io::Error::last_os_error()).context("make PTY master nonblocking");
        }
        Ok(Self {
            child,
            master: Some(pty.master),
            process_group,
            root,
            log,
        })
    }

    fn wait_for_tui_output(&mut self) -> Result<()> {
        let mut output = Vec::new();
        let result = wait_until(STARTUP_DEADLINE, || {
            if let Some(status) = self.child.try_wait()? {
                bail!("Omegon exited before its first TUI draw: {status}");
            }
            self.drain_pty(&mut output)?;
            Ok(self
                .log_text()
                .is_ok_and(|log| log.contains("terminal input boundary acquired")))
        });
        result.with_context(|| {
            format!(
                "Omegon TUI did not draw before the startup deadline; log:\n{}",
                self.log_text().unwrap_or_else(|error| error.to_string())
            )
        })
    }

    fn wait_for_authority_event(&mut self, event_type: &str) -> Result<()> {
        let mut output = Vec::new();
        wait_until(STARTUP_DEADLINE, || {
            self.drain_pty(&mut output)?;
            Ok(
                read_files_with_suffix(self.root.path(), ".authority.jsonl")?
                    .contains(&format!(r#""event_type":"{event_type}""#)),
            )
        })
        .with_context(|| format!("authority event {event_type} was not persisted"))
    }

    fn wait_for_provider_request(&mut self, provider: &HoldingProvider) -> Result<()> {
        let mut output = Vec::new();
        wait_until(STARTUP_DEADLINE, || {
            if let Some(status) = self.child.try_wait()? {
                bail!("Omegon exited before provider dispatch: {status}");
            }
            self.drain_pty(&mut output)?;
            Ok(provider.request_started.load(Ordering::Acquire))
        })
        .context("fixture provider did not receive the active model request")
    }

    fn drain_pty(&self, output: &mut Vec<u8>) -> Result<()> {
        let fd = self
            .master
            .as_ref()
            .context("PTY already detached")?
            .as_raw_fd()
            .context("PTY master has no Unix fd")?;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if read > 0 {
                output.extend_from_slice(&buffer[..read as usize]);
                if output.len() > 1024 * 1024 {
                    output.drain(..output.len() - 1024 * 1024);
                }
                continue;
            }
            if read == 0 {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(());
            }
            return Err(error).context("drain PTY output");
        }
    }

    fn detach_and_wait(&mut self) -> Result<portable_pty::ExitStatus> {
        drop(self.master.take());
        let status = wait_until_value(EXIT_DEADLINE, || self.child.try_wait().map_err(Into::into))
            .with_context(|| {
                format!(
                    "Omegon survived beyond the terminal-loss deadline; log:\n{}",
                    self.log_text().unwrap_or_else(|error| error.to_string())
                )
            })?;
        wait_until(EXIT_DEADLINE, || {
            Ok(!process_group_exists(self.process_group))
        })
        .context("Omegon process group remained after terminalization")?;
        Ok(status)
    }

    fn retained_authority(&self) -> Result<String> {
        read_files_with_suffix(self.root.path(), ".authority.jsonl")
    }

    fn retained_session_count(&self) -> Result<usize> {
        count_files_with_suffix(
            &self.root.path().join(".config/omegon/sessions"),
            ".json",
            ".meta.json",
        )
    }

    fn log_text(&self) -> Result<String> {
        fs::read_to_string(&self.log).with_context(|| format!("read {}", self.log.display()))
    }
}

impl Drop for PtyOmegon {
    fn drop(&mut self) {
        drop(self.master.take());
        if process_group_exists(self.process_group) {
            unsafe {
                libc::kill(-self.process_group, libc::SIGKILL);
            }
        }
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "real PTY/process acceptance; run in the authoritative Unix CI lane"]
fn real_terminal_detachment_terminalizes_idle_and_active_sessions() -> Result<()> {
    let provider = HoldingProvider::start()?;

    let mut idle = PtyOmegon::spawn(None, &provider)?;
    idle.wait_for_tui_output()?;
    let idle_status = idle.detach_and_wait()?;
    assert!(
        idle_status.success(),
        "idle terminalization: {idle_status}; log:\n{}",
        idle.log_text()?
    );
    assert!(
        idle.retained_session_count()? >= 1,
        "idle terminal loss must retain a session snapshot"
    );
    assert_last_boundary(&idle.log_text()?).context("idle terminal-loss boundary")?;

    let mut active = PtyOmegon::spawn(Some("remain active until terminal loss"), &provider)?;
    active.wait_for_authority_event("turn.started")?;
    active
        .wait_for_provider_request(&provider)
        .with_context(|| {
            format!(
                "active request log:\n{}",
                active.log_text().unwrap_or_else(|error| error.to_string())
            )
        })?;
    let active_status = active.detach_and_wait()?;
    assert!(
        active_status.success(),
        "active terminalization: {active_status}; log:\n{}",
        active.log_text()?
    );
    assert_last_boundary(&active.log_text()?).context("active terminal-loss boundary")?;
    assert_generation_scoped_revocation(&active.retained_authority()?).with_context(|| {
        format!(
            "active terminal-loss log:\n{}",
            active.log_text().unwrap_or_else(|error| error.to_string())
        )
    })?;
    assert!(
        active.retained_session_count()? >= 1,
        "active terminal loss must retain a session snapshot"
    );

    Ok(())
}

fn assert_last_boundary(log: &str) -> Result<()> {
    let acquired =
        log.contains("terminal input boundary acquired") && log.contains("terminal_input_acquired");
    let loss_observed = (log
        .contains("terminal input boundary lost; returning control to the runtime supervisor")
        && log.contains("terminal_input_lost"))
        || log.contains("TUI terminated at the terminal boundary");
    if !acquired || !loss_observed {
        bail!(
            "terminal-loss log did not retain the last completed boundary: acquired={acquired}, loss_observed={loss_observed}; log:\n{log}"
        );
    }
    Ok(())
}

fn assert_generation_scoped_revocation(authority: &str) -> Result<()> {
    let events = authority
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let started = events
        .iter()
        .find(|event| event["event_type"] == "turn.started")
        .context("missing turn.started")?;
    let turn_id = started["payload"]["turn_id"]
        .as_str()
        .context("turn.started has no turn identity")?;
    let generation = started["payload"]["runtime_generation_id"]
        .as_str()
        .context("turn.started has no runtime generation")?;
    if generation.is_empty() {
        bail!("turn.started retained an empty runtime generation");
    }
    let closures = events
        .iter()
        .filter(|event| event["event_type"] == "turn.closed")
        .collect::<Vec<_>>();
    if closures.len() != 1 {
        bail!(
            "expected exactly one terminal settlement, got {}",
            closures.len()
        );
    }
    let closure = closures[0];
    if closure["payload"]["turn_id"] != turn_id
        || closure["payload"]["outcome"] != "revoked"
        || closure["payload"]["reason_code"] != "terminal_lost"
    {
        bail!("terminal settlement did not revoke the active generation: {closure}");
    }
    let closure_index = events
        .iter()
        .position(|event| std::ptr::eq(event, closure))
        .context("closure index")?;
    if events[closure_index + 1..].iter().any(|event| {
        matches!(
            event["event_type"].as_str(),
            Some(
                "assistant.content_appended"
                    | "assistant.message_committed"
                    | "tool.call_recorded"
                    | "invocation.registered"
            )
        )
    }) {
        bail!("revoked generation published assistant or tool work after settlement");
    }
    Ok(())
}

fn resolve_omegon_binary() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_omegon") {
        return Ok(path.into());
    }
    let current = std::env::current_exe().context("resolve current test executable")?;
    let candidate = current
        .parent()
        .and_then(Path::parent)
        .context("integration test executable has no target directory")?
        .join("omegon");
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "Omegon binary not found at {}",
            candidate.display()
        ))
    }
}

fn wait_until(deadline: Duration, mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    let end = Instant::now() + deadline;
    loop {
        if condition()? {
            return Ok(());
        }
        let now = Instant::now();
        if now >= end {
            bail!("condition did not complete within its deadline");
        }
        thread::sleep(POLL_INTERVAL.min(end - now));
    }
}

fn wait_until_value<T>(
    deadline: Duration,
    mut value: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    let end = Instant::now() + deadline;
    loop {
        if let Some(value) = value()? {
            return Ok(value);
        }
        let now = Instant::now();
        if now >= end {
            bail!("value did not arrive within its deadline");
        }
        thread::sleep(POLL_INTERVAL.min(end - now));
    }
}

fn process_group_exists(process_group: libc::pid_t) -> bool {
    let result = unsafe { libc::kill(-process_group, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn read_files_with_suffix(root: &Path, suffix: &str) -> Result<String> {
    let mut content = String::new();
    visit_files(root, &mut |path| {
        if path.to_string_lossy().ends_with(suffix) {
            content.push_str(&fs::read_to_string(path)?);
        }
        Ok(())
    })?;
    Ok(content)
}

fn count_files_with_suffix(root: &Path, suffix: &str, excluded_suffix: &str) -> Result<usize> {
    let mut count = 0;
    visit_files(root, &mut |path| {
        let path = path.to_string_lossy();
        if path.ends_with(suffix) && !path.ends_with(excluded_suffix) {
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

fn visit_files(root: &Path, visitor: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            visit_files(&path, visitor)?;
        } else {
            visitor(&path)?;
        }
    }
    Ok(())
}
