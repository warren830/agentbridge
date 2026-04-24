//! Outgoing rate limiter: prevents sending messages too fast to platforms.
//!
//! Telegram: 30 messages/sec global, 1 msg/sec per chat
//! Discord: 5 messages/sec per channel
//!
//! Uses a simple token bucket per platform.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct OutgoingRateLimiter {
    /// Per-platform minimum interval between sends.
    intervals: HashMap<String, Duration>,
    /// Last send time per (platform, channel) key.
    last_send: Mutex<HashMap<String, Instant>>,
}

impl OutgoingRateLimiter {
    pub fn new() -> Self {
        let mut intervals = HashMap::new();
        // Telegram: ~1 msg/sec per chat to be safe
        intervals.insert("telegram".to_string(), Duration::from_millis(1000));
        // Discord: ~200ms per channel (5/sec)
        intervals.insert("discord".to_string(), Duration::from_millis(200));

        Self {
            intervals,
            last_send: Mutex::new(HashMap::new()),
        }
    }

    /// Wait (if needed) before sending a message to respect rate limits.
    /// Returns immediately if no wait is needed.
    pub async fn wait(&self, platform: &str, channel_id: &str) {
        let min_interval = match self.intervals.get(platform) {
            Some(d) => *d,
            None => return, // No limit for unknown platforms
        };

        let key = format!("{}:{}", platform, channel_id);
        let wait_duration = {
            let mut last = self.last_send.lock().unwrap();
            if let Some(ts) = last.get(&key) {
                let elapsed = ts.elapsed();
                if elapsed < min_interval {
                    Some(min_interval - elapsed)
                } else {
                    last.insert(key.clone(), Instant::now());
                    None
                }
            } else {
                last.insert(key.clone(), Instant::now());
                None
            }
        };

        if let Some(d) = wait_duration {
            tokio::time::sleep(d).await;
            let mut last = self.last_send.lock().unwrap();
            last.insert(key, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_platform_no_wait() {
        let limiter = OutgoingRateLimiter::new();
        // Should not panic or block
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            limiter.wait("unknown", "ch1").await;
        });
    }

    #[test]
    fn first_send_no_wait() {
        let limiter = OutgoingRateLimiter::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let start = Instant::now();
            limiter.wait("discord", "ch1").await;
            assert!(start.elapsed() < Duration::from_millis(50));
        });
    }
}
