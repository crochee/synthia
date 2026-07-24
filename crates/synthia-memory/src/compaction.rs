use crate::types::CompactionReport;

#[derive(Clone, Debug)]
pub struct CompactionConfig {
    pub soft_trim_ratio: f64,
    pub hard_clear_keep_rounds: usize,
    pub llm_summary_enabled: bool,
    pub summary_max_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            soft_trim_ratio: 0.7,
            hard_clear_keep_rounds: 2,
            llm_summary_enabled: true,
            summary_max_tokens: 512,
        }
    }
}

pub struct CompactionEngine {
    config: CompactionConfig,
}

impl CompactionEngine {
    pub fn new() -> Self {
        Self {
            config: CompactionConfig::default(),
        }
    }

    pub fn with_config(config: CompactionConfig) -> Self {
        Self { config }
    }

    pub fn stage_1_soft_trim(
        &self,
        entries: &[String],
        target_ratio: f64,
    ) -> Vec<String> {
        let target = (entries.len() as f64 * target_ratio) as usize;
        entries.iter().take(target).cloned().collect()
    }

    pub fn stage_2_hard_clear(
        &self,
        entries: &[String],
        keep_recent: usize,
    ) -> Vec<String> {
        let keep = keep_recent.min(entries.len());
        entries
            .iter()
            .rev()
            .take(keep)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn stage_3_differential(&self, entries: &[String]) -> String {
        if self.config.llm_summary_enabled {
            let combined = entries.join("\n");
            let max_chars = self.config.summary_max_tokens * 4;
            if combined.len() > max_chars {
                let truncated: String =
                    combined.chars().take(max_chars).collect();
                format!("[LLM Summary Placeholder] {}", truncated)
            } else {
                format!("[LLM Summary Placeholder] {}", combined)
            }
        } else {
            entries.join("\n").chars().take(1000).collect()
        }
    }

    pub fn compact(
        &self,
        entries: &[String],
        soft_ratio: f64,
    ) -> CompactionReport {
        let tokens_before = entries.iter().map(|e| e.len()).sum();
        let trimmed = self.stage_1_soft_trim(entries, soft_ratio);
        let tokens_after = trimmed.iter().map(|e| e.len()).sum();

        CompactionReport {
            tokens_before,
            tokens_after,
            stage: 1,
        }
    }

    pub fn compact_full(&self, entries: &[String]) -> CompactionReport {
        let tokens_before = entries.iter().map(|e| e.len()).sum();

        let (result, stage) = if tokens_before == 0 {
            (Vec::new(), 0)
        } else {
            let stage1 =
                self.stage_1_soft_trim(entries, self.config.soft_trim_ratio);
            let stage1_tokens: usize = stage1.iter().map(|e| e.len()).sum();
            let budget = tokens_before / 2;

            if stage1_tokens <= budget {
                (stage1, 1)
            } else {
                let stage2 = self.stage_2_hard_clear(
                    entries,
                    self.config.hard_clear_keep_rounds,
                );
                let stage2_tokens: usize = stage2.iter().map(|e| e.len()).sum();

                if stage2_tokens <= budget {
                    (stage2, 2)
                } else {
                    let summary = self.stage_3_differential(entries);
                    (vec![summary], 3)
                }
            }
        };

        let tokens_after = result.iter().map(|e| e.len()).sum();

        CompactionReport {
            tokens_before,
            tokens_after,
            stage,
        }
    }
}

impl Default for CompactionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_trim_30_percent() {
        let engine = CompactionEngine::new();
        let entries: Vec<_> = (0..10).map(|i| format!("entry {}", i)).collect();
        let result = engine.stage_1_soft_trim(&entries, 0.3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_hard_clear_keep_recent() {
        let engine = CompactionEngine::new();
        let entries: Vec<_> = (0..10).map(|i| format!("entry {}", i)).collect();
        let result = engine.stage_2_hard_clear(&entries, 3);
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("entry 7"));
    }

    #[test]
    fn test_hard_clear_keep_more_than_available() {
        let engine = CompactionEngine::new();
        let entries = vec!["a".to_string(), "b".to_string()];
        let result = engine.stage_2_hard_clear(&entries, 5);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_stage_3_differential_enabled() {
        let config = CompactionConfig {
            llm_summary_enabled: true,
            summary_max_tokens: 100,
            ..Default::default()
        };
        let engine = CompactionEngine::with_config(config);
        let entries = vec!["hello world".to_string()];
        let result = engine.stage_3_differential(&entries);
        assert!(result.contains("[LLM Summary Placeholder]"));
    }

    #[test]
    fn test_stage_3_differential_disabled() {
        let config = CompactionConfig {
            llm_summary_enabled: false,
            ..Default::default()
        };
        let engine = CompactionEngine::with_config(config);
        let entries = vec!["hello world".to_string()];
        let result = engine.stage_3_differential(&entries);
        assert!(!result.contains("[LLM Summary Placeholder]"));
    }

    #[test]
    fn test_compact_report() {
        let engine = CompactionEngine::new();
        let entries =
            vec!["hello".to_string(), "world".to_string(), "test".to_string()];
        let report = engine.compact(&entries, 0.5);
        assert_eq!(report.tokens_before, 14);
        assert_eq!(report.stage, 1);
    }

    #[test]
    fn test_compact_full_stage1() {
        let engine = CompactionEngine::new();
        let entries: Vec<_> = (0..20).map(|i| format!("entry {}", i)).collect();
        let report = engine.compact_full(&entries);
        assert!(report.stage >= 1);
        assert!(report.tokens_after < report.tokens_before);
    }

    #[test]
    fn test_compact_full_empty() {
        let engine = CompactionEngine::new();
        let report = engine.compact_full(&[]);
        assert_eq!(report.stage, 0);
        assert_eq!(report.tokens_before, 0);
    }

    #[test]
    fn test_config_default() {
        let config = CompactionConfig::default();
        assert!((config.soft_trim_ratio - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.hard_clear_keep_rounds, 2);
        assert!(config.llm_summary_enabled);
        assert_eq!(config.summary_max_tokens, 512);
    }
}
