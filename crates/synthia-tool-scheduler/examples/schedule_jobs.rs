//! `schedule_jobs` — the `schedule` tool driven through `Tool::call`,
//! with the host's `Scheduler` ticking the same store.
//!
//! The loop is the whole point of the adapter: the model plans through
//! the tool, the host delivers through the scheduler, and both sides
//! see one state because they share one `Arc<ScheduleStore>`.
//!
//! 1. `create` an interval job (a recurring check) and a `once` job.
//! 2. `list` them the way the model would read them back.
//! 3. The host's `Scheduler::tick` delivers the due job — the tool
//!    never fires anything itself. The scheduler's wheel is a snapshot
//!    of the store, and it catches up on its own: every pass compares
//!    the store's revision with the one its wheel was built from, so a
//!    write the tool made is in place by the next `tick` (`resync` only
//!    exists for a host that needs it sooner).
//! 4. `pause` stops delivery; `resume` restores it.
//! 5. `remove` deletes the row and its file.
//! 6. A `kind: "cron"` call: with the `cron` feature on it schedules the
//!    expression's real next fire; without it the same call is refused.
//!
//! ```bash
//! cargo run --example schedule_jobs -p synthia-tool-scheduler
//! ```

use std::sync::Arc;

use serde_json::json;
use synthia_core::SharedClock;
use synthia_scheduler::{ScheduleStore, Scheduler};
use synthia_tool::{Context, Tool, ToolOutput};
use synthia_tool_scheduler::SchedulerTool;
use tempfile::TempDir;

/// The textual projection of a `ToolOutput`.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| part.text().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One model-issued call, printed as the model would see it.
async fn call(tool: &SchedulerTool, arguments: serde_json::Value) {
    let output = tool.call(arguments, &Context::default()).await;
    println!(
        "[{}] {}",
        if output.is_error == Some(true) {
            "error"
        } else {
            "ok"
        },
        text_of(&output),
    );
}

#[tokio::main]
async fn main() {
    let dir = TempDir::new().expect("temp dir");
    let store = Arc::new(ScheduleStore::new(dir.path()));
    // Pin "now" so the printed countdowns are reproducible; production
    // uses `SchedulerTool::new` (the system clock).
    let tool = SchedulerTool::with_clock(
        Arc::clone(&store),
        SharedClock::fixed_from_rfc3339("2026-09-19T09:00:00Z"),
    );
    // The host's delivery path over the SAME store.
    let scheduler = Scheduler::new(Arc::clone(&store));
    // The instant this run is pinned to (the tool's clock above).
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-19T09:00:00Z")
        .expect("valid")
        .with_timezone(&chrono::Utc);

    println!("== the `schedule` tool: plan work, host delivers it ==\n");

    println!("-- create an interval job + a one-shot job");
    call(
        &tool,
        json!({
            "action": "create",
            "name": "recheck-ci",
            "kind": "interval",
            "interval_seconds": 300,
            "payload": {"prompt": "re-check the failing CI job"},
            "description": "watch the pipeline for an hour",
        }),
    )
    .await;
    call(
        &tool,
        json!({
            "action": "create",
            "name": "follow-up-report",
            "kind": "once",
            "first_fire_at": "2026-09-19T11:00:00Z",
            "payload": {"prompt": "write the follow-up report"},
        }),
    )
    .await;

    println!("\n-- list (how the model reads the schedule back)");
    call(&tool, json!({"action": "list"})).await;

    // The tool wrote the store directly, and the scheduler's wheel is a
    // *snapshot* of it (see the scheduler crate's module docs) — the tick
    // below picks that write up on its own, because the store's revision
    // moved since the wheel was built. No `resync` call is needed.

    println!("\n-- the host tick delivers the due job");
    let tick = scheduler.tick(now);
    for fired in &tick.fired {
        println!(
            "  fired {} ({}) payload={}",
            fired.name, fired.id, fired.payload
        );
    }
    println!("  next deadline: {:?}", tick.next_deadline);

    println!("\n-- pause stops delivery");
    call(&tool, json!({"action": "pause", "job": "recheck-ci"})).await;
    // The pause reached the store the same way; this tick sees it.
    let paused_at = now + chrono::Duration::seconds(600);
    let tick = scheduler.tick(paused_at);
    println!("  fired on the next tick: {} job(s)", tick.fired.len());

    println!("\n-- resume restores it");
    call(&tool, json!({"action": "resume", "job": "recheck-ci"})).await;
    let resumed_at = now + chrono::Duration::seconds(900);
    let tick = scheduler.tick(resumed_at);
    println!(
        "  fired after resume: {:?}",
        tick.fired
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
    );

    println!("\n-- remove deletes the row and its file");
    call(
        &tool,
        json!({"action": "remove", "job": "follow-up-report"}),
    )
    .await;
    println!("  files left: {}", store.list().len());

    #[cfg(not(feature = "cron"))]
    {
        println!(
            "\n-- a kind the scheduler cannot honour is refused, not accepted"
        );
        call(
            &tool,
            json!({
                "action": "create",
                "name": "cron-job",
                "kind": "cron",
                "interval_seconds": 60,
            }),
        )
        .await;
    }

    // Same call, same build knob as the scheduler's real recurrence:
    // with the feature on, the row lands on the expression's own next
    // occurrence (the pinned clock is a Saturday, so the next weekday
    // 09:00 is Monday) instead of a placeholder minute.
    #[cfg(feature = "cron")]
    {
        println!(
            "\n-- `create kind=cron` computes the expression's real first fire"
        );
        call(
            &tool,
            json!({
                "action": "create",
                "name": "weekday-standup",
                "kind": "cron",
                "cron_expr": "0 9 * * 1-5",
                "payload": {"prompt": "post the standup summary"},
            }),
        )
        .await;
    }

    #[cfg(not(feature = "cron"))]
    {
        assert_eq!(store.list().len(), 1, "only the interval job remains");
    }
    #[cfg(feature = "cron")]
    {
        assert_eq!(
            store.list().len(),
            2,
            "the interval job and the cron job remain"
        );
    }
    println!("\nSCHEDULE-TOOL: OK");
}
