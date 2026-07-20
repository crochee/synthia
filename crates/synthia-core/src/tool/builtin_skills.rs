//! Built-in skills — CodingSkill, SearchSkill, DebugSkill.
//!
//! These skills ship with synthia-core and provide baseline capabilities
//! for coding assistance, codebase search, and debugging.

use async_trait::async_trait;
use lazy_static::lazy_static;

use super::{
    skill_registry::{Skill, SkillProvenance},
    tool_name::ToolName,
};

// ── Keyword detection helper ────────────────────────────────────────────────

/// Detect whether the input text contains any of the given keywords.
///
/// Returns a confidence score in [0.0, 1.0]:
/// - 0.0 if the input is empty or no keywords match
/// - Linearly scaled by the ratio of matching keywords, capped at 1.0,
///   then mapped to [0.3, 1.0] so that even a single keyword hit yields
///   a reasonable confidence.
pub fn detect_invocation_keywords(input: &str, keywords: &[&str]) -> f64 {
    if input.is_empty() || keywords.is_empty() {
        return 0.0;
    }
    let lower = input.to_lowercase();
    let matched = keywords
        .iter()
        .filter(|kw| lower.contains(&kw.to_lowercase()))
        .count();
    if matched == 0 {
        return 0.0;
    }
    // ratio in (0.0, 1.0], then map to [0.3, 1.0]
    let ratio = matched as f64 / keywords.len() as f64;
    0.3 + 0.7 * ratio
}

// ── CodingSkill ─────────────────────────────────────────────────────────────

/// General coding assistance — reading, writing, and reasoning about code.
pub struct CodingSkill;

const CODING_KEYWORDS: &[&str] = &[
    "write",
    "implement",
    "fix",
    "refactor",
    "code",
    "function",
    "class",
    "module",
    "method",
    "variable",
    "type",
    "struct",
    "enum",
    "trait",
    "interface",
    "algorithm",
    "compile",
    "build",
    "syntax",
    "lint",
];

#[async_trait]
impl Skill for CodingSkill {
    fn name(&self) -> &str {
        "coding"
    }

    fn description(&self) -> &str {
        "General coding assistance — reading, writing, and reasoning about code"
    }

    fn instructions(&self) -> &str {
        concat!(
            "You are a coding assistant. Follow these principles:\n",
            "1. Read existing code before making changes to understand context.\n",
            "2. Make surgical, minimal changes — avoid unnecessary refactoring.\n",
            "3. Prefer editing over rewriting entire files.\n",
            "4. Match the existing code style and conventions.\n",
            "5. Ensure your changes compile and pass existing tests.\n",
            "6. Clean up only the dead code your changes create.\n",
            "7. Write clear, self-documenting code with appropriate comments.\n",
            "8. Handle errors explicitly; do not silently swallow them.\n",
            "9. Consider edge cases and boundary conditions.\n",
            "10. When in doubt, ask for clarification rather than guessing.",
        )
    }

    fn tools(&self) -> Vec<ToolName> {
        vec![
            ToolName::plain("read_file"),
            ToolName::plain("write_file"),
            ToolName::plain("edit_file"),
            ToolName::plain("bash"),
            ToolName::plain("search"),
        ]
    }

    fn provenance(&self) -> &SkillProvenance {
        static PROVENANCE: SkillProvenance = SkillProvenance::Core;
        &PROVENANCE
    }

    async fn detect_invocation(&self, user_input: &str) -> f64 {
        let base = detect_invocation_keywords(user_input, CODING_KEYWORDS);
        if base > 0.0 {
            // At least one keyword matched — boost to >= 0.8
            0.8_f64.max(base)
        } else if user_input.is_empty() {
            0.0
        } else {
            // Generic non-empty input gets a low baseline
            0.5
        }
    }
}

// ── SearchSkill ─────────────────────────────────────────────────────────────

/// Search and explore the codebase — finding files, symbols, and patterns.
pub struct SearchSkill;

const SEARCH_KEYWORDS: &[&str] = &[
    "find",
    "search",
    "where",
    "locate",
    "grep",
    "which file",
    "look for",
    "explore",
    "navigate",
    "browse",
    "show me",
    "list",
    "directory",
    "path",
];

#[async_trait]
impl Skill for SearchSkill {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search and explore the codebase — finding files, symbols, and patterns"
    }

    fn instructions(&self) -> &str {
        concat!(
            "You are a codebase search assistant. Follow these strategies:\n",
            "1. Start broad (grep for identifiers) then narrow down to specific files.\n",
            "2. Use `search` for semantic queries and `grep` for exact pattern matching.\n",
            "3. When looking for a definition, search for `fn name`, `struct Name`, `impl Name`.\n",
            "4. When looking for usages, search for the identifier directly.\n",
            "5. Use `find` to locate files by name or extension.\n",
            "6. Read matching files to understand context before reporting results.\n",
            "7. Report file paths and line numbers so the user can navigate.\n",
            "8. If no results are found, suggest alternative search terms or patterns.",
        )
    }

    fn tools(&self) -> Vec<ToolName> {
        vec![
            ToolName::plain("search"),
            ToolName::plain("grep"),
            ToolName::plain("find"),
            ToolName::plain("read_file"),
        ]
    }

    fn provenance(&self) -> &SkillProvenance {
        static PROVENANCE: SkillProvenance = SkillProvenance::Core;
        &PROVENANCE
    }

    async fn detect_invocation(&self, user_input: &str) -> f64 {
        let base = detect_invocation_keywords(user_input, SEARCH_KEYWORDS);
        if base > 0.0 {
            0.9_f64.max(base)
        } else if user_input.is_empty() {
            0.0
        } else {
            0.3
        }
    }
}

// ── DebugSkill ──────────────────────────────────────────────────────────────

/// Debug and diagnose errors — trace failures, analyze logs, and fix issues.
pub struct DebugSkill;

const DEBUG_KEYWORDS: &[&str] = &[
    "error",
    "bug",
    "crash",
    "fail",
    "exception",
    "stack trace",
    "debug",
    "diagnose",
    "trace",
    "panic",
    "segfault",
    "assertion",
    "timeout",
    "deadlock",
    "memory leak",
    "undefined",
    "null pointer",
    "regression",
    "log",
];

#[async_trait]
impl Skill for DebugSkill {
    fn name(&self) -> &str {
        "debug"
    }

    fn description(&self) -> &str {
        "Debug and diagnose errors — trace failures, analyze logs, and fix issues"
    }

    fn instructions(&self) -> &str {
        concat!(
            "You are a debugging assistant. Follow these strategies:\n",
            "1. Reproduce the issue first — a reproducible bug is a fixable bug.\n",
            "2. Read the error message and stack trace carefully before acting.\n",
            "3. Identify the failing component and trace the call chain.\n",
            "4. Check logs for warnings or errors preceding the failure.\n",
            "5. Form a hypothesis before making changes — avoid trial-and-error.\n",
            "6. Make minimal, targeted changes to fix the root cause.\n",
            "7. Verify the fix resolves the original issue and does not introduce regressions.\n",
            "8. Use `bash` to run the program with debug flags or verbose logging.\n",
            "9. Search the codebase for similar patterns that might have the same bug.\n",
            "10. Document the root cause and fix for future reference.",
        )
    }

    fn tools(&self) -> Vec<ToolName> {
        vec![
            ToolName::plain("bash"),
            ToolName::plain("read_file"),
            ToolName::plain("search"),
            ToolName::plain("grep"),
        ]
    }

    fn provenance(&self) -> &SkillProvenance {
        static PROVENANCE: SkillProvenance = SkillProvenance::Core;
        &PROVENANCE
    }

    async fn detect_invocation(&self, user_input: &str) -> f64 {
        let base = detect_invocation_keywords(user_input, DEBUG_KEYWORDS);
        if base > 0.0 {
            0.9_f64.max(base)
        } else if user_input.is_empty() {
            0.0
        } else {
            0.3
        }
    }
}

// ── Built-in skills registry ────────────────────────────────────────────────

lazy_static! {
    /// All built-in skills, lazily initialized.
    pub static ref BUILTIN_SKILLS: Vec<&'static dyn Skill> = vec![
        &CodingSkill as &dyn Skill,
        &SearchSkill as &dyn Skill,
        &DebugSkill as &dyn Skill,
    ];
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CodingSkill tests ───────────────────────────────────────────────

    #[test]
    fn coding_skill_name() {
        let skill = CodingSkill;
        assert_eq!(skill.name(), "coding");
    }

    #[test]
    fn coding_skill_description() {
        let skill = CodingSkill;
        assert_eq!(
            skill.description(),
            "General coding assistance — reading, writing, and reasoning about code"
        );
    }

    #[test]
    fn coding_skill_tools() {
        let skill = CodingSkill;
        let tools = skill.tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            vec!["read_file", "write_file", "edit_file", "bash", "search"]
        );
    }

    #[tokio::test]
    async fn coding_skill_detect_keyword() {
        let skill = CodingSkill;
        let confidence =
            skill.detect_invocation("implement a new function").await;
        assert!(confidence >= 0.8, "expected >= 0.8, got {confidence}");
    }

    #[tokio::test]
    async fn coding_skill_detect_empty() {
        let skill = CodingSkill;
        let confidence = skill.detect_invocation("").await;
        assert_eq!(confidence, 0.0);
    }

    #[tokio::test]
    async fn coding_skill_detect_generic() {
        let skill = CodingSkill;
        let confidence = skill.detect_invocation("hello world").await;
        assert_eq!(confidence, 0.5);
    }

    // ── SearchSkill tests ───────────────────────────────────────────────

    #[test]
    fn search_skill_name() {
        let skill = SearchSkill;
        assert_eq!(skill.name(), "search");
    }

    #[test]
    fn search_skill_tools() {
        let skill = SearchSkill;
        let tools = skill.tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["search", "grep", "find", "read_file"]);
    }

    #[tokio::test]
    async fn search_skill_detect_keyword() {
        let skill = SearchSkill;
        let confidence = skill.detect_invocation("find the file where").await;
        assert!(confidence >= 0.9, "expected >= 0.9, got {confidence}");
    }

    #[tokio::test]
    async fn search_skill_detect_empty() {
        let skill = SearchSkill;
        let confidence = skill.detect_invocation("").await;
        assert_eq!(confidence, 0.0);
    }

    // ── DebugSkill tests ────────────────────────────────────────────────

    #[test]
    fn debug_skill_name() {
        let skill = DebugSkill;
        assert_eq!(skill.name(), "debug");
    }

    #[test]
    fn debug_skill_tools() {
        let skill = DebugSkill;
        let tools = skill.tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["bash", "read_file", "search", "grep"]);
    }

    #[tokio::test]
    async fn debug_skill_detect_keyword() {
        let skill = DebugSkill;
        let confidence =
            skill.detect_invocation("there is a bug in the code").await;
        assert!(confidence >= 0.9, "expected >= 0.9, got {confidence}");
    }

    #[tokio::test]
    async fn debug_skill_detect_empty() {
        let skill = DebugSkill;
        let confidence = skill.detect_invocation("").await;
        assert_eq!(confidence, 0.0);
    }

    // ── Provenance tests ────────────────────────────────────────────────

    #[test]
    fn all_skills_have_core_provenance() {
        assert_eq!(CodingSkill.provenance(), &SkillProvenance::Core);
        assert_eq!(SearchSkill.provenance(), &SkillProvenance::Core);
        assert_eq!(DebugSkill.provenance(), &SkillProvenance::Core);
    }

    // ── Instructions tests ──────────────────────────────────────────────

    #[test]
    fn all_skills_have_non_empty_instructions() {
        assert!(!CodingSkill.instructions().is_empty());
        assert!(!SearchSkill.instructions().is_empty());
        assert!(!DebugSkill.instructions().is_empty());
    }

    // ── detect_invocation_keywords helper tests ─────────────────────────

    #[test]
    fn keyword_helper_empty_input() {
        assert_eq!(detect_invocation_keywords("", &["foo"]), 0.0);
    }

    #[test]
    fn keyword_helper_empty_keywords() {
        assert_eq!(detect_invocation_keywords("hello", &[] as &[&str]), 0.0);
    }

    #[test]
    fn keyword_helper_single_match() {
        let score = detect_invocation_keywords(
            "fix the bug",
            &["fix", "error", "crash"],
        );
        // matched 1/3, ratio = 0.333, score = 0.3 + 0.7*0.333 ≈ 0.533
        assert!(score > 0.3 && score < 1.0, "score = {score}");
    }

    #[test]
    fn keyword_helper_all_match() {
        let score = detect_invocation_keywords(
            "fix error crash",
            &["fix", "error", "crash"],
        );
        // matched 3/3, ratio = 1.0, score = 0.3 + 0.7*1.0 = 1.0
        assert!((score - 1.0).abs() < f64::EPSILON, "score = {score}");
    }

    #[test]
    fn keyword_helper_no_match() {
        let score =
            detect_invocation_keywords("hello world", &["fix", "error"]);
        assert_eq!(score, 0.0);
    }

    // ── BUILTIN_SKILLS tests ────────────────────────────────────────────

    #[test]
    fn builtin_skills_contains_all_three() {
        assert_eq!(BUILTIN_SKILLS.len(), 3);
        let names: Vec<&str> =
            BUILTIN_SKILLS.iter().map(|s| s.name()).collect();
        assert!(names.contains(&"coding"));
        assert!(names.contains(&"search"));
        assert!(names.contains(&"debug"));
    }

    // ── Low confidence for unrelated input ──────────────────────────────

    #[tokio::test]
    async fn search_skill_low_confidence_unrelated() {
        let skill = SearchSkill;
        let confidence = skill.detect_invocation("write a poem").await;
        assert_eq!(confidence, 0.3);
    }

    #[tokio::test]
    async fn debug_skill_low_confidence_unrelated() {
        let skill = DebugSkill;
        let confidence = skill.detect_invocation("write a poem").await;
        assert_eq!(confidence, 0.3);
    }
}
