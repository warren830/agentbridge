//! Shared message types for the gateway ↔ instance ↔ frontend protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Instance → Gateway messages
// ---------------------------------------------------------------------------

/// Message sent by an instance when it connects to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMessage {
    pub instance_id: String,
    pub instance_name: String,
    pub projects: Vec<ProjectInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub work_dir: String,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_key: String,
    pub session_id: String,
    pub name: Option<String>,
    pub agent_session_id: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub is_busy: bool,
}

/// An agent event relayed from instance to gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayedEvent {
    pub session_key: String,
    pub event: AgentEventPayload,
}

/// Simplified agent event for wire transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEventPayload {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "thinking")]
    Thinking { content: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, tool: String, input: String },
    #[serde(rename = "tool_result")]
    ToolResult { id: String, output: String, is_error: bool },
    #[serde(rename = "result")]
    Result {
        content: String,
        input_tokens: u32,
        output_tokens: u32,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "permission_request")]
    PermissionRequest {
        request_id: String,
        tool: String,
        input: String,
    },
}

// ---------------------------------------------------------------------------
// Instance → Gateway envelope
// ---------------------------------------------------------------------------

/// Top-level message from instance to gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InstanceMessage {
    #[serde(rename = "register")]
    Register(RegisterMessage),
    #[serde(rename = "event")]
    Event {
        instance_id: String,
        #[serde(flatten)]
        event: RelayedEvent,
    },
    #[serde(rename = "session_update")]
    SessionUpdate {
        instance_id: String,
        projects: Vec<ProjectInfo>,
    },
    #[serde(rename = "pong")]
    Pong,
}

// ---------------------------------------------------------------------------
// Gateway → Instance messages
// ---------------------------------------------------------------------------

/// Top-level message from gateway to instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayMessage {
    #[serde(rename = "send_message")]
    SendMessage {
        session_key: String,
        text: String,
        from: String,
    },
    #[serde(rename = "command")]
    Command {
        session_key: String,
        command: String,
        args: Option<String>,
    },
    #[serde(rename = "permission_response")]
    PermissionResponse {
        session_key: String,
        request_id: String,
        decision: String, // "allow" | "deny" | "allow_all"
    },
    #[serde(rename = "ping")]
    Ping,
}

// ---------------------------------------------------------------------------
// Frontend → Gateway messages (WebSocket)
// ---------------------------------------------------------------------------

/// Message from frontend to gateway over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FrontendMessage {
    #[serde(rename = "auth")]
    Auth { token: String },
    #[serde(rename = "subscribe")]
    Subscribe {
        instance_id: String,
        session_key: String,
    },
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        instance_id: String,
        session_key: String,
    },
}

// ---------------------------------------------------------------------------
// Gateway → Frontend messages (WebSocket)
// ---------------------------------------------------------------------------

/// Message from gateway to frontend over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FrontendPush {
    #[serde(rename = "auth_ok")]
    AuthOk,
    #[serde(rename = "auth_fail")]
    AuthFail { message: String },
    #[serde(rename = "event")]
    Event {
        instance_id: String,
        session_key: String,
        event: AgentEventPayload,
    },
    /// A user message sent from any frontend; broadcast to other tabs/devices.
    #[serde(rename = "user_message")]
    UserMessage {
        instance_id: String,
        session_key: String,
        text: String,
        /// Monotonic-ish id the frontend can use to dedup against its
        /// optimistic local copy.
        message_id: i64,
    },
    #[serde(rename = "instance_online")]
    InstanceOnline {
        instance_id: String,
        instance_name: String,
        projects: Vec<ProjectInfo>,
    },
    #[serde(rename = "instance_offline")]
    InstanceOffline { instance_id: String },
    #[serde(rename = "session_update")]
    SessionUpdate {
        instance_id: String,
        projects: Vec<ProjectInfo>,
    },
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

/// Response for GET /api/instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceListResponse {
    pub instances: Vec<InstanceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub instance_name: String,
    pub online: bool,
    pub projects: Vec<ProjectInfo>,
}

/// Request body for POST /api/instances/:id/send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub session_key: String,
    pub text: String,
}

/// Request body for POST /api/instances/:id/command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub session_key: String,
    pub command: String,
    pub args: Option<String>,
}

/// Request body for POST /api/instances/:id/permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub session_key: String,
    pub request_id: String,
    pub decision: String,
}

/// Generic API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub ok: bool,
    pub message: String,
}
