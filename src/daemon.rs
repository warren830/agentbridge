//! Daemon mode: manage agentbridge as a systemd user service.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "agentbridge";

/// Get the path to the systemd user service file.
fn service_file_path() -> Result<PathBuf> {
    let config_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
        .join(".config")
        .join("systemd")
        .join("user");
    Ok(config_dir.join(format!("{}.service", SERVICE_NAME)))
}

/// Get the path to the current binary.
fn current_binary_path() -> Result<String> {
    std::env::current_exe()
        .context("Cannot determine current binary path")?
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Binary path contains non-UTF8 characters"))
}

/// Generate the systemd unit file content.
fn generate_unit_file(binary_path: &str) -> String {
    format!(
        r#"[Unit]
Description=agentbridge - Bridge Claude Code to chat apps
After=network-online.target

[Service]
Type=simple
ExecStart={binary_path}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=agentbridge=info

[Install]
WantedBy=default.target
"#
    )
}

/// Install the systemd user service.
pub fn install_systemd_service() -> Result<()> {
    let binary_path = current_binary_path()?;
    let service_path = service_file_path()?;

    // Create the directory if needed
    if let Some(parent) = service_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let unit_content = generate_unit_file(&binary_path);
    fs::write(&service_path, &unit_content)
        .with_context(|| format!("Failed to write service file: {}", service_path.display()))?;

    println!("Service file written to: {}", service_path.display());
    println!("Binary path: {}", binary_path);

    // Reload systemd daemon
    let status = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("Failed to run systemctl daemon-reload")?;

    if !status.success() {
        anyhow::bail!("systemctl daemon-reload failed");
    }

    // Enable the service
    let status = Command::new("systemctl")
        .args(["--user", "enable", SERVICE_NAME])
        .status()
        .context("Failed to enable service")?;

    if status.success() {
        println!("Service installed and enabled.");
        println!("Run `agentbridge daemon start` to start it.");
    } else {
        println!("Service file written, but enable failed. You may need to enable it manually.");
    }

    Ok(())
}

/// Uninstall the systemd user service.
pub fn uninstall_systemd_service() -> Result<()> {
    // Stop and disable
    let _ = Command::new("systemctl")
        .args(["--user", "stop", SERVICE_NAME])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", SERVICE_NAME])
        .status();

    let service_path = service_file_path()?;
    if service_path.exists() {
        fs::remove_file(&service_path)
            .with_context(|| format!("Failed to remove {}", service_path.display()))?;
        println!("Service file removed: {}", service_path.display());
    } else {
        println!("Service file not found (already removed?).");
    }

    // Reload
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("Service uninstalled.");
    Ok(())
}

/// Start the daemon via systemctl.
pub fn daemon_start() -> Result<()> {
    let status = Command::new("systemctl")
        .args(["--user", "start", SERVICE_NAME])
        .status()
        .context("Failed to start service")?;

    if status.success() {
        println!("agentbridge daemon started.");
    } else {
        anyhow::bail!("Failed to start daemon. Run `agentbridge daemon status` for details.");
    }
    Ok(())
}

/// Stop the daemon via systemctl.
pub fn daemon_stop() -> Result<()> {
    let status = Command::new("systemctl")
        .args(["--user", "stop", SERVICE_NAME])
        .status()
        .context("Failed to stop service")?;

    if status.success() {
        println!("agentbridge daemon stopped.");
    } else {
        anyhow::bail!("Failed to stop daemon.");
    }
    Ok(())
}

/// Show daemon status.
pub fn daemon_status() -> Result<()> {
    let status = Command::new("systemctl")
        .args(["--user", "status", SERVICE_NAME])
        .status()
        .context("Failed to get service status")?;

    // systemctl status returns non-zero if the service is not running,
    // which is not an error from our perspective.
    if !status.success() {
        // Still OK, user can see the output
    }
    Ok(())
}

/// Show daemon logs.
pub fn daemon_logs(follow: bool) -> Result<()> {
    let mut args = vec![
        "--user",
        "--unit",
        SERVICE_NAME,
        "--no-pager",
        "-n",
        "100",
    ];
    if follow {
        args.push("-f");
    }

    let status = Command::new("journalctl")
        .args(&args)
        .status()
        .context("Failed to run journalctl")?;

    if !status.success() {
        // journalctl might fail if no logs yet, not a hard error
    }
    Ok(())
}
