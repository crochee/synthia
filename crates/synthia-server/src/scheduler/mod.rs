use std::sync::Arc;

use dashmap::DashMap;
use synthia_core::Error;
use synthia_job::{Job, ScheduledJob, TimeWheel};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct JobRegistry {
    jobs: DashMap<String, Arc<dyn Job>>,
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: DashMap::new(),
        }
    }

    pub fn register(&self, job: Arc<dyn Job>) {
        self.jobs.insert(job.key().to_string(), job);
    }

    pub fn lookup(&self, key: &str) -> Option<Arc<dyn Job>> {
        self.jobs.get(key).map(|e| Arc::clone(e.value()))
    }

    pub fn keys(&self) -> Vec<String> {
        self.jobs.iter().map(|e| e.key().clone()).collect()
    }
}

pub struct JobScheduler {
    time_wheel: Arc<TimeWheel>,
    registry: Arc<JobRegistry>,
    cancellation_token: CancellationToken,
    _handle: JoinHandle<synthia_job::Result<()>>,
    paused: DashMap<String, ()>,
}

impl JobScheduler {
    pub fn new(registry: Arc<JobRegistry>) -> Self {
        let time_wheel = Arc::new(TimeWheel::new());
        let cancellation_token = CancellationToken::new();
        let tw = Arc::clone(&time_wheel);
        let ct = cancellation_token.clone();

        let handle = tokio::spawn(async move { tw.run(ct).await });

        Self {
            time_wheel,
            registry,
            cancellation_token,
            _handle: handle,
            paused: DashMap::new(),
        }
    }

    pub fn registry(&self) -> &Arc<JobRegistry> {
        &self.registry
    }

    pub fn time_wheel(&self) -> &Arc<TimeWheel> {
        &self.time_wheel
    }

    pub async fn schedule(
        &self,
        key: &str,
        trigger: Arc<dyn synthia_job::Trigger>,
    ) -> synthia_job::Result<()> {
        if self.paused.contains_key(key) {
            return Err(Error::NotFound(format!("Job '{key}' is paused")));
        }

        let job = self
            .registry
            .lookup(key)
            .ok_or_else(|| Error::NotFound(key.to_string()))?;

        self.time_wheel.schedule_async(job, trigger).await
    }

    pub async fn remove(&self, key: &str) -> synthia_job::Result<()> {
        self.paused.remove(key);
        self.time_wheel.remove(key).await
    }

    pub async fn execute(&self, key: &str) -> synthia_job::Result<()> {
        let job = self
            .registry
            .lookup(key)
            .ok_or_else(|| Error::NotFound(key.to_string()))?;

        tokio::spawn(async move {
            job.execute().await;
        });

        Ok(())
    }

    pub fn is_paused(&self, key: &str) -> bool {
        self.paused.contains_key(key)
    }

    pub fn mark_paused(&self, key: &str) {
        self.paused.insert(key.to_string(), ());
    }

    pub fn unmark_paused(&self, key: &str) {
        self.paused.remove(key);
    }

    pub fn list_jobs(&self) -> Vec<ScheduledJob> {
        self.time_wheel.jobs()
    }

    pub fn list_paused(&self) -> Vec<String> {
        self.paused.iter().map(|e| e.key().clone()).collect()
    }
}

impl Drop for JobScheduler {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}
