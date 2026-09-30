//! Serialisation of one tool-result batch for the summariser.
//!
//! Every payload is wrapped in `<tool_outputs>` XML fences so the
//! LLM treats it as data, not instructions (the prompt-injection
//! mitigation borrowed from `pi-lcm/src/compaction/prompts.ts`).

use std::path::PathBuf;

use synthia_provider::{ContentPart, Message, Role};

use crate::compaction_settings::CompactionDetails;

pub(super) fn extract_tool_result_text(msg: &Message) -> Option<String> {
    if msg.role != Role::Tool {
        return None;
    }
    let mut out = String::new();
    for part in msg.content.iter() {
        match part {
            ContentPart::Text(tc) => out.push_str(&tc.text),
            ContentPart::ToolResult(tr) => {
                for inner in &tr.content {
                    if let ContentPart::Text(tc) = inner {
                        out.push_str(&tc.text);
                    }
                }
                if let Some(structured) = &tr.structured_content {
                    out.push_str(&structured.to_string());
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// Serialise `batch` (indices into `messages`) as the summariser
/// prompt body: one `[n] tool_call_id=… tool=… is_error=…` block per
/// tool call, so the entire payload sits inside `<tool_outputs>`
/// fences and the LLM treats it as data rather than instructions
/// (prompt-injection mitigation borrowed from
/// `pi-lcm/src/compaction/prompts.ts`).
pub(super) fn serialise_batch_for_summariser(
    batch: &[usize],
    messages: &[Message],
    details: Option<&CompactionDetails>,
) -> String {
    let mut out = String::new();
    out.push_str("<tool_outputs>\n");
    for (n, &i) in batch.iter().enumerate() {
        let Some(msg) = messages.get(i) else { continue };
        let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
        let mut tool_name = String::new();
        let mut is_error = false;
        let mut result_text = String::new();

        for part in msg.content.iter() {
            match part {
                ContentPart::ToolResult(tr) => {
                    tool_name = tr.tool_name.clone().unwrap_or_default();
                    is_error = tr.is_error.unwrap_or(false);
                    for inner in &tr.content {
                        if let ContentPart::Text(tc) = inner {
                            result_text.push_str(&tc.text);
                        }
                    }
                }
                ContentPart::Text(tc) => {
                    result_text.push_str(&tc.text);
                }
                _ => {}
            }
        }

        out.push_str(&format!(
            "[{n}] tool_call_id={tool_call_id} tool={tool_name} is_error={is_error}\n{result_text}\n\n"
        ));
    }
    out.push_str("</tool_outputs>");
    if let Some(details) = details {
        append_compaction_details(&mut out, details);
    }
    out
}

/// R29-C: append the "files touched since the last compaction"
/// line to a serialised batch. `read_files` / `modified_files`
/// are listed verbatim (display form); the whole line is omitted
/// when both are empty, so a manager without details produces
/// exactly the legacy prompt. The line lands *after* the
/// `<tool_outputs>` fence because it is harness metadata, not
/// tool output.
fn append_compaction_details(out: &mut String, details: &CompactionDetails) {
    let mut segments: Vec<String> = Vec::new();
    if !details.read_files.is_empty() {
        segments.push(format!("read [{}]", join_paths(&details.read_files)));
    }
    if !details.modified_files.is_empty() {
        segments.push(format!(
            "modified [{}]",
            join_paths(&details.modified_files)
        ));
    }
    if segments.is_empty() {
        return;
    }
    out.push_str(&format!(
        "\nFiles touched since the last compaction: {}",
        segments.join(", "),
    ));
}

/// Render a path list as a comma-separated display string.
fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
