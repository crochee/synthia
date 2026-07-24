//! Anchored Summary 8-section template (OpenSpec task 3.2).
//!
//! Provides:
//! - A fixed 8-section [`SECTIONS`] template + [`PLACEHOLDER`] for empty slots.
//! - [`AnchoredSummary::parse`] / [`AnchoredSummary::get`] for round-tripping a
//!   rendered summary.
//! - [`generate_prompt`] to render either a full-generation prompt (no
//!   previous summary) or an incremental-update prompt (with previous summary).
//! - [`split_at_token_budget`] to fit a previous summary + new messages into a
//!   token budget, preferring message-boundary splits and falling back to
//!   UTF-8-safe mid-message slicing.
//!
//! Token estimation is intentionally simple: `tokens ≈ chars / 4`. No tokenizer
//! dependency is introduced.

use std::collections::BTreeMap;

use synthia_provider::{Content, Message};

/// The 8 sections of an anchored summary, in canonical order.
pub const SECTIONS: [&str; 8] = [
    "Goal",
    "Constraints",
    "Progress",
    "Key Decisions",
    "Next Steps",
    "Critical Context",
    "Relevant Files",
    "Closing",
];

/// Placeholder rendered for sections that have no content.
pub const PLACEHOLDER: &str = "_(none)_";

/// A parsed anchored summary: a map from section name to its content.
///
/// All 8 sections are always present after [`parse`](Self::parse); missing or
/// empty sections map to [`PLACEHOLDER`].
#[derive(Debug, Clone, Default)]
pub struct AnchoredSummary {
    sections: BTreeMap<String, String>,
}

impl AnchoredSummary {
    /// Parse a rendered anchored summary text into its 8 sections.
    ///
    /// Sections are delimited by markdown headers (`## <Section Name>`).
    /// Unknown headers are ignored. Missing / empty sections resolve to
    /// [`PLACEHOLDER`].
    pub fn parse(text: &str) -> Self {
        let mut sections: BTreeMap<String, String> = SECTIONS
            .iter()
            .map(|s| (s.to_string(), PLACEHOLDER.to_string()))
            .collect();

        let mut current: Option<String> = None;
        let mut buf = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("## ") {
                finalize(&mut sections, &current, &buf);
                buf.clear();
                let name = rest.trim();
                if SECTIONS.contains(&name) {
                    current = Some(name.to_string());
                } else {
                    current = None;
                }
            } else if current.is_some() {
                buf.push_str(line);
                buf.push('\n');
            }
        }
        finalize(&mut sections, &current, &buf);

        AnchoredSummary { sections }
    }

    /// Look up a section by its canonical name.
    ///
    /// Returns `Some(value)` for any of the 8 known section names (always
    /// populated by [`parse`](Self::parse)) and `None` for unknown names.
    pub fn get(&self, section: &str) -> Option<&str> {
        self.sections.get(section).map(String::as_str)
    }
}

/// Store trimmed `buf` under `current` in `sections`, unless empty (in which
/// case the placeholder default is kept).
fn finalize(
    sections: &mut BTreeMap<String, String>,
    current: &Option<String>,
    buf: &str,
) {
    if let Some(cur) = current {
        let content = buf.trim();
        if !content.is_empty() {
            sections.insert(cur.clone(), content.to_string());
        }
    }
}

/// Render the LLM prompt for the anchored summary.
///
/// - `previous_summary = None` → full generation prompt instructing the model
///   to produce all 8 sections, using [`PLACEHOLDER`] for empty ones.
/// - `previous_summary = Some(prev)` → incremental update prompt embedding the
///   previous summary and instructing the model to preserve unchanged
///   sections.
pub fn generate_prompt(previous_summary: Option<&str>) -> String {
    match previous_summary {
        None => generation_prompt(),
        Some(prev) => update_prompt(prev),
    }
}

/// Full-generation prompt (no previous summary).
fn generation_prompt() -> String {
    let mut s = String::new();
    s.push_str(
        "Generate the anchored summary with the following 8 sections.\n\n",
    );
    s.push_str("Sections (in this exact order):\n");
    for (i, section) in SECTIONS.iter().enumerate() {
        s.push_str(&format!("{}. {section}\n", i + 1));
    }
    s.push_str(&format!(
        "\nIf a section has no content, use the placeholder: {PLACEHOLDER}\n"
    ));
    s.push_str("Render each section as a markdown header `## <Section Name>` followed by the section content.\n");
    s
}

/// Incremental-update prompt (with previous summary).
fn update_prompt(prev: &str) -> String {
    let mut s = String::new();
    s.push_str("Update the anchored summary below.\n\n");
    s.push_str("Preserve the 8-section structure and keep unchanged sections verbatim.\n");
    s.push_str(&format!(
        "If a section becomes empty, use the placeholder: {PLACEHOLDER}\n\n"
    ));
    s.push_str("Previous anchored summary:\n\n");
    s.push_str(prev);
    s
}

/// Fit `previous_summary` + `new_messages` into `budget` (estimated tokens).
///
/// Token estimate: `tokens ≈ chars / 4`. The previous summary consumes budget
/// first; remaining budget is spent on whole messages in order. When the next
/// whole message would not fit but budget remains, it is sliced at a UTF-8
/// char boundary and the `[truncated-mid-message]` marker is appended. Returns
/// the messages that fit (the previous summary itself is not included in the
/// returned vector).
pub fn split_at_token_budget(
    previous_summary: &str,
    new_messages: &[Message],
    budget: usize,
) -> Vec<Message> {
    // The previous anchored summary is accounted against the budget first
    // (it always travels with the request) but is not returned by this fn.
    let mut remaining =
        budget.saturating_sub(estimate_tokens(previous_summary));
    let mut out = Vec::new();

    for msg in new_messages {
        let text = msg.content.extract_text().unwrap_or_default();
        let cost = estimate_tokens(&text);
        if cost <= remaining {
            // Whole message fits.
            out.push(msg.clone());
            remaining -= cost;
        } else if remaining > 0 {
            // Mid-message slice: prefer message boundaries, but when the next
            // whole message would overflow, slice at a UTF-8 char boundary.
            let sliced = slice_to_fit_chars(&text, remaining * 4);
            out.push(Message::new(
                msg.role,
                Content::text(format!("{sliced}[truncated-mid-message]")),
            ));
            break;
        } else {
            // No budget left at all.
            break;
        }
    }
    out
}

/// `tokens ≈ chars / 4`. Intentionally simple — no tokenizer dependency.
fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

/// Return the longest `&str` prefix of `text` that contains at most
/// `max_chars` Unicode scalar values, landing on a UTF-8 char boundary.
fn slice_to_fit_chars(text: &str, max_chars: usize) -> &str {
    for (count, (byte_idx, _)) in text.char_indices().enumerate() {
        if count >= max_chars {
            return &text[..byte_idx];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    #[test]
    fn template_has_8_sections() {
        assert_eq!(SECTIONS.len(), 8);
        assert_eq!(
            SECTIONS,
            [
                "Goal",
                "Constraints",
                "Progress",
                "Key Decisions",
                "Next Steps",
                "Critical Context",
                "Relevant Files",
                "Closing",
            ]
        );
    }

    #[test]
    fn empty_section_uses_placeholder() {
        // Only the Goal section is provided; every other section must resolve
        // to the placeholder.
        let text = "## Goal\nBuild the thing\n";
        let s = AnchoredSummary::parse(text);
        assert_eq!(s.get("Goal"), Some("Build the thing"));
        for section in SECTIONS {
            if section == "Goal" {
                continue;
            }
            assert_eq!(
                s.get(section),
                Some(PLACEHOLDER),
                "section {section} should fall back to placeholder"
            );
        }
        // An explicitly-empty section also resolves to the placeholder.
        let text_with_empty =
            "## Goal\nBuild the thing\n\n## Constraints\n\n## Next Steps\n";
        let s2 = AnchoredSummary::parse(text_with_empty);
        assert_eq!(s2.get("Constraints"), Some(PLACEHOLDER));
        assert_eq!(s2.get("Next Steps"), Some(PLACEHOLDER));
    }

    #[test]
    fn generate_prompt_without_previous() {
        let prompt = generate_prompt(None);
        assert!(
            prompt.contains("Generate the anchored summary"),
            "prompt must mention generation: {prompt}"
        );
        // Every section name is referenced.
        for section in SECTIONS {
            assert!(
                prompt.contains(section),
                "prompt must mention section {section}: {prompt}"
            );
        }
        assert!(
            prompt.contains(PLACEHOLDER),
            "prompt must mention the placeholder: {prompt}"
        );
    }

    #[test]
    fn update_prompt_with_previous() {
        let prev = "## Goal\nShip it\n";
        let prompt = generate_prompt(Some(prev));
        assert!(
            prompt.contains("Update the anchored summary"),
            "prompt must mention update: {prompt}"
        );
        assert!(
            prompt.contains(prev),
            "prompt must embed the previous summary verbatim: {prompt}"
        );
    }

    #[test]
    fn split_at_message_boundary() {
        // 3 messages of 8 ASCII chars each = 2 tokens each.
        let msgs = vec![
            user_msg("aaaaaaaa"),
            user_msg("bbbbbbbb"),
            user_msg("cccccccc"),
        ];
        // Budget of 4 tokens fits the previous summary (empty) + exactly 2
        // messages. The third must be dropped whole (not sliced, since the
        // second message exhausts the budget).
        let out = split_at_token_budget("", &msgs, 4);
        assert_eq!(out.len(), 2, "expected exactly 2 messages to fit");
        assert_eq!(out[0], msgs[0]);
        assert_eq!(out[1], msgs[1]);
    }

    #[test]
    fn split_mid_message_utf8_safe() {
        // Multibyte content: "abc世" then more. A naive byte slice at the byte
        // length corresponding to the char budget would split the 3-byte 世;
        // char_indices slicing must land on a UTF-8 boundary instead.
        let text = "abc世界def";
        let msg = user_msg(text);
        // Budget of 1 token = 4 chars. The whole message is 8 chars = 2
        // tokens, so it must be sliced to 4 chars: "abc世".
        let out = split_at_token_budget("", std::slice::from_ref(&msg), 1);
        assert_eq!(out.len(), 1, "expected the single message to be sliced in");
        let sliced = out[0]
            .content
            .extract_text()
            .expect("sliced message should have text");
        assert!(
            sliced.ends_with("[truncated-mid-message]"),
            "sliced message must carry the truncation marker: {sliced}"
        );
        // The original-text prefix must be a valid UTF-8 prefix at a char
        // boundary: first 4 chars of "abc世界def" == "abc世".
        let prefix = sliced
            .strip_suffix("[truncated-mid-message]")
            .expect("marker should be present");
        assert_eq!(prefix, "abc世");
    }

    #[test]
    fn split_with_zero_budget() {
        let msgs = vec![user_msg("hello world")];
        let out = split_at_token_budget("", &msgs, 0);
        assert!(out.is_empty(), "zero budget must yield no messages");
        // Also no panic when a previous summary already consumes the budget.
        let out2 = split_at_token_budget("some previous summary", &msgs, 0);
        assert!(out2.is_empty());
    }

    #[test]
    fn parse_full_summary_roundtrip() {
        let text = "\
## Goal
Build the anchored summary template

## Constraints
No new tokenizer deps

## Progress
Done: section template
InProgress: prompt generation
Blocked: nothing

## Key Decisions
Use chars / 4 for tokens

## Next Steps
Wire into compactor

## Critical Context
KV-cache prefix stability is the top constraint

## Relevant Files
crates/synthia-context/src/anchored_summary.rs

## Closing
Keep it simple
";
        let s = AnchoredSummary::parse(text);
        assert_eq!(s.get("Goal"), Some("Build the anchored summary template"));
        assert_eq!(s.get("Constraints"), Some("No new tokenizer deps"));
        assert_eq!(
            s.get("Progress"),
            Some(
                "Done: section template\nInProgress: prompt generation\nBlocked: nothing"
            )
        );
        assert_eq!(s.get("Key Decisions"), Some("Use chars / 4 for tokens"));
        assert_eq!(s.get("Next Steps"), Some("Wire into compactor"));
        assert_eq!(
            s.get("Critical Context"),
            Some("KV-cache prefix stability is the top constraint")
        );
        assert_eq!(
            s.get("Relevant Files"),
            Some("crates/synthia-context/src/anchored_summary.rs")
        );
        assert_eq!(s.get("Closing"), Some("Keep it simple"));
    }
}
