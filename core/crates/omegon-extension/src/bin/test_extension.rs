//! Minimal test extension for integration testing.
//!
//! Speaks v2 protocol: initialize → tools/list → tools/call.
//! Provides tools: echo, slow_echo, progress_echo.

use async_trait::async_trait;
use omegon_extension::{Extension, HostProxy};
use serde_json::{Value, json};
use std::sync::OnceLock;

struct TestExtension {
    host: OnceLock<HostProxy>,
}

impl Default for TestExtension {
    fn default() -> Self {
        Self {
            host: OnceLock::new(),
        }
    }
}

#[async_trait]
impl Extension for TestExtension {
    fn name(&self) -> &str {
        "test-extension"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn handle_rpc(&self, method: &str, params: Value) -> omegon_extension::Result<Value> {
        match method {
            "initialize" => Ok(json!({
                "protocol_version": 2,
                "extension_info": {
                    "name": "test-extension",
                    "version": "0.1.0",
                    "sdk_version": "0.16.0"
                },
                "capabilities": {
                    "tools": true,
                    "widgets": false,
                    "mind": false,
                    "vox": false,
                    "resources": false,
                    "prompts": false,
                    "sampling": false,
                    "elicitation": false,
                    "streaming": true
                },
                "tools": self.tool_defs()
            })),

            "get_tools" | "tools/list" => Ok(Value::Array(self.tool_defs())),

            "bootstrap_secrets" => Ok(json!({"acknowledged": true})),

            "execute_tool" | "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params
                    .get("args")
                    .or(params.get("arguments"))
                    .cloned()
                    .unwrap_or(json!({}));
                let meta = params.get("_meta").cloned().unwrap_or(json!({}));

                match name {
                    "echo" => {
                        let message = args
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(empty)");
                        Ok(json!({"content": [{"type": "text", "text": message}]}))
                    }
                    "slow_echo" => {
                        let message = args
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(empty)");
                        let delay = args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(100);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        Ok(json!({"content": [{"type": "text", "text": message}]}))
                    }
                    "progress_echo" => {
                        // Emit progress notifications if a progress_token is provided.
                        let message = args
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(empty)");
                        let steps = args.get("steps").and_then(|v| v.as_u64()).unwrap_or(3);

                        if let Some(host) = self.host.get() {
                            if let Some(token) = meta.get("progress_token").and_then(|v| v.as_str())
                            {
                                for i in 1..=steps {
                                    let _ = host.notify(
                                        "notifications/tools/progress",
                                        json!({
                                            "progress_token": token,
                                            "progress": i,
                                            "total": steps,
                                            "phase": format!("step {}/{}", i, steps),
                                            "content": [{"type": "text", "text": format!("progress {}/{}", i, steps)}]
                                        }),
                                    ).await;
                                }
                            }
                        }

                        Ok(json!({"content": [{"type": "text", "text": message}]}))
                    }
                    _ => Err(omegon_extension::Error::method_not_found(&format!(
                        "tool '{name}'"
                    ))),
                }
            }

            _ => Err(omegon_extension::Error::method_not_found(method)),
        }
    }

    async fn on_initialized(&self, host: HostProxy) {
        let _ = self.host.set(host);
    }
}

impl TestExtension {
    fn tool_defs(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "echo",
                "label": "Echo",
                "description": "Returns the input arguments as-is",
                "parameters": {"type": "object", "properties": {"message": {"type": "string"}}}
            }),
            json!({
                "name": "slow_echo",
                "label": "Slow Echo",
                "description": "Returns after a short delay",
                "parameters": {"type": "object", "properties": {"message": {"type": "string"}, "delay_ms": {"type": "integer"}}}
            }),
            json!({
                "name": "progress_echo",
                "label": "Progress Echo",
                "description": "Emits progress notifications then returns",
                "parameters": {"type": "object", "properties": {"message": {"type": "string"}, "steps": {"type": "integer"}}}
            }),
        ]
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--rpc") {
        eprintln!("test-extension: must be invoked with --rpc");
        std::process::exit(1);
    }

    omegon_extension::serve_v2(TestExtension::default())
        .await
        .expect("serve failed");
}
