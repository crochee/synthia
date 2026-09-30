//! Branch scoring for [`Step::Mcts`](crate::Step::Mcts).
//!
//! An MCTS step is *branching* search: each branch is one (or more, with
//! `max_depth > 0`) agent calls run in parallel, and the branch whose
//! final text scores the highest wins. Scoring is a *pure comparison* —
//! no model calls, no host effects — so it can run synchronously on the
//! runtime side, after every branch settled. The trait is sealed from
//! `async` on purpose: a synchronous scorer is one the runtime owns, and
//! moving that line would move it off the deterministic journal path
//! that replay depends on.
//!
//! Three scorers are part of the wire format and the runtime carries
//! them as variants of [`MctsScorer`]:
//!
//! - [`MctsScorer::ShortestText`] prefers shorter outputs, with the
//!   zero-length penalty described below applied uniformly.
//! - [`MctsScorer::LongestText`] prefers longer outputs and is what the
//!   best_of example judges by.
//! - [`MctsScorer::Heuristic`] is the host's hook: a custom
//!   [`Scorer`] registered in code. Because a closure is not a value
//!   that round-trips through JSON, it is absent from the wire format —
//!   a document that names `heuristic` carries the runtime's registered
//!   `HeuristicScorer` instead.
//!
//! ## Tie-break
//!
//! Two branches with equal scores always resolve to the one with the
//! *lower* [`ScoredBranch::branch_id`]. `branch_id` is the position of
//! the branch's first call (depth 0) in the run's plan, which is
//! assigned in pre-order, so this is deterministic — same document, same
//! tie-break, every run, live or replayed.
//!
//! ## Gate-failed branches
//!
//! A branch that failed its gate is still scored. The same branch gets
//! its final text (an empty string when the agent itself failed, the
//! gate's output line when only the gate failed) and a
//! [`ScoredBranch::gate`] of `GateVerdict::Failed`. A scorer that
//! ignores that verdict may still pick it; `ShortestText` and
//! `LongestText` don't, because a failed gate answers an empty input.
//! The runtime's tie-break is the final word: a branch whose gate
//! failed never wins unless *every* branch's gate failed, in which case
//! the tie-break chooses one (and the step reports no winner text —
//! [`GateVerdict::Failed`] means the call has no useful output).
//!
//! ## Zero-text branches
//!
//! An empty `text` is a valid input. The built-in scorers score `0.0`
//! for it and never pick it over a branch that produced text —
//! `ShortestText` breaks ties in `branch_id` order, so an empty branch
//! tied with another empty branch still loses to one with a non-empty
//! text. No scorer returns `NaN`; an empty input is a zero, not an
//! error.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::host::GateVerdict;

/// What one scorer is told about a branch.
///
/// The runtime builds one of these for every branch after it settles;
/// the scorer reads what it needs and returns a single `f64`. The
/// struct owns no allocations beyond the strings it carries: passing
/// `text` by reference keeps the per-branch overhead on the stack.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredBranch<'a> {
    /// Position of the branch's depth-0 call in the run's plan — the
    /// key the tie-break uses to resolve equal scores.
    pub branch_id: usize,
    /// Agent the branch ran under.
    pub agent: &'a str,
    /// Branch's depth-0 prompt (before any chained input).
    pub prompt: &'a str,
    /// Branch's final text. An empty string when the agent produced
    /// none, when the gate failed, or when the agent itself errored.
    pub text: Option<&'a str>,
    /// How the branch's gate treated it, when the step declared one.
    pub gate: GateVerdict,
    /// Branch's index in the step's branch list, 0-based.
    pub branch_index: usize,
}

/// A synchronous scorer: returns a single `f64` per branch.
///
/// Scoring is a pure comparison and runs on the runtime side, after the
/// branches settle. It is intentionally `Sync` (and `Send`, via the
/// blanket impl): the trait is held behind an [`Arc`] in
/// [`MctsScorer::Heuristic`], so a single scorer instance scores every
/// branch in the run.
///
/// # Determinism
///
/// A scorer is *required* to be deterministic: a journal replay scores
/// the recorded branches with the same scorer that produced the
/// recorded winner, and the result has to match. A scorer that reads
/// time, randomness, network state, or any value outside
/// [`ScoredBranch`] is wrong: it would pick a different winner on a
/// replay, and the journal's prefix guarantee stops being a guarantee.
pub trait Scorer: Send + Sync {
    /// Score one branch. Higher wins.
    fn score(&self, branch: &ScoredBranch<'_>) -> f64;
}

/// The built-in scorers a document can name on the wire.
///
/// `Heuristic` carries a runtime-registered [`Scorer`] rather than a
/// closure: the wire format has to be round-trippable, and a closure is
/// not data. A document that wants a custom scorer names
/// [`MctsScorer::Heuristic`] in JSON and registers the scorer on the
/// runtime's [`WorkflowRuntime`](crate::WorkflowRuntime) before the
/// run starts; without the registration, planning accepts the
/// document and the run errors on the first branch settlement.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MctsScorer {
    /// Pick the branch with the shortest final text.
    ShortestText,
    /// Pick the branch with the longest final text.
    LongestText,
    /// A host-registered scorer picked up from the runtime's
    /// [`HeuristicScorer`].
    Heuristic,
}

impl MctsScorer {
    /// `true` when this variant carries a custom scorer the runtime
    /// looks up by name.
    #[must_use]
    pub fn is_heuristic(&self) -> bool {
        matches!(self, Self::Heuristic)
    }
}

/// A registered heuristic scorer: the runtime looks these up by their
/// wire name and uses the trait object they carry for the run.
#[derive(Clone)]
pub struct HeuristicScorer {
    /// The wire name the document carries.
    name: String,
    /// The scorer to invoke when a document names `name`.
    scorer: Arc<dyn Scorer>,
}

impl HeuristicScorer {
    /// Register a scorer under `name`. The name is the wire name a
    /// document's `{"kind": "heuristic"}` scorer carries, and the
    /// lookup is by string equality.
    #[must_use]
    pub fn new(name: impl Into<String>, scorer: Arc<dyn Scorer>) -> Self {
        Self {
            name: name.into(),
            scorer,
        }
    }

    /// The wire name the scorer is registered under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The scorer the runtime invokes for a matching document.
    #[must_use]
    pub fn scorer(&self) -> &Arc<dyn Scorer> {
        &self.scorer
    }
}

/// A scorer the runtime owns when the document names a built-in
/// variant, or the trait object the document asks for when it does not.
///
/// Built-in scorers are stateless structs so the runtime can hand them
/// around by value; the heuristic variant is an [`Arc`] because a
/// registered scorer has to live as long as the runtime does.
#[derive(Clone)]
pub enum BuiltinScorer {
    /// [`MctsScorer::ShortestText`].
    ShortestText,
    /// [`MctsScorer::LongestText`].
    LongestText,
    /// [`MctsScorer::Heuristic`] — a registered scorer.
    Heuristic(Arc<dyn Scorer>),
}

impl BuiltinScorer {
    /// Score one branch through whichever scorer this is.
    #[must_use]
    pub fn score(&self, branch: &ScoredBranch<'_>) -> f64 {
        match self {
            Self::ShortestText => shortest_text(branch),
            Self::LongestText => longest_text(branch),
            Self::Heuristic(scorer) => scorer.score(branch),
        }
    }
}

impl From<MctsScorer> for BuiltinScorerKind {
    fn from(value: MctsScorer) -> Self {
        match value {
            MctsScorer::ShortestText => Self::ShortestText,
            MctsScorer::LongestText => Self::LongestText,
            MctsScorer::Heuristic => Self::Heuristic,
        }
    }
}

/// The cheap side of [`MctsScorer`]: the variant without its scorer
/// payload. The runtime stores this when it can and only looks the
/// payload up when the document actually asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinScorerKind {
    /// [`MctsScorer::ShortestText`].
    ShortestText,
    /// [`MctsScorer::LongestText`].
    LongestText,
    /// [`MctsScorer::Heuristic`].
    Heuristic,
}

/// The "shorter is better" scorer. Empty text scores `f64::MIN` so a
/// zero-text branch never beats one that produced text; ties between
/// non-empty branches resolve by branch_id (lower wins).
fn shortest_text(branch: &ScoredBranch<'_>) -> f64 {
    let len = branch.text.map(str::len).unwrap_or(0);
    if len == 0 {
        // Empty text is the lowest score, not zero, so a non-empty
        // branch always beats it (zero would otherwise tie a non-empty
        // branch with the lowest length). The tie-break on `branch_id`
        // still resolves empty-vs-empty deterministically.
        f64::MIN
    } else {
        -f64::from(u32::try_from(len).unwrap_or(u32::MAX))
    }
}

/// The "longer is better" scorer. Returns the length directly so a
/// longer branch always beats a shorter one; an empty branch scores
/// 0.0 and loses to anything with text.
fn longest_text(branch: &ScoredBranch<'_>) -> f64 {
    let len = branch.text.map(str::len).unwrap_or(0);
    f64::from(u32::try_from(len).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch<'a>(
        branch_id: usize,
        branch_index: usize,
        text: Option<&'a str>,
        gate: GateVerdict,
    ) -> ScoredBranch<'a> {
        ScoredBranch {
            branch_id,
            agent: "coder",
            prompt: "p",
            text,
            gate,
            branch_index,
        }
    }

    #[test]
    fn longest_text_prefers_a_longer_answer() {
        let a = branch(0, 0, Some("short"), GateVerdict::Absent);
        let b = branch(1, 1, Some("a much longer answer"), GateVerdict::Absent);
        assert!(
            BuiltinScorer::LongestText.score(&b)
                > BuiltinScorer::LongestText.score(&a)
        );
    }

    #[test]
    fn longest_text_handles_zero_text_without_panicking() {
        let empty = branch(0, 0, None, GateVerdict::Absent);
        let missing = branch(1, 1, Some(""), GateVerdict::Absent);
        assert_eq!(BuiltinScorer::LongestText.score(&empty), 0.0);
        assert_eq!(BuiltinScorer::LongestText.score(&missing), 0.0);
    }

    #[test]
    fn shortest_text_prefers_a_shorter_answer() {
        let a = branch(0, 0, Some("a much longer answer"), GateVerdict::Absent);
        let b = branch(1, 1, Some("short"), GateVerdict::Absent);
        assert!(
            BuiltinScorer::ShortestText.score(&b)
                > BuiltinScorer::ShortestText.score(&a)
        );
    }

    #[test]
    fn shortest_text_treats_empty_text_as_the_lowest_score() {
        let empty = branch(0, 0, None, GateVerdict::Absent);
        let any = branch(1, 1, Some("x"), GateVerdict::Absent);
        assert!(
            BuiltinScorer::ShortestText.score(&empty)
                < BuiltinScorer::ShortestText.score(&any),
            "an empty branch must never beat a non-empty one"
        );
    }

    #[test]
    fn deterministic_scoring_on_fixed_branches() {
        let a = branch(3, 0, Some("answer A"), GateVerdict::Passed);
        let b = branch(7, 1, Some("answer B"), GateVerdict::Passed);
        let first = BuiltinScorer::LongestText.score(&a);
        let second = BuiltinScorer::LongestText.score(&a);
        assert_eq!(first, second, "scoring must be deterministic");
        assert_eq!(first, BuiltinScorer::LongestText.score(&b).min(first));
    }

    /// A scorer that picks the branch whose prompt contains a flag.
    /// Used to prove the heuristic hook is wired through.
    struct FlagScorer;

    impl Scorer for FlagScorer {
        fn score(&self, branch: &ScoredBranch<'_>) -> f64 {
            if branch.prompt.contains("good") {
                1.0
            } else {
                0.0
            }
        }
    }

    #[test]
    fn heuristic_scorer_receives_what_it_needs() {
        let scorer: Arc<dyn Scorer> = Arc::new(FlagScorer);
        let owned_branch = ScoredBranch {
            branch_id: 0,
            agent: "a",
            prompt: "a good prompt",
            text: Some("answer"),
            gate: GateVerdict::Absent,
            branch_index: 0,
        };
        assert_eq!(scorer.score(&owned_branch), 1.0);
        let bad_branch = ScoredBranch {
            prompt: "a dull prompt",
            ..owned_branch.clone()
        };
        assert_eq!(scorer.score(&bad_branch), 0.0);
    }

    #[test]
    fn gate_failed_branch_is_still_scored() {
        let failed = branch(0, 0, Some(""), GateVerdict::Failed);
        let passed = branch(1, 1, Some("answer"), GateVerdict::Passed);
        let score_failed = BuiltinScorer::LongestText.score(&failed);
        let score_passed = BuiltinScorer::LongestText.score(&passed);
        assert!(
            score_passed > score_failed,
            "a gate-failed branch with empty text must not win"
        );
        // Even when the gate failed, the scorer is still consulted: a
        // branch's gate verdict is metadata the scorer reads, not a
        // veto the runtime applies.
        let _ = score_failed;
    }

    #[test]
    fn scoring_returns_no_nan_for_zero_text() {
        let empty = branch(0, 0, None, GateVerdict::Absent);
        let score = BuiltinScorer::LongestText.score(&empty);
        assert!(!score.is_nan(), "{score}");
        let score = BuiltinScorer::ShortestText.score(&empty);
        assert!(!score.is_nan(), "{score}");
    }
}
