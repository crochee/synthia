//! `SessionOp::PromptMulti` — the lossless multimodal path.
//!
//! The text-only `InputQueue` is bypassed because it is
//! JSON-string-typed and cannot round-trip image/audio bytes;
//! the per-run `pending_multimodal` slot is the only path that
//! survives. A multimodal turn persists only the typed text (the
//! binary parts stay out of the sink by design — base64 image
//! bytes would bloat the JSONL).

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use synthia::{
    harness::{SessionEndReason, SystemEvent},
    provider::{ContentPart, ImageContent, TextContent, types::ImageDetail},
};

use super::{
    super::SessionOp,
    support::{
        RecordingFactory,
        find_events_jsonl,
        make_manager_and_controller,
    },
};

/// `SessionOp::PromptMulti` must reach the agent as
/// `AgentInput::multi_with_history(...)` carrying the
/// supplied parts losslessly. The text-only `InputQueue` is
/// bypassed (it's JSON-string-typed and cannot round-trip
/// image/audio bytes), and the per-run `pending_multimodal`
/// slot is the only path that survives.
///
/// Without this contract the multimodal prompt is silently
/// dropped — the agent only sees the typed text from
/// elsewhere, or runs with an empty input.
#[tokio::test]
async fn test_prompt_multi_round_trip_carries_parts() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![super::super::AgentEvent::System(
                SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                },
            )],
            vec![],
        ));

    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Image + text multimodal turn. The image carries a
    // recognisable base64 payload so we can assert lossless
    // round-trip.
    let image_part = ContentPart::Image(ImageContent {
        data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=".to_string(),
        mime_type: "image/png".to_string(),
        detail: Some(ImageDetail::Auto),
    });
    let text_part = ContentPart::Text(TextContent {
        text: "describe this".to_string(),
        cache_control: None,
    });

    controller
        .submit(SessionOp::PromptMulti {
            parts: vec![text_part.clone(), image_part.clone()],
            agent_name: Some("research-bot".to_string()),
            priority: 1,
        })
        .await
        .unwrap();

    // Wait for the run to consume the multimodal payload.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if calls.lock().unwrap().len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("multimodal run should start");

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let input = &recorded[0];

    // Two parts must reach the agent: text + image.
    assert_eq!(
        input.content.len(),
        2,
        "PromptMulti must carry both parts; got {:?}",
        input.content
    );
    assert!(
        matches!(&input.content[0], ContentPart::Text(t) if t.text == "describe this"),
        "first part must be the typed text; got {:?}",
        input.content[0]
    );
    match &input.content[1] {
        ContentPart::Image(img) => {
            assert_eq!(img.mime_type, "image/png");
            assert!(
                img.data.starts_with("iVBORw0"),
                "image bytes must round-trip losslessly; got prefix `{}`",
                &img.data[..img.data.len().min(10)]
            );
        }
        other => panic!("second part must be the Image; got {other:?}"),
    }
}

/// A multimodal turn MUST persist its typed text as a
/// `UserInput` row.
///
/// `PromptMulti` parks the whole payload in `parts` and leaves
/// the drained text queue empty, so the pre-existing
/// `if !prompt.is_empty()` guard saw an empty prompt and wrote
/// **no row at all** — a resumed session then showed the
/// assistant's reply with no question above it, and
/// `events_to_messages` handed the next turn's agent a history
/// whose last user message was missing.
///
/// The binary parts stay out of the sink by design (base64
/// image bytes would bloat the JSONL); only the text is
/// durable.
#[tokio::test]
async fn test_prompt_multi_persists_its_text_as_a_user_input_row() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![super::super::AgentEvent::System(
                SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                },
            )],
            vec![],
        ));

    let (controller, _manager, temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    let image_part = ContentPart::Image(ImageContent {
        data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="
            .to_string(),
        mime_type: "image/png".to_string(),
        detail: Some(ImageDetail::Auto),
    });
    let text_part = ContentPart::Text(TextContent {
        text: "describe this".to_string(),
        cache_control: None,
    });

    controller
        .submit(SessionOp::PromptMulti {
            parts: vec![text_part, image_part],
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();
    // `RecordingFactory` records the `AgentInput` it was
    // handed, so wait on that list — `wait_for_runs` observes
    // the `AgentRunConfig` list, which this factory does not
    // populate.
    tokio::time::timeout(Duration::from_secs(2), async {
        while calls.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("multimodal run should start");

    // Give the run task a moment to append its rows, then
    // close so the log is flushed and the file is stable.
    controller.close().await.unwrap();

    let events_path = find_events_jsonl(temp.path())
        .expect("events.jsonl must exist after close");
    let body = std::fs::read_to_string(&events_path).unwrap();
    assert!(
        body.contains("describe this"),
        "the multimodal prompt's text must be persisted; log:\n{body}"
    );
    assert!(
        body.lines().any(|l| l.contains("\"type\":\"UserInput\"")),
        "a UserInput row must be written for a multimodal turn; \
         log:\n{body}"
    );
    // And the bytes stay out of the durable log.
    assert!(
        !body.contains("iVBORw0KGgo"),
        "image bytes must NOT be serialised into the JSONL sink; \
         log:\n{body}"
    );
}
