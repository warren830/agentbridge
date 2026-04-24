//! Streaming preview tests: lifecycle, freeze/unfreeze.

use agentbridge::core::streaming::StreamPreview;

// ---------------------------------------------------------------------------
// Streaming preview lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_streaming_preview_lifecycle() {
    let mut sp = StreamPreview::new();
    assert!(sp.is_idle(), "should start Idle");

    let should_create = sp.append_text("Hello ");
    assert!(should_create, "first text should trigger preview creation");
    assert!(!sp.is_idle(), "should no longer be Idle");
    sp.mark_sent();

    let should_update = sp.append_text("world");
    assert!(
        !should_update,
        "should be throttled (not enough time/chars)"
    );

    let final_text = sp.finish().to_owned();
    assert!(sp.is_finished());
    assert_eq!(final_text, "Hello world");
    assert_eq!(sp.final_text(), "Hello world");
}

// ---------------------------------------------------------------------------
// Stream preview freeze/unfreeze
// ---------------------------------------------------------------------------

#[test]
fn test_stream_preview_freeze_unfreeze() {
    let mut sp = StreamPreview::new();

    assert!(sp.append_text("Starting response..."));
    sp.mark_sent();
    assert!(sp.was_active());

    // Freeze (simulating a permission request arriving)
    sp.freeze();

    // While frozen, appends accumulate but never trigger an update
    sp.append_text(" [thinking about permission]");
    assert_eq!(
        sp.preview_text(),
        "Starting response... [thinking about permission]"
    );

    // Even with generous delta, frozen returns false
    let big_chunk = "x".repeat(200);
    assert!(
        !sp.append_text(&big_chunk),
        "should not trigger update while frozen"
    );

    // Unfreeze (permission resolved)
    sp.unfreeze();

    let text_len = sp.preview_text().len();
    assert!(
        text_len > 200,
        "all text should be accumulated even while frozen"
    );

    // Verify freeze is a no-op from Idle state
    let mut sp2 = StreamPreview::new();
    sp2.freeze();
    assert!(sp2.is_idle(), "freeze from Idle should be a no-op");

    // Verify freeze is a no-op from Finished state
    let mut sp3 = StreamPreview::new();
    sp3.append_text("done");
    sp3.finish();
    assert!(sp3.is_finished());
    sp3.freeze();
    assert!(sp3.is_finished(), "freeze from Finished should be a no-op");
}
