//! Builder for [`AgentConfig`].
//!
//! Each setter returns `Self` (consuming builder) and accumulates a
//! `Some(value)` for the field. The `build()` method falls back to
//! [`AgentConfig::default`] for any unset field, then runs
//! [`AgentConfig::validate`] before returning the final struct.

use std::{path::PathBuf, time::Duration};

use synthia_core::Error;
use synthia_provider::types::ProviderConfig;
use synthia_session::types::TokenBudget;

use super::{
    agent_config::AgentConfig,
    enums::ExecutorKindConfig,
    observability::ObservabilityConfigInner,
};

#[derive(Default)]
pub struct AgentConfigBuilder {
    model: Option<String>,
    max_tokens: Option<usize>,
    max_iterations: Option<usize>,
    temperature: Option<f64>,
    workspace_root: Option<PathBuf>,
    token_budget: Option<usize>,
    checkpoint_dir: Option<PathBuf>,
    context_token_budget: Option<TokenBudget>,
    observability: Option<ObservabilityConfigInner>,
    compaction_provider: Option<ProviderConfig>,
    executor_kind: Option<ExecutorKindConfig>,
    session_wall_clock_timeout: Option<Duration>,
    doom_loop_threshold: Option<usize>,
}

impl AgentConfigBuilder {
    pub fn model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    pub fn max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = Some(n);
        self
    }

    pub fn max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = Some(n);
        self
    }

    pub fn temperature(mut self, t: f64) -> Self {
        self.temperature = Some(t);
        self
    }

    pub fn workspace_root(mut self, p: PathBuf) -> Self {
        self.workspace_root = Some(p);
        self
    }

    pub fn token_budget(mut self, n: usize) -> Self {
        self.token_budget = Some(n);
        self
    }

    pub fn checkpoint_dir(mut self, p: PathBuf) -> Self {
        self.checkpoint_dir = Some(p);
        self
    }

    pub fn token_budget_config(mut self, b: TokenBudget) -> Self {
        self.context_token_budget = Some(b);
        self
    }

    pub fn observability(mut self, obs: ObservabilityConfigInner) -> Self {
        self.observability = Some(obs);
        self
    }

    pub fn compaction_provider(mut self, provider: ProviderConfig) -> Self {
        self.compaction_provider = Some(provider);
        self
    }

    pub fn executor_kind(mut self, kind: ExecutorKindConfig) -> Self {
        self.executor_kind = Some(kind);
        self
    }

    pub fn session_wall_clock_timeout(mut self, timeout: Duration) -> Self {
        self.session_wall_clock_timeout = Some(timeout);
        self
    }

    pub fn doom_loop_threshold(mut self, n: usize) -> Self {
        self.doom_loop_threshold = Some(n);
        self
    }

    pub fn build(self) -> Result<AgentConfig, Error> {
        let defaults = AgentConfig::default();
        let config = AgentConfig {
            model: self.model.unwrap_or(defaults.model),
            max_tokens: self.max_tokens.unwrap_or(defaults.max_tokens),
            max_iterations: self
                .max_iterations
                .unwrap_or(defaults.max_iterations),
            temperature: self.temperature,
            workspace_root: self
                .workspace_root
                .unwrap_or(defaults.workspace_root),
            token_budget: self.token_budget,
            checkpoint_dir: self.checkpoint_dir,
            context_token_budget: self
                .context_token_budget
                .or(defaults.context_token_budget),
            observability: self.observability,
            compaction_provider: self.compaction_provider,
            agent_implementation: defaults.agent_implementation,
            executor_kind: self.executor_kind.unwrap_or(defaults.executor_kind),
            agents_md_enabled: defaults.agents_md_enabled,
            agents_md_filenames: defaults.agents_md_filenames,
            session_wall_clock_timeout: self
                .session_wall_clock_timeout
                .or(defaults.session_wall_clock_timeout),
            doom_loop_threshold: self
                .doom_loop_threshold
                .unwrap_or(defaults.doom_loop_threshold),
        };

        config.validate()?;
        Ok(config)
    }
}
