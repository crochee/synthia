//! Tool-pair repair and micro-compaction.

use synthia_provider::{ContentPart, Message};

use super::types::{is_tool_result, is_tool_use};

pub fn find_tool_use_for_result(
    messages: &[Message],
    result_id: &str,
) -> Option<usize> {
    messages.iter().position(|m| {
        is_tool_use(m)
            && super::types::get_tool_use_id(m).as_deref() == Some(result_id)
    })
}

pub fn find_result_for_tool_use(
    messages: &[Message],
    tool_use_id: &str,
) -> Option<usize> {
    messages.iter().position(|m| {
        is_tool_result(m)
            && super::types::get_tool_result_id(m).as_deref()
                == Some(tool_use_id)
    })
}

fn extract_tool_use_ids(msg: &Message) -> Vec<String> {
    (&msg.content)
        .into_iter()
        .filter_map(|c| match c {
            ContentPart::ToolUse(tu) => Some(tu.id.clone()),
            _ => None,
        })
        .collect()
}

fn extract_tool_result_ids(msg: &Message) -> Vec<String> {
    (&msg.content)
        .into_iter()
        .filter_map(|c| match c {
            ContentPart::ToolResult(tr) => Some(tr.tool_use_id.clone()),
            _ => None,
        })
        .collect()
}

pub fn fix_tool_pairs(messages: &[Message]) -> Vec<Message> {
    let mut fixed = Vec::with_capacity(messages.len());
    let mut pending_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    for msg in messages {
        if is_tool_use(msg) {
            for id in extract_tool_use_ids(msg) {
                pending_ids.insert(id);
            }
            fixed.push(msg.clone());
        } else if is_tool_result(msg) {
            for id in extract_tool_result_ids(msg) {
                pending_ids.remove(&id);
            }
            fixed.push(msg.clone());
        } else {
            fixed.push(msg.clone());
        }
    }

    fixed
}

pub fn micro_compact(messages: &mut [Message], keep_recent: usize) {
    let tool_result_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| {
            if let ContentPart::ToolResult(_) =
                (&msg.content).into_iter().next()?
            {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    if tool_result_indices.len() <= keep_recent {
        return;
    }

    for &msg_idx in tool_result_indices
        .iter()
        .take(tool_result_indices.len() - keep_recent)
    {
        if let Some(msg) = messages.get_mut(msg_idx) {
            msg.content = synthia_provider::Content::text("[cleared]");
        }
    }
}
