use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

use super::protocol::{RpcEnvelope, RpcError, RpcRequest, RpcResponse};

type NotifyHandler = Arc<dyn Fn(String, serde_json::Value) + Send + Sync>;
type ServerRequestHandler =
    Arc<dyn Fn(String, serde_json::Value, serde_json::Value) + Send + Sync>;
type RpcOutcome = Result<serde_json::Value, RpcError>;
type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<RpcOutcome>>>>;

pub struct Transport {
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicU64,
    pending: PendingMap,
    read_task: Mutex<Option<JoinHandle<()>>>,
}

impl Transport {
    pub fn new(
        stdin: ChildStdin,
        stdout: ChildStdout,
        on_notification: NotifyHandler,
        on_server_request: ServerRequestHandler,
    ) -> Arc<Self> {
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let t = Arc::new(Transport {
            stdin: Arc::new(Mutex::new(stdin)),
            next_id: AtomicU64::new(1),
            pending: Arc::clone(&pending),
            read_task: Mutex::new(None),
        });

        let read_pending = Arc::clone(&pending);
        let read_handle = tokio::spawn(async move {
            read_loop(stdout, read_pending, on_notification, on_server_request).await;
        });
        *t.read_task.try_lock().unwrap() = Some(read_handle);

        t
    }

    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let key = id.to_string();

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(key.clone(), tx);
        }

        let req = RpcRequest::new(id, method, params);
        let line = serde_json::to_vec(&req).context("serialize rpc request")?;

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(&line).await.context("write rpc request")?;
            stdin.write_all(b"\n").await.context("write newline")?;
            stdin.flush().await.context("flush rpc request")?;
        }

        let result = rx.await.map_err(|_| anyhow!("rpc response channel closed"))?;
        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(anyhow!("json-rpc {}: {}", e.code, e.message)),
        }
    }

    pub async fn respond_success(
        &self,
        id: serde_json::Value,
        result: serde_json::Value,
    ) -> Result<()> {
        let resp = RpcResponse::success(id, result);
        let line = serde_json::to_vec(&resp).context("serialize rpc response")?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&line).await.context("write rpc response")?;
        stdin.write_all(b"\n").await.context("write newline")?;
        stdin.flush().await.context("flush rpc response")?;
        Ok(())
    }

    pub async fn respond_error(
        &self,
        id: serde_json::Value,
        code: i32,
        message: String,
    ) -> Result<()> {
        let resp = RpcResponse::error(id, code, message);
        let line = serde_json::to_vec(&resp).context("serialize rpc error")?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&line).await.context("write rpc error")?;
        stdin.write_all(b"\n").await.context("write newline")?;
        stdin.flush().await.context("flush rpc error")?;
        Ok(())
    }

    pub async fn cancel_all_pending(&self) {
        let mut pending = self.pending.lock().await;
        let keys: Vec<String> = pending.keys().cloned().collect();
        for k in keys {
            if let Some(tx) = pending.remove(&k) {
                let _ = tx.send(Err(RpcError {
                    code: -32000,
                    message: "transport closed".to_string(),
                }));
            }
        }
    }
}

async fn read_loop(
    stdout: ChildStdout,
    pending: PendingMap,
    on_notification: NotifyHandler,
    on_server_request: ServerRequestHandler,
) {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut line_count: u64 = 0;
    loop {
        let next = lines.next_line().await;
        let line = match next {
            Ok(Some(line)) => line,
            Ok(None) => {
                tracing::warn!(line_count, "acp: read loop: stdout EOF");
                break;
            }
            Err(e) => {
                tracing::warn!(line_count, error = %e, "acp: read loop: read error");
                break;
            }
        };
        line_count += 1;
        if line.trim().is_empty() {
            continue;
        }
        // Trace first 200 chars of every raw line for debugging.
        tracing::debug!(
            line_preview = %line.chars().take(200).collect::<String>(),
            "acp: raw rpc line"
        );
        let env: RpcEnvelope = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(err) => {
                tracing::debug!(error = %err, line = %line, "acp: non-JSON-RPC line");
                continue;
            }
        };

        if env.is_notification() {
            let method = env.method.unwrap_or_default();
            let params = env.params.unwrap_or(serde_json::Value::Null);
            on_notification(method, params);
        } else if env.is_server_request() {
            let method = env.method.unwrap_or_default();
            let id = env.id.unwrap_or(serde_json::Value::Null);
            let params = env.params.unwrap_or(serde_json::Value::Null);
            tracing::info!(method = %method, "acp: server request received");
            on_server_request(method, id, params);
        } else if env.is_response() {
            let key = env.id_key();
            let mut p = pending.lock().await;
            if let Some(tx) = p.remove(&key) {
                drop(p);
                if let Some(err) = env.error {
                    let _ = tx.send(Err(err));
                } else {
                    let _ = tx.send(Ok(env.result.unwrap_or(serde_json::Value::Null)));
                }
            } else {
                tracing::debug!(id = %key, "acp: unmatched rpc response");
            }
        }
    }

    // EOF: cancel all pending requests so callers unblock
    let mut p = pending.lock().await;
    let keys: Vec<String> = p.keys().cloned().collect();
    for k in keys {
        if let Some(tx) = p.remove(&k) {
            let _ = tx.send(Err(RpcError {
                code: -32001,
                message: "acp: read loop ended".to_string(),
            }));
        }
    }
}
