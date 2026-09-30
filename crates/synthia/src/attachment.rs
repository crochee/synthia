//! [`synthia_attachment`] — content-addressed
//! multimodal attachments.
//!
//! [`ImageAttachmentRef`] is a `sha256` handle that travels through
//! the conversation instead of inline base64; [`AttachmentStore`]
//! holds the bytes (filesystem or in-memory) and re-verifies the hash
//! on read, so a tampered or truncated image is a typed
//! [`AttachmentError`], never a silent wrong answer.

pub use synthia_attachment::*;
