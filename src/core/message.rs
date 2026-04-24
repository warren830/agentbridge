#![allow(dead_code)] // data carriers for platform messages; fields are API surface

use super::platform::ReplyCtx;

// ---------------------------------------------------------------------------
// IncomingMessage -- normalised message from any platform.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct IncomingMessage {
    /// Platform-specific message id.
    pub id: String,

    /// Sender identifier (user id / username).
    pub from: String,

    /// Human-readable display name, if available.
    pub from_name: Option<String>,

    /// Plain-text body of the message.
    pub text: String,

    /// Attached images (already downloaded).
    pub images: Vec<ImageAttachment>,

    /// Attached files (already downloaded).
    pub files: Vec<FileAttachment>,

    /// Path to a downloaded voice file, if any.
    pub voice: Option<String>,

    /// Whether the message was sent in a group / channel context.
    pub is_group: bool,

    /// Platform-specific channel / chat id.
    pub channel_id: Option<String>,

    /// Human-readable channel / thread name for auto-naming sessions.
    pub channel_name: Option<String>,

    /// Opaque routing context the engine passes back when replying.
    pub reply_ctx: Box<dyn ReplyCtx>,
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ImageAttachment {
    pub data: Vec<u8>,
    pub mime_type: String,
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct FileAttachment {
    pub data: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
}
