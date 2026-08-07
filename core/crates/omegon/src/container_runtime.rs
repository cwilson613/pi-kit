//! Substrate-neutral OCI runtime discovery.

use std::process::Command;

/// Detect an available OCI runtime without coupling callers to a package provider.
pub fn detect() -> Option<String> {
    if command_available("podman") {
        Some("podman".to_string())
    } else if command_available("docker") {
        Some("docker".to_string())
    } else {
        None
    }
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_optional_and_never_panics() {
        let runtime = detect();
        assert!(
            runtime
                .as_ref()
                .is_none_or(|name| name == "podman" || name == "docker")
        );
    }
}
