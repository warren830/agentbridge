//! Streaming preview state machine.
//!
//! Manages the lifecycle of a live "preview" message that gets updated
//! as the AI agent produces text. The actual platform calls (send, edit)
//! happen in the engine; this module is pure synchronous state management.

#![allow(dead_code)] // state-machine methods are API surface; some used by tests only

use std::time::Instant;

/// Minimum time between preview edits (milliseconds).
const MIN_INTERVAL_MS: u64 = 1500;
/// Minimum new characters before triggering an edit.
const MIN_DELTA_CHARS: usize = 30;
/// Maximum characters shown in preview (truncated from start when exceeded).
const MAX_PREVIEW_LEN: usize = 2000;

/// States of the streaming preview lifecycle.
///
/// ```text
/// Idle -> Active      (first text received, preview message created)
/// Active -> Frozen    (permission request or tool notification pauses updates)
/// Frozen -> Active    (permission resolved, resume updates)
/// Active -> Finished  (result received, final text applied)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewState {
    /// No preview message exists yet.
    Idle,
    /// Preview message is active and being updated.
    Active,
    /// Preview updates are paused (permission prompt, tool notification, etc.).
    Frozen,
    /// Preview is done (result received).
    Finished,
}

/// Manages the lifecycle of a streaming preview message.
///
/// Text is accumulated via [`append_text`](StreamPreview::append_text) and the
/// caller is told whether enough time/content has elapsed to warrant an edit.
/// Freezing pauses updates (e.g. while a permission prompt is shown) and
/// finishing locks the final text.
pub struct StreamPreview {
    /// Full accumulated text from all Text events.
    full_text: String,
    /// Length of text at last successful preview update.
    last_sent_len: usize,
    /// When the last preview update was sent.
    last_update_time: Instant,
    /// Current state.
    state: PreviewState,
}

impl Default for StreamPreview {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamPreview {
    /// Create a new preview in the [`Idle`](PreviewState::Idle) state.
    pub fn new() -> Self {
        Self {
            full_text: String::new(),
            last_sent_len: 0,
            last_update_time: Instant::now(),
            state: PreviewState::Idle,
        }
    }

    /// Append text from an agent text event.
    ///
    /// Returns `true` if a preview update should be sent to the platform:
    /// - Always `true` on the very first text (triggers the initial preview).
    /// - `true` when enough time *and* characters have accumulated.
    /// - Always `false` while the preview is [`Frozen`](PreviewState::Frozen).
    pub fn append_text(&mut self, text: &str) -> bool {
        self.full_text.push_str(text);

        if self.state == PreviewState::Frozen {
            return false;
        }

        if self.state == PreviewState::Idle {
            self.state = PreviewState::Active;
            return true; // First text — trigger initial preview
        }

        self.should_update()
    }

    /// Check if enough time and characters have passed for a throttled update.
    fn should_update(&self) -> bool {
        let elapsed = self.last_update_time.elapsed().as_millis() as u64;
        let delta = self.full_text.len() - self.last_sent_len;
        elapsed >= MIN_INTERVAL_MS && delta >= MIN_DELTA_CHARS
    }

    /// Get the current preview text (the full accumulated buffer).
    pub fn preview_text(&self) -> &str {
        &self.full_text
    }

    /// Get display text suitable for showing in a chat message.
    ///
    /// If the text exceeds [`MAX_PREVIEW_LEN`] characters it is truncated from
    /// the start so the most recent content is visible. A `▌` cursor is appended
    /// to indicate the stream is still in progress.
    ///
    /// Truncation counts CHARACTERS, not bytes, and is char-boundary-safe:
    /// agentbridge relays CJK chat text, and a raw byte slice
    /// (`&text[text.len()-N..]`) panics when the cut lands inside a multibyte
    /// codepoint — which it did, crashing the whole event loop on a long
    /// Chinese reply.
    pub fn display_text(&self) -> String {
        let text = &self.full_text;
        if text.chars().count() > MAX_PREVIEW_LEN {
            let tail: String = {
                // Keep the last MAX_PREVIEW_LEN chars (whole codepoints).
                let mut chars: Vec<char> = text.chars().collect();
                let start = chars.len() - MAX_PREVIEW_LEN;
                chars.drain(..start);
                chars.into_iter().collect()
            };
            format!("{}\u{258C}", tail)
        } else {
            format!("{}\u{258C}", text)
        }
    }

    /// Mark that a preview update was successfully sent to the platform.
    ///
    /// Resets the throttle counters so the next update waits for the full
    /// interval and delta thresholds again.
    pub fn mark_sent(&mut self) {
        self.last_sent_len = self.full_text.len();
        self.last_update_time = Instant::now();
    }

    /// Freeze the preview — stop sending updates.
    ///
    /// Used when a permission prompt or tool notification is shown so the
    /// preview message is not overwritten mid-prompt.
    pub fn freeze(&mut self) {
        if self.state == PreviewState::Active {
            self.state = PreviewState::Frozen;
        }
    }

    /// Unfreeze and resume updates.
    pub fn unfreeze(&mut self) {
        if self.state == PreviewState::Frozen {
            self.state = PreviewState::Active;
        }
    }

    /// Get the final complete text (for the last message).
    pub fn final_text(&self) -> &str {
        &self.full_text
    }

    /// Mark as finished and return the final text.
    pub fn finish(&mut self) -> &str {
        self.state = PreviewState::Finished;
        &self.full_text
    }

    /// Whether this preview was ever activated (had any text appended).
    pub fn was_active(&self) -> bool {
        self.state != PreviewState::Idle || !self.full_text.is_empty()
    }

    /// Whether currently in [`Idle`](PreviewState::Idle) state
    /// (no preview message has been created yet).
    pub fn is_idle(&self) -> bool {
        self.state == PreviewState::Idle
    }

    /// Whether the preview has finished.
    pub fn is_finished(&self) -> bool {
        self.state == PreviewState::Finished
    }

    /// Reset the preview to Idle state for a new text segment.
    ///
    /// Used after freeze+detach: the old preview becomes a permanent message
    /// and a fresh preview starts for the next text segment.
    pub fn reset(&mut self) {
        self.full_text.clear();
        self.last_sent_len = 0;
        self.last_update_time = Instant::now();
        self.state = PreviewState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn new_preview_starts_idle() {
        let sp = StreamPreview::new();
        assert!(sp.is_idle());
        assert!(!sp.was_active());
        assert!(!sp.is_finished());
        assert_eq!(sp.preview_text(), "");
    }

    #[test]
    fn first_append_returns_true() {
        let mut sp = StreamPreview::new();
        assert!(sp.append_text("hello"));
        assert!(!sp.is_idle());
        assert!(sp.was_active());
    }

    #[test]
    fn second_append_within_threshold_returns_false() {
        let mut sp = StreamPreview::new();
        assert!(sp.append_text("hello"));
        sp.mark_sent();
        // Immediately after — neither time nor delta threshold met
        assert!(!sp.append_text("x"));
    }

    #[test]
    fn append_after_threshold_returns_true() {
        let mut sp = StreamPreview::new();
        assert!(sp.append_text("init"));
        sp.mark_sent();

        // Simulate elapsed time by creating a preview with an old timestamp
        sp.last_update_time = Instant::now() - Duration::from_millis(MIN_INTERVAL_MS + 100);

        // Append enough chars to exceed delta threshold
        let chunk: String = "a".repeat(MIN_DELTA_CHARS + 1);
        assert!(sp.append_text(&chunk));
    }

    #[test]
    fn time_met_but_delta_not_met_returns_false() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();

        sp.last_update_time = Instant::now() - Duration::from_millis(MIN_INTERVAL_MS + 100);
        // Only a few chars — delta not met
        assert!(!sp.append_text("ab"));
    }

    #[test]
    fn delta_met_but_time_not_met_returns_false() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();

        // Enough chars but time just reset by mark_sent
        let chunk: String = "a".repeat(MIN_DELTA_CHARS + 1);
        assert!(!sp.append_text(&chunk));
    }

    #[test]
    fn freeze_stops_updates() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();
        sp.freeze();

        // Even with thresholds met, frozen preview returns false
        sp.last_update_time = Instant::now() - Duration::from_millis(MIN_INTERVAL_MS + 100);
        let chunk: String = "a".repeat(MIN_DELTA_CHARS + 1);
        assert!(!sp.append_text(&chunk));
    }

    #[test]
    fn unfreeze_resumes_updates() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();
        sp.freeze();
        sp.unfreeze();

        sp.last_update_time = Instant::now() - Duration::from_millis(MIN_INTERVAL_MS + 100);
        let chunk: String = "a".repeat(MIN_DELTA_CHARS + 1);
        assert!(sp.append_text(&chunk));
    }

    #[test]
    fn freeze_only_from_active() {
        let mut sp = StreamPreview::new();
        // Idle -> freeze should be a no-op
        sp.freeze();
        assert!(sp.is_idle());

        // Activate, finish, then freeze should be a no-op
        sp.append_text("x");
        sp.finish();
        assert!(sp.is_finished());
        sp.freeze();
        assert!(sp.is_finished()); // Still finished, not frozen
    }

    #[test]
    fn unfreeze_only_from_frozen() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        // Active -> unfreeze should be a no-op (stays Active)
        sp.unfreeze();
        assert!(sp.was_active());
        assert!(!sp.is_idle());
        assert!(!sp.is_finished());
    }

    #[test]
    fn display_text_adds_cursor() {
        let mut sp = StreamPreview::new();
        sp.append_text("hello world");
        assert_eq!(sp.display_text(), "hello world\u{258C}");
    }

    #[test]
    fn display_text_truncates_long_text() {
        let mut sp = StreamPreview::new();
        let long_text: String = "x".repeat(MAX_PREVIEW_LEN + 500);
        sp.append_text(&long_text);

        let display = sp.display_text();
        // Should be MAX_PREVIEW_LEN chars of content + cursor
        // The cursor ▌ is 3 bytes in UTF-8 but 1 char
        let content_without_cursor = &display[..display.len() - "\u{258C}".len()];
        assert_eq!(content_without_cursor.len(), MAX_PREVIEW_LEN);
        assert!(display.ends_with('\u{258C}'));
    }

    #[test]
    fn display_text_truncates_long_cjk_without_panic() {
        // Regression: a long Chinese reply truncated from the start used to
        // panic (`byte index N is not a char boundary`), crashing the event
        // loop so the turn's reply never reached the user. Truncation must be
        // char-boundary-safe and count chars, not bytes.
        let mut sp = StreamPreview::new();
        let long_cjk: String = "当".repeat(MAX_PREVIEW_LEN + 500); // 3 bytes/char
        sp.append_text(&long_cjk);

        let display = sp.display_text(); // must not panic
        // Last MAX_PREVIEW_LEN chars kept, plus the cursor.
        assert_eq!(display.chars().count(), MAX_PREVIEW_LEN + 1);
        assert!(display.ends_with('\u{258C}'));
        assert!(display.starts_with('当'));
    }

    #[test]
    fn finish_transitions_to_finished() {
        let mut sp = StreamPreview::new();
        sp.append_text("done");
        let final_text = sp.finish().to_owned();
        assert!(sp.is_finished());
        assert_eq!(final_text, "done");
    }

    #[test]
    fn final_text_returns_full_buffer() {
        let mut sp = StreamPreview::new();
        sp.append_text("hello ");
        sp.append_text("world");
        assert_eq!(sp.final_text(), "hello world");
    }

    #[test]
    fn was_active_after_text_then_finish() {
        let mut sp = StreamPreview::new();
        sp.append_text("x");
        sp.finish();
        // Finished state != Idle, so was_active is true
        assert!(sp.was_active());
    }

    #[test]
    fn mark_sent_resets_throttle_counters() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();

        // After mark_sent, last_sent_len should match full_text length
        assert_eq!(sp.last_sent_len, sp.full_text.len());
    }

    #[test]
    fn append_while_frozen_still_accumulates_text() {
        let mut sp = StreamPreview::new();
        sp.append_text("init");
        sp.mark_sent();
        sp.freeze();

        sp.append_text(" more text");
        // Text accumulated even though update was suppressed
        assert_eq!(sp.preview_text(), "init more text");
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut sp = StreamPreview::new();
        sp.append_text("hello");
        sp.mark_sent();
        sp.freeze();

        // After reset, should be back to idle with empty text
        sp.reset();
        assert!(sp.is_idle());
        assert!(!sp.was_active());
        assert_eq!(sp.preview_text(), "");

        // First append after reset should return true (new segment)
        assert!(sp.append_text("new text"));
        assert!(sp.was_active());
    }
}
