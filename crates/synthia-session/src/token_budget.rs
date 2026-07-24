use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::{TokenBudgetStatus, TokenUsage};

pub struct TokenBudgetMonitor {
    last_status: std::sync::Mutex<Option<TokenBudgetStatus>>,
    compaction_triggered: AtomicBool,
}

impl TokenBudgetMonitor {
    pub fn new() -> Self {
        Self {
            last_status: std::sync::Mutex::new(None),
            compaction_triggered: AtomicBool::new(false),
        }
    }

    pub fn check_and_log(
        &self,
        session_id: &str,
        usage: &TokenUsage,
        status: &TokenBudgetStatus,
    ) {
        let mut last = self.last_status.lock().unwrap();
        let should_log = last.as_ref() != Some(status);

        if should_log {
            match status {
                TokenBudgetStatus::Notice => {
                    tracing::info!(
                        session_id = %session_id,
                        tokens = %usage.total_tokens,
                        threshold = "70%",
                        "token budget notice: compaction may be needed soon"
                    );
                }
                TokenBudgetStatus::Warning => {
                    tracing::warn!(
                        session_id = %session_id,
                        tokens = %usage.total_tokens,
                        threshold = "85%",
                        "token budget warning: recommend immediate compaction"
                    );
                }
                TokenBudgetStatus::MustCompact => {
                    if !self.compaction_triggered.load(Ordering::SeqCst) {
                        tracing::error!(
                            session_id = %session_id,
                            tokens = %usage.total_tokens,
                            threshold = "90%",
                            "token budget critical: triggering automatic context compaction"
                        );
                        self.compaction_triggered.store(true, Ordering::SeqCst);
                    }
                }
                TokenBudgetStatus::Ok => {
                    tracing::debug!(
                        session_id = %session_id,
                        tokens = %usage.total_tokens,
                        "token budget status: ok"
                    );
                }
            }
            *last = Some(*status);
        }
    }

    pub fn should_trigger_compaction(
        &self,
        status: &TokenBudgetStatus,
    ) -> bool {
        matches!(status, TokenBudgetStatus::MustCompact)
    }

    pub fn reset_compaction_flag(&self) {
        self.compaction_triggered.store(false, Ordering::SeqCst);
    }

    pub fn get_last_status(&self) -> Option<TokenBudgetStatus> {
        *self.last_status.lock().unwrap()
    }
}

impl Default for TokenBudgetMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_notice_level() {
        let monitor = TokenBudgetMonitor::new();
        let usage = TokenUsage {
            prompt_tokens: 700,
            completion_tokens: 0,
            total_tokens: 700,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        monitor.check_and_log(
            "test-session",
            &usage,
            &TokenBudgetStatus::Notice,
        );
        assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Notice));
    }

    #[test]
    fn test_monitor_warning_level() {
        let monitor = TokenBudgetMonitor::new();
        let usage = TokenUsage {
            prompt_tokens: 850,
            completion_tokens: 0,
            total_tokens: 850,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        monitor.check_and_log(
            "test-session",
            &usage,
            &TokenBudgetStatus::Warning,
        );
        assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Warning));
    }

    #[test]
    fn test_monitor_must_compact() {
        let monitor = TokenBudgetMonitor::new();
        let usage = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 0,
            total_tokens: 1000,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        monitor.check_and_log(
            "test-session",
            &usage,
            &TokenBudgetStatus::MustCompact,
        );
        assert!(
            monitor.should_trigger_compaction(&TokenBudgetStatus::MustCompact)
        );

        monitor.reset_compaction_flag();

        let usage_ok = TokenUsage {
            prompt_tokens: 500,
            completion_tokens: 0,
            total_tokens: 500,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        monitor.check_and_log(
            "test-session",
            &usage_ok,
            &TokenBudgetStatus::Ok,
        );

        monitor.check_and_log(
            "test-session",
            &usage,
            &TokenBudgetStatus::MustCompact,
        );
        assert!(
            monitor.should_trigger_compaction(&TokenBudgetStatus::MustCompact)
        );
    }

    #[test]
    fn test_monitor_status_change() {
        let monitor = TokenBudgetMonitor::new();
        let usage_ok = TokenUsage {
            prompt_tokens: 500,
            completion_tokens: 0,
            total_tokens: 500,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let usage_notice = TokenUsage {
            prompt_tokens: 700,
            completion_tokens: 0,
            total_tokens: 700,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };

        monitor.check_and_log(
            "test-session",
            &usage_ok,
            &TokenBudgetStatus::Ok,
        );
        assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Ok));

        monitor.check_and_log(
            "test-session",
            &usage_notice,
            &TokenBudgetStatus::Notice,
        );
        assert_eq!(monitor.get_last_status(), Some(TokenBudgetStatus::Notice));
    }
}
