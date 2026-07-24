//! Conversation-repair pipeline.
//!
//! Takes a raw `Vec<Message>` from a checkpoint, a tool
//! output, or any other source, and produces a
//! "well-formed" version that downstream LLM calls
//! accept. The only public entry point is
//! [`fix_conversation`].
//!
//! The work is split into focused submodules:
//!
//! - [`keys`]: stable per-message string keys used by
//!   [`processors::deduplicate_messages`].
//! - [`content_ops`]: low-level content-shape primitives
//!   (`is_empty_content`, `trim_text_content`,
//!   `effective_role`, `merge_messages`,
//!   `merge_text_in_message`) consumed by the processors.
//! - [`processors`]: the 8 `pub(crate)` processors
//!   composed in order by [`pipeline::fix_conversation`].
//! - [`pipeline`]: the [`fix_conversation`] entry point
//!   plus the [`pipeline::MessageProcessor`] type alias.
//! - [`tests`]: the 18 unit tests pinning the
//!   pipeline's behaviour and idempotency invariant.
//!
//! [`Message`]: synthia_provider::types::Message

mod content_ops;
mod keys;
mod pipeline;
mod processors;
// `tests` is the conventional name for the unit-test
// submodule; the `module_inception` warning is the
// standard cost of that convention.
#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use pipeline::fix_conversation;
