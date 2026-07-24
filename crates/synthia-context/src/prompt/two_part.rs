//! Two-part prompt assembly for KV-cache friendly LLM calls.
//!
//! Splits the system prompt into a stable `header` (byte-identical across
//! all LLM calls in a session) and a variable `body` (turn-specific). The
//! header is hashed with blake3 at build time and emitted as a structured
//! telemetry signal so callers can track `prefix_stability_ratio`.
//!
//! Spec: `openspec/changes/streaming-2part-truncate/specs/two-part-prompt/spec.md`

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use synthia_provider::{Content, Message, Role};
use tracing::warn;

/// Provider-family-specific message shaping. Used by `finalize` to choose
/// the right assembly for the upstream cache (Anthropic / OpenAI / generic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    Anthropic,
    OpenAI,
    Generic,
}

/// Whether the system prompt is delivered as a single message (`Single`,
/// legacy / non-cache-friendly default) or split into two system messages
/// (`TwoPart`, where the first is the stable header and the second is the
/// variable body).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum SystemMessageForm {
    /// One system message: `header + "\n\n" + body`.
    #[default]
    Single,
    /// Two consecutive system messages: header first, body second.
    TwoPart,
}

/// 32-byte blake3 hash of the prompt header.
pub type HeaderHash = [u8; 32];

/// Stable header / variable body container.
#[derive(Debug, Clone)]
pub struct TwoPartPrompt {
    pub header: String,
    pub body: String,
    pub header_hash: HeaderHash,
    pub model_family: ModelFamily,
}

impl TwoPartPrompt {
    /// Build a new two-part prompt. The `header` is hashed with blake3; if
    /// the resulting estimated token count is outside the 600..=3500 target
    /// band a `tracing::warn!` is emitted, but the build always succeeds.
    pub fn build(
        header: impl Into<String>,
        body: impl Into<String>,
        family: ModelFamily,
    ) -> Self {
        let header: String = header.into();
        let body: String = body.into();
        let header_hash = blake3_hash(header.as_bytes());
        let estimated_tokens = estimate_header_tokens(&header);
        if !(600..=3500).contains(&estimated_tokens) {
            warn!(
                target: "synthia.context.two_part",
                estimated_tokens,
                chars = header.chars().count(),
                "TwoPartPrompt header outside the 600-3500 token target band",
            );
        }
        Self {
            header,
            body,
            header_hash,
            model_family: family,
        }
    }

    /// Materialize the prompt into a sequence of messages, given the form
    /// (`Single` / `TwoPart`) and the previous header hash. The returned
    /// `TwoPartDecision` records whether the upstream cache hit is expected
    /// and, on drift, a human-readable reason.
    pub fn finalize(
        &self,
        prev_header_hash: Option<HeaderHash>,
        form: SystemMessageForm,
    ) -> TwoPartDecision {
        let cache_hit_expected = match prev_header_hash {
            None => false,
            Some(prev) if prev == self.header_hash => true,
            Some(_) => false,
        };

        match prev_header_hash {
            Some(prev) if prev != self.header_hash => {
                let summary = diff_summary(
                    &format!("{:02x?}", &prev[..8]),
                    &format!("{:02x?}", &self.header_hash[..8]),
                );
                warn!(
                    target: "synthia.context.two_part",
                    prev_hash = %format!("{:02x?}", &prev[..8]),
                    new_hash = %format!("{:02x?}", &self.header_hash[..8]),
                    diff = %summary,
                    "TwoPartPrompt header_unstable: header changed between turns",
                );
            }
            _ => {}
        }

        let final_messages = match form {
            SystemMessageForm::Single => vec![Message::new(
                Role::System,
                Content::text(format!("{}\n\n{}", self.header, self.body)),
            )],
            SystemMessageForm::TwoPart => vec![
                Message::new(Role::System, Content::text(self.header.clone())),
                // For Anthropic a second consecutive system message is
                // accepted; for OpenAI a developer role would be required,
                // but for parity with the spec we keep both as System and
                // let the provider layer translate. A `User`-fallback would
                // also be valid per the spec — we prefer System-System
                // because it survives tool-use ordering unchanged.
                Message::new(Role::System, Content::text(self.body.clone())),
            ],
        };

        TwoPartDecision {
            final_messages,
            header_hash: self.header_hash,
            cache_hit_expected,
            header_unstable_reason: if !cache_hit_expected
                && prev_header_hash.is_some()
            {
                Some("header bytes changed since previous turn".to_string())
            } else {
                None
            },
            model_family: self.model_family,
            form,
        }
    }
}

/// The output of `TwoPartPrompt::finalize` — the assembled messages plus
/// observability fields (hash, expected cache hit, drift reason).
#[derive(Debug, Clone)]
pub struct TwoPartDecision {
    pub final_messages: Vec<Message>,
    pub header_hash: HeaderHash,
    pub cache_hit_expected: bool,
    pub header_unstable_reason: Option<String>,
    pub model_family: ModelFamily,
    pub form: SystemMessageForm,
}

fn blake3_hash(bytes: &[u8]) -> HeaderHash {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

/// Header length estimator: chars / 3.5 (ceiling). The spec deliberately
/// avoids tiktoken to keep the build graph light.
pub fn estimate_header_tokens(header: &str) -> usize {
    let chars = header.chars().count();
    let raw = chars as f64 / 3.5;
    raw.ceil() as usize
}

fn diff_summary(prev_hex: &str, new_hex: &str) -> String {
    let prev = prev_hex.trim_start_matches('[').trim_end_matches(']');
    let new = new_hex.trim_start_matches('[').trim_end_matches(']');
    // Count leading chars that match in lock-step.
    let common = prev
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    format!("first {common} hex chars match; full hash diverges")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> String {
        "x".repeat(7000) // 2000 tokens via /3.5
    }

    #[test]
    fn build_computes_blake3_hash() {
        let p = TwoPartPrompt::build("abc", "def", ModelFamily::Generic);
        let expected = blake3_hash(b"abc");
        assert_eq!(p.header_hash, expected);
    }

    #[test]
    fn build_under_target_band_succeeds_without_warning() {
        // 7000 chars ≈ 2000 tokens → in the 600..=3500 band.
        let p =
            TwoPartPrompt::build(sample_header(), "body", ModelFamily::Generic);
        let est = estimate_header_tokens(&p.header);
        assert_eq!(est, 2000);
    }

    #[test]
    fn build_over_target_band_still_succeeds() {
        // 14000 chars ≈ 4000 tokens → above band, but build must succeed.
        let big = "x".repeat(14_000);
        let p = TwoPartPrompt::build(big.clone(), "body", ModelFamily::Generic);
        assert_eq!(p.header.len(), big.len());
    }

    #[test]
    fn finalize_first_call_has_no_cache_hit() {
        let p = TwoPartPrompt::build("h", "b", ModelFamily::Generic);
        let d = p.finalize(None, SystemMessageForm::Single);
        assert!(!d.cache_hit_expected);
        assert!(d.header_unstable_reason.is_none());
        assert_eq!(d.final_messages.len(), 1);
    }

    #[test]
    fn finalize_same_header_yields_cache_hit() {
        let p1 = TwoPartPrompt::build("h", "b1", ModelFamily::Generic);
        let d1 = p1.finalize(None, SystemMessageForm::TwoPart);
        let p2 = TwoPartPrompt::build("h", "b2", ModelFamily::Generic);
        let d2 = p2.finalize(Some(d1.header_hash), SystemMessageForm::TwoPart);
        assert!(d2.cache_hit_expected);
        assert!(d2.header_unstable_reason.is_none());
        assert_eq!(d2.final_messages.len(), 2);
    }

    #[test]
    fn finalize_header_drift_signals_unstable() {
        let p1 = TwoPartPrompt::build("h1", "b1", ModelFamily::Generic);
        let d1 = p1.finalize(None, SystemMessageForm::TwoPart);
        let p2 = TwoPartPrompt::build("h2", "b2", ModelFamily::Generic);
        let d2 = p2.finalize(Some(d1.header_hash), SystemMessageForm::TwoPart);
        assert!(!d2.cache_hit_expected);
        assert!(d2.header_unstable_reason.is_some());
    }

    #[test]
    fn finalize_single_form_emits_one_message() {
        let p = TwoPartPrompt::build("h", "b", ModelFamily::Generic);
        let d = p.finalize(None, SystemMessageForm::Single);
        assert_eq!(d.final_messages.len(), 1);
        assert_eq!(d.final_messages[0].role, Role::System);
        let text = d.final_messages[0].content.extract_text().unwrap();
        assert!(text.contains("h"));
        assert!(text.contains("b"));
    }

    #[test]
    fn finalize_two_part_form_emits_two_messages() {
        let p = TwoPartPrompt::build("h", "b", ModelFamily::Generic);
        let d = p.finalize(None, SystemMessageForm::TwoPart);
        assert_eq!(d.final_messages.len(), 2);
        assert_eq!(d.final_messages[0].role, Role::System);
        assert_eq!(d.final_messages[1].role, Role::System);
        let head_text = d.final_messages[0].content.extract_text().unwrap();
        let body_text = d.final_messages[1].content.extract_text().unwrap();
        assert_eq!(head_text, "h");
        assert_eq!(body_text, "b");
    }

    #[test]
    fn model_family_is_carried_through() {
        let p = TwoPartPrompt::build("h", "b", ModelFamily::Anthropic);
        let d = p.finalize(None, SystemMessageForm::TwoPart);
        assert_eq!(d.model_family, ModelFamily::Anthropic);
    }

    #[test]
    fn estimate_header_tokens_uses_chars_over_3_5() {
        // 7 chars → 7/3.5 = 2.0 → ceil = 2
        assert_eq!(estimate_header_tokens("1234567"), 2);
        // 0 chars → 0
        assert_eq!(estimate_header_tokens(""), 0);
    }
}
