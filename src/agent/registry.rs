//! Backend registry: maps `backend` strings from config to session factories.
//!
//! Each `AgentEntry` has a `backend` field ("claude" or "acp"). At runtime
//! the engine looks up the corresponding factory here to spawn an `AgentSession`.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

use crate::config::{AgentConfig, AgentEntry};

use super::acp::AcpAgent;
use super::tmux::TmuxAgent;
use super::{AgentSession, ClaudeAgent};

/// Spawn an agent session for the given entry.
///
/// `session_id`/`model`/`mode` are optional overrides used by Claude backend.
/// ACP backend ignores `model`/`mode` (those live in acp config).
pub async fn start_session_for_entry(
    entry: &AgentEntry,
    work_dir: PathBuf,
    project_name: &str,
    session_id: Option<&str>,
    model_override: Option<&str>,
    mode_override: Option<&str>,
    work_dir_override: Option<&str>,
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
            let tmux_config = entry
                .tmux
                .clone()
                .ok_or_else(|| anyhow!("agent '{}' has backend=tmux but no tmux config", entry.name))?;
            let agent = TmuxAgent::new(work_dir, tmux_config, project_name.to_string());
            let session = agent.start_session().await?;
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
        )
        .await;
        assert!(res.is_err());
    }
}
