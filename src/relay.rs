//! Bot-to-bot relay: allows projects to send messages to each other.
//!
//! Architecture:
//! - The server spawns a Unix socket listener at ~/.agentbridge/relay.sock
//! - The CLI subcommand `agentbridge relay send --to <project> "msg"` connects
//!   to that socket and sends a JSON request, then waits for a response.
//! - The server routes the message to the target project's engine, which
//!   processes it and sends the reply back over the socket.

#![allow(dead_code)] // RelayEnvelope fields are consumed via pattern matching on the receiver side

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};

/// A relay request from one project to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayRequest {
    /// Source project name (who is sending)
    pub from_project: String,
    /// Target project name (who should receive)
    pub to_project: String,
    /// The message content
    pub message: String,
}

/// A relay response back to the sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayResponse {
    /// Whether the relay was successful
    pub ok: bool,
    /// The reply text from the target project (if successful)
    pub reply: String,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Message sent internally from the socket listener to the relay router.
pub struct RelayEnvelope {
    pub request: RelayRequest,
    pub respond: oneshot::Sender<RelayResponse>,
}

/// The relay server that listens on a Unix socket.
pub struct RelayServer {
    socket_path: PathBuf,
    tx: mpsc::Sender<RelayEnvelope>,
}

impl RelayServer {
    pub fn new(tx: mpsc::Sender<RelayEnvelope>) -> Self {
        let socket_path = socket_path();
        Self { socket_path, tx }
    }

    /// Start listening for relay connections in the background.
    pub async fn start(&self) -> Result<()> {
        // Remove stale socket file if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        let tx = self.tx.clone();

        tracing::info!(path = %self.socket_path.display(), "relay server listening");

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        tokio::spawn(handle_relay_connection(stream, tx));
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "relay: accept failed");
                    }
                }
            }
        });

        Ok(())
    }

    /// Clean up the socket file.
    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Handle a single relay connection: read request, route, respond.
async fn handle_relay_connection(
    stream: tokio::net::UnixStream,
    tx: mpsc::Sender<RelayEnvelope>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one JSON line
    match buf_reader.read_line(&mut line).await {
        Ok(0) => return, // EOF
        Ok(_) => {}
        Err(e) => {
            tracing::debug!(error = %e, "relay: read error");
            return;
        }
    }

    // Parse the request
    let request: RelayRequest = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = RelayResponse {
                ok: false,
                reply: String::new(),
                error: Some(format!("invalid request: {}", e)),
            };
            let _ = writer
                .write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes())
                .await;
            return;
        }
    };

    // Send to router and wait for response
    let (respond_tx, respond_rx) = oneshot::channel();
    let envelope = RelayEnvelope {
        request,
        respond: respond_tx,
    };

    if tx.send(envelope).await.is_err() {
        let resp = RelayResponse {
            ok: false,
            reply: String::new(),
            error: Some("relay server shutting down".to_string()),
        };
        let _ = writer
            .write_all(format!("{}\n", serde_json::to_string(&resp).unwrap()).as_bytes())
            .await;
        return;
    }

    // Wait for router response (with timeout)
    let response = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        respond_rx,
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(_)) => RelayResponse {
            ok: false,
            reply: String::new(),
            error: Some("relay handler dropped".to_string()),
        },
        Err(_) => RelayResponse {
            ok: false,
            reply: String::new(),
            error: Some("relay timeout (120s)".to_string()),
        },
    };

    // Send response back
    let _ = writer
        .write_all(format!("{}\n", serde_json::to_string(&response).unwrap()).as_bytes())
        .await;
}

/// Send a relay message from the CLI (client side).
/// Connects to the Unix socket, sends the request, waits for response.
pub async fn send_relay(to_project: &str, message: &str) -> Result<RelayResponse> {
    let path = socket_path();

    if !path.exists() {
        anyhow::bail!(
            "Relay socket not found at {}. Is agentbridge running?",
            path.display()
        );
    }

    // Determine source project from environment or current directory
    let from_project = std::env::var("AGENTBRIDGE_PROJECT")
        .unwrap_or_else(|_| "unknown".to_string());

    let request = RelayRequest {
        from_project,
        to_project: to_project.to_string(),
        message: message.to_string(),
    };

    let stream = tokio::net::UnixStream::connect(&path).await?;
    let (reader, mut writer) = stream.into_split();

    // Send request as JSON line
    let req_json = serde_json::to_string(&request)?;
    writer.write_all(format!("{}\n", req_json).as_bytes()).await?;

    // Read response
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();
    buf_reader.read_line(&mut line).await?;

    let response: RelayResponse = serde_json::from_str(&line)?;
    Ok(response)
}

/// Default socket path.
fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".agentbridge")
        .join("relay.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_request_serialization() {
        let req = RelayRequest {
            from_project: "project-a".to_string(),
            to_project: "project-b".to_string(),
            message: "hello from A".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: RelayRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_project, "project-a");
        assert_eq!(parsed.to_project, "project-b");
        assert_eq!(parsed.message, "hello from A");
    }

    #[test]
    fn relay_response_success() {
        let resp = RelayResponse {
            ok: true,
            reply: "got it".to_string(),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: RelayResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.reply, "got it");
        assert!(parsed.error.is_none());
    }

    #[test]
    fn relay_response_error() {
        let resp = RelayResponse {
            ok: false,
            reply: String::new(),
            error: Some("project not found".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: RelayResponse = serde_json::from_str(&json).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("project not found"));
    }

    #[test]
    fn socket_path_is_under_home() {
        let path = socket_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".agentbridge"));
        assert!(path_str.ends_with("relay.sock"));
    }
}
