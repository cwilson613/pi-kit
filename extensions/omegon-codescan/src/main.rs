use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use omegon_codescan::{BM25Index, Indexer, ScanCache};
use omegon_codescan_contracts::{
    CODESCAN_PROTOCOL_VERSION, CODESCAN_RPC_METHOD, CODESCAN_STATUS_METHOD, CodescanCancelV1,
    CodescanErrorCodeV1, CodescanOperationV1, CodescanOutcomeV1, CodescanRequestV1,
    CodescanResponseV1, CodescanStatusV1, SearchResponseV1,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const EXTENSION_NAME: &str = "omegon-codescan";

struct EngineJob {
    request_id: u64,
    request: CodescanRequestV1,
    cancelled: Arc<AtomicBool>,
}

struct EngineReply {
    request_id: u64,
    outcome: CodescanOutcomeV1,
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() != Some("--rpc") {
        eprintln!("omegon-codescan must be launched by Omegon with --rpc");
        std::process::exit(2);
    }
    if let Err(error) = serve().await {
        eprintln!("omegon-codescan RPC server failed: {error:#}");
        std::process::exit(1);
    }
}

async fn serve() -> anyhow::Result<()> {
    let workspace = std::env::var_os("OMEGON_PROJECT_ROOT")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("OMEGON_PROJECT_ROOT is required"))?;
    let workspace = workspace.canonicalize().unwrap_or(workspace);
    let (jobs, receiver) = std::sync::mpsc::sync_channel::<EngineJob>(16);
    let (replies, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<EngineReply>();
    let (startup_tx, startup_rx) = std::sync::mpsc::sync_channel(1);
    let worker_workspace = workspace.clone();
    let worker = std::thread::Builder::new()
        .name("omegon-codescan-engine".into())
        .spawn(move || run_engine(worker_workspace, receiver, replies, startup_tx))?;

    tokio::task::spawn_blocking(move || startup_rx.recv())
        .await
        .map_err(|error| anyhow::anyhow!("codescan readiness task failed: {error}"))??
        .map_err(anyhow::Error::msg)?;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();
    let mut active = HashMap::<u64, Arc<AtomicBool>>::new();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let message: Value = match serde_json::from_str(&line) {
                    Ok(message) => message,
                    Err(error) => {
                        write_message(&mut stdout, &rpc_error(Value::Null, -32700, error.to_string())).await?;
                        continue;
                    }
                };
                let method = message.get("method").and_then(Value::as_str).unwrap_or("");
                let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
                let id = message.get("id").cloned();

                if id.is_none() {
                    if method == "notifications/cancelled" {
                        cancel_request(&active, params);
                    }
                    continue;
                }

                let id_value = id.expect("checked above");
                let Some(request_id) = id_value.as_u64() else {
                    write_message(&mut stdout, &rpc_error(id_value, -32600, "numeric request id required")).await?;
                    continue;
                };
                match method {
                    "initialize" => {
                        write_message(&mut stdout, &rpc_result(id_value, json!({
                            "protocol_version": 2,
                            "extension_info": {
                                "name": EXTENSION_NAME,
                                "version": env!("CARGO_PKG_VERSION"),
                                "codescan_protocol_version": CODESCAN_PROTOCOL_VERSION
                            },
                            "capabilities": {"tools": false, "codescan": true},
                            "tools": []
                        }))).await?;
                    }
                    "get_tools" | "tools/list" => {
                        write_message(&mut stdout, &rpc_result(id_value, json!([]))).await?;
                    }
                    "bootstrap_config" | "bootstrap_secrets" => {
                        write_message(&mut stdout, &rpc_result(id_value, json!({"acknowledged": true}))).await?;
                    }
                    CODESCAN_STATUS_METHOD => {
                        write_message(&mut stdout, &rpc_result(id_value, serde_json::to_value(CodescanStatusV1::ready())?)).await?;
                    }
                    CODESCAN_RPC_METHOD => {
                        let request = match serde_json::from_value::<CodescanRequestV1>(params) {
                            Ok(request) => request,
                            Err(error) => {
                                let outcome = CodescanOutcomeV1::failure(CodescanErrorCodeV1::InvalidRequest, error.to_string());
                                write_message(&mut stdout, &rpc_result(id_value, serde_json::to_value(outcome)?)).await?;
                                continue;
                            }
                        };
                        if request.protocol_version != CODESCAN_PROTOCOL_VERSION {
                            let outcome = CodescanOutcomeV1::failure(
                                CodescanErrorCodeV1::UnsupportedProtocol,
                                format!("unsupported codescan protocol {}", request.protocol_version),
                            );
                            write_message(&mut stdout, &rpc_result(id_value, serde_json::to_value(outcome)?)).await?;
                            continue;
                        }
                        let cancelled = Arc::new(AtomicBool::new(false));
                        active.insert(request_id, Arc::clone(&cancelled));
                        if let Err(error) = jobs.try_send(EngineJob { request_id, request, cancelled }) {
                            active.remove(&request_id);
                            let message = match error {
                                std::sync::mpsc::TrySendError::Full(_) => "codescan engine worker is busy",
                                std::sync::mpsc::TrySendError::Disconnected(_) => "codescan engine worker is unavailable",
                            };
                            let outcome = CodescanOutcomeV1::failure(CodescanErrorCodeV1::Internal, message);
                            write_message(&mut stdout, &rpc_result(id_value, serde_json::to_value(outcome)?)).await?;
                        }
                    }
                    _ => {
                        write_message(&mut stdout, &rpc_error(id_value, -32601, format!("method not found: {method}"))).await?;
                    }
                }
            }
            reply = reply_rx.recv() => {
                let Some(reply) = reply else { break };
                active.remove(&reply.request_id);
                write_message(
                    &mut stdout,
                    &rpc_result(json!(reply.request_id), serde_json::to_value(reply.outcome)?),
                ).await?;
            }
        }
    }

    for cancelled in active.values() {
        cancelled.store(true, Ordering::Release);
    }
    drop(jobs);
    tokio::task::spawn_blocking(move || worker.join())
        .await
        .map_err(|error| anyhow::anyhow!("codescan worker join failed: {error}"))?
        .map_err(|_| anyhow::anyhow!("codescan worker panicked"))?;
    Ok(())
}

fn cancel_request(active: &HashMap<u64, Arc<AtomicBool>>, params: Value) {
    let Ok(cancel) = serde_json::from_value::<CodescanCancelV1>(params) else {
        return;
    };
    if cancel.protocol_version != CODESCAN_PROTOCOL_VERSION {
        return;
    }
    if let Some(cancelled) = active.get(&cancel.request_id) {
        cancelled.store(true, Ordering::Release);
    }
}

fn run_engine(
    workspace: PathBuf,
    jobs: std::sync::mpsc::Receiver<EngineJob>,
    replies: tokio::sync::mpsc::UnboundedSender<EngineReply>,
    startup: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let db_path = workspace.join(".omegon/codescan.db");
    let mut cache = match ScanCache::open(&db_path) {
        Ok(cache) => {
            let _ = startup.send(Ok(()));
            cache
        }
        Err(error) => {
            let _ = startup.send(Err(error.to_string()));
            return;
        }
    };

    while let Ok(job) = jobs.recv() {
        let outcome = execute_operation(&workspace, &mut cache, job.request.operation, || {
            job.cancelled.load(Ordering::Acquire)
        });
        let _ = replies.send(EngineReply {
            request_id: job.request_id,
            outcome,
        });
    }
}

fn execute_operation(
    workspace: &Path,
    cache: &mut ScanCache,
    operation: CodescanOperationV1,
    is_cancelled: impl Fn() -> bool,
) -> CodescanOutcomeV1 {
    if is_cancelled() {
        return CodescanOutcomeV1::failure(CodescanErrorCodeV1::Cancelled, "request cancelled");
    }
    let result = match operation {
        CodescanOperationV1::Search(request) => {
            let within = request.within.as_deref().map(Path::new);
            Indexer::run_with_cancel(workspace, cache, &is_cancelled).and_then(|_| {
                let code_chunks = cache
                    .all_code_chunks()?
                    .into_iter()
                    .filter(|chunk| within.is_none_or(|prefix| chunk.path.starts_with(prefix)))
                    .collect::<Vec<_>>();
                let mut knowledge_chunks = cache
                    .all_knowledge_chunks()?
                    .into_iter()
                    .filter(|chunk| within.is_none_or(|prefix| chunk.path.starts_with(prefix)))
                    .collect::<Vec<_>>();
                if !request.tags.is_empty() {
                    knowledge_chunks
                        .retain(|chunk| request.tags.iter().any(|tag| chunk.tags.contains(tag)));
                }
                let indexed_code_chunks = code_chunks.len();
                let indexed_knowledge_chunks = knowledge_chunks.len();
                let results =
                    BM25Index::build_with_cancel(&code_chunks, &knowledge_chunks, &is_cancelled)?
                        .search_with_cancel(
                        &request.query,
                        request.scope,
                        request.max_results,
                        &is_cancelled,
                    )?;
                Ok(CodescanResponseV1::Search(SearchResponseV1 {
                    results,
                    indexed_code_chunks,
                    indexed_knowledge_chunks,
                }))
            })
        }
        CodescanOperationV1::Index(request) => {
            let result = if request.invalidate {
                cache.begin_full_rebuild()
            } else {
                Ok(())
            };
            result.and_then(|_| {
                Indexer::run_with_cancel(workspace, cache, &is_cancelled)
                    .map(CodescanResponseV1::Index)
            })
        }
    };

    match result {
        Ok(response) => CodescanOutcomeV1::success(response),
        Err(_error) if is_cancelled() => {
            CodescanOutcomeV1::failure(CodescanErrorCodeV1::Cancelled, "request cancelled")
        }
        Err(error) => CodescanOutcomeV1::failure(CodescanErrorCodeV1::Internal, error.to_string()),
    }
}

async fn write_message(stdout: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
    stdout
        .write_all(serde_json::to_string(message)?.as_bytes())
        .await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message.into()}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omegon_codescan_contracts::{IndexRequestV1, SearchRequestV1, SearchScope};

    #[test]
    fn index_and_search_share_one_cache() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("sample.rs"),
            "pub fn extension_boundary() -> bool { true }",
        )
        .unwrap();
        let mut cache = ScanCache::open(&temp.path().join(".omegon/codescan.db")).unwrap();
        let indexed = execute_operation(
            temp.path(),
            &mut cache,
            CodescanOperationV1::Index(IndexRequestV1 { invalidate: false }),
            || false,
        );
        assert!(matches!(indexed, CodescanOutcomeV1::Ok { .. }));

        let searched = execute_operation(
            temp.path(),
            &mut cache,
            CodescanOperationV1::Search(SearchRequestV1 {
                query: "extension_boundary".into(),
                scope: SearchScope::Code,
                max_results: 5,
                tags: vec![],
                within: None,
            }),
            || false,
        );
        let CodescanOutcomeV1::Ok {
            response: CodescanResponseV1::Search(response),
            ..
        } = searched
        else {
            panic!("unexpected search outcome: {searched:?}");
        };
        assert!(
            response
                .results
                .iter()
                .any(|result| result.file == "sample.rs")
        );
    }

    #[test]
    fn pre_cancelled_operation_does_not_mutate_cache() {
        let temp = tempfile::tempdir().unwrap();
        let mut cache = ScanCache::open(&temp.path().join(".omegon/codescan.db")).unwrap();
        let outcome = execute_operation(
            temp.path(),
            &mut cache,
            CodescanOperationV1::Index(IndexRequestV1 { invalidate: true }),
            || true,
        );
        assert!(matches!(
            outcome,
            CodescanOutcomeV1::Error {
                error: omegon_codescan_contracts::CodescanErrorV1 {
                    code: CodescanErrorCodeV1::Cancelled,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn cancellation_requires_the_current_codescan_protocol() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let active = HashMap::from([(42, Arc::clone(&cancelled))]);

        cancel_request(
            &active,
            json!({"protocol_version": CODESCAN_PROTOCOL_VERSION + 1, "request_id": 42}),
        );
        assert!(!cancelled.load(Ordering::Acquire));

        cancel_request(
            &active,
            json!({"protocol_version": CODESCAN_PROTOCOL_VERSION, "request_id": 42}),
        );
        assert!(cancelled.load(Ordering::Acquire));
    }
}
