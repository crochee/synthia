//! The 8 [`MessageProcessor`]s composed by
//! [`super::pipeline::fix_conversation`].
//!
//! Each function takes `Vec<Message>` and returns
//! `(Vec<Message>, Vec<String>)` — the fixed messages and
//! the human-readable list of "issues" the processor
//! reported (one entry per change). All processors are
//! pure: they do not touch the session store, the LLM,
//! or any other side-effecting subsystem.
//!
//! See [`super::pipeline::fix_conversation`] for the
//! pipeline order. The processors are listed in the same
//! order as the pipeline below:
//!
//! 1. [`deduplicate_messages`]
//! 2. [`merge_text_content_items`]
//! 3. [`trim_assistant_text_whitespace`]
//! 4. [`remove_empty_messages`]
//! 5. [`fix_tool_calling`]
//! 6. [`merge_consecutive_messages`]
//! 7. [`fix_lead_trail`]
//! 8. [`populate_if_empty`]

mod aggregate;
mod basic;
mod tool;

pub(crate) use aggregate::{
    fix_lead_trail,
    merge_consecutive_messages,
    populate_if_empty,
};
pub(crate) use basic::{
    deduplicate_messages,
    merge_text_content_items,
    remove_empty_messages,
    trim_assistant_text_whitespace,
};
pub(crate) use tool::fix_tool_calling;
