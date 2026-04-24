//! Discord-specific ReplyCtx and PreviewHandle types.

#![allow(dead_code)] // InteractionReplyCtx fields carry platform context consumed via trait methods

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::core::platform::{PreviewHandle, ReplyCtx};

// ---------------------------------------------------------------------------
// DiscordReplyCtx -- regular channel/thread message context.
// ---------------------------------------------------------------------------

/// Routing context for a regular Discord channel or thread message.
#[derive(Debug, Clone)]
pub struct DiscordReplyCtx {
    pub channel_id: String,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
}

impl ReplyCtx for DiscordReplyCtx {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn session_key_hint(&self) -> String {
        // Prefer thread_id (isolates sessions per thread), fall back to channel_id.
        if let Some(ref tid) = self.thread_id {
            format!("discord:{}", tid)
        } else {
            format!("discord:{}", self.channel_id)
        }
    }

    fn clone_box(&self) -> Box<dyn ReplyCtx> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// InteractionReplyCtx -- slash command / interaction context.
// ---------------------------------------------------------------------------

/// Routing context for a Discord slash command interaction.
///
/// The first response must go through the interaction webhook; subsequent
/// messages are sent as follow-up messages via the webhook URL.
#[derive(Debug, Clone)]
pub struct InteractionReplyCtx {
    pub interaction_id: String,
    pub interaction_token: String,
    pub channel_id: String,
    /// Tracks whether the initial deferred response has been edited yet.
    /// The first reply edits the original `@original`; follow-ups use the
    /// webhook follow-up endpoint.
    pub first_response_sent: Arc<AtomicBool>,
}

impl ReplyCtx for InteractionReplyCtx {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn session_key_hint(&self) -> String {
        format!("discord:{}", self.channel_id)
    }

    fn clone_box(&self) -> Box<dyn ReplyCtx> {
        Box::new(self.clone())
    }
}

impl InteractionReplyCtx {
    /// Returns `true` the first time it is called; `false` thereafter.
    pub fn claim_first_response(&self) -> bool {
        !self.first_response_sent.swap(true, Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// DiscordPreviewHandle -- editable message handle for streaming previews.
// ---------------------------------------------------------------------------

/// Handle to a Discord message that can be edited or deleted in-place.
#[derive(Debug)]
pub struct DiscordPreviewHandle {
    pub channel_id: String,
    pub message_id: String,
}

impl PreviewHandle for DiscordPreviewHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_reply_ctx_session_key_uses_thread_id() {
        let ctx = DiscordReplyCtx {
            channel_id: "100".into(),
            message_id: None,
            thread_id: Some("200".into()),
        };
        assert_eq!(ctx.session_key_hint(), "discord:200");
    }

    #[test]
    fn discord_reply_ctx_session_key_falls_back_to_channel() {
        let ctx = DiscordReplyCtx {
            channel_id: "100".into(),
            message_id: None,
            thread_id: None,
        };
        assert_eq!(ctx.session_key_hint(), "discord:100");
    }

    #[test]
    fn interaction_reply_ctx_claim_first_response() {
        let ctx = InteractionReplyCtx {
            interaction_id: "1".into(),
            interaction_token: "tok".into(),
            channel_id: "100".into(),
            first_response_sent: Arc::new(AtomicBool::new(false)),
        };
        assert!(ctx.claim_first_response());
        assert!(!ctx.claim_first_response());
    }

    #[test]
    fn clone_box_preserves_type() {
        let ctx = DiscordReplyCtx {
            channel_id: "42".into(),
            message_id: Some("7".into()),
            thread_id: None,
        };
        let boxed: Box<dyn ReplyCtx> = ctx.clone_box();
        let downcasted = boxed.as_any().downcast_ref::<DiscordReplyCtx>().unwrap();
        assert_eq!(downcasted.channel_id, "42");
        assert_eq!(downcasted.message_id.as_deref(), Some("7"));
    }

    #[test]
    fn preview_handle_downcast() {
        let handle = DiscordPreviewHandle {
            channel_id: "c1".into(),
            message_id: "m1".into(),
        };
        let any_ref: &dyn Any = handle.as_any();
        let downcasted = any_ref.downcast_ref::<DiscordPreviewHandle>().unwrap();
        assert_eq!(downcasted.message_id, "m1");
    }
}
