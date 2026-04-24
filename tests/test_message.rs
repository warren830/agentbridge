//! Message routing, access control, and agent event tests.

mod common;

use common::MockReplyCtx;

use agentbridge::core::event::AgentEvent;
use agentbridge::core::message::IncomingMessage;

// ---------------------------------------------------------------------------
// Message routing & access control
// ---------------------------------------------------------------------------

#[test]
fn test_message_routing_access_control() {
    fn is_allowed(allow_from: &str, user_id: &str) -> bool {
        if allow_from == "*" {
            return true;
        }
        allow_from.split(',').any(|id| id.trim() == user_id)
    }

    // Wildcard allows everyone
    assert!(is_allowed("*", "anyone"));
    assert!(is_allowed("*", "12345"));

    // Exact match
    assert!(is_allowed("123,456", "123"));
    assert!(is_allowed("123,456", "456"));
    assert!(!is_allowed("123,456", "789"));

    // Whitespace trimming
    assert!(is_allowed("123, 456", "456"));
    assert!(is_allowed(" 123 , 456 ", "123"));

    // Build an IncomingMessage and verify its fields are accessible.
    let ctx = MockReplyCtx {
        channel: "ch".to_string(),
        user: "allowed-user".to_string(),
    };

    let msg = IncomingMessage {
        id: "msg-42".to_string(),
        from: "user-123".to_string(),
        from_name: Some("Alice".to_string()),
        text: "Hello agent".to_string(),
        images: vec![],
        files: vec![],
        voice: None,
        is_group: false,
        channel_id: Some("ch-1".to_string()),
        channel_name: None,
        reply_ctx: Box::new(ctx),
    };

    assert_eq!(msg.id, "msg-42");
    assert_eq!(msg.from, "user-123");
    assert_eq!(msg.text, "Hello agent");
    assert!(!msg.is_group);
    assert!(is_allowed("user-123,user-456", &msg.from));
    assert!(!is_allowed("user-999", &msg.from));

    // Verify ReplyCtx can be downcast
    let any_ctx = msg.reply_ctx.as_any();
    let mock = any_ctx.downcast_ref::<MockReplyCtx>().unwrap();
    assert_eq!(mock.user, "allowed-user");
}

// ---------------------------------------------------------------------------
// Agent event variants
// ---------------------------------------------------------------------------

#[test]
fn test_agent_event_variants() {
    let events: Vec<AgentEvent> = vec![
        AgentEvent::System {
            session_id: "sess-1".to_string(),
            tools: vec!["Read".to_string(), "Write".to_string()],
            skills: vec![],
        },
        AgentEvent::Text {
            content: "Hello".to_string(),
        },
        AgentEvent::Thinking {
            content: "Let me think...".to_string(),
        },
        AgentEvent::ToolUse {
            id: "tu-1".to_string(),
            tool: "Read".to_string(),
            input: "{}".to_string(),
        },
        AgentEvent::ToolResult {
            id: "tu-1".to_string(),
            output: "file contents".to_string(),
            is_error: false,
        },
        AgentEvent::PermissionRequest {
            request_id: "perm-1".to_string(),
            tool: "Bash".to_string(),
            input: serde_json::json!({"command": "rm -rf /"}),
            options: vec![],
        },
        AgentEvent::Result {
            content: "Done!".to_string(),
            session_id: "sess-1".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
        },
        AgentEvent::Error {
            message: "something broke".to_string(),
        },
    ];

    assert_eq!(events.len(), 8);

    match &events[0] {
        AgentEvent::System {
            session_id, tools, ..
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(tools.len(), 2);
        }
        _ => panic!("expected System event"),
    }

    match &events[6] {
        AgentEvent::Result {
            input_tokens,
            output_tokens,
            ..
        } => {
            assert_eq!(*input_tokens, 1000);
            assert_eq!(*output_tokens, 500);
        }
        _ => panic!("expected Result event"),
    }
}
