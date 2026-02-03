//! Time wheel core implementation

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::{sync::mpsc, time};
use tokio_util::sync::CancellationToken;

use super::entry::{Entry, TaskLocation};
use crate::{
    Result,
    ScheduledJob,
    error::JobError,
    job::Job,
    trigger::Trigger,
};

type NowFn = Box<dyn Fn() -> i64 + Send + Sync>;

pub struct TimeWheel {
    interval: Duration,
    slots: Vec<Mutex<Vec<Arc<Entry>>>>,
    timer_map: DashMap<Arc<str>, TaskLocation>,
    current_pos: AtomicUsize,
    slot_count: usize,
    add_task_tx: Arc<mpsc::Sender<Entry>>,
    add_task_rx: Mutex<Option<mpsc::Receiver<Entry>>>,
    remove_task_tx: Arc<mpsc::Sender<Arc<str>>>,
    remove_task_rx: Mutex<Option<mpsc::Receiver<Arc<str>>>>,
    now_fn: NowFn,
    running: AtomicBool,
}

impl fmt::Debug for TimeWheel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimeWheel")
            .field("interval", &self.interval)
            .field("slot_count", &self.slot_count)
            .field("current_pos", &self.current_pos.load(Ordering::SeqCst))
            .field("job_count", &self.timer_map.len())
            .finish_non_exhaustive()
    }
}

impl Default for TimeWheel {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for TimeWheel {}
unsafe impl Sync for TimeWheel {}

impl TimeWheel {
    pub fn new() -> Self {
        let (add_task_tx, add_task_rx) = mpsc::channel(1024);
        let (remove_task_tx, remove_task_rx) = mpsc::channel(1024);
        let slot_count = 1024;
        let slots = (0..slot_count).map(|_| Mutex::new(Vec::new())).collect();
        let now_fn = Box::new(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_nanos() as i64
        });

        Self {
            interval: Duration::from_secs(1),
            slots,
            timer_map: DashMap::new(),
            current_pos: AtomicUsize::new(0),
            slot_count,
            add_task_tx: Arc::new(add_task_tx),
            add_task_rx: Mutex::new(Some(add_task_rx)),
            remove_task_tx: Arc::new(remove_task_tx),
            remove_task_rx: Mutex::new(Some(remove_task_rx)),
            now_fn,
            running: AtomicBool::new(false),
        }
    }

    pub fn schedule(
        &self,
        job: Arc<dyn Job>,
        trigger: Arc<dyn Trigger>,
    ) -> Result<()> {
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            return handle.block_on(self.schedule_async(job, trigger));
        }
        Err(JobError::NoRuntime(
            "Use schedule_async in async context".into(),
        ))
    }

    pub async fn schedule_async(
        &self,
        job: Arc<dyn Job>,
        trigger: Arc<dyn Trigger>,
    ) -> Result<()> {
        let key: Arc<str> = job.key().into();

        if self.timer_map.contains_key(&key) {
            return Err(JobError::DuplicateJob(key.to_string()));
        }

        let delay =
            self.calculate_delay(trigger.as_ref()).ok_or_else(|| {
                JobError::ChannelError("Failed to calculate delay".into())
            })?;

        let entry =
            Arc::new(Entry::new(Arc::clone(&job), Arc::clone(&trigger), delay));

        self.timer_map
            .insert(Arc::clone(&key), TaskLocation::new(0, Arc::clone(&entry)));

        self.add_task_tx
            .send(Entry::new(job, trigger, delay))
            .await
            .map_err(|e| JobError::ChannelError(e.to_string()))?;

        Ok(())
    }

    pub async fn run(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(JobError::DuplicateRun);
        }

        let add_task_rx = self.add_task_rx.lock().take();
        let remove_task_rx = self.remove_task_rx.lock().take();

        let mut add_task_rx = add_task_rx.ok_or(JobError::ChannelError(
            "Add task receiver already taken".into(),
        ))?;
        let mut remove_task_rx = remove_task_rx.ok_or(
            JobError::ChannelError("Remove task receiver already taken".into()),
        )?;

        let mut ticker = time::interval(self.interval);
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.tick();
                }
                Some(task) = add_task_rx.recv() => {
                    self.add_task(task);
                }
                Some(key) = remove_task_rx.recv() => {
                    self.do_remove(&key);
                }
                _ = cancellation_token.cancelled() => {
                    break;
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn jobs(&self) -> Vec<ScheduledJob> {
        self.jobs_with_filter(None, None)
    }

    pub fn jobs_with_filter(
        &self,
        key: Option<&str>,
        trigger_desc_contains: Option<&str>,
    ) -> Vec<ScheduledJob> {
        self.timer_map
            .iter()
            .filter_map(|entry| {
                let job_key = entry.key();
                let trigger_desc = entry.value().item.trigger.description();

                if let Some(k) = key
                    && job_key.as_ref() != k
                {
                    return None;
                }

                if let Some(ref pattern) = trigger_desc_contains
                    && !trigger_desc.contains(pattern)
                {
                    return None;
                }

                Some(ScheduledJob::new(
                    Arc::clone(&entry.value().item.job),
                    trigger_desc,
                    entry.value().item.delay,
                ))
            })
            .collect()
    }

    pub async fn remove(&self, key: &str) -> Result<()> {
        self.remove_task_tx
            .send(key.into())
            .await
            .map_err(|e| JobError::ChannelError(e.to_string()))
    }

    fn do_remove(&self, key: &str) {
        if let Some(location) = self.timer_map.get(key) {
            location.item.mark_removed();
        }
        self.timer_map.remove(key);
    }

    fn tick(&self) {
        let current = self.current_pos.fetch_add(1, Ordering::SeqCst);
        let pos = (current + 1) % self.slot_count;

        let tasks = {
            let mut slot = self.slots[pos].lock();
            slot.drain(..).collect::<Vec<_>>()
        };

        self.scan_and_run_task(tasks);
    }

    fn get_position_and_circle(&self, delay: i64) -> (usize, usize) {
        let interval_ns = self.interval.as_nanos() as i64;
        let steps = delay / interval_ns;
        let current = self.current_pos.load(Ordering::SeqCst) as i64;
        let pos = (current + steps) % self.slot_count as i64;
        let circle = ((steps - 1) / self.slot_count as i64).max(0) as usize;
        (pos as usize, circle)
    }

    fn scan_and_run_task(&self, tasks: Vec<Arc<Entry>>) {
        for entry in tasks {
            if entry.is_removed() {
                continue;
            }

            if entry.circle > 0 {
                let current_pos =
                    self.current_pos.load(Ordering::SeqCst) % self.slot_count;
                let new_entry = Entry::from_arc(&entry, entry.circle - 1);
                self.slots[current_pos].lock().push(new_entry);
                continue;
            }

            let job = Arc::clone(&entry.job);
            tokio::spawn(async move {
                job.execute().await;
            });

            if !entry.is_removed() {
                self.reschedule_task(&entry);
            }
        }
    }

    fn calculate_delay(&self, trigger: &dyn Trigger) -> Option<i64> {
        let now = (self.now_fn)();
        let next_run_time = trigger.next_fire_time(now)?;
        let interval_ns = self.interval.as_nanos() as i64;
        let delay = next_run_time.saturating_sub(now);
        Some(delay.max(interval_ns))
    }

    fn reschedule_task(&self, entry: &Entry) {
        let trigger = entry.trigger.as_ref();
        match self.calculate_delay(trigger) {
            Some(delay) => {
                let new_entry = Entry::new(
                    Arc::clone(&entry.job),
                    Arc::clone(&entry.trigger),
                    delay,
                );
                self.add_task(new_entry);
            }
            None => {
                self.timer_map.remove(entry.job.key());
            }
        }
    }

    fn add_task(&self, task: Entry) {
        let (pos, circle) = self.get_position_and_circle(task.delay);
        let entry = Arc::new(Entry::from_entry(&task, circle));

        let key: Arc<str> = entry.job.key().into();
        let location = TaskLocation::new(pos, Arc::clone(&entry));
        self.timer_map.insert(Arc::clone(&key), location);

        let slot = &self.slots[pos];
        slot.lock().push(entry);
    }
}
