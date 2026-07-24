//! Agent file parser.

use crate::agent_file::frontmatter::FileAgentFrontmatter;

/// Result of splitting an agent file into its YAML frontmatter and body.
#[derive(Debug)]
pub struct ParsedAgentFile {
    pub frontmatter: Option<FileAgentFrontmatter>,
    pub body: String,
}

/// Split `content` into YAML frontmatter and body.
///
/// - If `content` does not start with `---`, the entire content is returned
///   as the body with `frontmatter = None`.
/// - Otherwise the slice between the opening and closing `---` markers is
///   parsed as YAML into a [`FileAgentFrontmatter`], and the remainder is
///   returned as the body.
///
/// Returns an error string when the closing marker is missing or the YAML
/// cannot be deserialized.
pub fn split_frontmatter(content: &str) -> Result<ParsedAgentFile, String> {
    if !content.starts_with("---") {
        return Ok(ParsedAgentFile {
            frontmatter: None,
            body: content.to_string(),
        });
    }
    let closing = content[3..].find("---");
    let (frontmatter_yaml, body) = match closing {
        Some(i) => {
            let fm_end = 3 + i;
            let body_start = fm_end + 3;
            (&content[3..fm_end], content[body_start..].trim())
        }
        None => return Err("Missing closing ---".to_string()),
    };
    let frontmatter: FileAgentFrontmatter =
        serde_yaml::from_str(frontmatter_yaml)
            .map_err(|e| format!("YAML parse error: {}", e))?;
    Ok(ParsedAgentFile {
        frontmatter: Some(frontmatter),
        body: body.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::split_frontmatter;

    #[test]
    fn no_frontmatter_returns_whole_content_as_body() {
        let parsed =
            split_frontmatter("just body text\n").expect("should parse");
        assert!(parsed.frontmatter.is_none());
        assert_eq!(parsed.body, "just body text\n");
    }

    #[test]
    fn splits_minimal_frontmatter_and_body() {
        let content = "---\n---\nbody content\n";
        let parsed = split_frontmatter(content).expect("should parse");
        let fm = parsed.frontmatter.expect("frontmatter present");
        assert!(fm.model.is_none());
        assert!(fm.permission_rules.is_empty());
        assert!(fm.tools.is_none());
        assert_eq!(parsed.body, "body content");
    }

    #[test]
    fn parses_populated_frontmatter_fields() {
        let content = "\
---
model: claude-opus-4-7
mode: plan
hidden: true
---
the body
";
        let parsed = split_frontmatter(content).expect("should parse");
        let fm = parsed.frontmatter.expect("frontmatter present");
        assert_eq!(fm.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(fm.mode.as_deref(), Some("plan"));
        assert_eq!(fm.hidden, Some(true));
        assert_eq!(parsed.body, "the body");
    }

    #[test]
    fn empty_body_after_frontmatter_is_empty_string() {
        let content = "---\nmodel: x\n---\n";
        let parsed = split_frontmatter(content).expect("should parse");
        assert_eq!(parsed.body, "");
    }

    #[test]
    fn missing_closing_marker_is_error() {
        let content = "---\nmodel: x\nno closer here\n";
        let err = split_frontmatter(content).expect_err("should fail");
        assert!(err.contains("Missing closing"), "got: {err}");
    }

    #[test]
    fn invalid_yaml_is_error() {
        let content = "---\nmodel: [unterminated\n---\nbody\n";
        let err = split_frontmatter(content).expect_err("should fail");
        assert!(err.contains("YAML parse error"), "got: {err}");
    }
}
