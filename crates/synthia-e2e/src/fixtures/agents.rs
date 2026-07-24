#[derive(Debug, Clone)]
pub struct TestAgent {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub capabilities: Vec<String>,
}

impl TestAgent {
    pub fn code_reviewer() -> Self {
        Self {
            name: "code_reviewer".to_string(),
            description:
                "An agent specialized in code review and bug detection"
                    .to_string(),
            system_prompt: r#"You are a code reviewer agent specialized in:
- Finding bugs and security vulnerabilities
- Checking code style and best practices
- Suggesting improvements
- Verifying test coverage

When reviewing code, always check for:
1. Security issues (SQL injection, XSS, etc.)
2. Error handling completeness
3. Performance considerations
4. Code readability

Provide specific, actionable feedback with line numbers when possible."#
                .to_string(),
            capabilities: vec![
                "read_file".to_string(),
                "search_files".to_string(),
                "run_tests".to_string(),
            ],
        }
    }

    pub fn bug_reporter() -> Self {
        Self {
            name: "bug_reporter".to_string(),
            description: "An agent specialized in reporting and tracking bugs"
                .to_string(),
            system_prompt: r#"You are a bug reporter agent specialized in:
- Reproducing and documenting bugs
- Categorizing bug severity (critical, high, medium, low)
- Suggesting potential root causes
- Creating structured bug reports

When reporting bugs, always include:
1. Clear description of the issue
2. Steps to reproduce
3. Expected vs actual behavior
4. Environment details
5. Severity assessment

Format reports in a structured manner for easy triage."#
                .to_string(),
            capabilities: vec![
                "create_file".to_string(),
                "append_file".to_string(),
                "search_files".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_reviewer_has_capabilities() {
        let reviewer = TestAgent::code_reviewer();
        assert!(reviewer.capabilities.contains(&"read_file".to_string()));
        assert!(reviewer.system_prompt.contains("security"));
    }

    #[test]
    fn test_bug_reporter_has_capabilities() {
        let reporter = TestAgent::bug_reporter();
        assert!(reporter.capabilities.contains(&"create_file".to_string()));
        assert!(
            reporter
                .system_prompt
                .to_lowercase()
                .contains("reproducing")
        );
    }
}
