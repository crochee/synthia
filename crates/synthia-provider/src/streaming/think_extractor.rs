//! Shared `<think>…</think>` extraction for streaming providers.
//!
//! Several non-native reasoning providers (notably MiniMax-M2.7 and other
//! Qwen-derived models) emit reasoning inline in their text deltas using
//! `<think>…</think>` markers, even when the API surface exposes a
//! dedicated `reasoning_content` field. Without extraction, the reasoning
//! ends up concatenated with the visible answer on the frontend.
//!
//! Both the OpenAI and Anthropic streaming processors share this state
//! machine so reasoning is split out into its own `ContentPart::Reasoning`
//! chunks before the chunks reach the agent layer. The downstream A2A
//! mapping then turns reasoning into `kind: "model_reasoning"` and text
//! into `kind: "model_text"`, which the frontend dispatches on directly —
//! no client-side marker parsing required.
//!
//! Carry semantics:
//! - Outside a `<think>` block, the last `THINK_OPEN_LEN - 1` characters
//!   of pending text are withheld so a marker straddling two chunks is
//!   not misclassified as plain text.
//! - Inside a `<think>` block, the last `THINK_CLOSE_LEN - 1` characters
//!   of pending reasoning are withheld so a `</think>` straddling two
//!   chunks is still detected.
//!
//! The extractor is per-turn: the provider constructs a fresh instance
//! for each `message/stream` request, and the accumulated text / reasoning
//! strings feed into the final `SamplingResult`.

use crate::types::{ContentPart, ReasoningContent, StreamChunk, TextContent};

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";
const THINK_OPEN_LEN: usize = THINK_OPEN.len();
const THINK_CLOSE_LEN: usize = THINK_CLOSE.len();

/// Streaming state for splitting `<think>…</think>` markers out of
/// plain-text deltas. Constructed per-turn; `process_text` is fed each
/// delta, and `flush` is called once before the final `IsDone` chunk
/// to drain any residual buffer.
pub struct ThinkExtractor {
    /// True while we are between `<think>` and `</think>` markers.
    in_think: bool,
    /// Pending plain-text buffer (only used while `!in_think`). The
    /// last `THINK_OPEN_LEN - 1` characters are withheld as carry
    /// until the next delta resolves whether they are part of a
    /// marker.
    text_tail: String,
    /// Pending reasoning buffer (only used while `in_think`). The
    /// last `THINK_CLOSE_LEN - 1` characters are withheld as carry
    /// until the next delta resolves whether they are part of the
    /// closer.
    reasoning_tail: String,
}

impl Default for ThinkExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkExtractor {
    pub fn new() -> Self {
        Self {
            in_think: false,
            text_tail: String::new(),
            reasoning_tail: String::new(),
        }
    }

    /// Feed a single text delta. Returns zero or more
    /// `StreamChunk::Content` chunks: `Text` for plain content,
    /// `Reasoning` for content extracted between `<think>…</think>`
    /// markers. Both kinds are emitted incrementally so the frontend
    /// can stream-render reasoning alongside text.
    pub fn process_text(&mut self, text: &str) -> Vec<StreamChunk> {
        if self.in_think {
            self.feed_in_think(text)
        } else {
            self.feed_in_idle(text)
        }
    }

    /// Drain any residual text or reasoning that the carry was
    /// withholding. Callers should invoke this right before emitting
    /// `IsDone` so the final `SamplingResult` aggregates complete
    /// content.
    pub fn flush(&mut self) -> Vec<StreamChunk> {
        if self.in_think {
            if self.reasoning_tail.is_empty() {
                return Vec::new();
            }
            let tail = std::mem::take(&mut self.reasoning_tail);
            vec![StreamChunk::Content(ContentPart::Reasoning(
                ReasoningContent {
                    text: tail,
                    signature: None,
                },
            ))]
        } else {
            if self.text_tail.is_empty() {
                return Vec::new();
            }
            let tail = std::mem::take(&mut self.text_tail);
            vec![StreamChunk::Content(ContentPart::Text(TextContent {
                text: tail,
                cache_control: None,
            }))]
        }
    }

    fn feed_in_idle(&mut self, text: &str) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();
        self.text_tail.push_str(text);

        if let Some(open_idx) = self.text_tail.find(THINK_OPEN) {
            // Emit everything before the opener as plain text.
            if open_idx > 0 {
                let before = self.text_tail[..open_idx].to_string();
                chunks.push(text_chunk(&before));
            }
            // Drop the opener and switch into thinking mode. The
            // tail after the opener is processed recursively so a
            // single delta can both close a think block and start
            // a new one.
            let after = self.text_tail[open_idx + THINK_OPEN_LEN..].to_string();
            self.text_tail.clear();
            self.in_think = true;
            let more = self.feed_in_think(&after);
            chunks.extend(more);
            return chunks;
        }

        // No opener in the tail. Withhold the longest prefix of
        // `<think>` that the tail's suffix matches — only that
        // suffix could become a marker when the next delta
        // arrives. Everything else is safe to emit.
        let keep = longest_prefix_match(&self.text_tail, THINK_OPEN);
        if keep > 0 {
            let safe_len = self.text_tail.len() - keep;
            if safe_len > 0 {
                let safe = self.text_tail[..safe_len].to_string();
                self.text_tail = self.text_tail[safe_len..].to_string();
                chunks.push(text_chunk(&safe));
            }
        } else if !self.text_tail.is_empty() {
            // Tail can't be a partial marker; flush it.
            let safe = std::mem::take(&mut self.text_tail);
            chunks.push(text_chunk(&safe));
        }
        chunks
    }

    fn feed_in_think(&mut self, text: &str) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();
        self.reasoning_tail.push_str(text);

        if let Some(close_idx) = self.reasoning_tail.find(THINK_CLOSE) {
            // Emit everything before the closer as reasoning.
            if close_idx > 0 {
                let before = self.reasoning_tail[..close_idx].to_string();
                chunks.push(reasoning_chunk(&before));
            }
            // Drop the closer and switch back to idle. The tail
            // after the closer may contain further text or
            // additional markers — process recursively.
            let after =
                self.reasoning_tail[close_idx + THINK_CLOSE_LEN..].to_string();
            self.reasoning_tail.clear();
            self.in_think = false;
            let more = self.feed_in_idle(&after);
            chunks.extend(more);
            return chunks;
        }

        // No closer in the tail. Withhold the longest prefix of
        // `</think>` that the tail's suffix matches so a closer
        // straddling the next delta boundary is still detected.
        let keep = longest_prefix_match(&self.reasoning_tail, THINK_CLOSE);
        if keep > 0 {
            let safe_len = self.reasoning_tail.len() - keep;
            if safe_len > 0 {
                let safe = self.reasoning_tail[..safe_len].to_string();
                self.reasoning_tail =
                    self.reasoning_tail[safe_len..].to_string();
                chunks.push(reasoning_chunk(&safe));
            }
        } else if !self.reasoning_tail.is_empty() {
            let safe = std::mem::take(&mut self.reasoning_tail);
            chunks.push(reasoning_chunk(&safe));
        }
        chunks
    }
}

/// Length of the longest prefix of `marker` that the tail's suffix
/// matches. Returns 0 when the tail is empty or ends with a
/// character that cannot start the marker. Used to size the carry:
/// withholding more than that prefix would emit text that could
/// never become a marker, and withholding less risks misclassifying
/// a partial marker as plain text.
fn longest_prefix_match(tail: &str, marker: &str) -> usize {
    if tail.is_empty() {
        return 0;
    }
    let max = marker.len().saturating_sub(1).min(tail.len());
    for len in (1..=max).rev() {
        if tail.ends_with(&marker[..len]) {
            return len;
        }
    }
    0
}

fn text_chunk(text: &str) -> StreamChunk {
    StreamChunk::Content(ContentPart::Text(TextContent {
        text: text.to_string(),
        cache_control: None,
    }))
}

fn reasoning_chunk(text: &str) -> StreamChunk {
    StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
        text: text.to_string(),
        signature: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(chunk: &StreamChunk) -> &str {
        match chunk {
            StreamChunk::Content(ContentPart::Text(t)) => &t.text,
            StreamChunk::Content(ContentPart::Reasoning(r)) => &r.text,
            other => panic!("expected Content chunk, got {other:?}"),
        }
    }

    fn kind_of(chunk: &StreamChunk) -> &'static str {
        match chunk {
            StreamChunk::Content(ContentPart::Text(_)) => "text",
            StreamChunk::Content(ContentPart::Reasoning(_)) => "reasoning",
            other => panic!("expected Content chunk, got {other:?}"),
        }
    }

    #[test]
    fn plain_text_passes_through() {
        let mut ex = ThinkExtractor::new();
        let chunks = ex.process_text("Hello world");
        assert_eq!(chunks.len(), 1);
        assert_eq!(kind_of(&chunks[0]), "text");
        assert_eq!(text_of(&chunks[0]), "Hello world");
    }

    #[test]
    fn inline_think_block_splits_into_reasoning_then_text() {
        let mut ex = ThinkExtractor::new();
        let chunks = ex.process_text("<think>deep thought</think>answer");
        let kinds: Vec<&str> = chunks.iter().map(kind_of).collect();
        let texts: Vec<&str> = chunks.iter().map(text_of).collect();
        assert_eq!(kinds, vec!["reasoning", "text"]);
        assert_eq!(texts, vec!["deep thought", "answer"]);
    }

    #[test]
    fn text_before_think_marker() {
        let mut ex = ThinkExtractor::new();
        let chunks = ex.process_text("prefix<think>reason</think>");
        let kinds: Vec<&str> = chunks.iter().map(kind_of).collect();
        let texts: Vec<&str> = chunks.iter().map(text_of).collect();
        assert_eq!(kinds, vec!["text", "reasoning"]);
        assert_eq!(texts, vec!["prefix", "reason"]);
    }

    #[test]
    fn open_marker_split_across_chunks() {
        let mut ex = ThinkExtractor::new();
        // First chunk ends with "<thin" — a 5-char prefix of `<think>`,
        // withheld as carry.
        let chunks1 = ex.process_text("hello <thin");
        assert_eq!(chunks1.len(), 1);
        assert_eq!(kind_of(&chunks1[0]), "text");
        assert_eq!(text_of(&chunks1[0]), "hello ");
        // Second chunk completes "<think>" (the full `<think>` marker)
        // and provides the content.
        let chunks2 = ex.process_text("k>reason</think>done");
        let all: Vec<StreamChunk> =
            chunks1.into_iter().chain(chunks2).collect();
        let kinds: Vec<&str> = all.iter().map(kind_of).collect();
        assert_eq!(kinds, vec!["text", "reasoning", "text"]);
        let texts: Vec<&str> = all.iter().map(text_of).collect();
        assert_eq!(texts, vec!["hello ", "reason", "done"]);
    }

    #[test]
    fn close_marker_split_across_chunks() {
        let mut ex = ThinkExtractor::new();
        // First chunk enters think, then ends with "</thin" — a
        // 6-char prefix of `</think>`, withheld as carry.
        let chunks1 = ex.process_text("<think>thinking </thin");
        // The safe part "thinking " is emitted as reasoning; the
        // 6-char suffix "</thin" stays in the carry.
        assert_eq!(chunks1.len(), 1);
        assert_eq!(kind_of(&chunks1[0]), "reasoning");
        assert_eq!(text_of(&chunks1[0]), "thinking ");
        // Second chunk completes "</think>" (the full `</think>`
        // marker) and adds trailing text.
        let chunks2 = ex.process_text("k>after");
        let all: Vec<StreamChunk> =
            chunks1.into_iter().chain(chunks2).collect();
        let kinds: Vec<&str> = all.iter().map(kind_of).collect();
        assert_eq!(kinds, vec!["reasoning", "text"]);
        let texts: Vec<&str> = all.iter().map(text_of).collect();
        assert_eq!(texts, vec!["thinking ", "after"]);
    }

    #[test]
    fn multiple_think_blocks_in_one_stream() {
        let mut ex = ThinkExtractor::new();
        let chunks = ex
            .process_text("<think>a</think>mid<think>b</think>end")
            .into_iter()
            .chain(ex.flush())
            .collect::<Vec<_>>();
        let kinds: Vec<&str> = chunks.iter().map(kind_of).collect();
        assert_eq!(kinds, vec!["reasoning", "text", "reasoning", "text"]);
        let texts: Vec<&str> = chunks.iter().map(text_of).collect();
        assert_eq!(texts, vec!["a", "mid", "b", "end"]);
    }

    #[test]
    fn flush_releases_unresolved_partial_opener_as_text() {
        // When the stream ends with a partial opener that never
        // gets completed (no further delta), the carry is flushed
        // back as plain text rather than silently dropped.
        let mut ex = ThinkExtractor::new();
        ex.process_text("answer <thin");
        let chunks = ex.flush();
        assert_eq!(chunks.len(), 1);
        assert_eq!(kind_of(&chunks[0]), "text");
        assert_eq!(text_of(&chunks[0]), "<thin");
    }

    #[test]
    fn flush_releases_unresolved_partial_closer_as_reasoning() {
        // Mirror case for the close marker: a partial `</thin`
        // carried across the end of the stream is flushed as
        // reasoning rather than dropped.
        let mut ex = ThinkExtractor::new();
        ex.process_text("<think>reason </thin");
        let chunks = ex.flush();
        assert_eq!(chunks.len(), 1);
        assert_eq!(kind_of(&chunks[0]), "reasoning");
        assert_eq!(text_of(&chunks[0]), "</thin");
    }

    #[test]
    fn empty_input_emits_nothing() {
        let mut ex = ThinkExtractor::new();
        assert!(ex.process_text("").is_empty());
        assert!(ex.flush().is_empty());
    }

    #[test]
    fn no_marker_no_split_even_with_long_text() {
        let mut ex = ThinkExtractor::new();
        let long = "x".repeat(200);
        let chunks = ex.process_text(&long);
        // No `<think>` prefix can match a string of all `x`, so the
        // whole input is emitted as a single text chunk.
        let emitted: String = chunks
            .iter()
            .filter(|c| kind_of(c) == "text")
            .map(|c| text_of(c).to_string())
            .collect();
        assert_eq!(emitted, long);
    }
}
