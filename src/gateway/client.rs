//! Gateway client: connects an agentbridge instance to a gateway server.
//!
//! Runs as a background task alongside the normal bridge. Sends registration
//! on connect, relays agent events, and handles commands from the gateway.


use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::protocol::*;

/// Start the gateway client as a background task.
///
/// Connects to the gateway, registers this instance, and relays events.
/// Auto-reconnects on disconnect.
pub fn start(
    gateway_url: String,
    gateway_token: String,
    instance_id: String,
    instance_name: String,
    projects: Vec<super::protocol::ProjectInfo>,
) -> (mpsc::Sender<InstanceMessage>, mpsc::Receiver<GatewayMessage>) {
    let (event_tx, mut event_rx) = mpsc::channel::<InstanceMessage>(256);
    let (cmd_tx, cmd_rx) = mpsc::channel::<GatewayMessage>(64);

    tokio::spawn(async move {
        loop {
            tracing::info!(url = %gateway_url, "gateway-client: connecting");

            match connect_and_run(
                &gateway_url,
                &gateway_token,
                &instance_id,
                &instance_name,
                &projects,
                &mut event_rx,
                &cmd_tx,
            )
            .await
            {
                Ok(_) => tracing::info!("gateway-client: connection closed cleanly"),
                Err(e) => tracing::warn!(error = %e, "gateway-client: connection error"),
            }

            // Reconnect after 5 seconds
            tracing::info!("gateway-client: reconnecting in 5s");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    (event_tx, cmd_rx)
}

async fn connect_and_run(
    url: &str,
    token: &str,
    instance_id: &str,
    instance_name: &str,
    projects: &[super::protocol::ProjectInfo],
    event_rx: &mut mpsc::Receiver<InstanceMessage>,
    cmd_tx: &mpsc::Sender<GatewayMessage>,
) -> Result<()> {
    // Build connection request with auth header
    use tokio_tungstenite::tungstenite::http::Request;
    let req = Request::builder()
        .uri(url)
        .header("x-gateway-token", token)
        .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", url.replace("ws://", "").replace("wss://", "").split('/').next().unwrap_or(""))
        .body(())?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    tracing::info!("gateway-client: connected");

    // Send registration
    let register = InstanceMessage::Register(RegisterMessage {
        instance_id: instance_id.to_string(),
        instance_name: instance_name.to_string(),
        projects: projects.to_vec(),
    });
    let json = serde_json::to_string(&register)?;
    ws_tx.send(WsMessage::Text(json)).await?;

    // Send gateway token as a custom header isn't easy with tungstenite,
    // so we send it in the registration message or as a separate auth message.
    // For now, the gateway validates via header which we can't set easily.
    // TODO: Add token to URL query param or first message.

    loop {
        tokio::select! {
            // Forward events from engine to gateway
            Some(event) = event_rx.recv() => {
                let json = serde_json::to_string(&event)?;
                ws_tx.send(WsMessage::Text(json)).await?;
            }
            // Receive commands from gateway
            Some(msg) = ws_rx.next() => {
                match msg? {
                    WsMessage::Text(text) => {
                        let text = text.to_string();
                        if let Ok(cmd) = serde_json::from_str::<GatewayMessage>(&text) {
                            match cmd {
                                GatewayMessage::Ping => {
                                    let pong = serde_json::to_string(&InstanceMessage::Pong)?;
                                    ws_tx.send(WsMessage::Text(pong)).await?;
                                }
                                other => {
                                    let _ = cmd_tx.send(other).await;
                                }
                            }
                        }
                    }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            else => break,
        }
    }

    Ok(())
}
