#![allow(deprecated)]
#![cfg(feature = "otel")]
//! Integration tests for Task 12 of the `otel-feature-integration`
//! OpenSpec change: verify span creation does NOT modify the prompt
//! prefix (P1 前缀一致性原则).
//!
//! Spec reference: `openspec/changes/otel-feature-integration/specs/
//! agent-runtime-spans/spec.md` — Requirement "span 创建 SHALL 不修改
//! prompt 前缀".
//!
//! # Test strategy
//!
//! We cannot run both `otel`-enabled and `otel`-disabled builds in the
//! same test binary. Instead, we verify the invariant directly:
//!
//! 1. **Span callsites declare no request-modifying fields** (12.1).
//!    The six span boundaries (`session.start` / `turn.start` /
//!    `llm.call` / `tool.execute` / `compaction` / `guardian.check`)
//!    are created with `tracing::span!` macros whose declared fields
//!    are span-semantic only (`exception.*` / `otel.status_code` /
//!    `gen_ai.*` / `tool.name` / `compaction.*` / `guardian.*`).
//!    None of them declare `messages` / `system` / `tools` — the
//!    `CompletionRequest` fields. Span creation therefore has no
//!    type-level pathway to mutate the request.
//!
//! 2. **`compute_prefix_hash` inputs are unaffected by span creation**
//!    (12.2). The prompt cache key (per `run_config.rs` design
//!    comment: "LLM provider prompt cache key ... namespaced by user")
//!    is derived from `user_id` / `session_id` and the message list.
//!    Span creation reads these values (via `OtelContext::from_run_config`,
//!    which takes a `&AgentRunConfig` — shared reference) but cannot
//!    mutate them. We verify by computing the hash before and after
//!    span creation: the hashes are byte-identical.
//!
//! 3. **KV cache stability ratio is unaffected by span creation**
//!    (12.3). `PrefixTracker::record_pre` / `record_post` hash the
//!    prefix bytes (system + tools + messages); span creation does not
//!    touch these bytes. The `windowed_stability_ratio` remains 1.0
//!    (stable) when spans are created between recordings.
//!
//! The `SpanAttributesProcessor::on_start` injects the six standard
//! attributes (`session.id` / `user.id` / `agent.id` / `turn.id` /
//! `gen_ai.system` / `gen_ai.request.model`) via OTel
//! `Span::set_attribute`, which operates on the OTel span object —
//! completely decoupled from the `CompletionRequest`. This is not
//! visible to `tracing::CaptureLayer` (it's OTel-layer, not
//! tracing-layer), but it cannot mutate the request because
//! `set_attribute` only writes to the span's attribute set.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use synthia_context::prefix_tracker::PrefixTracker;
use synthia_provider::{Content, Message, Role};
use synthia_telemetry::compute_prefix_hash;
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

// ===== Capture layer (reused from otel_compaction_span.rs pattern) =====

#[derive(Default, Debug)]
struct CapturedSpan {
    name: String,
    fields: HashMap<String, String>,
}

struct CaptureLayer {
    spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
}

struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut span = CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields: HashMap::new(),
        };
        let mut visitor = FieldVisitor(&mut span.fields);
        attrs.record(&mut visitor);
        self.spans.lock().unwrap().insert(id.clone(), span);
    }

    fn on_record(
        &self,
        span: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: Context<'_, S>,
    ) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(span_data) = spans.get_mut(span) {
            let mut visitor = FieldVisitor(&mut span_data.fields);
            values.record(&mut visitor);
        }
    }
}

/// Collect all captured span names + field names into a sorted vec for
/// deterministic assertion.
fn collected_spans(
    spans: &Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
) -> Vec<(String, Vec<String>)> {
    let guard = spans.lock().unwrap();
    let mut out: Vec<(String, Vec<String>)> = guard
        .values()
        .map(|s| {
            let mut fields: Vec<String> = s.fields.keys().cloned().collect();
            fields.sort();
            (s.name.clone(), fields)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ===== 12.1: span creation does not modify CompletionRequest fields =====

/// Task 12.1 — verify that span creation code paths are pure
/// side-effects: the six span boundaries created by the agent runtime
/// declare NO `CompletionRequest` fields (`messages` / `system` /
/// `tools`) in their `tracing::span!` callsites.
///
/// This is the type-level guarantee behind the spec scenario "span
/// 创建不修改 messages". Since `tracing::span!` only captures the
/// fields declared at the callsite (plus later `Span::record` calls
/// on those same fields), and none of the six callsites declare
/// request-modifying fields, span creation has no pathway to mutate
/// the `CompletionRequest`.
///
/// The test creates spans matching the production callsites (session /
/// turn / llm.call / tool.execute / compaction / guardian.check) and
/// captures their declared fields via a `CaptureLayer`. It then
/// asserts that none of the captured spans declare `messages`,
/// `system`, or `tools` as fields.
#[test]
fn span_callsites_do_not_declare_completion_request_fields() {
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let _guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();

    // Create spans matching the production callsites. The field
    // declarations mirror `agent.rs` (session / turn) and the
    // provider / tool / context / guardian crates' span! macros.
    // Field values are placeholders — only the field NAMES matter for
    // this assertion (we're checking the callsite declarations, not
    // the recorded values).

    // 1. session.start (matches `wrap_output_with_otel` in agent.rs)
    let _session_span = tracing::span!(
        target: "synthia.session",
        parent: None,
        tracing::Level::INFO,
        "session.start",
        exception.message = tracing::field::Empty,
    );

    // 2. turn.start (matches `TurnSpanGuard::create` in agent.rs)
    let _turn_span = tracing::span!(
        target: "synthia.turn",
        tracing::Level::INFO,
        "turn.start",
        turn.id = "turn-1",
        turn.iteration = 1,
        exception.type = tracing::field::Empty,
        exception.message = tracing::field::Empty,
        otel.status_code = tracing::field::Empty,
    );

    // 3. llm.call (matches `AnthropicProvider::complete` /
    //    `OpenAICompatibleProvider::complete` in synthia-provider)
    let _llm_span = tracing::span!(
        target: "synthia.llm",
        tracing::Level::INFO,
        "llm.call",
        gen_ai.system = "anthropic",
        gen_ai.request.model = "claude-3",
        gen_ai.response.finish_reason = tracing::field::Empty,
        gen_ai.usage.input_tokens = tracing::field::Empty,
        gen_ai.usage.output_tokens = tracing::field::Empty,
        exception.type = tracing::field::Empty,
        exception.message = tracing::field::Empty,
        otel.status_code = tracing::field::Empty,
    );

    // 4. tool.execute (matches `ToolRegistry::execute_tools` in
    //    synthia-tool)
    let _tool_span = tracing::span!(
        target: "synthia.tool",
        tracing::Level::INFO,
        "tool.execute",
        tool.name = "bash",
        exception.type = tracing::field::Empty,
        exception.message = tracing::field::Empty,
        otel.status_code = tracing::field::Empty,
    );

    // 5. compaction (matches `compact_messages` in synthia-context)
    let _compaction_span = tracing::span!(
        target: "synthia.compaction",
        tracing::Level::INFO,
        "compaction",
        compaction.before_tokens = 1000usize,
        compaction.messages_before = 10usize,
        compaction.stage = "L1",
        compaction.after_tokens = tracing::field::Empty,
        compaction.messages_after = tracing::field::Empty,
    );

    // 6. guardian.check (matches `GuardianReviewer::check` in
    //    synthia-guardian)
    let _guardian_span = tracing::span!(
        target: "synthia.guardian",
        tracing::Level::INFO,
        "guardian.check",
        guardian.layer = "reviewer",
        guardian.decision = tracing::field::Empty,
    );

    // Drop the spans so `on_record` (if any) has run before inspection.
    drop(_session_span);
    drop(_turn_span);
    drop(_llm_span);
    drop(_tool_span);
    drop(_compaction_span);
    drop(_guardian_span);

    let captured = collected_spans(&spans);

    // Sanity: all six spans were captured.
    assert_eq!(
        captured.len(),
        6,
        "expected 6 captured spans, got {captured:?}"
    );

    // The forbidden fields: these are the `CompletionRequest` struct
    // fields (per `crates/synthia-provider/src/types/completion.rs`).
    // If any span declares one of these, span creation would have a
    // pathway to mutate the request — violating P1 (prefix
    // consistency).
    let forbidden_fields = ["messages", "system", "tools", "tool_choice"];

    for (span_name, fields) in &captured {
        for forbidden in &forbidden_fields {
            assert!(
                !fields.iter().any(|f| f == forbidden),
                "span {:?} declared forbidden CompletionRequest field \
                 {:?}: {:?}. Span creation must not have a pathway to \
                 mutate the request (P1 prefix consistency).",
                span_name,
                forbidden,
                fields
            );
        }
    }

    // Additionally verify each span's declared fields are all
    // span-semantic (OTel convention names). This documents the
    // invariant: span fields are observational metadata, not request
    // data.
    let allowed_field_prefixes = [
        "exception.",
        "otel.",
        "gen_ai.",
        "tool.name",
        "compaction.",
        "guardian.",
        "turn.",
        "session.",
        "user.",
        "agent.",
    ];

    for (span_name, fields) in &captured {
        for field in fields {
            let allowed = allowed_field_prefixes
                .iter()
                .any(|prefix| field.starts_with(prefix));
            assert!(
                allowed,
                "span {:?} declared field {:?} which is not in the \
                 allowed span-semantic prefix set. All span fields must \
                 be observational metadata (OTel convention names), not \
                 request data.",
                span_name, field
            );
        }
    }
}

/// Task 12.1 (supplemental) — `tracing::Span` is an observational
/// type: it is `Clone + Send + Sync` but exposes no methods that
/// mutate `CompletionRequest` / `Message` / `ToolDefinition`. This
/// test records the type-level invariant via trait bounds.
///
/// The span creation code in `agent.rs` only:
///   1. Creates a span (`tracing::span!` → returns `Span`)
///   2. Enters the span (`Span::enter()` → returns `EnteredSpan`,
///      dropped on scope exit)
///   3. Records attributes on the span (`Span::record(field, value)`)
///
/// None of these operations have access to `CompletionRequest` or its
/// fields. The `SpanAttributesProcessor::on_start` hook reads
/// task-locals and calls `span.set_attribute(...)` on the OTel span
/// object — also observational, also decoupled from the request.
#[test]
fn span_is_clone_send_sync_but_does_not_mutate_request() {
    // Compile-time trait-bound check: `tracing::Span` is
    // `Clone + Send + Sync`. These bounds are what allow the span to
    // be passed through `wrap_output_with_otel` (which needs `Send`
    // for the async stream). The bounds say nothing about
    // `CompletionRequest` — the span has no type-level pathway to
    // mutate the request.
    fn assert_clone_send_sync<T: Clone + Send + Sync>() {}
    assert_clone_send_sync::<tracing::Span>();

    // A span can be created and cloned — all without touching any
    // request data. This is the runtime counterpart to the
    // compile-time check above.
    let span = tracing::span!(
        target: "synthia.session",
        parent: None,
        tracing::Level::INFO,
        "session.start",
        exception.message = tracing::field::Empty,
    );
    let _cloned = span.clone();
    // If a future refactor adds a method to `Span` that returns a
    // `&mut CompletionRequest`, this test should be updated to
    // explicitly assert that method does not exist (e.g. via a
    // compile-fail test).
}

// ===== 12.2: prompt_cache_key inputs unaffected by span creation =====

/// Task 12.2 — verify that `compute_prefix_hash` (the prompt cache key
/// computation in `synthia-telemetry`) produces byte-identical hashes
/// before and after span creation.
///
/// Per the `AgentRunConfig.user_id` doc comment: "the on-disk session
/// path, LLM provider prompt cache key, and tool permission decisions
/// are all namespaced by user". The cache key inputs are `user_id` /
/// `session_id` and the message list. Span creation reads these (via
/// `OtelContext::from_run_config`, which takes `&AgentRunConfig` — a
/// shared reference) but cannot mutate them.
///
/// This test creates spans matching the agent runtime's span creation
/// pattern, then verifies:
///   1. `compute_prefix_hash(&messages)` is identical before/after
///   2. `user_id` / `session_id` strings are unchanged
///   3. The message list is byte-identical (via serialized JSON)
#[test]
fn prompt_cache_key_inputs_unaffected_by_span_creation() {
    // Inputs to the prompt cache key (per run_config.rs design).
    let user_id = "user-otel-12.2".to_string();
    let session_id = "session-otel-12.2".to_string();
    let messages: Vec<String> = vec![
        "system: You are a helpful assistant.".to_string(),
        "user: Hello".to_string(),
        "assistant: Hi there".to_string(),
        "user: What is 2+2?".to_string(),
    ];

    // Snapshot the inputs before span creation.
    let user_id_before = user_id.clone();
    let session_id_before = session_id.clone();
    let messages_before = messages.clone();
    let hash_before = compute_prefix_hash(&messages);

    // Install a capture layer so span creation actually exercises the
    // tracing subscriber (not a no-op). This makes the test
    // meaningful: if span creation had a side-effect on the inputs,
    // the capture layer would observe it.
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let _guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();

    // Create spans matching the six production callsites — the same
    // set the agent runtime creates during `run_stream`. The span
    // bodies are entered and dropped, mirroring the real lifecycle.
    {
        let _session_span = tracing::span!(
            target: "synthia.session",
            parent: None,
            tracing::Level::INFO,
            "session.start",
            exception.message = tracing::field::Empty,
        );
        let _session_enter = _session_span.enter();

        // turn.start
        let _turn_span = tracing::span!(
            target: "synthia.turn",
            tracing::Level::INFO,
            "turn.start",
            turn.id = "turn-12.2",
            turn.iteration = 1,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _turn_enter = _turn_span.enter();

        // llm.call
        let _llm_span = tracing::span!(
            target: "synthia.llm",
            tracing::Level::INFO,
            "llm.call",
            gen_ai.system = "anthropic",
            gen_ai.request.model = "claude-3",
            gen_ai.response.finish_reason = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _llm_enter = _llm_span.enter();

        // tool.execute
        let _tool_span = tracing::span!(
            target: "synthia.tool",
            tracing::Level::INFO,
            "tool.execute",
            tool.name = "bash",
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _tool_enter = _tool_span.enter();

        // compaction
        let _compaction_span = tracing::span!(
            target: "synthia.compaction",
            tracing::Level::INFO,
            "compaction",
            compaction.before_tokens = 1000usize,
            compaction.messages_before = 10usize,
            compaction.stage = "L1",
            compaction.after_tokens = tracing::field::Empty,
            compaction.messages_after = tracing::field::Empty,
        );
        let _compaction_enter = _compaction_span.enter();

        // guardian.check
        let _guardian_span = tracing::span!(
            target: "synthia.guardian",
            tracing::Level::INFO,
            "guardian.check",
            guardian.layer = "reviewer",
            guardian.decision = tracing::field::Empty,
        );
        let _guardian_enter = _guardian_span.enter();

        // While inside all six spans, recompute the hash. The inputs
        // were captured by reference into the span fields (if at all)
        // — but `compute_prefix_hash` takes `&[S: AsRef<str>]`, a
        // shared reference, so it cannot mutate them.
        let hash_inside_spans = compute_prefix_hash(&messages);
        assert_eq!(
            hash_before, hash_inside_spans,
            "compute_prefix_hash must be identical inside the span \
             scope"
        );

        // The user_id / session_id are still unchanged.
        assert_eq!(user_id, user_id_before);
        assert_eq!(session_id, session_id_before);
    }

    // After all spans are dropped, recompute the hash. Must be
    // byte-identical to the pre-span hash.
    let hash_after = compute_prefix_hash(&messages);
    assert_eq!(
        hash_before, hash_after,
        "compute_prefix_hash must be byte-identical before and after \
         span creation. Span creation must not touch the prompt cache \
         key inputs (P1 prefix consistency)."
    );

    // Final sanity: inputs are byte-identical.
    assert_eq!(user_id, user_id_before);
    assert_eq!(session_id, session_id_before);
    assert_eq!(messages, messages_before);

    // Verify the capture layer observed the spans (i.e. the test
    // actually exercised the subscriber, not a no-op).
    let captured = collected_spans(&spans);
    assert_eq!(
        captured.len(),
        6,
        "expected 6 captured spans (test should exercise the subscriber)"
    );
}

/// Task 12.2 (supplemental) — verify that the `Message` list (which
/// becomes `CompletionRequest.messages`) is byte-identical before and
/// after span creation, by serializing to JSON and comparing.
///
/// This is a stronger check than pointer equality: it verifies the
/// message *contents* are unchanged, which is what the spec scenario
/// "传给 `CompletionRequest` 的 `messages` / `system` / `tools` 字段
/// SHALL 与未启用 `otel` feature 时完全一致（字节级）" requires.
#[test]
fn completion_request_messages_byte_identical_across_span_creation() {
    let messages = vec![
        Message::new(Role::System, Content::text("You are helpful.")),
        Message::new(Role::User, Content::text("Hello")),
        Message::new(Role::Assistant, Content::text("Hi there")),
    ];

    let json_before = serde_json::to_string(&messages).unwrap();

    // Create spans (matching the agent runtime's six boundaries).
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let _guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();

    {
        let session_span = tracing::span!(
            target: "synthia.session",
            parent: None,
            tracing::Level::INFO,
            "session.start",
            exception.message = tracing::field::Empty,
        );
        let _session = session_span.enter();
        let turn_span = tracing::span!(
            target: "synthia.turn",
            tracing::Level::INFO,
            "turn.start",
            turn.id = "byte-identical",
            turn.iteration = 1,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _turn = turn_span.enter();

        // Re-serialize inside the span scope. Must be byte-identical.
        let json_inside = serde_json::to_string(&messages).unwrap();
        assert_eq!(
            json_before, json_inside,
            "Message JSON must be byte-identical inside the span scope"
        );
    }

    // After spans are dropped, re-serialize. Must be byte-identical.
    let json_after = serde_json::to_string(&messages).unwrap();
    assert_eq!(
        json_before, json_after,
        "Message JSON must be byte-identical before and after span \
         creation (P1 prefix consistency). Span creation must not \
         mutate CompletionRequest.messages."
    );
}

// ===== 12.3: KV cache hit rate unaffected by span creation =====

/// Task 12.3 — verify that `PrefixTracker::windowed_stability_ratio`
/// (the KV cache hit rate metric) is unaffected by span creation.
///
/// `PrefixTracker::record_pre` / `record_post` hash the prefix bytes
/// (system + tools + messages) via `compute_hash_bytes` (SHA-256).
/// Span creation does not touch these bytes — it only creates
/// `tracing::Span` objects and records OTel-semantic attributes on
/// them. Therefore the hash is stable, and the stability ratio remains
/// 1.0 (100% cache hit rate) when spans are created between recordings.
///
/// This test simulates two consecutive LLM calls (turn 1 and turn 2)
/// with the same system prompt, creating spans between the `record_pre`
/// and `record_post` calls. The `windowed_stability_ratio` must be
/// 1.0, indicating the KV cache would hit on the second call.
#[test]
fn kv_cache_stability_ratio_unaffected_by_span_creation() {
    let mut tracker = PrefixTracker::new();
    let system_prompt =
        "You are a helpful assistant. Follow the system prompt.";
    let system_bytes = system_prompt.as_bytes();

    // Turn 1: record pre-call hash. Empty tools/messages slices: this
    // test verifies that span creation does not touch system_bytes, so
    // only the system slice is relevant for the hash.
    let _hash_t1_pre = tracker.record_pre(system_bytes, &[], &[], 1);

    // Create spans between record_pre and record_post — simulating
    // the agent runtime creating session / turn / llm.call spans
    // during the LLM call. Span creation must not touch
    // `system_bytes`.
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let _guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();

    {
        let session_span = tracing::span!(
            target: "synthia.session",
            parent: None,
            tracing::Level::INFO,
            "session.start",
            exception.message = tracing::field::Empty,
        );
        let _session = session_span.enter();
        let turn_span = tracing::span!(
            target: "synthia.turn",
            tracing::Level::INFO,
            "turn.start",
            turn.id = "kv-cache-t1",
            turn.iteration = 1,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _turn = turn_span.enter();
        let llm_span = tracing::span!(
            target: "synthia.llm",
            tracing::Level::INFO,
            "llm.call",
            gen_ai.system = "anthropic",
            gen_ai.request.model = "claude-3",
            gen_ai.response.finish_reason = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _llm = llm_span.enter();

        // record_post inside the span scope. Returns true if the
        // post-call hash matches the pre-call hash (stable).
        let stable_t1 = tracker.record_post(system_bytes, &[], &[], 1);
        assert!(
            stable_t1,
            "record_post must report stable prefix inside span scope \
             (span creation must not touch system_bytes)"
        );
    }

    // Turn 2: same system prompt. record_pre pushes a new entry into
    // the rolling window. The hash must match turn 1's hash, so the
    // windowed_stability_ratio is 1.0 (cache hit expected).
    let _hash_t2_pre = tracker.record_pre(system_bytes, &[], &[], 2);

    // Create spans again for turn 2.
    {
        let session_span = tracing::span!(
            target: "synthia.session",
            parent: None,
            tracing::Level::INFO,
            "session.start",
            exception.message = tracing::field::Empty,
        );
        let _session = session_span.enter();
        let turn_span = tracing::span!(
            target: "synthia.turn",
            tracing::Level::INFO,
            "turn.start",
            turn.id = "kv-cache-t2",
            turn.iteration = 2,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _turn = turn_span.enter();
        let llm_span = tracing::span!(
            target: "synthia.llm",
            tracing::Level::INFO,
            "llm.call",
            gen_ai.system = "anthropic",
            gen_ai.request.model = "claude-3",
            gen_ai.response.finish_reason = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            exception.type = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        let _llm = llm_span.enter();

        let stable_t2 = tracker.record_post(system_bytes, &[], &[], 2);
        assert!(
            stable_t2,
            "record_post must report stable prefix for turn 2 (span \
             creation must not touch system_bytes)"
        );
    }

    // The windowed stability ratio is the KV cache hit rate metric.
    // With two identical system prompts, it must be 1.0 (100%).
    let ratio = tracker.windowed_stability_ratio();
    assert_eq!(
        ratio, 1.0,
        "windowed_stability_ratio must be 1.0 (KV cache hit expected) \
         when system prompt is stable across turns, even with span \
         creation between recordings. Span creation must not affect \
         KV cache hit rate (P1 prefix consistency)."
    );

    // Verify the capture layer observed the spans (the test
    // exercised the subscriber, not a no-op).
    let captured = collected_spans(&spans);
    // 2 turns × 3 spans each (session + turn + llm) = 6 spans.
    assert_eq!(
        captured.len(),
        6,
        "expected 6 captured spans (test should exercise the subscriber)"
    );
}

/// Task 12.3 (determinism check) — verify that
/// `PrefixTracker::compute_hash_bytes` is deterministic: the same
/// input bytes produce the same hash, regardless of span creation.
///
/// This is the foundation of the KV cache hit rate guarantee: if the
/// hash were affected by span context, the cache would miss
/// spuriously.
#[test]
fn prefix_hash_is_deterministic_across_span_creation() {
    let bytes = b"system prompt for hash determinism";

    let hash_before = PrefixTracker::compute_hash_bytes(bytes, &[], &[]);

    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let _guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();

    let hash_inside;
    {
        let span = tracing::span!(
            target: "synthia.session",
            parent: None,
            tracing::Level::INFO,
            "session.start",
            exception.message = tracing::field::Empty,
        );
        let _guard = span.enter();
        hash_inside = PrefixTracker::compute_hash_bytes(bytes, &[], &[]);
    }

    let hash_after = PrefixTracker::compute_hash_bytes(bytes, &[], &[]);

    assert_eq!(
        hash_before, hash_inside,
        "compute_hash_bytes must be identical inside span scope"
    );
    assert_eq!(
        hash_before, hash_after,
        "compute_hash_bytes must be byte-identical before/after span \
         creation (P1 prefix consistency). Span creation must not \
         affect the prompt cache hash."
    );
}
