use super::PromptSection;
use crate::prompt::{Result, SectionCaching};

const TOKEN_BUDGET_SECTION: &str = r#"## Token Budget

When user specifies a token target (e.g., "+500k", "spend 2M tokens"), your output token count will be shown each turn. Keep working until you approach the target. The target is a hard minimum, not a suggestion."#;

const NUMERIC_LENGTH_ANCHORS: &str = r#"## Output Length

Keep text between tool calls to ≤25 words. Final responses to ≤100 words unless task requires more detail."#;

#[derive(Debug, Clone, Default)]
pub struct TokenBudgetSection {
    pub include_numeric_anchors: bool,
}

impl TokenBudgetSection {
    pub fn new() -> Self {
        Self {
            include_numeric_anchors: false,
        }
    }

    pub fn with_numeric_anchors(mut self, include: bool) -> Self {
        self.include_numeric_anchors = include;
        self
    }
}

impl PromptSection for TokenBudgetSection {
    fn name(&self) -> &str {
        "token_budget"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Volatile
    }

    fn build(&self, _ctx: &crate::prompt::PromptContext<'_>) -> Result<String> {
        if !self.include_numeric_anchors {
            return Ok(String::new());
        }
        Ok(format!(
            "{TOKEN_BUDGET_SECTION}\n\n{NUMERIC_LENGTH_ANCHORS}"
        ))
    }
}
