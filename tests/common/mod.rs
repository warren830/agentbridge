//! Shared mock types for integration tests.

#![allow(dead_code)]

use std::any::Any;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use async_trait::async_trait;

use agentbridge::core::platform::{
    Button, ImageSender, InlineButtonSender, MessageHandler, MessageUpdater, Platform,
    PlatformCapabilities, PreviewHandle, ReplyCtx, TypingIndicator,
};

// =========================================================================
// Mock ReplyCtx
// =========================================================================

#[derive(Debug, Clone)]
pub struct MockReplyCtx {
    pub channel: String,
    pub user: String,
}

impl ReplyCtx for MockReplyCtx {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn session_key_hint(&self) -> String {
        format!("mock:{}:{}", self.channel, self.user)
    }

    fn clone_box(&self) -> Box<dyn ReplyCtx> {
        Box::new(self.clone())
    }
}

// =========================================================================
// Mock PreviewHandle
// =========================================================================

#[derive(Debug)]
pub struct MockPreviewHandle {
    pub id: u64,
}

impl PreviewHandle for MockPreviewHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// =========================================================================
// Recorded events for assertions
// =========================================================================

#[derive(Debug, Clone)]
pub struct SentMessage {
    pub content: String,
    pub channel: String,
}

#[derive(Debug, Clone)]
pub enum PreviewUpdate {
    Created { id: u64, text: String },
    Updated { id: u64, text: String },
    Deleted { id: u64 },
}

// =========================================================================
// MockPlatform
// =========================================================================

pub struct MockPlatform {
    pub name: String,
    pub messages_sent: Arc<Mutex<Vec<SentMessage>>>,
    pub previews: Arc<Mutex<Vec<PreviewUpdate>>>,
    pub next_preview_id: Arc<Mutex<u64>>,
    pub typing_active: Arc<AtomicBool>,
}

impl MockPlatform {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            messages_sent: Arc::new(Mutex::new(Vec::new())),
            previews: Arc::new(Mutex::new(Vec::new())),
            next_preview_id: Arc::new(Mutex::new(1)),
            typing_active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn sent_messages(&self) -> Vec<SentMessage> {
        self.messages_sent.lock().unwrap().clone()
    }

    pub fn preview_updates(&self) -> Vec<PreviewUpdate> {
        self.previews.lock().unwrap().clone()
    }
}

// -- Platform trait --

#[async_trait]
impl Platform for MockPlatform {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self, _handler: MessageHandler) -> Result<()> {
        Ok(())
    }

    async fn reply(&self, ctx: &dyn ReplyCtx, content: &str) -> Result<()> {
        let channel = ctx.session_key_hint();
        self.messages_sent.lock().unwrap().push(SentMessage {
            content: content.to_string(),
            channel,
        });
        Ok(())
    }

    async fn send(&self, ctx: &dyn ReplyCtx, content: &str) -> Result<()> {
        self.reply(ctx, content).await
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

// -- PlatformCapabilities --

impl PlatformCapabilities for MockPlatform {
    fn as_message_updater(&self) -> Option<&dyn MessageUpdater> {
        Some(self)
    }

    fn as_image_sender(&self) -> Option<&dyn ImageSender> {
        Some(self)
    }

    fn as_inline_button_sender(&self) -> Option<&dyn InlineButtonSender> {
        Some(self)
    }

    fn as_typing_indicator(&self) -> Option<&dyn TypingIndicator> {
        Some(self)
    }
}

// -- MessageUpdater --

#[async_trait]
impl MessageUpdater for MockPlatform {
    async fn send_preview(
        &self,
        _ctx: &dyn ReplyCtx,
        text: &str,
    ) -> Result<Box<dyn PreviewHandle>> {
        let mut id_lock = self.next_preview_id.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        drop(id_lock);

        self.previews.lock().unwrap().push(PreviewUpdate::Created {
            id,
            text: text.to_string(),
        });

        Ok(Box::new(MockPreviewHandle { id }))
    }

    async fn update_preview(&self, handle: &dyn PreviewHandle, text: &str) -> Result<()> {
        let mock = handle.as_any().downcast_ref::<MockPreviewHandle>().unwrap();
        self.previews.lock().unwrap().push(PreviewUpdate::Updated {
            id: mock.id,
            text: text.to_string(),
        });
        Ok(())
    }

    async fn delete_preview(&self, handle: &dyn PreviewHandle) -> Result<()> {
        let mock = handle.as_any().downcast_ref::<MockPreviewHandle>().unwrap();
        self.previews
            .lock()
            .unwrap()
            .push(PreviewUpdate::Deleted { id: mock.id });
        Ok(())
    }
}

// -- ImageSender --

#[async_trait]
impl ImageSender for MockPlatform {
    async fn send_image(
        &self,
        ctx: &dyn ReplyCtx,
        _data: &[u8],
        filename: &str,
        _mime: &str,
    ) -> Result<()> {
        let channel = ctx.session_key_hint();
        self.messages_sent.lock().unwrap().push(SentMessage {
            content: format!("[image:{}]", filename),
            channel,
        });
        Ok(())
    }
}

// -- InlineButtonSender --

#[async_trait]
impl InlineButtonSender for MockPlatform {
    async fn send_with_buttons(
        &self,
        ctx: &dyn ReplyCtx,
        text: &str,
        buttons: &[Button],
    ) -> Result<Box<dyn PreviewHandle>> {
        let channel = ctx.session_key_hint();
        let btn_text: Vec<&str> = buttons.iter().map(|b| b.text.as_str()).collect();
        self.messages_sent.lock().unwrap().push(SentMessage {
            content: format!("{} [buttons: {}]", text, btn_text.join(", ")),
            channel,
        });

        let mut id_lock = self.next_preview_id.lock().unwrap();
        let id = *id_lock;
        *id_lock += 1;
        Ok(Box::new(MockPreviewHandle { id }))
    }

    async fn answer_callback(&self, _callback_id: &str, _text: &str) -> Result<()> {
        Ok(())
    }
}

// -- TypingIndicator --

#[async_trait]
impl TypingIndicator for MockPlatform {
    async fn start_typing(&self, _ctx: &dyn ReplyCtx) -> Result<Box<dyn FnOnce() + Send>> {
        self.typing_active.store(true, Ordering::SeqCst);
        let flag = Arc::clone(&self.typing_active);
        Ok(Box::new(move || {
            flag.store(false, Ordering::SeqCst);
        }))
    }
}
