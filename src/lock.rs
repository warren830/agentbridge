//! Single-instance guard: prevents multiple agentbridge processes from running.
//!
//! Uses a PID file at ~/.agentbridge/agentbridge.pid.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Acquire the instance lock. Returns an error if another instance is running.
pub fn acquire() -> Result<LockGuard> {
    acquire_at(&lock_path())
}

/// Acquire the instance lock at a specific path (used by tests for isolation).
fn acquire_at(path: &Path) -> Result<LockGuard> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if existing PID file points to a live process
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if is_process_alive(pid) {
                    anyhow::bail!(
                        "Another agentbridge instance is running (PID {}). \
                         If this is stale, delete {}",
                        pid,
                        path.display()
                    );
                }
            }
        }
        // Stale lock file, remove it
        fs::remove_file(path).ok();
    }

    // Write our PID
    let pid = std::process::id();
    fs::write(path, pid.to_string())?;

    tracing::info!(pid = pid, "instance lock acquired");
    Ok(LockGuard { path: path.to_path_buf() })
}

pub struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        fs::remove_file(&self.path).ok();
    }
}

fn lock_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".agentbridge")
        .join("agentbridge.pid")
}

fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn acquire_and_release() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("agentbridge.pid");

        let guard = acquire_at(&path);
        assert!(guard.is_ok());
        let guard = guard.unwrap();

        assert!(path.exists());

        drop(guard);
        assert!(!path.exists());
    }

    #[test]
    fn stale_lock_is_cleaned_up() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("agentbridge.pid");

        // Plant a stale PID file
        fs::write(&path, "9999999").unwrap();

        let guard = acquire_at(&path);
        assert!(guard.is_ok());
        drop(guard.unwrap());
    }
}
