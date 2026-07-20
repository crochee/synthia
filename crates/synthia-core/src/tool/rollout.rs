//! RolloutTracker — 文件变更版本控制 + token 预算追踪。
//!
//! Phase 3.3: 追踪 rollout 过程中的文件变更和 token 使用情况，
//! 提供变更统计和 token 预算管理能力。

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 文件变更类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

/// 单个文件变更记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub tool_name: String,
    pub iteration: usize,
}

/// Token 预算追踪。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    pub total_used: usize,
    pub hard_limit: Option<usize>,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub cached_tokens: usize,
}

impl TokenBudget {
    /// 创建无上限的 TokenBudget。
    pub fn new() -> Self {
        Self {
            total_used: 0,
            hard_limit: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
        }
    }

    /// 创建带硬上限的 TokenBudget。
    pub fn with_limit(hard_limit: usize) -> Self {
        Self {
            total_used: 0,
            hard_limit: Some(hard_limit),
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_tokens: 0,
        }
    }

    /// 记录一次 token 使用。
    pub fn record(&mut self, prompt: usize, completion: usize, cached: usize) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.cached_tokens += cached;
        self.total_used += prompt + completion;
    }

    /// 返回剩余 token 额度，无上限时返回 None。
    pub fn remaining(&self) -> Option<usize> {
        self.hard_limit
            .map(|limit| limit.saturating_sub(self.total_used))
    }

    /// 判断 token 是否已耗尽（仅在有上限时可能为 true）。
    pub fn is_exhausted(&self) -> bool {
        match self.hard_limit {
            Some(limit) => self.total_used >= limit,
            None => false,
        }
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new()
    }
}

/// Rollout 汇总信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutSummary {
    pub total_changes: usize,
    pub files_created: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
    pub total_tokens_used: usize,
    pub token_remaining: Option<usize>,
}

/// RolloutTracker — 追踪文件变更和 token 使用。
pub struct RolloutTracker {
    changes: RwLock<Vec<FileChange>>,
    token_budget: RwLock<TokenBudget>,
}

impl RolloutTracker {
    /// 创建无 token 上限的 RolloutTracker。
    pub fn new() -> Self {
        Self {
            changes: RwLock::new(Vec::new()),
            token_budget: RwLock::new(TokenBudget::new()),
        }
    }

    /// 创建带 token 硬上限的 RolloutTracker。
    pub fn new_with_token_limit(hard_limit: usize) -> Self {
        Self {
            changes: RwLock::new(Vec::new()),
            token_budget: RwLock::new(TokenBudget::with_limit(hard_limit)),
        }
    }

    /// 记录文件变更。
    pub async fn record_change(&self, change: FileChange) {
        self.changes.write().await.push(change);
    }

    /// 记录 token 使用。
    pub async fn record_token_usage(
        &self,
        prompt: usize,
        completion: usize,
        cached: usize,
    ) {
        self.token_budget
            .write()
            .await
            .record(prompt, completion, cached);
    }

    /// 获取所有变更。
    pub async fn changes(&self) -> Vec<FileChange> {
        self.changes.read().await.clone()
    }

    /// 获取 token 预算快照。
    pub async fn token_budget(&self) -> TokenBudget {
        self.token_budget.read().await.clone()
    }

    /// 获取按工具分组的变更统计。
    pub async fn changes_by_tool(&self) -> HashMap<String, usize> {
        let changes = self.changes.read().await;
        let mut map = HashMap::new();
        for change in changes.iter() {
            *map.entry(change.tool_name.clone()).or_insert(0) += 1;
        }
        map
    }

    /// 获取按变更类型分组的统计。
    pub async fn changes_by_type(&self) -> HashMap<ChangeType, usize> {
        let changes = self.changes.read().await;
        let mut map = HashMap::new();
        for change in changes.iter() {
            *map.entry(change.change_type).or_insert(0) += 1;
        }
        map
    }

    /// 汇总信息。
    pub async fn summary(&self) -> RolloutSummary {
        let changes = self.changes.read().await;
        let budget = self.token_budget.read().await;

        let files_created = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Created)
            .count();
        let files_modified = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .count();
        let files_deleted = changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Deleted)
            .count();

        RolloutSummary {
            total_changes: changes.len(),
            files_created,
            files_modified,
            files_deleted,
            total_tokens_used: budget.total_used,
            token_remaining: budget.remaining(),
        }
    }
}

impl Default for RolloutTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_tracker_is_empty() {
        let tracker = RolloutTracker::new();
        assert!(tracker.changes().await.is_empty());
        let budget = tracker.token_budget().await;
        assert_eq!(budget.total_used, 0);
    }

    #[tokio::test]
    async fn record_change_counts() {
        let tracker = RolloutTracker::new();
        let change = FileChange {
            path: PathBuf::from("src/main.rs"),
            change_type: ChangeType::Created,
            tool_name: "write".to_string(),
            iteration: 1,
        };
        tracker.record_change(change).await;
        assert_eq!(tracker.changes().await.len(), 1);
    }

    #[tokio::test]
    async fn record_token_usage_accumulates() {
        let tracker = RolloutTracker::new();
        tracker.record_token_usage(100, 50, 20).await;
        tracker.record_token_usage(200, 80, 30).await;

        let budget = tracker.token_budget().await;
        assert_eq!(budget.prompt_tokens, 300);
        assert_eq!(budget.completion_tokens, 130);
        assert_eq!(budget.cached_tokens, 50);
        assert_eq!(budget.total_used, 430);
    }

    #[tokio::test]
    async fn token_budget_with_limit_remaining() {
        let mut budget = TokenBudget::with_limit(1000);
        budget.record(300, 200, 50);
        assert_eq!(budget.remaining(), Some(500));
        assert!(!budget.is_exhausted());
    }

    #[tokio::test]
    async fn token_budget_is_exhausted() {
        let mut budget = TokenBudget::with_limit(500);
        budget.record(300, 200, 0);
        assert_eq!(budget.remaining(), Some(0));
        assert!(budget.is_exhausted());

        // 超限后仍然追踪
        budget.record(100, 0, 0);
        assert!(budget.is_exhausted());
    }

    #[tokio::test]
    async fn changes_by_tool_groups_correctly() {
        let tracker = RolloutTracker::new();
        tracker
            .record_change(FileChange {
                path: PathBuf::from("a.rs"),
                change_type: ChangeType::Created,
                tool_name: "write".to_string(),
                iteration: 1,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("b.rs"),
                change_type: ChangeType::Modified,
                tool_name: "write".to_string(),
                iteration: 2,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("c.rs"),
                change_type: ChangeType::Deleted,
                tool_name: "delete".to_string(),
                iteration: 3,
            })
            .await;

        let by_tool = tracker.changes_by_tool().await;
        assert_eq!(by_tool.get("write"), Some(&2));
        assert_eq!(by_tool.get("delete"), Some(&1));
    }

    #[tokio::test]
    async fn changes_by_type_groups_correctly() {
        let tracker = RolloutTracker::new();
        tracker
            .record_change(FileChange {
                path: PathBuf::from("a.rs"),
                change_type: ChangeType::Created,
                tool_name: "write".to_string(),
                iteration: 1,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("b.rs"),
                change_type: ChangeType::Created,
                tool_name: "write".to_string(),
                iteration: 2,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("c.rs"),
                change_type: ChangeType::Modified,
                tool_name: "edit".to_string(),
                iteration: 3,
            })
            .await;

        let by_type = tracker.changes_by_type().await;
        assert_eq!(by_type.get(&ChangeType::Created), Some(&2));
        assert_eq!(by_type.get(&ChangeType::Modified), Some(&1));
        assert_eq!(by_type.get(&ChangeType::Deleted), None);
    }

    #[tokio::test]
    async fn summary_aggregates_correctly() {
        let tracker = RolloutTracker::new_with_token_limit(1000);
        tracker
            .record_change(FileChange {
                path: PathBuf::from("a.rs"),
                change_type: ChangeType::Created,
                tool_name: "write".to_string(),
                iteration: 1,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("b.rs"),
                change_type: ChangeType::Modified,
                tool_name: "edit".to_string(),
                iteration: 2,
            })
            .await;
        tracker
            .record_change(FileChange {
                path: PathBuf::from("c.rs"),
                change_type: ChangeType::Deleted,
                tool_name: "rm".to_string(),
                iteration: 3,
            })
            .await;
        tracker.record_token_usage(200, 100, 50).await;

        let summary = tracker.summary().await;
        assert_eq!(summary.total_changes, 3);
        assert_eq!(summary.files_created, 1);
        assert_eq!(summary.files_modified, 1);
        assert_eq!(summary.files_deleted, 1);
        assert_eq!(summary.total_tokens_used, 300);
        assert_eq!(summary.token_remaining, Some(700));
    }

    #[tokio::test]
    async fn multiple_changes_tracked() {
        let tracker = RolloutTracker::new();
        for i in 0..5 {
            tracker
                .record_change(FileChange {
                    path: PathBuf::from(format!("file_{i}.rs")),
                    change_type: ChangeType::Modified,
                    tool_name: "edit".to_string(),
                    iteration: i,
                })
                .await;
        }
        assert_eq!(tracker.changes().await.len(), 5);
    }

    #[tokio::test]
    async fn default_impls_work() {
        let budget = TokenBudget::default();
        assert_eq!(budget.total_used, 0);
        assert!(budget.hard_limit.is_none());

        let tracker = RolloutTracker::default();
        assert!(tracker.changes().await.is_empty());
    }

    #[tokio::test]
    async fn token_budget_no_limit_remaining_is_none() {
        let budget = TokenBudget::new();
        assert_eq!(budget.remaining(), None);
        assert!(!budget.is_exhausted());
    }
}
