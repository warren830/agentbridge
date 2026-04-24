use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

impl RpcRequest {
    pub fn new(id: u64, method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RpcEnvelope {
    pub id: Option<serde_json::Value>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}

impl RpcEnvelope {
    pub fn is_notification(&self) -> bool {
        self.method.is_some() && self.id_is_null_or_absent()
    }

    pub fn is_server_request(&self) -> bool {
        self.method.is_some() && !self.id_is_null_or_absent()
    }

    pub fn is_response(&self) -> bool {
        !self.id_is_null_or_absent() && self.method.is_none()
    }

    fn id_is_null_or_absent(&self) -> bool {
        matches!(&self.id, None | Some(serde_json::Value::Null))
    }

    pub fn id_key(&self) -> String {
        match &self.id {
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorOut>,
}

#[derive(Debug, Serialize)]
pub struct RpcErrorOut {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcErrorOut { code, message }),
        }
    }
}

// ---------------------------------------------------------------------------
// ACP method constants
// ---------------------------------------------------------------------------

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_SESSION_NEW: &str = "session/new";
pub const METHOD_SESSION_LOAD: &str = "session/load";
pub const METHOD_SESSION_PROMPT: &str = "session/prompt";
pub const METHOD_SESSION_UPDATE: &str = "session/update";
pub const METHOD_SESSION_REQUEST_PERMISSION: &str = "session/request_permission";
pub const METHOD_SESSION_CANCEL: &str = "session/cancel";

// ---------------------------------------------------------------------------
// ACP initialize
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: serde_json::Value,
    pub agent_capabilities: Option<AgentCapabilities>,
    pub auth_methods: Option<Vec<serde_json::Value>>,
    pub agent_info: Option<AgentInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub load_session: bool,
    pub prompt_capabilities: Option<PromptCapabilities>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    #[serde(default)]
    pub image: bool,
    #[serde(default)]
    pub audio: bool,
}

#[derive(Debug, Deserialize)]
pub struct AgentInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// ACP session/new, session/load
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewResult {
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// ACP session/update notification
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    pub session_id: Option<String>,
    pub update: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateHead {
    pub session_update: String,
}

#[derive(Debug, Deserialize)]
pub struct AgentMessageChunk {
    pub content: Option<AgentMessageContent>,
}

#[derive(Debug, Deserialize)]
pub struct AgentMessageContent {
    #[serde(rename = "type")]
    pub content_type: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    pub tool_call_id: Option<String>,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub raw_input: Option<serde_json::Value>,
    #[serde(default)]
    pub content: Vec<ToolCallContentBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallContentBlock {
    pub content: Option<ToolCallContentInner>,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallContentInner {
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlanUpdate {
    #[serde(default)]
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Deserialize)]
pub struct PlanEntry {
    pub content: Option<String>,
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// ACP session/request_permission (server → client request)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestParams {
    pub session_id: Option<String>,
    pub tool_call: Option<PermissionToolCall>,
    #[serde(default)]
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolCall {
    pub tool_call_id: Option<String>,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub raw_input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Permission response helpers
// ---------------------------------------------------------------------------

pub fn build_permission_result(option_id: &str) -> serde_json::Value {
    serde_json::json!({
        "outcome": {
            "outcome": "selected",
            "optionId": option_id
        }
    })
}

pub fn build_permission_cancelled() -> serde_json::Value {
    serde_json::json!({
        "outcome": {
            "outcome": "cancelled"
        }
    })
}

pub fn pick_permission_option_id(allow: bool, options: &[PermissionOption]) -> Option<&str> {
    if options.is_empty() {
        return None;
    }
    if allow {
        for o in options {
            if o.kind.to_lowercase().contains("allow") {
                return Some(&o.option_id);
            }
        }
        for o in options {
            if o.name.to_lowercase().contains("allow") {
                return Some(&o.option_id);
            }
        }
        return Some(&options[0].option_id);
    }
    for o in options {
        let k = o.kind.to_lowercase();
        if k.contains("reject") || k.contains("deny") {
            return Some(&o.option_id);
        }
    }
    for o in options {
        let n = o.name.to_lowercase();
        if n.contains("reject") || n.contains("deny") {
            return Some(&o.option_id);
        }
    }
    Some(&options[options.len() - 1].option_id)
}

// ---------------------------------------------------------------------------
// Tool input summarization
// ---------------------------------------------------------------------------

pub fn summarize_acp_tool_input(kind: &str, raw_input: &serde_json::Value) -> String {
    let map = match raw_input.as_object() {
        Some(m) if !m.is_empty() => m,
        _ => return String::new(),
    };

    let kind_lower = kind.to_lowercase();
    match kind_lower.as_str() {
        "bash" | "shell" | "terminal" | "execute" => {
            if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
                if let Some(desc) = map.get("description").and_then(|v| v.as_str()) {
                    if !desc.is_empty() {
                        return format!("# {}\n{}", desc, cmd);
                    }
                }
                return cmd.to_string();
            }
        }
        "read" | "write" | "edit" => {
            if let Some(fp) = map
                .get("file_path")
                .or_else(|| map.get("path"))
                .and_then(|v| v.as_str())
            {
                return fp.to_string();
            }
        }
        _ => {}
    }

    if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
        if let Some(desc) = map.get("description").and_then(|v| v.as_str()) {
            if !desc.is_empty() {
                return format!("# {}\n{}", desc, cmd);
            }
        }
        return cmd.to_string();
    }

    serde_json::to_string_pretty(raw_input).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_request_serializes() {
        let req = RpcRequest::new(1, "initialize", serde_json::json!({"key": "val"}));
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"id\":1"));
        assert!(s.contains("\"method\":\"initialize\""));
    }

    #[test]
    fn rpc_envelope_notification() {
        let env: RpcEnvelope = serde_json::from_str(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc"}}"#,
        )
        .unwrap();
        assert!(env.is_notification());
        assert!(!env.is_server_request());
        assert!(!env.is_response());
    }

    #[test]
    fn rpc_envelope_server_request() {
        let env: RpcEnvelope = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":42,"method":"session/request_permission","params":{}}"#,
        )
        .unwrap();
        assert!(env.is_server_request());
        assert!(!env.is_notification());
        assert_eq!(env.id_key(), "42");
    }

    #[test]
    fn rpc_envelope_response() {
        let env: RpcEnvelope = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"result":{"sessionId":"s1"}}"#,
        )
        .unwrap();
        assert!(env.is_response());
        assert!(!env.is_notification());
    }

    #[test]
    fn rpc_response_success_serializes() {
        let resp = RpcResponse::success(serde_json::json!(1), serde_json::json!({"ok": true}));
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn rpc_response_error_serializes() {
        let resp = RpcResponse::error(serde_json::json!(1), -32603, "boom".into());
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
    }

    #[test]
    fn initialize_result_deserializes() {
        let json = r#"{
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": {"image": true, "audio": false}
            },
            "authMethods": [],
            "agentInfo": {"name": "Kiro CLI Agent", "version": "2.0.1"}
        }"#;
        let r: InitializeResult = serde_json::from_str(json).unwrap();
        let caps = r.agent_capabilities.unwrap();
        assert!(caps.load_session);
        assert!(caps.prompt_capabilities.unwrap().image);
        assert_eq!(r.agent_info.unwrap().name.unwrap(), "Kiro CLI Agent");
    }

    #[test]
    fn session_new_result_deserializes() {
        let json = r#"{"sessionId":"81c7e194-de0c-485a-9e52-fa09921e0b3f","modes":{}}"#;
        let r: SessionNewResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.session_id, "81c7e194-de0c-485a-9e52-fa09921e0b3f");
    }

    #[test]
    fn permission_option_roundtrip() {
        let opt = PermissionOption {
            option_id: "opt1".into(),
            name: "Allow once".into(),
            kind: "allow_once".into(),
        };
        let s = serde_json::to_string(&opt).unwrap();
        let d: PermissionOption = serde_json::from_str(&s).unwrap();
        assert_eq!(d.option_id, "opt1");
        assert_eq!(d.kind, "allow_once");
    }

    #[test]
    fn pick_permission_allow() {
        let opts = vec![
            PermissionOption { option_id: "r1".into(), name: "Reject".into(), kind: "reject_once".into() },
            PermissionOption { option_id: "a1".into(), name: "Allow once".into(), kind: "allow_once".into() },
            PermissionOption { option_id: "a2".into(), name: "Allow always".into(), kind: "allow_always".into() },
        ];
        assert_eq!(pick_permission_option_id(true, &opts), Some("a1"));
    }

    #[test]
    fn pick_permission_deny() {
        let opts = vec![
            PermissionOption { option_id: "a1".into(), name: "Allow".into(), kind: "allow_once".into() },
            PermissionOption { option_id: "r1".into(), name: "Reject".into(), kind: "reject_once".into() },
        ];
        assert_eq!(pick_permission_option_id(false, &opts), Some("r1"));
    }

    #[test]
    fn pick_permission_allow_fallback_first() {
        let opts = vec![
            PermissionOption { option_id: "x".into(), name: "Do it".into(), kind: "custom".into() },
        ];
        assert_eq!(pick_permission_option_id(true, &opts), Some("x"));
    }

    #[test]
    fn pick_permission_deny_fallback_last() {
        let opts = vec![
            PermissionOption { option_id: "x".into(), name: "Custom".into(), kind: "custom".into() },
            PermissionOption { option_id: "y".into(), name: "Other".into(), kind: "other".into() },
        ];
        assert_eq!(pick_permission_option_id(false, &opts), Some("y"));
    }

    #[test]
    fn pick_permission_empty_returns_none() {
        assert_eq!(pick_permission_option_id(true, &[]), None);
    }

    #[test]
    fn build_permission_result_selected() {
        let r = build_permission_result("opt1");
        assert_eq!(r["outcome"]["outcome"], "selected");
        assert_eq!(r["outcome"]["optionId"], "opt1");
    }

    #[test]
    fn build_permission_cancelled_result() {
        let r = build_permission_cancelled();
        assert_eq!(r["outcome"]["outcome"], "cancelled");
    }

    #[test]
    fn summarize_bash_command() {
        let input = serde_json::json!({"command": "ls -la", "description": "List files"});
        assert_eq!(summarize_acp_tool_input("bash", &input), "# List files\nls -la");
    }

    #[test]
    fn summarize_read_file() {
        let input = serde_json::json!({"file_path": "/tmp/foo.rs"});
        assert_eq!(summarize_acp_tool_input("read", &input), "/tmp/foo.rs");
    }

    #[test]
    fn summarize_empty_input() {
        assert_eq!(summarize_acp_tool_input("bash", &serde_json::json!({})), "");
    }

    #[test]
    fn summarize_unknown_tool_with_command() {
        let input = serde_json::json!({"command": "cargo build"});
        assert_eq!(summarize_acp_tool_input("unknown", &input), "cargo build");
    }

    #[test]
    fn permission_request_params_deserialize() {
        let json = r#"{
            "sessionId": "sess1",
            "toolCall": {
                "toolCallId": "tc1",
                "title": "Bash",
                "kind": "bash",
                "rawInput": {"command": "ls"}
            },
            "options": [
                {"optionId": "a1", "name": "Allow", "kind": "allow_once"},
                {"optionId": "r1", "name": "Reject", "kind": "reject_once"}
            ]
        }"#;
        let p: PermissionRequestParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.options.len(), 2);
        assert_eq!(p.tool_call.as_ref().unwrap().title.as_deref(), Some("Bash"));
    }

    #[test]
    fn session_update_agent_message_chunk() {
        let json = r#"{
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Hello world"}
            }
        }"#;
        let p: SessionUpdateParams = serde_json::from_str(json).unwrap();
        let head: SessionUpdateHead = serde_json::from_value(p.update.clone()).unwrap();
        assert_eq!(head.session_update, "agent_message_chunk");
        let chunk: AgentMessageChunk = serde_json::from_value(p.update).unwrap();
        assert_eq!(chunk.content.unwrap().text.unwrap(), "Hello world");
    }

    #[test]
    fn session_update_tool_call() {
        let json = r#"{
            "sessionUpdate": "tool_call",
            "toolCallId": "tc1",
            "title": "Read file",
            "kind": "read",
            "status": "pending",
            "rawInput": {"file_path": "/tmp/x.rs"}
        }"#;
        let tc: ToolCallUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(tc.tool_call_id.as_deref(), Some("tc1"));
        assert_eq!(tc.kind.as_deref(), Some("read"));
    }

    #[test]
    fn plan_update_deserializes() {
        let json = r#"{
            "entries": [
                {"content": "Step 1", "status": "done"},
                {"content": "Step 2", "status": "pending"}
            ]
        }"#;
        let p: PlanUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].content.as_deref(), Some("Step 1"));
    }
}
