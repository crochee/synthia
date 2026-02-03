//! Time wheel entry types

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{job::Job, trigger::Trigger};

pub(crate) struct Entry {
    pub(crate) job: Arc<dyn Job>,
    pub(crate) trigger: Arc<dyn Trigger>,
    pub(crate) delay: i64,
    pub(crate) circle: usize,
    pub(crate) removed: AtomicBool,
}

impl Entry {
    pub(crate) fn new(
        job: Arc<dyn Job>,
        trigger: Arc<dyn Trigger>,
        delay: i64,
    ) -> Self {
        Self {
            job,
            trigger,
            delay,
            circle: 0,
            removed: AtomicBool::new(false),
        }
    }

    pub(crate) fn from_entry(entry: &Entry, circle: usize) -> Self {
        Self {
            job: Arc::clone(&entry.job),
            trigger: Arc::clone(&entry.trigger),
            delay: entry.delay,
            circle,
            removed: AtomicBool::new(false),
        }
    }

    pub(crate) fn from_arc(entry: &Arc<Entry>, circle: usize) -> Arc<Self> {
        Arc::new(Self::from_entry(entry, circle))
    }

    pub(crate) fn is_removed(&self) -> bool {
        self.removed.load(Ordering::SeqCst)
    }

    pub(crate) fn mark_removed(&self) {
        self.removed.store(true, Ordering::SeqCst);
    }
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("job_key", &self.job.key())
            .field("delay", &self.delay)
            .field("circle", &self.circle)
            .field("removed", &self.is_removed())
            .finish_non_exhaustive()
    }
}

pub(crate) struct TaskLocation {
    pub(crate) pos: usize,
    pub(crate) item: Arc<Entry>,
}

impl TaskLocation {
    pub(crate) fn new(pos: usize, item: Arc<Entry>) -> Self {
        Self { pos, item }
    }
}

impl fmt::Debug for TaskLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskLocation")
            .field("pos", &self.pos)
            .field("job_key", &self.item.job.key())
            .finish()
    }
}
