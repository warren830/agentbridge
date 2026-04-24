//! Gateway server: accepts WebSocket connections from instances and frontends,
//! routes messages between them, and serves the REST API.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        DefaultBodyLimit, Path, Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;

use super::protocol::*;

/// Maximum request body size (10 MiB).
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Gateway state
// ---------------------------------------------------------------------------

/// A connected instance.
struct ConnectedInstance {
    instance_id: String,
    instance_name: String,
    projects: Vec<ProjectInfo>,
    /// Channel to send commands to this instance.
    tx: tokio::sync::mpsc::Sender<String>,
}

/// Shared gateway state.
pub struct GatewayState {
    /// Connected instances keyed by instance_id.
    instances: RwLock<HashMap<String, ConnectedInstance>>,
    /// Broadcast channel for frontend event push.
    frontend_tx: broadcast::Sender<String>,
    /// API token for authentication.
    api_token: String,
    /// Token required for instance registration.
    gateway_token: String,
    /// Message store for chat history.
    message_store: Option<super::db::MessageStore>,
}

impl GatewayState {
    pub fn new(api_token: String, gateway_token: String, db_path: Option<&std::path::Path>) -> Self {
        let (frontend_tx, _) = broadcast::channel(1024);
        let message_store = db_path.and_then(|p| {
            super::db::MessageStore::open(p)
                .map_err(|e| tracing::error!(error = %e, "gateway: failed to open message store"))
                .ok()
        });
        Self {
            instances: RwLock::new(HashMap::new()),
            frontend_tx,
            api_token,
            gateway_token,
            message_store,
        }
    }
}

// ---------------------------------------------------------------------------
// Start the gateway server
// ---------------------------------------------------------------------------

pub async fn start(port: u16, api_token: String, gateway_token: String, static_dir: Option<String>) -> Result<()> {
    // SQLite db next to static dir or in current dir
    let db_path = static_dir.as_ref()
        .map(|d| std::path::PathBuf::from(d).parent().unwrap_or(std::path::Path::new(".")).join("agentbridge.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("agentbridge.db"));
    let state = Arc::new(GatewayState::new(api_token, gateway_token, Some(&db_path)));

    let mut app = Router::new()
        // Instance WebSocket endpoint (reverse connection)
        .route("/gateway/ws", get(handle_instance_ws))
        // Frontend WebSocket endpoint
        .route("/api/ws", get(handle_frontend_ws))
        // REST API
        .route("/api/instances", get(list_instances))
        .route("/api/instances/{id}/send", post(send_message))
        .route("/api/instances/{id}/command", post(send_command))
        .route("/api/instances/{id}/permission", post(send_permission))
        .route("/api/instances/{id}/history", get(get_history))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Serve static files (Nuxt SPA build output) if configured.
    // ServeDir serves files, and falls back to index.html for SPA routing.
    if let Some(ref dir) = static_dir {
        use tower_http::services::ServeDir;
        let index_path = std::path::PathBuf::from(dir).join("index.html");
        let serve = ServeDir::new(dir)
            .not_found_service(tower_http::services::ServeFile::new(index_path));
        app = app.fallback_service(serve);
    }

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(port = port, "gateway: server listening");

    axum::serve(listener, app).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Instance WebSocket handler
// ---------------------------------------------------------------------------

async fn handle_instance_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Authenticate via query param or header
    let token = headers
        .get("x-gateway-token")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    ws.on_upgrade(move |socket| instance_connection(socket, state, token))
}

async fn instance_connection(mut socket: WebSocket, state: Arc<GatewayState>, token: Option<String>) {
    // Validate token. On failure, send a Close frame before returning so the
    // client knows the connection was rejected (not dropped mid-flight).
    if token.as_deref() != Some(&state.gateway_token) {
        tracing::warn!("gateway: instance rejected (bad token)");
        let close = Message::Close(Some(CloseFrame {
            code: 4001,
            reason: "invalid gateway token".into(),
        }));
        let _ = socket.send(close).await;
        return;
    }

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<String>(64);

    let mut instance_id: Option<String> = None;

    // Forward commands from gateway to instance
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = cmd_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Heartbeat ping every 30s
    let cmd_tx_ping = cmd_tx.clone();
    let ping_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let ping = serde_json::json!({"type": "ping"}).to_string();
            if cmd_tx_ping.send(ping).await.is_err() {
                break;
            }
        }
    });

    // Read messages from instance
    while let Some(Ok(msg)) = ws_rx.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let parsed: Result<InstanceMessage, _> = serde_json::from_str(&text);
        match parsed {
            Ok(InstanceMessage::Register(reg)) => {
                let id = reg.instance_id.clone();
                let name = reg.instance_name.clone();
                tracing::info!(instance = %id, name = %name, projects = reg.projects.len(), "gateway: instance registered");

                // Notify frontends
                let push = FrontendPush::InstanceOnline {
                    instance_id: id.clone(),
                    instance_name: name.clone(),
                    projects: reg.projects.clone(),
                };
                let _ = state.frontend_tx.send(serde_json::to_string(&push).unwrap_or_default());

                let mut instances = state.instances.write().await;
                instances.insert(
                    id.clone(),
                    ConnectedInstance {
                        instance_id: id.clone(),
                        instance_name: name,
                        projects: reg.projects,
                        tx: cmd_tx.clone(),
                    },
                );
                instance_id = Some(id);
            }
            Ok(InstanceMessage::Event { instance_id: id, event }) => {
                // Store in database
                if let Some(ref store) = state.message_store {
                    if let Err(e) = store.insert_event(&id, &event.session_key, &event.event) {
                        tracing::warn!(error = %e, "gateway: failed to store event");
                    }
                }

                // Forward to subscribed frontends
                let push = FrontendPush::Event {
                    instance_id: id,
                    session_key: event.session_key,
                    event: event.event,
                };
                let _ = state.frontend_tx.send(serde_json::to_string(&push).unwrap_or_default());
            }
            Ok(InstanceMessage::SessionUpdate { instance_id: id, projects }) => {
                // Update stored state
                let mut instances = state.instances.write().await;
                if let Some(inst) = instances.get_mut(&id) {
                    inst.projects = projects.clone();
                }
                drop(instances);

                let push = FrontendPush::SessionUpdate {
                    instance_id: id,
                    projects,
                };
                let _ = state.frontend_tx.send(serde_json::to_string(&push).unwrap_or_default());
            }
            Ok(InstanceMessage::Pong) => {}
            Err(e) => {
                tracing::debug!(error = %e, "gateway: invalid instance message");
            }
        }
    }

    // Cleanup on disconnect
    ping_handle.abort();
    write_handle.abort();

    if let Some(ref id) = instance_id {
        tracing::info!(instance = %id, "gateway: instance disconnected");
        state.instances.write().await.remove(id);

        let push = FrontendPush::InstanceOffline {
            instance_id: id.clone(),
        };
        let _ = state.frontend_tx.send(serde_json::to_string(&push).unwrap_or_default());
    }
}

// ---------------------------------------------------------------------------
// Frontend WebSocket handler
// ---------------------------------------------------------------------------

async fn handle_frontend_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| frontend_connection(socket, state))
}

async fn frontend_connection(socket: WebSocket, state: Arc<GatewayState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut authed = false;

    // Wait for auth message first
    if let Some(Ok(Message::Text(text))) = ws_rx.next().await {
        if let Ok(FrontendMessage::Auth { token }) = serde_json::from_str(&text) {
            if token == state.api_token {
                authed = true;
                let ok = serde_json::to_string(&FrontendPush::AuthOk).unwrap_or_default();
                let _ = ws_tx.send(Message::Text(ok.into())).await;
            }
        }
    }

    if !authed {
        // Send AuthFail, then close the socket so clients don't stay connected
        // on a dead channel waiting for events that will never arrive.
        let fail = serde_json::to_string(&FrontendPush::AuthFail {
            message: "invalid token".to_string(),
        })
        .unwrap_or_default();
        let _ = ws_tx.send(Message::Text(fail.into())).await;
        let close = Message::Close(Some(CloseFrame {
            code: 4001,
            reason: "invalid api token".into(),
        }));
        let _ = ws_tx.send(close).await;
        return;
    }

    // Subscribe to broadcast
    let mut broadcast_rx = state.frontend_tx.subscribe();

    // TODO: track per-frontend subscriptions for filtering.
    // For V1, forward ALL events to all authenticated frontends.

    let forward_handle = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read subscription messages (V1: ignore, forward everything)
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(_) => {} // V1: ignore subscribe/unsubscribe
            Message::Close(_) => break,
            _ => {}
        }
    }

    forward_handle.abort();
}

// ---------------------------------------------------------------------------
// REST API handlers
// ---------------------------------------------------------------------------

fn check_auth(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == expected)
        .unwrap_or(false)
}

async fn list_instances(
    headers: HeaderMap,
    State(state): State<Arc<GatewayState>>,
) -> Result<Json<InstanceListResponse>, StatusCode> {
    if !check_auth(&headers, &state.api_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let instances = state.instances.read().await;
    let list: Vec<InstanceInfo> = instances
        .values()
        .map(|i| InstanceInfo {
            instance_id: i.instance_id.clone(),
            instance_name: i.instance_name.clone(),
            online: true,
            projects: i.projects.clone(),
        })
        .collect();

    Ok(Json(InstanceListResponse { instances: list }))
}

async fn send_message(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    if !check_auth(&headers, &state.api_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let instances = state.instances.read().await;
    let inst = instances.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    // Store user message and capture its row id for frontend dedup.
    let message_id = state.message_store.as_ref()
        .and_then(|store| store.insert_user_message(&id, &req.session_key, &req.text, "web").ok())
        .unwrap_or(0);

    // Broadcast to all frontends so other tabs/devices see the sent message.
    let push = FrontendPush::UserMessage {
        instance_id: id.clone(),
        session_key: req.session_key.clone(),
        text: req.text.clone(),
        message_id,
    };
    let _ = state.frontend_tx.send(serde_json::to_string(&push).unwrap_or_default());

    let msg = GatewayMessage::SendMessage {
        session_key: req.session_key.clone(),
        text: req.text.clone(),
        from: "web".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap_or_default();
    inst.tx.send(json).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApiResponse {
        ok: true,
        message: "sent".to_string(),
    }))
}

async fn send_command(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<CommandRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    if !check_auth(&headers, &state.api_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let instances = state.instances.read().await;
    let inst = instances.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    let msg = GatewayMessage::Command {
        session_key: req.session_key,
        command: req.command,
        args: req.args,
    };
    let json = serde_json::to_string(&msg).unwrap_or_default();
    inst.tx.send(json).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApiResponse {
        ok: true,
        message: "command sent".to_string(),
    }))
}

async fn send_permission(
    headers: HeaderMap,
    Path(id): Path<String>,
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<super::protocol::PermissionRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    if !check_auth(&headers, &state.api_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let instances = state.instances.read().await;
    let inst = instances.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    let msg = GatewayMessage::PermissionResponse {
        session_key: req.session_key,
        request_id: req.request_id,
        decision: req.decision,
    };
    let json = serde_json::to_string(&msg).unwrap_or_default();
    inst.tx.send(json).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApiResponse {
        ok: true,
        message: "permission response sent".to_string(),
    }))
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    session_key: String,
    limit: Option<usize>,
    before: Option<i64>,
}

async fn get_history(
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
    State(state): State<Arc<GatewayState>>,
) -> Result<Json<super::db::HistoryResponse>, StatusCode> {
    if !check_auth(&headers, &state.api_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let store = state.message_store.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = query.limit.unwrap_or(50).min(200);

    store
        .history(&id, &query.session_key, limit, query.before)
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "gateway: history query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
