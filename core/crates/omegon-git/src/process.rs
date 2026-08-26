//! Synchronous, cancellation-aware process-tree execution for Git and JJ.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static OUTPUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn run<I, S>(
    program: &str,
    args: I,
    cwd: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if cancelled() {
        anyhow::bail!("{program} operation cancelled");
    }
    let sequence = OUTPUT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let prefix = format!("omegon-git-{}-{sequence}", std::process::id());
    let stdout_path = std::env::temp_dir().join(format!("{prefix}.stdout"));
    let stderr_path = std::env::temp_dir().join(format!("{prefix}.stderr"));
    let _cleanup = OutputCleanup([stdout_path.clone(), stderr_path.clone()]);
    let stdout = output_file(&stdout_path)?;
    let stderr = output_file(&stderr_path)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    configure_tree(&mut command);
    let child = command
        .spawn()
        .with_context(|| format!("failed to execute {program}"))?;
    let mut owned = OwnedProcessTree::new(child)?;
    let result = loop {
        if cancelled() {
            owned.terminate();
            let _ = owned.wait();
            break Err(anyhow::anyhow!("{program} operation cancelled"));
        }
        match owned.try_wait().context("failed to poll child process")? {
            Some(status) => break Ok(status),
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    // Closing the tree owner kills and joins any descendants that outlived the
    // direct child. No process may retain the output files past this point.
    owned.terminate_descendants();
    drop(owned);
    let stdout = std::fs::read(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read(&stderr_path).unwrap_or_default();
    result.map(|status| Output {
        status,
        stdout,
        stderr,
    })
}

struct OutputCleanup([std::path::PathBuf; 2]);

impl Drop for OutputCleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn output_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create process output file {}", path.display()))
}

#[cfg(unix)]
fn configure_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_tree(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
}

struct OwnedProcessTree {
    child: std::process::Child,
    #[cfg(unix)]
    process_group: i32,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

impl OwnedProcessTree {
    fn new(mut child: std::process::Child) -> Result<Self> {
        #[cfg(unix)]
        {
            let process_group = match i32::try_from(child.id()).context("child PID exceeds i32") {
                Ok(process_group) => process_group,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            Ok(Self {
                child,
                process_group,
            })
        }
        #[cfg(windows)]
        {
            use std::mem::{size_of, zeroed};
            use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            };
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
            };
            // SAFETY: handles are checked and closed on every failure path. The
            // initialized structure is the documented input for this job class.
            unsafe {
                let job = windows_sys::Win32::System::JobObjects::CreateJobObjectW(
                    std::ptr::null(),
                    std::ptr::null(),
                );
                if job.is_null() {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("failed to create Git process Job Object");
                }
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == FALSE
                {
                    CloseHandle(job);
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("failed to configure Git process Job Object");
                }
                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, FALSE, child.id());
                if process.is_null() || AssignProcessToJobObject(job, process) == FALSE {
                    if !process.is_null() {
                        CloseHandle(process);
                    }
                    CloseHandle(job);
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("failed to assign Git process to Job Object");
                }
                CloseHandle(process);
                Ok(Self { child, job })
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self { child })
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait()
    }

    fn terminate(&mut self) {
        self.terminate_descendants();
        let _ = self.child.kill();
    }

    fn terminate_descendants(&mut self) {
        #[cfg(unix)]
        // SAFETY: the child was created as leader of this dedicated process
        // group. ESRCH means every member has already exited.
        unsafe {
            libc::kill(-self.process_group, libc::SIGKILL);
        }
        #[cfg(windows)]
        // SAFETY: this handle exclusively owns the job assigned above.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}

impl Drop for OwnedProcessTree {
    fn drop(&mut self) {
        self.terminate();
        let _ = self.child.wait();
        #[cfg(windows)]
        // SAFETY: `job` is a live owned handle and is closed exactly once.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn captures_output_and_settles() {
        let output = if cfg!(windows) {
            run("cmd", ["/C", "echo", "managed"], Path::new("."), &|| false)
        } else {
            run("sh", ["-c", "printf managed"], Path::new("."), &|| false)
        }
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("managed"));
    }

    #[test]
    fn cancellation_reaps_process_tree() {
        let cancelled = AtomicBool::new(false);
        let calls = AtomicU64::new(0);
        let result = if cfg!(windows) {
            run(
                "cmd",
                ["/C", "ping", "-n", "10", "127.0.0.1"],
                Path::new("."),
                &|| cancelled.load(Ordering::Acquire) || calls.fetch_add(1, Ordering::AcqRel) > 1,
            )
        } else {
            run("sh", ["-c", "sleep 30 & wait"], Path::new("."), &|| {
                cancelled.load(Ordering::Acquire) || calls.fetch_add(1, Ordering::AcqRel) > 1
            })
        };
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn production_git_and_jj_processes_use_tree_owner() {
        for (name, source) in [
            ("repo.rs", include_str!("repo.rs")),
            ("commit.rs", include_str!("commit.rs")),
            ("merge.rs", include_str!("merge.rs")),
            ("worktree.rs", include_str!("worktree.rs")),
            ("submodule.rs", include_str!("submodule.rs")),
            ("jj.rs", include_str!("jj.rs")),
        ] {
            let production = source.split("#[cfg(test)]").next().unwrap_or(source);
            assert!(
                !production.contains("Command::new(\"git\")")
                    && !production.contains("Command::new(\"jj\")"),
                "{name} launches an unowned Git/JJ process"
            );
        }
    }
}
