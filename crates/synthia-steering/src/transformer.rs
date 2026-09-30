//! Built-in [`OutputTransformer`] implementations, adopted from
//! traitclaw's `traitclaw-core/src/transformers.rs`.
//!
//! # Composition
//!
//! [`TransformerChain`] pipes an output through several
//! transformers in order; [`JsonExtractor`] keeps only the JSON
//! payload of a verbose result; [`BudgetAwareTruncator`] cuts an
//! oversized result down to a head/tail excerpt and stashes the
//! full text in a
//! [`FullOutputStore`], so the model
//! can pull it back on demand through the `__get_full_output` tool
//! (`synthia_tool::RetrieveFullOutputTool`).
//!
//! # Divergence from traitclaw
//!
//! traitclaw splits this work across two types: `ProgressiveTransformer`
//! (an LLM summarises the output, the full text is cached under the
//! tool name) and `BudgetAwareTruncator` (char-count truncation with a
//! halved limit under context pressure). Synthia keeps one:
//! [`BudgetAwareTruncator`], the deterministic half. The divergences
//! and their reasons:
//!
//! - **Excerpt instead of summary.** The commit path must not depend
//!   on a provider call: a summary can fail, cost a round trip per
//!   oversized output, and rewrite data the model may need verbatim.
//!   Head + tail + a handle to the full text is free, cannot fail,
//!   and is the same two-phase shape (elide now, retrieve on demand).
//! - **Handle keyed, not tool-name keyed.** traitclaw cached the full
//!   output under `tool_name`, so a second oversized call from the
//!   same tool silently replaced the first. Handles are minted per
//!   `put`, so every stashed output stays addressable.
//! - **Per-call adaptive halving.** [`OutputTransformer::transform`]
//!   receives no tracker — only the per-call
//!   [`AgentState`] snapshot — so the effective budget is recomputed
//!   on every call from `AgentState::context_utilization()`. That is
//!   the same signal and the same rule traitclaw applied
//!   (`> aggressive_threshold` ⇒ halve), just read from the snapshot
//!   instead of a long-lived tracker.
//! - **Characters, not bytes.** Every length here is a Unicode
//!   scalar-value count (`char`), matching the `budget_chars` name.
//!   traitclaw compared `String::len` (bytes) against a char-based
//!   limit, so multibyte output could overrun the advertised budget.
//!
//! The marker format is a wire contract shared with the tool crate
//! (which cannot be depended on from here):
//! `[truncated: N chars elided; full output via __get_full_output("out-1")]`.

use std::{ops::Range, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use synthia_context::AgentState;
use synthia_core::FullOutputStore;

use crate::output_transformer::OutputTransformer;

/// Name of the virtual tool that serves a stashed output back.
///
/// Deliberately duplicated from
/// `synthia_tool::FULL_OUTPUT_TOOL_NAME`: the dependency runs
/// tool → core, never steering → tool, and the literal is the
/// contract between the marker and the tool.
const FULL_OUTPUT_TOOL_NAME: &str = "__get_full_output";

/// Separator between the head and the tail of a truncated output.
const ELISION: &str = "\n…\n";

/// Context utilization above which [`BudgetAwareTruncator`] halves
/// its budget (traitclaw's default threshold).
const DEFAULT_AGGRESSIVE_THRESHOLD: f32 = 0.8;

// ===========================================================================
// TransformerChain
// ===========================================================================

/// Applies transformers to one output, in order.
///
/// An empty chain is the identity: the output is returned untouched,
/// so callers never need to hold an `Option<OutputTransformer>` for
/// the "no transforms configured" case.
pub struct TransformerChain {
    transformers: Vec<Arc<dyn OutputTransformer>>,
}

impl TransformerChain {
    /// Chain over `transformers`, applied front to back.
    #[must_use]
    pub fn new(transformers: Vec<Arc<dyn OutputTransformer>>) -> Self {
        Self { transformers }
    }

    /// Append one transformer (builder style), returning the chain.
    #[must_use]
    pub fn push(mut self, transformer: Arc<dyn OutputTransformer>) -> Self {
        self.transformers.push(transformer);
        self
    }
}

#[async_trait]
impl OutputTransformer for TransformerChain {
    async fn transform(
        &self,
        mut output: String,
        tool_name: &str,
        state: &AgentState,
    ) -> String {
        for transformer in &self.transformers {
            output = transformer.transform(output, tool_name, state).await;
        }
        output
    }
}

// ===========================================================================
// JsonExtractor
// ===========================================================================

/// Extracts the first top-level JSON object or array from an
/// output, discarding the text around it.
///
/// Useful for tools whose payload is embedded in verbose logging.
/// Scanning is string- and escape-aware, so braces and brackets
/// inside JSON strings do not end the object early.
///
/// Candidates must also parse as JSON (`serde_json`), which keeps
/// log prefixes like `[INFO] ready …` from being mistaken for an
/// array.
///
/// # Fail-soft
///
/// When no JSON is found the input is returned **unchanged**. The
/// alternative — returning an error or an empty string — would
/// destroy output the model may still need, and the transformer has
/// no channel to report a parse failure.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonExtractor;

impl JsonExtractor {
    /// New extractor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl OutputTransformer for JsonExtractor {
    async fn transform(
        &self,
        output: String,
        _tool_name: &str,
        _state: &AgentState,
    ) -> String {
        match json_span(&output) {
            Some(span) => output[span].to_string(),
            None => output,
        }
    }
}

/// Span of the first balanced top-level JSON object or array in
/// `text` that `serde_json` accepts.
///
/// A candidate that does not parse is skipped and the scan resumes
/// one byte into it, so a valid payload nested inside a malformed
/// outer span is still recovered.
fn json_span(text: &str) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // Skip string literals so quoted braces/brackets are
            // not mistaken for structure.
            b'"' => {
                index = skip_string(bytes, index).unwrap_or(index + 1);
            }
            b'{' | b'[' => {
                if let Some(end) = balanced_end(bytes, index)
                    && serde_json::from_str::<Value>(&text[index..end]).is_ok()
                {
                    return Some(index..end);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

/// Byte index just past the string literal opening at
/// `bytes[start]`, or `None` when the literal never closes.
fn skip_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            // `\` escapes the next byte, including a closing quote.
            b'\\' => index += 2,
            b'"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// Byte index just past the brace/bracket group opening at
/// `bytes[start]`, or `None` when it never closes or closes out of
/// order.
fn balanced_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut closers = vec![if bytes[start] == b'{' { b'}' } else { b']' }];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = skip_string(bytes, index)?,
            b'{' => {
                closers.push(b'}');
                index += 1;
            }
            b'[' => {
                closers.push(b']');
                index += 1;
            }
            b'}' | b']' => {
                if closers.pop() != Some(bytes[index]) {
                    return None;
                }
                if closers.is_empty() {
                    return Some(index + 1);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

/// Byte offset just past the first `count` characters of `text`
/// (`text.len()` when it holds fewer).
fn head_end(text: &str, count: usize) -> usize {
    match text.char_indices().nth(count) {
        Some((offset, _)) => offset,
        None => text.len(),
    }
}

/// Byte offset at which the last `count` characters of `text` begin
/// — `text.len()` for `count == 0` (empty tail), and `0` when
/// `text` holds fewer than `count` characters (whole text).
fn tail_start(text: &str, count: usize) -> usize {
    if count == 0 {
        return text.len();
    }
    let mut seen = 0;
    for (offset, _) in text.char_indices().rev() {
        seen += 1;
        if seen == count {
            return offset;
        }
    }
    0
}

// ===========================================================================
// BudgetAwareTruncator
// ===========================================================================

/// Truncates an oversized output to a head/tail excerpt and stashes
/// the full text in a [`FullOutputStore`].
///
/// Behavior:
///
/// - Output within the effective budget → returned **unchanged**, no
///   store traffic.
/// - Otherwise → the full text goes into the store, and the returned
///   string is the first `head_chars` characters, an `…` separator,
///   the last `budget - head_chars` characters, then the marker
///   `[truncated: N chars elided; full output via
///   __get_full_output("out-1")]` naming the minted handle.
///
/// The effective budget is `budget_chars`, halved when the call's
/// [`AgentState`] reports context utilization above the aggressive
/// threshold (0.8 by default). The head defaults to half the
/// effective budget.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
///
/// use synthia_core::InMemoryFullOutputStore;
/// use synthia_steering::BudgetAwareTruncator;
///
/// let store = Arc::new(InMemoryFullOutputStore::unbounded());
/// let truncator =
///     BudgetAwareTruncator::new(store, 4_000).with_head_tail(1_500);
/// ```
pub struct BudgetAwareTruncator {
    store: Arc<dyn FullOutputStore>,
    /// Visible-character budget; halved under context pressure.
    budget_chars: usize,
    /// Explicit head size; `None` ⇒ half the effective budget.
    head_chars: Option<usize>,
    /// Context utilization above which the budget halves.
    aggressive_threshold: f32,
}

impl BudgetAwareTruncator {
    /// Truncator stashing into `store`, keeping `budget_chars`
    /// visible characters (excluding the separator and the marker).
    #[must_use]
    pub fn new(store: Arc<dyn FullOutputStore>, budget_chars: usize) -> Self {
        Self {
            store,
            budget_chars,
            head_chars: None,
            aggressive_threshold: DEFAULT_AGGRESSIVE_THRESHOLD,
        }
    }

    /// Keep `head_chars` characters from the start of the output;
    /// the remainder of the budget goes to the tail. Values larger
    /// than the effective budget are clamped, leaving no tail.
    #[must_use]
    pub fn with_head_tail(mut self, head_chars: usize) -> Self {
        self.head_chars = Some(head_chars);
        self
    }

    /// Halve the effective budget once context utilization exceeds
    /// `threshold` (clamped to `0.0..=1.0`). Defaults to traitclaw's
    /// `0.8`.
    #[must_use]
    pub fn with_aggressive_threshold(mut self, threshold: f32) -> Self {
        self.aggressive_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Visible-character budget for one call, derived from the
    /// per-call state snapshot.
    fn effective_budget(&self, state: &AgentState) -> usize {
        if state.context_utilization() > self.aggressive_threshold {
            self.budget_chars / 2
        } else {
            self.budget_chars
        }
    }
}

#[async_trait]
impl OutputTransformer for BudgetAwareTruncator {
    async fn transform(
        &self,
        output: String,
        _tool_name: &str,
        state: &AgentState,
    ) -> String {
        let budget = self.effective_budget(state);
        let total = output.chars().count();
        if total <= budget {
            return output;
        }
        let head_chars = self
            .head_chars
            .map(|head| head.min(budget))
            .unwrap_or(budget / 2);
        let tail_chars = budget - head_chars;
        let elided = total - head_chars - tail_chars;
        // Stash before composing: the full text must be retrievable
        // by the time the model can read the marker.
        let handle = self.store.put(output.clone());
        let head = &output[..head_end(&output, head_chars)];
        let tail = &output[tail_start(&output, tail_chars)..];
        let marker = format!(
            "[truncated: {elided} chars elided; full output via \
             {FULL_OUTPUT_TOOL_NAME}(\"{handle}\")]"
        );
        if tail_chars == 0 {
            format!("{head}\n{marker}")
        } else {
            format!("{head}{ELISION}{tail}\n{marker}")
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_core::InMemoryFullOutputStore;

    use super::*;

    /// Tags the output — chain-order evidence.
    struct Tag(&'static str);

    #[async_trait]
    impl OutputTransformer for Tag {
        async fn transform(
            &self,
            output: String,
            _tool_name: &str,
            _state: &AgentState,
        ) -> String {
            format!("{output}|{}", self.0)
        }
    }

    /// Idle state: zero utilization, so never in aggressive mode.
    fn state() -> AgentState {
        AgentState::with_window(1_000)
    }

    // -- TransformerChain ---------------------------------------------

    #[tokio::test]
    async fn chain_applies_every_transformer_in_order() {
        let chain = TransformerChain::new(vec![])
            .push(Arc::new(Tag("a")))
            .push(Arc::new(Tag("b")));
        assert_eq!(
            chain.transform("x".to_string(), "shell", &state()).await,
            "x|a|b"
        );
    }

    #[tokio::test]
    async fn empty_chain_is_the_identity() {
        let chain = TransformerChain::new(vec![]);
        assert_eq!(
            chain.transform("x".to_string(), "shell", &state()).await,
            "x"
        );
    }

    // -- JsonExtractor -------------------------------------------------

    #[tokio::test]
    async fn json_extractor_pulls_the_object_out_of_noisy_output() {
        let input = "starting job\n[INFO] ready\nresult: {\"ok\": true, \
                     \"n\": 2}\ndone";
        let out = JsonExtractor::new()
            .transform(input.to_string(), "shell", &state())
            .await;
        assert_eq!(out, "{\"ok\": true, \"n\": 2}");
    }

    #[tokio::test]
    async fn json_extractor_keeps_nesting_and_escapes_intact() {
        let json = r#"{"a": [1, {"b": "}"}], "c": "say \"hi\" [x]"}"#;
        let input = format!("noise before\n{json}\nnoise after");
        let out = JsonExtractor::new()
            .transform(input, "shell", &state())
            .await;
        assert_eq!(out, json);
    }

    #[tokio::test]
    async fn json_extractor_accepts_a_top_level_array() {
        let input = "values: [1, 2, [3, 4]] (parsed)";
        let out = JsonExtractor::new()
            .transform(input.to_string(), "shell", &state())
            .await;
        assert_eq!(out, "[1, 2, [3, 4]]");
    }

    #[tokio::test]
    async fn json_extractor_returns_json_free_output_unchanged() {
        for input in [
            "plain text with no payload",
            "[INFO] brackets that are not JSON",
            "{\"unbalanced\": 1",
            "trailing } brace",
        ] {
            let out = JsonExtractor::new()
                .transform(input.to_string(), "shell", &state())
                .await;
            assert_eq!(out, input);
        }
    }

    #[tokio::test]
    async fn json_extractor_recovers_a_value_from_a_malformed_span() {
        let out = JsonExtractor::new()
            .transform("{not json: {\"a\": 1}}".to_string(), "shell", &state())
            .await;
        assert_eq!(out, "{\"a\": 1}");
    }

    #[tokio::test]
    async fn chain_extracts_before_truncating() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let chain = TransformerChain::new(vec![Arc::new(JsonExtractor::new())])
            .push(Arc::new(BudgetAwareTruncator::new(Arc::clone(&store), 40)));
        let json = format!("{{\"k\": \"{}\"}}", "v".repeat(200));
        let noisy = format!("log: start\n{json}\nlog: end");
        let out = chain.transform(noisy, "shell", &state()).await;
        // Truncated down to the extracted JSON, not the noisy
        // wrapper it arrived in.
        assert!(out.starts_with("{\"k\": \"vv"));
        assert_eq!(store.get("out-1").as_deref(), Some(json.as_str()));
    }

    // -- BudgetAwareTruncator ------------------------------------------

    #[tokio::test]
    async fn truncator_passes_short_output_through_untouched() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 32);
        let out = truncator
            .transform("short".to_string(), "shell", &state())
            .await;
        assert_eq!(out, "short");
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn truncator_marks_and_stashes_long_output() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 20);
        let text = format!("HEAD{}TAILMARK", "x".repeat(200));
        let out = truncator.transform(text.clone(), "shell", &state()).await;
        assert!(out.contains("HEAD"));
        assert!(out.contains("TAILMARK"));
        assert!(out.contains(
            "[truncated: 192 chars elided; full output via \
             __get_full_output(\"out-1\")]"
        ));
        assert_eq!(store.get("out-1"), Some(text));
    }

    #[tokio::test]
    async fn with_head_tail_sets_the_head_size() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 20)
            .with_head_tail(15);
        let text = format!("{}M{}TAIL!", "H".repeat(15), "m".repeat(60));
        let out = truncator.transform(text.clone(), "shell", &state()).await;
        let (head, rest) = out.split_once(ELISION).expect("elision separator");
        assert_eq!(head, "H".repeat(15));
        assert!(rest.starts_with("TAIL!\n[truncated: 61 chars elided"));
        assert!(rest.contains("__get_full_output(\"out-1\")"));
        assert_eq!(store.get("out-1"), Some(text));
    }

    #[tokio::test]
    async fn head_larger_than_the_budget_drops_the_tail() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 20)
            .with_head_tail(999);
        let text = format!("{}TAIL", "H".repeat(50));
        let out = truncator.transform(text.clone(), "shell", &state()).await;
        assert!(!out.contains(ELISION));
        assert!(out.starts_with(&"H".repeat(20)));
        assert!(out.contains("chars elided"));
        assert_eq!(store.get("out-1"), Some(text));
    }

    #[tokio::test]
    async fn truncator_counts_characters_not_bytes() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 20);
        let text = "é".repeat(300);
        let out = truncator.transform(text.clone(), "shell", &state()).await;
        assert_eq!(out.chars().filter(|ch| *ch == 'é').count(), 20);
        assert!(out.contains(
            "[truncated: 280 chars elided; full output via \
             __get_full_output(\"out-1\")]"
        ));
        assert_eq!(store.get("out-1"), Some(text));
    }

    #[tokio::test]
    async fn high_utilization_halves_the_budget_per_call() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 100);
        let text = "y".repeat(80);
        let mut hot = state();
        hot.context_window = 100;
        hot.estimated_tokens = 10;
        assert_eq!(
            truncator.transform(text.clone(), "shell", &hot).await,
            text
        );
        assert!(store.is_empty());
        hot.estimated_tokens = 90;
        let out = truncator.transform(text.clone(), "shell", &hot).await;
        assert!(out.contains("chars elided"));
        assert_eq!(store.get("out-1"), Some(text));
    }

    #[tokio::test]
    async fn custom_threshold_moves_the_halving_point() {
        let store: Arc<dyn FullOutputStore> =
            Arc::new(InMemoryFullOutputStore::unbounded());
        let truncator = BudgetAwareTruncator::new(Arc::clone(&store), 100)
            .with_aggressive_threshold(0.5);
        let text = "y".repeat(80);
        let mut warm = state();
        warm.context_window = 100;
        warm.estimated_tokens = 60;
        let out = truncator.transform(text.clone(), "shell", &warm).await;
        assert!(out.contains("chars elided"));
        assert_eq!(store.get("out-1"), Some(text));
    }
}
