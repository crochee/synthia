//! Cron tool types

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const CRONTAB_DESCRIPTION: &str = r#"Cron expression. Formats:
- 5-field: `minute hour day month weekday`
- 6-field: `second minute hour day month weekday`

Special chars: `*`(any), `?`(any), `-`(range), `/`(step), `,`(list)

Descriptors: `@yearly`, `@monthly`, `@weekly`, `@daily`, `@hourly`, `@every <duration>`, `@at <timestamp>`, `@delay <duration>`

Timezone: `TZ=<timezone>` prefix (e.g., `TZ=Asia/Shanghai 0 0 * * *`)

Examples: `0 30 * * * *` (hourly at :30), `0 0 9-17 * * 1-5` (work hours weekdays), `@every 2h`"#;

/// Request to add a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Create a new scheduled cron job")]
pub(crate) struct CronAddRequest {
    #[schemars(description = CRONTAB_DESCRIPTION)]
    pub(crate) crontab: String,
    #[schemars(description = "Human-readable description of the job")]
    pub(crate) description: String,
    #[schemars(description = "Job content: agent prompt")]
    pub(crate) content: String,
    #[schemars(description = "Whether the job is enabled")]
    pub(crate) enabled: Option<bool>,
}

/// Request to get a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Get a specific cron job by ID")]
pub(crate) struct CronGetRequest {
    #[schemars(description = "The unique ID of the job")]
    pub(crate) job_id: String,
}

/// Request to remove a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Remove a cron job by ID")]
pub(crate) struct CronRemoveRequest {
    #[schemars(description = "The unique ID of the job to remove")]
    pub(crate) job_id: String,
}

/// Request to update a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Update an existing cron job")]
pub(crate) struct CronUpdateRequest {
    #[schemars(description = "The unique ID of the job to update")]
    pub(crate) job_id: String,
    #[schemars(description = CRONTAB_DESCRIPTION)]
    pub(crate) crontab: Option<String>,
    #[schemars(description = "New description")]
    pub(crate) description: Option<String>,
    #[schemars(description = "New job content")]
    pub(crate) content: Option<String>,
    #[schemars(description = "Enable or disable the job")]
    pub(crate) enabled: Option<bool>,
}

/// Request to run a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Force-run a cron job immediately")]
pub(crate) struct CronRunRequest {
    #[schemars(description = "The unique ID of the job to run")]
    pub(crate) job_id: String,
}

/// Request to list runs of a cron job
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "List run history for a cron job")]
pub(crate) struct CronRunsRequest {
    #[schemars(description = "The unique ID of the job")]
    pub(crate) job_id: String,
    #[schemars(description = "Maximum number of runs to return")]
    pub(crate) limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_add_request_serialization() {
        let request = CronAddRequest {
            crontab: "0 0 * * * *".to_string(),
            description: "Daily job".to_string(),
            content: "echo hello".to_string(),
            enabled: Some(true),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronAddRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.crontab, "0 0 * * * *");
        assert_eq!(deserialized.description, "Daily job");
        assert_eq!(deserialized.content, "echo hello");
        assert_eq!(deserialized.enabled, Some(true));
    }

    #[test]
    fn test_cron_add_request_without_enabled() {
        let json = r#"{"crontab":"0 30 * * * *","description":"Test","content":"job"}"#;
        let request: CronAddRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.crontab, "0 30 * * * *");
        assert!(request.enabled.is_none());
    }

    #[test]
    fn test_cron_get_request_serialization() {
        let request = CronGetRequest {
            job_id: "job-123".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronGetRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.job_id, "job-123");
    }

    #[test]
    fn test_cron_remove_request_serialization() {
        let request = CronRemoveRequest {
            job_id: "job-456".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronRemoveRequest =
            serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.job_id, "job-456");
    }

    #[test]
    fn test_cron_update_request_serialization() {
        let request = CronUpdateRequest {
            job_id: "job-789".to_string(),
            crontab: Some("0 0 * * 1-5".to_string()),
            description: Some("Updated".to_string()),
            content: None,
            enabled: Some(false),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronUpdateRequest =
            serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.job_id, "job-789");
        assert_eq!(deserialized.crontab, Some("0 0 * * 1-5".to_string()));
        assert_eq!(deserialized.description, Some("Updated".to_string()));
        assert!(deserialized.content.is_none());
        assert_eq!(deserialized.enabled, Some(false));
    }

    #[test]
    fn test_cron_run_request_serialization() {
        let request = CronRunRequest {
            job_id: "job-run".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronRunRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.job_id, "job-run");
    }

    #[test]
    fn test_cron_runs_request_serialization() {
        let request = CronRunsRequest {
            job_id: "job-runs".to_string(),
            limit: Some(10),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CronRunsRequest =
            serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.job_id, "job-runs");
        assert_eq!(deserialized.limit, Some(10));
    }

    #[test]
    fn test_cron_runs_request_without_limit() {
        let json = r#"{"job_id":"job-abc"}"#;
        let request: CronRunsRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.job_id, "job-abc");
        assert!(request.limit.is_none());
    }

    #[test]
    fn test_cron_update_request_partial_fields() {
        let json = r#"{"job_id":"job-xyz","enabled":true}"#;
        let request: CronUpdateRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.job_id, "job-xyz");
        assert!(request.crontab.is_none());
        assert!(request.description.is_none());
        assert!(request.content.is_none());
        assert_eq!(request.enabled, Some(true));
    }
}
