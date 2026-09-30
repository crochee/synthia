//! Typed compaction policy (pi `CompactionSettings` parity).
//!
//! R5-1: synthia had two unrelated knobs (the truncation cut-point
//! algorithm inside `TruncatingContextManager` and the LLM-summary
//! trigger inside `SummarizingContextManager`). Operators that
//! wanted to express "compact at 75 % utilisation, reserve 16 k
//! tokens for the summary, keep 20 k tokens of recent context"
//! had to hand-roll env vars. This struct is the typed knob.
//!
//! It mirrors `pi/agent/src/harness/compaction/compaction.ts`'s
//! `CompactionSettings`:
//! ```ts
//! export interface CompactionSettings {
//!     enabled: boolean;
//!     reserveTokens: number;     // tokens reserved for summary prompt + output
//!     keepRecentTokens: number;  // budget for the retained tail after cut
//! }
//! ```
//! plus one synthia-only knob: `min_messages_between_compaction`,
//! which throttle-gates the trigger so a back-to-back overflow
//! doesn't compact every iteration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use synthia_session::TokenMeasurement;

/// Typed policy for `ContextManager::prepare`-time compaction.
///
/// # Defaults
///
/// Mirrors pi's defaults: enable compaction, reserve 16 384 tokens
/// for the summary prompt + output, retain 20 000 tokens of recent
/// context, throttle to one compaction every four iterations.
///
/// # Trigger
///
/// `should_compact` returns `true` when
/// `context_tokens > context_window - reserve_tokens` and
/// `enabled` is `true` and the message-count throttle allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionSettings {
    /// Master switch. `false` makes `should_compact` always return
    /// `false` regardless of utilisation.
    pub enabled: bool,
    /// Tokens reserved for the summary prompt + the LLM output.
    /// Trigger only fires once utilisation crosses
    /// `context_window - reserve_tokens`.
    pub reserve_tokens: u32,
    /// Token budget for the retained tail after cut. The summariser
    /// / truncator picks a cut point such that the post-cut
    /// messages stay under this budget.
    pub keep_recent_tokens: u32,
    /// Minimum number of messages between two consecutive
    /// compactions. Throttle-gates the trigger so a tight-loop
    /// overflow cannot compact every iteration.
    #[serde(default = "default_min_messages_between_compaction")]
    pub min_messages_between_compaction: u32,
}

fn default_min_messages_between_compaction() -> u32 {
    4
}

/// Default settings — matches pi `DEFAULT_COMPACTION_SETTINGS`.
///
/// Operators who want to start from a known-good policy can
/// `let cfg = CompactionSettings::default();` and adjust
/// individual fields.
impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            reserve_tokens: 16_384,
            keep_recent_tokens: 20_000,
            min_messages_between_compaction:
                default_min_messages_between_compaction(),
        }
    }
}

impl CompactionSettings {
    /// Validate the settings before they are committed to a session.
    ///
    /// `Err(ValidationError)` is returned when:
    /// - `reserve_tokens == 0` (would trigger compaction on the
    ///   first message)
    /// - `keep_recent_tokens == 0` (no recent context would survive)
    /// - `reserve_tokens + keep_recent_tokens > u32::MAX` (overflow)
    ///
    /// `reserve_tokens + keep_recent_tokens > context_window` is
    /// allowed (the manager will fall back to pairwise drop in that
    /// case) but operators should know — `SaneForWindow` surfaces
    /// a non-fatal warning via `Option<&'static str>`.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !self.enabled {
            return Ok(());
        }
        if self.reserve_tokens == 0 {
            return Err(ValidationError::ReserveTokensZero);
        }
        if self.keep_recent_tokens == 0 {
            return Err(ValidationError::KeepRecentTokensZero);
        }
        if self
            .reserve_tokens
            .checked_add(self.keep_recent_tokens)
            .is_none()
        {
            return Err(ValidationError::Overflow);
        }
        Ok(())
    }

    /// Returns `Some("...")` when the policy is structurally valid
    /// but the combined reserve + keep budget exceeds the model's
    /// context window. The caller may log this as a soft warning;
    /// compaction still proceeds (the manager falls back to pairwise
    /// drop when the cut cannot satisfy the budget).
    pub fn sane_for_window(&self, context_window: u64) -> Option<&'static str> {
        if !self.enabled {
            return None;
        }
        let total =
            u64::from(self.reserve_tokens) + u64::from(self.keep_recent_tokens);
        if total > context_window {
            Some(
                "reserve_tokens + keep_recent_tokens exceeds context_window; \
                 manager will fall back to pairwise drop",
            )
        } else {
            None
        }
    }

    /// Anchor-aware variant of [`should_compact`] (opt-in).
    ///
    /// Gates on the token meter's projected next-request total
    /// instead of a caller-supplied heuristic count: when the
    /// meter adopted a provider usage sample,
    /// `measurement.total_tokens` carries the provider-anchored
    /// pressure plus the signed heuristic surface delta, so the
    /// trigger reacts to what the next request will actually
    /// cost — including a compaction that just shadowed a span,
    /// which no provider sample reports on its own.
    ///
    /// Without an anchor (`MeasurementBaseline::None` /
    /// `Estimated`) the projection degenerates to the heuristic
    /// surface price, so the decision falls back to the same
    /// threshold predicate as [`should_compact`]. The free
    /// function's behaviour is unchanged; this is the opt-in
    /// path for callers that hold a meter.
    ///
    /// Returns `true` iff the policy is enabled, the projected
    /// total crosses `context_window - reserve_tokens`, and the
    /// message-count throttle allows it — exactly the
    /// [`should_compact`] predicate applied to
    /// [`TokenMeasurement::total_tokens`].
    #[must_use]
    pub fn should_compact_anchored(
        &self,
        measurement: &TokenMeasurement,
        context_window: u64,
        messages_since_last_compaction: u32,
    ) -> bool {
        should_compact(
            measurement.total_tokens,
            context_window,
            messages_since_last_compaction,
            self,
        )
    }
}

/// File-activity detail recorded alongside one compaction so the
/// *next* compaction's summariser prompt can name what touched
/// disk in the most recent run.
///
/// Deliberately **not** a field on [`CompactionSettings`]: the
/// policy is a small `Copy` knob describing *when* to compact,
/// while this is per-compaction data describing what happened.
/// Keeping the two apart lets the policy stay copyable and cheap
/// to pass by value, and lets callers attach details only when
/// they actually have them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionDetails {
    /// Paths the agent read since the previous compaction.
    #[serde(default)]
    pub read_files: Vec<PathBuf>,
    /// Paths the agent modified since the previous compaction.
    #[serde(default)]
    pub modified_files: Vec<PathBuf>,
}

/// Predicate: should the agent loop trigger compaction now?
///
/// Returns `true` iff:
/// - the policy is enabled,
/// - the utilisation predicate fires
///   (`context_tokens > context_window - reserve_tokens`), and
/// - `messages_since_last_compaction >= min_messages_between_compaction`.
#[must_use]
pub fn should_compact(
    context_tokens: u64,
    context_window: u64,
    messages_since_last_compaction: u32,
    settings: &CompactionSettings,
) -> bool {
    if !settings.enabled {
        return false;
    }
    if context_window == 0 {
        return false;
    }
    let reserve = u64::from(settings.reserve_tokens);
    let threshold = context_window.saturating_sub(reserve);
    if context_tokens <= threshold {
        return false;
    }
    messages_since_last_compaction >= settings.min_messages_between_compaction
}

/// Errors returned by [`CompactionSettings::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// `reserve_tokens == 0` would trigger compaction on the
    /// first message.
    ReserveTokensZero,
    /// `keep_recent_tokens == 0` would drop the entire conversation.
    KeepRecentTokensZero,
    /// `reserve_tokens + keep_recent_tokens` overflows `u32`.
    Overflow,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReserveTokensZero => f.write_str(
                "CompactionSettings.reserve_tokens must be > 0 when enabled",
            ),
            Self::KeepRecentTokensZero => {
                f.write_str("CompactionSettings.keep_recent_tokens must be > 0")
            }
            Self::Overflow => f.write_str(
                "CompactionSettings: reserve_tokens + keep_recent_tokens \
                 overflows u32",
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_pi() {
        let s = CompactionSettings::default();
        assert!(s.enabled);
        assert_eq!(s.reserve_tokens, 16_384);
        assert_eq!(s.keep_recent_tokens, 20_000);
        assert_eq!(s.min_messages_between_compaction, 4);
    }

    #[test]
    fn disabled_never_triggers() {
        let s = CompactionSettings {
            enabled: false,
            ..CompactionSettings::default()
        };
        assert!(!should_compact(1_000_000, 200_000, 0, &s));
    }

    #[test]
    fn trigger_only_when_over_threshold() {
        let s = CompactionSettings::default();
        // 200k context, 16k reserve → trigger only above 183_616.
        assert!(!should_compact(183_616, 200_000, 10, &s));
        assert!(should_compact(183_617, 200_000, 10, &s));
    }

    #[test]
    fn validate_zero_reserve_rejected() {
        let s = CompactionSettings {
            reserve_tokens: 0,
            ..CompactionSettings::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            ValidationError::ReserveTokensZero
        );
    }

    #[test]
    fn validate_zero_keep_recent_rejected() {
        let s = CompactionSettings {
            keep_recent_tokens: 0,
            ..CompactionSettings::default()
        };
        assert_eq!(
            s.validate().unwrap_err(),
            ValidationError::KeepRecentTokensZero
        );
    }

    #[test]
    fn sane_for_window_warns_when_budget_overflows() {
        let s = CompactionSettings::default();
        // 200k context, 36k combined budget — sane.
        assert!(s.sane_for_window(40_000).is_none());
        // 30k context, 36k combined — warn.
        assert!(s.sane_for_window(30_000).is_some());
    }

    #[test]
    fn disabled_settings_always_validate() {
        let s = CompactionSettings {
            enabled: false,
            reserve_tokens: 0,
            keep_recent_tokens: 0,
            min_messages_between_compaction: 0,
        };
        assert!(s.validate().is_ok());
        assert!(s.sane_for_window(0).is_none());
    }

    #[test]
    fn compaction_details_default_is_empty() {
        let d = CompactionDetails::default();
        assert!(d.read_files.is_empty());
        assert!(d.modified_files.is_empty());
        // Deserialising a payload with both fields omitted must
        // land on the same empty default (`#[serde(default)]`).
        let from_json: CompactionDetails = serde_json::from_str("{}").unwrap();
        assert_eq!(from_json, d);
        // R29-C: the type is public at the crate root, so callers
        // outside this module can build one.
        let from_root: crate::CompactionDetails = CompactionDetails::default();
        assert_eq!(from_root, d);
    }

    #[test]
    fn compaction_details_round_trip_through_serde() {
        let d = CompactionDetails {
            read_files: vec![PathBuf::from("a.rs"), PathBuf::from("dir/b.rs")],
            modified_files: vec![PathBuf::from("b.rs")],
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: CompactionDetails = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    // -- Anchor-aware gate (R30) --------------------------------------

    /// Settings whose trigger threshold is `context_window - 100`
    /// with the throttle held open in tests.
    fn anchored_settings() -> CompactionSettings {
        CompactionSettings {
            reserve_tokens: 100,
            min_messages_between_compaction: 1,
            ..CompactionSettings::default()
        }
    }

    fn user_event(seq: u64, text: &str) -> synthia_session::SessionEvent {
        use synthia_provider::Message;
        use synthia_session::{SessionEvent, SurfaceOp};
        SessionEvent::UserMessage {
            seq,
            ts: "2026-09-12T00:00:00Z".to_string(),
            data: serde_json::to_value(Message::user(text)).expect("ser"),
            surface_op: Some(SurfaceOp::append()),
        }
    }

    fn compaction_event(seq: u64) -> synthia_session::SessionEvent {
        use synthia_provider::Message;
        use synthia_session::{SessionEvent, SurfaceOp};
        SessionEvent::Compaction {
            seq,
            ts: "2026-09-12T00:00:00Z".to_string(),
            data: serde_json::to_value(Message::assistant("sum")).expect("ser"),
            surface_op: SurfaceOp::Replace {
                start: 0,
                end: 1,
                source_event_seqs: vec![0],
            },
        }
    }

    fn assistant_event(seq: u64) -> synthia_session::SessionEvent {
        use synthia_provider::Message;
        use synthia_session::{SessionEvent, SurfaceOp};
        SessionEvent::AssistantMessage {
            seq,
            ts: "2026-09-12T00:00:00Z".to_string(),
            data: serde_json::to_value(Message::assistant("ok")).expect("ser"),
            surface_op: Some(SurfaceOp::append()),
        }
    }

    fn meter_over(log: &[synthia_session::SessionEvent]) -> TokenMeasurement {
        let mut meter = synthia_session::TokenMeter::new();
        meter.observe(log).expect("fold");
        meter.measure()
    }

    #[test]
    fn anchored_gate_fires_on_provider_pressure_the_heuristic_cannot_see() {
        // The provider counts the full prompt envelope (system +
        // tool schemas) that the surface heuristic cannot price;
        // anchoring is what lets the gate fire on it.
        let settings = anchored_settings();
        let log = vec![
            user_event(0, "hello there"),
            synthia_session::usage(950, 20, 970, None, None, None),
        ];
        let m = meter_over(&log);
        assert!(
            !should_compact(m.surface_tokens, 1_000, 1, &settings),
            "heuristic surface is far below the threshold"
        );
        assert!(settings.should_compact_anchored(&m, 1_000, 1));
    }

    #[test]
    fn anchored_gate_threshold_is_exclusive() {
        let settings = anchored_settings();
        let at = measurement_with_total(900);
        let over = measurement_with_total(901);
        assert!(!settings.should_compact_anchored(&at, 1_000, 1));
        assert!(settings.should_compact_anchored(&over, 1_000, 1));
        // Throttle still applies on the anchored path.
        assert!(!settings.should_compact_anchored(&over, 1_000, 0));
    }

    #[test]
    fn anchored_gate_stops_firing_after_a_compaction_shrinks_the_surface() {
        let settings = anchored_settings();
        let big = "x".repeat(400);
        // The anchored sample prices the surface its request saw
        // (the big user message, before the assistant reply).
        let before = vec![
            user_event(0, &big),
            assistant_event(1),
            synthia_session::usage(950, 10, 960, None, None, None),
        ];
        // The compaction then shadows that very message.
        let after = vec![
            user_event(0, &big),
            assistant_event(1),
            synthia_session::usage(950, 10, 960, None, None, None),
            compaction_event(2),
        ];
        let m_before = meter_over(&before);
        let m_after = meter_over(&after);
        assert!(
            settings.should_compact_anchored(&m_before, 1_000, 1),
            "anchored pressure over threshold fires before compaction"
        );
        assert!(m_before.total_tokens > 900, "{m_before:?}");
        assert!(
            !settings.should_compact_anchored(&m_after, 1_000, 1),
            "shadowed span must lower the projection below threshold"
        );
        assert!(m_after.total_tokens < m_before.total_tokens);
    }

    #[test]
    fn anchored_gate_falls_back_to_heuristic_without_a_sample() {
        let settings = anchored_settings();
        let log = vec![user_event(0, &"x".repeat(400))];
        let m = meter_over(&log);
        assert!(matches!(
            m.baseline,
            synthia_session::MeasurementBaseline::Estimated { .. }
        ));
        // Heuristic price ≈ 105: below a 900 threshold, above a
        // threshold of 0 (window 50 < reserve 100 saturates).
        assert!(!settings.should_compact_anchored(&m, 1_000, 1));
        assert!(settings.should_compact_anchored(&m, 50, 1));
    }

    fn measurement_with_total(total: u64) -> TokenMeasurement {
        TokenMeasurement {
            log_revision: 0,
            baseline: synthia_session::MeasurementBaseline::Estimated {
                tokens: total,
            },
            surface_delta_tokens: 0,
            total_tokens: total,
            surface_tokens: total,
        }
    }
}
