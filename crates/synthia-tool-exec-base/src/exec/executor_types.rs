use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use thiserror::Error;
use tokio::sync::oneshot;

use super::priority::TaskPriority;

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("task was cancelled")]
    Cancelled,

    #[error("task timed out after {0:?}")]
    Timeout(Duration),

    #[error("executor is shutting down")]
    Shutdown,

    #[error("task execution failed: {0}")]
    Custom(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceUsage {
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
    pub cpu_time_estimate_ms: Option<u64>,
    pub memory_estimate_bytes: Option<u64>,
}

impl ResourceUsage {
    pub fn new() -> Self {
        Self {
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            cpu_time_estimate_ms: None,
            memory_estimate_bytes: None,
        }
    }

    pub fn mark_completed(mut self) -> Self {
        let end_time = Utc::now();
        let duration = end_time.signed_duration_since(self.start_time);
        self.end_time = Some(end_time);
        self.duration =
            Some(Duration::from_millis(duration.num_milliseconds() as u64));

        self.cpu_time_estimate_ms = Self::estimate_cpu_time();
        self.memory_estimate_bytes = Self::estimate_memory();

        self
    }

    #[cfg(target_os = "linux")]
    fn estimate_cpu_time() -> Option<u64> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let parts: Vec<&str> = stat.split_whitespace().collect();
        if parts.len() < 16 {
            return None;
        }

        let utime: u64 = parts[13].parse().ok()?;
        let stime: u64 = parts[14].parse().ok()?;

        let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
        if clk_tck == 0 {
            return None;
        }

        Some(((utime + stime) * 1000) / clk_tck)
    }

    #[cfg(not(target_os = "linux"))]
    fn estimate_cpu_time() -> Option<u64> {
        None
    }

    #[cfg(target_os = "linux")]
    fn estimate_memory() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("VmRSS:") {
                let value = value.trim();
                let kb: u64 =
                    value.trim_end_matches("kB").trim().parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    fn estimate_memory() -> Option<u64> {
        None
    }
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TaskHandle<T: Send + 'static> {
    result_rx:
        parking_lot::Mutex<Option<oneshot::Receiver<Result<T, TaskError>>>>,
    resource_usage: Arc<parking_lot::Mutex<ResourceUsage>>,
    cancelled: Arc<parking_lot::Mutex<bool>>,
    priority: TaskPriority,
    deadline: Option<Instant>,
}

impl<T: Send + 'static> TaskHandle<T> {
    pub(crate) fn new(
        result_rx: oneshot::Receiver<Result<T, TaskError>>,
        resource_usage: Arc<parking_lot::Mutex<ResourceUsage>>,
        cancelled: Arc<parking_lot::Mutex<bool>>,
        priority: TaskPriority,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            result_rx: parking_lot::Mutex::new(Some(result_rx)),
            resource_usage,
            cancelled,
            priority,
            deadline,
        }
    }

    pub async fn await_result(&self) -> Result<T, TaskError> {
        let rx = self.result_rx.lock().take();

        match rx {
            Some(rx) => match rx.await {
                Ok(result) => {
                    let mut usage = self.resource_usage.lock();
                    *usage = std::mem::take(&mut *usage).mark_completed();
                    result
                }
                Err(_) => Err(TaskError::Cancelled),
            },
            None => Err(TaskError::Cancelled),
        }
    }

    pub fn is_completed(&self) -> bool {
        self.result_rx.lock().is_none()
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.lock()
    }

    pub fn priority(&self) -> TaskPriority {
        self.priority
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn resource_usage(&self) -> ResourceUsage {
        self.resource_usage.lock().clone()
    }
}

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub max_concurrent: usize,
    pub default_timeout: Duration,
    pub queue_capacity: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            default_timeout: Duration::from_secs(30),
            queue_capacity: 100,
        }
    }
}
