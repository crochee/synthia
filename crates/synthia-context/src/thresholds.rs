/// 上下文窗口安全阈值
pub const HARD_MIN_TOKENS: usize = 16_000;
pub const WARN_BELOW_TOKENS: usize = 32_000;

/// 上下文状态
#[derive(Debug, Clone)]
pub struct ContextStatus {
    pub total_tokens: usize,
    pub available_tokens: usize,
    pub usage_percent: f64,
    pub is_critical: bool,
    pub is_warning: bool,
}

impl ContextStatus {
    pub fn new(total_tokens: usize, max_tokens: usize) -> Self {
        let available = max_tokens.saturating_sub(total_tokens);
        Self {
            total_tokens,
            available_tokens: available,
            usage_percent: if max_tokens > 0 {
                (total_tokens as f64 / max_tokens as f64) * 100.0
            } else {
                0.0
            },
            is_critical: available < HARD_MIN_TOKENS,
            is_warning: available < WARN_BELOW_TOKENS,
        }
    }

    /// 是否可以安全执行
    pub fn can_execute(&self) -> bool {
        !self.is_critical
    }

    /// 获取状态描述
    pub fn status_message(&self) -> &str {
        if self.is_critical {
            "CRITICAL: Context window nearly full. Execution blocked."
        } else if self.is_warning {
            "WARNING: Context window getting full. Consider compaction."
        } else {
            "OK: Context window healthy."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_status_healthy() {
        let status = ContextStatus::new(50_000, 200_000);
        assert_eq!(status.available_tokens, 150_000);
        assert!(!status.is_critical);
        assert!(!status.is_warning);
        assert!(status.can_execute());
    }

    #[test]
    fn test_context_status_warning() {
        let status = ContextStatus::new(180_000, 200_000);
        assert_eq!(status.available_tokens, 20_000);
        assert!(!status.is_critical);
        assert!(status.is_warning);
        assert!(status.can_execute());
    }

    #[test]
    fn test_context_status_critical() {
        let status = ContextStatus::new(190_000, 200_000);
        assert_eq!(status.available_tokens, 10_000);
        assert!(status.is_critical);
        assert!(status.is_warning);
        assert!(!status.can_execute());
    }

    #[test]
    fn test_context_status_zero_max() {
        let status = ContextStatus::new(100, 0);
        assert!(status.is_critical);
    }

    #[test]
    fn test_status_messages() {
        let critical = ContextStatus::new(190_000, 200_000);
        assert!(critical.status_message().contains("CRITICAL"));

        let warning = ContextStatus::new(180_000, 200_000);
        assert!(warning.status_message().contains("WARNING"));

        let healthy = ContextStatus::new(50_000, 200_000);
        assert!(healthy.status_message().contains("OK"));
    }
}
