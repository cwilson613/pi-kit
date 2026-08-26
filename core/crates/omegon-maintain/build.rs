use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let sha = git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&[
        "status",
        "--porcelain",
        "--untracked-files=no",
        "--ignored=no",
    ])
    .map(|status| if status.is_empty() { "" } else { "-dirty" })
    .unwrap_or("");
    println!("cargo:rustc-env=OMEGON_MAINTAIN_GIT_SHA={sha}{dirty}");
    println!(
        "cargo:rustc-env=OMEGON_MAINTAIN_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
    println!("cargo:rerun-if-changed=../../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    if let Some(head_ref) = git(&["symbolic-ref", "--short", "HEAD"]) {
        println!("cargo:rerun-if-changed=../../../.git/refs/heads/{head_ref}");
    }
}
