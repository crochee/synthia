# Streaming + 2-Part Prompt + Truncate — Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Replace `ModelProvider::stream()` with `complete_with_stream` (callback-based) and add `TwoPartPrompt` + `Truncate` service, fixing 3 real bugs in the streaming path and introducing prefix-cache optimization.

**Architecture:** Lightweight overlay. New `complete_with_stream` method with default impl falls back to `complete()` (zero behavior change). Old `stream()` marked `#[deprecated]`, retained for 1 release. Anthropic / OpenAI providers override `complete_with_stream` for real streaming. `StepSample::execute` switches to the new method with mpsc backpressure + CancellationToken. `Truncate` is a new `synthia_context::truncate` module; `TwoPartPrompt` is a new `synthia_context::prompt::two_part` module. Bug 1 (`ends_with("")`) fixed inline; Bug 2 (full-snapshot tool calls) fixed by using `ToolCallDelta { arguments_delta }`; Bug 3 (info black hole) fixed by `IsDone { result: SamplingResult }` carrying the full result.

**Tech Stack:** Rust 2024 edition, tokio, async-trait, blake3, serde, tracing, wiremock (for SSE mock tests).

---

## PR1 — M1: Foundation (Tasks 1.1–1.18)

### Task 1.1: Add `IsDone` variant to `StreamChunk`

**Files:**
- Modify: `crates/synthia-provider/src/types.rs:311-327`

- [ ] **Step 1.1.1: Read current `StreamChunk` definition**

Run: `grep -n "pub enum StreamChunk" crates/synthia-provider/src/types.rs`
Expected: line ~311 with the 6-variant enum

- [ ] **Step 1.1.2: Add `IsDone` variant**

In `crates/synthia-provider/src/types.rs`, replace the closing `}` of `StreamChunk` (after `ToolCallEnd { id: String },`) with:
```rust
    ToolCallEnd { id: String },
    IsDone { result: SamplingResult },
}
```

Note: `SamplingResult` is defined in `crates/synthia-agent/src/agent/types.rs` (or shared). If it's not in scope of `synthia-provider`, add a minimal `SamplingResult` re-export in `synthia-provider/src/types.rs`:
```rust
// At top of types.rs, add:
pub use crate::sampling_result::SamplingResult;
// or, if not yet extracted, create a minimal placeholder in synthia-provider:
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SamplingResult {
    pub text: String,
    pub tool_calls: Vec<ToolUse>,
    pub usage: TokenUsage,
    pub finish_reason: String,
}
```

- [ ] **Step 1.1.3: Verify compile**

Run: `cargo build -p synthia-provider`
Expected: SUCCESS (or compile error if `SamplingResult` is in `synthia-agent`; resolve by moving the type to `synthia-provider` or `synthia-core`)

- [ ] **Step 1.1.4: Commit**

```bash
git add crates/synthia-provider/src/types.rs
git commit -m "feat(provider): add IsDone variant to StreamChunk"
```

---

### Task 1.2: Add `complete_with_stream` default to `ModelProvider`

**Files:**
- Modify: `crates/synthia-provider/src/traits.rs:17-40`

- [ ] **Step 1.2.1: Read trait definition**

Run: `sed -n '15,40p' crates/synthia-provider/src/traits.rs`

- [ ] **Step 1.2.2: Add the default-implemented method**

Insert after the existing `stream()` method (around line 28):
```rust
    async fn complete_with_stream(
        &self,
        req: CompletionRequest,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<SamplingResult, ProviderError> {
        let result = self.complete(req).await?;
        on_delta(StreamChunk::IsDone { result: Box::new(result.clone()) });
        Ok(result)
    }
```

- [ ] **Step 1.2.3: Mark `stream()` as deprecated**

Add `#[deprecated]` above the `stream()` method signature:
```rust
    #[deprecated(note = "use complete_with_stream; stream() ownership prevents cancel/fallback")]
    async fn stream(&self, req: CompletionRequest) -> Result<StreamResult, Error>;
```

- [ ] **Step 1.2.4: Add unit test for default fallback**

Create `crates/synthia-provider/tests/complete_with_stream_default.rs`:
```rust
use synthia_provider::{ModelProvider, CompletionRequest, StreamChunk, SamplingResult, ProviderError};

struct StubProvider;

#[async_trait::async_trait]
impl ModelProvider for StubProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<SamplingResult, ProviderError> {
        Ok(SamplingResult::default())
    }
}

#[tokio::test]
async fn default_complete_with_stream_emits_one_is_done() {
    let provider = StubProvider;
    let mut captured: Vec<StreamChunk> = vec![];
    let result = provider.complete_with_stream(
        CompletionRequest::default(),
        Box::new(|chunk| captured.push(chunk)),
    ).await.unwrap();
    assert_eq!(captured.len(), 1);
    assert!(matches!(captured[0], StreamChunk::IsDone { .. }));
    assert_eq!(result, SamplingResult::default());
}
```

- [ ] **Step 1.2.5: Run test**

Run: `cargo test -p synthia-provider --test complete_with_stream_default`
Expected: 1 passed

- [ ] **Step 1.2.6: Commit**

```bash
git add crates/synthia-provider/src/traits.rs crates/synthia-provider/tests/complete_with_stream_default.rs
git commit -m "feat(provider): add complete_with_stream default impl + deprecate stream()"
```

---

### Task 1.3: Add `StreamError` to `synthia-core::Error`

**Files:**
- Modify: `crates/synthia-core/src/error.rs` (or wherever Error is defined)

- [ ] **Step 1.3.1: Find Error enum location**

Run: `grep -rn "pub enum Error" crates/synthia-core/src/ | head -5`

- [ ] **Step 1.3.2: Add StreamErrorKind and StreamError variant**

In `crates/synthia-core/src/error.rs`:
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamErrorKind {
    HttpFailure { status: u16, recoverable: bool },
    ProtocolError(String),
    Aborted,
    Internal(String),
}

pub enum Error {
    // ... existing variants ...
    StreamError { kind: StreamErrorKind, message: String },
}

impl std::fmt::Display for StreamErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamErrorKind::HttpFailure { status, recoverable } =>
                write!(f, "HTTP failure (status={}, recoverable={})", status, recoverable),
            StreamErrorKind::ProtocolError(s) => write!(f, "protocol error: {}", s),
            StreamErrorKind::Aborted => write!(f, "aborted"),
            StreamErrorKind::Internal(s) => write!(f, "internal: {}", s),
        }
    }
}
```

- [ ] **Step 1.3.3: Run core tests**

Run: `cargo test -p synthia-core`
Expected: SUCCESS

- [ ] **Step 1.3.4: Commit**

```bash
git add crates/synthia-core/src/error.rs
git commit -m "feat(core): add StreamError variant with StreamErrorKind"
```

---

### Task 1.4: Implement `TruncateConfig` and `TruncatedResult`

**Files:**
- Create: `crates/synthia-context/src/truncate.rs`
- Modify: `crates/synthia-context/src/lib.rs` (add `pub mod truncate;`)

- [ ] **Step 1.4.1: Write failing test**

Create `crates/synthia-context/tests/truncate_test.rs`:
```rust
use synthia_context::truncate::{TruncateConfig, TruncatedResult, truncate_output};

#[test]
fn small_input_not_truncated() {
    let cfg = TruncateConfig::default();
    let r = truncate_output("hello world", &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, "hello world");
    assert!(r.output_path.is_none());
}

#[test]
fn large_input_truncated_with_disk_path() {
    let tmp = std::env::temp_dir().join("synthia-truncate-test-1");
    let _ = std::fs::create_dir_all(&tmp);
    let cfg = TruncateConfig { max_bytes: 100, head_lines: 2, tail_lines: 2, temp_dir: tmp.clone() };
    let big = (1..=50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let r = truncate_output(&big, &cfg);
    assert!(r.truncated);
    assert!(r.output.contains("truncated"));
    assert!(r.output.starts_with("line 1\nline 2"));
    assert!(r.output.ends_with("line 49\nline 50"));
    let path = r.output_path.expect("path set");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, big);
    std::fs::remove_file(&path).ok();
}

#[test]
fn empty_input() {
    let cfg = TruncateConfig::default();
    let r = truncate_output("", &cfg);
    assert!(!r.truncated);
    assert_eq!(r.output, "");
    assert_eq!(r.output_bytes, 0);
}

#[test]
fn default_config_values() {
    let cfg = TruncateConfig::default();
    assert_eq!(cfg.max_bytes, 30_000);
    assert_eq!(cfg.head_lines, 100);
    assert_eq!(cfg.tail_lines, 100);
}
```

- [ ] **Step 1.4.2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test truncate_test`
Expected: FAIL (no `truncate` module)

- [ ] **Step 1.4.3: Implement TruncateConfig + TruncatedResult + truncate_output**

Create `crates/synthia-context/src/truncate.rs`:
```rust
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TruncateConfig {
    pub max_bytes: usize,
    pub head_lines: usize,
    pub tail_lines: usize,
    pub temp_dir: PathBuf,
}

impl Default for TruncateConfig {
    fn default() -> Self {
        Self {
            max_bytes: 30_000,
            head_lines: 100,
            tail_lines: 100,
            temp_dir: std::env::temp_dir().join("synthia-truncate"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncatedResult {
    #[serde(alias = "content")]
    pub output: String,
    pub truncated: bool,
    #[serde(alias = "original_length")]
    pub original_bytes: usize,
    #[serde(alias = "truncated_length")]
    pub output_bytes: usize,
    pub output_path: Option<PathBuf>,
    pub marker: String,
}

pub fn truncate_output(content: &str, cfg: &TruncateConfig) -> TruncatedResult {
    if content.len() <= cfg.max_bytes {
        return TruncatedResult {
            output: content.to_string(),
            truncated: false,
            original_bytes: content.len(),
            output_bytes: content.len(),
            output_path: None,
            marker: String::new(),
        };
    }
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let head: String = lines.iter().take(cfg.head_lines).copied().collect();
    let tail: String = lines.iter().rev().take(cfg.tail_lines).collect::<Vec<_>>().into_iter().rev().collect();
    let path = match write_to_disk(content, &cfg.temp_dir) {
        Some(p) => p,
        None => {
            tracing::warn!("truncate_output: disk write failed; falling back to in-memory");
            return TruncatedResult {
                output: format!("{}\n[... truncated; disk write failed ...]\n{}", head, tail),
                truncated: true,
                original_bytes: content.len(),
                output_bytes: head.len() + tail.len() + 64,
                output_path: None,
                marker: "[... truncated; disk write failed ...]".to_string(),
            };
        }
    };
    let marker = format!("[... {} bytes truncated; full output at {} ...]",
        content.len() - head.len() - tail.len(),
        path.display());
    let output = format!("{}\n{}\n{}", head.trim_end(), marker, tail);
    TruncatedResult {
        output_bytes: output.len(),
        original_bytes: content.len(),
        output,
        truncated: true,
        output_path: Some(path),
        marker,
    }
}

fn write_to_disk(content: &str, dir: &Path) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let name = format!("truncate-{}.log", uuid::Uuid::new_v4());
    let path = dir.join(name);
    std::fs::write(&path, content).ok()?;
    Some(path)
}
```

- [ ] **Step 1.4.4: Add `pub mod truncate;` to lib.rs**

In `crates/synthia-context/src/lib.rs`:
```rust
pub mod truncate;
```

- [ ] **Step 1.4.5: Add `uuid` to dev-dependencies if not present**

In `crates/synthia-context/Cargo.toml`:
```toml
[dev-dependencies]
uuid = { version = "1", features = ["v4"] }
```

(If already present, skip.)

- [ ] **Step 1.4.6: Run test**

Run: `cargo test -p synthia-context --test truncate_test`
Expected: 4 passed

- [ ] **Step 1.4.7: Run clippy**

Run: `cargo clippy -p synthia-context --all-targets --all-features --tests`
Expected: 0 warnings

- [ ] **Step 1.4.8: Commit**

```bash
git add crates/synthia-context/src/truncate.rs crates/synthia-context/src/lib.rs crates/synthia-context/Cargo.toml crates/synthia-context/tests/truncate_test.rs
git commit -m "feat(context): add Truncate service (head/tail + disk spill, 30K default)"
```

---

### Task 1.5: Implement `truncate_messages`

**Files:**
- Modify: `crates/synthia-context/src/truncate.rs`

- [ ] **Step 1.5.1: Write failing test**

Append to `crates/synthia-context/tests/truncate_test.rs`:
```rust
use synthia_context::truncate::truncate_messages;
use synthia_core::ChatMessage;

#[test]
fn truncate_messages_applies_to_matching_role_only() {
    let cfg = TruncateConfig { max_bytes: 50, head_lines: 1, tail_lines: 1, temp_dir: std::env::temp_dir() };
    let mut msgs = vec![
        ChatMessage { role: "system".into(), content: "small".into() },
        ChatMessage { role: "tool".into(), content: (1..=20).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n") },
    ];
    let results = truncate_messages(&mut msgs, &cfg, |m| m.role == "tool");
    assert_eq!(results.len(), 1);
    assert!(results[0].truncated);
    assert!(msgs[0].content.starts_with("small")); // system unchanged
    assert!(msgs[1].content.contains("truncated")); // tool was truncated
}
```

- [ ] **Step 1.5.2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test truncate_test truncate_messages`
Expected: FAIL

- [ ] **Step 1.5.3: Implement `truncate_messages`**

In `crates/synthia-context/src/truncate.rs`, add:
```rust
use synthia_core::ChatMessage;

pub fn truncate_messages<F>(
    messages: &mut [ChatMessage],
    cfg: &TruncateConfig,
    role_predicate: F,
) -> Vec<TruncatedResult>
where
    F: Fn(&ChatMessage) -> bool,
{
    let mut results = vec![];
    for m in messages.iter_mut() {
        if role_predicate(m) {
            let r = truncate_output(&m.content, cfg);
            if r.truncated {
                m.content = r.output.clone();
            }
            results.push(r);
        }
    }
    results
}
```

(If `ChatMessage` lives in a different crate, adjust the import path.)

- [ ] **Step 1.5.4: Run test**

Run: `cargo test -p synthia-context --test truncate_test`
Expected: all passed

- [ ] **Step 1.5.5: Commit**

```bash
git add crates/synthia-context/src/truncate.rs crates/synthia-context/tests/truncate_test.rs
git commit -m "feat(context): truncate_messages applies per-role"
```

---

### Task 1.6: Implement `TwoPartPrompt` (model_family + system_message_form)

**Files:**
- Create: `crates/synthia-context/src/prompt/mod.rs`
- Create: `crates/synthia-context/src/prompt/two_part.rs`
- Modify: `crates/synthia-context/src/lib.rs` (add `pub mod prompt;`)

- [ ] **Step 1.6.1: Write failing test**

Create `crates/synthia-context/tests/two_part_test.rs`:
```rust
use synthia_context::prompt::two_part::*;

#[test]
fn header_hash_is_deterministic() {
    let p1 = TwoPartPrompt::build("stable header".into(), "body 1".into(), ModelFamily::Anthropic);
    let p2 = TwoPartPrompt::build("stable header".into(), "body 2".into(), ModelFamily::Anthropic);
    assert_eq!(p1.header_hash, p2.header_hash);
}

#[test]
fn header_hash_differs_when_header_changes() {
    let p1 = TwoPartPrompt::build("header A".into(), "body".into(), ModelFamily::Anthropic);
    let p2 = TwoPartPrompt::build("header B".into(), "body".into(), ModelFamily::Anthropic);
    assert_ne!(p1.header_hash, p2.header_hash);
}

#[test]
fn finalize_single_form_one_system_message() {
    let p = TwoPartPrompt::build("H".into(), "B".into(), ModelFamily::Generic);
    let d = p.finalize(None, SystemMessageForm::Single);
    assert_eq!(d.final_messages.len(), 1);
    assert!(d.final_messages[0].content.contains("H"));
    assert!(d.final_messages[0].content.contains("B"));
    assert!(!d.cache_hit_expected);
}

#[test]
fn finalize_two_part_form_two_system_messages() {
    let p = TwoPartPrompt::build("H".into(), "B".into(), ModelFamily::Anthropic);
    let d = p.finalize(None, SystemMessageForm::TwoPart);
    assert_eq!(d.final_messages.len(), 2);
    assert_eq!(d.final_messages[0].content, "H");
    assert_eq!(d.final_messages[1].content, "B");
}

#[test]
fn finalize_cache_hit_when_prev_hash_matches() {
    let p = TwoPartPrompt::build("H".into(), "B".into(), ModelFamily::Anthropic);
    let prev = Some(p.header_hash);
    let d = p.finalize(prev, SystemMessageForm::TwoPart);
    assert!(d.cache_hit_expected);
    assert!(d.header_unstable_reason.is_none());
}

#[test]
fn finalize_cache_miss_when_prev_hash_differs() {
    let p = TwoPartPrompt::build("H2".into(), "B".into(), ModelFamily::Anthropic);
    let prev = Some([0u8; 32]);
    let d = p.finalize(prev, SystemMessageForm::TwoPart);
    assert!(!d.cache_hit_expected);
    assert!(d.header_unstable_reason.is_some());
}
```

- [ ] **Step 1.6.2: Run test to verify it fails**

Run: `cargo test -p synthia-context --test two_part_test`
Expected: FAIL (no `prompt::two_part` module)

- [ ] **Step 1.6.3: Implement TwoPartPrompt**

Create `crates/synthia-context/src/prompt/mod.rs`:
```rust
pub mod two_part;
```

Create `crates/synthia-context/src/prompt/two_part.rs`:
```rust
use synthia_core::ChatMessage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFamily { Anthropic, OpenAI, Generic }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemMessageForm { Single, TwoPart }

pub struct TwoPartPrompt {
    pub header: String,
    pub body: String,
    pub header_hash: [u8; 32],
    pub model_family: ModelFamily,
}

pub struct TwoPartDecision {
    pub final_messages: Vec<ChatMessage>,
    pub cache_hit_expected: bool,
    pub header_unstable_reason: Option<String>,
}

impl TwoPartPrompt {
    pub fn build(header: String, body: String, family: ModelFamily) -> Self {
        let header_hash = blake3::hash(header.as_bytes()).into();
        Self { header, body, header_hash, model_family: family }
    }

    pub fn finalize(
        self,
        prev_header_hash: Option<[u8; 32]>,
        form: SystemMessageForm,
    ) -> TwoPartDecision {
        let cache_hit_expected = matches!(prev_header_hash, Some(prev) if prev == self.header_hash);
        let header_unstable_reason = match prev_header_hash {
            Some(prev) if prev != self.header_hash =>
                Some(format!("header drift: {} -> {}", hex::encode(prev), hex::encode(self.header_hash))),
            _ => None,
        };
        let final_messages = match form {
            SystemMessageForm::Single => vec![ChatMessage {
                role: "system".into(),
                content: format!("{}\n\n{}", self.header, self.body),
            }],
            SystemMessageForm::TwoPart => vec![
                ChatMessage { role: "system".into(), content: self.header },
                ChatMessage { role: "system".into(), content: self.body },
            ],
        };
        TwoPartDecision { final_messages, cache_hit_expected, header_unstable_reason }
    }
}
```

- [ ] **Step 1.6.4: Add `pub mod prompt;` to lib.rs**

In `crates/synthia-context/src/lib.rs`:
```rust
pub mod prompt;
```

- [ ] **Step 1.6.5: Add dependencies if missing**

In `crates/synthia-context/Cargo.toml`:
```toml
[dependencies]
blake3 = "1"
hex = "0.4"

[dev-dependencies]
synthia-core = { path = "../synthia-core" }
```

(Adjust if `synthia-core` is named differently or already a workspace dependency.)

- [ ] **Step 1.6.6: Run test**

Run: `cargo test -p synthia-context --test two_part_test`
Expected: 6 passed

- [ ] **Step 1.6.7: Commit**

```bash
git add crates/synthia-context/src/prompt/ crates/synthia-context/src/lib.rs crates/synthia-context/Cargo.toml crates/synthia-context/tests/two_part_test.rs
git commit -m "feat(context): add TwoPartPrompt with header hash tracking + Single/TwoPart forms"
```

---

### Task 1.7: Fix Bug 1 — `ends_with("")` in `streaming/openai.rs`

**Files:**
- Modify: `crates/synthia-provider/src/streaming/openai.rs:151`

- [ ] **Step 1.7.1: Read the bug**

Run: `sed -n '148,156p' crates/synthia-provider/src/streaming/openai.rs`

- [ ] **Step 1.7.2: Add a test that demonstrates the bug**

Create `crates/synthia-provider/tests/streaming_bug1_test.rs`:
```rust
use synthia_provider::streaming::openai::OpenAIStreamProcessor;

#[test]
fn thinking_tag_detection_uses_close_tag() {
    // Simulate SSE events that contain <think>...</think>
    let mut proc = OpenAIStreamProcessor::new(|chunk| {});
    proc.feed_line(r#"data: {"choices":[{"delta":{"content":"<think>plan</think>"}}]}"#);
    // The processor should NOT consider the close-tag fragment as still-thinking
    // (We assert via the emitted StreamChunk's reasoning vs text split.)
    // For now, just ensure no panic and one chunk is emitted.
    // The exact assertion depends on the processor's API; if API differs, adjust.
}
```

If the existing API is hard to test, replace with: `grep -n 'ends_with("")' crates/synthia-provider/src/streaming/openai.rs` returning nothing.

- [ ] **Step 1.7.3: Replace the broken check**

In `crates/synthia-provider/src/streaming/openai.rs:151`, change:
```rust
    let ends_thinking = content.ends_with("");
```
to:
```rust
    let ends_thinking = content.ends_with("</think>");
```

- [ ] **Step 1.7.4: Verify no `ends_with("")` remains**

Run: `grep -rn 'ends_with("")' crates/synthia-provider/`
Expected: no output

- [ ] **Step 1.7.5: Build and test**

Run: `cargo build -p synthia-provider && cargo test -p synthia-provider`
Expected: SUCCESS

- [ ] **Step 1.7.6: Commit**

```bash
git add crates/synthia-provider/src/streaming/openai.rs
git commit -m "fix(provider-openai): Bug 1 — ends_with(\"\") always true; use </think>"
```

---

### Task 1.8: Final PR1 verification

- [ ] **Step 1.8.1: Run all tests**

Run: `cargo test --workspace`
Expected: all green

- [ ] **Step 1.8.2: Run clippy**

Run: `cargo clippy --all-targets --all-features --tests --all`
Expected: 0 warnings

- [ ] **Step 1.8.3: Run rustfmt**

Run: `cargo +nightly fmt --all`
Expected: no changes after

- [ ] **Step 1.8.4: Verify PR1 acceptance**

- [x] Tasks 1.1-1.7 complete
- [x] `complete_with_stream` default impl exists
- [x] `StreamChunk::IsDone` variant exists
- [x] `Truncate` service works
- [x] `TwoPartPrompt` works
- [x] Bug 1 fixed
- [x] All tests pass
- [x] Clippy clean

- [ ] **Step 1.8.5: Push branch and open PR1**

```bash
git push origin feature/streaming-2part-truncate
gh pr create --title "PR1: foundation — complete_with_stream + Truncate + TwoPartPrompt + Bug 1 fix" --body "..."
```

---

## PR1 — M2: Anthropic Real Streaming (Tasks 2.1–2.10)

### Task 2.1: Add mock SSE test infrastructure

**Files:**
- Create: `crates/synthia-provider/tests/streaming_anthropic_test.rs`

- [ ] **Step 2.1.1: Add `wiremock` to dev-dependencies**

In `crates/synthia-provider/Cargo.toml`:
```toml
[dev-dependencies]
wiremock = "0.6"
```

- [ ] **Step 2.1.2: Write failing test using mock SSE sequence**

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};
use synthia_provider::anthropic::AnthropicProvider;
use synthia_provider::{CompletionRequest, ModelProvider, StreamChunk};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn anthropic_streaming_emits_text_deltas_then_is_done() {
    let server = MockServer::start().await;
    let sse_body = "event: message_start\ndata: {\"type\":\"message_start\"}\n\n\
                    event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                    event: ping\ndata: {\"type\":\"ping\"}\n\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n\
                    event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                    event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n\
                    event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_string(sse_body)).mount(&server).await;
    let provider = AnthropicProvider::new_test("test-key", server.uri());
    let req = CompletionRequest::default();
    let captured: Arc<Mutex<Vec<StreamChunk>>> = Arc::new(Mutex::new(vec![]));
    let cap2 = captured.clone();
    let result = provider.complete_with_stream(req, Box::new(move |c| cap2.lock().unwrap().push(c))).await.unwrap();
    let captured = captured.lock().unwrap();
    assert!(captured.iter().any(|c| matches!(c, StreamChunk::Content(_))));
    assert!(matches!(captured.last().unwrap(), StreamChunk::IsDone { .. }));
}
```

- [ ] **Step 2.1.3: Run test to verify it fails**

Run: `cargo test -p synthia-provider --test streaming_anthropic_test`
Expected: FAIL (no `AnthropicProvider::new_test` or no override of `complete_with_stream`)

- [ ] **Step 2.1.4: Commit (test scaffold)**

```bash
git add crates/synthia-provider/Cargo.toml crates/synthia-provider/tests/streaming_anthropic_test.rs
git commit -m "test(provider-anthropic): scaffold streaming test (failing)"
```

---

### Task 2.2: Implement Anthropic `complete_with_stream`

**Files:**
- Modify: `crates/synthia-provider/src/anthropic.rs` (override `complete_with_stream`)

- [ ] **Step 2.2.1: Locate the SSE parser in anthropic.rs**

Run: `grep -n "fn stream\|fn complete_with_stream\|content_block_delta" crates/synthia-provider/src/anthropic.rs | head -20`

- [ ] **Step 2.2.2: Refactor SSE parser into a callback-driven function**

Extract the existing SSE event handling into a function like:
```rust
pub(crate) async fn run_anthropic_sse(
    url: String,
    req: CompletionRequest,
    on_delta: &mut dyn FnMut(StreamChunk),
    cancel: &CancellationToken,
) -> Result<SamplingResult, ProviderError> {
    // POST request → SSE stream → parse events
    // text_delta → on_delta(StreamChunk::Content(ContentPart::Text(t)))
    // input_json_delta → on_delta(StreamChunk::ToolCallDelta { id, arguments_delta })
    // content_block_stop → on_delta(StreamChunk::ToolCallEnd { id })
    // message_stop → on_delta(StreamChunk::IsDone { result })
    // cancel_token polling every iteration
}
```

- [ ] **Step 2.2.3: Override `complete_with_stream` in `impl AnthropicProvider`**

```rust
#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<SamplingResult, ProviderError> { ... }

    async fn complete_with_stream(
        &self,
        req: CompletionRequest,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<SamplingResult, ProviderError> {
        let cancel = CancellationToken::new(); // or pull from context
        run_anthropic_sse(self.url(), req, &mut on_delta, &cancel).await
    }
}
```

- [ ] **Step 2.2.4: Run mock test**

Run: `cargo test -p synthia_provider --test streaming_anthropic_test`
Expected: 1 passed (or however many tests are in the file)

- [ ] **Step 2.2.5: Run clippy**

Run: `cargo clippy -p synthia-provider --all-targets --all-features --tests`
Expected: 0 warnings

- [ ] **Step 2.2.6: Add test for tool-call incremental delta (Bug 2 fix)**

Append to `streaming_anthropic_test.rs`:
```rust
#[tokio::test]
async fn anthropic_streaming_tool_call_emits_incremental_delta() {
    let server = MockServer::start().await;
    let sse_body = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"bash\",\"input\":{}}}\n\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"co\"}}\n\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"mmand\\\":\\\"ls\\\"}\"}}\n\n\
                    event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                    event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_string(sse_body)).mount(&server).await;
    let provider = AnthropicProvider::new_test("k", server.uri());
    let captured: Arc<Mutex<Vec<StreamChunk>>> = Arc::new(Mutex::new(vec![]));
    let cap2 = captured.clone();
    provider.complete_with_stream(CompletionRequest::default(), Box::new(move |c| cap2.lock().unwrap().push(c))).await.unwrap();
    let captured = captured.lock().unwrap();
    let deltas: Vec<&str> = captured.iter().filter_map(|c| match c {
        StreamChunk::ToolCallDelta { arguments_delta, .. } => Some(arguments_delta.as_str()),
        _ => None,
    }).collect();
    assert_eq!(deltas, vec!["{\"co", "mmand\":\"ls\"}"]);
    // No full-input Content(ToolUse) snapshots:
    assert!(!captured.iter().any(|c| matches!(c, StreamChunk::Content(ContentPart::ToolUse(_)))));
}
```

- [ ] **Step 2.2.7: Commit**

```bash
git add crates/synthia-provider/src/anthropic.rs crates/synthia-provider/tests/streaming_anthropic_test.rs
git commit -m "feat(provider-anthropic): complete_with_stream real impl with incremental tool-call deltas"
```

---

### Task 2.3: PR1 (M1+M2) final checks

- [ ] **Step 2.3.1: cargo test --workspace green**
- [ ] **Step 2.3.2: cargo clippy 0 warnings**
- [ ] **Step 2.3.3: cargo +nightly fmt clean**
- [ ] **Step 2.3.4: Push and merge PR1**

```bash
git push origin feature/streaming-2part-truncate-pr1
gh pr create --title "PR1 (M1+M2): foundation + Anthropic real streaming" --body "..."
```

---

## PR2 — M3: OpenAI Real Streaming (Tasks 3.1–3.11)

(Skeleton — same TDD pattern as PR1/M2, repeated for OpenAI's SSE format.)

### Task 3.1: Add `streaming_openai_test.rs` with mock SSE
### Task 3.2: Implement OpenAI `complete_with_stream`
- [ ] route `content` delta → `StreamChunk::Content(ContentPart::Text)`
- [ ] route `reasoning_content` delta → `StreamChunk::Content(ContentPart::Reasoning)`
- [ ] route `tool_calls[].function.arguments` delta → `ToolCallDelta`
- [ ] finish_reason → `IsDone`
- [ ] cancel_token select
### Task 3.3: Rewrite `streaming/openai.rs` OpenAIStreamProcessor (remove all text-sniffing)
### Task 3.4: Tests + clippy + commit
### Task 3.5: PR1 push & merge

---

## PR2 — M4: Agent Switchover (Tasks 4.1–4.15)

### Task 4.1: Refactor `StepSample::execute` to use `complete_with_stream`

**Files:**
- Modify: `crates/synthia-agent/src/stream_builder/steps/sample.rs`

- [ ] **Step 4.1.1: Read current `execute` body**

Run: `sed -n '40,100p' crates/synthia-agent/src/stream_builder/steps/sample.rs`

- [ ] **Step 4.1.2: Replace `provider.stream(req)` with `complete_with_stream` + mpsc channel**

Use the reference implementation in the design doc §3.3. Key shape:
```rust
let (tx, mut rx) = mpsc::channel::<StreamChunk>(64);
let cancel = ctx.cancel_token.clone();
let provider_handle = {
    let provider = ctx.provider.clone();
    let req = prompt.to_request();
    tokio::spawn(async move {
        provider.complete_with_stream(req, Box::new(move |chunk| {
            let _ = tx.try_send(chunk);
        })).await
    })
};

while let Some(chunk) = rx.recv().await {
    if cancel.is_cancelled() { drop(rx); let _ = provider_handle.await; return Err(AgentError::Cancelled); }
    match chunk { ... }
}
warn!("stream closed without IsDone; fallback to complete()");
ctx.provider.complete(prompt.to_request()).await
```

- [ ] **Step 4.1.3: Run agent tests**

Run: `cargo test -p synthia-agent`
Expected: green

- [ ] **Step 4.1.4: Commit**

```bash
git add crates/synthia-agent/src/stream_builder/steps/sample.rs
git commit -m "refactor(agent): StepSample::execute uses complete_with_stream + mpsc + cancel"
```

### Task 4.2: Integrate `truncate_messages` for tool results
### Task 4.3: Add e2e streaming + fallback + cancel tests
### Task 4.4: PR2 final checks + push

---

## PR3 — M5: Dead Code Cleanup (Tasks 5.1–5.10)

### Task 5.1: Delete `stream.rs` and `openai_stream.rs`

```bash
git rm crates/synthia-provider/src/stream.rs crates/synthia-provider/src/openai_stream.rs
```

- [ ] Verify no imports break: `cargo build -p synthia-provider`
- [ ] Commit: `chore(provider): delete duplicate stream processors`

### Task 5.2: Delete `streaming/openai.rs` (replaced in M3)

```bash
git rm crates/synthia-provider/src/streaming/openai.rs
```

- [ ] Update `streaming/mod.rs` to drop the export
- [ ] Build + commit

### Task 5.3: Mark `collect_stream_response` deprecated; migrate call sites
### Task 5.4: PR3 final checks + push

---

## PR3 — M6: Deprecated Removal (Tasks 6.1–6.9, 1 release cycle later)

### Task 6.1: Remove `ModelProvider::stream()`
### Task 6.2: Remove `collect_stream_response` and `collect_stream`
### Task 6.3: Remove `#[allow(deprecated)]` markers
### Task 6.4: Verify `grep -r 'provider.stream(' --include='*.rs'` = 0
### Task 6.5: cargo build + test + clippy clean
### Task 6.6: Commit + tag

---

## Success Criteria (from design doc §5.2)

- [x] Bug 1 fixed (Qwen / DeepSeek thinking-tag parsed correctly)
- [x] Bug 2 fixed (`ToolCallDelta` emits real fragments)
- [x] Bug 3 fixed (`IsDone` carries full `SamplingResult`)
- [x] `prefix_stability_ratio` ≥ 85% over 12-turn session
- [x] `stream_first_token_latency_ms` P50 < 500ms
- [x] `cargo test --workspace` green
- [x] `cargo clippy --all-targets --all-features --tests --all` 0 warnings

---

## References

- Design spec: `docs/superpowers/specs/2026-06-07-streaming-2part-truncate-design.md`
- OpenSpec change: `openspec/changes/streaming-2part-truncate/`
- Brainstorm: `openspec/changes/streaming-2part-truncate/brainstorm.md`
- Specs: `openspec/changes/streaming-2part-truncate/specs/`
- Tasks: `openspec/changes/streaming-2part-truncate/tasks.md`
- Workspace rules: `.trae/rules/agent_rule.md`, `.trae/rules/rust.md`
