use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Utc};

use super::data::{CronJob, CronJobPatch, CronRun};
use crate::{
    Result,
    tools::storage::{FileStore, StoragePaths},
};

#[derive(Debug)]
pub struct CronFileStore {
    base: FileStore,
    paths: StoragePaths,
    run_id_counter: AtomicI64,
}

impl CronFileStore {
    pub fn new() -> Self {
        let paths = StoragePaths::new();
        let base = FileStore::new(paths.cron_dir());

        Self {
            base,
            paths,
            run_id_counter: AtomicI64::new(1),
        }
    }

    pub(crate) async fn create_job(&self, job: &CronJob) -> Result<()> {
        self.base.ensure_dir(&self.paths.cron_dir()).await?;

        let mut jobs = self.load_jobs().await?;
        jobs.push(job.clone());

        self.save_jobs(&jobs).await
    }

    pub(crate) async fn find_job(&self, job_id: &str) -> Result<CronJob> {
        let jobs = self.load_jobs().await?;
        jobs.into_iter().find(|j| j.id == job_id).ok_or_else(|| {
            crate::AgentError::session(format!("Job not found: {job_id}"))
        })
    }

    pub(crate) async fn all_jobs(&self) -> Result<Vec<CronJob>> {
        self.load_jobs().await
    }

    pub(crate) async fn delete_job(&self, job_id: &str) -> Result<()> {
        let mut jobs = self.load_jobs().await?;
        jobs.retain(|j| j.id != job_id);
        self.save_jobs(&jobs).await
    }

    pub(crate) async fn patch_job(
        &self,
        job_id: &str,
        patch: &CronJobPatch,
    ) -> Result<CronJob> {
        let mut jobs = self.load_jobs().await?;

        let job = jobs.iter_mut().find(|j| j.id == job_id);

        if let Some(job) = job {
            if let Some(crontab) = &patch.crontab {
                job.crontab = crontab.clone();
            }
            if let Some(description) = &patch.description {
                job.description = description.clone();
            }
            if let Some(content) = &patch.content {
                job.content = content.clone();
            }
            if let Some(enabled) = patch.enabled {
                job.enabled = enabled;
            }
            if let Some(next_run) = patch.next_run {
                job.next_run = Some(next_run);
            }

            let updated = job.clone();
            self.save_jobs(&jobs).await?;
            return Ok(updated);
        }

        Err(crate::AgentError::session(format!(
            "Job not found: {job_id}"
        )))
    }

    pub(crate) async fn find_due_jobs(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<CronJob>> {
        let jobs = self.load_jobs().await?;
        Ok(jobs
            .into_iter()
            .filter(|j| j.enabled && j.next_run.is_none_or(|next| next <= now))
            .collect())
    }

    pub(crate) async fn save_run(
        &self,
        job_id: &str,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        status: &str,
        output: Option<&str>,
        duration_ms: i64,
    ) -> Result<()> {
        self.base.ensure_dir(&self.paths.cron_runs_dir()).await?;

        let mut jobs = self.load_jobs().await?;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.last_run = Some(finished_at);
            job.last_status = Some(status.to_string());
            job.last_output = output.map(std::string::ToString::to_string);
        }
        self.save_jobs(&jobs).await?;

        let run = CronRun {
            id: self.run_id_counter.fetch_add(1, Ordering::SeqCst),
            job_id: job_id.to_string(),
            started_at,
            finished_at,
            status: status.to_string(),
            output: output.map(std::string::ToString::to_string),
            duration_ms: Some(duration_ms),
        };

        let runs_path = self.paths.cron_runs_file(job_id);
        self.base.append_jsonl(&runs_path, &run).await
    }

    pub(crate) async fn update_after_run(
        &self,
        job_id: &str,
        next_run: DateTime<Utc>,
        last_run: DateTime<Utc>,
        status: &str,
        output: Option<&str>,
    ) -> Result<()> {
        let mut jobs = self.load_jobs().await?;

        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.next_run = Some(next_run);
            job.last_run = Some(last_run);
            job.last_status = Some(status.to_string());
            job.last_output = output.map(std::string::ToString::to_string);
        }

        self.save_jobs(&jobs).await
    }

    pub(crate) async fn get_runs(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<CronRun>> {
        let runs_path = self.paths.cron_runs_file(job_id);

        if !self.base.file_exists(&runs_path).await {
            return Ok(Vec::new());
        }

        let mut runs: Vec<CronRun> = self.base.read_jsonl(&runs_path).await?;
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        runs.truncate(limit);
        Ok(runs)
    }

    async fn load_jobs(&self) -> Result<Vec<CronJob>> {
        let jobs_path = self.paths.cron_jobs_file();

        if !self.base.file_exists(&jobs_path).await {
            return Ok(Vec::new());
        }

        self.base.read_json(&jobs_path).await
    }

    async fn save_jobs(&self, jobs: &[CronJob]) -> Result<()> {
        let jobs_path = self.paths.cron_jobs_file();
        self.base.write_json(&jobs_path, &jobs).await
    }
}

impl Default for CronFileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CronFileStore {
    pub async fn add_job(&self, job: &CronJob) -> Result<()> {
        self.create_job(job).await
    }

    pub async fn get_job(&self, job_id: &str) -> Result<CronJob> {
        self.find_job(job_id).await
    }

    pub async fn list_jobs(&self) -> Result<Vec<CronJob>> {
        self.all_jobs().await
    }

    pub async fn remove_job(&self, job_id: &str) -> Result<()> {
        self.delete_job(job_id).await
    }

    pub async fn update_job(
        &self,
        job_id: &str,
        patch: &CronJobPatch,
    ) -> Result<CronJob> {
        self.patch_job(job_id, patch).await
    }

    pub async fn due_jobs(&self, now: DateTime<Utc>) -> Result<Vec<CronJob>> {
        self.find_due_jobs(now).await
    }

    pub async fn record_run(
        &self,
        job_id: &str,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        status: &str,
        output: Option<&str>,
        duration_ms: i64,
    ) -> Result<()> {
        self.save_run(
            job_id,
            started_at,
            finished_at,
            status,
            output,
            duration_ms,
        )
        .await
    }

    pub async fn update_job_after_run(
        &self,
        job_id: &str,
        next_run: DateTime<Utc>,
        last_run: DateTime<Utc>,
        status: &str,
        output: Option<&str>,
    ) -> Result<()> {
        self.update_after_run(job_id, next_run, last_run, status, output)
            .await
    }

    pub async fn list_runs(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<CronRun>> {
        self.get_runs(job_id, limit).await
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    struct TestContext {
        store: CronFileStore,
        _temp_dir: tempfile::TempDir,
    }

    impl TestContext {
        async fn new() -> Self {
            let temp_dir = tempdir().expect("Failed to create temp dir");
            let paths = StoragePaths::with_base(temp_dir.path().to_path_buf());
            let base = FileStore::new(paths.cron_dir());

            Self {
                store: CronFileStore {
                    base,
                    paths,
                    run_id_counter: AtomicI64::new(1),
                },
                _temp_dir: temp_dir,
            }
        }
    }

    #[tokio::test]
    async fn test_cron_file_store_create_and_get() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Test job".to_string(),
            content: "echo hello".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let retrieved = ctx.store.find_job("job-1").await.unwrap();
        assert_eq!(retrieved.id, "job-1");
        assert_eq!(retrieved.crontab, "0 0 * * * *");
        assert_eq!(retrieved.description, "Test job");
    }

    #[tokio::test]
    async fn test_cron_file_store_list() {
        let ctx = TestContext::new().await;

        let job1 = CronJob {
            id: "job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Job 1".to_string(),
            content: "echo 1".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        let job2 = CronJob {
            id: "job-2".to_string(),
            crontab: "0 30 * * * *".to_string(),
            description: "Job 2".to_string(),
            content: "echo 2".to_string(),
            enabled: false,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job1).await.unwrap();
        ctx.store.create_job(&job2).await.unwrap();

        let jobs = ctx.store.all_jobs().await.unwrap();
        assert_eq!(jobs.len(), 2);
    }

    #[tokio::test]
    async fn test_cron_file_store_update() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Original".to_string(),
            content: "echo hello".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let patch = CronJobPatch {
            crontab: Some("0 30 * * * *".to_string()),
            description: Some("Updated".to_string()),
            content: None,
            enabled: Some(false),
            next_run: None,
        };

        let updated = ctx.store.patch_job("job-1", &patch).await.unwrap();
        assert_eq!(updated.crontab, "0 30 * * * *");
        assert_eq!(updated.description, "Updated");
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_cron_file_store_delete() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Test job".to_string(),
            content: "echo hello".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();
        assert!(ctx.store.find_job("job-1").await.is_ok());

        ctx.store.delete_job("job-1").await.unwrap();
        assert!(ctx.store.find_job("job-1").await.is_err());
    }

    #[tokio::test]
    async fn test_cron_file_store_record_run() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Test job".to_string(),
            content: "echo hello".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let started = Utc::now();
        let finished = started + chrono::Duration::seconds(1);

        ctx.store
            .save_run(
                "job-1",
                started,
                finished,
                "success",
                Some("output"),
                1000,
            )
            .await
            .unwrap();

        let runs = ctx.store.get_runs("job-1", 10).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(runs[0].output, Some("output".to_string()));

        let updated_job = ctx.store.find_job("job-1").await.unwrap();
        assert_eq!(updated_job.last_status, Some("success".to_string()));
    }

    #[tokio::test]
    async fn test_find_job_exists() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-find".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Find test job".to_string(),
            content: "echo find".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let found = ctx.store.find_job("job-find").await.unwrap();
        assert_eq!(found.id, "job-find");
        assert_eq!(found.description, "Find test job");
    }

    #[tokio::test]
    async fn test_find_job_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx.store.find_job("nonexistent-job").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_due_jobs_none_due() {
        let ctx = TestContext::new().await;

        let future_time = Utc::now() + chrono::Duration::hours(1);

        let job = CronJob {
            id: "job-future".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Future job".to_string(),
            content: "echo future".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: Some(future_time),
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let now = Utc::now();
        let due_jobs = ctx.store.find_due_jobs(now).await.unwrap();
        assert!(due_jobs.is_empty());
    }

    #[tokio::test]
    async fn test_find_due_jobs_some_due() {
        let ctx = TestContext::new().await;

        let past_time = Utc::now() - chrono::Duration::hours(1);
        let future_time = Utc::now() + chrono::Duration::hours(1);

        let job1 = CronJob {
            id: "job-due".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Due job".to_string(),
            content: "echo due".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: Some(past_time),
            last_run: None,
            last_status: None,
            last_output: None,
        };

        let job2 = CronJob {
            id: "job-not-due".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Not due job".to_string(),
            content: "echo not-due".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: Some(future_time),
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job1).await.unwrap();
        ctx.store.create_job(&job2).await.unwrap();

        let now = Utc::now();
        let due_jobs = ctx.store.find_due_jobs(now).await.unwrap();
        assert_eq!(due_jobs.len(), 1);
        assert_eq!(due_jobs[0].id, "job-due");
    }

    #[tokio::test]
    async fn test_find_due_jobs_disabled_not_due() {
        let ctx = TestContext::new().await;

        let past_time = Utc::now() - chrono::Duration::hours(1);

        let job = CronJob {
            id: "job-disabled".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Disabled job".to_string(),
            content: "echo disabled".to_string(),
            enabled: false,
            created_at: Utc::now(),
            next_run: Some(past_time),
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let now = Utc::now();
        let due_jobs = ctx.store.find_due_jobs(now).await.unwrap();
        assert!(due_jobs.is_empty());
    }

    #[tokio::test]
    async fn test_get_runs_empty() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-no-runs".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "No runs job".to_string(),
            content: "echo no-runs".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let runs = ctx.store.get_runs("job-no-runs", 10).await.unwrap();
        assert!(runs.is_empty());
    }

    #[tokio::test]
    async fn test_get_runs_with_data() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-runs".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Runs job".to_string(),
            content: "echo runs".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let start1 = Utc::now() - chrono::Duration::hours(2);
        let end1 = start1 + chrono::Duration::seconds(5);
        ctx.store
            .save_run(
                "job-runs",
                start1,
                end1,
                "success",
                Some("output1"),
                5000,
            )
            .await
            .unwrap();

        let start2 = Utc::now() - chrono::Duration::hours(1);
        let end2 = start2 + chrono::Duration::seconds(10);
        ctx.store
            .save_run(
                "job-runs",
                start2,
                end2,
                "failure",
                Some("output2"),
                10000,
            )
            .await
            .unwrap();

        let runs = ctx.store.get_runs("job-runs", 10).await.unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].status, "failure");
        assert_eq!(runs[1].status, "success");
    }

    #[tokio::test]
    async fn test_get_runs_respects_limit() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-limit".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Limit job".to_string(),
            content: "echo limit".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        for i in 0..5 {
            let start = Utc::now() - chrono::Duration::minutes(i as i64);
            let end = start + chrono::Duration::seconds(1);
            ctx.store
                .save_run(
                    "job-limit",
                    start,
                    end,
                    "success",
                    Some(&format!("output{i}")),
                    1000,
                )
                .await
                .unwrap();
        }

        let runs = ctx.store.get_runs("job-limit", 3).await.unwrap();
        assert_eq!(runs.len(), 3);
    }

    #[tokio::test]
    async fn test_update_after_run() {
        let ctx = TestContext::new().await;

        let job = CronJob {
            id: "job-update".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Update job".to_string(),
            content: "echo update".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job).await.unwrap();

        let last_run = Utc::now();
        let next_run = last_run + chrono::Duration::hours(1);

        ctx.store
            .update_after_run(
                "job-update",
                next_run,
                last_run,
                "success",
                Some("job output"),
            )
            .await
            .unwrap();

        let updated = ctx.store.find_job("job-update").await.unwrap();
        assert_eq!(updated.next_run, Some(next_run));
        assert_eq!(updated.last_run, Some(last_run));
        assert_eq!(updated.last_status, Some("success".to_string()));
        assert_eq!(updated.last_output, Some("job output".to_string()));
    }

    #[tokio::test]
    async fn test_all_jobs() {
        let ctx = TestContext::new().await;

        let job1 = CronJob {
            id: "job-a".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Job A".to_string(),
            content: "echo a".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        let job2 = CronJob {
            id: "job-b".to_string(),
            crontab: "0 30 * * * *".to_string(),
            description: "Job B".to_string(),
            content: "echo b".to_string(),
            enabled: false,
            created_at: Utc::now(),
            next_run: None,
            last_run: None,
            last_status: None,
            last_output: None,
        };

        ctx.store.create_job(&job1).await.unwrap();
        ctx.store.create_job(&job2).await.unwrap();

        let all = ctx.store.all_jobs().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_all_jobs_empty() {
        let ctx = TestContext::new().await;

        let all = ctx.store.all_jobs().await.unwrap();
        assert!(all.is_empty());
    }
}
