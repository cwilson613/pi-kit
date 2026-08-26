#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessObservation {
    Absent,
    Present(String),
    Unavailable(String),
}

#[cfg(unix)]
pub fn current_monotonic_ns() -> Option<u64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: value is a valid writable timespec.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value) } != 0 {
        return None;
    }
    u64::try_from(value.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u64::try_from(value.tv_nsec).ok()?)
}

#[cfg(not(unix))]
pub fn current_monotonic_ns() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
pub fn current_boot_id() -> Option<String> {
    use std::ffi::CString;

    let name = CString::new("kern.boottime").ok()?;
    let mut value = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut size = std::mem::size_of::<libc::timeval>();
    // SAFETY: value/size are valid writable buffers for sysctlbyname.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut libc::timeval).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || size != std::mem::size_of::<libc::timeval>()
    {
        return None;
    }
    Some(format!("macos:{}:{}", value.tv_sec, value.tv_usec))
}

#[cfg(target_os = "linux")]
pub fn current_boot_id() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|value| format!("linux:{}", value.trim()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn current_boot_id() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
pub fn observe_process_start(pid: u32) -> ProcessObservation {
    let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
    // SAFETY: info is a valid writable proc_bsdinfo buffer.
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            std::mem::size_of::<libc::proc_bsdinfo>() as i32,
        )
    };
    if read == std::mem::size_of::<libc::proc_bsdinfo>() as i32 {
        return ProcessObservation::Present(format!(
            "macos:{}:{}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        ));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        ProcessObservation::Absent
    } else {
        ProcessObservation::Unavailable(error.to_string())
    }
}

#[cfg(target_os = "linux")]
pub fn observe_process_start(pid: u32) -> ProcessObservation {
    let value = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ProcessObservation::Absent;
        }
        Err(error) => return ProcessObservation::Unavailable(error.to_string()),
    };
    let Some(close) = value.rfind(')') else {
        return ProcessObservation::Unavailable("process stat framing is malformed".into());
    };
    let Some(start) = value[close + 1..].split_whitespace().nth(19) else {
        return ProcessObservation::Unavailable("process stat lacks field 22".into());
    };
    if start.parse::<u64>().is_err() {
        return ProcessObservation::Unavailable("process start token is malformed".into());
    }
    ProcessObservation::Present(format!("linux:{start}"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn observe_process_start(_pid: u32) -> ProcessObservation {
    ProcessObservation::Unavailable("process evidence is unsupported on this platform".into())
}
