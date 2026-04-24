//! Webhook HTTP endpoint for external triggers (git hooks, CI, etc.).
//!
//! Listens on a configurable port and accepts POST /hook with JSON body:
//! { "project": "my-project", "prompt": "run tests", "secret": "optional" }

#![allow(dead_code)] // WebhookEvent fields are deserialized by serde and routed to engine

use anyhow::Result;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::config::WebhookConfig;

/// Webhook request payload.
#[derive(Debug, Deserialize)]
pub struct WebhookRequest {
    /// Target project name
    pub project: String,
    /// Prompt to send to the agent
    pub prompt: String,
    /// Optional secret for authentication
    pub secret: Option<String>,
}

/// Webhook response.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub ok: bool,
    pub message: String,
}

/// A webhook event forwarded to the engine.
pub struct WebhookEvent {
    pub project: String,
    pub prompt: String,
}

/// Shared state for the webhook handler.
struct WebhookState {
    secret: Option<String>,
    tx: mpsc::Sender<WebhookEvent>,
}

/// Start the webhook HTTP server in the background.
pub async fn start(
    config: &WebhookConfig,
    tx: mpsc::Sender<WebhookEvent>,
) -> Result<()> {
    let state = Arc::new(WebhookState {
        secret: config.secret.clone(),
        tx,
    });

    let app = Router::new()
        .route("/hook", post(handle_webhook))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(port = config.port, "webhook server listening");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "webhook server error");
        }
    });

    Ok(())
}

async fn handle_webhook(
    State(state): State<Arc<WebhookState>>,
    Json(req): Json<WebhookRequest>,
) -> (StatusCode, Json<WebhookResponse>) {
    // Authenticate if secret is configured
    if let Some(ref expected) = state.secret {
        match &req.secret {
            Some(provided) if provided == expected => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(WebhookResponse {
                        ok: false,
                        message: "invalid or missing secret".to_string(),
                    }),
                );
            }
        }
    }

    if req.project.is_empty() || req.prompt.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(WebhookResponse {
                ok: false,
                message: "project and prompt are required".to_string(),
            }),
        );
    }

    let event = WebhookEvent {
        project: req.project.clone(),
        prompt: req.prompt.clone(),
    };

    match state.tx.send(event).await {
        Ok(_) => (
            StatusCode::OK,
            Json(WebhookResponse {
                ok: true,
                message: format!("prompt sent to project '{}'", req.project),
            }),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WebhookResponse {
                ok: false,
                message: "failed to queue webhook event".to_string(),
            }),
        ),
    }
}
