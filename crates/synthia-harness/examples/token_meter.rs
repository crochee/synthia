//! # Usage-anchored token meter
//!
//! Seam: `synthia_session::token_meter::TokenMeter` folds the
//! durable `SessionEvent` log into a `TokenMeasurement`: provider
//! `Usage` rows become the pressure *baseline*, the heuristic
//! surface delta since that sample is the projection, and
//! `CompactionSettings::should_compact_anchored` gates on it — so
//! a compaction that shadows a span stops the trigger firing
//! without any fresh provider sample.
//!
//! Run: cargo run -p synthia-harness --example token_meter

use synthia_context::CompactionSettings;
use synthia_provider::{Message, Role};
use synthia_session::{
    SessionEvent,
    SurfaceOp,
    TokenMeasurement,
    TokenMeter,
    request_header,
    usage,
};

/// Route capacity the demo's compaction gate is configured against.
const CONTEXT_WINDOW: u64 = 1_000;

/// One surface-eligible append row carrying `message`.
fn append(seq: u64, message: Message) -> SessionEvent {
    let data = serde_json::to_value(&message).expect("message serializes");
    let surface_op = Some(SurfaceOp::append());
    let ts = String::new();
    match message.role {
        Role::User => SessionEvent::UserMessage {
            seq,
            ts,
            data,
            surface_op,
        },
        _ => SessionEvent::AssistantMessage {
            seq,
            ts,
            data,
            surface_op,
        },
    }
}

/// Re-stamp a builder event's placeholder seq (the controller does
/// this at append time).
fn restamp(mut event: SessionEvent, seq: u64) -> SessionEvent {
    match &mut event {
        SessionEvent::RequestHeader { seq: slot, .. }
        | SessionEvent::Usage { seq: slot, .. } => *slot = seq,
        _ => {}
    }
    event
}

/// Replace the whole `[0, span_len)` surface span with one row.
fn compaction(seq: u64, span_len: usize) -> SessionEvent {
    SessionEvent::Compaction {
        seq,
        ts: String::new(),
        data: serde_json::to_value(Message::assistant("condensed"))
            .expect("message serializes"),
        surface_op: SurfaceOp::Replace {
            start: 0,
            end: span_len,
            source_event_seqs: vec![1, 2, 5],
        },
    }
}

fn report(label: &str, m: &TokenMeasurement, settings: &CompactionSettings) {
    println!(
        "{label}: baseline={:?} surface_delta_tokens={} projected_tokens={} \
         should_compact_anchored={}",
        m.baseline,
        m.surface_delta_tokens,
        m.total_tokens,
        settings.should_compact_anchored(m, CONTEXT_WINDOW, 1),
    );
}

fn main() {
    let settings = CompactionSettings {
        enabled: true,
        reserve_tokens: 100,
        keep_recent_tokens: 0,
        min_messages_between_compaction: 1,
    };
    let mut meter = TokenMeter::new();

    // Request envelope, the provider sample that priced it, and a
    // later assistant surface growth with no fresh sample.
    let anchored_log = vec![
        append(1, Message::user("context ".repeat(120))),
        append(2, Message::assistant("ack")),
        restamp(request_header("replay", "scripted", "hash", "p"), 3),
        restamp(usage(700, 25, 725, None, None, None), 4),
    ];
    meter.observe(&anchored_log).expect("fold anchored prefix");
    let anchored = meter.measure();
    report("after usage anchor", &anchored, &settings);

    let mut grown_log = anchored_log.clone();
    grown_log.push(append(5, Message::assistant("reasoning ".repeat(240))));
    meter.observe(&grown_log).expect("fold surface growth");
    let grown = meter.measure();
    report("after surface growth", &grown, &settings);

    let mut compacted_log = grown_log.clone();
    compacted_log.push(compaction(6, 3));
    meter.observe(&compacted_log).expect("fold compaction");
    let compacted = meter.measure();
    report("after compaction", &compacted, &settings);

    assert!(!settings.should_compact_anchored(&anchored, CONTEXT_WINDOW, 1));
    assert!(settings.should_compact_anchored(&grown, CONTEXT_WINDOW, 1));
    assert!(!settings.should_compact_anchored(&compacted, CONTEXT_WINDOW, 1));
    println!("TOKEN-METER: OK");
}
