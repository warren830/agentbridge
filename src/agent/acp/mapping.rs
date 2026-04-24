use crate::core::event::AgentEvent;

use super::protocol::*;

pub fn map_session_update(session_id: &str, params: &serde_json::Value) -> Vec<AgentEvent> {
    // Single shallow-deserialize up front; then inspect the discriminator and
    // deserialize only the inner update into the concrete shape once.
    let sid = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(session_id);

    let update = match params.get("update") {
        Some(u) => u,
        None => return vec![],
    };

    let discriminator = update
        .get("sessionUpdate")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match discriminator {
        "agent_message_chunk" => map_agent_message_chunk(sid, update),
        "tool_call" => map_tool_call(update),
        "tool_call_update" => map_tool_call_update(update),
        "plan" => map_plan(update),
        "user_message_chunk" => vec![],
        other => map_fallback(other, update),
    }
}

fn map_agent_message_chunk(_session_id: &str, update: &serde_json::Value) -> Vec<AgentEvent> {
    let chunk: AgentMessageChunk = match serde_json::from_value(update.clone()) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let text = chunk
        .content
        .and_then(|c| c.text)
        .unwrap_or_default();
    if text.is_empty() {
        return vec![];
    }
    vec![AgentEvent::Text { content: text }]
}

fn map_tool_call(update: &serde_json::Value) -> Vec<AgentEvent> {
    let tc: ToolCallUpdate = match serde_json::from_value(update.clone()) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let tool_name = tc
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(tc.kind.as_deref())
        .unwrap_or("tool")
        .to_string();
    let raw_input = tc.raw_input.as_ref().unwrap_or(&serde_json::Value::Null);
    let kind = tc.kind.as_deref().unwrap_or("");
    let input = summarize_acp_tool_input(kind, raw_input);
    let input = if input.is_empty() {
        tc.title.unwrap_or_else(|| "tool".to_string())
    } else {
        input
    };
    vec![AgentEvent::ToolUse {
        id: tc.tool_call_id.unwrap_or_default(),
        tool: tool_name,
        input,
    }]
}

fn map_tool_call_update(update: &serde_json::Value) -> Vec<AgentEvent> {
    let tc: ToolCallUpdate = match serde_json::from_value(update.clone()) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let _tool_label = tc
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(tc.tool_call_id.as_deref())
        .unwrap_or("tool")
        .to_string();

    let body = extract_tool_content_text(&tc.content);
    let status = tc.status.as_deref().unwrap_or("").to_lowercase();

    match status.as_str() {
        "completed" | "failed" => {
            let output = if body.is_empty() && status == "completed" {
                return vec![];
            } else if body.is_empty() {
                "(failed)".to_string()
            } else {
                truncate_chars(&body, 800)
            };
            vec![AgentEvent::ToolResult {
                id: tc.tool_call_id.unwrap_or_default(),
                output,
                is_error: status == "failed",
            }]
        }
        "in_progress" | "pending" => {
            if body.is_empty() {
                return vec![];
            }
            vec![AgentEvent::ToolResult {
                id: tc.tool_call_id.unwrap_or_default(),
                output: truncate_chars(&body, 800),
                is_error: false,
            }]
        }
        _ => {
            if body.is_empty() {
                return vec![];
            }
            vec![AgentEvent::ToolResult {
                id: tc.tool_call_id.unwrap_or_default(),
                output: truncate_chars(&body, 800),
                is_error: false,
            }]
        }
    }
}

fn map_plan(_update: &serde_json::Value) -> Vec<AgentEvent> {
    let plan: PlanUpdate = match serde_json::from_value(_update.clone()) {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    if plan.entries.is_empty() {
        return vec![];
    }
    let mut out = String::new();
    for (i, entry) in plan.entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let content = entry.content.as_deref().unwrap_or("");
        if let Some(status) = entry.status.as_deref() {
            if !status.is_empty() {
                out.push('[');
                out.push_str(status);
                out.push_str("] ");
            }
        }
        out.push_str(content);
    }
    vec![AgentEvent::Thinking { content: out }]
}

fn map_fallback(kind: &str, update: &serde_json::Value) -> Vec<AgentEvent> {
    let kind_lower = kind.to_lowercase();
    match kind_lower.as_str() {
        "reasoning" | "reasoning_chunk" | "thinking" | "agent_thinking_chunk" => {
            let text = update
                .get("content")
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .or_else(|| update.get("text").and_then(|t| t.as_str()))
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                return vec![];
            }
            vec![AgentEvent::Thinking { content: text }]
        }
        _ => vec![],
    }
}

fn extract_tool_content_text(blocks: &[ToolCallContentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let Some(inner) = &block.content {
            if let Some(text) = &inner.text {
                if !text.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(text);
                }
            }
        }
    }
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let truncated: String = chars[..max].iter().collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_agent_message_chunk_text() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Hello"}
            }
        });
        let events = map_session_update("default_sid", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Text { content } => assert_eq!(content, "Hello"),
            _ => panic!("expected Text event"),
        }
    }

    #[test]
    fn map_agent_message_chunk_empty_text_ignored() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": ""}
            }
        });
        let events = map_session_update("s1", &params);
        assert!(events.is_empty());
    }

    #[test]
    fn map_tool_call_event() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tc1",
                "title": "Read file",
                "kind": "read",
                "status": "pending",
                "rawInput": {"file_path": "/tmp/x.rs"}
            }
        });
        let events = map_session_update("s1", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::ToolUse { id, tool, input } => {
                assert_eq!(id, "tc1");
                assert_eq!(tool, "Read file");
                assert_eq!(input, "/tmp/x.rs");
            }
            _ => panic!("expected ToolUse event"),
        }
    }

    #[test]
    fn map_tool_call_update_completed() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc1",
                "title": "Bash",
                "status": "completed",
                "content": [{"content": {"text": "output here"}}]
            }
        });
        let events = map_session_update("s1", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::ToolResult { id, output, is_error } => {
                assert_eq!(id, "tc1");
                assert_eq!(output, "output here");
                assert!(!is_error);
            }
            _ => panic!("expected ToolResult event"),
        }
    }

    #[test]
    fn map_tool_call_update_failed() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc1",
                "title": "Bash",
                "status": "failed",
                "content": []
            }
        });
        let events = map_session_update("s1", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::ToolResult { is_error, output, .. } => {
                assert!(is_error);
                assert_eq!(output, "(failed)");
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn map_tool_call_update_completed_empty_ignored() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc1",
                "status": "completed",
                "content": []
            }
        });
        let events = map_session_update("s1", &params);
        assert!(events.is_empty());
    }

    #[test]
    fn map_plan_entries() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "plan",
                "entries": [
                    {"content": "Read files", "status": "done"},
                    {"content": "Write code", "status": "pending"}
                ]
            }
        });
        let events = map_session_update("s1", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Thinking { content } => {
                assert!(content.contains("[done] Read files"));
                assert!(content.contains("[pending] Write code"));
            }
            _ => panic!("expected Thinking event"),
        }
    }

    #[test]
    fn map_user_message_chunk_suppressed() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "user_message_chunk",
                "content": {"type": "text", "text": "user said something"}
            }
        });
        let events = map_session_update("s1", &params);
        assert!(events.is_empty());
    }

    #[test]
    fn map_thinking_fallback() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "reasoning_chunk",
                "content": {"type": "text", "text": "thinking..."}
            }
        });
        let events = map_session_update("s1", &params);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Thinking { content } => assert_eq!(content, "thinking..."),
            _ => panic!("expected Thinking"),
        }
    }

    #[test]
    fn map_unknown_update_ignored() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "_kiro.dev/something",
                "data": 123
            }
        });
        let events = map_session_update("s1", &params);
        assert!(events.is_empty());
    }

    #[test]
    fn session_id_from_update_preferred() {
        let params = serde_json::json!({
            "sessionId": "from_update",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "hi"}
            }
        });
        let events = map_session_update("default", &params);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn truncate_long_text() {
        let long_text = "x".repeat(1000);
        let result = truncate_chars(&long_text, 800);
        assert_eq!(result.len(), 803); // 800 chars + "..."
        assert!(result.ends_with("..."));
    }
}
