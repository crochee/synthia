use std::sync::atomic::{AtomicU64, Ordering};

/// 告警级别
#[derive(Debug, Clone)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// 告警事件
#[derive(Debug, Clone)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 本地告警器
pub struct LocalAlerter {
    consecutive_errors: AtomicU64,
    error_threshold: u64,
}

impl LocalAlerter {
    pub const fn new(error_threshold: u64) -> Self {
        Self {
            consecutive_errors: AtomicU64::new(0),
            error_threshold,
        }
    }

    /// 记录成功操作
    pub fn record_success(&self) {
        self.consecutive_errors.store(0, Ordering::Relaxed);
    }

    /// 记录错误，可能触发告警
    pub fn record_error(&self) -> Option<Alert> {
        let count = self.consecutive_errors.fetch_add(1, Ordering::Relaxed) + 1;

        if count >= self.error_threshold {
            Some(Alert {
                level: AlertLevel::Critical,
                message: format!("{count} consecutive errors detected"),
                timestamp: chrono::Utc::now(),
            })
        } else if count >= self.error_threshold / 2 {
            Some(Alert {
                level: AlertLevel::Warning,
                message: format!(
                    "{count} consecutive errors, approaching threshold"
                ),
                timestamp: chrono::Utc::now(),
            })
        } else {
            None
        }
    }
}

impl Default for LocalAlerter {
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_triggered() {
        let alerter = LocalAlerter::new(4);

        // 2 errors - no alert
        alerter.record_error();
        alerter.record_error();
        assert!(alerter.record_error().is_some()); // 3rd = warning
        assert!(alerter.record_error().is_some()); // 4th = critical
    }

    #[test]
    fn test_success_resets() {
        let alerter = LocalAlerter::new(5);

        alerter.record_error();
        alerter.record_error();
        alerter.record_success(); // resets

        // After reset, need to reach threshold again
        // Warning at count >= 5/2 = 2, Critical at count >= 5
        assert!(alerter.record_error().is_none()); // count = 1
        assert!(alerter.record_error().is_some()); // count = 2 → Warning
    }

    #[test]
    fn test_default_threshold() {
        let alerter = LocalAlerter::default();

        // threshold = 5, so warning at 5/2 = 2
        alerter.record_error(); // 1 - none
        assert!(alerter.record_error().is_some()); // 2 - warning
    }

    #[test]
    fn test_alert_level_is_warning() {
        let alerter = LocalAlerter::new(4);
        alerter.record_error();
        alerter.record_error();
        let alert = alerter.record_error().unwrap(); // 3rd = warning
        match alert.level {
            AlertLevel::Warning => {}
            _ => panic!("Expected Warning level"),
        }
    }

    #[test]
    fn test_alert_level_is_critical() {
        let alerter = LocalAlerter::new(4);
        alerter.record_error();
        alerter.record_error();
        alerter.record_error(); // 3rd = warning
        let alert = alerter.record_error().unwrap(); // 4th = critical
        match alert.level {
            AlertLevel::Critical => {}
            _ => panic!("Expected Critical level"),
        }
    }

    #[test]
    fn test_alert_message_content() {
        let alerter = LocalAlerter::new(4);
        alerter.record_error();
        alerter.record_error();
        alerter.record_error(); // 3rd
        let alert = alerter.record_error().unwrap(); // 4th
        assert!(alert.message.contains("4"));
        assert!(alert.message.contains("consecutive errors"));
    }
}
