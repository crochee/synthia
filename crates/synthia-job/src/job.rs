//! Job trait and basic job types

use std::{fmt, sync::Arc};

use async_trait::async_trait;

#[async_trait]
pub trait Job: Send + Sync {
    fn description(&self) -> &str;
    fn key(&self) -> &str;
    async fn execute(&self);
}

pub struct ScheduledJob {
    pub job: Arc<dyn Job>,
    pub trigger_desc: String,
    pub delay: i64,
}

impl ScheduledJob {
    pub fn new(job: Arc<dyn Job>, trigger_desc: String, delay: i64) -> Self {
        Self {
            job,
            trigger_desc,
            delay,
        }
    }
}

impl fmt::Debug for ScheduledJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScheduledJob")
            .field("job", &self.job.description())
            .field("trigger_desc", &self.trigger_desc)
            .field("delay", &self.delay)
            .finish()
    }
}
