//! Session sync: rsync Claude Code sessions between local and remote machine.
//!
//! Pull: remote → local (get sessions from your computer to the server)
//! Push: local → remote (send sessions from the server to your computer)

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::SyncConfig;

/// Sync direction.
pub enum Direction {
    /// Remote → local (before working on server)
    Pull,
    /// Local → remote (after working on server)
    Push,
}

/// Run rsync between local and remote Claude session directories.
pub fn run_sync(config: &SyncConfig, direction: Direction) -> Result<SyncResult> {
    let (src, dst, label) = match direction {
        Direction::Pull => (config.remote.as_str(), config.local.as_str(), "pull"),
        Direction::Push => (config.local.as_str(), config.remote.as_str(), "push"),
    };

    tracing::info!(src = %src, dst = %dst, "sync: {} starting", label);

    let output = Command::new("rsync")
        .arg("-az")           // archive + compress
        .arg("--update")      // skip files newer on destination (last-write-wins)
        .arg("--exclude")
        .arg("*.lock")
        .arg("--exclude")
        .arg("CLAUDE.md")
        .arg(src)
        .arg(dst)
        .output()
        .context("failed to run rsync (is it installed?)")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        tracing::info!("sync: {} completed", label);
        Ok(SyncResult {
            success: true,
            direction: label.to_string(),
            message: if stdout.is_empty() {
                "synced (no changes)".to_string()
            } else {
                format!("synced:\n{}", stdout.trim())
            },
        })
    } else {
        let err_msg = if stderr.is_empty() {
            format!("rsync exited with code {}", output.status)
        } else {
            stderr.trim().to_string()
        };
        tracing::error!(error = %err_msg, "sync: {} failed", label);
        Ok(SyncResult {
            success: false,
            direction: label.to_string(),
            message: err_msg,
        })
    }
}

pub struct SyncResult {
    pub success: bool,
    pub direction: String,
    pub message: String,
}

impl std::fmt::Display for SyncResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let icon = if self.success { "✅" } else { "❌" };
        write!(f, "{} sync {} — {}", icon, self.direction, self.message)
    }
}
