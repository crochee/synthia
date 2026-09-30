//! The workflow document: what to run, as data.
//!
//! A [`WorkflowSpec`] is ordered [`Step`]s. Four shapes cover the
//! orchestration the reference script VM could express for a whole run:
//!
//! - [`Step::Agent`] — one agent, optionally gated.
//! - [`Step::FanOut`] — N prompts under one agent, run concurrently.
//! - [`Step::BestOf`] — N prompts under one agent, run concurrently;
//!   the host's selection chooses the winner, by default the first
//!   success.
//! - [`Step::Pipeline`] — stages in order, each stage's text output
//!   feeding the next stage's prompt.
//!
//! [`Phase`]s are optional labels over step ids, for reporting only:
//! they never change execution order, and a document without them runs
//! exactly the same.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::WorkflowError;

/// A whole workflow, as a document.
///
/// Steps run top to bottom; a step is done when every call it planned
/// has settled. Serialize it to JSON to store or hand it around — the
/// runtime reads the same shape back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    /// Workflow identifier: used for the run id prefix and in logs.
    pub id: String,
    /// Ordered steps, executed top to bottom.
    pub steps: Vec<Step>,
    /// Optional phase labels over step ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<Phase>,
}

/// One ordered unit of work.
///
/// The `kind` field on the wire is `agent`, `fan_out`, `best_of`,
/// `pipeline` or `mcts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    /// One agent call.
    Agent(AgentStep),
    /// One agent, many prompts, run concurrently.
    FanOut(FanOutStep),
    /// One agent, many prompts, run concurrently: the first candidate to
    /// succeed wins and supplies the step's output.
    BestOf(BestOfStep),
    /// Ordered stages, each fed the previous stage's text output.
    Pipeline(PipelineStep),
    /// Branching search: many parallel branches, each expanded across
    /// depths, scored by [`MctsScorer`](crate::scorer::MctsScorer); the
    /// branch with the highest score wins.
    Mcts(MctsStep),
}

/// A single agent call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStep {
    /// Step id; unique across the document, and the handle live control
    /// uses to skip or retry this step.
    pub id: String,
    /// Agent to spawn.
    pub agent: String,
    /// Prompt the agent starts from.
    pub prompt: String,
    /// Command that must pass for the call to count as a success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateRef>,
    /// Ask the host to run the agent in an isolated worktree.
    #[serde(default)]
    pub isolation: bool,
}

/// One agent, one prompt per item, run concurrently.
///
/// The contract's shape listed no agent for this variant; one is
/// required, because items are prompts and something has to run them.
/// Heterogeneous work is a [`PipelineStep`] of [`AgentStep`]s instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanOutStep {
    /// Step id; unique across the document.
    pub id: String,
    /// Agent every item runs under.
    pub agent: String,
    /// One agent call per prompt, each its own journal position.
    pub items: Vec<String>,
    /// Cap on this fan-out's own concurrency, on top of the run's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
    /// Cap on `items` for this step; `None` means
    /// [`WorkflowCaps::max_items`](crate::WorkflowCaps::max_items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
}

/// One agent, one prompt per candidate, run concurrently: the candidate
/// its step's selection chooses is the step's output.
///
/// Every candidate spawns — one call each, each its own journal
/// position, so the selector stays inside the plan's prefix replay —
/// and when the step declares a gate, every candidate's gate runs too.
/// The choice itself is the host's
/// ([`WorkflowHost::select_candidate`](crate::WorkflowHost::select_candidate)):
/// the default rule — and the fallback whenever a host declines or a
/// replayed step carries no recorded decision — is the first candidate,
/// in item order, that settled as a success: with a gate that is the
/// first candidate that passed it, without one the first candidate that
/// spawned successfully. Whatever the rule, the decision is journaled,
/// so a replayed step reproduces it instead of asking the host again.
/// Every other candidate that ran is reported as a loser and no
/// candidate's failure can fail the step; a step whose candidates all
/// failed reports no output and keeps every candidate's failure text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BestOfStep {
    /// Step id; unique across the document, and the handle live control
    /// uses to skip or retry this step.
    pub id: String,
    /// Agent every candidate runs under.
    pub agent: String,
    /// One candidate per prompt, each its own journal position.
    pub items: Vec<String>,
    /// Command every candidate has to pass to count as a success. When
    /// it is absent, any candidate that succeeds may win.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateRef>,
    /// Cap on `items` for this step; `None` means
    /// [`WorkflowCaps::max_items`](crate::WorkflowCaps::max_items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    /// Cap on this step's own concurrency, on top of the run's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
}

/// Stages in order, stage N's text output seeding stage N+1.
///
/// The chained input is appended to a stage's own prompt, after a
/// `--- output from the previous stage ---` marker; a stage that
/// produced no text leaves the next stage unchained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStep {
    /// Step id; unique across the document.
    pub id: String,
    /// Stages, executed in order.
    pub stages: Vec<Step>,
}
/// Branching search: many parallel branches, each expanded across
/// depths, scored by [`MctsScorer`](crate::scorer::MctsScorer); the
/// branch with the highest score wins.
///
/// An MCTS step plans `branches * (max_depth + 1)` calls: every branch
/// runs all `max_depth + 1` depths, each depth's prompt chained off
/// the previous depth's text output. The runtime runs them concurrently
/// under the existing semaphore bound; the step's `concurrency` field
/// caps the in-flight count further when it sets one. The score is the
/// branch's depth-`max_depth` text — the only call whose text the
/// scorer sees.
///
/// Every branch's gate is run if the step declared one. A branch whose
/// gate failed is still scored: its text (empty when the agent itself
/// failed) is fed to the scorer like every other branch, with the gate
/// verdict attached. The branch with the highest score wins; on equal
/// scores, the branch with the lower `branch_id` (its depth-0 call's
/// position in the plan) wins, deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctsStep {
    /// Step id; unique across the document.
    pub id: String,
    /// Agent every branch runs under.
    pub agent: String,
    /// The depth-0 prompt every branch starts from.
    pub prompt: String,
    /// Number of parallel branches. Must be greater than zero.
    pub branches: u32,
    /// Number of *additional* depths after depth 0. `0` is a single
    /// round of calls (one per branch, scored by their text); `2`
    /// plans three calls per branch. Must be less than `u32::MAX`.
    pub max_depth: u32,
    /// Command every branch has to pass to count as a success. When
    /// it is absent, any branch that succeeded may win.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateRef>,
    /// Cap on this step's own concurrency, on top of the run's. Bounds
    /// the in-flight count of all branches at every depth combined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
    /// Scorer the runtime ranks branches by. The wire form is one of
    /// `shortest_text`, `longest_text` or `heuristic` (see
    /// [`MctsScorer`](crate::scorer::MctsScorer)).
    pub scorer: crate::scorer::MctsScorer,
}

/// A shell gate an agent call has to pass to count as a success.
///
/// The runtime never runs the command itself: it hands the reference to
/// the host, which owns the process, the timeout and the working
/// directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRef {
    /// Command to run.
    pub command: String,
    /// Optional display label; the command is used when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A reporting group of step ids.
///
/// Sugar: every step the phase names gets the phase's label in its
/// [`CallRun`](crate::CallRun), but the document's step order is still
/// the execution order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase {
    /// Phase id; unique across the document.
    pub id: String,
    /// Human-readable label, copied onto the calls it groups.
    pub label: String,
    /// Step ids in this phase. A step may belong to at most one phase.
    pub steps: Vec<String>,
}

impl WorkflowSpec {
    /// Decode a workflow document from JSON.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::Document`] when the text is not a workflow
    /// document.
    pub fn from_json(json: &str) -> Result<Self, WorkflowError> {
        serde_json::from_str(json).map_err(|error| WorkflowError::Document {
            message: error.to_string(),
        })
    }

    /// Read a workflow document from a JSON file.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::Document`] when the file cannot be read or
    /// decoded.
    pub fn load(path: &Path) -> Result<Self, WorkflowError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            WorkflowError::Document {
                message: format!("{}: {error}", path.display()),
            }
        })?;
        serde_json::from_str(&text).map_err(|error| WorkflowError::Document {
            message: format!("{}: {error}", path.display()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_round_trips_through_its_wire_form() {
        let json = r#"{
          "id": "implement",
          "steps": [{
            "kind": "best_of",
            "id": "pick",
            "agent": "coder",
            "items": ["one", "two"],
            "gate": { "command": "cargo test" },
            "max_items": 4,
            "concurrency": 2
          }]
        }"#;
        let document = WorkflowSpec::from_json(json).unwrap();
        assert_eq!(
            document.steps,
            vec![Step::BestOf(BestOfStep {
                id: "pick".to_owned(),
                agent: "coder".to_owned(),
                items: vec!["one".to_owned(), "two".to_owned()],
                gate: Some(GateRef {
                    command: "cargo test".to_owned(),
                    label: None,
                }),
                max_items: Some(4),
                concurrency: Some(2),
            })]
        );

        let encoded = serde_json::to_string(&document).unwrap();
        assert!(
            encoded.contains(r#""kind":"best_of""#),
            "the wire name is part of the document format: {encoded}"
        );
        assert_eq!(WorkflowSpec::from_json(&encoded).unwrap(), document);
    }

    #[test]
    fn a_selection_needs_only_its_id_agent_and_items() {
        let document = WorkflowSpec::from_json(
            r#"{
              "id": "wf",
              "steps": [{
                "kind": "best_of",
                "id": "pick",
                "agent": "coder",
                "items": ["one"]
              }]
            }"#,
        )
        .unwrap();
        let Step::BestOf(step) = &document.steps[0] else {
            panic!("`best_of` should decode as a selection");
        };
        assert!(step.gate.is_none());
        assert!(step.max_items.is_none());
        assert!(step.concurrency.is_none());

        // Absent options stay off the wire, so a document that does not
        // set them does not carry them.
        let encoded = serde_json::to_string(&document).unwrap();
        assert!(!encoded.contains("gate"), "{encoded}");
        assert!(!encoded.contains("concurrency"), "{encoded}");
    }
}
