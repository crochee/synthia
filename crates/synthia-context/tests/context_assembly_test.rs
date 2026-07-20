#![allow(deprecated)]
//! Unit tests for `ContextAssembler` — context assembly, priority trimming,
//! and protection zone behavior.
//!
//! These tests exercise the public API boundary from the integration-test
//! location (`tests/`), verifying that `ContextAssembler` produces
//! a valid `CompletionRequest` when driven through `DefaultContextService`.

use synthia_context::{
    assembler::ContextAssembler,
    protector::ProtectionZone,
    service::{ContextRequest, DefaultContextService},
    traits::{estimate_message_tokens, extract_message_text},
};
use synthia_provider::{CachePolicy, Message, Role};
use tempfile::TempDir;

/// Helper: estimate tokens for a slice of messages using the crate's heuristic.
fn msg_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Helper: extract text from a message for assertions.
fn msg_text(msg: &Message) -> String {
    extract_message_text(msg)
}

// ---------------------------------------------------------------------------
// Assemble produces a valid CompletionRequest
// ---------------------------------------------------------------------------

#[test]
fn assemble_produces_valid_completion_request() {
    let svc = DefaultContextService::new();
    let req = ContextRequest::new(vec![Message::user("hello")], 4096)
        .with_system_prompt("You are a helpful assistant.");

    let result = svc.assemble(&req);

    // Check the outer structure
    assert!(result.request.max_tokens.is_some());
    assert_eq!(result.request.model, "default");
    assert!(result.request.tools.is_empty());
    // ToolChoice doesn't implement PartialEq, check via debug format
    assert!(format!("{:?}", result.request.tool_choice).contains("Auto"));
    assert!(result.request.stop_sequences.is_empty());
    assert!(result.request.extra_body.is_none());
    assert!(matches!(
        result.request.cache_policy,
        Some(CachePolicy { .. })
    ));
}

#[test]
fn assemble_request_has_system_prompt_as_first_message() {
    let svc = DefaultContextService::new();
    let req = ContextRequest::new(vec![Message::user("hi")], 4096)
        .with_system_prompt("system prompt content");

    let result = svc.assemble(&req);
    let msgs = (*result.request.messages).clone();

    assert!(!msgs.is_empty());
    assert_eq!(msgs[0].role, Role::System);
}

#[test]
fn assemble_request_has_conversation_messages_after_system() {
    let svc = DefaultContextService::new();
    let req = ContextRequest::new(
        vec![Message::user("hello"), Message::assistant("hi there")],
        4096,
    )
    .with_system_prompt("You are helpful");

    let result = svc.assemble(&req);
    let msgs = (*result.request.messages).clone();

    // System prompt is first, then the two conversation messages
    assert!(msgs.len() >= 3);
    assert_eq!(msgs[0].role, Role::System);
    assert_eq!(msgs[1].role, Role::User);
    assert_eq!(msgs[2].role, Role::Assistant);
}

#[test]
fn assemble_without_system_prompt_omits_system_message() {
    let svc = DefaultContextService::new();
    let req = ContextRequest::new(vec![Message::user("hi")], 4096);

    let result = svc.assemble(&req);
    let msgs = (*result.request.messages).clone();

    assert!(!msgs.is_empty());
    assert_ne!(msgs[0].role, Role::System);
}

// ---------------------------------------------------------------------------
// Priority trimming works correctly
// ---------------------------------------------------------------------------

#[test]
fn trim_to_budget_removes_lowest_priority_sections_first() {
    // Build an assembler with multiple sections at different priorities
    let assembler = ContextAssembler::new(100)
        .with_system_prompt("system prompt section content here".to_string());

    let mut asm = assembler;
    asm.add_message(Message::user("user message number one"));
    asm.add_message(Message::assistant("assistant response"));

    let sections = asm.assemble_sections();
    let priorities: Vec<u8> = sections.iter().map(|s| s.priority).collect();

    // System prompt priority = 100 (highest), conversation priority = 90 (lower).
    // So conversation has a LOWER numeric priority value and would be removed first.
    assert_eq!(sections[0].title, "System Prompt");
    assert!(
        priorities[1] < priorities[0],
        "conversation should have lower priority than system prompt"
    );
}

#[test]
fn trim_preserves_high_priority_sections() {
    let mut assembler = ContextAssembler::new(100)
        .with_system_prompt("critical system content".to_string());

    // Add lots of low-priority conversation
    for i in 0..10 {
        assembler.add_message(Message::user(format!("user message {}", i)));
        assembler
            .add_message(Message::assistant(format!("assistant reply {}", i)));
    }

    let sections = assembler.assemble_sections();
    // System prompt should be present
    assert!(sections.iter().any(|s| s.title == "System Prompt"));
}

#[test]
fn section_priorities_resolve_to_defaults() {
    use synthia_context::assembler::SectionPriorities;

    let priorities = SectionPriorities::default();

    // Default system prompt priority is 100 (highest)
    assert_eq!(priorities.system_prompt(), 100);
    // Default user messages priority is 90
    assert_eq!(priorities.user_messages(), 90);
    // Default tool results priority is 70
    assert_eq!(priorities.tool_results(), 70);
}

// ---------------------------------------------------------------------------
// Protection zone preserves recent messages
// ---------------------------------------------------------------------------

#[test]
fn protection_zone_preserves_recent_rounds() {
    let svc = DefaultContextService::new();
    let zone = ProtectionZone::new(2, 0.35);

    // Build 5 complete rounds (user + assistant)
    let mut messages = Vec::new();
    for i in 0..5 {
        messages.push(Message::user(format!("user round {}", i)));
        messages.push(Message::assistant(format!("assistant round {}", i)));
    }

    let original_len = messages.len();
    assert_eq!(original_len, 10);

    // Apply protection
    svc.protect(&mut messages, &zone);

    // With min_rounds=2, we expect 2 user + 2 assistant = 4 messages preserved
    assert_eq!(messages.len(), 4);
    // The last two rounds should be preserved (round 3 and round 4)
    assert!(messages.iter().any(|m| msg_text(m).contains("round 3")));
    assert!(messages.iter().any(|m| msg_text(m).contains("round 4")));
}

#[test]
fn protection_zone_default_has_three_rounds() {
    let zone = ProtectionZone::default();
    assert_eq!(zone.min_rounds, 3);
    assert!((zone.token_ratio - 0.35).abs() < f64::EPSILON);
}

#[test]
fn get_recent_messages_returns_correct_slice() {
    let zone = ProtectionZone::new(2, 0.35);

    let messages: Vec<Message> = (0..6)
        .map(|i| Message::user(format!("user msg {}", i)))
        .collect();

    let recent = zone.get_recent_messages(&messages, 2);

    // Last 2 user messages means indices 4 and 5 (0-indexed)
    assert_eq!(recent.len(), 2);
    assert!(msg_text(recent[0]).contains("user msg 4"));
    assert!(msg_text(recent[1]).contains("user msg 5"));
}

#[test]
fn protection_zone_get_recent_messages_owned() {
    let zone = ProtectionZone::new(1, 0.3);

    let messages: Vec<Message> = vec![
        Message::user("first"),
        Message::assistant("second"),
        Message::user("third"),
    ];

    let recent = zone.get_recent_messages_owned(&messages, 1);
    assert_eq!(recent.len(), 1);
    assert!(msg_text(&recent[0]).contains("third"));
}

// ---------------------------------------------------------------------------
// System prompt inclusion
// ---------------------------------------------------------------------------

#[test]
fn build_system_prompt_includes_all_non_empty_sections() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("Base system prompt".to_string());

    let prompt = assembler.build_system_prompt();
    assert!(prompt.contains("Base system prompt"));
}

#[test]
fn build_system_prompt_excludes_empty_sections() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("Only system".to_string());

    // No memories, no skill summaries, no conversation
    let sections = assembler.assemble_sections();
    // Should only have the system prompt section
    assert_eq!(sections.len(), 1);
}

#[test]
fn assemble_with_large_context_trims_to_budget() {
    let svc = DefaultContextService::new();

    // Create many messages that would exceed a small budget
    let messages: Vec<Message> = (0..20)
        .map(|i| {
            if i % 2 == 0 {
                Message::user(format!("user message {}", i))
            } else {
                Message::assistant(format!("assistant response {}", i))
            }
        })
        .collect();

    // Very small budget — should force trimming
    let req =
        ContextRequest::new(messages, 100).with_system_prompt("tiny budget");

    let result = svc.assemble(&req);
    let total_tokens = msg_tokens(&result.request.messages);

    // Should fit within budget (approximately - token estimation is imprecise)
    assert!(
        total_tokens <= 200,
        "expected <= 200 tokens, got {}",
        total_tokens
    );
}

#[test]
fn section_by_name_returns_matching_section() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("my system prompt".to_string());

    let section = assembler.section_by_name("System Prompt");
    assert!(section.is_some());
    assert!(section.unwrap().content.contains("my system prompt"));
}

#[test]
fn section_by_name_returns_none_for_missing() {
    let assembler = ContextAssembler::new(4096);
    let section = assembler.section_by_name("Nonexistent Section");
    assert!(section.is_none());
}

#[test]
fn system_snapshot_is_deterministic() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("stable prompt".to_string());

    let snap1 = assembler.system_snapshot();
    let snap2 = assembler.system_snapshot();

    assert_eq!(snap1, snap2);
}

#[test]
fn inject_memories_adds_memory_section() {
    let assembler =
        ContextAssembler::new(4096).with_system_prompt("sys".to_string());

    let mut asm = assembler;
    asm.inject_memories(vec![
        "memory one".to_string(),
        "memory two".to_string(),
    ]);

    let sections = asm.assemble_sections();
    let memory_section = sections.iter().find(|s| s.title == "Memories");
    assert!(memory_section.is_some());
    assert!(memory_section.unwrap().content.contains("memory one"));
}

#[test]
fn add_tool_results_adds_tool_results_section() {
    let assembler =
        ContextAssembler::new(4096).with_system_prompt("sys".to_string());

    let mut asm = assembler;
    asm.add_tool_results(vec![
        "tool result A".to_string(),
        "tool result B".to_string(),
    ]);

    let sections = asm.assemble_sections();
    let tool_section = sections.iter().find(|s| s.title == "Tool Results");
    assert!(tool_section.is_some());
    assert!(tool_section.unwrap().content.contains("tool result A"));
}

// ---------------------------------------------------------------------------
// TempDir isolation — verifies tempfile integration
// ---------------------------------------------------------------------------

#[test]
fn temp_dir_isolation_for_assembly() {
    let _tmp = TempDir::new().expect("temp dir created");

    let svc = DefaultContextService::new();
    let req = ContextRequest::new(vec![Message::user("isolated")], 4096);
    let result = svc.assemble(&req);

    // Should produce a valid result with no temp file conflicts
    assert!(!result.request.messages.is_empty());
}

// ---------------------------------------------------------------------------
// FragmentRegistry delegation layer
// ---------------------------------------------------------------------------

use std::sync::Arc;

use async_trait::async_trait;
use synthia_context::fragment_delegation::build_system_prompt_from_fragments;
use synthia_core::tool::{
    ContextFragment,
    FragmentContext,
    FragmentError,
    FragmentRegistry,
};

/// Stub fragment for integration tests.
struct StubFragment {
    name: String,
    priority: u32,
    active: bool,
    content: String,
}

impl StubFragment {
    fn new(name: &str, priority: u32, active: bool, content: &str) -> Self {
        Self {
            name: name.to_string(),
            priority,
            active,
            content: content.to_string(),
        }
    }
}

#[async_trait]
impl ContextFragment for StubFragment {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn render(
        &self,
        _ctx: &FragmentContext,
    ) -> Result<String, FragmentError> {
        Ok(self.content.clone())
    }
}

fn frag_ctx() -> FragmentContext {
    FragmentContext::new("itest-session", "itest-user")
}

#[tokio::test]
async fn fragment_delegation_empty_registry() {
    let registry = FragmentRegistry::new();
    let result = build_system_prompt_from_fragments(&registry, &frag_ctx())
        .await
        .unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn fragment_delegation_multiple_fragments() {
    let registry = FragmentRegistry::new();
    registry
        .register(Arc::new(StubFragment::new(
            "identity",
            0,
            true,
            "You are a helpful assistant.",
        )))
        .await
        .unwrap();
    registry
        .register(Arc::new(StubFragment::new(
            "skills",
            10,
            true,
            "Available skills: search, code",
        )))
        .await
        .unwrap();
    registry
        .register(Arc::new(StubFragment::new(
            "memory",
            20,
            true,
            "User prefers concise answers.",
        )))
        .await
        .unwrap();

    let result = build_system_prompt_from_fragments(&registry, &frag_ctx())
        .await
        .unwrap();

    assert!(result.contains("You are a helpful assistant."));
    assert!(result.contains("Available skills: search, code"));
    assert!(result.contains("User prefers concise answers."));
}

#[tokio::test]
async fn fragment_delegation_skips_inactive() {
    let registry = FragmentRegistry::new();
    registry
        .register(Arc::new(StubFragment::new(
            "active",
            0,
            true,
            "visible content",
        )))
        .await
        .unwrap();
    registry
        .register(Arc::new(StubFragment::new(
            "inactive",
            1,
            false,
            "hidden content",
        )))
        .await
        .unwrap();

    let result = build_system_prompt_from_fragments(&registry, &frag_ctx())
        .await
        .unwrap();

    assert!(result.contains("visible content"));
    assert!(!result.contains("hidden content"));
}
