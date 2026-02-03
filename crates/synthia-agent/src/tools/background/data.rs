use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
pub enum BackgroundTaskStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Stopped,
}

impl BackgroundTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            BackgroundTaskStatus::Running => "running",
            BackgroundTaskStatus::Completed => "completed",
            BackgroundTaskStatus::Failed => "failed",
            BackgroundTaskStatus::Stopped => "stopped",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "completed" => BackgroundTaskStatus::Completed,
            "failed" => BackgroundTaskStatus::Failed,
            "stopped" => BackgroundTaskStatus::Stopped,
            _ => BackgroundTaskStatus::Running,
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, BackgroundTaskStatus::Running)
    }
}

impl std::fmt::Display for BackgroundTaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTask {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub status: BackgroundTaskStatus,
    pub pid: Option<u32>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub output: Vec<String>,
    pub error: Vec<String>,
    pub notification_delivered: bool,
}

impl BackgroundTask {
    pub fn new(id: String, command: String, cwd: String) -> Self {
        Self {
            id,
            command,
            cwd,
            status: BackgroundTaskStatus::Running,
            pid: None,
            started_at: chrono::Utc::now().timestamp(),
            ended_at: None,
            exit_code: None,
            output: Vec::new(),
            error: Vec::new(),
            notification_delivered: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.status.is_running()
    }

    pub fn complete(&mut self, exit_code: i32) {
        self.status = if exit_code == 0 {
            BackgroundTaskStatus::Completed
        } else {
            BackgroundTaskStatus::Failed
        };
        self.exit_code = Some(exit_code);
        self.ended_at = Some(chrono::Utc::now().timestamp());
    }

    pub fn stop(&mut self) {
        self.status = BackgroundTaskStatus::Stopped;
        self.ended_at = Some(chrono::Utc::now().timestamp());
    }

    pub fn output_preview(&self, max_lines: usize) -> String {
        if self.output.len() <= max_lines {
            self.output.join("\n")
        } else {
            let start = self.output.len() - max_lines;
            self.output[start..].join("\n")
        }
    }

    pub fn error_preview(&self, max_lines: usize) -> Option<String> {
        if self.error.is_empty() {
            None
        } else if self.error.len() <= max_lines {
            Some(self.error.join("\n"))
        } else {
            let start = self.error.len() - max_lines;
            Some(self.error[start..].join("\n"))
        }
    }
}
