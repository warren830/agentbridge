//! Session tests: lock, queue, and session manager CRUD.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::MockReplyCtx;

use agentbridge::core::session::{InteractiveState, QueuedMessage, Session, SessionManager};

// ---------------------------------------------------------------------------
// Session try_lock
// ---------------------------------------------------------------------------

#[test]
fn test_session_try_lock() {
    let session = Session::new(None);
    assert!(!session.is_busy(), "new session should not be busy");

    let guard = session.try_lock();
    assert!(guard.is_some(), "first try_lock should succeed");
    assert!(session.is_busy(), "session should be busy after lock");

    let second = session.try_lock();
    assert!(second.is_none(), "second try_lock should fail while locked");

    drop(guard);
    assert!(!session.is_busy(), "session should be free after guard drop");

    let third = session.try_lock();
    assert!(third.is_some(), "try_lock should succeed after release");
}

// ---------------------------------------------------------------------------
// Session queue
// ---------------------------------------------------------------------------

#[test]
fn test_session_queue() {
    let session = Arc::new(Session::new(None));
    let mut state = InteractiveState::new(session);

    for i in 0..5 {
        let msg = QueuedMessage {
            text: format!("msg-{}", i),
            images: vec![],
            files: vec![],
            voice: None,
            from: "tester".to_string(),
            reply_ctx: Box::new(MockReplyCtx {
                channel: "test".into(),
                user: "tester".into(),
            }),
        };
        assert!(state.queue_message(msg), "message {} should be accepted", i);
    }
    assert_eq!(state.queue_len(), 5);

    let overflow = QueuedMessage {
        text: "overflow".to_string(),
        images: vec![],
        files: vec![],
        voice: None,
        from: "tester".to_string(),
        reply_ctx: Box::new(MockReplyCtx {
            channel: "test".into(),
            user: "tester".into(),
        }),
    };
    assert!(
        !state.queue_message(overflow),
        "6th message should be rejected"
    );

    let drained = state.drain_next();
    assert!(drained.is_some());
    assert_eq!(drained.unwrap().text, "msg-0");

    let late = QueuedMessage {
        text: "late".to_string(),
        images: vec![],
        files: vec![],
        voice: None,
        from: "tester".to_string(),
        reply_ctx: Box::new(MockReplyCtx {
            channel: "test".into(),
            user: "tester".into(),
        }),
    };
    assert!(
        state.queue_message(late),
        "should accept after draining one"
    );
}

// ---------------------------------------------------------------------------
// Session manager CRUD
// ---------------------------------------------------------------------------

#[test]
fn test_session_manager_crud() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mgr = SessionManager::new(tmp.path(), Path::new("/integration/test"));

    // Create
    let s1 = mgr.get_or_create("user:alice");
    assert!(!s1.id.is_empty());

    // Same key returns same session
    let s1_again = mgr.get_or_create("user:alice");
    assert_eq!(s1.id, s1_again.id);

    // Different key creates different session
    let s2 = mgr.get_or_create("user:bob");
    assert_ne!(s1.id, s2.id);

    // List
    let alice_list = mgr.list("user:alice");
    assert_eq!(alice_list.len(), 1);

    // Create a second session for alice
    let s3 = mgr.new_session("user:alice", Some("second".to_string()));
    assert_ne!(s3.id, s1.id);
    assert_eq!(s3.name.as_deref(), Some("second"));

    let alice_list = mgr.list("user:alice");
    assert_eq!(alice_list.len(), 2);

    // Switch back to s1
    let switched = mgr.switch_session("user:alice", &s1.id);
    assert!(switched.is_some());
    assert_eq!(switched.unwrap().id, s1.id);

    // Verify active is now s1
    let active = mgr.get_or_create("user:alice");
    assert_eq!(active.id, s1.id);

    // Delete s3
    mgr.delete_session("user:alice", &s3.id);
    let alice_list = mgr.list("user:alice");
    assert_eq!(alice_list.len(), 1);
    assert_eq!(alice_list[0].id, s1.id);

    // Cross-key switch should fail
    let cross = mgr.switch_session("user:alice", &s2.id);
    assert!(cross.is_none());
}
