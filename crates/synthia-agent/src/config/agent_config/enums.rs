//! Serde enums used by [`AgentConfig`] and the executor selector.

/// Internal selector for the agent implementation strategy.
///
/// The `Legacy` arm is reserved for backward compatibility and is
/// expected to be removed once the `StreamBuilder` path is the only
/// supported runtime.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum AgentImplementation {
    Legacy,
    #[default]
    StreamBuilder,
}

/// Which executor to run inside the agent loop.
///
/// - [`Build`](ExecutorKindConfig::Build) — guided, evidence-gated edits.
/// - [`Plan`](ExecutorKindConfig::Plan) — produces a plan without side effects.
/// - [`General`](ExecutorKindConfig::General) — full agent loop.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ExecutorKindConfig {
    #[default]
    Build,
    Plan,
    General,
}
