//! Hook receiver: a localhost HTTP endpoint that turns Claude Code hook events
//! into `AgentEvent`s fed into the matching bridged session's event channel.
//!
//! Claude Code is configured (via `agentbridge hook-install`) to POST its Stop
//! and PostToolUse hook payloads to `POST /hook-event` on this server. The
//! receiver resolves the payload's `cwd` to a bound session through the
//! `HookRouteRegistry`, maps the payload to an `AgentEvent`, and sends it into
//! the same channel the engine's `process_agent_events` is already draining.
//!
//! It listens on `127.0.0.1` only (single-user, single-host) and ALWAYS
//! responds `200` so the hook script never retries or blocks Claude Code.

use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;

use crate::core::event::AgentEvent;
use crate::hook_route::HookRouteRegistry;

/// A Claude Code hook payload.
///
/// Every field is optional: hook payloads differ by event type, and a
/// permissive shape means a malformed or partial body deserializes rather than
/// being rejected — the receiver then decides what (if anything) it maps to.
// tool_response/duration_ms are deserialized for completeness but not yet
// surfaced; allow them to sit unused without a warning.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    /// "Stop" | "PostToolUse" | other event names we ignore.
    pub hook_event_name: Option<String>,
    /// Claude Code's own agent session id (not agentbridge's session key).
    pub session_id: Option<String>,
    /// The tmux session the cc runs in, injected by the hook script. The
    /// reliable routing key for an attached session whose cwd differs from the
    /// configured work_dir; preferred over `cwd`.
    pub tmux_session: Option<String>,
    /// Working directory of the Claude Code instance; the fallback routing key.
    pub cwd: Option<String>,
    /// Stop: the assistant's final message for the turn.
    pub last_assistant_message: Option<String>,
    /// PostToolUse: the tool that just ran.
    pub tool_name: Option<String>,
    /// PostToolUse: the tool's input arguments.
    pub tool_input: Option<serde_json::Value>,
    /// PostToolUse: the tool's response.
    pub tool_response: Option<serde_json::Value>,
    /// Optional event duration, when the hook reports it.
    pub duration_ms: Option<u64>,
}

/// Map a hook payload to an `AgentEvent`, or `None` if it should be dropped.
///
/// - `Stop` with a non-empty `last_assistant_message` becomes a `Result` (a
///   turn boundary); an empty/whitespace message maps to `None` so an empty
///   turn is never relayed (BR-1).
/// - `PostToolUse` becomes a `ToolUse` progress event carrying a short hint of
///   what the tool acted on; the event loop coalesces these into one in-place
///   message. An empty tool name maps to `None`.
/// - Any other event type maps to `None`.
///
/// Tokens are reported as 0 because hook payloads carry no token counts; the
/// engine's context indicator is gated on `input_tokens > 0` and simply does
/// not render for hook-driven turns (an accepted capability loss, ADR-3 M-3).
fn map_hook(p: &HookPayload) -> Option<AgentEvent> {
    match p.hook_event_name.as_deref() {
        Some("Stop") => {
            let content = p.last_assistant_message.clone().unwrap_or_default();
            if content.trim().is_empty() {
                return None;
            }
            Some(AgentEvent::Result {
                content,
                session_id: p.session_id.clone().unwrap_or_default(),
                input_tokens: 0,
                output_tokens: 0,
            })
        }
        // PostToolUse → a ToolUse progress event. The downstream event loop
        // coalesces these into a single in-place-edited progress message (it
        // does not send one chat message per tool), so a tool-heavy turn does
        // not spam the channel. An empty tool name is dropped.
        Some("PostToolUse") => {
            let tool = p.tool_name.clone().unwrap_or_default();
            if tool.trim().is_empty() {
                return None;
            }
            // A short, human-readable hint of what the tool acted on. Kept tiny
            // here; the event loop applies the display truncation/formatting.
            let input = tool_input_hint(p);
            Some(AgentEvent::ToolUse {
                id: String::new(),
                tool,
                input,
            })
        }
        _ => None,
    }
}

/// A short, human-readable hint of what a tool acted on, for the progress line.
///
/// Pulls the single most informative field per tool (the bash command, the
/// edited file, the search pattern); falls back to empty when nothing useful is
/// present. The event loop applies length truncation, so this stays whole.
fn tool_input_hint(p: &HookPayload) -> String {
    let Some(obj) = p.tool_input.as_ref().and_then(|v| v.as_object()) else {
        return String::new();
    };
    // First matching key wins, in rough order of how telling it is.
    for key in ["command", "file_path", "path", "pattern", "query", "url"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Shared handler state.
struct HookReceiverState {
    registry: Arc<HookRouteRegistry>,
}

/// Start the hook receiver HTTP server in the background.
///
/// Binds `127.0.0.1:{port}` (localhost only — no auth/TLS by design) and spawns
/// the serve loop so the caller is not blocked.
pub async fn start(port: u16, registry: Arc<HookRouteRegistry>) -> Result<()> {
    let state = Arc::new(HookReceiverState { registry });

    let app = Router::new()
        .route("/hook-event", post(handle_hook_event))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(port, "hook receiver listening on localhost");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "hook receiver server error");
        }
    });

    Ok(())
}

/// Handle one inbound hook event. Returns `200` unconditionally (BR-7): the
/// hook script must never see an error and must never retry or block Claude
/// Code. A hook for a non-bridged `cwd`, an unmapped event, or an empty turn is
/// silently dropped.
async fn handle_hook_event(
    State(state): State<Arc<HookReceiverState>>,
    Json(payload): Json<HookPayload>,
) -> StatusCode {
    // Gate: only hooks matching a bound session are relayed (by tmux session
    // name first, then cwd). A miss is the common, expected case for unrelated
    // Claude Code instances on the host.
    let Some(tx) = state
        .registry
        .resolve(payload.tmux_session.as_deref(), payload.cwd.as_deref())
    else {
        tracing::warn!(
            tmux_session = payload.tmux_session.as_deref().unwrap_or(""),
            cwd = payload.cwd.as_deref().unwrap_or(""),
            event = payload.hook_event_name.as_deref().unwrap_or(""),
            "hook dropped: no bound session"
        );
        return StatusCode::OK;
    };

    let Some(event) = map_hook(&payload) else {
        // Unmapped event type or empty turn — nothing to relay.
        return StatusCode::OK;
    };

    // Feed the existing session channel. A send error means the consumer has
    // gone away (session torn down between resolve and send); drop silently.
    let ev_kind = payload.hook_event_name.as_deref().unwrap_or("");
    if let Err(e) = tx.send(event).await {
        tracing::warn!(error = %e, event = ev_kind, "hook event dropped: session channel closed (no consumer)");
    } else {
        tracing::info!(event = ev_kind, "hook event relayed into session channel");
    }

    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(event: &str) -> HookPayload {
        HookPayload {
            hook_event_name: Some(event.to_string()),
            session_id: Some("sess-123".to_string()),
            tmux_session: None,
            cwd: Some("/tmp/project".to_string()),
            last_assistant_message: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            duration_ms: None,
        }
    }

    #[test]
    fn map_stop_with_text_yields_result() {
        let mut p = payload("Stop");
        p.last_assistant_message = Some("All done.".to_string());
        match map_hook(&p) {
            Some(AgentEvent::Result {
                content,
                session_id,
                input_tokens,
                output_tokens,
            }) => {
                assert_eq!(content, "All done.");
                assert_eq!(session_id, "sess-123");
                assert_eq!(input_tokens, 0);
                assert_eq!(output_tokens, 0);
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn map_stop_empty_message_yields_none() {
        // BR-1: an empty (or whitespace-only) Stop must NOT produce a turn.
        let mut p = payload("Stop");
        p.last_assistant_message = Some("   \n  ".to_string());
        assert!(map_hook(&p).is_none());

        let mut p2 = payload("Stop");
        p2.last_assistant_message = None;
        assert!(map_hook(&p2).is_none());
    }

    #[test]
    fn map_post_tool_use_yields_tooluse_with_hint() {
        // PostToolUse now relays a ToolUse progress event carrying a one-field
        // hint (the bash command here), coalesced downstream into one message.
        let mut p = payload("PostToolUse");
        p.tool_name = Some("Bash".to_string());
        p.tool_input = Some(serde_json::json!({ "command": "ls -la" }));
        match map_hook(&p) {
            Some(AgentEvent::ToolUse { id, tool, input }) => {
                assert!(id.is_empty(), "hook tool-use carries no tool id");
                assert_eq!(tool, "Bash");
                assert_eq!(input, "ls -la");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn map_post_tool_use_empty_tool_name_yields_none() {
        let mut p = payload("PostToolUse");
        p.tool_name = Some("   ".to_string());
        p.tool_input = Some(serde_json::json!({ "command": "ls" }));
        assert!(map_hook(&p).is_none());
    }

    #[test]
    fn tool_input_hint_prefers_command_then_path() {
        let mut p = payload("PostToolUse");
        p.tool_input = Some(serde_json::json!({ "file_path": "/x/y.rs", "command": "cargo test" }));
        // "command" wins by key order even when file_path is also present.
        assert_eq!(tool_input_hint(&p), "cargo test");

        p.tool_input = Some(serde_json::json!({ "file_path": "/x/y.rs" }));
        assert_eq!(tool_input_hint(&p), "/x/y.rs");

        // No telling field → empty hint (the tool name alone is shown).
        p.tool_input = Some(serde_json::json!({ "limit": 50 }));
        assert_eq!(tool_input_hint(&p), "");
    }

    /// Live wire test of the full receiver + python script path. Requires
    /// `python3` on PATH. Run with:
    ///   cargo test --bin agentbridge hook_relay_end_to_end -- --ignored --nocapture
    /// Verifies: the real `start()` server + the real `agentbridge_hook.py`
    /// script deliver a Stop payload as an `AgentEvent::Result` into a channel
    /// bound by cwd, and that an unbound cwd is dropped (still HTTP 200).
    #[tokio::test]
    #[ignore]
    async fn hook_relay_end_to_end() {
        use crate::hook_route::HookRouteRegistry;

        // Pick an ephemeral port by binding :0 then dropping it (best-effort —
        // a race is possible but unlikely on a dev box).
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);

        let registry = Arc::new(HookRouteRegistry::new());
        let work_dir = std::env::temp_dir().canonicalize().unwrap();
        let work_dir_str = work_dir.to_string_lossy().to_string();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(8);
        registry.bind(&work_dir_str, None, tx);

        start(port, Arc::clone(&registry)).await.expect("start receiver");
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/agentbridge_hook.py");

        // Bound cwd → should deliver a Result.
        let payload = serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "live-sess",
            "cwd": work_dir_str,
            "last_assistant_message": "live reply 你好"
        });
        run_hook_script(script, port, &payload.to_string()).await;

        match tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await {
            Ok(Some(AgentEvent::Result { content, .. })) => {
                assert_eq!(content, "live reply 你好");
            }
            other => panic!("expected delivered Result, got {other:?}"),
        }

        // Bound cwd, PostToolUse → should deliver a ToolUse progress event
        // carrying the command hint (the new tool-progress relay path).
        let tool_payload = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "session_id": "live-sess",
            "cwd": work_dir_str,
            "tool_name": "Bash",
            "tool_input": { "command": "ls -la 你好" }
        });
        run_hook_script(script, port, &tool_payload.to_string()).await;

        match tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await {
            Ok(Some(AgentEvent::ToolUse { tool, input, .. })) => {
                assert_eq!(tool, "Bash");
                assert_eq!(input, "ls -la 你好");
            }
            other => panic!("expected delivered ToolUse, got {other:?}"),
        }

        // Unbound cwd → dropped, no event, but the script still exits 0.
        let miss = serde_json::json!({
            "hook_event_name": "Stop",
            "cwd": "/definitely/not/bound",
            "last_assistant_message": "should be dropped"
        });
        run_hook_script(script, port, &miss.to_string()).await;
        let got = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await;
        assert!(got.is_err(), "unbound cwd must not deliver an event");
    }

    async fn run_hook_script(script: &str, port: u16, body: &str) {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::new("python3")
            .arg(script)
            .arg(port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn python hook script");
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(body.as_bytes()).await.unwrap();
            drop(stdin);
        }
        let status = child.wait().await.unwrap();
        assert!(status.success(), "hook script must exit 0");
    }

    #[test]
    fn map_unknown_event_is_none() {
        assert!(map_hook(&payload("SessionStart")).is_none());
        assert!(map_hook(&payload("PreToolUse")).is_none());

        let p = HookPayload {
            hook_event_name: None,
            session_id: None,
            tmux_session: None,
            cwd: None,
            last_assistant_message: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            duration_ms: None,
        };
        assert!(map_hook(&p).is_none());
    }
}
