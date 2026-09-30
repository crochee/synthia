//! [`ModelTier`] — coarse capacity classification for an LLM.
//!
//! Synthia consumers want to **adapt steering density and tool
//! visibility to model capacity** without round-tripping a network call
//! to re-resolve the tier every loop iteration. R10 ships the cheap
//! tier resolver.
//!
//! ## Why not a method on `Provider::model_info()`?
//!
//! `Provider::model_info()` exists for the dsh parity surface, but it
//! returns a `ModelInfo` struct (per-model metadata) and is async. The
//! steering layer needs a `Clone`, sync value resolved once at
//! `Agent::build()` time. The convention is:
//!
//! 1. At agent construction, the builder calls `provider.model_config()`
//!    once, derives `ModelTier` from `context_window`, and threads the
//!    tier through `AgentRunConfig`.
//! 2. The steering facade `Steering::for_tier(tier)` reads the tier
//!    from the bundle to pick guard/hint/tracker preset densities.
//! 3. The tool registry wrapper `AdaptiveRegistry` reads the tier to
//!    decide which tool groups are visible.
//!
//! Adopted from traitclaw `crates/traitclaw-core/src/types/model_info.rs:11-25`
//! (`ModelTier::{Small, Medium, Large}`) +
//! `crates/traitclaw-core/src/registries.rs:373-457`
//! (`AdaptiveRegistry::TierLimits`).

use serde::{Deserialize, Serialize};

use crate::types::ModelConfig;

/// Coarse capacity bucket. Cheap to clone, no allocation, no I/O.
///
/// Default mapping (derived from `ModelConfig::context_window`):
///
/// | `context_window` | Tier    |
/// |------------------|---------|
/// | `>= 100_000`     | `Large` |
/// | `>= 32_000`      | `Medium`|
/// | `<  32_000`      | `Small` |
///
/// Override via [`ModelTier::Override`] (a manual escape hatch) when
/// the heuristic does not match reality — e.g. a small model with
/// extended thinking, or a large-context model that is still
/// instruction-following-limited.
#[derive(
    Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize,
)]
#[non_exhaustive]
pub enum ModelTier {
    /// `<= 32K` context. Tight steering; small tool set; concurrency 1.
    #[default]
    Small,
    /// `32K < N <= 100K` context. Default steering; medium tool set;
    /// concurrency 2.
    Medium,
    /// `> 100K` context, usually with reasoning. Loose steering; full
    /// tool set; concurrency up to `AdaptiveTracker::default_max`.
    Large,
    /// Caller-supplied override. The inner `ModelTier` carries the
    /// effective tier; the override just bypasses the heuristic.
    /// Useful for testing (e.g. force `Small` for a `Large` model).
    Override(Box<ModelTier>),
}

impl ModelTier {
    /// Wire tag for serialisation / log filters.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::Override(inner) => match &**inner {
                Self::Small => "override_small",
                Self::Medium => "override_medium",
                Self::Large => "override_large",
                // Double-overrides collapse to `override_large` so
                // consumers never see nested `override_override_*`.
                Self::Override(_) => "override_large",
            },
        }
    }

    /// Resolve the override to a flat tier. `Override(x)` returns `x`;
    /// flat variants return themselves. The loop terminates because
    /// each recursion unwraps one `Box` layer.
    #[must_use]
    pub fn effective(self) -> Self {
        let mut cur = self;
        loop {
            match cur {
                Self::Override(inner) => cur = *inner,
                other => return other,
            }
        }
    }

    /// Cheap heuristic: derive the tier from a [`ModelConfig`].
    ///
    /// Override the result via [`Self::Override`] when the heuristic
    /// is wrong (a `Large` model that is instruction-limited, a
    /// `Small` model with extended thinking, etc.).
    #[must_use]
    pub fn from_model_config(config: &ModelConfig) -> Self {
        match config.context_window {
            n if n >= 100_000 => Self::Large,
            n if n >= 32_000 => Self::Medium,
            _ => Self::Small,
        }
    }

    /// True when the tier is at least `Medium` (i.e. `Medium` or
    /// `Large`). Steering callers use this as the single-source gate
    /// for "safe to enable parallel tool execution".
    #[must_use]
    pub fn supports_parallel_tools(self) -> bool {
        matches!(self.effective(), Self::Medium | Self::Large)
    }
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ModelTier {
    type Err = TierParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "small" => Ok(Self::Small),
            "medium" => Ok(Self::Medium),
            "large" => Ok(Self::Large),
            "override_small" => Ok(Self::Override(Box::new(Self::Small))),
            "override_medium" => Ok(Self::Override(Box::new(Self::Medium))),
            "override_large" => Ok(Self::Override(Box::new(Self::Large))),
            other => Err(TierParseError(other.to_string())),
        }
    }
}

/// Error returned when a string cannot be parsed as a [`ModelTier`].
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown model tier: {0}")]
pub struct TierParseError(pub String);

/// Per-tier steering density + tool-visibility knobs.
///
/// Adopted from traitclaw
/// `crates/traitclaw-core/src/registries.rs:373-457`
/// (`AdaptiveRegistry::TierLimits`). One preset per tier;
/// `for_tier(t)` reads the matching preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierLimits {
    /// Maximum number of tool definitions visible to the model.
    /// Beyond this, `AdaptiveRegistry` hides lower-priority tools.
    pub max_visible_tools: usize,
    /// `LoopDetectionGuard` fingerprint tail length.
    pub loop_detection_window: usize,
    /// `ToolBudgetGuard` per-run tool-call cap.
    pub tool_budget: usize,
    /// `AdaptiveTracker` concurrency cap.
    pub max_concurrency: usize,
    /// `ContextBudgetHint` utilisation threshold (0.0–1.0).
    pub context_budget_threshold: f32,
}

impl TierLimits {
    /// Preset for the `Large` tier: loose safety, full tool set,
    /// parallel execution.
    pub const LARGE: Self = Self {
        max_visible_tools: 32,
        loop_detection_window: 5,
        tool_budget: 250,
        max_concurrency: 8,
        context_budget_threshold: 0.80,
    };
    /// Preset for the `Medium` tier: default safety, balanced tool set.
    pub const MEDIUM: Self = Self {
        max_visible_tools: 12,
        loop_detection_window: 3,
        tool_budget: 100,
        max_concurrency: 3,
        context_budget_threshold: 0.75,
    };
    /// Preset for the `Small` tier: tightest safety, smallest tool set.
    pub const SMALL: Self = Self {
        max_visible_tools: 5,
        loop_detection_window: 3,
        tool_budget: 50,
        max_concurrency: 1,
        context_budget_threshold: 0.50,
    };

    /// Look up the preset matching a [`ModelTier`].
    #[must_use]
    pub fn for_tier(tier: ModelTier) -> Self {
        match tier.effective() {
            ModelTier::Small => Self::SMALL,
            ModelTier::Medium => Self::MEDIUM,
            ModelTier::Large => Self::LARGE,
            // `effective()` flattens overrides, so this synthetic arm
            // is unreachable. `Large` is the safe upper bound.
            ModelTier::Override(_) => Self::LARGE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(context_window: usize) -> ModelConfig {
        ModelConfig {
            name: "test-model".into(),
            provider: "test".into(),
            context_window,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    #[test]
    fn tier_heuristic_boundaries() {
        assert_eq!(ModelTier::from_model_config(&cfg(8_000)), ModelTier::Small);
        assert_eq!(
            ModelTier::from_model_config(&cfg(32_000)),
            ModelTier::Medium
        );
        assert_eq!(
            ModelTier::from_model_config(&cfg(100_000)),
            ModelTier::Large
        );
        assert_eq!(
            ModelTier::from_model_config(&cfg(200_000)),
            ModelTier::Large
        );
    }
    #[test]
    fn tier_override_roundtrip() {
        let ov = ModelTier::Override(Box::new(ModelTier::Small));
        assert_eq!(ov.clone().effective(), ModelTier::Small);
        assert_eq!(ov.as_str(), "override_small");
        let parsed: ModelTier = "override_small".parse().unwrap();
        assert_eq!(parsed, ov);
    }

    #[test]
    fn tier_parse_error_is_typed() {
        let err = "ultra".parse::<ModelTier>().unwrap_err();
        assert_eq!(err, TierParseError("ultra".into()));
    }

    #[test]
    fn tier_default_is_small() {
        // `Small` is marked `#[default]` so the safe default is the
        // tightest tier (most conservative steering).
        assert_eq!(ModelTier::default(), ModelTier::Small);
    }

    #[test]
    fn supports_parallel_tools_only_medium_plus() {
        assert!(!ModelTier::Small.supports_parallel_tools());
        assert!(ModelTier::Medium.supports_parallel_tools());
        assert!(ModelTier::Large.supports_parallel_tools());
        assert!(
            !ModelTier::Override(Box::new(ModelTier::Small))
                .supports_parallel_tools()
        );
        assert!(
            ModelTier::Override(Box::new(ModelTier::Large))
                .supports_parallel_tools()
        );
    }

    #[test]
    fn tier_limits_preset_for_each_tier() {
        assert_eq!(TierLimits::for_tier(ModelTier::Small).max_visible_tools, 5);
        assert_eq!(
            TierLimits::for_tier(ModelTier::Medium).max_visible_tools,
            12
        );
        assert_eq!(
            TierLimits::for_tier(ModelTier::Large).max_visible_tools,
            32
        );
    }

    #[test]
    fn tier_limits_override_falls_through() {
        let via_override_small = TierLimits::for_tier(ModelTier::Override(
            Box::new(ModelTier::Small),
        ));
        let via_flat_small = TierLimits::SMALL;
        assert_eq!(via_override_small, via_flat_small);
    }

    #[test]
    fn tier_display_matches_as_str() {
        assert_eq!(format!("{}", ModelTier::Small), "small");
        assert_eq!(
            format!("{}", ModelTier::Override(Box::new(ModelTier::Large))),
            "override_large"
        );
    }

    #[test]
    fn tier_effective_unwraps_nested_overrides() {
        let nested = ModelTier::Override(Box::new(ModelTier::Override(
            Box::new(ModelTier::Small),
        )));
        assert_eq!(nested.effective(), ModelTier::Small);
    }
}
