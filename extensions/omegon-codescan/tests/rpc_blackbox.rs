use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use omegon_codescan_contracts::{
    CODESCAN_PROTOCOL_VERSION, CODESCAN_RPC_METHOD, CODESCAN_SERVICE_ID, CODESCAN_STATUS_METHOD,
};
use omegon_extension::SDK_CONTRACT_VERSION;
use serde_json::{Value, json};

fn call(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    )
    .unwrap();
    stdin.flush().unwrap();

    loop {
        let mut line = String::new();
        assert_ne!(stdout.read_line(&mut line).unwrap(), 0, "extension exited");
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["id"] == id {
            assert!(message.get("error").is_none(), "RPC failed: {message}");
            return message["result"].clone();
        }
    }
}

fn call_outcome(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    id: u64,
    params: Value,
) -> Value {
    call(stdin, stdout, id, CODESCAN_RPC_METHOD, params)
}

#[test]
fn native_process_indexes_the_assigned_workspace_and_shuts_down() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join("sidecar_fixture.rs"),
        "pub fn release_coupled_sidecar() -> bool { true }",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_omegon-codescan"))
        .arg("--rpc")
        .env_clear()
        .env("OMEGON_PROJECT_ROOT", workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let initialize = call(&mut stdin, &mut stdout, 1, "initialize", json!({}));
    assert_eq!(initialize["sdk_contract_version"], SDK_CONTRACT_VERSION);
    assert_eq!(initialize["extension_info"]["name"], "omegon-codescan");
    assert_eq!(
        initialize["extension_info"]["sdk_version"],
        SDK_CONTRACT_VERSION
    );
    assert_eq!(initialize["capabilities"]["codescan"], true);

    let status = call(
        &mut stdin,
        &mut stdout,
        2,
        CODESCAN_STATUS_METHOD,
        json!({}),
    );
    assert_eq!(status["protocol_version"], CODESCAN_PROTOCOL_VERSION);
    assert_eq!(status["service"], CODESCAN_SERVICE_ID);
    assert_eq!(status["ready"], true);

    let indexed = call(
        &mut stdin,
        &mut stdout,
        3,
        CODESCAN_RPC_METHOD,
        json!({
            "protocol_version": CODESCAN_PROTOCOL_VERSION,
            "operation": {"kind": "index", "invalidate": false}
        }),
    );
    assert_eq!(indexed["outcome"], "ok");

    let searched = call(
        &mut stdin,
        &mut stdout,
        4,
        CODESCAN_RPC_METHOD,
        json!({
            "protocol_version": CODESCAN_PROTOCOL_VERSION,
            "operation": {
                "kind": "search",
                "query": "release_coupled_sidecar",
                "scope": "code",
                "max_results": 5,
                "tags": []
            }
        }),
    );
    assert_eq!(searched["outcome"], "ok");
    assert!(
        searched["response"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["file"] == "sidecar_fixture.rs")
    );

    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn native_process_rejects_invalid_and_unsupported_requests() {
    let workspace = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_omegon-codescan"))
        .arg("--rpc")
        .env_clear()
        .env("OMEGON_PROJECT_ROOT", workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let malformed = call_outcome(
        &mut stdin,
        &mut stdout,
        1,
        json!({"protocol_version": CODESCAN_PROTOCOL_VERSION}),
    );
    assert_eq!(malformed["outcome"], "error");
    assert_eq!(malformed["error"]["code"], "invalid_request");

    let unsupported = call_outcome(
        &mut stdin,
        &mut stdout,
        2,
        json!({
            "protocol_version": CODESCAN_PROTOCOL_VERSION + 1,
            "operation": {"kind": "index", "invalidate": false}
        }),
    );
    assert_eq!(unsupported["outcome"], "error");
    assert_eq!(unsupported["error"]["code"], "unsupported_protocol");

    drop(stdin);
    assert!(child.wait().unwrap().success());
}
