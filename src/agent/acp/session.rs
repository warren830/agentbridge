use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};

use crate::config::AcpConfig;
use crate::core::event::AgentEvent;
use crate::core::message::{FileAttachment, ImageAttachment};

use super::mapping::map_session_update;
use super::protocol::{
    build_permission_cancelled, build_permission_result, pick_permission_option_id,
    InitializeResult, PermissionOption, SessionNewResult,
    METHOD_INITIALIZE, METHOD_SESSION_LOAD, METHOD_SESSION_NEW, METHOD_SESSION_PROMPT,
    METHOD_SESSION_REQUEST_PERMISSION, METHOD_SESSION_UPDATE,
};
use super::transport::Transport;
use crate::agent::{AgentSession, PermissionResponder, CONTINUE_SESSION};

struct PendingPermission {
    rpc_id: serde_json::Value,
    options: Vec<PermissionOption>,
}

pub struct AcpSession {
    transport: Arc<Transport>,
    pub(crate) events_rx: mpsc::Receiver<AgentEvent>,
    event_tx: mpsc::Sender<AgentEvent>,
    acp_session_id: Arc<std::sync::Mutex<Option<String>>>,
    alive: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
    work_dir: PathBuf,
    pending_permissions: Arc<Mutex<HashMap<String, PendingPermission>>>,
    tool_input_cache: Arc<Mutex<HashMap<String, String>>>,
}

#[async_trait]
impl AgentSession for AcpSession {
    async fn send(&self, prompt: &str) -> Result<()> {
        self.send_with_attachments(prompt, &[], &[]).await
    }

    async fn send_with_attachments(
        &self,
        prompt: &str,
        images: &[ImageAttachment],
        files: &[FileAttachment],
    ) -> Result<()> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err(anyhow!("acp: session closed"));
        }
        let sid = self
            .acp_session_id
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_default();
        if sid.is_empty() {
            return Err(anyhow!("acp: no session id"));
        }

        // Save attachments to disk and reference them in the prompt text.
        let attach_dir = self.work_dir.join(".agentbridge").join("attachments");
        let _ = tokio::fs::create_dir_all(&attach_dir).await;

        let mut saved_files = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let name = if f.filename.is_empty() {
                format!("file_{}", i)
            } else {
                f.filename.clone()
            };
            let fpath = attach_dir.join(format!(
                "file_{}_{}",
                chrono::Utc::now().timestamp_millis(),
                name
            ));
            if tokio::fs::write(&fpath, &f.data).await.is_ok() {
                saved_files.push(fpath.display().to_string());
            }
        }

        let mut prompt_blocks = Vec::new();
        for (i, img) in images.iter().enumerate() {
            let ext = ext_from_mime(&img.mime_type);
            let fname = format!(
                "img_{}_{}{}",
                chrono::Utc::now().timestamp_millis(),
                i,
                ext
            );
            let fpath = attach_dir.join(&fname);
            if tokio::fs::write(&fpath, &img.data).await.is_err() {
                continue;
            }
            let mime = if img.mime_type.is_empty() {
                "image/png".to_string()
            } else {
                img.mime_type.clone()
            };
            let b64 = crate::agent::base64_encode(&img.data);
            prompt_blocks.push(serde_json::json!({
                "type": "image",
                "mimeType": mime,
                "data": b64,
            }));
        }

        let mut text = prompt.to_string();
        if !saved_files.is_empty() {
            text.push_str(&format!(
                "\n\n(Files saved locally, please read them: {})",
                saved_files.join(", ")
            ));
        }
        if text.is_empty() {
            text = "(attachment)".to_string();
        }
        prompt_blocks.push(serde_json::json!({ "type": "text", "text": text }));

        let params = serde_json::json!({
            "sessionId": sid,
            "prompt": prompt_blocks,
        });
        tracing::info!(
            session_id = %sid,
            prompt_blocks = prompt_blocks.len(),
            "acp: sending session/prompt"
        );

        // Fire session/prompt in a background task so the caller can start
        // consuming events (including permission requests) immediately.
        // Blocking here would deadlock: the agent may emit
        // session/request_permission mid-turn and wait for our response,
        // but we can only respond via the event loop that runs after send.
        // We therefore run the Send on a detached task and signal completion
        // via a oneshot channel the event loop can observe.
        let transport = Arc::clone(&self.transport);
        let event_tx = self.event_tx.clone();
        let sid_clone = sid.clone();
        tokio::spawn(async move {
            let result = transport.call(METHOD_SESSION_PROMPT, params).await;
            match result {
                Ok(ref v) => tracing::info!(response = %v, "acp: session/prompt response"),
                Err(ref e) => tracing::warn!(error = %e, "acp: session/prompt failed"),
            }
            let ev = match result {
                Ok(_) => AgentEvent::Result {
                    content: String::new(),
                    session_id: sid_clone,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                Err(e) => AgentEvent::Error { message: e.to_string() },
            };
            let _ = event_tx.send(ev).await;
        });
        Ok(())
    }

    async fn respond_permission(&self, request_id: &str, allow: bool) -> Result<()> {
        let pending = {
            let mut lock = self.pending_permissions.lock().await;
            lock.remove(request_id)
        };
        let pending = match pending {
            Some(p) => p,
            None => return Err(anyhow!("unknown permission request: {}", request_id)),
        };

        let option_id = pick_permission_option_id(allow, &pending.options);
        let result = match option_id {
            Some(id) => build_permission_result(id),
            None => {
                if allow {
                    return Err(anyhow!("agent did not provide permission options"));
                }
                build_permission_cancelled()
            }
        };
        self.transport.respond_success(pending.rpc_id, result).await
    }

    fn permission_responder(&self) -> Arc<dyn PermissionResponder> {
        Arc::new(AcpPermissionResponder {
            transport: Arc::clone(&self.transport),
            pending: Arc::clone(&self.pending_permissions),
        })
    }

    fn take_events(&mut self) -> Option<mpsc::Receiver<AgentEvent>> {
        let replacement = mpsc::channel(1).1;
        Some(std::mem::replace(&mut self.events_rx, replacement))
    }

    fn replace_events(&mut self, rx: mpsc::Receiver<AgentEvent>) {
        self.events_rx = rx;
    }

    fn events(&mut self) -> &mut mpsc::Receiver<AgentEvent> {
        &mut self.events_rx
    }

    fn drain_stale_events(&mut self) {
        while self.events_rx.try_recv().is_ok() {}
    }

    fn session_id(&self) -> Option<String> {
        self.acp_session_id.lock().ok()?.clone()
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    async fn close(&self) -> Result<()> {
        self.alive.store(false, Ordering::Release);
        self.transport.cancel_all_pending().await;

        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;
        Ok(())
    }
}


fn ext_from_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/png" | "" => ".png",
        _ => ".bin",
    }
}

/// Build an `AgentEvent::PermissionRequest` from an ACP permission request
/// payload. Returns `None` if the params fail to parse.
///
/// Exposed for testing the permission event construction without spinning
/// up a full subprocess.
pub(crate) fn build_permission_event(
    params: &serde_json::Value,
    request_id: String,
) -> Option<(AgentEvent, Vec<super::protocol::PermissionOption>)> {
    let parsed: super::protocol::PermissionRequestParams =
        serde_json::from_value(params.clone()).ok()?;
    let tool_name = parsed
        .tool_call
        .as_ref()
        .and_then(|tc| tc.title.clone().or(tc.kind.clone()))
        .unwrap_or_else(|| "permission".to_string());
    let tool_input = parsed
        .tool_call
        .as_ref()
        .and_then(|tc| tc.raw_input.clone())
        .unwrap_or(serde_json::Value::Null);
    let event_options: Vec<crate::core::event::PermissionOption> = parsed
        .options
        .iter()
        .map(|o| crate::core::event::PermissionOption {
            option_id: o.option_id.clone(),
            label: o.name.clone(),
            kind: o.kind.clone(),
        })
        .collect();
    Some((
        AgentEvent::PermissionRequest {
            request_id,
            tool: tool_name,
            input: tool_input,
            options: event_options,
        },
        parsed.options,
    ))
}

/// Permission responder for AcpSession — looks up the pending RPC id and
/// sends a JSON-RPC response. Clonable via Arc.
struct AcpPermissionResponder {
    transport: Arc<super::transport::Transport>,
    pending: Arc<Mutex<HashMap<String, PendingPermission>>>,
}

#[async_trait]
impl PermissionResponder for AcpPermissionResponder {
    async fn respond(&self, request_id: &str, allow: bool) -> Result<()> {
        let pending = {
            let mut lock = self.pending.lock().await;
            lock.remove(request_id)
        };
        let pending = match pending {
            Some(p) => p,
            None => return Err(anyhow!("unknown permission request: {}", request_id)),
        };
        let option_id = pick_permission_option_id(allow, &pending.options);
        let result = match option_id {
            Some(id) => build_permission_result(id),
            None => {
                if allow {
                    return Err(anyhow!("agent did not provide permission options"));
                }
                build_permission_cancelled()
            }
        };
        self.transport.respond_success(pending.rpc_id, result).await
    }
}

pub(crate) fn command_on_path(cmd: &str) -> bool {
    // Absolute or relative path: check directly.
    if cmd.contains('/') {
        return std::path::Path::new(cmd).exists();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        if dir.join(cmd).exists() {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// AcpAgent: factory/manager
// ---------------------------------------------------------------------------

pub struct AcpAgent {
    work_dir: PathBuf,
    acp_config: AcpConfig,
}

impl AcpAgent {
    pub fn new(work_dir: PathBuf, acp_config: AcpConfig) -> Self {
        Self {
            work_dir,
            acp_config,
        }
    }

    pub fn display_name(&self) -> String {
        self.acp_config
            .display_name
            .clone()
            .unwrap_or_else(|| self.acp_config.command.clone())
    }

    pub fn command(&self) -> &str {
        &self.acp_config.command
    }

    pub async fn start_session(
        &self,
        resume_session_id: Option<&str>,
        work_dir_override: Option<&str>,
    ) -> Result<AcpSession> {
        let effective_work_dir: PathBuf = work_dir_override
            .map(PathBuf::from)
            .unwrap_or_else(|| self.work_dir.clone());

        // Sanity check: command must be non-empty and exist in PATH.
        let command = self.acp_config.command.trim();
        if command.is_empty() {
            return Err(anyhow!("acp: command is empty"));
        }
        if !command_on_path(command) {
            return Err(anyhow!(
                "acp: command '{}' not found in PATH",
                command
            ));
        }

        tracing::info!(
            command = %command,
            args = ?self.acp_config.args,
            work_dir = %effective_work_dir.display(),
            "acp: spawning agent"
        );

        let mut cmd = Command::new(command);
        cmd.args(&self.acp_config.args)
            .current_dir(&effective_work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for env_pair in &self.acp_config.env {
            if let Some((k, v)) = env_pair.split_once('=') {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn().context("failed to spawn acp agent")?;
        let stdin = child.stdin.take().context("acp child has no stdin")?;
        let stdout = child.stdout.take().context("acp child has no stdout")?;
        let stderr = child.stderr.take();

        let alive = Arc::new(AtomicBool::new(true));
        let acp_session_id: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        let pending_permissions: Arc<Mutex<HashMap<String, PendingPermission>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let tool_input_cache: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(128);

        // stderr drain: log but do not fail
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(stderr = %line, "acp: subprocess stderr");
                }
            });
        }

        // Handlers for Transport's read loop.
        //
        // ORDER-CRITICAL: map events and send them synchronously via
        // blocking_send so the stream preserves the order kiro emits. The
        // read_loop calls us in order; we MUST NOT spawn tasks per chunk —
        // the tokio scheduler would reorder streamed text chunks.
        // blocking_send is safe here because the callback runs in the
        // read_loop's future (a tokio task), not a blocking thread, and the
        // channel has 128-slot buffer so backpressure is rare.
        let session_id_clone = Arc::clone(&acp_session_id);
        let event_tx_clone = event_tx.clone();
        let on_notification = Arc::new(move |method: String, params: serde_json::Value| {
            if method == METHOD_SESSION_UPDATE {
                let sid = session_id_clone
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_default();
                let disc = params
                    .get("update")
                    .and_then(|u| u.get("sessionUpdate"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                tracing::info!(session_update = %disc, "acp: session/update");
                let events = map_session_update(&sid, &params);
                for ev in events {
                    if event_tx_clone.try_send(ev).is_err() {
                        tracing::warn!("acp: event channel full or closed, dropping event");
                        break;
                    }
                }
            } else {
                tracing::debug!(method = %method, "acp: notification");
            }
        });

        // Server request handler. Permission requests are handled
        // synchronously (try_send) to maintain ordering with notifications.
        // Non-permission vendor requests (cursor/*, _kiro.dev/*) are spawned
        // since they only need to ack and don't affect event ordering.
        //
        // The transport handle is stored as a Weak to avoid an Arc cycle
        // (Transport -> callbacks -> Arc<Transport>). The session owns the
        // only strong reference; when it drops, the transport drops too.
        let event_tx_req = event_tx.clone();
        let _tool_cache_clone = Arc::clone(&tool_input_cache);
        let transport_holder: Arc<std::sync::Mutex<Option<std::sync::Weak<Transport>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let transport_holder_req = Arc::clone(&transport_holder);
        let pending_for_respond = Arc::clone(&pending_permissions);
        let on_server_request = Arc::new(
            move |method: String, id: serde_json::Value, params: serde_json::Value| {
                if method == METHOD_SESSION_REQUEST_PERMISSION {
                    let request_id = format!("acp-perm-{}", uuid::Uuid::new_v4());
                    let (event, raw_options) =
                        match build_permission_event(&params, request_id.clone()) {
                            Some(tuple) => tuple,
                            None => {
                                tracing::warn!("acp: permission request parse error");
                                let t = transport_holder_req
                                    .lock()
                                    .ok()
                                    .and_then(|g| g.as_ref().and_then(|w| w.upgrade()));
                                if let Some(t) = t {
                                    tokio::spawn(async move {
                                        let _ = t
                                            .respond_error(id, -32602, "invalid params".into())
                                            .await;
                                    });
                                }
                                return;
                            }
                        };

                    // Insert into pending map (needs async Mutex; spawn a tiny task).
                    {
                        let pending_for_respond = Arc::clone(&pending_for_respond);
                        let req_id = request_id.clone();
                        let rpc_id = id;
                        tokio::spawn(async move {
                            let mut lock = pending_for_respond.lock().await;
                            lock.insert(
                                req_id,
                                PendingPermission {
                                    rpc_id,
                                    options: raw_options,
                                },
                            );
                        });
                    }

                    // Send event synchronously via try_send (ordering with notifications).
                    match event_tx_req.try_send(event) {
                        Ok(()) => tracing::info!(request_id = %request_id, "acp: permission event queued to engine"),
                        Err(e) => tracing::error!(request_id = %request_id, error = %e, "acp: permission event DROPPED (channel closed/full)"),
                    }
                } else {
                    // Non-permission requests: spawn async ack (ordering doesn't matter).
                    let t = transport_holder_req
                        .lock()
                        .ok()
                        .and_then(|g| g.as_ref().and_then(|w| w.upgrade()));
                    let m = method.clone();
                    if let Some(t) = t {
                        tokio::spawn(async move {
                            if m.starts_with("cursor/") || m.starts_with("_kiro.dev/") {
                                let _ = t.respond_success(id, serde_json::json!({})).await;
                            } else {
                                tracing::info!(method = %m, "acp: unhandled server request");
                                let _ = t.respond_error(id, -32601, "method not implemented".into()).await;
                            }
                        });
                    }
                }
            },
        );

        let transport = Transport::new(stdin, stdout, on_notification, on_server_request);
        if let Ok(mut g) = transport_holder.lock() {
            *g = Some(Arc::downgrade(&transport));
        }

        // Handshake: initialize → (session/load if resume) → session/new fallback
        let init_result = transport
            .call(
                METHOD_INITIALIZE,
                serde_json::json!({
                    "protocolVersion": 1,
                    "clientCapabilities": {
                        "fs": {"readTextFile": false, "writeTextFile": false},
                        "terminal": false,
                    },
                    "clientInfo": {"name": "agentbridge", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await
            .context("acp: initialize")?;

        let init: InitializeResult = serde_json::from_value(init_result.clone())
            .context("acp: parse initialize result")?;
        let supports_load = init
            .agent_capabilities
            .as_ref()
            .map(|c| c.load_session)
            .unwrap_or(false);
        tracing::info!(
            agent = ?init.agent_info.as_ref().and_then(|a| a.name.as_deref()),
            load_session = supports_load,
            "acp: initialized"
        );

        let want_resume = resume_session_id
            .map(|s| !s.is_empty() && s != CONTINUE_SESSION)
            .unwrap_or(false);

        let mut got_session_id: Option<String> = None;
        if want_resume && supports_load {
            let sid = resume_session_id.unwrap();
            let load_params = serde_json::json!({
                "sessionId": sid,
                "cwd": effective_work_dir.display().to_string(),
                "mcpServers": [],
            });
            match transport.call(METHOD_SESSION_LOAD, load_params).await {
                Ok(result) => {
                    if let Ok(r) = serde_json::from_value::<SessionNewResult>(result) {
                        got_session_id = Some(r.session_id);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "acp: session/load failed, starting new");
                }
            }
        }

        if got_session_id.is_none() {
            let new_params = serde_json::json!({
                "cwd": effective_work_dir.display().to_string(),
                "mcpServers": [],
            });
            let result = transport
                .call(METHOD_SESSION_NEW, new_params)
                .await
                .context("acp: session/new")?;
            let r: SessionNewResult = serde_json::from_value(result)
                .context("acp: parse session/new result")?;
            if r.session_id.is_empty() {
                return Err(anyhow!("acp: session/new returned empty sessionId"));
            }
            got_session_id = Some(r.session_id);
        }

        let final_sid = got_session_id.unwrap();
        if let Ok(mut g) = acp_session_id.lock() {
            *g = Some(final_sid.clone());
        }

        // Emit synthetic System event so engine sees the new session id.
        let _ = event_tx
            .send(AgentEvent::System {
                session_id: final_sid,
                tools: vec![],
                skills: vec![],
            })
            .await;

        // Watch subprocess lifetime and flip `alive` on exit.
        let child_watch = Arc::new(Mutex::new(child));
        let child_for_exit = Arc::clone(&child_watch);
        let alive_for_exit = Arc::clone(&alive);
        let event_tx_exit = event_tx.clone();
        tokio::spawn(async move {
            let status = child_for_exit.lock().await.wait().await;
            tracing::warn!(status = ?status, "acp: subprocess wait returned");
            alive_for_exit.store(false, Ordering::Release);
            let _ = event_tx_exit
                .send(AgentEvent::Error {
                    message: "acp: subprocess exited".to_string(),
                })
                .await;
        });

        Ok(AcpSession {
            transport,
            events_rx: event_rx,
            event_tx,
            acp_session_id,
            alive,
            child: child_watch,
            work_dir: effective_work_dir,
            pending_permissions,
            tool_input_cache,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- Permission event construction (P3-11) ----------

    #[test]
    fn build_permission_event_maps_options_1_to_1() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "toolCall": {
                "toolCallId": "tc1",
                "title": "Bash",
                "kind": "bash",
                "rawInput": {"command": "ls -la"}
            },
            "options": [
                {"optionId": "a", "name": "Allow once", "kind": "allow_once"},
                {"optionId": "b", "name": "Allow always", "kind": "allow_always"},
                {"optionId": "c", "name": "Reject", "kind": "reject_once"},
            ]
        });
        let (event, raw_opts) =
            build_permission_event(&params, "req-1".to_string()).expect("parse");
        match event {
            AgentEvent::PermissionRequest { request_id, tool, options, .. } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(tool, "Bash");
                assert_eq!(options.len(), 3);
                assert_eq!(options[0].option_id, "a");
                assert_eq!(options[0].label, "Allow once");
                assert_eq!(options[0].kind, "allow_once");
                assert_eq!(options[2].kind, "reject_once");
            }
            other => panic!("unexpected event: {:?}", other),
        }
        assert_eq!(raw_opts.len(), 3);
        // raw_opts retain the on-wire shape for storing in pending_permissions
        assert_eq!(raw_opts[1].option_id, "b");
    }

    #[test]
    fn build_permission_event_no_tool_call_uses_fallback() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "options": []
        });
        let (event, opts) =
            build_permission_event(&params, "req-2".to_string()).expect("parse");
        match event {
            AgentEvent::PermissionRequest { tool, options, .. } => {
                assert_eq!(tool, "permission");
                assert!(options.is_empty());
            }
            _ => panic!("wrong variant"),
        }
        assert!(opts.is_empty());
    }

    #[test]
    fn build_permission_event_malformed_returns_none() {
        let params = serde_json::json!("not an object");
        assert!(build_permission_event(&params, "req-3".to_string()).is_none());
    }

    #[test]
    fn build_permission_event_kind_fallback_when_no_title() {
        let params = serde_json::json!({
            "toolCall": {
                "toolCallId": "tc1",
                "kind": "read",
                "rawInput": {"file_path": "/tmp/x"}
            },
            "options": [{"optionId": "a", "name": "OK", "kind": "allow_once"}]
        });
        let (event, _) =
            build_permission_event(&params, "req-4".to_string()).expect("parse");
        match event {
            AgentEvent::PermissionRequest { tool, .. } => assert_eq!(tool, "read"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_permission_event_preserves_raw_input() {
        let params = serde_json::json!({
            "toolCall": {
                "toolCallId": "tc1",
                "title": "Write",
                "kind": "write",
                "rawInput": {"file_path": "/etc/passwd", "content": "evil"}
            },
            "options": [{"optionId": "a", "name": "Allow", "kind": "allow_once"}]
        });
        let (event, _) =
            build_permission_event(&params, "req-5".to_string()).expect("parse");
        match event {
            AgentEvent::PermissionRequest { input, .. } => {
                assert_eq!(input["file_path"], "/etc/passwd");
                assert_eq!(input["content"], "evil");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ext_from_mime_mapping() {
        assert_eq!(ext_from_mime("image/png"), ".png");
        assert_eq!(ext_from_mime("image/jpeg"), ".jpg");
        assert_eq!(ext_from_mime("image/gif"), ".gif");
        assert_eq!(ext_from_mime("image/webp"), ".webp");
        assert_eq!(ext_from_mime(""), ".png");
        assert_eq!(ext_from_mime("application/octet-stream"), ".bin");
    }

    #[test]
    fn acp_agent_display_name_from_config() {
        let agent = AcpAgent::new(
            PathBuf::from("/tmp"),
            AcpConfig {
                command: "kiro-cli".to_string(),
                args: vec!["acp".to_string()],
                env: vec![],
                auth_method: None,
                display_name: Some("Kiro".to_string()),
            },
        );
        assert_eq!(agent.display_name(), "Kiro");
    }

    #[test]
    fn acp_agent_display_name_falls_back_to_command() {
        let agent = AcpAgent::new(
            PathBuf::from("/tmp"),
            AcpConfig {
                command: "kiro-cli".to_string(),
                args: vec![],
                env: vec![],
                auth_method: None,
                display_name: None,
            },
        );
        assert_eq!(agent.display_name(), "kiro-cli");
    }

    #[tokio::test]
    async fn start_session_fails_when_command_missing() {
        let agent = AcpAgent::new(
            PathBuf::from("/tmp"),
            AcpConfig {
                command: "nonexistent-cli-xyz-12345".to_string(),
                args: vec![],
                env: vec![],
                auth_method: None,
                display_name: None,
            },
        );
        let res = agent.start_session(None, None).await;
        assert!(res.is_err());
        let msg = res.err().unwrap().to_string();
        assert!(msg.contains("not found in PATH"));
    }

    #[test]
    fn command_on_path_finds_common_binaries() {
        // `sh` should always be on PATH in POSIX environments
        assert!(command_on_path("sh"));
    }

    #[test]
    fn command_on_path_rejects_missing() {
        assert!(!command_on_path("this-binary-should-not-exist-zz-ypv-11"));
    }

    #[test]
    fn command_on_path_handles_absolute_path() {
        assert!(command_on_path("/bin/sh"));
        assert!(!command_on_path("/nonexistent/path/to/binary-xyz"));
    }

    /// Live integration test: requires kiro-cli on PATH.
    /// Run with: `cargo test --bin agentbridge acp_live -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn acp_live_kiro_cli_handshake() {
        if !command_on_path("kiro-cli") {
            eprintln!("skipping: kiro-cli not on PATH");
            return;
        }
        let agent = AcpAgent::new(
            std::env::temp_dir(),
            AcpConfig {
                command: "kiro-cli".to_string(),
                args: vec!["acp".to_string()],
                env: vec![],
                auth_method: None,
                display_name: Some("Kiro".to_string()),
            },
        );
        let mut session = agent
            .start_session(None, None)
            .await
            .expect("session init");

        // We should get a System event with the session id.
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            session.events().recv(),
        )
        .await
        .expect("event timeout")
        .expect("event channel closed");
        match event {
            AgentEvent::System { session_id, .. } => {
                assert!(!session_id.is_empty(), "session_id should be set");
            }
            other => panic!("unexpected first event: {:?}", other),
        }

        session.close().await.expect("close");
    }

    /// Full end-to-end: send a prompt to kiro-cli via the `AgentSession` trait
    /// and verify we receive at least one Text event back. This exercises the
    /// same code path engine uses (trait methods only, no concrete type).
    ///
    /// Run with: `cargo test --bin agentbridge acp_live_kiro_cli_prompt -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn acp_live_kiro_cli_prompt() {
        if !command_on_path("kiro-cli") {
            eprintln!("skipping: kiro-cli not on PATH");
            return;
        }
        let agent = AcpAgent::new(
            std::env::temp_dir(),
            AcpConfig {
                command: "kiro-cli".to_string(),
                args: vec!["acp".to_string()],
                env: vec![],
                auth_method: None,
                display_name: Some("Kiro".to_string()),
            },
        );

        // Exercise via Box<dyn AgentSession> — same path engine takes.
        let mut session: Box<dyn AgentSession> = Box::new(
            agent
                .start_session(None, None)
                .await
                .expect("session init"),
        );

        // Drain System event.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            session.events().recv(),
        )
        .await
        .expect("system event timeout");

        // Send a real prompt.
        session
            .send("Reply with just the single word: pong")
            .await
            .expect("send prompt");

        // Collect events until we see text or Result.
        let mut saw_text = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let event = match tokio::time::timeout(remaining, session.events().recv()).await {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(_) => break,
            };
            match event {
                AgentEvent::Text { content } => {
                    eprintln!("[kiro] text: {}", content);
                    if !content.trim().is_empty() {
                        saw_text = true;
                    }
                }
                AgentEvent::Result { .. } => {
                    eprintln!("[kiro] result received");
                    break;
                }
                AgentEvent::Error { message } => {
                    panic!("kiro returned error: {}", message);
                }
                other => {
                    eprintln!("[kiro] other event: {:?}", other);
                }
            }
        }

        assert!(saw_text, "expected at least one Text event from kiro");
        session.close().await.expect("close");
    }
}
