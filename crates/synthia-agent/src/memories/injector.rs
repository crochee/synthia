use std::path::PathBuf;

/// 记忆注入策略
///
/// - 启动加载：session 启动时加载最近 N 条记忆
/// - 按需搜索：agent 主动调用 memory_search
/// - 末尾复述：关键状态推到上下文末尾
pub struct MemoryInjector {
    log_dir: PathBuf,
    max_startup_memories: usize,
}

impl MemoryInjector {
    pub fn new(log_dir: PathBuf, max_startup_memories: usize) -> Self {
        Self {
            log_dir,
            max_startup_memories,
        }
    }

    /// 启动时加载最近记忆
    /// 仅加载决策和错误类型事件，按时间倒序
    pub async fn load_startup_memories(&self) -> Vec<String> {
        let mut memories = Vec::new();

        let entries = match tokio::fs::read_dir(&self.log_dir).await {
            Ok(e) => e,
            Err(_) => return memories,
        };

        let mut entries = entries;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            // 逆序读取行
            let mut lines: Vec<&str> = content.lines().collect();
            lines.reverse();

            for line in lines {
                if memories.len() >= self.max_startup_memories {
                    return memories;
                }

                if let Ok(event) =
                    serde_json::from_str::<serde_json::Value>(line)
                    && let Some(event_type) =
                        event.get("event_type").and_then(|v| v.as_str())
                    && (event_type == "decision" || event_type == "error")
                {
                    memories.push(format!("[{}] {}", event_type, line));
                }
            }
        }

        memories
    }

    /// 生成末尾复述内容
    /// 将最近的决策和错误汇总为结构化摘要
    pub fn generate_recency_summary(
        &self,
        recent_decisions: &[String],
        recent_errors: &[String],
    ) -> String {
        let mut summary = String::from("## Recent Context\n");

        if !recent_decisions.is_empty() {
            summary.push_str("\n### Recent Decisions\n");
            for d in recent_decisions.iter().take(5) {
                summary.push_str(&format!("- {}\n", d));
            }
        }

        if !recent_errors.is_empty() {
            summary.push_str("\n### Recent Errors\n");
            for e in recent_errors.iter().take(3) {
                summary.push_str(&format!("- {}\n", e));
            }
        }

        summary
    }

    /// 获取日志目录路径
    pub fn log_dir(&self) -> &PathBuf {
        &self.log_dir
    }

    /// 获取最大启动记忆数量
    pub fn max_startup_memories(&self) -> usize {
        self.max_startup_memories
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_generate_recency_summary_empty() {
        let injector = MemoryInjector::new(PathBuf::from("/tmp"), 10);
        let summary = injector.generate_recency_summary(&[], &[]);
        assert!(summary.contains("## Recent Context"));
        assert!(!summary.contains("Recent Decisions"));
        assert!(!summary.contains("Recent Errors"));
    }

    #[test]
    fn test_generate_recency_summary_with_decisions() {
        let injector = MemoryInjector::new(PathBuf::from("/tmp"), 10);
        let decisions =
            vec!["selected JWT".to_string(), "chosen SQLite".to_string()];
        let summary = injector.generate_recency_summary(&decisions, &[]);
        assert!(summary.contains("Recent Decisions"));
        assert!(summary.contains("selected JWT"));
        assert!(summary.contains("chosen SQLite"));
    }

    #[test]
    fn test_generate_recency_summary_with_errors() {
        let injector = MemoryInjector::new(PathBuf::from("/tmp"), 10);
        let errors = vec![
            "timeout on API call".to_string(),
            "connection refused".to_string(),
        ];
        let summary = injector.generate_recency_summary(&[], &errors);
        assert!(summary.contains("Recent Errors"));
        assert!(summary.contains("timeout on API call"));
    }

    #[test]
    fn test_generate_recency_summary_limits_entries() {
        let injector = MemoryInjector::new(PathBuf::from("/tmp"), 10);
        let decisions = (0..10)
            .map(|i| format!("decision {}", i))
            .collect::<Vec<_>>();
        let errors = (0..5).map(|i| format!("error {}", i)).collect::<Vec<_>>();
        let summary = injector.generate_recency_summary(&decisions, &errors);

        // 决策最多保留 5 条
        assert!(summary.contains("decision 0"));
        assert!(summary.contains("decision 4"));
        // 错误最多保留 3 条
        assert!(summary.contains("error 0"));
        assert!(summary.contains("error 2"));
        assert!(!summary.contains("error 3"));
    }

    #[tokio::test]
    async fn test_load_startup_memories_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let injector = MemoryInjector::new(temp_dir.path().to_path_buf(), 10);

        let memories = injector.load_startup_memories().await;
        assert!(memories.is_empty());
    }

    #[tokio::test]
    async fn test_load_startup_memories_with_events() {
        let temp_dir = TempDir::new().unwrap();
        let log_file = temp_dir.path().join("events-2024-01-15.jsonl");
        let content = r#"{"event_type":"decision","content":"selected JWT","timestamp":"2024-01-15T10:00:00Z"}
{"event_type":"tool_call","tool":"read_file","timestamp":"2024-01-15T10:01:00Z"}
{"event_type":"error","message":"timeout","timestamp":"2024-01-15T10:02:00Z"}
{"event_type":"decision","content":"chosen SQLite","timestamp":"2024-01-15T10:03:00Z"}"#;
        fs::write(&log_file, content).unwrap();

        let injector = MemoryInjector::new(temp_dir.path().to_path_buf(), 5);
        let memories = injector.load_startup_memories().await;

        // 应该只包含 decision 和 error 类型
        assert!(memories.len() >= 3);
        for memory in &memories {
            assert!(
                memory.starts_with("[decision]")
                    || memory.starts_with("[error]")
            );
        }
    }

    #[tokio::test]
    async fn test_load_startup_memories_respects_limit() {
        let temp_dir = TempDir::new().unwrap();
        let log_file = temp_dir.path().join("events.jsonl");

        // 生成 20 条 decision 事件
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!(
                r#"{{"event_type":"decision","content":"decision {}"}}"#,
                i
            ));
        }
        fs::write(&log_file, lines.join("\n")).unwrap();

        let injector = MemoryInjector::new(temp_dir.path().to_path_buf(), 5);
        let memories = injector.load_startup_memories().await;

        assert!(memories.len() <= 5);
    }

    #[tokio::test]
    async fn test_load_startup_memories_ignores_non_jsonl() {
        let temp_dir = TempDir::new().unwrap();

        // 创建非 JSONL 文件
        fs::write(temp_dir.path().join("events.txt"), "some text").unwrap();
        fs::write(temp_dir.path().join("config.json"), "{}").unwrap();

        let injector = MemoryInjector::new(temp_dir.path().to_path_buf(), 10);
        let memories = injector.load_startup_memories().await;

        assert!(memories.is_empty());
    }

    #[test]
    fn test_injector_constructor() {
        let injector =
            MemoryInjector::new(PathBuf::from("/var/log/synthia"), 20);
        assert_eq!(injector.log_dir().to_str().unwrap(), "/var/log/synthia");
        assert_eq!(injector.max_startup_memories(), 20);
    }
}
