//! `ScheduleStore` — PID-locked atomic JSON persistence for [`Job`]s.
//!
//! ## Layout
//!
//! ```text
//! <root>/
//!   schedules/
//!     <job_id>.json   ← one Job per file
//!     <job_id>.lock   ← PID-locked file; absent = free
//! ```
//!
//! Every mutation acquires the per-job `.lock` (PID-tagged
//! file, retried with backoff), re-reads the latest state
//! from disk, applies the change, writes atomically via
//! `temp + rename`, then releases. Same shape as
//! `pi-subagents/src/schedule-store.ts`.
//!
//! `kill -0` is the liveness check: if the lock holder is
//! dead (different process id from `getpid()`), the lock is
//! considered stale and is reaped on the spot. This is the
//! cheapest cross-platform way to keep a long-lived
//! process's lock state honest without depending on `flock`
//! or `nix` — both of which would force us to take on a
//! system dependency that does not compose with the
//! runtime-neutral library rule.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use parking_lot::Mutex;
use thiserror::Error;
use tracing::warn;

use crate::job::{Job, JobId, JobStatus};

const LOCK_RETRY_MS: u64 = 50;
const LOCK_MAX_RETRIES: u32 = 100;
const PID_STALE_AFTER_SECS: i64 = 60;

/// Compute the disk path for a single [`Job`]. Pure —
/// callers use this to delete a stale file directly.
#[must_use]
pub(crate) fn schedule_path(root: &Path, job_id: &JobId) -> PathBuf {
    root.join("schedules")
        .join(format!("{}.json", job_id.as_str()))
}

/// Compute the disk path for a single job's `.lock` file.
/// Same naming convention as [`schedule_path`] so `lock`
/// and `data` stay paired.
#[must_use]
pub(crate) fn lock_path(root: &Path, job_id: &JobId) -> PathBuf {
    root.join("schedules")
        .join(format!("{}.lock", job_id.as_str()))
}

/// Errors returned by [`ScheduleStore`].
#[derive(Debug, Error)]
pub enum ScheduleStoreError {
    /// Lock acquisition exhausted its retry budget. A peer
    /// process is actively writing the same job, or the
    /// lock file is wedged.
    #[error("failed to acquire schedule lock for {job_id}: timeout")]
    LockTimeout { job_id: String },
    /// I/O failure other than the lock wait.
    #[error("schedule store I/O: {message}")]
    Io { message: String },
    /// Stored JSON could not be deserialised.
    #[error("schedule store JSON: {message}")]
    Json { message: String },
}

impl ScheduleStoreError {
    fn io(err: std::io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}

/// PID-locked JSON store for scheduled jobs.
#[derive(Debug)]
pub struct ScheduleStore {
    root: PathBuf,
    /// Per-job in-memory cache. The store reloads from disk
    /// under lock to keep multi-process writers honest, but
    /// the cache is what every read serves from so the
    /// hot path is a cheap `HashMap` lookup.
    cache: Mutex<std::collections::HashMap<JobId, Job>>,
    /// Revision counter, bumped by every mutating route.
    ///
    /// A reader that caches a projection of this store (the
    /// scheduler's wheel is one) compares this against the value it
    /// last read to learn — in O(1) — that its projection is stale.
    /// `list()` alone cannot answer that without scanning.
    version: AtomicU64,
}

impl ScheduleStore {
    /// Construct a store rooted at `root`. The directory is
    /// not created until the first mutation; the store
    /// lazy-loads on `add` / `update`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: Mutex::new(std::collections::HashMap::new()),
            version: AtomicU64::new(0),
        }
    }

    /// Storage root passed to the constructor. Useful when
    /// callers want to inspect or sweep the on-disk layout
    /// from their own tooling.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Revision of this store's contents: every mutating route
    /// (`load` / `add` / `update` / `remove` / `gc_completed`)
    /// advances it exactly once per change of state — a `gc_completed`
    /// sweep of three rows advances it three times — while read-only
    /// routes (`list` / `get` / `has_name`) never touch it. It is the
    /// value a wheel cache watches: compare a remembered reading to
    /// learn, without scanning a single job, that its projection is
    /// stale.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Advance the revision. Called at the end of every mutating
    /// route, once the cache reflects the change.
    fn bump_version(&self) {
        self.version.fetch_add(1, Ordering::Release);
    }

    /// Snapshot of every cached job, in `JobId` order so
    /// callers get a stable iteration order for tests and
    /// operator dashboards.
    pub fn list(&self) -> Vec<Job> {
        let cache = self.cache.lock();
        let mut jobs: Vec<Job> = cache.values().cloned().collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        jobs
    }

    /// Lookup by id without going to disk.
    pub fn get(&self, id: &JobId) -> Option<Job> {
        self.cache.lock().get(id).cloned()
    }

    /// True when another job in this store already uses
    /// `name`. Pass `except_id` to allow renaming a job
    /// to its current name (the no-op rename path).
    pub fn has_name(&self, name: &str, except_id: Option<&JobId>) -> bool {
        let cache = self.cache.lock();
        cache
            .values()
            .any(|j| j.name == name && except_id != Some(&j.id))
    }

    /// Bootstrap the cache from disk. Idempotent — safe to
    /// call twice. Skips jobs whose `data` file is missing
    /// (the lock was stale and reaped between writes) and
    /// logs a `warn!` for each such drop so operators can
    /// see the recovery.
    pub fn load(&self) -> Result<usize, ScheduleStoreError> {
        let dir = self.root.join("schedules");
        if !dir.exists() {
            return Ok(0);
        }
        let mut loaded = 0;
        let mut cache = self.cache.lock();
        cache.clear();
        let entries = fs::read_dir(&dir).map_err(ScheduleStoreError::io)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(err) => {
                    warn!(
                        schedule_file = %path.display(),
                        error = %err,
                        "schedule_store: skipping unreadable file",
                    );
                    continue;
                }
            };
            match serde_json::from_str::<Job>(&raw) {
                Ok(job) => {
                    cache.insert(job.id.clone(), job);
                    loaded += 1;
                }
                Err(err) => {
                    warn!(
                        schedule_file = %path.display(),
                        error = %err,
                        "schedule_store: skipping malformed JSON",
                    );
                }
            }
        }
        self.bump_version();
        Ok(loaded)
    }

    /// Persist a new job. Fails with a duplicate-name error
    /// when the store already carries a job with the same
    /// name (mirroring pi-subagents' invariant).
    ///
    /// # Errors
    ///
    /// [`ScheduleStoreError::LockTimeout`] when the per-job
    /// lock cannot be acquired; [`ScheduleStoreError::Io`]
    /// on filesystem failure; the duplicate-name case is
    /// reported as `Io` with a stable prefix to keep the
    /// public error enum small.
    pub fn add(&self, job: Job) -> Result<(), ScheduleStoreError> {
        if self.has_name(&job.name, None) {
            return Err(ScheduleStoreError::Io {
                message: format!("duplicate schedule name: {}", job.name),
            });
        }
        let path = schedule_path(&self.root, &job.id);
        self.write_under_lock(&job, &path)?;
        self.cache.lock().insert(job.id.clone(), job);
        self.bump_version();
        Ok(())
    }

    /// Mutate an existing job under lock. The caller
    /// supplies the patched view; the store reloads from
    /// disk first so cross-process writers do not clobber.
    ///
    /// # Errors
    ///
    /// [`ScheduleStoreError::Io`] when the id is unknown;
    /// [`ScheduleStoreError::LockTimeout`] on contention.
    pub fn update<F>(
        &self,
        id: &JobId,
        mutator: F,
    ) -> Result<Option<Job>, ScheduleStoreError>
    where
        F: FnOnce(&mut Job),
    {
        let path = schedule_path(&self.root, id);
        let _guard = self.acquire_lock(id)?;
        let mut job = match self.read_under_lock(id, &path)? {
            Some(j) => j,
            None => return Ok(None),
        };
        mutator(&mut job);
        self.write_raw(&job, &path)?;
        let snapshot = job.clone();
        self.cache
            .lock()
            .insert(snapshot.id.clone(), snapshot.clone());
        self.bump_version();
        Ok(Some(snapshot))
    }

    /// Remove a job from disk and cache. Returns `true`
    /// when the job was present.
    pub fn remove(&self, id: &JobId) -> Result<bool, ScheduleStoreError> {
        let path = schedule_path(&self.root, id);
        let _guard = self.acquire_lock(id)?;
        let removed = if path.exists() {
            fs::remove_file(&path).map_err(ScheduleStoreError::io)?;
            let lock = lock_path(&self.root, id);
            if lock.exists() {
                // Best-effort lock cleanup. A stale lock is
                // not an error — the next acquire will
                // reap it.
                let _ = fs::remove_file(&lock);
            }
            true
        } else {
            false
        };
        if removed {
            self.cache.lock().remove(id);
            self.bump_version();
        }
        Ok(removed)
    }

    /// Sweep every `Completed` job whose `last_fired_at`
    /// is older than `cutoff` (UTC). Returns the ids
    /// removed so callers can log / report.
    pub fn gc_completed(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<JobId>, ScheduleStoreError> {
        let mut removed = Vec::new();
        let candidates: Vec<JobId> = {
            let cache = self.cache.lock();
            cache
                .values()
                .filter(|j| {
                    j.status == JobStatus::Completed
                        && j.last_fired_at.is_some_and(|t| t <= cutoff)
                })
                .map(|j| j.id.clone())
                .collect()
        };
        for id in candidates {
            if self.remove(&id)? {
                removed.push(id);
            }
        }
        Ok(removed)
    }

    // -- private helpers -----------------------------------------

    fn acquire_lock(
        &self,
        job_id: &JobId,
    ) -> Result<LockGuard, ScheduleStoreError> {
        let dir = self.root.join("schedules");
        fs::create_dir_all(&dir).map_err(ScheduleStoreError::io)?;
        let path = lock_path(&self.root, job_id);
        let pid = process::id();
        for _ in 0..LOCK_MAX_RETRIES {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = writeln!(f, "{pid}");
                    return Ok(LockGuard { path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    if self.reap_stale_lock(&path) {
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(LOCK_RETRY_MS));
                }
                Err(err) => return Err(ScheduleStoreError::io(err)),
            }
        }
        Err(ScheduleStoreError::LockTimeout {
            job_id: job_id.to_string(),
        })
    }

    /// Inspect the lock file. If the recorded pid is dead
    /// or older than [`PID_STALE_AFTER_SECS`], unlink the
    /// lock and return `true` so the caller can retry.
    fn reap_stale_lock(&self, path: &Path) -> bool {
        let Ok(mut f) = File::open(path) else {
            return false;
        };
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return false;
        }
        let Ok(pid) = buf.trim().parse::<u32>() else {
            return false;
        };
        // `kill(pid, 0)` is the POSIX liveness check —
        // succeeds with no effect when `pid` is alive.
        // Windows libc has no `kill`; on non-unix we skip
        // the liveness probe and fall through to the
        // same-pid + mtime-age checks below, which is the
        // path the function takes on Linux when the pid
        // happens to be alive too.
        #[cfg(unix)]
        {
            let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
            if !alive {
                let _ = fs::remove_file(path);
                return true;
            }
        }
        // Same-pid lock is fine: re-entrant writes from the
        // current process are legal. Different-pid alive
        // locks must wait.
        if pid == process::id() {
            return false;
        }
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => return false,
        };
        let elapsed =
            match std::time::SystemTime::now().duration_since(modified) {
                Ok(d) => d,
                Err(_) => return false,
            };
        if elapsed.as_secs() > PID_STALE_AFTER_SECS as u64 {
            let _ = fs::remove_file(path);
            return true;
        }
        false
    }

    fn read_under_lock(
        &self,
        job_id: &JobId,
        path: &Path,
    ) -> Result<Option<Job>, ScheduleStoreError> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path).map_err(ScheduleStoreError::io)?;
        let job: Job = serde_json::from_str(&raw).map_err(|err| {
            ScheduleStoreError::Json {
                message: format!("{job_id}: {err}"),
            }
        })?;
        Ok(Some(job))
    }

    fn write_under_lock(
        &self,
        job: &Job,
        path: &Path,
    ) -> Result<(), ScheduleStoreError> {
        let _guard = self.acquire_lock(&job.id)?;
        self.write_raw(job, path)
    }

    fn write_raw(
        &self,
        job: &Job,
        path: &Path,
    ) -> Result<(), ScheduleStoreError> {
        let dir = self.root.join("schedules");
        fs::create_dir_all(&dir).map_err(ScheduleStoreError::io)?;
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_string_pretty(job).map_err(|err| {
            ScheduleStoreError::Json {
                message: err.to_string(),
            }
        })?;
        {
            let mut f = File::create(&tmp).map_err(ScheduleStoreError::io)?;
            f.write_all(raw.as_bytes())
                .map_err(ScheduleStoreError::io)?;
            f.sync_all().map_err(ScheduleStoreError::io)?;
        }
        fs::rename(&tmp, path).map_err(ScheduleStoreError::io)?;
        Ok(())
    }
}

/// RAII lock guard. `Drop` unlinks the lock file so the
/// next acquire does not have to wait on a stale entry. If
/// the lock is already gone (e.g. reap path), the unlink is
/// a silent best-effort.
struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration as StdDuration;

    use chrono::Duration as ChronoDuration;
    use tempfile::TempDir;

    use super::*;
    use crate::job::{Job, JobKind};

    fn pinned_now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn sample_job(name: &str, kind: JobKind) -> Job {
        let next = pinned_now() + ChronoDuration::seconds(60);
        Job::new(name, kind, serde_json::json!({"agent": name}), next)
    }

    #[test]
    fn add_persists_to_disk() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let job = sample_job(
            "every-5s",
            JobKind::Interval {
                interval: StdDuration::from_secs(5),
            },
        );
        store.add(job.clone()).unwrap();
        assert!(schedule_path(dir.path(), &job.id).exists());
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn duplicate_name_rejected() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        store.add(sample_job("dup", JobKind::Once)).unwrap();
        let err = store.add(sample_job("dup", JobKind::Once)).unwrap_err();
        assert!(matches!(err, ScheduleStoreError::Io { .. }));
    }

    #[test]
    fn load_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let job = sample_job(
            "round-trip",
            JobKind::Interval {
                interval: StdDuration::from_secs(10),
            },
        );
        store.add(job.clone()).unwrap();
        let reloaded = ScheduleStore::new(dir.path());
        assert_eq!(reloaded.load().unwrap(), 1);
        assert_eq!(reloaded.get(&job.id), Some(job));
    }

    #[test]
    fn update_under_lock_persists() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let job = sample_job("upd", JobKind::Once);
        store.add(job.clone()).unwrap();
        let updated = store
            .update(&job.id, |j| {
                j.description = "updated".to_string();
            })
            .unwrap()
            .unwrap();
        assert_eq!(updated.description, "updated");
        // Re-read from a fresh store to confirm the
        // mutation went to disk and not just the cache.
        let reloaded = ScheduleStore::new(dir.path());
        reloaded.load().unwrap();
        assert_eq!(reloaded.get(&job.id).unwrap().description, "updated");
    }

    #[test]
    fn gc_completed_drops_old_completed_rows() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let mut j = sample_job("g", JobKind::Once);
        j.status = JobStatus::Completed;
        let now = pinned_now();
        j.last_fired_at = Some(now - ChronoDuration::hours(1));
        store.add(j.clone()).unwrap();
        let removed = store
            .gc_completed(now - ChronoDuration::minutes(30))
            .unwrap();
        assert_eq!(removed, vec![j.id.clone()]);
        assert!(!schedule_path(dir.path(), &j.id).exists());
    }

    #[test]
    fn lock_unlinks_on_drop() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let job = sample_job("lock", JobKind::Once);
        store.add(job.clone()).unwrap();
        let lock = lock_path(dir.path(), &job.id);
        assert!(!lock.exists(), "lock must be released post-add");
    }

    /// The revision is what a cache of this store watches, so a
    /// mutating route that forgot to bump it would leave that cache
    /// serving a row that no longer exists (or missing one that does).
    #[test]
    fn version_advances_on_every_mutating_route() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        assert_eq!(store.version(), 0, "a fresh store is unmutated");
        let job = sample_job("v", JobKind::Once);

        store.add(job.clone()).unwrap();
        let after_add = store.version();
        assert!(after_add > 0, "add must advance the revision");

        // A read is not a mutation.
        store.list();
        store.get(&job.id);
        store.has_name("v", None);
        assert_eq!(store.version(), after_add, "reads must not advance it");

        store
            .update(&job.id, |j| j.description = "x".into())
            .unwrap();
        let after_update = store.version();
        assert!(after_update > after_add, "update must advance it");

        store.remove(&job.id).unwrap();
        let after_remove = store.version();
        assert!(after_remove > after_update, "remove must advance it");

        store.load().unwrap();
        assert!(
            store.version() > after_remove,
            "a reload can change what this process sees, so it advances it"
        );
    }

    /// `gc_completed` is the one mutating route that reaches the
    /// revision only by delegating each deletion to `remove`. Pin the
    /// observable rather than the delegation: a sweep that deletes a
    /// row advances the revision, and one that deletes nothing does not.
    #[test]
    fn gc_completed_advances_the_revision() {
        let dir = TempDir::new().unwrap();
        let store = ScheduleStore::new(dir.path());
        let now = pinned_now();
        let mut job = sample_job("gc", JobKind::Once);
        job.status = JobStatus::Completed;
        job.last_fired_at = Some(now - ChronoDuration::hours(1));
        store.add(job.clone()).unwrap();

        // A cutoff older than every completion keeps the row, so the
        // sweep visits a candidate and leaves it: no change, no bump.
        let before = store.version();
        let kept = store.gc_completed(now - ChronoDuration::hours(2)).unwrap();
        assert!(kept.is_empty(), "nothing is old enough to sweep: {kept:?}");
        assert_eq!(
            store.version(),
            before,
            "a sweep that changes nothing must not advance the revision"
        );

        let removed = store
            .gc_completed(now - ChronoDuration::minutes(30))
            .unwrap();
        assert_eq!(removed, vec![job.id.clone()]);
        assert!(store.get(&job.id).is_none(), "the swept row is gone");
        assert!(
            store.version() > before,
            "sweeping a row must advance the revision the wheel cache watches"
        );
    }
}
