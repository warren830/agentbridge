//! Dedup tracker tests.

use agentbridge::dedup::DedupTracker;

#[test]
fn test_dedup_tracker() {
    let tracker = DedupTracker::new();

    // First occurrence of each ID should pass
    assert!(tracker.check("msg-1"), "first check of msg-1 should pass");
    assert!(tracker.check("msg-2"), "first check of msg-2 should pass");
    assert!(tracker.check("msg-3"), "first check of msg-3 should pass");

    // Duplicate of msg-1 should be rejected
    assert!(
        !tracker.check("msg-1"),
        "duplicate msg-1 should be rejected"
    );

    // msg-2 and msg-3 duplicates also rejected
    assert!(!tracker.check("msg-2"));
    assert!(!tracker.check("msg-3"));

    // A new ID still passes
    assert!(tracker.check("msg-4"), "new msg-4 should pass");
}
