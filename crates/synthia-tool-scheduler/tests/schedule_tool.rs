//! `schedule` tool — the full action matrix against a real on-disk
//! store, plus the one thing that matters end to end: a job the model
//! created through the tool is the job the host's `Scheduler::tick`
//! delivers.

use std::sync::Arc;

use serde_json::{Value, json};
use synthia_core::SharedClock;
#[cfg(feature = "cron")]
use synthia_scheduler::{CronTrigger, JobKind};
use synthia_scheduler::{JobStatus, ScheduleStore, Scheduler};
use synthia_tool::{Context, Tool, ToolOutput, ToolRegistry};
use synthia_tool_scheduler::{SchedulerTool, register_scheduler_tool};
use tempfile::TempDir;

/// Pinned "now" so every rendered instant is deterministic.
const T0: &str = "2026-09-19T09:00:00Z";

struct Fixture {
    _dir: TempDir,
    store: Arc<ScheduleStore>,
    tool: SchedulerTool,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(ScheduleStore::new(dir.path()));
        let tool = SchedulerTool::with_clock(
            Arc::clone(&store),
            SharedClock::fixed_from_rfc3339(T0),
        );
        Self {
            _dir: dir,
            store,
            tool,
        }
    }

    async fn call(&self, arguments: Value) -> ToolOutput {
        self.tool.call(arguments, &Context::default()).await
    }

    /// The host's scheduler over the same store — the delivery path.
    fn host_scheduler(&self) -> Scheduler {
        Scheduler::new(Arc::clone(&self.store))
    }
}

/// The textual projection of a `ToolOutput`.
fn text_of(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|part| part.text().map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert the output is a model-facing error and return its text.
fn error_of(output: &ToolOutput) -> String {
    assert_eq!(output.is_error, Some(true), "expected an error output");
    text_of(output)
}

#[tokio::test]
async fn list_on_an_empty_store_says_so() {
    let fx = Fixture::new();
    let out = fx.call(json!({"action": "list"})).await;
    assert_eq!(out.is_error, None);
    assert!(text_of(&out).contains("No scheduled jobs."));
}

#[tokio::test]
async fn create_then_list_reports_the_job() {
    let fx = Fixture::new();
    let created = fx
        .call(json!({
            "action": "create",
            "name": "nightly-report",
            "kind": "interval",
            "interval_seconds": 3600,
            "payload": {"prompt": "summarise the day"},
            "description": "daily digest"
        }))
        .await;
    assert_eq!(created.is_error, None);
    let text = text_of(&created);
    assert!(text.contains("Scheduled `nightly-report`"), "{text}");
    assert!(text.contains("every 3600s"), "{text}");
    // The delivery boundary is stated, not implied.
    assert!(text.contains("never fires a job itself"), "{text}");

    let listed = fx.call(json!({"action": "list"})).await;
    let text = text_of(&listed);
    assert!(text.contains("Scheduled jobs (1):"), "{text}");
    assert!(text.contains("- nightly-report ["), "{text}");
    assert!(
        text.contains("next fire: 2026-09-19T09:00:00+00:00"),
        "{text}"
    );
    assert!(
        text.contains("next fire: 2026-09-19T09:00:00+00:00 (due)"),
        "{text}"
    );
    assert!(text.contains("description: daily digest"), "{text}");
    assert!(
        text.contains(r#"payload: {"prompt":"summarise the day"}"#),
        "{text}"
    );
}

#[tokio::test]
async fn a_created_job_is_the_job_the_host_tick_delivers() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "recheck-ci",
        "kind": "interval",
        "interval_seconds": 60,
        "payload": {"prompt": "re-check CI"}
    }))
    .await;

    // The host drives the scheduler; the tool only wrote the row.
    let now = chrono::DateTime::parse_from_rfc3339(T0)
        .expect("valid T0")
        .with_timezone(&chrono::Utc);
    let tick = fx.host_scheduler().tick(now);
    assert_eq!(tick.fired.len(), 1, "the host must see the new job");
    assert_eq!(tick.fired[0].name, "recheck-ci");
    assert_eq!(tick.fired[0].payload, json!({"prompt": "re-check CI"}));
    // A recurring job rearms itself.
    assert_eq!(
        tick.next_deadline,
        Some(now + chrono::Duration::seconds(60))
    );

    // And the store on disk carries it (survives a reload).
    let reloaded = ScheduleStore::new(fx.store.root());
    assert_eq!(reloaded.load().expect("reload"), 1);
}

#[tokio::test]
async fn a_once_job_completes_after_its_single_fire() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "remind",
        "kind": "once",
        "first_fire_at": "2026-09-19T10:30:00Z"
    }))
    .await;

    let scheduler = fx.host_scheduler();
    let before = chrono::DateTime::parse_from_rfc3339("2026-09-19T10:00:00Z")
        .expect("valid")
        .with_timezone(&chrono::Utc);
    assert!(scheduler.tick(before).fired.is_empty(), "not due yet");

    let after = chrono::DateTime::parse_from_rfc3339("2026-09-19T10:30:00Z")
        .expect("valid")
        .with_timezone(&chrono::Utc);
    assert_eq!(scheduler.tick(after).fired.len(), 1);

    // Terminal: the row is `completed`, not silently re-firing.
    let job = fx.store.list().into_iter().next().expect("job");
    assert_eq!(job.status, JobStatus::Completed);
    let listed = fx.call(json!({"action": "list"})).await;
    assert!(text_of(&listed).contains("(completed)"));
}

#[tokio::test]
async fn pause_stops_delivery_and_resume_restores_it() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "tick",
        "kind": "interval",
        "interval_seconds": 60
    }))
    .await;

    let paused = fx.call(json!({"action": "pause", "job": "tick"})).await;
    assert_eq!(paused.is_error, None);
    assert!(text_of(&paused).contains("is now paused"));

    let now = chrono::DateTime::parse_from_rfc3339(T0)
        .expect("valid")
        .with_timezone(&chrono::Utc);
    assert!(
        fx.host_scheduler().tick(now).fired.is_empty(),
        "a paused job must not fire"
    );

    let resumed = fx.call(json!({"action": "resume", "job": "tick"})).await;
    assert!(text_of(&resumed).contains("is now active"));
    assert_eq!(fx.host_scheduler().tick(now).fired.len(), 1);
}

#[tokio::test]
async fn jobs_are_addressable_by_id_as_well_as_by_name() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "by-id",
        "kind": "once"
    }))
    .await;
    let id = fx.store.list().into_iter().next().expect("job").id;

    let out = fx
        .call(json!({"action": "pause", "job": id.as_str()}))
        .await;
    assert_eq!(out.is_error, None);
    assert!(text_of(&out).contains("is now paused"), "{}", text_of(&out));

    let listed = fx.call(json!({"action": "list"})).await;
    assert!(text_of(&listed).contains("(paused)"));
}

#[tokio::test]
async fn remove_deletes_the_row_and_its_file() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "doomed",
        "kind": "once"
    }))
    .await;

    let removed = fx.call(json!({"action": "remove", "job": "doomed"})).await;
    assert_eq!(removed.is_error, None);
    assert!(text_of(&removed).contains("Removed `doomed`"));

    assert!(fx.store.list().is_empty());
    let file = fx.store.root().join("schedules");
    assert_eq!(
        std::fs::read_dir(&file).expect("schedules dir").count(),
        0,
        "the job's file must be gone"
    );
}

#[tokio::test]
async fn an_unknown_selector_lists_what_does_exist() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "known",
        "kind": "once"
    }))
    .await;

    let out = fx.call(json!({"action": "remove", "job": "nope"})).await;
    let text = error_of(&out);
    assert!(
        text.contains("No scheduled job with id or name `nope`"),
        "{text}"
    );
    assert!(text.contains("Known jobs: known ("), "{text}");

    let missing = error_of(&fx.call(json!({"action": "pause"})).await);
    assert!(missing.contains("requires `job`"), "{missing}");
}

#[tokio::test]
async fn create_rejects_a_duplicate_name() {
    let fx = Fixture::new();
    let first = fx
        .call(json!({"action": "create", "name": "dup", "kind": "once"}))
        .await;
    assert_eq!(first.is_error, None);

    let second = fx
        .call(json!({"action": "create", "name": "dup", "kind": "once"}))
        .await;
    let text = error_of(&second);
    // A name problem, not the store's generic `Io` category.
    assert!(text.contains("A job named `dup` already exists"), "{text}");
    assert!(text.contains("`remove`"), "{text}");
    assert_eq!(fx.store.list().len(), 1, "the original job survives");
}

#[tokio::test]
async fn create_refuses_what_the_scheduler_cannot_honour() {
    let fx = Fixture::new();

    // No cron: the crate cannot compute a recurrence, so accepting the
    // expression would make a job that fires every minute.
    //
    // Refused only while the feature is off — with it on, the same call
    // is a valid `kind: "cron"` (see the cron-feature tests below).
    #[cfg(not(feature = "cron"))]
    {
        let cron = error_of(
            &fx.call(json!({
                "action": "create",
                "name": "c",
                "kind": "cron",
                "interval_seconds": 60
            }))
            .await,
        );
        assert!(cron.contains("Unknown `kind` `cron`"), "{cron}");
        assert!(cron.contains("cannot compute a cron recurrence"), "{cron}");
    }

    // An interval job needs its cadence, and it must advance.
    let missing = error_of(
        &fx.call(json!({
            "action": "create",
            "name": "i",
            "kind": "interval"
        }))
        .await,
    );
    assert!(missing.contains("requires `interval_seconds`"), "{missing}");

    let zero = error_of(
        &fx.call(json!({
            "action": "create",
            "name": "z",
            "kind": "interval",
            "interval_seconds": 0
        }))
        .await,
    );
    assert!(zero.contains("must be greater than zero"), "{zero}");

    // A stray cadence on a one-shot is a mistake, not a no-op.
    let stray = error_of(
        &fx.call(json!({
            "action": "create",
            "name": "o",
            "kind": "once",
            "interval_seconds": 30
        }))
        .await,
    );
    assert!(stray.contains("requires `kind: \"interval\"`"), "{stray}");

    assert!(fx.store.list().is_empty(), "nothing was created");
}

/// An unbounded cadence would abort the **host's** tick, not the tool
/// call: `Interval` reaches `chrono::Duration::seconds`, which panics
/// above `i64::MAX / 1000`, and the failed advance is never persisted,
/// so it would panic again on every subsequent tick. A value above
/// `i64::MAX` is worse — it wraps negative, leaving a past `next_fire_at`
/// and a job that re-fires forever. Both are refused at `create`.
#[tokio::test]
async fn create_refuses_an_interval_that_could_panic_the_host() {
    let fx = Fixture::new();

    for absurd in [
        10_000_000_000_000_000u64, // 1e16 — chrono's panicking range
        u64::MAX,                  // wraps negative in `as i64`
    ] {
        let text = error_of(
            &fx.call(json!({
                "action": "create",
                "name": "absurd",
                "kind": "interval",
                "interval_seconds": absurd
            }))
            .await,
        );
        assert!(text.contains("exceeds the maximum"), "{text}");
    }
    assert!(fx.store.list().is_empty(), "nothing was created");

    // The boundary itself is accepted, and the host tick survives it.
    let at_max = fx
        .call(json!({
            "action": "create",
            "name": "at-max",
            "kind": "interval",
            "interval_seconds": synthia_scheduler::MAX_INTERVAL_SECS
        }))
        .await;
    assert_eq!(at_max.is_error, None, "{}", text_of(&at_max));

    let now = chrono::DateTime::parse_from_rfc3339(T0)
        .expect("valid T0")
        .with_timezone(&chrono::Utc);
    let tick = fx.host_scheduler().tick(now);
    assert_eq!(tick.fired.len(), 1, "the host tick must not panic");
    assert!(tick.next_deadline.is_some(), "and must rearm");
}

/// `Completed` is terminal in the scheduler crate's state machine, so
/// neither flip is allowed: `pause` would make the row un-reapable
/// (`gc_completed` only collects `Completed` rows) and `resume` would
/// re-fire a one-shot that already fired.
#[tokio::test]
async fn a_completed_job_cannot_be_paused_or_resumed() {
    let fx = Fixture::new();
    fx.call(json!({
        "action": "create",
        "name": "finished",
        "kind": "once"
    }))
    .await;
    let now = chrono::DateTime::parse_from_rfc3339(T0)
        .expect("valid T0")
        .with_timezone(&chrono::Utc);
    assert_eq!(fx.host_scheduler().tick(now).fired.len(), 1);
    assert_eq!(
        fx.store.list().into_iter().next().expect("job").status,
        JobStatus::Completed
    );

    for action in ["pause", "resume"] {
        let text = error_of(
            &fx.call(json!({"action": action, "job": "finished"})).await,
        );
        assert!(text.contains("already completed"), "{text}");
        // The remedy names the order that actually works: a completed
        // row keeps its name taken, so `remove` must come first.
        assert!(text.contains("`remove` this row first"), "{text}");
    }

    // Still terminal, so the store can still reap it.
    assert_eq!(
        fx.store.list().into_iter().next().expect("job").status,
        JobStatus::Completed
    );
    assert_eq!(
        fx.store
            .gc_completed(now + chrono::Duration::hours(1))
            .expect("sweep")
            .len(),
        1,
        "the row must stay reapable"
    );
}

/// `create` must not be an unbounded write: each job is a file on disk,
/// and nothing in-tree reaps them.
#[tokio::test]
async fn create_refuses_past_the_job_cap() {
    let fx = Fixture::new();
    for i in 0..synthia_tool_scheduler::MAX_JOBS {
        let out = fx
            .call(json!({
                "action": "create",
                "name": format!("job-{i}"),
                "kind": "once"
            }))
            .await;
        assert_eq!(out.is_error, None, "job {i}: {}", text_of(&out));
    }

    let text = error_of(
        &fx.call(json!({
            "action": "create",
            "name": "one-too-many",
            "kind": "once"
        }))
        .await,
    );
    assert!(text.contains("the maximum"), "{text}");
    assert!(text.contains("`remove`"), "{text}");
    assert_eq!(fx.store.list().len(), synthia_tool_scheduler::MAX_JOBS);

    // The interval bound *is* mirrored into the schema, so the model is
    // told that limit before it hits it; the job-count cap has no schema
    // slot (no argument carries a count) and reaches the model through
    // `description()` instead.
    let schema = fx.tool.parameters();
    assert_eq!(schema["properties"]["action"]["enum"][0], "list");
    assert_eq!(
        schema["properties"]["interval_seconds"]["maximum"],
        json!(synthia_scheduler::MAX_INTERVAL_SECS),
        "the interval bound must be mirrored into the schema"
    );
    assert!(
        fx.tool.description().contains("At most 64 jobs"),
        "the job cap must be stated to the model"
    );
}

/// A duplicate name is reported as a name problem, not as the generic
/// `Io` error the store uses — and the message names the order that
/// actually works, since a completed row keeps its name taken.
#[tokio::test]
async fn a_duplicate_name_names_the_remedy() {
    let fx = Fixture::new();
    fx.call(json!({"action": "create", "name": "taken", "kind": "once"}))
        .await;

    let text = error_of(
        &fx.call(json!({"action": "create", "name": "taken", "kind": "once"}))
            .await,
    );
    assert!(
        text.contains("A job named `taken` already exists"),
        "{text}"
    );
    assert!(text.contains("`remove`"), "{text}");

    // The row that names the remedy actually works: remove, then re-create.
    assert_eq!(
        fx.call(json!({"action": "remove", "job": "taken"}))
            .await
            .is_error,
        None
    );
    assert_eq!(
        fx.call(json!({"action": "create", "name": "taken", "kind": "once"}))
            .await
            .is_error,
        None
    );
}

#[tokio::test]
async fn create_rejects_a_malformed_fire_time() {
    let fx = Fixture::new();
    let out = fx
        .call(json!({
            "action": "create",
            "name": "bad-time",
            "kind": "once",
            "first_fire_at": "tomorrow morning"
        }))
        .await;
    let text = error_of(&out);
    assert!(text.contains("is not an RFC 3339 timestamp"), "{text}");
    assert!(fx.store.list().is_empty());
}

#[tokio::test]
async fn malformed_calls_are_model_facing_errors() {
    let fx = Fixture::new();

    let unknown = error_of(&fx.call(json!({"action": "launch"})).await);
    assert!(unknown.contains("Unknown `action` `launch`"), "{unknown}");
    assert!(
        unknown.contains("list, create, pause, resume, remove"),
        "{unknown}"
    );

    let nameless =
        error_of(&fx.call(json!({"action": "create", "kind": "once"})).await);
    assert!(nameless.contains("requires `name`"), "{nameless}");

    let typo =
        error_of(&fx.call(json!({"action": "list", "jobs": "all"})).await);
    assert!(typo.contains("Invalid arguments"), "{typo}");
}

#[tokio::test]
async fn a_future_fire_time_reads_as_a_countdown() {
    let fx = Fixture::new();
    let out = fx
        .call(json!({
            "action": "create",
            "name": "later",
            "kind": "once",
            "first_fire_at": "2026-09-19T11:30:00Z"
        }))
        .await;
    assert!(text_of(&out).contains("(in 2h)"), "{}", text_of(&out));

    let listed = fx.call(json!({"action": "list"})).await;
    assert!(text_of(&listed).contains("(in 2h)"), "{}", text_of(&listed));
}

#[tokio::test]
async fn registration_publishes_a_sequential_json_tool() {
    let fx = Fixture::new();
    let registry = ToolRegistry::new();
    assert!(register_scheduler_tool(
        &registry,
        SchedulerTool::new(Arc::clone(&fx.store)),
    ));

    use synthia_core::registry::Registry as _;
    let entry = registry.get("schedule").await.unwrap().expect("registered");
    let tool = entry.tool_instance();
    assert_eq!(tool.name(), "schedule");
    assert_eq!(tool.mode(), synthia_tool::ExecutionMode::Sequential);
    assert_eq!(
        tool.output_definition().kind,
        synthia_tool::RenderKind::Json
    );
    assert_eq!(tool.parameters()["required"][0], json!("action"));

    // Re-registering under the same name is allowed and the newest
    // registration wins — the registry's documented LIFO rule.
    let replacement = ToolRegistry::new();
    assert!(register_scheduler_tool(
        &replacement,
        SchedulerTool::new(Arc::clone(&fx.store)),
    ));
    assert!(register_scheduler_tool(
        &replacement,
        SchedulerTool::new(Arc::clone(&fx.store)),
    ));
    assert_eq!(replacement.tool_count(), 1);
}

/// The schema is the model's contract: `cron` is offered exactly when
/// the build can honour it, and `cron_expr` is advertised only then — a
/// property `create` would refuse is a trap for the model.
#[test]
fn the_schema_offers_cron_only_with_the_feature() {
    let fx = Fixture::new();
    let schema = fx.tool.parameters();
    #[cfg(feature = "cron")]
    assert_eq!(
        schema["properties"]["kind"]["enum"],
        json!(["interval", "once", "cron"]),
    );
    #[cfg(not(feature = "cron"))]
    assert_eq!(
        schema["properties"]["kind"]["enum"],
        json!(["interval", "once"]),
    );
    #[cfg(feature = "cron")]
    assert_eq!(schema["properties"]["cron_expr"]["type"], json!("string"));
    #[cfg(not(feature = "cron"))]
    assert!(
        schema["properties"].get("cron_expr").is_none(),
        "a refused argument must not be advertised: {schema}",
    );
}

/// Feature `cron`: a cron job's first fire is the expression's own next
/// occurrence — `CronTrigger::first_after` at the tool's pinned now —
/// not the one-minute placeholder a build without the feature re-arms.
/// The host's tick is armed for that same instant.
#[cfg(feature = "cron")]
#[tokio::test]
async fn create_accepts_kind_cron_with_the_expressions_first_fire() {
    let fx = Fixture::new();
    let created = fx
        .call(json!({
            "action": "create",
            "name": "weekday-standup",
            "kind": "cron",
            "cron_expr": "0 9 * * 1-5",
            "payload": {"prompt": "post the standup summary"}
        }))
        .await;
    assert_eq!(created.is_error, None, "{}", text_of(&created));

    let now = chrono::DateTime::parse_from_rfc3339(T0)
        .expect("valid T0")
        .with_timezone(&chrono::Utc);
    // T0 is a Saturday, so the next weekday firing is Monday 09:00.
    let expected = CronTrigger::first_after("0 9 * * 1-5", now)
        .expect("the expression parses");
    assert_eq!(expected.to_rfc3339(), "2026-09-21T09:00:00+00:00");

    let job = fx.store.list().into_iter().next().expect("stored row");
    assert_eq!(job.next_fire_at, expected);
    assert!(
        matches!(&job.kind, JobKind::Cron { expr } if expr == "0 9 * * 1-5"),
        "{:?}",
        job.kind,
    );
    assert_ne!(
        job.next_fire_at,
        now + chrono::Duration::seconds(60),
        "the placeholder minute is the one instant the expression rules out",
    );

    let listed = fx.call(json!({"action": "list"})).await;
    assert!(
        text_of(&listed).contains("next fire: 2026-09-21T09:00:00+00:00"),
        "{}",
        text_of(&listed),
    );

    let tick = fx.host_scheduler().tick(now);
    assert!(tick.fired.is_empty(), "Monday's job is not due on Saturday");
    assert_eq!(
        tick.next_deadline,
        Some(expected),
        "the host must arm for the real first fire",
    );
}

/// Feature `cron`: an expression the parser rejects is refused with the
/// parser's own reason, and nothing is written.
#[cfg(feature = "cron")]
#[tokio::test]
async fn create_rejects_an_expression_the_cron_parser_cannot_read() {
    let fx = Fixture::new();
    let text = error_of(
        &fx.call(json!({
            "action": "create",
            "name": "six-field",
            "kind": "cron",
            "cron_expr": "0 */5 * * * *"
        }))
        .await,
    );
    assert!(text.contains("invalid cron"), "{text}");
    assert!(
        text.contains("expected 5 fields (m h dom mon dow), got 6"),
        "the parser's reason must reach the model: {text}",
    );
    assert!(text.contains("0 */5 * * * *"), "{text}");
    assert!(fx.store.list().is_empty(), "a refused call writes nothing");
}

/// Feature `cron`: the arguments around `kind: "cron"` are checked, each
/// naming its own remedy rather than writing a row that means something
/// else.
#[cfg(feature = "cron")]
#[tokio::test]
async fn create_refuses_cron_arguments_that_do_not_fit_the_kind() {
    let fx = Fixture::new();
    let cases = [
        (
            json!({"action": "create", "name": "no-expr", "kind": "cron"}),
            "requires `cron_expr`",
        ),
        (
            json!({
                "action": "create",
                "name": "both-fires",
                "kind": "cron",
                "cron_expr": "0 9 * * *",
                "first_fire_at": "2026-09-19T10:00:00Z"
            }),
            "mutually exclusive",
        ),
        (
            json!({
                "action": "create",
                "name": "interval-with-expr",
                "kind": "interval",
                "interval_seconds": 60,
                "cron_expr": "0 9 * * *"
            }),
            "`cron_expr` requires `kind: \"cron\"`",
        ),
        (
            json!({"action": "create", "name": "daily", "kind": "daily"}),
            "`interval`, `once` or `cron`",
        ),
    ];
    for (call, expected) in cases {
        let text = error_of(&fx.call(call).await);
        assert!(text.contains(expected), "expected {expected:?} in {text}");
    }
    assert!(fx.store.list().is_empty(), "nothing was created");
}
