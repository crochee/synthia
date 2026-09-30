//! # Cache-stable runtime-context snapshots
//!
//! Seam: `synthia_harness::RuntimeContext` — per-dispatch facts (cwd,
//! workspace root, platform, today, model id) rendered as a
//! user-role snapshot instead of the system prompt, so the cached
//! provider prefix survives. With the clock injected, two renders
//! of unchanged facts are byte-identical and the loop can skip the
//! append; a new day or a new model changes the body.
//!
//! Run: cargo run -p synthia-harness --example runtime_context

use chrono::{DateTime, Utc};
use synthia_harness::RuntimeContext;

fn clock(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .expect("valid RFC 3339 timestamp")
        .with_timezone(&Utc)
}

fn main() {
    let root = "/workspace/demo";
    let same_day_later = clock("2026-09-12T23:59:00Z");
    let next_day = clock("2026-09-13T00:01:00Z");

    let first = RuntimeContext::from_runtime(root, same_day_later);
    let second = RuntimeContext::from_runtime(root, same_day_later);
    let third = RuntimeContext::from_runtime(root, next_day);

    let first_body = first.render_snapshot();
    let second_body = second.render_snapshot();
    let third_body = third.render_snapshot();

    println!("--- snapshot ---");
    println!("{first_body}");
    println!("--- cache behaviour ---");
    println!(
        "byte-identical for the same injected day: {}",
        first_body == second_body,
    );
    println!(
        "changed once the clock advances a day:  {}",
        first_body != third_body,
    );

    // The model id is another volatile fact the snapshot carries.
    let mut with_model = first.clone();
    with_model.model_id = Some("replay/scripted-model".to_string());
    let model_body = with_model.render_snapshot();
    println!(
        "changed once the model id changes:       {}",
        first_body != model_body,
    );

    assert!(
        first_body == second_body,
        "unchanged facts must render byte-identical for the prefix cache",
    );
    assert!(first_body != third_body, "a new day must change the body");
    assert!(
        first_body != model_body,
        "a new model id must change the body",
    );
    println!("RUNTIME-CONTEXT: OK");
}
