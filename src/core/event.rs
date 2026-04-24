#![allow(dead_code)] // data-carrying enum; variants and fields are API surface

// ---------------------------------------------------------------------------
// AgentEvent -- events emitted by the Claude Code agent process.
// ---------------------------------------------------------------------------

/// A permission option offered by an ACP agent.
#[derive(Debug, Clone)]
pub struct PermissionOption {
    pub option_id: String,
    pub label: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Initial handshake after the agent session starts.
    System {
        session_id: String,
        tools: Vec<String>,
        skills: Vec<String>,
    },

    /// Streamed assistant text.
    Text {
        content: String,
    },

    /// Extended thinking / chain-of-thought block.
    Thinking {
        content: String,
    },

    /// The agent is invoking a tool.
    ToolUse {
        id: String,
        tool: String,
        input: String,
    },

    /// Result of a tool invocation.
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },

    /// Agent needs explicit permission before running a tool.
    ///
    /// `options` is non-empty for ACP agents (each option becomes a button).
    /// Empty for Claude (engine falls back to hardcoded Allow/Deny/AllowAll).
    PermissionRequest {
        request_id: String,
        tool: String,
        input: serde_json::Value,
        options: Vec<PermissionOption>,
    },

    /// Final result of the session.
    Result {
        content: String,
        session_id: String,
        input_tokens: u32,
        output_tokens: u32,
    },

    /// Unrecoverable error from the agent process.
    Error {
        message: String,
    },
}
