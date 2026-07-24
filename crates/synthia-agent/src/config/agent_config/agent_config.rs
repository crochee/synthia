//! The serialisable agent configuration ([`AgentConfig`]) plus its
//! `Default` impl, `validate()` invariant check, and the bridge into
//! the `AgentsMdConfig` consumed by the prompt builder.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use synthia_context::prompt::sections::agents_md::{
    AgentsMdConfig,
    DEFAULT_FILENAME,
    DEFAULT_MAX_CHARS_PER_FILE,
    DEFAULT_MAX_CHARS_TOTAL,
};
use synthia_core::Error;
use synthia_provider::types::ProviderConfig;
use synthia_session::types::TokenBudget;

use super::{
    enums::{AgentImplementation, ExecutorKindConfig},
    observability::ObservabilityConfigInner,
};

/// Static, on-disk agent configuration.
///
/// Serialised to TOML/JSON via serde. For the runtime counterparts
/// (provider, tool registry, hook registry, etc.) see
/// [`AgentRunConfig`](super::run_config::AgentRunConfig).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    pub model: String,
    pub max_tokens: usize,
    pub max_iterations: usize,
    pub temperature: Option<f64>,
    pub workspace_root: std::path::PathBuf,
    pub token_budget: Option<usize>,
    pub checkpoint_dir: Option<std::path::PathBuf>,
    #[serde(skip, default = "default_context_token_budget")]
    pub context_token_budget: Option<TokenBudget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityConfigInner>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_provider: Option<ProviderConfig>,
    #[serde(default)]
    pub agent_implementation: AgentImplementation,
    #[serde(default)]
    pub executor_kind: ExecutorKindConfig,
    /// Master switch for `AGENTS.md` hierarchical discovery.
    ///
    /// When `true` (default), the prompt builder walks `workspace_root`'s
    /// ancestor directories looking for files named in
    /// `agents_md_filenames`, merges them farthest-to-closest, and
    /// injects the merged content into the system prompt.
    #[serde(default = "default_agents_md_enabled")]
    pub agents_md_enabled: bool,
    /// Filenames to look for at each ancestor directory level. The order
    /// within this list is the tie-breaker when multiple matches exist
    /// at the same ancestor level (first listed wins).
    #[serde(default = "default_agents_md_filenames")]
    pub agents_md_filenames: Vec<String>,
    /// 会话墙上时钟超时阈值。超过该时长后主循环停止。
    /// 默认 30 分钟。设为 `None` 或 `Some(Duration::ZERO)` 表示禁用。
    #[serde(default = "default_session_wall_clock_timeout")]
    pub session_wall_clock_timeout: Option<Duration>,
    /// Number of identical tool calls before triggering doom-loop detection.
    /// Default: 3.
    #[serde(default = "default_doom_loop_threshold")]
    pub doom_loop_threshold: usize,
}

fn default_context_token_budget() -> Option<TokenBudget> {
    Some(TokenBudget::default())
}

fn default_agents_md_enabled() -> bool {
    true
}

fn default_agents_md_filenames() -> Vec<String> {
    vec!["AGENTS.md".to_string()]
}

fn default_session_wall_clock_timeout() -> Option<Duration> {
    Some(Duration::from_secs(1800))
}

fn default_doom_loop_threshold() -> usize {
    3
}

impl AgentConfig {
    /// Validate the configuration invariants:
    /// - `max_iterations > 0`
    /// - If `context_token_budget` is set, thresholds must be monotonic:
    ///   `hard_limit > compaction_at > soft_limit > 0`.
    pub fn validate(&self) -> Result<(), Error> {
        if self.max_iterations == 0 {
            return Err(Error::Validation(format!(
                "max_iterations must be greater than 0, got {}",
                self.max_iterations
            )));
        }

        if let Some(ref budget) = self.context_token_budget
            && !(budget.soft_limit > 0
                && budget.compaction_at > budget.soft_limit
                && budget.hard_limit > budget.compaction_at)
        {
            return Err(Error::Validation(format!(
                "token budget thresholds must satisfy hard_limit ({}) > soft_limit ({}) > compaction_at ({}) > 0",
                budget.hard_limit, budget.soft_limit, budget.compaction_at
            )));
        }

        Ok(())
    }

    pub fn builder() -> super::agent_config_builder::AgentConfigBuilder {
        super::agent_config_builder::AgentConfigBuilder::default()
    }

    /// Build an `AgentsMdConfig` for the `AgentsMdSection` from this
    /// agent's `agents_md_*` fields, applying library defaults for the
    /// per-file and total size caps.
    pub fn agents_md_config(&self) -> AgentsMdConfig {
        let filenames = if self.agents_md_filenames.is_empty() {
            vec![DEFAULT_FILENAME.to_string()]
        } else {
            self.agents_md_filenames.clone()
        };
        AgentsMdConfig {
            enabled: self.agents_md_enabled,
            filenames,
            max_chars_per_file: DEFAULT_MAX_CHARS_PER_FILE,
            max_chars_total: DEFAULT_MAX_CHARS_TOTAL,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            max_iterations: 90,
            temperature: None,
            workspace_root: std::path::PathBuf::from("."),
            token_budget: None,
            checkpoint_dir: None,
            context_token_budget: Some(TokenBudget::default()),
            observability: None,
            compaction_provider: None,
            agent_implementation: AgentImplementation::default(),
            executor_kind: ExecutorKindConfig::default(),
            agents_md_enabled: true,
            agents_md_filenames: vec!["AGENTS.md".to_string()],
            session_wall_clock_timeout: default_session_wall_clock_timeout(),
            doom_loop_threshold: default_doom_loop_threshold(),
        }
    }
}
