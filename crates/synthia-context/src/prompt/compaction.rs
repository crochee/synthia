#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionType {
    Auto,
    Manual,
    ToolLoop,
}

pub const COMPACTION_SYSTEM_PROMPT: &str = r#"## Task

Context limit reached. Summarize below messages, keeping only essential content for session continuation.

{{ messages }}

<analysis>
Review chronologically, log: user goals, your methods, key decisions, files/code, errors/fixes, user feedback.
</analysis>

### Include:

1. **User Intent** – All goals/requests (preserve verbatim when short)
2. **Technical Decisions** – Choices made AND why (rationale is critical for consistency)
3. **Files + Code** – Viewed/edited files, key code snippets, change descriptions
4. **Errors + Fixes** – Full error messages + root cause + resolution path
5. **Problem Solving** – Issues solved/in-progress, approaches tried and abandoned
6. **User Messages** – All user messages (truncate long tool args/results)
7. **Pending Tasks** – Unresolved requests with their current state
8. **Current Work** – Active work: filenames, code, alignment with original goal
9. **Next Step** – Only if directly continues user's last instruction

### Tiered Retention (strictly follow this order):

- **Tier 1 (Preserve fully — these have long half-life)**:
  - Key architectural/implementation decisions with rationale
  - Error messages with complete root cause analysis and fix
  - Security-related findings or concerns
  - User's explicit feedback, corrections, or rejections of your approach
  - Task completion confirmations with outcomes

- **Tier 2 (One-line summary each — medium half-life)**:
  - File modifications: `path: description of change`
  - Test results: `suite: N passed, M failed`
  - Configuration changes: `file: key changed from X to Y`
  - Dependencies added/removed: `package@version`

- **Tier 3 (Omit entirely — can be reacquired on demand)**:
  - File read outputs (use Read tool to re-read)
  - Command output dumps (re-run the command)
  - Search/Grep results (search again)
  - Intermediate reasoning drafts

> Rules:
> - Never invent details that weren't in the conversation
> - Preserve exact file paths and function/variable names
> - If a task was abandoned, note WHY (error? user redirect? better approach found?)
> - No new ideas unless user confirmed"#;

pub const COMPACTION_USER_PROMPT: &str =
    "Please summarize the conversation history provided in the system prompt.";

pub const CONVERSATION_CONTINUATION_TEXT: &str = "The previous message contains a summary due to context limit. Continue naturally without mentioning summarization.";

pub const TOOL_LOOP_CONTINUATION_TEXT: &str = "The previous message contains a summary due to context limit. Continue calling tools as necessary.";

pub const MANUAL_COMPACT_CONTINUATION_TEXT: &str = "The previous message contains a summary prepared at user request. Continue naturally without mentioning summarization.";

pub const AUTO_COMPACT_CONTINUATION_TEXT: &str = "The previous message contains a summary due to context limit. Continue naturally.";

pub fn render_compaction_prompt(messages_text: &str) -> anyhow::Result<String> {
    Ok(COMPACTION_SYSTEM_PROMPT.replace("{{ messages }}", messages_text))
}

pub fn render_compaction_prompt_with_type(
    compaction_type: CompactionType,
    messages_text: &str,
) -> anyhow::Result<String> {
    let system_prompt =
        COMPACTION_SYSTEM_PROMPT.replace("{{ messages }}", messages_text);

    let continuation_text = match compaction_type {
        CompactionType::Auto => AUTO_COMPACT_CONTINUATION_TEXT,
        CompactionType::Manual => MANUAL_COMPACT_CONTINUATION_TEXT,
        CompactionType::ToolLoop => TOOL_LOOP_CONTINUATION_TEXT,
    };

    Ok(format!(
        "{system_prompt}\n\n### Continuation\n\n{continuation_text}"
    ))
}

pub fn format_compact_summary(raw: &str) -> String {
    let mut formatted = raw.to_string();

    formatted = formatted
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let analysis_pattern =
        regex::Regex::new(r"<analysis>[\s\S]*?</analysis>").ok();
    if let Some(re) = analysis_pattern {
        formatted = re.replace_all(&formatted, "").to_string();
    }

    let summary_pattern =
        regex::Regex::new(r"<summary>([\s\S]*?)</summary>").ok();
    if let Some(re) = summary_pattern
        && let Some(caps) = re.captures(&formatted)
        && let Some(content) = caps.get(1)
    {
        formatted = format!("Summary:\n{}", content.as_str().trim());
    }

    formatted.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_compaction_prompt() {
        let messages = "User: Hello\nAssistant: Hi there!";
        let result = render_compaction_prompt(messages).unwrap();
        assert!(result.contains("Hello"));
        assert!(result.contains("Hi there!"));
        assert!(result.contains("User Intent"));
    }

    #[test]
    fn test_render_compaction_prompt_with_type() {
        let messages = "User: Hello\nAssistant: Hi there!";
        let result =
            render_compaction_prompt_with_type(CompactionType::Auto, messages)
                .unwrap();
        assert!(result.contains("Hello"));
        assert!(result.contains("context limit"));
    }

    #[test]
    fn test_format_compact_summary_strips_analysis() {
        let raw = "<analysis>draft thoughts</analysis>\n\n<summary>\n1. Task: Test\n</summary>";
        let result = format_compact_summary(raw);
        assert!(!result.contains("draft thoughts"));
        assert!(result.contains("Task: Test"));
    }

    #[test]
    fn test_format_compact_summary_extracts_summary() {
        let raw = "<summary>\n1. Primary: Test task\n2. Pending: More work\n</summary>";
        let result = format_compact_summary(raw);
        assert!(result.contains("Primary: Test task"));
        assert!(result.contains("Pending: More work"));
    }
}
