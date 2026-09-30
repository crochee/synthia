//! # Run inbox: steering and follow-up injection
//!
//! Seam: `synthia_harness::RunInbox` — an interactive front
//! end feeds a running ReAct loop through `MpscInbox` /
//! `RunInboxHandle`. Steering is drained at iteration boundaries
//! and injected as a trailing user message; follow-ups are polled
//! at a would-be stop and revive the run. Every non-empty drain
//! surfaces as the typed `SystemEvent::SteeringInjected`.
//!
//! Run: cargo run -p synthia-harness --example run_inbox_steering

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use synthia_core::AtomicCancelToken;
use synthia_harness::{
    Agent,
    AgentEvent,
    AgentInput,
    ReActAgent,
    agent::{MpscInbox, RunInboxHandle},
    events::{SteeringSource, SystemEvent},
};
use synthia_provider::{Message, SamplingResult, StreamChunk};
use synthia_test_support::ReplayProvider;
use synthia_tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};

/// Stands in for a user typing while a tool round runs: when the
/// model calls it, it queues one steering message and one
/// follow-up into the run inbox.
struct QueueOnCall {
    handle: RunInboxHandle,
}

#[async_trait]
impl Tool for QueueOnCall {
    fn name(&self) -> &str {
        "queue_on_call"
    }

    fn description(&self) -> &str {
        "Queue steering + follow-up messages into the run inbox."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        self.handle
            .send_steering(Message::user("use the short form instead"))
            .expect("inbox receiver alive");
        self.handle
            .send_follow_up(Message::user("now summarize that"))
            .expect("inbox receiver alive");
        ToolOutput::text("queued")
    }
}

/// One scripted model call: ask for `queue_on_call`, then stop.
fn tool_sample() -> Vec<StreamChunk> {
    vec![
        StreamChunk::ToolCallStart {
            id: "call-1".to_string(),
            name: "queue_on_call".to_string(),
            arguments: serde_json::Value::String("{}".to_string()),
        },
        StreamChunk::ToolCallEnd {
            id: "call-1".to_string(),
        },
        StreamChunk::IsDone {
            result: Box::new(SamplingResult::default()),
        },
    ]
}

/// One scripted model call that answers with `text` and stops.
fn text_sample(text: &str) -> Vec<StreamChunk> {
    vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: text.to_string(),
            stop_reason: Some("end_turn".to_string()),
            ..Default::default()
        }),
    }]
}

#[tokio::main]
async fn main() {
    let (inbox, handle) = MpscInbox::channel();
    let provider = Arc::new(ReplayProvider::new(vec![
        tool_sample(),
        text_sample("first answer"),
        text_sample("revived answer"),
    ]));

    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(ToolEntry::new(Arc::new(QueueOnCall { handle })));
    let agent = ReActAgent::new(provider.clone(), registry)
        .with_run_inbox(Arc::new(inbox));

    let cancel = Arc::new(AtomicCancelToken::new());
    let mut stream = agent.run(AgentInput::text("hello"), cancel).await;

    let mut injected: Vec<SteeringSource> = Vec::new();
    let mut revived = false;
    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::System(SystemEvent::SteeringInjected {
                source,
                count,
            }) => {
                println!("injected: source={source:?} count={count}");
                injected.push(source);
            }
            AgentEvent::ModelDone(result) if !result.text.is_empty() => {
                println!("model answered: {}", result.text);
                revived |= result.text.contains("revived");
            }
            _ => {}
        }
    }

    provider
        .assert_consumed()
        .expect("every scripted sample was used");
    assert!(injected.contains(&SteeringSource::Steering));
    assert!(injected.contains(&SteeringSource::FollowUp));
    assert!(revived, "the follow-up must revive the stopping run");
    println!("RUN-INBOX: OK");
}
