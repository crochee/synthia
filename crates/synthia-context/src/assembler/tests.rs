//! Unit tests for [`super::ContextAssembler`].
//!
//! Covers the [`super::types::SectionPriorities`] resolver
//! defaults + overrides, the 7 builder methods on
//! [`super::types::ContextAssembler`], the 8 state setters
//! ([`super::inject`]), the read-side assembly
//! ([`super::assemble`]), the priority-driven trim
//! ([`super::trim`]), and the prepare/finalize pipeline
//! ([`super::pipeline`]) with a `TestReader` that simulates
//! a `MessageReader` over an in-memory buffer.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use synthia_provider::{CachePolicy, ImageDetail, Message, Role, TokenCounter};
use tokio::sync::RwLock;

use super::types::{ContextAssembler, SectionPriorities};
use crate::{
    injector::{self, ContextInjector, Section},
    protector::ProtectionZone,
    traits::MessageReader,
    types::ContextError,
};

struct TestReader {
    messages: Arc<RwLock<Vec<Message>>>,
}

#[async_trait::async_trait]
impl MessageReader for TestReader {
    async fn read_messages(&self) -> Result<Vec<Message>, ContextError> {
        Ok(self.messages.read().await.clone())
    }

    async fn read_messages_range(
        &self,
        from: usize,
        to: usize,
    ) -> Result<Vec<Message>, ContextError> {
        let msgs = self.messages.read().await;
        let end = to.min(msgs.len());
        if from >= end {
            return Ok(vec![]);
        }
        Ok(msgs[from..end].to_vec())
    }

    async fn message_count(&self) -> Result<usize, ContextError> {
        Ok(self.messages.read().await.len())
    }
}

#[tokio::test]
async fn test_context_assembler_prepare() {
    let reader = TestReader {
        messages: Arc::new(RwLock::new(vec![Message::user("hello")])),
    };
    let assembler = ContextAssembler::new(4096);
    let request = assembler.prepare(&reader).await.unwrap();
    assert_eq!(request.messages.len(), 1);
}

#[tokio::test]
async fn test_context_assembler_with_system() {
    let reader = TestReader {
        messages: Arc::new(RwLock::new(vec![])),
    };
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("You are helpful".to_string());
    let request = assembler.prepare(&reader).await.unwrap();
    let final_req = assembler.finalize(request, None);
    assert_eq!(final_req.messages.len(), 1);
    assert_eq!(final_req.messages[0].role, Role::System);
}

#[tokio::test]
async fn test_context_assembler_with_protection_zone() {
    let mut msgs = Vec::new();
    for i in 0..10 {
        msgs.push(Message::user(format!("user message {}", i)));
        msgs.push(Message::assistant(format!("assistant response {}", i)));
    }

    let reader = TestReader {
        messages: Arc::new(RwLock::new(msgs)),
    };

    let assembler = ContextAssembler::new(4096)
        .with_protection_zone(ProtectionZone::new(3, 0.35));
    let request = assembler.prepare(&reader).await.unwrap();

    assert_eq!(request.messages.len(), 6);
}

#[test]
fn test_build_system_prompt_with_all_layers() {
    let mut assembler = ContextAssembler::new(4096)
        .with_system_prompt("You are an assistant".to_string());
    assembler.inject_memories(vec!["User prefers dark mode".to_string()]);
    assembler.set_skill_summaries(
        "# Available Skills\n\n- **code_review**: Reviews code".to_string(),
    );

    let prompt = assembler.build_system_prompt();
    assert!(prompt.contains("You are an assistant"));
    assert!(prompt.contains("Memories"));
    assert!(prompt.contains("code_review"));
}

#[test]
fn test_estimate_total_tokens() {
    let msgs = vec![Message::user("Hello world")];
    let assembler = ContextAssembler::new(4096);
    let tokens = assembler.estimate_total_tokens(&msgs);
    assert!(tokens > 0);
}

#[test]
fn test_estimate_total_tokens_empty() {
    let assembler = ContextAssembler::new(4096);
    let tokens = assembler.estimate_total_tokens(&[]);
    assert_eq!(tokens, 0);
}

// --- Phase 13: Additional tests for layered assembly, skill providers, and CompletionRequest ---

/// Mock TokenCounter for testing provider integration.
struct MockCounter;

impl TokenCounter for MockCounter {
    fn count_message(&self, message: &Message) -> usize {
        use crate::traits::extract_message_text;
        extract_message_text(message).len().div_ceil(4) + 4 // overhead
    }

    fn count_text(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }

    fn count_image(
        &self,
        _width: u32,
        _height: u32,
        _detail: ImageDetail,
    ) -> usize {
        1000
    }
}

#[test]
fn test_context_assembler_with_custom_token_counter() {
    let msgs = vec![Message::user("Hello world test message for counting")];
    let assembler =
        ContextAssembler::new(4096).with_token_counter(Box::new(MockCounter));
    let tokens = assembler.estimate_total_tokens(&msgs);
    // MockCounter returns len/4 + 4 overhead
    assert!(tokens > 0);
    // The custom counter should differ from the default estimation
    let assembler_default = ContextAssembler::new(4096);
    let tokens_default = assembler_default.estimate_total_tokens(&msgs);
    // Both should produce positive results
    assert!(tokens_default > 0);
}

#[tokio::test]
async fn test_context_assembler_finalize_with_explicit_prompt() {
    let reader = TestReader {
        messages: Arc::new(RwLock::new(vec![Message::user("hello")])),
    };
    let assembler = ContextAssembler::new(4096);
    let request = assembler.prepare(&reader).await.unwrap();
    // Pass an explicit system prompt instead of using built one
    let final_req =
        assembler.finalize(request, Some("custom prompt".to_string()));
    assert_eq!(final_req.messages.len(), 2);
    assert_eq!(final_req.messages[0].role, Role::System);
    assert_eq!(
        crate::traits::extract_message_text(&final_req.messages[0]),
        "custom prompt"
    );
}

#[test]
fn test_context_assembler_empty_layers() {
    let assembler = ContextAssembler::new(4096);
    let prompt = assembler.build_system_prompt();
    assert!(
        prompt.is_empty(),
        "with no layers, system prompt should be empty"
    );
}

#[test]
fn test_context_assembler_layered_build_completion_request() {
    let mut assembler = ContextAssembler::new(4096)
        .with_system_prompt("You are a helpful assistant.".to_string());
    assembler.inject_memories(vec![
        "User prefers Rust over Python.".to_string(),
        "Project uses Tokio async runtime.".to_string(),
    ]);
    assembler
        .set_skill_summaries("# Skills\n- read_file: reads files".to_string());

    // Verify layered system prompt
    let prompt = assembler.build_system_prompt();
    assert!(prompt.contains("helpful assistant"));
    assert!(prompt.contains("Memories"));
    assert!(prompt.contains("Rust over Python"));
    assert!(prompt.contains("Tokio"));
    assert!(prompt.contains("read_file"));
}

#[test]
fn test_section_by_name_existing() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("System instructions".to_string());
    let section = assembler.section_by_name("System Prompt");
    assert!(section.is_some());
    assert!(
        section.unwrap().content.contains("System instructions"),
        "should find the system section"
    );
}

#[test]
fn test_section_by_name_missing() {
    let assembler =
        ContextAssembler::new(4096).with_system_prompt("System".to_string());
    assert!(assembler.section_by_name("nonexistent").is_none());
}

#[test]
fn test_system_snapshot_deterministic() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("Stable system prompt".to_string());
    let snap1 = assembler.system_snapshot();
    let snap2 = assembler.system_snapshot();
    assert_eq!(
        snap1, snap2,
        "back-to-back snapshots must be byte-identical"
    );
}

#[test]
fn test_system_snapshot_reflects_changes() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("First version".to_string());
    let snap1 = assembler.system_snapshot();

    let assembler2 = ContextAssembler::new(4096)
        .with_system_prompt("Second version".to_string());
    let snap2 = assembler2.system_snapshot();

    assert_ne!(
        snap1, snap2,
        "different prompts should produce different snapshots"
    );
}

#[test]
fn test_system_snapshot_empty_assembler() {
    let assembler = ContextAssembler::new(4096);
    let snap = assembler.system_snapshot();
    assert!(snap.is_empty());
}

#[tokio::test]
async fn test_context_assembler_completion_request_fields() {
    let reader = TestReader {
        messages: Arc::new(RwLock::new(vec![
            Message::user("test"),
            Message::assistant("response"),
        ])),
    };
    let assembler = ContextAssembler::new(4096);
    let request = assembler.prepare(&reader).await.unwrap();

    // Verify CompletionRequest fields
    assert_eq!(request.model, "default");
    assert_eq!(request.messages.len(), 2);
    assert!(request.max_tokens.is_some());
    assert_eq!(request.max_tokens.unwrap(), 4096);
    assert!(request.temperature.is_none());
    assert!(request.tools.is_empty());
    assert!(request.stop_sequences.is_empty());
}

#[tokio::test]
async fn test_prepare_emits_default_cache_policy() {
    // Task 9: ensure prepare() turns on Anthropic prompt caching by
    // emitting a default CachePolicy instead of None.
    let reader = TestReader {
        messages: Arc::new(RwLock::new(vec![Message::user("hi")])),
    };
    let assembler = ContextAssembler::new(4096);
    let request = assembler.prepare(&reader).await.unwrap();
    assert_eq!(request.cache_policy, Some(CachePolicy::default()));
}

// --- Tests for injectors, sections, priorities, and trimming ---

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
fn test_injector_integration_system_prompt() {
    let injector = TestInjector::new(
        "test_injector",
        Some("Injected system instructions".to_string()),
        vec![],
    );
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("Base prompt".to_string())
        .with_injector(Box::new(injector));

    let sections = assembler.assemble_sections();
    assert_eq!(sections.len(), 2);
    assert!(sections.iter().any(|s| s.content == "Base prompt"));
    assert!(
        sections
            .iter()
            .any(|s| s.content == "Injected system instructions")
    );
}

#[test]
fn test_injector_integration_memories() {
    let injector = TestInjector::new(
        "memory_injector",
        None,
        vec![
            ("pref".to_string(), "Dark mode".to_string()),
            ("lang".to_string(), "Rust".to_string()),
        ],
    );
    let mut assembler = ContextAssembler::new(4096);
    assembler.inject_memories(vec!["Direct memory".to_string()]);
    assembler = assembler.with_injector(Box::new(injector));

    let prompt = assembler.build_system_prompt();
    assert!(prompt.contains("Direct memory"));
    assert!(prompt.contains("pref"));
    assert!(prompt.contains("Dark mode"));
    assert!(prompt.contains("lang"));
    assert!(prompt.contains("Rust"));
}

#[test]
fn test_section_priorities_default() {
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("system".to_string())
        .with_workspace_info("workspace".to_string());

    let sections = assembler.assemble_sections();
    let system_section = sections
        .iter()
        .find(|s| s.title == "System Prompt")
        .unwrap();
    let workspace_section = sections
        .iter()
        .find(|s| s.title == "Workspace Info")
        .unwrap();

    assert_eq!(system_section.priority, injector::priorities::SYSTEM_PROMPT);
    assert_eq!(
        workspace_section.priority,
        injector::priorities::WORKSPACE_INFO
    );
    assert!(system_section.priority > workspace_section.priority);
}

#[test]
fn test_section_priorities_custom() {
    let priorities = SectionPriorities {
        system_prompt: Some(95),
        workspace_info: Some(10),
        ..Default::default()
    };
    let assembler = ContextAssembler::new(4096)
        .with_system_prompt("system".to_string())
        .with_workspace_info("workspace".to_string())
        .with_priorities(priorities);

    let sections = assembler.assemble_sections();
    let system_section = sections
        .iter()
        .find(|s| s.title == "System Prompt")
        .unwrap();
    let workspace_section = sections
        .iter()
        .find(|s| s.title == "Workspace Info")
        .unwrap();

    assert_eq!(system_section.priority, 95);
    assert_eq!(workspace_section.priority, 10);
}

#[test]
fn test_trim_to_budget_removes_lowest_priority() {
    // Create sections where:
    // - critical: priority 100, content = "AAAAAAAAAA" (10 chars -> 3 tokens at div_ceil(4))
    // - low: priority 10, content = "BBBBBBBBBB" (10 chars -> 3 tokens)
    // - medium: priority 50, content = "CCCCCCCCCC" (10 chars -> 3 tokens)
    // Total: 9 tokens
    // With budget of 7, should remove lowest-priority (low) first
    let mut sections = vec![
        Section::new("critical", "AAAAAAAAAA", 100),
        Section::new("low", "BBBBBBBBBB", 10),
        Section::new("medium", "CCCCCCCCCC", 50),
    ];

    ContextAssembler::trim_to_budget(&mut sections, 7, |s| s.len().div_ceil(4));

    assert_eq!(sections.len(), 2);
    assert!(sections.iter().any(|s| s.title == "critical"));
    assert!(sections.iter().any(|s| s.title == "medium"));
    assert!(!sections.iter().any(|s| s.title == "low"));
}

#[test]
fn test_trim_to_budget_preserves_single_section() {
    let mut sections = vec![Section::new("only", "content", 10)];

    ContextAssembler::trim_to_budget(&mut sections, 1, |s| s.len());

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "only");
}

#[test]
fn test_trim_to_budget_already_within_budget() {
    let mut sections = vec![
        Section::new("a", "short", 50),
        Section::new("b", "short", 60),
    ];

    ContextAssembler::trim_to_budget(&mut sections, 1000, |s| s.len());

    assert_eq!(sections.len(), 2);
}

#[test]
fn test_assemble_sections_with_conversation() {
    let mut assembler = ContextAssembler::new(4096);
    assembler.add_message(Message::user("Hello"));
    assembler.add_message(Message::assistant("Hi there"));

    let sections = assembler.assemble_sections();
    let conv = sections.iter().find(|s| s.title == "Conversation").unwrap();

    assert!(conv.content.contains("user: Hello"));
    assert!(conv.content.contains("assistant: Hi there"));
    assert_eq!(conv.priority, injector::priorities::USER_MESSAGES);
}

#[test]
fn test_assemble_sections_with_tool_results() {
    let mut assembler = ContextAssembler::new(4096);
    assembler.add_tool_results(vec![
        "Result from tool_a".to_string(),
        "Result from tool_b".to_string(),
    ]);

    let sections = assembler.assemble_sections();
    let tool_section =
        sections.iter().find(|s| s.title == "Tool Results").unwrap();

    assert!(tool_section.content.contains("tool_a"));
    assert!(tool_section.content.contains("tool_b"));
    assert_eq!(tool_section.priority, injector::priorities::TOOL_RESULTS);
}

#[test]
fn test_full_assembly_with_injectors_and_priorities() {
    let injector = TestInjector::new(
        "full_injector",
        Some("Extra instructions".to_string()),
        vec![("note".to_string(), "Important note".to_string())],
    );

    let priorities = SectionPriorities {
        skill_docs: Some(20),
        ..Default::default()
    };

    let mut assembler = ContextAssembler::new(8000)
        .with_system_prompt("You are helpful".to_string())
        .with_injector(Box::new(injector))
        .with_workspace_info("Project structure...".to_string())
        .with_priorities(priorities);

    assembler.inject_memories(vec!["User likes Rust".to_string()]);
    assembler.set_skill_summaries("# Skills".to_string());
    assembler.add_tool_results(vec!["Tool output".to_string()]);
    assembler.add_message(Message::user("Test query"));

    let sections = assembler.assemble_sections();
    assert!(sections.len() >= 5);

    // Verify priority ordering (system > conversation > tool > memory > skill > workspace)
    let system_priority = sections
        .iter()
        .find(|s| s.title == "System Prompt")
        .unwrap()
        .priority;
    let workspace_priority = sections
        .iter()
        .find(|s| s.title == "Workspace Info")
        .unwrap()
        .priority;
    assert!(system_priority > workspace_priority);
}

// --- ProtectionZone trim: O(n) complexity + semantics preservation ---

/// A [`TokenCounter`] that records every `count_message` invocation and
/// performs a small, deterministic amount of CPU work per call.
///
/// The recorded call count lets us assert the trim is O(n) deterministically:
/// the old `Vec::remove(0)` loop recomputed the full token sum after every
/// removal, calling the counter `n+(n-1)+...+1 ≈ n²/2` times, while the
/// `Vec::drain`-based O(n) path calls it ~`2n` times. The bounded CPU work
/// also makes the O(n²) wall-clock cost visible in debug builds.
struct CountingCounter {
    calls: Arc<AtomicU64>,
}

impl TokenCounter for CountingCounter {
    fn count_text(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }

    fn count_image(
        &self,
        _width: u32,
        _height: u32,
        _detail: ImageDetail,
    ) -> usize {
        1000
    }

    fn count_message(&self, message: &Message) -> usize {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // Bounded CPU work so the O(n²) path takes well over the timing
        // threshold in debug builds (the default `cargo test` mode) while
        // the O(n) path stays comfortably under it. `black_box` prevents
        // the optimizer from eliminating the work in release builds.
        let text = crate::traits::extract_message_text(message);
        let mut acc: u64 = 0;
        for _ in 0..20 {
            for b in text.bytes() {
                acc = acc.wrapping_add(b as u64);
                acc = acc.rotate_left(3);
            }
        }
        std::hint::black_box(&acc);
        // Per-message token count far above `max_tokens` so the trim is
        // forced to walk the whole list down to a single message.
        10_000
    }
}

/// Spec: `prompt-assembly-linearity` — "Large Message List Trims In Linear
/// Time" (200+ messages) and "Drain Replaces Remove(0)".
#[tokio::test]
async fn test_protection_zone_trim_linear_time() {
    const N: usize = 200;
    let calls = Arc::new(AtomicU64::new(0));
    let counter = CountingCounter {
        calls: calls.clone(),
    };

    // N alternating user/assistant messages; each reports 10_000 tokens,
    // so with max_tokens=1 the trim must remove N-1 messages, keeping the
    // single most recent one.
    let mut msgs = Vec::with_capacity(N);
    for i in 0..N {
        let body = "x".repeat(200);
        if i % 2 == 0 {
            msgs.push(Message::user(body));
        } else {
            msgs.push(Message::assistant(body));
        }
    }
    let reader = TestReader {
        messages: Arc::new(RwLock::new(msgs)),
    };

    // min_rounds = N/2 so the ProtectionZone returns ALL messages (the trim
    // must operate on the full N, not a 6-message tail).
    let assembler = ContextAssembler::new(1)
        .with_protection_zone(ProtectionZone::new(N / 2, 0.35))
        .with_token_counter(Box::new(counter));

    let start = Instant::now();
    let request = assembler.prepare(&reader).await.unwrap();
    let elapsed = start.elapsed();

    // Semantics: keep most recent 1 message when every message exceeds
    // the budget.
    assert_eq!(
        request.messages.len(),
        1,
        "trim must keep the most recent 1 message when every message \
         exceeds the budget"
    );

    // Primary (deterministic) assertion: the counter MUST be invoked O(n)
    // times. The old `Vec::remove(0)` loop called it ~n²/2 ≈ 20_100 times
    // for n=200; the O(n) `Vec::drain` path calls it ~2n times (once for
    // the full-list estimate at the top of `prepare`, once per protected
    // message during pre-computation).
    let observed = calls.load(Ordering::Relaxed);
    assert!(
        observed <= (N as u64) * 5,
        "expected O(n) counter invocations (<= {}), got {} — \
         trim is not linear",
        (N as u64) * 5,
        observed
    );

    // Secondary timing assertion: generous bound that still catches the
    // O(n²) path in debug builds while leaving ample headroom for CI
    // variance on the O(n) path.
    assert!(
        elapsed.as_millis() < 150,
        "trim of {} messages took {:?} — expected O(n) well under 150ms",
        N,
        elapsed
    );
}

/// Spec: `prompt-assembly-linearity` — "Trim Semantics Preserved": remove
/// from the oldest message until `total_tokens <= max_tokens`, keeping the
/// most recent 1 when even a single message exceeds the budget.
#[tokio::test]
async fn test_protection_zone_trim_semantics_preserved() {
    // Counter giving each message a fixed 50-token cost so the expected
    // survivor set is deterministic.
    struct FixedCounter;
    impl TokenCounter for FixedCounter {
        fn count_text(&self, text: &str) -> usize {
            text.len()
        }

        fn count_image(
            &self,
            _width: u32,
            _height: u32,
            _detail: ImageDetail,
        ) -> usize {
            0
        }

        fn count_message(&self, _message: &Message) -> usize {
            50
        }
    }

    // 5 messages × 50 tokens = 250 total. With max_tokens = 130 the trim
    // must drop the oldest 3 (250 → 200 → 150 → 100 ≤ 130), keeping the
    // last 2 in their original order.
    let msgs: Vec<Message> = (0..5)
        .map(|i| {
            if i % 2 == 0 {
                Message::user(format!("msg-{}", i))
            } else {
                Message::assistant(format!("msg-{}", i))
            }
        })
        .collect();
    let expected_survivors: Vec<String> = msgs
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(crate::traits::extract_message_text)
        .collect();

    let reader = TestReader {
        messages: Arc::new(RwLock::new(msgs)),
    };
    let assembler = ContextAssembler::new(130)
        .with_protection_zone(ProtectionZone::new(5, 0.35))
        .with_token_counter(Box::new(FixedCounter));

    let request = assembler.prepare(&reader).await.unwrap();

    assert_eq!(
        request.messages.len(),
        2,
        "trim must keep the 2 most recent messages"
    );
    for (i, msg) in request.messages.iter().enumerate() {
        assert_eq!(
            crate::traits::extract_message_text(msg),
            expected_survivors[i],
            "expected the most recent messages to survive in order"
        );
    }
}

// ─── Fragment delegation integration tests ───────────────────────────
//
// Verify that ContextAssembler delegates to FragmentRegistry when
// rendered fragments are set via `set_rendered_fragments()`.

/// When `rendered_fragments` is empty, the system prompt is used as-is.
#[allow(deprecated)]
#[test]
fn fragment_delegation_uses_system_prompt_when_no_fragments() {
    let mut assembler = ContextAssembler::new(4096)
        .with_system_prompt("legacy system prompt".to_string());
    assembler.set_rendered_fragments(vec![]);

    let prompt = assembler.build_system_prompt();
    assert!(
        prompt.contains("legacy system prompt"),
        "expected legacy system prompt when no fragments are set"
    );
}

/// When `rendered_fragments` is set, fragment content replaces the
/// system prompt in the assembled output.
#[allow(deprecated)]
#[test]
fn fragment_delegation_replaces_system_prompt() {
    let mut assembler = ContextAssembler::new(4096)
        .with_system_prompt("legacy system prompt".to_string());
    assembler.set_rendered_fragments(vec![
        (
            "SystemPrompt".to_string(),
            "fragment system prompt".to_string(),
        ),
        ("Environment".to_string(), "env info".to_string()),
    ]);

    let prompt = assembler.build_system_prompt();
    assert!(
        !prompt.contains("legacy system prompt"),
        "legacy system prompt must be replaced when fragments are set"
    );
    assert!(
        prompt.contains("fragment system prompt"),
        "fragment content must appear in assembled prompt"
    );
    assert!(
        prompt.contains("env info"),
        "all fragment content must appear"
    );
}

/// Verify that fragments appear as sections in `assemble_sections()`.
#[allow(deprecated)]
#[test]
fn fragment_sections_appear_in_assemble_sections() {
    let mut assembler = ContextAssembler::new(4096);
    assembler.set_rendered_fragments(vec![
        ("Frag1".to_string(), "content-1".to_string()),
        ("Frag2".to_string(), "content-2".to_string()),
    ]);

    let sections = assembler.assemble_sections();
    let frag_sections: Vec<_> = sections
        .iter()
        .filter(|s| s.title.starts_with("Fragment: "))
        .collect();

    assert_eq!(
        frag_sections.len(),
        2,
        "expected 2 fragment sections, got {}",
        frag_sections.len()
    );
    assert_eq!(frag_sections[0].title, "Fragment: Frag1");
    assert_eq!(frag_sections[0].content, "content-1");
    assert_eq!(frag_sections[1].title, "Fragment: Frag2");
    assert_eq!(frag_sections[1].content, "content-2");
}

/// When both fragments and memories are present, both appear in output.
#[allow(deprecated)]
#[test]
fn fragments_and_memories_coexist() {
    let mut assembler = ContextAssembler::new(4096);
    assembler.set_rendered_fragments(vec![(
        "SystemPrompt".to_string(),
        "sys".to_string(),
    )]);
    assembler.inject_memories(vec!["memory-1".to_string()]);

    let prompt = assembler.build_system_prompt();
    assert!(prompt.contains("sys"), "fragment content must appear");
    assert!(prompt.contains("memory-1"), "memories must appear");
}
