//! The pipeline entry point — [`fix_conversation`].
//!
//! Composes the 8 processors in [`super::processors`] in
//! a fixed order. The order is significant: earlier
//! processors may produce output that later processors
//! rely on (e.g., [`super::processors::fix_tool_calling`]
//! prunes tool uses that would otherwise block
//! [`super::processors::merge_consecutive_messages`] from
//! merging user/assistant runs).
//!
//! [`Message`]: synthia_provider::types::Message

use synthia_provider::Message;

use super::processors::{
    deduplicate_messages,
    fix_lead_trail,
    fix_tool_calling,
    merge_consecutive_messages,
    merge_text_content_items,
    populate_if_empty,
    remove_empty_messages,
    trim_assistant_text_whitespace,
};

/// A pure message-list transformer: takes `Vec<Message>`
/// and returns `(fixed_messages, issues)`.
pub(super) type MessageProcessor =
    fn(Vec<Message>) -> (Vec<Message>, Vec<String>);

/// Run all 8 message processors in order, accumulating
/// every reported issue.
///
/// The pipeline is **idempotent**: running
/// `fix_conversation` twice on the same input produces
/// no new issues on the second pass. This is the
/// invariant the `run_verify` test helper pins down.
pub fn fix_conversation(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let processors: Vec<MessageProcessor> = vec![
        deduplicate_messages,
        merge_text_content_items,
        trim_assistant_text_whitespace,
        remove_empty_messages,
        fix_tool_calling,
        merge_consecutive_messages,
        fix_lead_trail,
        populate_if_empty,
    ];

    processors.into_iter().fold(
        (messages, Vec::new()),
        |(msgs, issues), processor| {
            let (new_msgs, new_issues) = processor(msgs);
            (new_msgs, issues.into_iter().chain(new_issues).collect())
        },
    )
}
