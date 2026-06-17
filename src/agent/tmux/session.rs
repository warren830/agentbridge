//! Tmux backend: control an existing tmux session running Claude Code interactively
//! via `tmux send-keys` / `tmux capture-pane`.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::config::TmuxConfig;
use crate::core::event::AgentEvent;
use crate::core::message::{FileAttachment, ImageAttachment};

use super::super::{AgentSession, PermissionResponder};

// ---------------------------------------------------------------------------
// TmuxSession
// ---------------------------------------------------------------------------

/// A live session backed by a tmux pane. Communication happens via
/// `tmux send-keys` (input) and `tmux capture-pane` (output polling).
pub struct TmuxSession {
    session_name: String,
    events_rx: mpsc::Receiver<AgentEvent>,
    event_tx: mpsc::Sender<AgentEvent>,
    alive: Arc<AtomicBool>,
    last_output: Arc<Mutex<Vec<String>>>,
    _poll_handle: JoinHandle<()>,
}

#[async_trait]
impl AgentSession for TmuxSession {
    async fn send(&self, prompt: &str) -> Result<()> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err(anyhow!("tmux: session '{}' is not alive", self.session_name));
        }
        tmux_send_keys(&self.session_name, prompt).await?;
        Ok(())
    }

    async fn send_with_attachments(
        &self,
        prompt: &str,
        _images: &[ImageAttachment],
        _files: &[FileAttachment],
    ) -> Result<()> {
        // Tmux cannot send binary attachments; just send the text prompt.
        self.send(prompt).await
    }

    async fn respond_permission(&self, _request_id: &str, allow: bool) -> Result<()> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err(anyhow!("tmux: session not alive"));
        }
        let key = if allow { "y" } else { "n" };
        tmux_send_raw_keys(&self.session_name, key).await?;
        Ok(())
    }

    fn permission_responder(&self) -> Arc<dyn PermissionResponder> {
        Arc::new(TmuxPermissionResponder {
            session_name: self.session_name.clone(),
            alive: Arc::clone(&self.alive),
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
        // Tmux sessions do not have agentbridge session IDs.
        None
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    async fn close(&self) -> Result<()> {
        // Send /exit to gracefully close Claude, then mark dead.
        let _ = tmux_send_keys(&self.session_name, "/exit").await;
        // Give it a moment, then send C-c as fallback.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.session_name, "C-c", ""])
            .output()
            .await;
        self.alive.store(false, Ordering::Release);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TmuxPermissionResponder
// ---------------------------------------------------------------------------

struct TmuxPermissionResponder {
    session_name: String,
    alive: Arc<AtomicBool>,
}

#[async_trait]
impl PermissionResponder for TmuxPermissionResponder {
    async fn respond(&self, _request_id: &str, allow: bool) -> Result<()> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err(anyhow!("tmux: session not alive"));
        }
        let key = if allow { "y" } else { "n" };
        tmux_send_raw_keys(&self.session_name, key).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TmuxAgent (factory)
// ---------------------------------------------------------------------------

/// Factory that creates or attaches to a tmux session running Claude Code.
pub struct TmuxAgent {
    work_dir: PathBuf,
    tmux_config: TmuxConfig,
    #[allow(dead_code)]
    project_name: String,
}

impl TmuxAgent {
    pub fn new(work_dir: PathBuf, tmux_config: TmuxConfig, project_name: String) -> Self {
        Self {
            work_dir,
            tmux_config,
            project_name,
        }
    }

    /// Start (or attach to) the tmux session and return a TmuxSession.
    pub async fn start_session(&self) -> Result<TmuxSession> {
        let session_name = &self.tmux_config.session;

        // Check if the tmux session already exists.
        let exists = tmux_has_session(session_name).await;

        if !exists && self.tmux_config.auto_start {
            tracing::info!(
                session = %session_name,
                work_dir = %self.work_dir.display(),
                "tmux: creating new session and starting claude"
            );
            // Create a new detached tmux session.
            let output = Command::new("tmux")
                .args([
                    "new-session",
                    "-d",
                    "-s",
                    session_name,
                    "-c",
                    &self.work_dir.display().to_string(),
                ])
                .output()
                .await?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!(
                    "tmux: failed to create session '{}': {}",
                    session_name,
                    stderr.trim()
                ));
            }
            // Start claude inside the session.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            tmux_send_keys(session_name, "claude").await?;
        } else if !exists {
            return Err(anyhow!(
                "tmux: session '{}' does not exist and auto_start is disabled",
                session_name
            ));
        } else {
            tracing::info!(
                session = %session_name,
                "tmux: attaching to existing session"
            );
        }

        let alive = Arc::new(AtomicBool::new(true));
        let last_output: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(128);

        // Spawn background polling task.
        let poll_session = session_name.clone();
        let poll_alive = Arc::clone(&alive);
        let poll_last_output = Arc::clone(&last_output);
        let poll_tx = event_tx.clone();
        let poll_handle = tokio::spawn(async move {
            poll_loop(poll_session, poll_alive, poll_last_output, poll_tx).await;
        });

        Ok(TmuxSession {
            session_name: session_name.clone(),
            events_rx: event_rx,
            event_tx,
            alive,
            last_output,
            _poll_handle: poll_handle,
        })
    }
}

// ---------------------------------------------------------------------------
// Background polling loop
// ---------------------------------------------------------------------------

/// Polls `tmux capture-pane` every 150ms, diffs against previous output,
/// and emits AgentEvent for new lines.
async fn poll_loop(
    session_name: String,
    alive: Arc<AtomicBool>,
    last_output: Arc<Mutex<Vec<String>>>,
    tx: mpsc::Sender<AgentEvent>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(150));
    // Track whether we have detected a "waiting for input" state after content.
    let mut had_content = false;

    loop {
        interval.tick().await;

        if !alive.load(Ordering::Relaxed) {
            break;
        }

        // Check session still exists.
        if !tmux_has_session(&session_name).await {
            alive.store(false, Ordering::Release);
            let _ = tx
                .send(AgentEvent::Error {
                    message: format!("tmux: session '{}' no longer exists", session_name),
                })
                .await;
            break;
        }

        // Capture the pane content (last 100 lines).
        let current_lines = match tmux_capture_pane(&session_name).await {
            Ok(lines) => lines,
            Err(e) => {
                tracing::debug!(error = %e, "tmux: capture-pane failed");
                continue;
            }
        };

        // Diff against last known output.
        let mut prev = last_output.lock().await;
        let new_lines = diff_lines(&prev, &current_lines);

        if !new_lines.is_empty() {
            let content = new_lines.join("\n");

            // Check for permission request patterns.
            if contains_permission_prompt(&content) {
                let request_id = format!("tmux-perm-{}", uuid::Uuid::new_v4());
                let _ = tx
                    .send(AgentEvent::PermissionRequest {
                        request_id,
                        tool: "tmux_permission".to_string(),
                        input: serde_json::json!({"prompt": content.clone()}),
                        options: vec![],
                    })
                    .await;
            } else {
                let _ = tx
                    .send(AgentEvent::Text {
                        content: content.clone(),
                    })
                    .await;
                had_content = true;
            }

            // Detect if Claude has finished (back to prompt / waiting state).
            if had_content && looks_like_prompt_ready(&new_lines) {
                let _ = tx
                    .send(AgentEvent::Result {
                        content: String::new(),
                        session_id: String::new(),
                        input_tokens: 0,
                        output_tokens: 0,
                    })
                    .await;
                had_content = false;
            }
        }

        *prev = current_lines;
    }
}

// ---------------------------------------------------------------------------
// Tmux helpers
// ---------------------------------------------------------------------------

/// Check if a tmux session exists.
async fn tmux_has_session(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Send text to a tmux session via send-keys (text is escaped, Enter is appended).
async fn tmux_send_keys(session_name: &str, text: &str) -> Result<()> {
    let escaped = escape_for_tmux(text);
    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, "--", &escaped, "Enter"])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("tmux send-keys failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Send raw keys (like "y", "n", "C-c") without Enter.
async fn tmux_send_raw_keys(session_name: &str, keys: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, keys, "Enter"])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("tmux send-keys failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Capture the last 100 lines from the tmux pane.
async fn tmux_capture_pane(session_name: &str) -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", session_name, "-p", "-S", "-100"])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("tmux capture-pane failed: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout.lines().map(|l| l.to_string()).collect();
    Ok(lines)
}

/// Escape text for safe use with `tmux send-keys`.
/// Tmux interprets semicolons, quotes, and backslashes specially.
fn escape_for_tmux(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            ';' => escaped.push_str("\\;"),
            '\'' => escaped.push_str("'\\''"),
            '"' => escaped.push_str("\\\""),
            '$' => escaped.push_str("\\$"),
            '`' => escaped.push_str("\\`"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// Output diffing and pattern detection
// ---------------------------------------------------------------------------

/// Find lines in `current` that are new compared to `previous`.
/// Uses a simple suffix-match: find the longest suffix of `previous` that
/// appears as a prefix of `current`, then the remainder is "new".
fn diff_lines(previous: &[String], current: &[String]) -> Vec<String> {
    if previous.is_empty() {
        // If we had no previous output, treat only non-empty trailing lines as new
        // to avoid flooding with the initial screen content.
        return Vec::new();
    }

    if current.is_empty() {
        return Vec::new();
    }

    // Find where the previous output ends in the current output.
    // Look for the last N lines of previous that match a subsequence in current.
    let prev_trimmed: Vec<&str> = previous.iter().map(|s| s.trim_end()).collect();
    let curr_trimmed: Vec<&str> = current.iter().map(|s| s.trim_end()).collect();

    // Try to find the last line of previous in current (from the end backward).
    if let Some(last_prev) = prev_trimmed.last() {
        if !last_prev.is_empty() {
            // Search for this line in current (from end).
            for i in (0..curr_trimmed.len()).rev() {
                if curr_trimmed[i] == *last_prev {
                    // Everything after index i is new.
                    let new_start = i + 1;
                    if new_start < current.len() {
                        let new_lines: Vec<String> = current[new_start..]
                            .iter()
                            .filter(|l| !l.trim().is_empty())
                            .cloned()
                            .collect();
                        return new_lines;
                    }
                    return Vec::new();
                }
            }
        }
    }

    // Fallback: if previous is completely different from current (e.g. screen cleared),
    // return the trailing non-empty lines of current as new content.
    let new_lines: Vec<String> = current
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    // Only return if there are fewer lines than the full capture (indicates partial new content).
    if new_lines.len() < current.len() / 2 {
        new_lines
    } else {
        // Too much output changed at once; skip to avoid flooding.
        Vec::new()
    }
}

/// Detect permission-related prompts in the terminal output.
fn contains_permission_prompt(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Claude Code asks things like "Allow tool?" or shows "allow" / "deny" buttons.
    (lower.contains("allow") && (lower.contains("deny") || lower.contains("tool")))
        || lower.contains("do you want to allow")
        || lower.contains("approve")
        || lower.contains("permission")
}

/// Detect if the last lines look like Claude is back at the input prompt.
/// Claude Code shows a prompt marker (e.g. ">", "$", or the agent waiting indicator).
fn looks_like_prompt_ready(lines: &[String]) -> bool {
    if let Some(last) = lines.last() {
        let trimmed = last.trim();
        // Claude Code typically shows ">" when ready for input, or ends with "$"/"%" for shell.
        trimmed == ">"
            || trimmed.ends_with('>')
            || trimmed.ends_with('$')
            || trimmed.ends_with('%')
            || trimmed.contains("Enter a prompt")
            || trimmed.contains("What can I help")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_for_tmux_handles_special_chars() {
        assert_eq!(escape_for_tmux("hello"), "hello");
        assert_eq!(escape_for_tmux("a;b"), "a\\;b");
        assert_eq!(escape_for_tmux("a\\b"), "a\\\\b");
        assert_eq!(escape_for_tmux("he said \"hi\""), "he said \\\"hi\\\"");
        assert_eq!(escape_for_tmux("$HOME"), "\\$HOME");
        assert_eq!(escape_for_tmux("`cmd`"), "\\`cmd\\`");
    }

    #[test]
    fn diff_lines_empty_previous_returns_empty() {
        let prev: Vec<String> = vec![];
        let curr = vec!["line1".to_string(), "line2".to_string()];
        // First capture returns empty (we skip the initial screen).
        assert!(diff_lines(&prev, &curr).is_empty());
    }

    #[test]
    fn diff_lines_finds_new_content() {
        let prev = vec![
            "old line 1".to_string(),
            "old line 2".to_string(),
        ];
        let curr = vec![
            "old line 1".to_string(),
            "old line 2".to_string(),
            "new line 1".to_string(),
            "new line 2".to_string(),
        ];
        let new = diff_lines(&prev, &curr);
        assert_eq!(new, vec!["new line 1", "new line 2"]);
    }

    #[test]
    fn diff_lines_no_change_returns_empty() {
        let lines = vec!["line1".to_string(), "line2".to_string()];
        assert!(diff_lines(&lines, &lines).is_empty());
    }

    #[test]
    fn contains_permission_prompt_detects_patterns() {
        assert!(contains_permission_prompt("Allow tool? (y/n)"));
        assert!(contains_permission_prompt("Do you want to allow this action?"));
        assert!(contains_permission_prompt("Allow deny"));
        assert!(!contains_permission_prompt("Hello world"));
        assert!(!contains_permission_prompt("allow me to explain"));
    }

    #[test]
    fn looks_like_prompt_ready_detects_markers() {
        assert!(looks_like_prompt_ready(&[">".to_string()]));
        assert!(looks_like_prompt_ready(&["claude >".to_string()]));
        assert!(looks_like_prompt_ready(&["user@host$".to_string()]));
        assert!(looks_like_prompt_ready(&["Enter a prompt".to_string()]));
        assert!(!looks_like_prompt_ready(&["some text".to_string()]));
        assert!(!looks_like_prompt_ready(&[]));
    }
}
