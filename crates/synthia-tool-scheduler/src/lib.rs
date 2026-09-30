//! The `schedule` tool: create and manage scheduled jobs from the
//! model side.
//!
//! [`synthia-scheduler`](https://docs.rs/synthia-scheduler) is a
//! runtime-free building block: a [`ScheduleStore`] persists `Job`
//! rows and a `Scheduler::tick(now)` reports which are due. Nothing in
//! that crate is a [`Tool`], so an agent could not touch it — this
//! crate is the adapter that turns the store into one model-facing
//! tool (`schedule`) with five actions:
//!
//! | action | effect |
//! |---|---|
//! | `list` | every job with its id, kind, status and next fire time |
//! | `create` | add an `interval` or `once` job; `cron` with the feature |
//! | `pause` / `resume` | flip a job's status without deleting it |
//! | `remove` | delete a job and its file |
//!
//! Jobs are addressed by `id` **or** `name` (name is unique per store —
//! including completed rows, which `remove` is what frees), so the model
//! can work with whichever it recorded. The store holds at most
//! [`MAX_JOBS`] rows, because each one is a file on disk.
//!
//! ## The delivery boundary, stated plainly
//!
//! The tool manages the *schedule*, never the *firing*. A due job is
//! reported by `Scheduler::tick(now)` to whoever drives the scheduler —
//! the host. The tool's `create` result says so, and no tool action
//! waits, sleeps, or spawns: a `create` that fired its own payload
//! would be a second, divergent delivery path beside the host's.
//!
//! Nothing in-tree drives a `tick`, so a deployment that installs this
//! tool must own that timer (arm it from `Scheduler::next_deadline`,
//! which is the helper the crate ships for exactly that). Without a
//! driver the schedule is a durable plan nobody executes — which is why
//! the tool says "the host's scheduler tick delivers it" in every
//! `create` result rather than implying it fires.
//!
//! ## A write reaches the next pass on its own
//!
//! `Scheduler` ticks off a timing wheel, which is a *snapshot* of the
//! store's active jobs — not a live view of it. Every tool action
//! (`create`, `pause`, `resume`, `remove`) writes the store directly, so
//! the wheel has to learn about it: each pass compares the store's
//! revision (`ScheduleStore::version`) with the one its wheel was built
//! from and rebuilds when they differ. A tool-side change — or rows
//! another process wrote and the host ingested with
//! [`ScheduleStore::load`] — is therefore visible to the next `tick` and
//! `next_deadline` with no call from the host.
//!
//! `Scheduler::resync` remains as a retained explicit-rebuild escape
//! hatch: it forces the rebuild at an instant of the caller's choosing,
//! rather than waiting for the next pass to notice. The facade module's
//! doctest and `MINIMAL.md`'s Step-3 recipe both say the same thing.
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use synthia_scheduler::{ScheduleStore, Scheduler};
//!
//! let store = Arc::new(ScheduleStore::new("/tmp/synthia-schedules"));
//! let scheduler = Scheduler::new(Arc::clone(&store));
//! # let now = chrono::DateTime::from_timestamp(0, 0).unwrap();
//! // …the model's `create` / `pause` / `remove` wrote `store`…
//! scheduler.resync(now); // optional: the next tick reconciles anyway
//! ```
//!
//! ## The payload convention
//!
//! [`Job::payload`] is a free-form `Value` the scheduler only stores and
//! re-emits — deliberately, so the crate stays domain-free. A model
//! asked for a bare `payload` therefore has nothing to go on, so this
//! tool documents one convention and the schema's description repeats
//! it:
//!
//! ```json
//! {"agent": "researcher", "prompt": "re-check the failing CI job"}
//! ```
//!
//! That is [`TaskSpec`]'s vocabulary (`agent` = registry name of the
//! peer to run, `prompt` = its user input), which makes a firing's
//! payload directly usable by the delegation path — the reference
//! consumer of `Job::on_fire` is a subagent notification. `agent` may be
//! omitted when the host has a default. The host interprets the payload,
//! so a deployment with another convention should say so in the system
//! prompt; the tool does not enforce this shape.
//!
//! [`TaskSpec`]: https://docs.rs/synthia-tool-task
//!
//! ## What it deliberately refuses
//!
//! `cron` is a `create` kind only when the `cron` feature is on. On a
//! default build the scheduler crate stores a cron expression but
//! cannot compute a recurrence (`Job::advance` uses a one-minute
//! placeholder), and free-form cron parsing is the host's job by that
//! crate's own design. A tool that accepted `cron` there would
//! therefore create jobs that silently fire every minute — so the
//! schema offers `interval` (with `interval_seconds`) and `once`, which
//! the crate can actually honour, and refuses `kind: "cron"` as an
//! unknown kind.
//!
//! With the `cron` feature on, the scheduler's real recurrence is
//! linked in: `create` accepts `kind: "cron"` with a 5-field POSIX
//! `cron_expr`, the job is stored as `JobKind::Cron`, and its first
//! fire is the expression's own next occurrence
//! (`CronTrigger::first_after`) instead of a placeholder minute. What
//! does not change is the delivery boundary above: the host still owns
//! the tick.
//!
//! `interval_seconds` is bounded at 10 years ([`MAX_INTERVAL_SECS`]), and
//! a job that already `Completed` cannot be paused or resumed. Both are
//! refusals rather than defaults because each would otherwise be a bug in
//! the *host* rather than in the call — the [`MAX_INTERVAL_SECS`] docs
//! carry the first case.
//!
//! ```
//! use std::sync::Arc;
//!
//! use synthia_scheduler::ScheduleStore;
//! use synthia_tool::ToolRegistry;
//! use synthia_tool_scheduler::{SchedulerTool, register_scheduler_tool};
//!
//! let store = Arc::new(ScheduleStore::new("/tmp/synthia-schedules"));
//! let registry = ToolRegistry::new();
//! assert!(register_scheduler_tool(
//!     &registry,
//!     SchedulerTool::new(store)
//! ));
//! ```
//!
//! See `examples/schedule_jobs.rs` for the same wiring driven through
//! `Tool::call` — including the host's `Scheduler` tick delivering a job
//! the tool created — and `synthia-tool-scheduler/tests/` for the full
//! action matrix.

use std::{fmt::Write as _, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use synthia_core::{Clock, SharedClock};
#[cfg(feature = "cron")]
use synthia_scheduler::CronTrigger;
use synthia_scheduler::{
    Job,
    JobKind,
    JobStatus,
    MAX_INTERVAL_SECS,
    ScheduleStore,
    ScheduleStoreError,
};
use synthia_tool::{
    Context,
    Tool,
    ToolAnnotations,
    ToolEntry,
    ToolOutput,
    ToolRegistry,
    output::{RenderKind, ToolOutputDefinition},
    traits::ExecutionMode,
};

/// The name the tool registers under.
pub const SCHEDULE_TOOL_NAME: &str = "schedule";

/// Actions the tool accepts, in the schema's `enum` order.
const ACTIONS: [&str; 5] = ["list", "create", "pause", "resume", "remove"];

/// `kind` values `create` accepts, in the schema's `enum` order.
///
/// `cron` joins them only with the `cron` feature: the schema must not
/// advertise a kind `create` would refuse, and it must not hide one the
/// build can honour.
#[cfg(feature = "cron")]
const KINDS: [&str; 3] = ["interval", "once", "cron"];
#[cfg(not(feature = "cron"))]
const KINDS: [&str; 2] = ["interval", "once"];

/// Schema description for `kind`, matching [`KINDS`]' cfg-dual shape.
#[cfg(feature = "cron")]
const KIND_DESCRIPTION: &str = "`interval` re-fires every \
                                `interval_seconds`; `once` fires a single \
                                time at `first_fire_at`; `cron` fires on \
                                the 5-field POSIX expression in \
                                `cron_expr`. `create` only.";
#[cfg(not(feature = "cron"))]
const KIND_DESCRIPTION: &str = "`interval` re-fires every \
                                `interval_seconds`; `once` fires a single \
                                time at `first_fire_at`. `create` only.";

/// Most jobs one store may hold.
///
/// These rows are *files on disk*, and each `create` is a model-issued
/// write that nothing in-tree reaps (`gc_completed` collects only
/// `Completed` rows, and only when the host calls it). Without a cap, a
/// model looping on `create` grows `<root>/schedules/` without bound.
/// `synthia-tool-todo` sets the same kind of ceiling for the same reason
/// (`MAX_TODO_ITEMS`).
///
/// Unlike the interval bound there is no schema slot for it — `create`
/// adds one job per call and no argument carries a count — so the limit
/// reaches the model through the tool description instead, plus this
/// check before the write.
pub const MAX_JOBS: usize = 64;

/// One `schedule` call.
///
/// `deny_unknown_fields` mirrors the schema's
/// `additionalProperties: false`: a misspelled key is a model-facing
/// error instead of a silently dropped argument.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleRequest {
    action: String,
    /// Job id or name — required by everything except `list`/`create`.
    #[serde(default)]
    job: Option<String>,
    /// `create`: operator-facing unique name.
    #[serde(default)]
    name: Option<String>,
    /// `create`: `"interval"` or `"once"`.
    #[serde(default)]
    kind: Option<String>,
    /// `create` with `kind: "interval"`.
    #[serde(default)]
    interval_seconds: Option<u64>,
    /// `create` with `kind: "cron"` (requires the feature).
    #[serde(default)]
    cron_expr: Option<String>,
    /// `create`: first fire time (RFC 3339). Defaults to now.
    #[serde(default)]
    first_fire_at: Option<String>,
    /// `create`: free-form payload the host interprets on firing.
    #[serde(default)]
    payload: Option<Value>,
    /// `create`: human-readable description for operator dashboards.
    #[serde(default)]
    description: Option<String>,
}

/// The `schedule` tool: one handle over a shared [`ScheduleStore`].
///
/// The store is shared (not owned) because the host's scheduler ticks
/// the same rows: `Scheduler::new(Arc::clone(&store))` in the host,
/// `SchedulerTool::new(store)` here, one on-disk truth. `Arc`, not two
/// stores over one root — `list` serves from the store's cache, so a
/// second instance would read stale until it reloaded.
pub struct SchedulerTool {
    store: Arc<ScheduleStore>,
    clock: SharedClock,
}

impl SchedulerTool {
    /// Bind the tool to `store`, reloading the on-disk jobs into its
    /// cache so `list` reports what a previous process left behind
    /// (the store's documented reload-on-boot, paid once at
    /// construction). Reads then serve that cache; a *peer process's*
    /// later writes appear at the next reload, exactly as
    /// [`ScheduleStore`] documents. Reads the wall clock through
    /// [`SystemClock`](synthia_core::SystemClock).
    #[must_use]
    pub fn new(store: Arc<ScheduleStore>) -> Self {
        Self::with_clock(store, SharedClock::system())
    }

    /// Same, with an injected [`Clock`] — how tests pin "now" instead
    /// of racing the real one.
    #[must_use]
    pub fn with_clock(store: Arc<ScheduleStore>, clock: SharedClock) -> Self {
        if let Err(error) = store.load() {
            // Boot recovery is best-effort: an unreadable store leaves
            // the cache empty and every later action reports the real
            // error, which beats refusing to build the tool.
            tracing::warn!(
                target: "synthia.scheduler",
                error = %error,
                "schedule tool could not preload the store",
            );
        }
        Self { store, clock }
    }

    /// The store this tool manages — the same handle the host ticks.
    #[must_use]
    pub fn store(&self) -> &Arc<ScheduleStore> {
        &self.store
    }

    /// Every job as a one-line-per-field listing.
    fn list(&self) -> ToolOutput {
        let mut jobs = self.store.list();
        if jobs.is_empty() {
            return ToolOutput::text(format!(
                "No scheduled jobs. (store: {})",
                self.store.root().display()
            ));
        }
        // Earliest fire first: the order an operator reads.
        jobs.sort_by_key(|job| (job.next_fire_at, job.id.clone()));
        let now = self.clock.now();
        let mut out = format!("Scheduled jobs ({}):", jobs.len());
        for job in &jobs {
            let _ = write!(
                out,
                "\n- {} [{}] {}{}",
                job.name,
                job.id.as_str(),
                describe_kind(&job.kind),
                if job.status == JobStatus::Active {
                    String::new()
                } else {
                    format!(" ({})", status_name(job.status))
                },
            );
            let _ = write!(
                out,
                "\n  next fire: {} ({})",
                job.next_fire_at.to_rfc3339(),
                relative(now, job.next_fire_at),
            );
            if let Some(fired) = job.last_fired_at {
                let _ = write!(out, "\n  last fired: {}", fired.to_rfc3339());
            }
            if !job.description.is_empty() {
                let _ = write!(out, "\n  description: {}", job.description);
            }
            if job.payload != json!({}) {
                let _ = write!(out, "\n  payload: {}", job.payload);
            }
        }
        ToolOutput::text(out)
    }

    /// Add one job.
    fn create(&self, request: &ScheduleRequest) -> ToolOutput {
        let Some(name) = request.name.as_deref().filter(|n| !n.is_empty())
        else {
            return missing_argument("create", "name");
        };
        let payload = request.payload.clone().unwrap_or_else(|| json!({}));
        let kind = match request.kind.as_deref() {
            Some("once") => JobKind::Once,
            Some("interval") => {
                let Some(seconds) = request.interval_seconds else {
                    return missing_argument("create", "interval_seconds");
                };
                if seconds == 0 {
                    return ToolOutput::error(
                        "`interval_seconds` must be greater than zero.",
                    );
                }
                if seconds > MAX_INTERVAL_SECS {
                    return ToolOutput::error(format!(
                        "`interval_seconds` of {seconds} exceeds the \
                         maximum of {MAX_INTERVAL_SECS} (10 years). Pick a \
                         shorter cadence."
                    ));
                }
                JobKind::Interval {
                    interval: Duration::from_secs(seconds),
                }
            }
            // The expression is only *parsed* below, where the first
            // fire is computed: one call site, one error message.
            #[cfg(feature = "cron")]
            Some("cron") => {
                let Some(expr) = request.cron_expr.as_deref() else {
                    return missing_argument("create", "cron_expr");
                };
                if request.first_fire_at.is_some() {
                    return ToolOutput::error(
                        "`first_fire_at` and `cron_expr` are mutually \
                         exclusive: the expression decides when the job \
                         first fires. Give one or the other.",
                    );
                }
                JobKind::Cron {
                    expr: expr.to_string(),
                }
            }
            #[cfg(not(feature = "cron"))]
            Some(other) => {
                return ToolOutput::error(format!(
                    "Unknown `kind` `{other}`; expected `interval` or \
                     `once`. A recurring `interval` job with \
                     `interval_seconds` covers what `cron` would; the \
                     scheduler crate cannot compute a cron recurrence."
                ));
            }
            #[cfg(feature = "cron")]
            Some(other) => {
                return ToolOutput::error(format!(
                    "Unknown `kind` `{other}`; expected `interval`, \
                     `once` or `cron`."
                ));
            }
            None => return missing_argument("create", "kind"),
        };
        if request.interval_seconds.is_some()
            && request.kind.as_deref() != Some("interval")
        {
            return ToolOutput::error(
                "`interval_seconds` requires `kind: \"interval\"`.",
            );
        }
        if request.cron_expr.is_some()
            && request.kind.as_deref() != Some("cron")
        {
            return ToolOutput::error("`cron_expr` requires `kind: \"cron\"`.");
        }
        let next_fire_at = match &kind {
            // A cron job's first fire belongs to the expression, not to
            // the caller: `first_after` is strictly after `now`, so the
            // row lands on a real occurrence instead of the one-minute
            // placeholder `advance` re-arms in a build without the
            // feature.
            #[cfg(feature = "cron")]
            JobKind::Cron { expr } => {
                match CronTrigger::first_after(expr, self.clock.now()) {
                    Ok(first) => first,
                    Err(error) => {
                        return ToolOutput::error(format!(
                            "`cron_expr` `{expr}` was refused: {error}."
                        ));
                    }
                }
            }
            _ => match request.first_fire_at.as_deref() {
                Some(raw) => match parse_instant(raw) {
                    Ok(instant) => instant,
                    Err(error) => return ToolOutput::error(error),
                },
                None => self.clock.now(),
            },
        };
        let mut job = Job::new(name, kind, payload, next_fire_at);
        if let Some(description) = request.description.as_deref() {
            job = job.with_description(description);
        }
        // Both refusals are checked before the write, and each names its
        // own remedy: `ScheduleStore::add` reports a duplicate name as a
        // generic `Io` error, which reads to the model as an infrastructure
        // failure rather than "pick another name".
        if self.store.has_name(name, None) {
            return ToolOutput::error(format!(
                "A job named `{name}` already exists — including a \
                 completed one, whose name stays taken. Use action `remove` \
                 on it first, or pick a different name."
            ));
        }
        if self.store.list().len() >= MAX_JOBS {
            return ToolOutput::error(format!(
                "The schedule already holds {MAX_JOBS} jobs, the maximum. \
                 Use action `list` and `remove` the jobs you no longer need."
            ));
        }
        if let Err(error) = self.store.add(job.clone()) {
            return store_error("create", &error);
        }
        let mut out = format!(
            "Scheduled `{}` ({}) as {} — next fire {} ({}).",
            job.name,
            job.id.as_str(),
            describe_kind(&job.kind),
            job.next_fire_at.to_rfc3339(),
            relative(self.clock.now(), job.next_fire_at),
        );
        out.push_str(
            "\nThe host's scheduler tick delivers it; this tool never \
             fires a job itself. `list` shows the schedule, `pause` / \
             `resume` / `remove` change it.",
        );
        ToolOutput::text(out)
    }

    /// Flip a job between `Active` and `Paused`.
    ///
    /// A `Completed` job is refused: the crate documents that state as
    /// terminal, and the two flips would each be a bug —
    /// `pause` makes the row un-reapable (the store's `gc_completed`
    /// sweep only collects `Completed` rows, so a paused ex-completed
    /// job leaks on disk forever), and `resume` sets `Active` while the
    /// row keeps its original past `next_fire_at`, so the next tick
    /// re-delivers a one-shot that already fired. Re-arming is
    /// `create`, and clearing is `remove`.
    fn set_status(
        &self,
        request: &ScheduleRequest,
        active: bool,
    ) -> ToolOutput {
        let action = if active { "resume" } else { "pause" };
        let Some(job) = self.resolve(request) else {
            return self.unresolved(request, action);
        };
        if job.status == JobStatus::Completed {
            return ToolOutput::error(format!(
                "`{}` ({}) already completed, which is final — `{action}` \
                 would not re-arm it. To schedule this work again, \
                 `remove` this row first (its name stays taken until you \
                 do) and then `create` it, or `create` under a different \
                 name.",
                job.name,
                job.id.as_str(),
            ));
        }
        let target = if active {
            JobStatus::Active
        } else {
            JobStatus::Paused
        };
        match self.store.update(&job.id, |row| row.status = target) {
            Ok(Some(snapshot)) => ToolOutput::text(format!(
                "`{}` ({}) is now {}.",
                snapshot.name,
                snapshot.id.as_str(),
                status_name(snapshot.status),
            )),
            Ok(None) => removed_under_us(&job),
            Err(error) => store_error(action, &error),
        }
    }

    /// Delete a job.
    fn remove(&self, request: &ScheduleRequest) -> ToolOutput {
        let Some(job) = self.resolve(request) else {
            return self.unresolved(request, "remove");
        };
        match self.store.remove(&job.id) {
            Ok(true) => ToolOutput::text(format!(
                "Removed `{}` ({}); its file is gone.",
                job.name,
                job.id.as_str(),
            )),
            Ok(false) => removed_under_us(&job),
            Err(error) => store_error("remove", &error),
        }
    }

    /// The job `job` names, by id first then by name.
    fn resolve(&self, request: &ScheduleRequest) -> Option<Job> {
        let selector = request.job.as_deref()?;
        self.store
            .list()
            .into_iter()
            .find(|job| job.id.as_str() == selector || job.name == selector)
    }

    /// Model-facing error for a `job` selector that matched nothing.
    fn unresolved(
        &self,
        request: &ScheduleRequest,
        action: &str,
    ) -> ToolOutput {
        let Some(selector) = request.job.as_deref() else {
            return missing_argument(action, "job");
        };
        let mut jobs = self.store.list();
        jobs.sort_by_key(|job| job.id.clone());
        let known: Vec<String> = jobs
            .iter()
            .map(|job| format!("{} ({})", job.name, job.id.as_str()))
            .collect();
        ToolOutput::error(format!(
            "No scheduled job with id or name `{selector}`. {}",
            if known.is_empty() {
                "The store has no jobs.".to_string()
            } else {
                format!("Known jobs: {}.", known.join(", "))
            }
        ))
    }
}

#[async_trait]
impl Tool for SchedulerTool {
    fn name(&self) -> &str {
        SCHEDULE_TOOL_NAME
    }

    fn description(&self) -> &str {
        #[cfg(feature = "cron")]
        let description = "Create and manage scheduled jobs: `list` shows \
         what is scheduled, `create` adds an `interval` (every N seconds), \
         `once` (single fire) or `cron` (5-field POSIX expression in \
         `cron_expr`) job, and `pause` / `resume` / `remove` change an \
         existing one by its id or name. A job carries a `payload` the \
         host delivers when it fires — recommended shape \
         {\"agent\": \"<peer>\", \"prompt\": \"<what to do>\"}. \
         Scheduling is not firing: the host's scheduler ticks and \
         delivers due jobs, so use this tool to *plan* recurring or \
         deferred work, not to wait for it. At most 64 jobs exist at \
         once; job names are unique, including completed ones.";
        #[cfg(not(feature = "cron"))]
        let description = "Create and manage scheduled jobs: `list` shows \
         what is scheduled, `create` adds an `interval` (every N seconds) \
         or `once` (single fire) job, and `pause` / `resume` / `remove` \
         change an existing one by its id or name. A job carries a \
         `payload` the host delivers when it fires — recommended shape \
         {\"agent\": \"<peer>\", \"prompt\": \"<what to do>\"}. \
         Scheduling is not firing: the host's scheduler ticks and \
         delivers due jobs, so use this tool to *plan* recurring or \
         deferred work, not to wait for it. At most 64 jobs exist at \
         once; job names are unique, including completed ones.";
        description
    }

    fn parameters(&self) -> Value {
        // Advertised only where `create` accepts it: a schema property
        // the build would refuse is a trap for the model. The insertion
        // below is unconditional — one schema literal for both builds,
        // no `mut`-suppression attribute and no duplicated literal.
        #[cfg(feature = "cron")]
        let cron_expr: Option<Value> = Some(json!({
            "type": "string",
            "description": "5-field POSIX cron expression \
                            (`m h dom mon dow`), e.g. `0 9 * * 1-5` \
                            for 09:00 on weekdays. Requires \
                            `kind: \"cron\"`."
        }));
        #[cfg(not(feature = "cron"))]
        let cron_expr: Option<Value> = None;
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ACTIONS,
                    "description": "`list` all jobs; `create` a new one; \
                                    `pause` / `resume` / `remove` an \
                                    existing one."
                },
                "job": {
                    "type": "string",
                    "description": "Job id or name, exactly as `list` or \
                                    a previous `create` reported it. \
                                    Required by `pause`, `resume` and \
                                    `remove`."
                },
                "name": {
                    "type": "string",
                    "description": "Unique job name. `create` only; \
                                    reused names are refused."
                },
                "kind": {
                    "type": "string",
                    "enum": KINDS,
                    "description": KIND_DESCRIPTION
                },
                "interval_seconds": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_INTERVAL_SECS,
                    "description": "Re-fire cadence in seconds; requires \
                                    `kind: \"interval\"`. At most \
                                    315360000 (10 years)."
                },
                "first_fire_at": {
                    "type": "string",
                    "description": "RFC 3339 timestamp of the first \
                                    fire, e.g. `2026-09-19T09:00:00Z`. \
                                    Defaults to now (fires on the host's \
                                    next tick)."
                },
                "payload": {
                    "type": "object",
                    "description": "What the host delivers when the job \
                                    fires. Recommended shape: \
                                    {\"agent\": \"<peer>\", \"prompt\": \
                                    \"<what to do>\"} — the delegation \
                                    vocabulary; `agent` may be omitted. \
                                    Defaults to {}."
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable note for operator \
                                    dashboards."
                }
            },
            "required": ["action"]
        });
        if let Some(property) = cron_expr {
            schema["properties"]["cron_expr"] = property;
        }
        schema
    }

    fn mode(&self) -> ExecutionMode {
        // The store serialises per-job writes with a file lock, but a
        // create-then-update batch read as two states is a real
        // ordering question; keep the calls in order.
        ExecutionMode::Sequential
    }

    /// R124: MCP-native descriptor hints. `schedule` mutates the
    /// persisted schedule store; the action surface (`list`,
    /// `create`, `pause`, `resume`, `remove`) is heterogeneous on
    /// idempotency (`create` adds a row; `pause`/`resume` toggle;
    /// `remove` deletes), so the conservative aggregate is
    /// `destructive = true, idempotent = false`. No outbound
    /// network (the scheduler host ticks the store on a
    /// `FixedClock`).
    fn annotations(&self) -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(true),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        })
    }

    fn output_definition(&self) -> ToolOutputDefinition {
        ToolOutputDefinition::passthrough(SCHEDULE_TOOL_NAME)
            .with_kind(RenderKind::Json)
            .with_title("Schedule")
    }

    async fn call(&self, input: Value, _context: &Context) -> ToolOutput {
        let request = match serde_json::from_value::<ScheduleRequest>(input) {
            Ok(request) => request,
            Err(error) => {
                return ToolOutput::error(format!(
                    "Invalid arguments: {error}"
                ));
            }
        };
        match request.action.as_str() {
            "list" => self.list(),
            "create" => self.create(&request),
            "pause" => self.set_status(&request, false),
            "resume" => self.set_status(&request, true),
            "remove" => self.remove(&request),
            other => ToolOutput::error(format!(
                "Unknown `action` `{other}`; expected one of {}.",
                ACTIONS.join(", "),
            )),
        }
    }
}

/// Render a job status for a model-facing message.
fn status_name(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Active => "active",
        JobStatus::Paused => "paused",
        JobStatus::Completed => "completed",
    }
}

/// Render a [`JobKind`] as the cadence it means.
fn describe_kind(kind: &JobKind) -> String {
    match kind {
        JobKind::Once => "once".to_string(),
        JobKind::Interval { interval } => {
            format!("every {}s", interval.as_secs())
        }
        JobKind::Cron { expr } => format!("cron `{expr}`"),
    }
}

/// "now" → human phrasing for a fire time, with past times named as
/// due rather than as a negative countdown.
fn relative(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
    let delta = at - now;
    if delta <= chrono::Duration::zero() {
        return "due".to_string();
    }
    let seconds = delta.num_seconds();
    if seconds < 60 {
        format!("in {seconds}s")
    } else if seconds < 3_600 {
        format!("in {}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("in {}h", seconds / 3_600)
    } else {
        format!("in {}d", seconds / 86_400)
    }
}

/// Parse an RFC 3339 instant, returning a model-facing message.
fn parse_instant(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|instant| instant.with_timezone(&Utc))
        .map_err(|error| {
            format!(
                "`first_fire_at` `{raw}` is not an RFC 3339 timestamp \
                 ({error}); e.g. `2026-09-19T09:00:00Z`."
            )
        })
}

/// Model-facing message for a required argument that was not supplied.
fn missing_argument(action: &str, argument: &str) -> ToolOutput {
    ToolOutput::error(format!("Action `{action}` requires `{argument}`."))
}

/// The job vanished between resolution and mutation — a real race with
/// another process, not a bug in the call.
fn removed_under_us(job: &Job) -> ToolOutput {
    ToolOutput::error(format!(
        "Job `{}` ({}) disappeared while the call was in flight; run \
         `list` to re-read the schedule.",
        job.name,
        job.id.as_str(),
    ))
}

/// Map a store failure onto a model-facing error.
fn store_error(action: &str, error: &ScheduleStoreError) -> ToolOutput {
    ToolOutput::error(format!("`{action}` failed: {error}"))
}

/// Publish the `schedule` tool into `registry`.
///
/// Returns `true` when the entry was inserted; `false` when a Core tool
/// already occupies the `schedule` name (the registry's immutability
/// guard refuses to overwrite built-in entries with the same name).
pub fn register_scheduler_tool(
    registry: &ToolRegistry,
    tool: SchedulerTool,
) -> bool {
    registry.register_entry(ToolEntry::new(Arc::new(tool)))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    #[test]
    fn relative_names_due_for_a_past_instant() {
        assert_eq!(relative(at(100), at(100)), "due");
        assert_eq!(relative(at(100), at(99)), "due");
    }

    #[test]
    fn relative_scales_through_seconds_minutes_hours_days() {
        assert_eq!(relative(at(0), at(30)), "in 30s");
        assert_eq!(relative(at(0), at(90)), "in 1m");
        assert_eq!(relative(at(0), at(7_200)), "in 2h");
        assert_eq!(relative(at(0), at(172_800)), "in 2d");
    }

    #[test]
    fn parse_instant_accepts_rfc3339_and_reports_the_rest() {
        assert_eq!(
            parse_instant("2026-09-19T09:00:00Z").expect("valid"),
            Utc.with_ymd_and_hms(2026, 9, 19, 9, 0, 0).unwrap(),
        );
        let error = parse_instant("tomorrow").expect_err("invalid");
        assert!(error.contains("not an RFC 3339 timestamp"), "{error}");
    }

    #[test]
    fn describe_kind_covers_every_variant() {
        assert_eq!(describe_kind(&JobKind::Once), "once");
        assert_eq!(
            describe_kind(&JobKind::Interval {
                interval: Duration::from_secs(30),
            }),
            "every 30s",
        );
        assert_eq!(
            describe_kind(&JobKind::Cron {
                expr: "0 9 * * *".into(),
            }),
            "cron `0 9 * * *`",
        );
    }

    #[test]
    fn status_names_are_lowercase() {
        assert_eq!(status_name(JobStatus::Active), "active");
        assert_eq!(status_name(JobStatus::Paused), "paused");
        assert_eq!(status_name(JobStatus::Completed), "completed");
    }

    /// R124: pin the MCP-native descriptor hints — destructive
    /// (writes the persisted schedule store), not idempotent
    /// (action surface is heterogeneous; the conservative default
    /// is `false`), no outbound network.
    #[test]
    fn annotations_are_mcp_native_destructive() {
        let tool = SchedulerTool::new(Arc::new(ScheduleStore::new(
            std::env::temp_dir().join("synthia-r124-anno"),
        )));
        let a = tool
            .annotations()
            .expect("schedule declares MCP-style annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
    }
}
