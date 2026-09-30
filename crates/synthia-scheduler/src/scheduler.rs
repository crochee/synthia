//! `Scheduler` — runtime-neutral dispatcher.
//!
//! The scheduler never owns a timer; it never spawns a task.
//! Every [`Scheduler::tick`] is driven by the caller, who
//! passes `now` (typically `chrono::Utc::now()`) and gets
//! back the [`ScheduleTick`] of jobs whose `next_fire_at`
//! has arrived. The caller then executes the firing on
//! whichever executor it uses (`tokio::spawn_local`,
//! `smol::spawn`, `std::thread::spawn`, …). This is the
//! same "callers arm their own timer" discipline the rest
//! of synthia's library crates follow — and is what makes
//! the crate usable from any runtime.
//!
//! ## Reentrancy
//!
//! `tick` borrows `&self` and returns a snapshot. The
//! in-memory `Job` cache is updated under a single
//! `parking_lot::Mutex`, so concurrent ticks from multiple
//! callers serialise on the lock but never deadlock
//! (the lock is never held across an await).
//!
//! ## The timing wheel
//!
//! `tick` does not scan the store: the scheduler keeps a
//! [`TimingWheel`] of the active jobs, so a
//! pass with nothing due costs a couple of hash lookups instead of a
//! visit per job. The wheel is an *index*, never a second source of
//! truth — [`Job::is_due`] still decides whether a candidate fires, and
//! every firing still goes through one `store.update`, so the row is
//! persisted exactly as before.
//!
//! The wheel is a **snapshot of the store**, taken by
//! [`Scheduler::new`] (the constructor reads no clock, so the first
//! `tick` anchors it) and rebuilt by `tick` itself when the host clock
//! moves backwards. A write that bypasses the scheduler — the
//! `schedule` tool's actions, an operator's own store handle, another
//! process's rows after a `load` — is picked up by the next `tick` (and
//! by `next_deadline` / `tick_dry_run`), which compares the store's
//! [`ScheduleStore::version`] against the value it last saw and rebuilds
//! when they differ. [`Scheduler::resync`] stays available as an
//! explicit rebuild for hosts that want the wheel reconciled *now*
//! rather than at the next pass.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use tracing::warn;

use crate::{
    job::{Job, JobId, JobStatus, ScheduleError},
    store::ScheduleStore,
    wheel::TimingWheel,
};

/// The cursor a wheel starts at when its owner has no clock to anchor it
/// with: [`Scheduler::new`] takes no `now`, so it builds the wheel here
/// and the first tick re-anchors it. `MIN_UTC` is unmistakable as a
/// sentinel — no job deadline can be earlier than the clock's origin.
const UNPRIMED: DateTime<Utc> = DateTime::<Utc>::MIN_UTC;

/// One second: the wheel's smallest step. A wheel anchored this far
/// *behind* `now` hands back everything already due on the single
/// `advance_to(now)` that follows, because the wheel collects
/// at-or-past-cursor entries on its coming step, not on its current one.
const PRIME_STEP: Duration = Duration::seconds(1);

/// Insert every active job `store` holds into `wheel`.
///
/// Only active jobs: a paused or completed row has no future fire to
/// wait on, and the wheel has no notion of status — whatever it holds is
/// a firing candidate.
fn insert_active_into(store: &ScheduleStore, wheel: &mut TimingWheel) {
    for job in store.list() {
        if job.status == JobStatus::Active {
            wheel.insert(job.next_fire_at, job.id);
        }
    }
}

/// One [`Scheduler::tick`] result — the set of jobs that
/// fired in this round and the bookkeeping the caller
/// needs to deliver them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScheduleTick {
    /// Jobs whose `next_fire_at` has arrived. Each entry
    /// carries the original `payload` so the caller can
    /// deliver it verbatim.
    pub fired: Vec<FiredJob>,
    /// Earliest `next_fire_at` across every still-active
    /// job. `None` when no jobs are scheduled. Callers use
    /// this to arm their timer — `next_deadline(now)` is
    /// the helper that takes clock skew into account.
    pub next_deadline: Option<DateTime<Utc>>,
}

impl ScheduleTick {
    /// True when no jobs fired and no future fire is
    /// queued. Callers can drop their timer in this case.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fired.is_empty() && self.next_deadline.is_none()
    }
}

/// One entry in a [`ScheduleTick::fired`] list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiredJob {
    pub id: JobId,
    pub name: String,
    pub payload: serde_json::Value,
}

/// Runtime-neutral dispatcher around a [`ScheduleStore`].
pub struct Scheduler {
    store: Arc<ScheduleStore>,
    /// Index of the active jobs by fire time — see the module docs for
    /// what it does and does not know about.
    wheel: Mutex<TimingWheel>,
    /// Serialises the compound operations on the wheel (a tick's
    /// advance, a rebuild) so two callers cannot interleave them.
    tick_lock: Mutex<()>,
    /// The store revision `wheel` was built from. Anything else in the
    /// store means a write went around this scheduler, and the next
    /// pass rebuilds before it answers. Read and written under
    /// `tick_lock`; atomic only so the check costs one load.
    last_seen_version: AtomicU64,
    /// How many times the wheel has been rebuilt from the store. Test
    /// instrumentation: the point of caching the revision is that a
    /// rebuild is *not* the normal path, and the only way to assert a
    /// pass did not take it is to count.
    #[cfg(test)]
    resyncs: AtomicU64,
}

impl Scheduler {
    /// Construct a scheduler backed by `store`.
    ///
    /// The handle is shared (`Arc`), not owned, because the store is
    /// the one thing a deployment must not duplicate: `list` serves
    /// from the store's in-memory cache, so a second `ScheduleStore`
    /// over the same root would answer with a stale view until it
    /// reloaded. Hand the same `Arc` to every component that reads or
    /// writes the schedule — a `SchedulerTool`, an operator endpoint —
    /// and they see one state. Against *other processes* the per-job
    /// file locks keep concurrent writes from clobbering each other,
    /// and every mutation re-reads under the lock; a peer's writes
    /// become visible to this process's reads at the next
    /// [`ScheduleStore::load`], and the pass after that (or an explicit
    /// [`Scheduler::resync`]) puts them in the wheel.
    ///
    /// The wheel is built from the store's active jobs here, at an
    /// unanchored cursor: a constructor that reads no clock cannot know
    /// where "now" is, so the *first* [`Scheduler::tick`] anchors it.
    /// That first tick is the only one that touches the whole schedule;
    /// afterwards the wheel tracks the caller's clock and a tick costs
    /// what has fallen due.
    #[must_use]
    pub fn new(store: Arc<ScheduleStore>) -> Self {
        // Read the revision *before* the snapshot below: a write that
        // lands between the two then looks newer than what the wheel
        // holds, so the next pass rebuilds — a redundant rebuild, never
        // a missed row. The other order could swallow it.
        let version = store.version();
        let mut wheel = TimingWheel::new(UNPRIMED);
        insert_active_into(&store, &mut wheel);
        Self {
            store,
            wheel: Mutex::new(wheel),
            tick_lock: Mutex::new(()),
            last_seen_version: AtomicU64::new(version),
            #[cfg(test)]
            resyncs: AtomicU64::new(0),
        }
    }

    /// Borrow the underlying store. Use this to
    /// `add` / `update` / `remove` jobs from operator code;
    /// those routes do their own locking and stay
    /// independent of the scheduler's tick state.
    #[must_use]
    pub fn store(&self) -> &ScheduleStore {
        &self.store
    }

    /// Drive one scheduling pass. Returns the jobs whose
    /// `next_fire_at <= now`, after advancing their
    /// `next_fire_at` and stamping `last_fired_at`.
    ///
    /// The bookkeeping is durable: every fired job is
    /// re-persisted to disk before `tick` returns, so a
    /// crash between `tick` and the caller's delivery does
    /// not lose the fire-record (the job simply fires
    /// again on the next tick — same semantics as
    /// `pi-subagents`).
    ///
    /// The work is proportional to the jobs the wheel hands back, not
    /// to the size of the schedule.
    pub fn tick(&self, now: DateTime<Utc>) -> ScheduleTick {
        let _guard = self.tick_lock.lock();
        let mut wheel = self.wheel.lock();
        self.sync_with_store_locked(&mut wheel, now);
        self.anchor_locked(&mut wheel, now);
        // The revision this pass starts from — after every rebuild
        // above, so it is the one the wheel reflects. `tick`'s own
        // store writes bump the counter mid-pass; leaving it there
        // would make every following tick see a change it did not make
        // and rebuild the whole wheel (O(n) per pass), so the pass
        // reports back exactly the revisions it produced itself.
        let base = self.last_seen_version.load(Ordering::Acquire);
        let mut own_writes = 0u64;
        let mut fired = Vec::new();
        // The wheel locates the candidates; the store still decides
        // which of them fire. `advance_to` has already dropped each
        // entry it hands over, so every candidate goes back into the
        // wheel below unless the store says it is done.
        for id in wheel.advance_to(now) {
            let Some(job) = self.store.get(&id) else {
                continue;
            };
            if !job.is_due(now) {
                // Stale entry: the wheel's snapshot says due, the store
                // says otherwise — a manual fire, a pause, or a direct
                // schedule edit landed between the two. Re-checking
                // `is_due` here is the whole point of the indirection;
                // re-arming keeps the job from dropping out of the
                // schedule entirely, which is what a bare discard would
                // do to it.
                self.rearm(&mut wheel, &job);
                continue;
            }
            // Each advance is a discrete per-job mutation,
            // so we route through the store's `update` to
            // keep the on-disk + cache view in lockstep.
            let mut advance_failed: Option<ScheduleError> = None;
            let advanced = self
                .store
                .update(&job.id, |j| {
                    // `is_due` already filtered on the
                    // pre-snapshot value. Re-check inside
                    // the lock so a manual fire that ran
                    // between snapshot and update does not
                    // double-fire us.
                    if !j.is_due(now) {
                        return;
                    }
                    // `advance` is total but fallible: an interval outside
                    // the representable range is reported, not panicked on
                    // (a panic here would abort the host's tick loop).
                    if let Err(err) = j.advance(now) {
                        // Park it. Leaving the job `Active` with its past
                        // `next_fire_at` would keep it permanently due, so
                        // `next_deadline` would report an elapsed deadline,
                        // the host would re-tick at once, and every pass
                        // would take the per-job file lock and rewrite this
                        // row — a disk-write hot loop, not just log noise.
                        // `Paused` is not due and drops out of
                        // `next_deadline`, which is what breaks the loop.
                        //
                        // Deliberately *not* `Completed`: that state means
                        // "this unit of work fired" and is terminal (the
                        // store's `gc_completed` sweep is the only thing
                        // that removes such rows, and the tool layer
                        // refuses to pause or resume it). This job ran
                        // nothing, so `list` must not report it as
                        // finished work. It stays visible and addressable
                        // instead: `remove` clears it, and `resume`
                        // re-attempts — which parks it again, since the
                        // stored interval is what cannot be represented.
                        j.status = JobStatus::Paused;
                        advance_failed = Some(err);
                    }
                })
                .ok()
                .flatten();
            if advanced.is_some() {
                // One persisted row, one revision.
                own_writes += 1;
            }
            if let Some(err) = advance_failed {
                // Do NOT report it as fired: the job did not advance, so
                // emitting a fire would claim a schedule change that did
                // not happen. It is now paused, so this is logged once
                // rather than on every tick.
                warn!(
                    job = %job.name,
                    job_id = %job.id,
                    error = %err,
                    "scheduled job could not be advanced; paused it instead of firing",
                );
                continue;
            }
            match advanced {
                Some(snapshot) if snapshot.status == JobStatus::Completed => {
                    // `Once` jobs fire and complete in the
                    // same tick. Nothing to come back for, so
                    // nothing goes back into the wheel.
                    fired.push(FiredJob {
                        id: snapshot.id,
                        name: snapshot.name,
                        payload: snapshot.payload,
                    });
                }
                Some(snapshot) => {
                    // Recurring: `advance` computed the next fire, the
                    // wheel is what remembers it.
                    wheel.insert(snapshot.next_fire_at, snapshot.id.clone());
                    fired.push(FiredJob {
                        id: snapshot.id,
                        name: snapshot.name,
                        payload: snapshot.payload,
                    });
                }
                None => {
                    // The closure's own re-check refused it (the row
                    // changed between the snapshot above and the
                    // update). The wheel already gave the entry up, so
                    // take the store's current word on where it belongs.
                    if let Some(current) = self.store.get(&id) {
                        self.rearm(&mut wheel, &current);
                    }
                }
            }
        }
        // Everything this pass persisted is now in the wheel, so the
        // wheel reflects `base + own_writes`. A write from anywhere else
        // that landed during the pass bumped the store past that — it is
        // not in the wheel, and the next pass rebuilds to pick it up.
        self.last_seen_version
            .store(base.wrapping_add(own_writes), Ordering::Release);
        ScheduleTick {
            fired,
            next_deadline: wheel.next_deadline(),
        }
    }

    /// Rebuild the wheel from the store *now*, instead of letting the
    /// next pass notice the change on its own.
    ///
    /// `tick`, `next_deadline`, and `tick_dry_run` already reconcile
    /// the wheel with the store before they answer, so this is not
    /// required for a write to become visible: a job the `schedule`
    /// tool created, a row an operator paused through their own
    /// [`ScheduleStore`] handle, or rows ingested by
    /// [`ScheduleStore::load`] are all picked up by the next pass. What
    /// this retains is the rebuild itself: forcing the wheel correct at
    /// an instant of the caller's choosing — an operator endpoint
    /// answering "what fires at T" — instead of waiting for the next
    /// pass to notice. `now` is the instant the rebuild is taken against
    /// (the same `now` the caller's next `tick` will use).
    ///
    /// Takes `&self`: the wheel has its own lock, so a scheduler
    /// already shared behind an `Arc` can be resynced in place.
    pub fn resync(&self, now: DateTime<Utc>) {
        let _guard = self.tick_lock.lock();
        let mut wheel = self.wheel.lock();
        self.resync_locked(&mut wheel, now);
    }

    /// Rebuild the wheel when the store has moved on without this
    /// scheduler: every mutating store route bumps its revision, so one
    /// comparison answers "is my snapshot still current?" in O(1) — a
    /// tick over an unchanged store never scans it. Normal operation,
    /// not an anomaly, so it logs nothing.
    ///
    /// Callers hold the tick lock and the wheel.
    fn sync_with_store_locked(
        &self,
        wheel: &mut TimingWheel,
        now: DateTime<Utc>,
    ) {
        if self.last_seen_version.load(Ordering::Acquire)
            != self.store.version()
        {
            self.resync_locked(wheel, now);
        }
    }

    /// The rebuild behind [`Scheduler::resync`], under the tick lock its
    /// callers hold.
    fn resync_locked(&self, wheel: &mut TimingWheel, now: DateTime<Utc>) {
        // Read the revision *before* the snapshot: a write landing
        // between the two then looks newer than the wheel, so the next
        // check rebuilds again (a redundant pass, never a missed row).
        let version = self.store.version();
        // Anchored a step behind `now` so that the `advance_to(now)` a
        // tick runs next still collects what this rebuild found due.
        *wheel = TimingWheel::new(now - PRIME_STEP);
        insert_active_into(&self.store, wheel);
        #[cfg(test)]
        self.resyncs.fetch_add(1, Ordering::Relaxed);
        self.last_seen_version.store(version, Ordering::Release);
    }

    /// Put the wheel's clock in step with `now` before a pass runs.
    ///
    /// The wheel steps forward one second at a time and never backward,
    /// so two states need this help: one that has never been anchored
    /// (see [`Scheduler::new`]), and one whose clock is *ahead* of `now`
    /// because the host clock was set back. Callers hold the tick lock
    /// and the wheel.
    fn anchor_locked(&self, wheel: &mut TimingWheel, now: DateTime<Utc>) {
        if wheel.cursor() == UNPRIMED {
            // Re-home the entries the wheel already holds. The store is
            // not re-read here: the caller has just reconciled the wheel
            // with it (`sync_with_store_locked`), and re-reading would
            // duplicate that at the cost of a second scan.
            wheel.reanchor(now - PRIME_STEP);
        } else if now < wheel.cursor() {
            // The clock moved backwards: the wheel is anchored to an
            // instant that has been undone, so its slots locate nothing
            // trustworthy. The store is the schedule's truth — rebuild
            // from it rather than guess.
            warn!(
                now = %now,
                wheel_cursor = %wheel.cursor(),
                "scheduler clock moved backwards; rebuilding the timing wheel",
            );
            self.resync_locked(wheel, now);
        }
    }

    /// Put a wheel candidate back on the deadline the store holds for
    /// it.
    ///
    /// `advance_to` removes what it hands over, so a candidate that the
    /// store no longer calls due has to be either re-armed or left out
    /// deliberately: dropping an *active* job's entry would mean it
    /// never fires again. A job that is no longer active is left out —
    /// only active jobs belong in the wheel.
    fn rearm(&self, wheel: &mut TimingWheel, job: &Job) {
        if job.status == JobStatus::Active {
            wheel.insert(job.next_fire_at, job.id.clone());
        }
    }

    /// Compute the wall-clock duration the caller should
    /// sleep before the next tick. Returns `None` when
    /// there is nothing scheduled. The returned duration
    /// is clamped at zero — a deadline that has already
    /// passed means "tick immediately".
    ///
    /// This is the helper callers use to arm `tokio::time`
    /// / `async_io::Timer` / `std::thread::sleep` from the
    /// scheduler's deadline. The scheduler itself never
    /// sleeps.
    #[must_use]
    pub fn next_deadline(
        &self,
        now: DateTime<Utc>,
    ) -> Option<std::time::Duration> {
        let tick = self.tick_dry_run(now);
        tick.next_deadline
            .map(|t| (t - now).to_std().unwrap_or(std::time::Duration::ZERO))
    }

    /// Read-only peek at the next tick — does not mutate job state.
    /// Useful for tests and for "when does anything fire next?"
    /// operator queries.
    ///
    /// It can move the *wheel* (never a job row): a peek that arrives
    /// after a direct write or a clock rollback has to rebuild it exactly
    /// as `tick` would, or the answer would come from slots that no
    /// longer describe the schedule.
    #[must_use]
    pub fn tick_dry_run(&self, now: DateTime<Utc>) -> ScheduleTick {
        let _guard = self.tick_lock.lock();
        let mut wheel = self.wheel.lock();
        self.sync_with_store_locked(&mut wheel, now);
        self.anchor_locked(&mut wheel, now);
        ScheduleTick {
            fired: Vec::new(),
            next_deadline: wheel.next_deadline(),
        }
    }

    /// How many times this scheduler has rebuilt its wheel from the
    /// store. Test instrumentation for the revision check: "a pass over
    /// an unchanged store does not rescan it" is only assertable by
    /// counting the rebuilds.
    #[cfg(test)]
    fn resync_count(&self) -> u64 {
        self.resyncs.load(Ordering::Relaxed)
    }

    /// The scan `tick` ran before the wheel: every active job whose
    /// `next_fire_at` has arrived, straight from the store.
    ///
    /// Retained as the equivalence oracle — the wheel is an index over
    /// the same schedule, and the tests hold the two to the same answer
    /// on every probe. Test-only so it cannot drift into a second
    /// production path.
    #[cfg(test)]
    fn linear_due(&self, now: DateTime<Utc>) -> Vec<Job> {
        self.store
            .list()
            .into_iter()
            .filter(|j| j.is_due(now))
            .collect()
    }
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler")
            .field("store", &self.store)
            .field("wheel_cursor", &self.wheel.lock().cursor())
            .field("tick_lock", &"<mutex>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc, time::Duration as StdDuration};

    use chrono::{Duration as ChronoDuration, TimeZone};
    use tempfile::TempDir;

    use super::*;
    use crate::job::{Job, JobKind};

    fn now_plus(secs: i64) -> DateTime<Utc> {
        Utc::now() + ChronoDuration::seconds(secs)
    }

    fn once_job(name: &str, secs: i64) -> Job {
        Job::new(
            name,
            JobKind::Once,
            serde_json::json!({"agent": name}),
            now_plus(secs),
        )
    }

    #[test]
    fn tick_emits_due_job_and_advances() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store.load().unwrap();
        let job = once_job("t-0", 0);
        store.add(job.clone()).unwrap();
        let sched = Scheduler::new(store);
        let tick = sched.tick(Utc::now());
        assert_eq!(tick.fired.len(), 1);
        assert_eq!(tick.fired[0].id, job.id);
        // `Once` jobs have no future fire.
        assert!(tick.next_deadline.is_none());
        // Cache must reflect `Completed` post-tick.
        let snapshot = sched.store().get(&job.id).unwrap();
        assert_eq!(snapshot.status, JobStatus::Completed);
    }

    #[test]
    fn interval_rolls_next_fire() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store.load().unwrap();
        let job = Job::new(
            "every-5s",
            JobKind::Interval {
                interval: StdDuration::from_secs(5),
            },
            serde_json::json!({}),
            now_plus(0),
        );
        store.add(job.clone()).unwrap();
        let sched = Scheduler::new(store);
        let tick = sched.tick(Utc::now());
        assert_eq!(tick.fired.len(), 1);
        let deadline = tick.next_deadline.expect("interval must rearm");
        let now = Utc::now();
        let delta = (deadline - now).num_seconds();
        // Accept any value in [3, 6] — wall-clock drift
        // between the two `now` reads is real.
        assert!((3..=6).contains(&delta), "expected ~5s rearm, got {delta}s");
    }

    #[test]
    fn paused_job_does_not_fire() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store.load().unwrap();
        let mut job = once_job("paused", -10);
        job.status = JobStatus::Paused;
        store.add(job).unwrap();
        let sched = Scheduler::new(store);
        let tick = sched.tick(Utc::now());
        assert!(tick.fired.is_empty());
    }

    #[test]
    fn next_deadline_helper_returns_zero_for_imminent() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store.load().unwrap();
        store.add(once_job("due", -1)).unwrap();
        let sched = Scheduler::new(store);
        let delta = sched.next_deadline(Utc::now()).unwrap();
        assert_eq!(delta, StdDuration::ZERO);
    }

    /// A job whose `advance` fails must not stay permanently due: that
    /// would make `next_deadline` report an elapsed deadline, the host
    /// would re-tick immediately, and every pass would take the file lock
    /// and rewrite the row — a disk-write hot loop.
    #[test]
    fn an_unadvanceable_job_is_parked_not_left_due() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store.load().unwrap();
        // Pinned, so the test is deterministic (and adds no wall-clock
        // read to the `check-clock` ratchet).
        let now = DateTime::parse_from_rfc3339("2026-09-19T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // The library API accepts this; `advance` cannot represent it.
        let job = Job::new(
            "hostile",
            JobKind::Interval {
                interval: StdDuration::from_secs(u64::MAX),
            },
            serde_json::json!({}),
            now,
        );
        store.add(job.clone()).unwrap();
        let sched = Scheduler::new(Arc::clone(&store));

        let tick = sched.tick(now);
        assert!(tick.fired.is_empty(), "a failed advance is not a firing");
        assert_eq!(
            tick.next_deadline, None,
            "a parked job must not contribute a deadline"
        );
        assert_eq!(
            sched.next_deadline(now),
            None,
            "so the host has nothing to arm — no busy loop"
        );

        // Paused, not Completed. It is not merely cosmetic: `Completed`
        // means "this unit of work fired" and is terminal, whereas this
        // job ran nothing. The parking also keeps the row *addressable*
        // (`remove` clears it, `resume` re-attempts) instead of handing it
        // to a sweep that exists to retire finished work.
        let parked = sched.store().get(&job.id).expect("job");
        assert_eq!(parked.status, JobStatus::Paused);
        assert_eq!(
            parked.last_fired_at, None,
            "a job that never fired must not carry a fire time"
        );
        assert!(
            sched
                .store()
                .gc_completed(now + ChronoDuration::days(365))
                .unwrap()
                .is_empty(),
            "a parked job must not be swept as completed"
        );

        // And a second tick is quiet — the log line is once, not per pass.
        assert!(sched.tick(now).fired.is_empty());
        assert_eq!(sched.next_deadline(now), None);
    }

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    /// The wheel lives behind the scheduler, so a host that shares one
    /// `Arc<Scheduler>` across threads must keep working — that is the
    /// compatibility the integration promised.
    #[test]
    fn scheduler_is_send_and_sync() {
        fn _assert<T: Send + Sync>() {}
        _assert::<Scheduler>();
    }

    /// The tool's primary flow: `create` stamps `next_fire_at = now` and
    /// the host ticks a second later. The wheel is then anchored a second
    /// behind that deadline, so the entry sits exactly at the cursor —
    /// the boundary where "the deadline has arrived" must not turn into
    /// "wait for the slot lap". Both anchor sites (`new` → first tick,
    /// and `resync`) go through it.
    #[test]
    fn a_deadline_one_step_behind_now_fires_now() {
        for resynced in [false, true] {
            let dir = TempDir::new().unwrap();
            let store = Arc::new(ScheduleStore::new(dir.path()));
            store
                .add(Job::new(
                    "created-this-second",
                    JobKind::Once,
                    serde_json::json!({}),
                    at(60),
                ))
                .unwrap();
            let sched = Scheduler::new(Arc::clone(&store));
            let now = at(61);
            if resynced {
                sched.resync(now);
            }
            let linear: HashSet<String> = sched
                .linear_due(now)
                .into_iter()
                .map(|j| j.id.to_string())
                .collect();
            let tick = sched.tick(now);
            let via_wheel: HashSet<String> =
                tick.fired.iter().map(|f| f.id.to_string()).collect();
            assert_eq!(linear, via_wheel, "resynced={resynced}");
            assert_eq!(via_wheel.len(), 1, "resynced={resynced}");
            assert_eq!(
                tick.next_deadline, None,
                "a fired one-shot must not leave a past deadline armed \
                 (resynced={resynced})",
            );
        }
    }

    /// `linear_due` is the oracle for this shape too: a deadline that has
    /// already elapsed when the tick runs is due, and the wheel's cursor is
    /// anchored one step behind `now` — the same boundary from the other
    /// side, plus the store's answer to "is it still due".
    #[test]
    fn an_elapsed_deadline_fires_and_matches_the_oracle() {
        for probe in [7i64, 8, 9] {
            let dir = TempDir::new().unwrap();
            let store = Arc::new(ScheduleStore::new(dir.path()));
            store
                .add(Job::new(
                    "elapsed",
                    JobKind::Interval {
                        interval: StdDuration::from_secs(3600),
                    },
                    serde_json::json!({}),
                    at(8),
                ))
                .unwrap();
            let sched = Scheduler::new(Arc::clone(&store));
            let now = at(probe);
            let linear = sched.linear_due(now).len();
            let via_wheel = sched.tick(now).fired.len();
            assert_eq!(linear, via_wheel, "probe t={probe}");
        }
    }

    /// A tick that arrives exactly where the previous anchor left the
    /// wheel's cursor: no second to step over, and yet the schedule is
    /// not empty — the store holds a job that is due *now*. It must
    /// fire, and (the hazard the deadline report feeds) the caller must
    /// not be handed a past deadline, which would arm a zero-length
    /// timer and spin the host against a tick that can never progress.
    #[test]
    fn a_zero_step_tick_delivers_what_is_due() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store
            .add(Job::new(
                "due-now",
                JobKind::Once,
                serde_json::json!({}),
                at(59),
            ))
            .unwrap();
        let sched = Scheduler::new(Arc::clone(&store));
        // `resync` anchors the cursor at `now - 1` and files the job at
        // exactly that second; the tick then lands on it with nothing to
        // step over.
        sched.resync(at(60));
        let tick = sched.tick(at(59));
        assert_eq!(tick.fired.len(), 1, "a due job must not wait for a step");
        assert_eq!(
            tick.next_deadline, None,
            "and no elapsed deadline may be left armed"
        );
        assert_eq!(sched.next_deadline(at(59)), None);
    }

    /// The tool's `create` without `first_fire_at` stamps
    /// `SystemClock`'s fractional now, so a real deployment's very first
    /// deadline is fractional. The wheel must not sit on it while
    /// `next_deadline` reports an elapsed instant (which arms a
    /// zero-length timer and spins the host) — the whole schedule would
    /// stall for a lap.
    #[test]
    fn a_fractional_creation_time_does_not_strand_the_job() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        // 09:00:00.400 "now", created with next_fire_at 10.9s out.
        let created_at = at(0) + ChronoDuration::milliseconds(400);
        store
            .add(Job::new(
                "fractional",
                JobKind::Once,
                serde_json::json!({}),
                created_at + ChronoDuration::milliseconds(10_500),
            ))
            .unwrap();
        let sched = Scheduler::new(Arc::clone(&store));

        // Whole-second host ticks, as a real host would drive them.
        let deadline = created_at + ChronoDuration::milliseconds(10_500);
        let mut fired_at = None;
        for secs in 1..=20i64 {
            let now = at(secs);
            if !sched.tick(now).fired.is_empty() {
                fired_at = Some(now);
                break;
            }
            assert_eq!(
                sched.next_deadline(now),
                (deadline - now).to_std().ok(),
                "the armed timer must point at the deadline, never at a \
                 past instant (a zero timer spins the host) — t={secs}",
            );
        }
        assert_eq!(
            fired_at,
            Some(at(11)),
            "the job fires at the first whole second past its deadline"
        );
    }

    /// The wheel is an index over the same schedule, not a second
    /// scheduler: on a fixed mix of intervals and one-shots, every probe
    /// has to report exactly the jobs the retained linear scan
    /// (`linear_due`, this file's pre-wheel implementation) calls due.
    /// Set equality is the assertion because the wheel walks slots, not
    /// the store's id order.
    #[test]
    fn wheel_tick_matches_linear_scan_on_fixed_schedule() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        for i in 0..10u64 {
            store
                .add(Job::new(
                    format!("ivl-{i}"),
                    JobKind::Interval {
                        interval: StdDuration::from_secs(5 + i * 5),
                    },
                    serde_json::json!({"i": i}),
                    at(0),
                ))
                .unwrap();
        }
        for i in 0..10u64 {
            store
                .add(Job::new(
                    format!("once-{i}"),
                    JobKind::Once,
                    serde_json::json!({"i": i}),
                    at(10 + i as i64 * 7),
                ))
                .unwrap();
        }
        let sched = Scheduler::new(Arc::clone(&store));
        for probe in [0i64, 7, 30, 60, 100, 200] {
            let now = at(probe);
            let linear: HashSet<String> = sched
                .linear_due(now)
                .into_iter()
                .map(|j| j.id.to_string())
                .collect();
            let via_wheel: HashSet<String> = sched
                .tick(now)
                .fired
                .iter()
                .map(|f| f.id.to_string())
                .collect();
            assert_eq!(linear, via_wheel, "probe t={probe}");
        }
    }

    #[test]
    fn next_deadline_matches_full_scan_minimum() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        for (name, t) in [("a", 30i64), ("b", 10), ("c", 70)] {
            store
                .add(Job::new(
                    name,
                    JobKind::Once,
                    serde_json::json!({}),
                    at(t),
                ))
                .unwrap();
        }
        let sched = Scheduler::new(store);
        let expected = sched
            .store()
            .list()
            .into_iter()
            .filter(|j| j.status == JobStatus::Active)
            .map(|j| j.next_fire_at)
            .min();
        assert_eq!(sched.tick_dry_run(at(0)).next_deadline, expected);
        assert_eq!(sched.next_deadline(at(0)).map(|d| d.as_secs()), Some(10));
    }

    #[test]
    fn backward_tick_rebuilds_wheel_and_keeps_firing() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store
            .add(Job::new("o", JobKind::Once, serde_json::json!({}), at(50)))
            .unwrap();
        let sched = Scheduler::new(store);
        // Forward past 50s: the one-shot fires and completes.
        assert_eq!(sched.tick(at(60)).fired.len(), 1);
        // A tick behind the wheel's cursor must rebuild, not re-fire.
        assert!(
            sched.tick(at(30)).fired.is_empty(),
            "rollback must not replay"
        );
        // Nor does the rewind resurrect it once time moves on again.
        assert!(sched.tick(at(90)).fired.is_empty());
    }

    /// The wheel follows the store. A write that bypasses the
    /// scheduler — the `schedule` tool's, an operator's own store
    /// handle — is visible to the *next* pass, with no manual rebuild:
    /// the store's revision is what tells the scheduler its snapshot is
    /// stale.
    ///
    /// The wheel held nothing when that write landed (the job was added
    /// after `new`), so this only passes if the pass reconciles with the
    /// store before it answers — the read R117's wheel traded for O(1)
    /// ticks, restored without giving the O(1) back.
    #[test]
    fn a_direct_store_write_is_visible_to_the_next_tick() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        let sched = Scheduler::new(Arc::clone(&store));
        store
            .add(Job::new(
                "late",
                JobKind::Once,
                serde_json::json!({}),
                at(40),
            ))
            .unwrap();
        let tick = sched.tick(at(45));
        assert_eq!(tick.fired.len(), 1, "the next tick must see the write");
        assert_eq!(tick.fired[0].name, "late");
    }

    /// The peek paths read the same wheel, so they reconcile first too:
    /// a host that arms its timer from `next_deadline` between ticks must
    /// not be told "nothing scheduled" for a job that was just created.
    #[test]
    fn a_direct_store_write_moves_the_peeked_deadline() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        let sched = Scheduler::new(Arc::clone(&store));
        assert_eq!(sched.tick_dry_run(at(0)).next_deadline, None);

        store
            .add(Job::new(
                "later",
                JobKind::Once,
                serde_json::json!({}),
                at(500),
            ))
            .unwrap();
        assert_eq!(sched.tick_dry_run(at(0)).next_deadline, Some(at(500)));
        assert_eq!(
            sched.next_deadline(at(0)),
            Some(StdDuration::from_secs(500))
        );
    }

    /// The revision is a *hint*, not a duty to rescan: a pass whose own
    /// `store.update` calls advanced it must not make the next pass
    /// rebuild. Getting this wrong is invisible in the results — every
    /// tick still fires the right jobs — and shows up only as an O(n)
    /// scan per pass, so it is asserted by counting rebuilds.
    #[test]
    fn a_ticks_own_writes_do_not_trigger_a_resync() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(ScheduleStore::new(dir.path()));
        store
            .add(Job::new(
                "hourly",
                JobKind::Interval {
                    interval: StdDuration::from_secs(3600),
                },
                serde_json::json!({}),
                at(0),
            ))
            .unwrap();
        let sched = Scheduler::new(Arc::clone(&store));
        // Built after the `add`, so the first pass starts in step.
        assert_eq!(sched.resync_count(), 0);

        // Fires and re-arms: one row persisted, one revision bumped.
        assert_eq!(sched.tick(at(0)).fired.len(), 1);
        assert_eq!(
            sched.resync_count(),
            0,
            "a pass must not rebuild the wheel it just built"
        );

        // And the *next* pass does not mistake that write for someone
        // else's: it fires the re-armed job and still does not rebuild.
        assert_eq!(sched.tick(at(3600)).fired.len(), 1);
        assert_eq!(
            sched.resync_count(),
            0,
            "the write-back is what keeps this at zero"
        );
        assert_eq!(
            sched.tick_dry_run(at(7200)).next_deadline,
            Some(at(7200)),
            "the re-armed job is in the wheel, from the pass that armed it"
        );
        assert_eq!(sched.resync_count(), 0, "nor do the peek paths rebuild");
    }
}
