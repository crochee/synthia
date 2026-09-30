//! Parse a `0.0..=1.0` score from a judge-model reply.
//!
//! Canonical location for the LLM-as-judge reply parser. Both
//! [`synthia_eval::LlmJudgeMetric`](https://docs.rs/synthia_eval)
//! and [`synthia_harness::LlmJudgeScorer`](https://docs.rs/synthia_harness)
//! read judge replies through this function, so a change to the
//! shape of a score line (`Score: 0.85`, case-insensitive, with
//! or without a space after the colon, clamped to the unit
//! interval) cannot drift between the offline grading path and
//! the in-loop scorer. Previously each crate had its own copy;
//! R76 collapsed them.
//!
//! # Contract
//!
//! The function recognises:
//!
//! - `Score: 0.85` — case-insensitive prefix on any line.
//! - `Score:0.85` — the same prefix with no space after the
//!   colon (some models elide it; both must score).
//! - a standalone number line (`0.85`).
//!
//! The parsed value is clamped to `0.0..=1.0` so an out-of-range
//! judge reply (`Score: 7`, `Score: -0.5`) cannot break the
//! score domain. Anything else scores `0.0`.

/// Parse a `0.0..=1.0` score from a judge reply.
///
/// See the [module-level docs](self) for the full contract. The
/// implementation walks the reply line by line and returns the
/// first parseable score; the loop is bounded by `lines()` so a
/// pathological multi-MB reply cannot loop forever.
#[must_use]
pub fn parse_judge_score(reply: &str) -> f64 {
    for line in reply.lines() {
        let trimmed = line.trim();
        if let Some(rest) = stripped_prefix_ci(trimmed, "Score:")
            && let Some(value) = parse_first_f64(rest.trim())
        {
            return value.clamp(0.0, 1.0);
        }
    }
    // Fallback: any line that *is* a single number (some judges
    // reply with just the score on its own line).
    for line in reply.lines() {
        let trimmed = line.trim();
        if let Some(value) = parse_first_f64(trimmed) {
            return value.clamp(0.0, 1.0);
        }
    }
    0.0
}

fn stripped_prefix_ci<'a>(haystack: &'a str, prefix: &str) -> Option<&'a str> {
    if haystack.len() < prefix.len() {
        return None;
    }
    let (head, tail) = haystack.split_at(prefix.len());
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

fn parse_first_f64(text: &str) -> Option<f64> {
    // Find the first run of digits / `+-.` and try `f64::from_str`.
    let bytes = text.as_bytes();
    let mut start = None;
    for (index, byte) in bytes.iter().enumerate() {
        let is_digit = byte.is_ascii_digit();
        let is_sign = matches!(byte, b'+' | b'-' | b'.');
        if is_digit || is_sign {
            if start.is_none() {
                start = Some(index);
            }
        } else if start.is_some() {
            return text[start?..index].parse::<f64>().ok();
        }
    }
    start.and_then(|s| text[s..].parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_score_prefix_with_space() {
        assert_eq!(parse_judge_score("Score: 0.85"), 0.85);
        assert_eq!(parse_judge_score("score: 0.5"), 0.5);
        assert_eq!(parse_judge_score("SCORE: 1.0"), 1.0);
    }

    #[test]
    fn parses_score_prefix_without_space() {
        // Some models elide the space after the colon. Both shapes
        // must score, otherwise the in-loop scorer and the eval
        // grader disagree on what a score line looks like.
        assert_eq!(parse_judge_score("Score:0.7"), 0.7);
        assert_eq!(parse_judge_score("verdict:\nScore:0.7\nthanks"), 0.7);
    }

    #[test]
    fn parses_standalone_number() {
        assert_eq!(parse_judge_score("\n\n0.42\n"), 0.42);
        assert_eq!(parse_judge_score("0.5"), 0.5);
        assert_eq!(parse_judge_score("  1.0  "), 1.0);
    }

    #[test]
    fn finds_score_on_later_line() {
        assert_eq!(
            parse_judge_score("The candidate is mostly correct.\nScore: 0.7"),
            0.7
        );
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(parse_judge_score("Score: 7"), 1.0);
        assert_eq!(parse_judge_score("Score: -3"), 0.0);
        assert_eq!(parse_judge_score("Score: -0.5"), 0.0);
    }

    #[test]
    fn unknown_reply_scores_zero() {
        assert_eq!(parse_judge_score("nothing to see here"), 0.0);
        assert_eq!(parse_judge_score(""), 0.0);
    }

    #[test]
    fn handles_lowercase_and_mixed_case() {
        // The contract is case-insensitive; the original
        // `synthia_eval::parse_score` did not honour it, so a judge
        // that said "score: 0.5" used to work in `LlmJudgeScorer`
        // and silently score 0.0 in `LlmJudgeMetric`. The shared
        // parser collapses both.
        assert_eq!(parse_judge_score("score: 0.5"), 0.5);
        assert_eq!(parse_judge_score("Score : 0.3"), 0.3);
    }
}
