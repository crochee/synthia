//! Short aliases (`t1`, `t2`, …) and the summary message that
//! carries them, so `context_tree_query` can resolve a ref back
//! to the archived original.

use synthia_provider::{Content, Message, Role};

use super::SUMMARY_TAG;

#[derive(Clone, Debug)]
pub(super) struct ShortRef {
    pub(super) short_id: String,
}

pub(super) fn allocate_short_refs(batch: &[usize]) -> Vec<ShortRef> {
    batch
        .iter()
        .enumerate()
        .map(|(n, _)| ShortRef {
            short_id: format!("t{}", n + 1),
        })
        .collect()
}

pub(super) fn build_summary_message(
    refs: &[ShortRef],
    summary_text: &str,
) -> Message {
    let mut body = String::new();
    body.push_str(&format!("<{SUMMARY_TAG}>\n{summary_text}\n\n"));
    body.push_str("Summarized tool refs:");
    for r in refs {
        body.push_str(&format!(" `{}`", r.short_id));
    }
    body.push_str("\nUse `context_tree_query` with these refs to retrieve the original full outputs.\n");
    body.push_str(&format!("</{SUMMARY_TAG}>"));

    Message::new(Role::Assistant, Content::text(body))
}
