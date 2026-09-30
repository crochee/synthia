//! `TimingWheel` — hierarchical timing wheel for due-job lookup.
//!
//! A pure data structure: no locks, no threads, no timers. The
//! caller drives it with injected `DateTime<Utc>` values (the
//! `Scheduler` host `tick`), which keeps it testable with a
//! fixed clock and free of any runtime dependency.
//!
//! Design (see the wheel-cron design doc §2):
//!
//! - **4 levels × 64 slots.** Level `i` has granularity
//!   `64^i` seconds (1s / 64s / 4096s / 262144s), so the wheel
//!   spans `64^4` seconds (~194 days) — beyond any real
//!   scheduling horizon. A `BTreeMap` overflow tail exists
//!   purely as defense.
//! - **Entries carry their own deadline as ground truth.** The
//!   slot placement only locates candidates; firing is decided
//!   by `deadline <= now`, which sidesteps every boundary
//!   alignment error a slot-only scheme would invite.
//! - **Cascading.** When the cursor crosses a level-`i` lap
//!   boundary, the level-`i` slot it lands on is demoted entry
//!   by entry into lower levels via the general insert path, so
//!   every entry settles into level 0 no later than its own
//!   deadline.
//! - **Backward advance is a noop.** Clock rollback is the
//!   caller's problem (the `Scheduler` rebuilds the wheel).

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Duration, Utc};

use crate::job::JobId;

/// Number of hierarchy levels.
const LEVELS: usize = 4;
/// Slots per level.
const SLOTS: i64 = 64;
/// Total wheel span in seconds (`64^4`, ~194 days). Inserts
/// beyond it go to the overflow tail; stepping is capped at it.
const SPAN_SECS: i64 = 16_777_216;
/// Sentinel `index_of` level for entries parked in `overflow`.
const OVERFLOW: usize = LEVELS;

/// Granularity of `level` in seconds: `64^level`.
const fn granularity(level: usize) -> i64 {
    64_i64.pow(level as u32)
}

/// One slot's pending entries, deadline truth included.
type Slot = Vec<(JobId, DateTime<Utc>)>;
/// Hierarchical timing wheel of pending [`JobId`]s, driven
/// entirely by caller-injected timestamps.
///
/// Entries live as `(JobId, deadline)` pairs; the slot math
/// only decides *where to look*, the embedded deadline decides
/// *whether to fire*.
pub struct TimingWheel {
    /// `levels[i][slot]` holds every entry whose deadline maps
    /// to `(deadline / 64^i) % 64` while its distance from the
    /// cursor still belongs to level `i`.
    levels: [Vec<Slot>; LEVELS],
    /// The wheel's notion of "now"; only moves forward.
    cursor: DateTime<Utc>,
    /// `id -> (level, slot)` so cancel/re-insert never scans
    /// the wheel. Level `OVERFLOW` marks overflow entries.
    index_of: HashMap<JobId, (usize, usize)>,
    /// Deadlines beyond the wheel span, keyed by deadline.
    /// Defensive only — the span (~194 days) exceeds every
    /// real scheduling horizon.
    overflow: BTreeMap<DateTime<Utc>, Vec<JobId>>,
}

impl TimingWheel {
    /// Build an empty wheel whose cursor starts at `now`.
    #[must_use]
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            levels: std::array::from_fn(|_| vec![Vec::new(); SLOTS as usize]),
            cursor: now,
            index_of: HashMap::new(),
            overflow: BTreeMap::new(),
        }
    }

    /// Schedule `id` to fire at `deadline`.
    ///
    /// The contract is `deadline > cursor` (the `Scheduler`
    /// guarantees `next_fire_at > now`); an at/past deadline is
    /// still accepted defensively and fires at the next 1s
    /// step. Re-inserting an id already pending replaces its
    /// old entry so no ghost copy can fire later.
    pub fn insert(&mut self, deadline: DateTime<Utc>, id: JobId) {
        self.remove_indexed(&id);
        self.insert_internal(deadline, id);
    }

    /// Remove a pending `id`. Returns `false` if it was not in
    /// the wheel.
    pub fn cancel(&mut self, id: &JobId) -> bool {
        self.remove_indexed(id)
    }

    /// Move the cursor to `now` (stepping 1s at a time to keep
    /// the cascade boundaries honest) and return every pending
    /// entry whose deadline is `<= now`.
    ///
    /// `now` earlier than the cursor is a noop: the caller
    /// owns rollback recovery.
    ///
    /// One call advances the cursor by at most one wheel span
    /// (`64^4` seconds, ~194 days): a jump larger than that is
    /// walked in span-sized pieces across calls rather than in one
    /// step. Entries due on the other side are still returned by the
    /// first of those calls (the overflow tail is swept by deadline,
    /// not by stepping).
    pub fn advance_to(&mut self, now: DateTime<Utc>) -> Vec<JobId> {
        if now < self.cursor {
            return Vec::new();
        }
        // Stepping is capped at one wheel span; beyond that
        // (theoretically impossible with real horizons) the
        // overflow tail is drained as a fallback.
        let limit =
            std::cmp::min(now, self.cursor + Duration::seconds(SPAN_SECS));
        let step = Duration::seconds(1);
        let mut fired = Vec::new();
        while self.cursor + step <= limit {
            self.cursor += step;
            let secs = self.cursor.timestamp();
            // Lap crossings: demote level i's landed slot into
            // level i-1, highest level first so a boundary hit
            // at several levels settles in one pass.
            for level in (1..LEVELS).rev() {
                let gran = granularity(level);
                if secs.rem_euclid(gran) == 0 {
                    let slot = secs.div_euclid(gran).rem_euclid(SLOTS) as usize;
                    let demoted = std::mem::take(&mut self.levels[level][slot]);
                    for (id, deadline) in demoted {
                        self.insert_internal(deadline, id);
                    }
                }
            }
            // Collect level-0: the slot locates candidates, the
            // embedded deadline is the firing truth.
            let slot = secs.rem_euclid(SLOTS) as usize;
            fired.extend(self.take_due_from_slot(slot, now));
        }
        // The walk only ever collects the slots it steps onto, and a call
        // whose `now` *is* the cursor takes no step at all. But entries
        // that are already due park in the slot the coming step would
        // visit (see `insert_internal`), so sweeping it here is what makes
        // the promise above hold for every call: nothing with
        // `deadline <= now` is left behind, and the caller is never handed
        // a deadline it cannot collect. Future entries stay put (the sweep
        // partitions on the same embedded deadline the steps do).
        let coming = (self.cursor.timestamp() + 1).rem_euclid(SLOTS) as usize;
        fired.extend(self.take_due_from_slot(coming, now));
        // Overflow fallback: with the step cap hit (theoretical
        // only — the span exceeds every real horizon) or with
        // entries left over from an earlier capped walk, sweep
        // everything due out of the tail. A no-op whenever the
        // overflow is empty or not yet due.
        while let Some((&deadline, _)) = self.overflow.first_key_value() {
            if deadline > now {
                break;
            }
            if let Some(ids) = self.overflow.remove(&deadline) {
                for id in ids {
                    self.index_of.remove(&id);
                    fired.push(id);
                }
            }
        }
        fired
    }

    /// Move the cursor to `cursor` outright, re-homing every pending
    /// entry relative to the new position.
    ///
    /// [`advance_to`](Self::advance_to) walks one second at a time and
    /// refuses to move backward — that is what keeps the cascade
    /// boundaries honest — so a wheel whose cursor is out of step with
    /// the caller's clock cannot be walked to it: from a cursor that far
    /// behind, the step cap alone costs a full wheel span per call. This
    /// is the way out. It is deliberately *not* a firing operation:
    /// nothing is collected, and an entry at or past the new cursor
    /// simply stays due (it lands in the slot the coming step visits).
    ///
    /// The `Scheduler` needs it for the wheel it builds before it has a
    /// clock: a constructor that reads no clock cannot anchor the cursor
    /// at its caller's "now", so the first tick re-anchors it here.
    pub(crate) fn reanchor(&mut self, cursor: DateTime<Utc>) {
        let mut pending: Vec<(JobId, DateTime<Utc>)> =
            Vec::with_capacity(self.len());
        for level in &mut self.levels {
            for slot in level.iter_mut() {
                pending.append(&mut std::mem::take(slot));
            }
        }
        for (deadline, ids) in std::mem::take(&mut self.overflow) {
            pending.extend(ids.into_iter().map(|id| (id, deadline)));
        }
        self.index_of.clear();
        self.cursor = cursor;
        for (id, deadline) in pending {
            // The general insert path: ids are unique (the index
            // guaranteed it), so nothing needs replacing here.
            self.insert_internal(deadline, id);
        }
    }

    /// Earliest pending deadline, looking at most 64 slots per
    /// level (O(256), queries only) plus the overflow head.
    #[must_use]
    pub fn next_deadline(&self) -> Option<DateTime<Utc>> {
        let mut best = self.overflow.keys().next().copied();
        let secs = self.cursor.timestamp();
        for level in 0..LEVELS {
            let start = secs.div_euclid(granularity(level)).rem_euclid(SLOTS);
            // Nearest non-empty slot *the cursor has yet to reach*. The
            // slot it is sitting on has just been serviced, so a deadline
            // filed there (a lap tail — see `insert_internal`) is the last
            // this level reaches, not the first: starting the scan on it
            // reported a deadline a lap away ahead of one a few seconds
            // out, and the host, arming its timer from this answer, ran
            // the near job late. Scanning from the coming slot keeps the
            // order monotone in deadline and still reaches the passed slot
            // as the final `off`, so a genuinely stale entry stays visible.
            for off in 0..SLOTS {
                let slot = (start + 1 + off).rem_euclid(SLOTS) as usize;
                if let Some(min) =
                    self.levels[level][slot].iter().map(|(_, d)| *d).min()
                {
                    if best.is_none_or(|b| min < b) {
                        best = Some(min);
                    }
                    break;
                }
            }
        }
        best
    }

    /// The wheel's current time (the caller uses this to detect
    /// clock rollback).
    #[must_use]
    pub fn cursor(&self) -> DateTime<Utc> {
        self.cursor
    }

    /// Number of pending entries (wheel + overflow).
    #[must_use]
    pub fn len(&self) -> usize {
        self.levels
            .iter()
            .flat_map(|level| level.iter())
            .map(Vec::len)
            .sum::<usize>()
            + self.overflow.values().map(Vec::len).sum::<usize>()
    }

    /// Whether no entry is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Collect and return every entry in level-0 `slot` whose deadline has
    /// arrived; future entries stay where they are.
    ///
    /// Shared by the stepping walk and the post-walk sweep so both decide
    /// firing the same way: the slot locates candidates, the embedded
    /// deadline is the truth.
    fn take_due_from_slot(
        &mut self,
        slot: usize,
        now: DateTime<Utc>,
    ) -> Vec<JobId> {
        if self.levels[0][slot].is_empty() {
            return Vec::new();
        }
        let entries = std::mem::take(&mut self.levels[0][slot]);
        let (keep, due): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|(_, d)| *d > now);
        for (id, _) in &due {
            self.index_of.remove(id);
        }
        self.levels[0][slot] = keep;
        due.into_iter().map(|(id, _)| id).collect()
    }

    /// Place one entry by its distance from the cursor:
    /// `level = floor(log64(delta))`, slot by the deadline's
    /// own time. Shared by `insert` and cascade demotion.
    fn insert_internal(&mut self, deadline: DateTime<Utc>, id: JobId) {
        let delta = deadline - self.cursor;
        let level = match delta.num_seconds() {
            // At/past the cursor (or a sub-second remainder; the
            // seconds are truncated, so `0` covers both ends of the
            // boundary): park in the next level-0 slot so it fires at
            // the coming step. The cursor's *own* slot is not visited —
            // stepping moves to `cursor + 1` — so an entry left there
            // would wait for the 64-slot lap to wrap.
            ..=0 => {
                let slot =
                    (self.cursor.timestamp() + 1).rem_euclid(SLOTS) as usize;
                self.levels[0][slot].push((id.clone(), deadline));
                self.index_of.insert(id, (0, slot));
                return;
            }
            secs if secs < granularity(1) => 0,
            secs if secs < granularity(2) => 1,
            secs if secs < granularity(3) => 2,
            secs if secs < SPAN_SECS => 3,
            _ => OVERFLOW,
        };
        if level == OVERFLOW {
            self.overflow.entry(deadline).or_default().push(id.clone());
            self.index_of.insert(id, (OVERFLOW, 0));
            return;
        }
        // The cursor visits a level-0 slot at `second + cursor_fraction`,
        // so an entry filed under its own truncated second is inspected
        // just *before* it is due whenever its fraction is larger — and
        // then waits a full lap (~64 s) for the next visit. File it under
        // the first second whose visit is at or past the deadline.
        let due_second = deadline.timestamp()
            + i64::from(
                deadline.timestamp_subsec_nanos()
                    > self.cursor.timestamp_subsec_nanos(),
            );
        let slot = due_second.div_euclid(granularity(level)).rem_euclid(SLOTS)
            as usize;
        self.levels[level][slot].push((id.clone(), deadline));
        self.index_of.insert(id, (level, slot));
    }

    /// Drop `id` wherever it lives. Returns whether it was
    /// pending.
    fn remove_indexed(&mut self, id: &JobId) -> bool {
        let Some((level, slot)) = self.index_of.remove(id) else {
            return false;
        };
        if level == OVERFLOW {
            let key = self
                .overflow
                .iter()
                .find(|(_, ids)| ids.iter().any(|e| e == id))
                .map(|(k, _)| *k);
            if let Some(key) = key {
                let drained = self
                    .overflow
                    .get_mut(&key)
                    .map(|ids| {
                        ids.retain(|e| e != id);
                        ids.is_empty()
                    })
                    .unwrap_or(false);
                if drained {
                    self.overflow.remove(&key);
                }
            }
        } else {
            self.levels[level][slot].retain(|(eid, _)| eid != id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};

    use crate::job::JobId;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }
    /// A fractional instant — the shape `SystemClock` hands the scheduler.
    fn at_ms(secs: i64, millis: u32) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, millis * 1_000_000)
            .single()
            .unwrap()
    }
    fn id(n: u64) -> JobId {
        JobId::from_string(format!("j{n}"))
    }

    #[test]
    fn fires_in_slot_order_and_reports_next_deadline() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(3), id(1));
        w.insert(at(9), id(2));
        assert_eq!(w.next_deadline(), Some(at(3)));
        assert_eq!(w.len(), 2);
        let fired = w.advance_to(at(5));
        assert_eq!(fired, vec![id(1)]);
        assert_eq!(w.next_deadline(), Some(at(9)));
        assert_eq!(w.advance_to(at(9)), vec![id(2)]);
        assert_eq!(w.next_deadline(), None);
    }

    #[test]
    fn cancel_prevents_firing() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(10), id(1));
        assert!(w.cancel(&id(1)));
        assert!(!w.cancel(&id(1)));
        assert!(w.advance_to(at(20)).is_empty());
        assert_eq!(w.next_deadline(), None);
    }

    #[test]
    fn same_second_slot_fires_together() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(7), id(1));
        w.insert(at(7), id(2));
        let mut fired = w.advance_to(at(7));
        fired.sort();
        assert_eq!(fired.len(), 2);
    }

    #[test]
    fn long_delay_cascades_down_correctly() {
        // 4097s 落在层 2；到 4097s 时必须经级联落到层 0 后如期收走
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(4097), id(1));
        assert_eq!(w.next_deadline(), Some(at(4097)));
        assert_eq!(w.advance_to(at(4100)), vec![id(1)]);
    }

    #[test]
    fn past_deadline_at_advance_collects_immediately() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(1), id(1));
        // 一次大步进（含边界）
        assert_eq!(w.advance_to(at(1)), vec![id(1)]);
    }

    #[test]
    fn backward_advance_is_noop() {
        let mut w = super::TimingWheel::new(at(100));
        w.insert(at(200), id(1));
        assert!(w.advance_to(at(50)).is_empty());
        assert_eq!(w.next_deadline(), Some(at(200)));
        assert_eq!(w.len(), 1);
    }

    /// The `Scheduler` builds its wheel before it has a clock, so the
    /// first tick has to move a cursor that starts far in the past onto
    /// `now` — `advance_to` cannot walk there.
    #[test]
    fn reanchor_moves_the_cursor_and_keeps_every_entry() {
        let mut w = super::TimingWheel::new(DateTime::<Utc>::MIN_UTC);
        w.insert(at(10), id(1));
        w.insert(at(4097), id(2));
        assert_eq!(w.len(), 2);

        w.reanchor(at(9));
        assert_eq!(w.cursor(), at(9));
        assert_eq!(
            w.next_deadline(),
            Some(at(10)),
            "the deadlines are the entries' own, not the slots'"
        );
        // An entry at or past the new cursor stays due: it lands in the
        // slot the coming step visits, so the move loses nothing.
        assert_eq!(w.advance_to(at(10)), vec![id(1)]);
        assert_eq!(w.advance_to(at(4100)), vec![id(2)]);
        assert_eq!(w.len(), 0);
    }

    #[test]
    fn reanchor_rebuilds_the_id_index() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(100), id(1));
        w.reanchor(at(50));
        assert!(w.cancel(&id(1)), "the index must survive a re-anchor");
        assert!(w.advance_to(at(200)).is_empty());
    }

    /// The at/past-cursor arm's whole job: an entry whose deadline is at
    /// or behind the cursor is already due, so it must be reachable by the
    /// coming step — not by the cursor's own slot, which the upward walk
    /// never visits again. `advance_to`'s promise is "every pending entry
    /// with `deadline <= now`", so it holds even when `now` is the cursor
    /// itself and there is no step to take.
    #[test]
    fn entry_at_the_cursor_is_due_not_pending() {
        let mut w = super::TimingWheel::new(at(10));
        w.insert(at(10), id(1));
        w.insert(at(11), id(2));
        // No step: the cursor is already at `now` — and yet the entry that
        // has arrived fires while the future one stays pending.
        assert_eq!(w.advance_to(at(10)), vec![id(1)]);
        assert_eq!(w.next_deadline(), Some(at(11)));
        // The step that follows still finds the future entry exactly once.
        assert_eq!(w.advance_to(at(11)), vec![id(2)]);
        assert_eq!(w.len(), 0);
    }

    /// A deadline already behind the cursor is collected by the call that
    /// finds it due: `insert_internal` files it in the slot after the
    /// cursor, and `advance_to`'s sweep reaches that slot even when the
    /// call has no step left to take.
    #[test]
    fn past_deadline_still_fires_at_the_coming_step() {
        let mut w = super::TimingWheel::new(at(100));
        w.insert(at(40), id(1));
        assert_eq!(w.advance_to(at(100)), vec![id(1)]);
        assert_eq!(w.len(), 0);
    }

    /// The sub-second remainder of the same boundary: `num_seconds`
    /// truncates, so a deadline half a second past the cursor is
    /// "delta 0" too and has to take the same next-slot route.
    #[test]
    fn sub_second_deadline_fires_on_the_next_step() {
        let mut w = super::TimingWheel::new(at(10));
        w.insert(at(10) + chrono::Duration::milliseconds(500), id(1));
        assert!(w.advance_to(at(10)).is_empty(), "still in the future");
        assert_eq!(w.advance_to(at(11)), vec![id(1)]);
    }

    /// A deadline filed in the slot the cursor is sitting on is the *last*
    /// thing its level reaches, because that slot has just been serviced.
    /// Scanning from it made `next_deadline` report a lap-away deadline
    /// ahead of a nearer one in the same level; the host arms its timer
    /// from that answer, so the near job ran up to a lap late.
    ///
    /// One case per level, each pairing a lap-tail deadline (fractional,
    /// so its slot rounds up onto the cursor's own) with a nearer one in
    /// the same level.
    #[test]
    fn lap_tail_entry_does_not_mask_a_nearer_deadline() {
        // Level 0 (64s lap): the tail second rounds up onto slot 0.
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(63, 900), id(1));
        w.insert(at(5), id(2));
        assert_eq!(w.next_deadline(), Some(at(5)));

        // Level 1 (4096s lap).
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(4_095, 900), id(1));
        w.insert(at(100), id(2));
        assert_eq!(w.next_deadline(), Some(at(100)));

        // Level 2 (262144s lap).
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(262_143, 900), id(1));
        w.insert(at(5_000), id(2));
        assert_eq!(w.next_deadline(), Some(at(5_000)));

        // Level 3 (the span itself, 16777216s).
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(16_777_215, 900), id(1));
        w.insert(at(300_000), id(2));
        assert_eq!(w.next_deadline(), Some(at(300_000)));
    }

    /// The scan's last resort still reaches the passed slot: a lap-tail
    /// entry is a real pending deadline, so it must stay visible rather
    /// than be masked by scanning order in the other direction.
    ///
    /// It also fires exactly on time: at its deadline the walk has reached
    /// the second before the wrap, which puts the entry's slot first in
    /// line for the post-walk sweep.
    #[test]
    fn a_lap_tail_entry_is_still_reported_and_fires_on_time() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(63, 900), id(1));
        assert_eq!(w.next_deadline(), Some(at_ms(63, 900)));
        assert_eq!(
            w.advance_to(at_ms(63, 900)),
            vec![id(1)],
            "no lateness: the sweep reaches the wrapped slot at the deadline"
        );
        assert_eq!(w.next_deadline(), None);
    }

    /// Cross-level ordering: the nearest deadline wins whatever level
    /// holds it (`best` is a `min`, and the per-level scans must not
    /// short-circuit it).
    #[test]
    fn next_deadline_is_the_nearest_across_levels() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at(5_000), id(1)); // level 2
        w.insert(at(100), id(2)); // level 1
        w.insert(at(9), id(3)); // level 0
        assert_eq!(w.next_deadline(), Some(at(9)));
        // Each level's own entry becomes the nearest as the nearer ones
        // are collected.
        assert_eq!(w.advance_to(at(9)), vec![id(3)]);
        assert_eq!(w.next_deadline(), Some(at(100)));
        assert_eq!(w.advance_to(at(100)), vec![id(2)]);
        assert_eq!(w.next_deadline(), Some(at(5_000)));
    }

    /// A fractional deadline below the cursor's fraction is filed under
    /// its own second (the visit at `second + fraction` is already past
    /// it), so it is collectible at its deadline — the rounding must not
    /// shove it a second later.
    #[test]
    fn fractional_deadline_below_the_cursor_fraction_still_fires_on_time() {
        let mut w = super::TimingWheel::new(at_ms(0, 400));
        w.insert(at_ms(10, 200), id(1));
        assert_eq!(w.advance_to(at_ms(10, 200)), vec![id(1)]);
    }

    /// And when the fractions are equal, the deadline is due at that very
    /// instant: the visit lands exactly on it.
    #[test]
    fn equal_fractions_fire_at_the_deadline() {
        let mut w = super::TimingWheel::new(at_ms(0, 400));
        w.insert(at_ms(10, 400), id(1));
        assert_eq!(w.advance_to(at_ms(10, 400)), vec![id(1)]);
    }

    #[test]
    fn ten_thousand_jobs_empty_tick_is_cheap_and_correct() {
        // 等价性 + 规模烟囱测试：1 万任务、无到期 tick 立即返回
        let mut w = super::TimingWheel::new(at(0));
        for i in 0..10_000u64 {
            w.insert(at(10_000 + i as i64), id(i));
        }
        assert!(w.advance_to(at(5_000)).is_empty());
        let fired = w.advance_to(at(10_100));
        assert_eq!(fired.len(), 101);
    }

    /// A fractional deadline must not wait a lap for its slot to come
    /// round again.
    ///
    /// The cursor keeps its fraction as it steps, and a whole-second host
    /// visits a slot only at the whole second: filing an entry under its
    /// own truncated second put it in the slot the cursor had already
    /// passed for that second (inspected just *before* it was due), and
    /// that slot is not visited again until the level-0 lap wraps (~64 s)
    /// — with `next_deadline` reporting a past deadline the whole time.
    #[test]
    fn fractional_deadline_is_not_stranded_for_a_lap() {
        let mut w = super::TimingWheel::new(at(0));
        w.insert(at_ms(10, 900), id(1));
        assert!(w.advance_to(at(10)).is_empty(), "not due at 10.0");
        assert_eq!(
            w.advance_to(at(11)),
            vec![id(1)],
            "the second after the deadline collects it — no lap wait"
        );
        assert_eq!(w.len(), 0);
    }

    /// The same, with a fractional cursor: the visit lands at
    /// `second + fraction`, so the entry has to be filed under the second
    /// whose visit reaches it (11.4), not the one it was inserted in.
    #[test]
    fn fractional_deadline_with_a_fractional_cursor_is_not_stranded() {
        let mut w = super::TimingWheel::new(at_ms(0, 400));
        w.insert(at_ms(10, 900), id(1));
        assert!(w.advance_to(at_ms(10, 400)).is_empty(), "not due yet");
        assert_eq!(w.advance_to(at_ms(11, 400)), vec![id(1)]);
    }

    /// A whole-second deadline is untouched by the cursor's fraction:
    /// it is collected as soon as a call finds it due, never a second
    /// late (rounding every integral deadline up would be the easy way
    /// to get the fractional case wrong).
    #[test]
    fn whole_second_deadline_is_not_pushed_a_second_late() {
        let mut w = super::TimingWheel::new(at_ms(0, 400));
        w.insert(at(10), id(1));
        assert_eq!(w.advance_to(at(10)), vec![id(1)]);
    }
}
