---
task_id: 1
label: ipc-web-bridge
siblings: [0:executor-core, 2:tests-docs]
---

# Task 1: ipc-web-bridge

## Root Directive

> Implement a canonical slash command executor layer so TUI, IPC run_slash_command, and web WebSocket slash_command all share one command semantics path. Remote callers must reach model switching, context status/compact/clear, new session, sessions, and auth flows through structured outcomes instead of bypassing to BusCommand only.

## Mission

Update IPC run_slash_command and web WebSocket slash_command handling to use the new shared slash command executor path instead of emitting BusCommand directly. Ensure structured results are returned or surfaced consistently for remote callers.

## Scope

- `core/crates/omegon/src/ipc/connection.rs`
- `core/crates/omegon/src/web/ws.rs`
- `core/crates/omegon/src/main.rs`

**Depends on:** executor-core

## Siblings

- **executor-core**: Create a shared slash command executor abstraction in the Omegon core that can execute canonical slash semantics without going through the TUI-only parser. It must represent structured outcomes and cover model switching, context status/compact/clear, new session, sessions, bus command forwarding, and auth command routing with explicit remote-safe vs interactive-only outcomes.
- **tests-docs**: Add or update tests covering parity between TUI, IPC, and web slash execution for at least model switching, context compact/clear, and remote handling of auth/login paths. Update any command-surface assertions affected by the new shared executor semantics.

## Dependency Versions

Use these exact versions — do not rely on training data for API shapes:

```toml
# core/crates/omegon/Cargo.toml
[dependencies]
omegon-extension = { path = "../omegon-extension" }
omegon-traits = { path = "../omegon-traits" }
omegon-git = { path = "../omegon-git" }
omegon-memory = { path = "../omegon-memory" }
omegon-codescan = { path = "../omegon-codescan" }
omegon-secrets = { path = "../omegon-secrets" }
opsx-core = { path = "../opsx-core" }
tokio = { workspace = true }
serde = { workspace = true }
toml = "0.8"
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
async-trait = { workspace = true }
clap = { workspace = true }
rusqlite = { workspace = true }
tokio-util = { workspace = true }
indexmap = { workspace = true }
dirs = "6.0.0"
unicode-truncate = "2.0"
chrono = "0.4"
libc = "0.2"
regex-lite = "0.1"
ratatui = "0.30.0"
syntect = { version = "5", default-features = false, features = ["default-syntaxes", "default-themes", "regex-onig"] }
tui-syntax-highlight = "0.2"
tachyonfx = { version = "0.25.0", features = ["sendable"] }
crossterm = "0.29.0"
reqwest = { version = "0.13.2", features = ["json", "stream"] }
tokio-stream = "0.1.18"
sha2 = "0.10.9"
secrecy = "0.10"
sysinfo = "0.33"
getrandom = "0.4.2"
open = "5.3.3"
tracing-appender = "0.2.4"
unicode-width = "0.2.2"
ratatui-image = { version = "10.0.6", default-features = false, features = ["crossterm", "image-defaults"] }
image = { version = "0.25.10", default-features = false, features = ["png", "jpeg", "gif", "webp"] }
axum = { version = "0.8.8", features = ["ws", "macros"] }
tower-http = { version = "0.6.8", features = ["cors"] }
futures-util = "0.3.32"
base64 = "0.22"
hmac = "0.12"
ansi-to-tui = "8.0"
tui-tree-widget = "0.24"
ratatui-toaster = "0.1"
ratatui-textarea = { version = "0.8", features = ["crossterm"] }
tui-popup = "0.7"
hyperrat = "0.1"
rmcp = { version = "1.2", features = ["transport-child-process", "client", "transport-streamable-http-client-reqwest", "auth"], default-features = false }
tar = "0.4"
flate2 = "1.0"
sigstore = { version = "0.13.0", default-features = false, features = ["cosign", "rustls-tls"] }
x509-parser = "0.17"
rpassword = "7"

[dev-dependencies]
insta = "1.46"
tempfile = "3.27.0"

```

## Test Convention

Follow this pattern from an existing test in the same crate:

```rust
// From bridge.rs
    #[test]
    fn llm_message_user_round_trip() {
        let msg = LlmMessage::User {
            content: "hello".into(),
            images: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user"#));
        let parsed: LlmMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            LlmMessage::User { content, .. } => assert_eq!(content, "hello"),
            _ => panic!("wrong variant"),
        }
    }
```



## Testing Requirements

### Test Convention

Write tests as #[test] functions in the same file or a tests submodule

Example from codebase:

```rust
// From bridge.rs
    #[test]
    fn llm_message_user_round_trip() {
        let msg = LlmMessage::User {
            content: "hello".into(),
            images: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user"#));
        let parsed: LlmMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            LlmMessage::User { content, .. } => assert_eq!(content, "hello"),
            _ => panic!("wrong variant"),
        }
    }
```


## Contract

1. Only work on files within your scope
2. Follow the Testing Requirements section above
3. If the task is too complex, set status to NEEDS_DECOMPOSITION

## Finalization (REQUIRED before completion)

You MUST complete these steps before finishing:

1. Run all guardrail checks listed above and fix failures
2. Commit your in-scope work with a clean git state when you are done
3. Commit with a clear message: `git commit -m "feat(<label>): <summary>"`
4. Verify clean state: `git status` should show nothing to commit

Do NOT edit `.cleave-prompt.md` or any task/result metadata files. Those are orchestrator-owned and may be ignored by git.
Return your completion summary in your normal final response instead of modifying the prompt file.

> ⚠️ Uncommitted work will be lost. The orchestrator merges from your branch's commits.

## Result

**Status:** PENDING

**Summary:**

**Artifacts:**

**Decisions Made:**

**Assumptions:**
