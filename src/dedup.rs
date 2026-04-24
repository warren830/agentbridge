//! Message deduplication with 60-second TTL.
//!
//! Prevents duplicate processing when platforms deliver the same message twice.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_TTL_SECS: u64 = 60;

pub struct DedupTracker {
    ttl: Duration,
    seen: Mutex<HashMap<String, Instant>>,
}

impl Default for DedupTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupTracker {
    pub fn new() -> Self {
        Self {
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Returns true if this message ID has NOT been seen recently (i.e., is new).
    /// Returns false if it's a duplicate within the TTL window.
    pub fn check(&self, message_id: &str) -> bool {
        let now = Instant::now();
        let mut seen = self.seen.lock().unwrap();

        // Evict expired entries periodically (every 100 checks)
        if seen.len() > 100 {
            seen.retain(|_, ts| now.duration_since(*ts) < self.ttl);
        }

        // Check if we've seen this message
        if let Some(ts) = seen.get(message_id) {
            if now.duration_since(*ts) < self.ttl {
                return false; // Duplicate
            }
        }

        seen.insert(message_id.to_string(), now);
        true // New message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_message_passes() {
        let tracker = DedupTracker::new();
        assert!(tracker.check("msg-1"));
    }

    #[test]
    fn duplicate_rejected() {
        let tracker = DedupTracker::new();
        assert!(tracker.check("msg-1"));
        assert!(!tracker.check("msg-1")); // duplicate
    }

    #[test]
    fn different_ids_pass() {
        let tracker = DedupTracker::new();
        assert!(tracker.check("msg-1"));
        assert!(tracker.check("msg-2"));
        assert!(tracker.check("msg-3"));
    }

    #[test]
    fn expired_entry_passes_again() {
        let tracker = DedupTracker {
            ttl: Duration::from_millis(10),
            seen: Mutex::new(HashMap::new()),
        };
        assert!(tracker.check("msg-1"));
        std::thread::sleep(Duration::from_millis(20));
        assert!(tracker.check("msg-1")); // TTL expired, should pass
    }
}
