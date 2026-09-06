//! Deterministic fake MCP wire fixtures; no external process or network required.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn phase_shutdown_releases_registry_before_settlement() {
    let (client, _server) = fake_connection(None).await;
    let feature = feature(client, 30);
    let supervisor = feature.supervisor();
    let mut shutdown = Box::pin(supervisor.shutdown(Duration::from_secs(10)));
    // Poll once to enter shutdown and suspend on the service task's settlement.
    // Calls acquiring a peer must not wait behind that asynchronous cleanup.
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(shutdown.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert!(
        feature.clients.try_lock().is_ok(),
        "shutdown held registry while awaiting service settlement"
    );
    assert!(shutdown.await.is_empty());
}

struct FakeServer {
    task: tokio::task::JoinHandle<()>,
    observed: tokio::sync::mpsc::UnboundedReceiver<Value>,
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn fake_connection(stall: Option<&'static str>) -> (McpConnection, FakeServer) {
    let (client_io, server_io) = tokio::io::duplex(16384);
    let (read, write) = tokio::io::split(server_io);
    let write = Arc::new(Mutex::new(write));
    let (events, observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut lines = BufReader::new(read).lines();
        let mut jobs = tokio::task::JoinSet::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            let request: Value = serde_json::from_str(&line).unwrap();
            events.send(request.clone()).unwrap();
            let Some(id) = request.get("id").cloned() else {
                continue;
            };
            let write = Arc::clone(&write);
            jobs.spawn(async move {
                let method = request["method"].as_str().unwrap();
                if stall == Some(method) {
                    if request["params"]["cursor"].is_string() {
                        std::future::pending::<()>().await;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let mut result = match method {
                    "initialize" => json!({"protocolVersion":"2024-11-05", "capabilities":{"tools":{},"resources":{},"prompts":{}}, "serverInfo":{"name":"fake","version":"1"}}),
                    "tools/list" => {
                        json!({"tools":[{"name":"slow","inputSchema":{"type":"object"}}]})
                    }
                    "resources/list" => json!({"resources":[]}),
                    "resources/templates/list" => json!({"resourceTemplates":[]}),
                    "prompts/list" => json!({"prompts":[]}),
                    "tools/call" | "resources/read" | "prompts/get" => {
                        for progress in 1..=3 {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            let notice = json!({"jsonrpc":"2.0", "method":"notifications/progress", "params":{"progressToken":request["params"]["_meta"]["progressToken"], "progress":progress, "total":3}});
                            write.lock().await.write_all(format!("{notice}\n").as_bytes()).await.unwrap();
                        }
                        match method {
                            "tools/call" => json!({"content":[{"type":"text","text":"complete"}]}),
                            "resources/read" => json!({"contents":[{"uri":"test://resource", "text":"complete"}]}),
                            _ => json!({"messages":[{"role":"user","content":{"type":"text","text":"complete"}}]}),
                        }
                    }
                    _ => panic!("unexpected fake server method {method}"),
                };
                if stall == Some(method) { result["nextCursor"] = json!("second-page"); }
                let response = json!({"jsonrpc":"2.0", "id":id, "result":result});
                write.lock().await.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            });
        }
    });
    let service = service::serve_client(
        OmegonMcpClient {
            progress: Arc::new(ProgressRegistry::default()),
        },
        client_io,
    )
    .await
    .unwrap();
    (
        McpConnection {
            service,
            _process_group: McpProcessGroup::default(),
        },
        FakeServer { task, observed },
    )
}

fn feature(client: McpConnection, execution_secs: u64) -> McpFeature {
    McpFeature {
        feature_name: "fixture".into(),
        tools: vec![],
        resources: vec![],
        resource_templates: vec![],
        prompts: vec![],
        clients: Arc::new(Mutex::new(HashMap::from([("fake".into(), client)]))),
        timeouts: HashMap::from([("fake".into(), execution_secs)]),
        progress: Arc::new(ProgressRegistry::default()),
        host_action_policies: HashMap::new(),
        admission: crate::dynamic_admission::DynamicAdmissionPermit::for_test_id(
            "mcp:test",
            omegon_traits::RuntimeDynamicSourceKind::McpProcess,
        )
        .unwrap(),
        readiness_timeout_ms: 2_000,
    }
}

#[test]
fn phase_defaults_overrides_and_legacy_zero() {
    let legacy: McpServerConfig = toml::from_str("command='fake'").unwrap();
    assert_eq!(
        (
            legacy.startup_secs(),
            legacy.catalog_secs(),
            legacy.execution_secs(),
            legacy.readiness_ms()
        ),
        (30, 30, 30, 30_000)
    );
    let partial: McpServerConfig =
        toml::from_str("command='fake'\ntimeout_secs=30\nexecution_timeout_secs=90").unwrap();
    assert_eq!(
        (
            partial.startup_secs(),
            partial.catalog_secs(),
            partial.execution_secs()
        ),
        (30, 30, 90)
    );
    let explicit: McpServerConfig = toml::from_str(
        "command='fake'\nstartup_timeout_secs=2\ncatalog_timeout_secs=3\nexecution_timeout_secs=9",
    )
    .unwrap();
    assert_eq!(
        (
            explicit.startup_secs(),
            explicit.catalog_secs(),
            explicit.execution_secs(),
            explicit.readiness_ms()
        ),
        (2, 3, 9, 5000)
    );
    let zero: McpServerConfig = toml::from_str("command='fake'\ntimeout_secs=0").unwrap();
    assert_eq!(
        (
            zero.startup_secs(),
            zero.catalog_secs(),
            zero.execution_secs(),
            zero.readiness_ms()
        ),
        (1, 1, 0, 1)
    );
}

#[tokio::test(start_paused = true)]
async fn slow_tool_resource_and_prompt_use_execution_budget() {
    let (client, _server) = fake_connection(None).await;
    let feature = feature(client, 5);
    let cancel = CancellationToken::new();
    let start = tokio::time::Instant::now();
    let resource_args = json!({"server":"fake", "uri":"test://resource"});
    let prompt_args = json!({"server":"fake", "name":"slow"});
    let (tool, resource, prompt) = tokio::join!(
        feature.execute_with_context(
            "fake::slow",
            "tool",
            json!({}),
            cancel.clone(),
            ToolProgressSink::noop(),
            ToolExecutionContext::default()
        ),
        feature.execute_read_resource(&resource_args, &cancel),
        feature.execute_get_prompt(&prompt_args, &cancel),
    );
    assert!(tool.is_ok(), "{tool:?}");
    assert!(resource.is_ok(), "{resource:?}");
    assert!(prompt.is_ok(), "{prompt:?}");
    assert_eq!(
        start.elapsed(),
        Duration::from_secs(3),
        "calls must run concurrently and exceed readiness budget"
    );
}

#[tokio::test(start_paused = true)]
async fn progress_does_not_extend_deadline_or_kill_unrelated_call() {
    let (client, mut server) = fake_connection(None).await;
    let peer = client.peer().clone();
    let cancel = CancellationToken::new();
    let request =
        || ClientRequest::CallToolRequest(CallToolRequest::new(CallToolRequestParams::new("slow")));
    let start = tokio::time::Instant::now();
    let (expired, completed) = tokio::join!(
        execute_request(&peer, "fake", 2, request(), &cancel),
        execute_request(&peer, "fake", 5, request(), &cancel)
    );
    let error = expired.unwrap_err().to_string();
    assert!(error.contains("execution timed out after 2s"), "{error}");
    assert!(error.contains("cancellation notification sent"), "{error}");
    assert!(error.contains("termination not confirmed"), "{error}");
    assert!(completed.is_ok(), "{completed:?}");
    assert_eq!(start.elapsed(), Duration::from_secs(3));
    let mut cancellation_seen = false;
    while let Ok(event) = server.observed.try_recv() {
        cancellation_seen |= event["method"] == "notifications/cancelled";
    }
    assert!(
        cancellation_seen,
        "fake server must observe request cancellation"
    );
}

#[tokio::test(start_paused = true)]
async fn operator_cancel_settles_before_execution_deadline() {
    let (client, mut server) = fake_connection(None).await;
    let cancel = CancellationToken::new();
    let operation = execute_request(
        client.peer(),
        "fake",
        90,
        ClientRequest::CallToolRequest(CallToolRequest::new(CallToolRequestParams::new("slow"))),
        &cancel,
    );
    let cancellation = async {
        while let Some(event) = server.observed.recv().await {
            if event["method"] == "tools/call" {
                cancel.cancel();
                break;
            }
        }
    };
    let start = tokio::time::Instant::now();
    let (result, _) = tokio::join!(operation, cancellation);
    let error = result.unwrap_err().to_string();
    assert!(error.contains("execution cancelled"), "{error}");
    assert!(error.contains("termination not confirmed"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn catalog_stalled_next_page_uses_one_phase_deadline() {
    for method in [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ] {
        let (client, mut server) = fake_connection(Some(method)).await;
        let start = tokio::time::Instant::now();
        let result = discover_catalog(
            "fake",
            client.peer(),
            phase_deadline("fake", "catalog", 2).unwrap(),
            2,
            false,
        )
        .await;
        let error = result.err().expect("second page must time out").to_string();
        assert!(error.contains("catalog timed out after 2s"), "{error}");
        assert_eq!(start.elapsed(), Duration::from_secs(2));
        let mut second_page_seen = false;
        while let Ok(event) = server.observed.try_recv() {
            second_page_seen |=
                event["method"] == method && event["params"]["cursor"] == "second-page";
        }
        assert!(second_page_seen, "catalog must request the subsequent page");
    }
}

#[tokio::test(start_paused = true)]
async fn complete_catalog_discovers_all_inventory_kinds() {
    let (client, mut server) = fake_connection(None).await;
    let catalog = discover_catalog(
        "fake",
        client.peer(),
        phase_deadline("fake", "catalog", 1).unwrap(),
        1,
        false,
    )
    .await
    .unwrap();
    assert_eq!(catalog.tools.len(), 1);
    let mut methods = Vec::new();
    while let Ok(event) = server.observed.try_recv() {
        methods.push(event["method"].as_str().unwrap().to_owned());
    }
    for method in [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ] {
        assert!(
            methods.iter().any(|observed| observed == method),
            "missing {method}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn startup_timeout_cleans_owned_descendant_process() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("descendant.pid");
    let mut config: McpServerConfig =
        toml::from_str("command='/bin/sh'\nstartup_timeout_secs=1\ncatalog_timeout_secs=1")
            .unwrap();
    config.args = vec![
        "-c".into(),
        "sleep 60 & echo $! > \"$1\"; wait".into(),
        "fixture".into(),
        pid_file.display().to_string(),
    ];
    let result = McpFeature::connect_one(
        "stall",
        &config,
        None,
        Arc::new(ProgressRegistry::default()),
        phase_deadline("stall", "startup", 1).unwrap(),
    )
    .await;
    let error = result
        .err()
        .expect("uninitialized process must time out")
        .to_string();
    assert!(error.contains("startup timed out after 1s"), "{error}");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("fixture must spawn descendant")
        .trim()
        .parse()
        .unwrap();
    for _ in 0..100 {
        // SAFETY: signal zero only tests existence of the fixture's recorded pid.
        if unsafe { libc::kill(pid, 0) } == -1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("MCP descendant {pid} remains alive after startup timeout");
}

#[test]
fn absent_phase_budgets_roundtrip_as_json_null() {
    let config: McpServerConfig = toml::from_str("command='fake'").unwrap();
    let value = serde_json::to_value(&config).unwrap();
    assert!(value["startup_timeout_secs"].is_null());
    let reloaded: McpServerConfig = serde_json::from_value(value).unwrap();
    assert_eq!(reloaded.startup_secs(), 30);
    assert_eq!(reloaded.catalog_secs(), 30);
    assert_eq!(reloaded.execution_secs(), 30);
}

#[test]
fn json_invalid_phase_budgets_identify_the_phase() {
    for phase in ["startup", "catalog", "execution"] {
        for value in [json!(0), json!(-1), json!("bad"), json!(u64::MAX)] {
            let field = format!("{phase}_timeout_secs");
            let input = json!({"command":"fake", field.clone(): value});
            let error = serde_json::from_value::<McpServerConfig>(input)
                .unwrap_err()
                .to_string();
            assert!(error.contains(&field), "{error}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn legacy_optional_catalog_timeout_preserves_discovered_tools() {
    for method in ["resources/list", "resources/templates/list", "prompts/list"] {
        let (client, _server) = fake_connection(Some(method)).await;
        let start = tokio::time::Instant::now();
        let catalog = discover_catalog(
            "fake",
            client.peer(),
            phase_deadline("fake", "catalog", 2).unwrap(),
            2,
            true,
        )
        .await
        .expect("legacy optional catalog stalls must not discard available tools");
        assert_eq!(catalog.tools.len(), 1);
        assert_eq!(catalog.tools[0].name, "fake::slow");
        assert_eq!(start.elapsed(), Duration::from_secs(2));
    }
}
