//! Keyless replay: drive a recorded model-call script through the
//! `ModelProvider` trait, then prove the scenario drained it.
//!
//! What to look at:
//!
//! - the two-call script — one chunk list per model call, replayed in
//!   order; `complete_with_stream` hands each chunk to the `on_delta`
//!   callback before the call returns its folded response.
//! - `assert_consumed()` — turns "the scenario made fewer calls than
//!   were recorded" (or cancelled mid-stream) into a crisp teardown
//!   error instead of a silent underrun.
//! - the `from_events_jsonl` half — the same provider built from an
//!   inline session-event log, so the fixture path is exercised too.
//!
//! No network, no API key, no clock reads.
//!
//! Run: cargo run -p synthia-test-support --example replay_provider

use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ModelProvider,
    SamplingResult,
    StreamChunk,
    TextContent,
};
use synthia_test_support::ReplayProvider;

fn text_chunk(text: &str) -> StreamChunk {
    StreamChunk::Content(ContentPart::Text(TextContent {
        text: text.to_string(),
        cache_control: None,
    }))
}

fn done(text: &str, reason: &str) -> StreamChunk {
    StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: text.to_string(),
            stop_reason: Some(reason.to_string()),
            ..SamplingResult::default()
        }),
    }
}

/// Text of a completion response, flattening multi-part content.
fn response_text(response: &CompletionResponse) -> String {
    match &response.content {
        Content::Single(ContentPart::Text(text)) => text.text.clone(),
        Content::Multi(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect(),
        _ => String::new(),
    }
}

/// An inline session-event log: the header and user message are
/// ignored, the `assistant_chunk` records become one call script.
fn fixture_jsonl() -> String {
    let records = [
        serde_json::json!({"type": "user_message", "data": {}}),
        serde_json::json!({
            "type": "assistant_chunk",
            "data": {"delta": "from "},
        }),
        serde_json::json!({
            "type": "assistant_chunk",
            "data": {"delta": "the fixture"},
            "finish_reason": "stop",
        }),
    ];
    let mut out = String::new();
    for record in records {
        out.push_str(&record.to_string());
        out.push('\n');
    }
    out
}

#[tokio::main]
async fn main() {
    let provider = ReplayProvider::new(vec![
        vec![
            text_chunk("Tool groups "),
            text_chunk("are ready."),
            done("Tool groups are ready.", "end_turn"),
        ],
        vec![
            text_chunk("Second call "),
            text_chunk("acknowledged."),
            done("Second call acknowledged.", "end_turn"),
        ],
    ]);

    for call in 1..=2 {
        println!("--- call {call} ---");
        let response = provider
            .complete_with_stream(
                CompletionRequest::default(),
                None,
                Box::new(|chunk| match chunk {
                    StreamChunk::Content(ContentPart::Text(text)) => {
                        println!("  chunk text: {:?}", text.text);
                    }
                    StreamChunk::Stop(reason) => {
                        println!("  chunk stop: {reason}");
                    }
                    StreamChunk::IsDone { result } => {
                        println!(
                            "  chunk is_done: text={:?} stop_reason={:?}",
                            result.text, result.stop_reason
                        );
                    }
                    other => println!("  chunk other: {other:?}"),
                }),
            )
            .await
            .expect("scripted call replays");
        println!(
            "  response: id={} text={:?} stop_reason={:?}",
            response.id,
            response_text(&response),
            response.stop_reason
        );
    }

    provider
        .assert_consumed()
        .expect("both scripts fully consumed");
    println!("assert_consumed: Ok (2 of 2 scripts, no undelivered chunks)");

    let jsonl = fixture_jsonl();
    let fixture =
        ReplayProvider::from_events_jsonl(&jsonl).expect("fixture parses");
    println!("--- fixture session (from_events_jsonl) ---");
    let response = fixture
        .complete(CompletionRequest::default())
        .await
        .expect("fixture script replays");
    println!("  response text: {:?}", response_text(&response));
    fixture
        .assert_consumed()
        .expect("fixture script fully consumed");
    println!("assert_consumed: Ok (1 of 1 script)");

    println!("REPLAY-PROVIDER: OK");
}
