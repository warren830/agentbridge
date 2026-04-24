#![allow(dead_code)] // message_id reserved for edit/reply routing

use std::any::Any;

use crate::core::platform::{PreviewHandle, ReplyCtx};

// ---------------------------------------------------------------------------
// TelegramReplyCtx -- carries chat/thread/message routing info.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TelegramReplyCtx {
    pub chat_id: i64,
    pub thread_id: Option<i64>,
    pub message_id: Option<i64>,
}

impl ReplyCtx for TelegramReplyCtx {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn session_key_hint(&self) -> String {
        format!("telegram:{}", self.chat_id)
    }

    fn clone_box(&self) -> Box<dyn ReplyCtx> {
        Box::new(self.clone())
    }
}

// ---------------------------------------------------------------------------
// TelegramPreviewHandle -- identifies a sent message for later edits/deletes.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TelegramPreviewHandle {
    pub chat_id: i64,
    pub message_id: i64,
}

impl PreviewHandle for TelegramPreviewHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
