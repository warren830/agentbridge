//! Per-key sliding window rate limiter.
//!
//! Tracks message timestamps per user key and rejects messages
//! that exceed the configured rate.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;

use crate::config::RateLimitConfig;

pub struct RateLimiter {
    max_messages: u32,
    window_secs: u64,
    /// Per-key queue of timestamps within the sliding window.
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            max_messages: config.max_messages,
            window_secs: config.window_secs,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Returns true if the message should be allowed, false if rate-limited.
    /// A max_messages of 0 disables the limiter (always allows).
    pub fn check(&self, key: &str) -> bool {
        if self.max_messages == 0 {
            return true;
        }

        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let mut buckets = self.buckets.lock().unwrap();
        let queue = buckets.entry(key.to_string()).or_default();

        // Evict entries outside the window
        while let Some(front) = queue.front() {
            if now.duration_since(*front) > window {
                queue.pop_front();
            } else {
                break;
            }
        }

        if queue.len() >= self.max_messages as usize {
            return false;
        }

        queue.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(max: u32, window: u64) -> RateLimitConfig {
        RateLimitConfig {
            max_messages: max,
            window_secs: window,
        }
    }

    #[test]
    fn disabled_limiter_always_allows() {
        let limiter = RateLimiter::new(&make_config(0, 60));
        for _ in 0..100 {
            assert!(limiter.check("user1"));
        }
    }

    #[test]
    fn allows_up_to_max() {
        let limiter = RateLimiter::new(&make_config(3, 60));
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(limiter.check("user1"));
        assert!(!limiter.check("user1"));
    }

    #[test]
    fn different_keys_independent() {
        let limiter = RateLimiter::new(&make_config(2, 60));
        assert!(limiter.check("a"));
        assert!(limiter.check("a"));
        assert!(!limiter.check("a"));
        // Different key should still be allowed
        assert!(limiter.check("b"));
        assert!(limiter.check("b"));
        assert!(!limiter.check("b"));
    }

    #[test]
    fn window_expiry_allows_again() {
        let limiter = RateLimiter::new(&make_config(1, 0)); // 0-second window
        assert!(limiter.check("user1"));
        // With a 0-second window, previous entries immediately expire
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(limiter.check("user1"));
    }
}
