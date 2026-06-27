//! Backend registry: maps `backend` strings from config to session factories.
//!
//! Each `AgentEntry` has a `backend` field ("claude" or "acp"). At runtime
//! the engine looks up the corresponding factory here to spawn an `AgentSession`.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{AgentConfig, AgentEntry};
use crate::hook_route::HookRouteRegistry;

use super::acp::AcpAgent;
use super::tmux::TmuxAgent;
use super::{AgentSession, ClaudeAgent};

/// Sanitize an arbitrary string into a tmux-safe token.
///
/// tmux session names may not contain `.` or `:` (`:` selects a window), so any
/// non-alphanumeric char is folded to `-` and runs are trimmed.
fn sanitize_tmux_token(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    t.trim_matches('-').to_string()
}

/// Build a per-channel tmux session name from the work_dir's folder name.
///
/// The folder name makes `tmux ls` self-describing (`agentbridge`, `project-x`)
/// instead of an opaque channel id. A short channel-derived suffix is appended
/// as a collision guard so two channels bound to same-named folders in
/// different paths (e.g. `~/a/api` and `~/b/api`) never share a pane.
fn derive_tmux_session_name(work_dir: &std::path::Path, session_key: &str) -> String {
    let folder = work_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_tmux_token)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "session".to_string());

    // Short, stable suffix from the channel id to disambiguate same-named
    // folders. Last 6 alphanumerics of the session key are enough.
    let key_token = sanitize_tmux_token(session_key);
    let suffix: String = key_token
        .chars()
        .rev()
        .filter(|c| c.is_alphanumeric())
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if suffix.is_empty() {
        folder
    } else {
        format!("{}-{}", folder, suffix)
    }
}

/// Spawn an agent session for the given entry.
///
/// `session_id`/`model`/`mode` are optional overrides used by Claude backend.
/// ACP backend ignores `model`/`mode` (those live in acp config).
#[allow(clippy::too_many_arguments)]
pub async fn start_session_for_entry(
    entry: &AgentEntry,
    work_dir: PathBuf,
    project_name: &str,
    session_id: Option<&str>,
    model_override: Option<&str>,
    mode_override: Option<&str>,
    work_dir_override: Option<&str>,
    session_key: &str,
    tmux_session_override: Option<&str>,
    hook_route: &Arc<HookRouteRegistry>,
) -> Result<Box<dyn AgentSession>> {
    match entry.backend.as_str() {
        "claude" => {
            let config: AgentConfig = entry.to_agent_config();
            let agent = ClaudeAgent::new(work_dir, config, project_name.to_string());
            let effective_model = model_override.or(entry.model.as_deref());
            let effective_mode = mode_override.unwrap_or(&entry.mode);
            let session = agent
                .start_session(session_id, effective_model, effective_mode, work_dir_override)
                .await?;
            Ok(Box::new(session))
        }
        "acp" => {
            let acp_config = entry
                .acp
                .clone()
                .ok_or_else(|| anyhow!("agent '{}' has backend=acp but no acp config", entry.name))?;
            let agent = AcpAgent::new(work_dir, acp_config);
            let session = agent
                .start_session(session_id, work_dir_override)
                .await?;
            Ok(Box::new(session))
        }
        "tmux" => {
            let mut tmux_config = entry
                .tmux
                .clone()
                .ok_or_else(|| anyhow!("agent '{}' has backend=tmux but no tmux config", entry.name))?;
            // Per-channel work_dir (set via /dir) overrides the project default,
            // so each channel's tmux session can run in its own directory.
            let effective_work_dir = work_dir_override
                .map(PathBuf::from)
                .unwrap_or(work_dir);

            // Session name resolution, highest priority first:
            //  1. /attach <name> override → this channel attaches that exact
            //     session. Lets different channels drive different hand-started
            //     cc sessions, even in attach mode.
            //  2. auto_start=true → agentbridge owns the sessions: name each
            //     after its work_dir folder (+ a channel suffix) so `tmux ls`
            //     is self-describing and channels never share a pane.
            //  3. auto_start=false (no override) → attach the configured name
            //     verbatim (single hand-started session, phone+laptop share it).
            if let Some(name) = tmux_session_override.filter(|s| !s.is_empty()) {
                tmux_config.session = name.to_string();
            } else if tmux_config.auto_start {
                tmux_config.session =
                    derive_tmux_session_name(&effective_work_dir, session_key);
            }
            // Hook relay mode routes Claude Code's Stop hook into this session's
            // event channel. Bind the session's event-sender (the "binder"
            // lifecycle point, ADR-1) under two keys so the receiver can resolve
            // an inbound hook: the tmux session name (reliable — an attached cc
            // often runs in a dir unrelated to work_dir) and the work_dir (cwd
            // fallback). Removed in `cleanup_agent_session` when torn down.
            let hook_relay = tmux_config.hook_relay;
            let bind_work_dir = effective_work_dir.display().to_string();
            let bind_tmux_session = tmux_config.session.clone();
            let agent = TmuxAgent::new(effective_work_dir, tmux_config, project_name.to_string());
            let session = agent.start_session().await?;
            if hook_relay {
                hook_route.bind(
                    &bind_work_dir,
                    Some(&bind_tmux_session),
                    session.hook_sender(),
                );
            }
            Ok(Box::new(session))
        }
        other => Err(anyhow!(
            "unknown agent backend: '{}' (valid: claude, acp, tmux)",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AcpConfig;

    #[test]
    fn derive_tmux_session_name_uses_folder_name() {
        // The work_dir folder name drives the session name, so `tmux ls` is
        // self-describing.
        let n = derive_tmux_session_name(
            std::path::Path::new("/Users/me/code/agentbridge"),
            "discord:1519686118134644937",
        );
        assert!(n.starts_with("agentbridge-"), "got: {n}");
        assert!(!n.contains(':') && !n.contains('.'));
    }

    #[test]
    fn derive_tmux_session_name_same_folder_diff_channel_disambiguates() {
        // Same folder name, different channels → different sessions (no clash).
        let a = derive_tmux_session_name(std::path::Path::new("/a/api"), "discord:111111");
        let b = derive_tmux_session_name(std::path::Path::new("/b/api"), "discord:222222");
        assert!(a.starts_with("api-") && b.starts_with("api-"));
        assert_ne!(a, b, "different channels must not share a tmux session");
    }

    #[test]
    fn derive_tmux_session_name_no_key_falls_back_to_folder() {
        assert_eq!(
            derive_tmux_session_name(std::path::Path::new("/x/myproj"), ""),
            "myproj"
        );
    }

    #[tokio::test]
    async fn unknown_backend_returns_error() {
        let entry = AgentEntry {
            name: "bad".to_string(),
            backend: "invented".to_string(),
            mode: "default".to_string(),
            model: None,
            allowed_tools: vec![],
            max_turns: None,
            acp: None,
            tmux: None,
        };
        let res = start_session_for_entry(
            &entry,
            PathBuf::from("/tmp"),
            "test",
            None,
            None,
            None,
            None,
            "discord:test",
            None,
            &Arc::new(HookRouteRegistry::new()),
        )
        .await;
        assert!(res.is_err());
        assert!(res.err().unwrap().to_string().contains("unknown agent backend"));
    }

    #[tokio::test]
    async fn acp_backend_without_config_errors() {
        let entry = AgentEntry {
            name: "kiro".to_string(),
            backend: "acp".to_string(),
            mode: "default".to_string(),
            model: None,
            allowed_tools: vec![],
            max_turns: None,
            acp: None,
            tmux: None,
        };
        let res = start_session_for_entry(
            &entry,
            PathBuf::from("/tmp"),
            "test",
            None,
            None,
            None,
            None,
            "discord:test",
            None,
            &Arc::new(HookRouteRegistry::new()),
        )
        .await;
        assert!(res.is_err());
        assert!(res.err().unwrap().to_string().contains("no acp config"));
    }

    #[tokio::test]
    async fn acp_backend_missing_command_errors() {
        let entry = AgentEntry {
            name: "kiro".to_string(),
            backend: "acp".to_string(),
            mode: "default".to_string(),
            model: None,
            allowed_tools: vec![],
            max_turns: None,
            acp: Some(AcpConfig {
                command: "nonexistent-xyzzy-abc".to_string(),
                args: vec![],
                env: vec![],
                auth_method: None,
                display_name: None,
            }),
            tmux: None,
        };
        let res = start_session_for_entry(
            &entry,
            PathBuf::from("/tmp"),
            "test",
            None,
            None,
            None,
            None,
            "discord:test",
            None,
            &Arc::new(HookRouteRegistry::new()),
        )
        .await;
        assert!(res.is_err());
    }
}
