//! Platform tests: reply, preview flow, capabilities, typing indicator.

mod common;

use std::sync::atomic::Ordering;

use common::{MockPlatform, MockReplyCtx, PreviewUpdate};

use agentbridge::core::platform::{
    MessageUpdater, Platform, PlatformCapabilities, TypingIndicator,
};

// ---------------------------------------------------------------------------
// Platform reply
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mock_platform_reply() {
    let platform = MockPlatform::new("test-platform");
    assert_eq!(platform.name(), "test-platform");

    let ctx = MockReplyCtx {
        channel: "general".to_string(),
        user: "alice".to_string(),
    };

    platform.reply(&ctx, "Hello, Alice!").await.unwrap();
    platform.send(&ctx, "Second message").await.unwrap();

    let sent = platform.sent_messages();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].content, "Hello, Alice!");
    assert_eq!(sent[0].channel, "mock:general:alice");
    assert_eq!(sent[1].content, "Second message");
}

// ---------------------------------------------------------------------------
// Platform preview flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mock_platform_preview_flow() {
    let platform = MockPlatform::new("preview-test");
    let ctx = MockReplyCtx {
        channel: "ch1".to_string(),
        user: "bob".to_string(),
    };

    let handle = platform.send_preview(&ctx, "Thinking...").await.unwrap();
    platform
        .update_preview(handle.as_ref(), "Thinking... got it")
        .await
        .unwrap();
    platform.delete_preview(handle.as_ref()).await.unwrap();

    let updates = platform.preview_updates();
    assert_eq!(updates.len(), 3);

    match &updates[0] {
        PreviewUpdate::Created { id, text } => {
            assert_eq!(*id, 1);
            assert_eq!(text, "Thinking...");
        }
        other => panic!("expected Created, got {:?}", other),
    }

    match &updates[1] {
        PreviewUpdate::Updated { id, text } => {
            assert_eq!(*id, 1);
            assert_eq!(text, "Thinking... got it");
        }
        other => panic!("expected Updated, got {:?}", other),
    }

    match &updates[2] {
        PreviewUpdate::Deleted { id } => {
            assert_eq!(*id, 1);
        }
        other => panic!("expected Deleted, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Platform capabilities detection
// ---------------------------------------------------------------------------

#[test]
fn test_platform_capabilities_detection() {
    let platform = MockPlatform::new("full-featured");

    assert!(
        platform.as_message_updater().is_some(),
        "should support message updates"
    );
    assert!(
        platform.as_image_sender().is_some(),
        "should support image sending"
    );
    assert!(
        platform.as_inline_button_sender().is_some(),
        "should support inline buttons"
    );
    assert!(
        platform.as_typing_indicator().is_some(),
        "should support typing indicator"
    );
    assert!(
        platform.as_file_sender().is_none(),
        "should not support file sending (not implemented)"
    );
}

// ---------------------------------------------------------------------------
// Typing indicator lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_typing_indicator_lifecycle() {
    let platform = MockPlatform::new("typing-test");
    let ctx = MockReplyCtx {
        channel: "ch".to_string(),
        user: "u".to_string(),
    };

    assert!(
        !platform.typing_active.load(Ordering::SeqCst),
        "typing should be inactive initially"
    );

    let stop_fn = platform.start_typing(&ctx).await.unwrap();
    assert!(
        platform.typing_active.load(Ordering::SeqCst),
        "typing should be active after start"
    );

    stop_fn();
    assert!(
        !platform.typing_active.load(Ordering::SeqCst),
        "typing should be inactive after stop"
    );
}
