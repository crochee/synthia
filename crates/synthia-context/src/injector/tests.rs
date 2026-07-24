use super::*;

#[test]
fn test_skill_injector_new() {
    let injector = SkillInjector::new();
    assert!(injector.is_empty());
    assert_eq!(injector.level1_count(), 0);
    assert_eq!(injector.level2_count(), 0);
}

#[test]
fn test_skill_injector_with_level0() {
    let injector = SkillInjector::new().with_level0("Skill list".to_string());
    assert!(!injector.is_empty());
    assert!(injector.build_injection().contains("Skill list"));
}

#[test]
fn test_skill_injector_add_level1() {
    let injector = SkillInjector::new().add_level1(
        "code_review".to_string(),
        "## Code Review\n\n...".to_string(),
    );
    assert_eq!(injector.level1_count(), 1);
    assert!(injector.build_injection().contains("Code Review"));
}

#[test]
fn test_skill_injector_add_level2() {
    let injector = SkillInjector::new().add_level2(
        "test_runner".to_string(),
        "## Snippets for test_runner".to_string(),
        vec!["snippet1".to_string(), "snippet2".to_string()],
    );
    assert_eq!(injector.level2_count(), 1);
    assert!(injector.build_injection().contains("Snippets"));
}

#[test]
fn test_skill_injector_estimate_tokens() {
    let injector = SkillInjector::new()
        .with_level0("x".repeat(100))
        .add_level1("skill".to_string(), "y".repeat(100));
    let tokens = injector.estimate_tokens();
    assert!(tokens > 0);
}

#[test]
fn test_skill_injector_context_injector_trait() {
    let injector = SkillInjector::new()
        .with_level0("Available skills: code_review".to_string())
        .add_level1(
            "code_review".to_string(),
            "Full code review content".to_string(),
        );

    assert_eq!(injector.name(), "skill_injector");
    let prompt = injector.inject_system_prompt();
    assert!(prompt.is_some());
    let content = prompt.unwrap();
    assert!(content.contains("Available skills"));
    assert!(content.contains("Full code review"));

    assert!(injector.inject_memories().is_empty());
}

#[test]
fn test_skill_injector_empty_returns_none() {
    let injector = SkillInjector::new();
    assert!(injector.inject_system_prompt().is_none());
}

struct TestInjector {
    name: String,
    system_prompt: Option<String>,
    memories: Vec<(String, String)>,
}

impl TestInjector {
    fn new(
        name: &str,
        system_prompt: Option<String>,
        memories: Vec<(String, String)>,
    ) -> Self {
        Self {
            name: name.to_string(),
            system_prompt,
            memories,
        }
    }
}

impl ContextInjector for TestInjector {
    fn name(&self) -> &str {
        &self.name
    }

    fn inject_system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    fn inject_memories(&self) -> Vec<(String, String)> {
        self.memories.clone()
    }
}

#[test]
fn test_section_creation() {
    let section = Section::new("Test", "Content", 50);
    assert_eq!(section.title, "Test");
    assert_eq!(section.content, "Content");
    assert_eq!(section.priority, 50);
}

#[test]
fn test_section_critical() {
    let section = Section::critical("Important", "Critical content");
    assert_eq!(section.priority, 100);
}

#[test]
fn test_section_token_count() {
    let section = Section::new("Test", "Hello world", 50);
    let count = section.token_count(|s| s.len().div_ceil(4));
    assert_eq!(count, 3);
}

#[test]
fn test_context_injector_injection() {
    let injector = TestInjector::new(
        "test",
        Some("Be helpful".to_string()),
        vec![("pref".to_string(), "Dark mode".to_string())],
    );

    assert_eq!(injector.name(), "test");
    assert_eq!(
        injector.inject_system_prompt(),
        Some("Be helpful".to_string())
    );
    let memories = injector.inject_memories();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0], ("pref".to_string(), "Dark mode".to_string()));
}

#[test]
fn test_context_injector_empty_injection() {
    let injector = TestInjector::new("empty", None, vec![]);

    assert_eq!(injector.name(), "empty");
    assert_eq!(injector.inject_system_prompt(), None);
    assert!(injector.inject_memories().is_empty());
}

#[test]
fn test_priority_constants() {
    assert_eq!(priorities::SYSTEM_PROMPT, 100);
    assert_eq!(priorities::USER_MESSAGES, 90);
    assert_eq!(priorities::TOOL_RESULTS, 70);
    assert_eq!(priorities::INJECTED_MEMORIES, 50);
    assert_eq!(priorities::SKILL_DOCS, 40);
    assert_eq!(priorities::WORKSPACE_INFO, 30);
}
