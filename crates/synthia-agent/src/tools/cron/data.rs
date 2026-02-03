use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronJob {
    #[schemars(description = "Unique identifier")]
    pub id: String,
    #[schemars(
        description = "Cron expression (6-field: second minute hour day month weekday)"
    )]
    pub crontab: String,
    #[schemars(description = "Human-readable description")]
    pub description: String,
    #[schemars(description = "Job content: agent prompt")]
    pub content: String,
    #[schemars(description = "Whether the job is enabled")]
    pub enabled: bool,
    #[schemars(description = "Creation timestamp")]
    pub created_at: DateTime<Utc>,
    #[schemars(description = "Next scheduled run time")]
    pub next_run: Option<DateTime<Utc>>,
    #[schemars(description = "Last run timestamp")]
    pub last_run: Option<DateTime<Utc>>,
    #[schemars(description = "Last run status")]
    pub last_status: Option<String>,
    #[schemars(description = "Last run output")]
    pub last_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronRun {
    #[schemars(description = "Run ID")]
    pub id: i64,
    #[schemars(description = "Job ID")]
    pub job_id: String,
    #[schemars(description = "Run start timestamp")]
    pub started_at: DateTime<Utc>,
    #[schemars(description = "Run finish timestamp")]
    pub finished_at: DateTime<Utc>,
    #[schemars(description = "Run status")]
    pub status: String,
    #[schemars(description = "Run output")]
    pub output: Option<String>,
    #[schemars(description = "Run duration in milliseconds")]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CronJobPatch {
    #[schemars(description = "New cron expression")]
    pub crontab: Option<String>,
    #[schemars(description = "New description")]
    pub description: Option<String>,
    #[schemars(description = "New job content")]
    pub content: Option<String>,
    #[schemars(description = "Enable or disable the job")]
    pub enabled: Option<bool>,
    #[schemars(description = "Next scheduled run time")]
    pub next_run: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_job_serialization() {
        let job = CronJob {
            id: "test-job-1".to_string(),
            crontab: "0 0 * * * *".to_string(),
            description: "Test job".to_string(),
            content: "echo hello".to_string(),
            enabled: true,
            created_at: Utc::now(),
            next_run: Some(Utc::now()),
            last_run: None,
            last_status: None,
            last_output: None,
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: CronJob = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test-job-1");
        assert_eq!(deserialized.crontab, "0 0 * * * *");
        assert_eq!(deserialized.description, "Test job");
        assert_eq!(deserialized.content, "echo hello");
        assert!(deserialized.enabled);
        assert!(deserialized.next_run.is_some());
    }

    #[test]
    fn test_cron_job_json_schema_generation() {
        let schema = schemars::schema_for!(CronJob);
        let json = serde_json::to_string(&schema);
        assert!(json.is_ok());
    }

    #[test]
    fn test_cron_run_serialization() {
        let run = CronRun {
            id: 42,
            job_id: "job-123".to_string(),
            started_at: Utc::now(),
            finished_at: Utc::now(),
            status: "ok".to_string(),
            output: Some("success output".to_string()),
            duration_ms: Some(1500),
        };

        let json = serde_json::to_string(&run).unwrap();
        let deserialized: CronRun = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, 42);
        assert_eq!(deserialized.job_id, "job-123");
        assert_eq!(deserialized.status, "ok");
        assert_eq!(deserialized.output, Some("success output".to_string()));
        assert_eq!(deserialized.duration_ms, Some(1500));
    }

    #[test]
    fn test_cron_run_without_optional_fields() {
        let json = r#"{
            "id": 1,
            "job_id": "job-abc",
            "started_at": "2024-01-01T00:00:00Z",
            "finished_at": "2024-01-01T00:01:00Z",
            "status": "error"
        }"#;

        let run: CronRun = serde_json::from_str(json).unwrap();

        assert_eq!(run.id, 1);
        assert_eq!(run.job_id, "job-abc");
        assert_eq!(run.status, "error");
        assert!(run.output.is_none());
        assert!(run.duration_ms.is_none());
    }

    #[test]
    fn test_cron_job_patch_serialization() {
        let patch = CronJobPatch {
            crontab: Some("0 30 * * * *".to_string()),
            description: None,
            content: Some("new content".to_string()),
            enabled: Some(false),
            next_run: None,
        };

        let json = serde_json::to_string(&patch).unwrap();
        let deserialized: CronJobPatch = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.crontab, Some("0 30 * * * *".to_string()));
        assert!(deserialized.description.is_none());
        assert_eq!(deserialized.content, Some("new content".to_string()));
        assert_eq!(deserialized.enabled, Some(false));
    }

    #[test]
    fn test_cron_job_patch_default() {
        let patch = CronJobPatch::default();

        assert!(patch.crontab.is_none());
        assert!(patch.description.is_none());
        assert!(patch.content.is_none());
        assert!(patch.enabled.is_none());
        assert!(patch.next_run.is_none());
    }

    #[test]
    fn test_cron_job_patch_partial_update() {
        let json = r#"{"enabled": false}"#;
        let patch: CronJobPatch = serde_json::from_str(json).unwrap();

        assert!(patch.crontab.is_none());
        assert!(patch.enabled.is_some());
        assert_eq!(patch.enabled, Some(false));
    }

    #[test]
    fn test_cron_job_all_fields_present() {
        let now = Utc::now();
        let job = CronJob {
            id: "full-job".to_string(),
            crontab: "0 0 9-17 * * 1-5".to_string(),
            description: "Work hours job".to_string(),
            content: "do work".to_string(),
            enabled: true,
            created_at: now,
            next_run: Some(now),
            last_run: Some(now),
            last_status: Some("ok".to_string()),
            last_output: Some("completed".to_string()),
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: CronJob = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "full-job");
        assert!(deserialized.last_run.is_some());
        assert_eq!(deserialized.last_status, Some("ok".to_string()));
        assert_eq!(deserialized.last_output, Some("completed".to_string()));
    }
}
