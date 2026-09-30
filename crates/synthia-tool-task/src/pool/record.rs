//! Invocation records — the observability half of the pool.
//!
//! [`InvocationRecord`] is what `record_completion` stores;
//! the requested-vs-effective model / tier pair is the point
//! (pi-subagents `onResolved`): a substitution is visible
//! after the fact instead of silent.

use chrono::{DateTime, Utc};
use synthia_harness::SessionEndReason;
use synthia_provider::{ModelTier, TokenUsage};

/// How many finished children stay addressable by id.
///
/// A bound on memory, not a window of interest: only the most recent
/// [`MAX_TOMBSTONES`] completions are kept, oldest evicted first.
pub const MAX_TOMBSTONES: usize = 100;

/// How an invocation ended, derived from the child's
/// [`SessionEndReason`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum InvocationStatus {
    /// The child finished its work.
    #[default]
    Completed,
    /// The child surfaced a fatal error.
    Failed,
    /// The child was cancelled.
    Cancelled,
    /// The child hit its iteration cap without converging.
    MaxIterations,
}

impl InvocationStatus {
    /// Classify a child's terminal reason.
    #[must_use]
    pub fn from_end_reason(reason: &SessionEndReason) -> Self {
        match reason {
            SessionEndReason::Completed => Self::Completed,
            SessionEndReason::Cancelled => Self::Cancelled,
            SessionEndReason::Error(_) => Self::Failed,
            SessionEndReason::MaxIterations => Self::MaxIterations,
        }
    }

    /// Stable lowercase label for log lines and records.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::MaxIterations => "max_iterations",
        }
    }

    /// Whether the child produced a usable result.
    #[must_use]
    pub fn is_success(self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// What one finished child invocation actually ran as.
///
/// The requested-vs-effective pairs are the observability point
/// (pi-subagents `onResolved`): a deployment that routes a spawn with
/// a requested model or tier can see afterwards what the child really
/// used. Each field is optional because the layer that records the
/// completion only fills what it can observe — the delegation seam
/// sees the child's requested model and its usage, while the
/// effective model / tier are known to whoever owns the provider.
#[derive(Clone, Debug, Default)]
pub struct InvocationRecord {
    /// Model the spawn asked for.
    pub requested_model: Option<String>,
    /// Model the child actually sampled with.
    pub effective_model: Option<String>,
    /// Tier the spawn asked for.
    pub requested_tier: Option<ModelTier>,
    /// Tier the child actually ran at.
    pub effective_tier: Option<ModelTier>,
    /// When the child was admitted.
    pub started_at: DateTime<Utc>,
    /// When the child settled.
    pub finished_at: DateTime<Utc>,
    /// Lifetime token usage, accumulated over every sampling pass.
    pub usage: TokenUsage,
    /// Nesting depth of the child (`1` = a child of a top-level
    /// session).
    pub depth: usize,
    /// How the child ended.
    pub status: InvocationStatus,
}

impl InvocationRecord {
    /// Accumulate one sampling pass's usage into the lifetime totals
    /// (pi-subagents `addUsage`). Optional buckets stay `None` until a
    /// provider reports them, then accumulate independently.
    pub fn add_usage(&mut self, delta: &TokenUsage) {
        let usage = &mut self.usage;
        usage.prompt_tokens += delta.prompt_tokens;
        usage.completion_tokens += delta.completion_tokens;
        usage.total_tokens += delta.total_tokens;
        usage.cached_prompt_tokens = add_optional(
            usage.cached_prompt_tokens,
            delta.cached_prompt_tokens,
        );
        usage.cache_read_tokens =
            add_optional(usage.cache_read_tokens, delta.cache_read_tokens);
        usage.cache_write_tokens =
            add_optional(usage.cache_write_tokens, delta.cache_write_tokens);
        usage.reasoning_tokens =
            add_optional(usage.reasoning_tokens, delta.reasoning_tokens);
    }

    /// `true` when both models are known and differ — the child did
    /// not run as asked. Unknown on either side is not a
    /// substitution.
    #[must_use]
    pub fn model_substituted(&self) -> bool {
        substituted(
            self.requested_model.as_deref(),
            self.effective_model.as_deref(),
        )
    }

    /// `true` when both tiers are known and differ.
    #[must_use]
    pub fn tier_substituted(&self) -> bool {
        match (self.requested_tier.as_ref(), self.effective_tier.as_ref()) {
            (Some(requested), Some(effective)) => {
                resolved(requested) != resolved(effective)
            }
            _ => false,
        }
    }
}

/// Resolve one tier's `Override` chain **by reference**, mirroring
/// [`ModelTier::effective`] (which consumes). The comparison above
/// must not clone either side — for an override that would allocate.
fn resolved(tier: &ModelTier) -> &ModelTier {
    let mut cur = tier;
    while let ModelTier::Override(inner) = cur {
        cur = inner;
    }
    cur
}

fn add_optional(into: Option<usize>, delta: Option<usize>) -> Option<usize> {
    match (into, delta) {
        (Some(into), Some(delta)) => Some(into + delta),
        (Some(into), None) => Some(into),
        (None, delta) => delta,
    }
}

fn substituted(requested: Option<&str>, effective: Option<&str>) -> bool {
    match (requested, effective) {
        (Some(requested), Some(effective)) => requested != effective,
        _ => false,
    }
}
