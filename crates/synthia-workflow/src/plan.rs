//! The executable plan: caps, positions and identity keys.
//!
//! [`WorkflowSpec::plan`] is the one place a document is checked and
//! flattened. It answers three questions before anything spawns:
//!
//! - **Is the document runnable?** Empty ids, duplicate step ids, empty
//!   prompts and zero bounds are rejected as
//!   [`WorkflowError::InvalidSpec`].
//! - **Does it fit the caps?** The agent, item and nesting caps are
//!   enforced here, so an oversized workflow costs a typed error rather
//!   than a thousand spawned children.
//! - **What is every call's identity?** Positions are assigned by a
//!   deterministic pre-order walk — pipeline stages in order, fan-out
//!   items and `best_of` candidates in order — and each call gets the
//!   [`journal_key`] of its agent, prompt, gate and position. Because
//!   the walk is the document's order and not call arrival, a resume
//!   matches the previous run exactly.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    error::WorkflowError,
    journal::{JournalEntry, JournalKeyInput, journal_key, replay_lookup},
    spec::{GateRef, Phase, Step, WorkflowSpec},
};

/// Agents one run may schedule, unless configured otherwise.
pub const DEFAULT_MAX_AGENTS: usize = 1000;

/// Items one fan-out (or stages one pipeline) may hold.
pub const DEFAULT_MAX_ITEMS: usize = 4096;

/// Pipelines one run may nest.
pub const DEFAULT_MAX_NESTED: usize = 256;

/// Bounds a document is held to before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowCaps {
    /// Total agent calls one run may plan.
    pub max_agents: usize,
    /// Items per fan-out and stages per pipeline.
    pub max_items: usize,
    /// Pipeline nesting depth.
    pub max_nested: usize,
}

impl Default for WorkflowCaps {
    fn default() -> Self {
        Self {
            max_agents: DEFAULT_MAX_AGENTS,
            max_items: DEFAULT_MAX_ITEMS,
            max_nested: DEFAULT_MAX_NESTED,
        }
    }
}

/// One agent call, resolved.
///
/// `position` doubles as this call's index into
/// [`WorkflowPlan::calls`] and as its journal position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCall {
    /// Position in the run's plan, assigned in pre-order.
    pub position: usize,
    /// The step that planned this call.
    pub step_id: String,
    /// The phase label covering this step, when one does.
    pub phase: Option<String>,
    /// Agent to spawn.
    pub agent: String,
    /// The document's prompt, before pipeline chaining.
    pub prompt: String,
    /// Gate declared for this step.
    pub gate: Option<GateRef>,
    /// Whether the call asked for an isolated worktree.
    pub isolation: bool,
    /// Number of enclosing pipelines.
    pub depth: usize,
    /// [`journal_key`] of this call's identity fields.
    pub identity: String,
}

/// A planned step, in the shape the runtime executes.
///
/// Agent calls are addressed by index into [`WorkflowPlan::calls`] so
/// the plan stores each call exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedStep {
    /// One agent call.
    Agent(usize),
    /// Concurrent calls, in item order.
    FanOut {
        /// The step's id.
        id: String,
        /// This fan-out's own admission bound, when it set one.
        concurrency: Option<usize>,
        /// Indices of the item calls.
        items: Vec<usize>,
    },
    /// Concurrent candidates, in item order: the step's selection —
    /// the host's, by default the first success — picks the winner.
    BestOf {
        /// The step's id.
        id: String,
        /// This step's own admission bound, when it set one.
        concurrency: Option<usize>,
        /// Indices of the candidate calls.
        items: Vec<usize>,
    },
    /// Sequential stages, chaining text.
    Pipeline {
        /// The step's id.
        id: String,
        /// The stages, in order.
        stages: Vec<PlannedStep>,
    },

    /// Branching search: every branch is a row of calls (depth 0 ..
    /// max_depth), all chained, all scored by the same [`MctsScorer`](crate::scorer::MctsScorer).
    Mcts {
        /// The step's id.
        id: String,
        /// The step's own admission bound, when it set one.
        concurrency: Option<usize>,
        /// Indices of the depth-0 calls, in branch order. The depth
        /// rows that follow them are addressed by
        /// `branches[branch_index] + 1 + depth_index` — the depth rows
        /// are contiguous and grouped per branch, because the plan
        /// walks depths in lockstep.
        branches: Vec<usize>,
        /// Depth rows: `depths[d][b]` is the position of branch `b`'s
        /// depth-`d+1` call. `depths[d].len() == branches.len()`.
        depths: Vec<Vec<usize>>,
        /// Scorer the runtime uses to pick the winner.
        scorer: crate::scorer::MctsScorer,
    },
}

/// A document that passed planning, ready to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowPlan {
    id: String,
    calls: Vec<PlannedCall>,
    steps: Vec<PlannedStep>,
}

impl WorkflowPlan {
    /// The workflow's id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The plan's steps, in execution order.
    pub fn steps(&self) -> &[PlannedStep] {
        &self.steps
    }

    /// Every call, in position order: `calls()[n].position == n`.
    pub fn calls(&self) -> &[PlannedCall] {
        &self.calls
    }

    /// How many agent calls the run will make.
    pub fn call_count(&self) -> usize {
        self.calls.len()
    }

    /// The longest leading run of positions that `entries` can answer
    /// for, given the plan's identities.
    ///
    /// The walk stops at the first position that has no entry, whose key
    /// no longer matches, or whose call failed — that is, at the first
    /// call this run has to make for real. Everything from there on runs
    /// live, however well it matches later.
    ///
    /// The one exception is a [`PlannedStep::BestOf`] candidate: a
    /// candidate that failed is a *decided loss* when the journal
    /// already answers for another candidate of the same step, because
    /// whatever rule picks a winner — the built-in first success or a
    /// host's judgement — it may only pick a candidate that succeeded,
    /// and one of them is already on record. Re-running a decided loss
    /// could not change the step, so reusing it costs nothing and keeps
    /// the step's later candidates — and every position after them —
    /// replayable.
    pub fn replay_prefix(&self, entries: &[JournalEntry]) -> usize {
        let lookup = replay_lookup(entries);
        let decided = self.decided_candidates(&lookup);
        let mut prefix = 0;
        for call in &self.calls {
            let Some(entry) = lookup.get(&call.position) else {
                break;
            };
            if entry.position != call.position || entry.key != call.identity {
                break;
            }
            if !entry.ok && !decided.contains(&call.position) {
                break;
            }
            prefix += 1;
        }
        prefix
    }

    /// The `best_of` candidate positions whose step the journal already
    /// has a winner for: one candidate of the step has an entry that
    /// matches the plan and records a success.
    ///
    /// The walk consults this only for a candidate whose entry failed,
    /// where membership means the failure is a decided loss: a winner
    /// is always a candidate that succeeded, so one successful entry
    /// means the step's decision is already made and re-running this
    /// candidate could not change what the step reports — it would only
    /// pay for it twice.
    fn decided_candidates(
        &self,
        lookup: &BTreeMap<usize, &JournalEntry>,
    ) -> BTreeSet<usize> {
        let mut decided = BTreeSet::new();
        for group in self.candidate_groups() {
            let answered = group.iter().any(|position| {
                lookup.get(position).is_some_and(|entry| {
                    entry.ok && entry.key == self.calls[*position].identity
                })
            });
            if answered {
                decided.extend(group.iter().copied());
            }
        }
        decided
    }

    /// Every `best_of` or `mcts` step's call positions, one group per
    /// step, in plan order. Fan-out items are deliberately absent:
    /// nothing about a fan-out's result can be settled by a sibling's.
    ///
    /// For an MCTS step every branch's depth-0 position is in the
    /// group along with every depth row — the step's decision is
    /// "which branch won", and once any depth-`max_depth` call of any
    /// branch settles as a success, re-running any failed call in the
    /// step is a decided loss.
    fn candidate_groups(&self) -> Vec<Vec<usize>> {
        fn collect(step: &PlannedStep, groups: &mut Vec<Vec<usize>>) {
            match step {
                PlannedStep::Agent(_) | PlannedStep::FanOut { .. } => {}
                PlannedStep::Pipeline { stages, .. } => {
                    for stage in stages {
                        collect(stage, groups);
                    }
                }
                PlannedStep::BestOf { items, .. } => groups.push(items.clone()),
                PlannedStep::Mcts {
                    branches, depths, ..
                } => {
                    let mut owned: Vec<usize> = Vec::with_capacity(
                        branches.len()
                            + depths.iter().map(Vec::len).sum::<usize>(),
                    );
                    owned.extend(branches.iter().copied());
                    for row in depths {
                        owned.extend(row.iter().copied());
                    }
                    groups.push(owned);
                }
            }
        }
        let mut groups = Vec::new();
        for step in &self.steps {
            collect(step, &mut groups);
        }
        groups
    }
}

impl WorkflowSpec {
    /// Check the document against `caps` and flatten it into a plan.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::InvalidSpec`] for a document that cannot run, and
    /// [`WorkflowError::CapExceeded`] for one that is too large. Both are
    /// raised before any spawn.
    pub fn plan(
        &self,
        caps: &WorkflowCaps,
    ) -> Result<WorkflowPlan, WorkflowError> {
        require_non_empty(&self.id, "workflow id", "workflow")?;
        if self.steps.is_empty() {
            return Err(invalid("workflow has no steps"));
        }
        let mut planner = Planner {
            caps,
            calls: Vec::new(),
            step_ids: BTreeSet::new(),
        };
        let steps = planner.plan_steps(&self.steps, 0)?;
        planner.label_phases(&self.phases)?;
        Ok(WorkflowPlan {
            id: self.id.clone(),
            calls: planner.calls,
            steps,
        })
    }
}

/// What a step hands to [`Planner::push_call`].
struct CallDraft<'a> {
    step_id: &'a str,
    agent: &'a str,
    prompt: &'a str,
    gate: Option<&'a GateRef>,
    isolation: bool,
    depth: usize,
}

struct Planner<'a> {
    caps: &'a WorkflowCaps,
    calls: Vec<PlannedCall>,
    step_ids: BTreeSet<String>,
}

impl Planner<'_> {
    fn plan_steps(
        &mut self,
        steps: &[Step],
        depth: usize,
    ) -> Result<Vec<PlannedStep>, WorkflowError> {
        let mut planned = Vec::with_capacity(steps.len());
        for step in steps {
            planned.push(self.plan_step(step, depth)?);
        }
        Ok(planned)
    }

    fn plan_step(
        &mut self,
        step: &Step,
        depth: usize,
    ) -> Result<PlannedStep, WorkflowError> {
        match step {
            Step::Agent(agent) => {
                self.claim_id(&agent.id, "agent")?;
                require_non_empty(&agent.agent, "agent name", &agent.id)?;
                require_non_empty(&agent.prompt, "prompt", &agent.id)?;
                if let Some(gate) = &agent.gate {
                    require_non_empty(
                        &gate.command,
                        "gate command",
                        &agent.id,
                    )?;
                }
                let index = self.push_call(CallDraft {
                    step_id: &agent.id,
                    agent: &agent.agent,
                    prompt: &agent.prompt,
                    gate: agent.gate.as_ref(),
                    isolation: agent.isolation,
                    depth,
                })?;
                Ok(PlannedStep::Agent(index))
            }
            Step::FanOut(fan) => {
                self.claim_id(&fan.id, "fan-out")?;
                require_non_empty(&fan.agent, "agent name", &fan.id)?;
                if fan.items.is_empty() {
                    return Err(invalid(format!(
                        "fan-out `{}` has no items",
                        fan.id
                    )));
                }
                if fan.concurrency == Some(0) {
                    return Err(invalid(format!(
                        "fan-out `{}` sets concurrency 0",
                        fan.id
                    )));
                }
                let limit = self.item_limit(fan.max_items);
                if fan.items.len() > limit {
                    return Err(WorkflowError::CapExceeded {
                        cap: "max_items",
                        limit,
                        requested: fan.items.len(),
                    });
                }
                let mut items = Vec::with_capacity(fan.items.len());
                for (index, prompt) in fan.items.iter().enumerate() {
                    require_non_empty(
                        prompt,
                        &format!("item prompt {index}"),
                        &fan.id,
                    )?;
                    items.push(self.push_call(CallDraft {
                        step_id: &fan.id,
                        agent: &fan.agent,
                        prompt,
                        gate: None,
                        isolation: false,
                        depth,
                    })?);
                }
                Ok(PlannedStep::FanOut {
                    id: fan.id.clone(),
                    concurrency: fan.concurrency,
                    items,
                })
            }
            Step::BestOf(best) => {
                self.claim_id(&best.id, "best-of")?;
                require_non_empty(&best.agent, "agent name", &best.id)?;
                if best.items.is_empty() {
                    return Err(invalid(format!(
                        "best-of `{}` has no items",
                        best.id
                    )));
                }
                if best.concurrency == Some(0) {
                    return Err(invalid(format!(
                        "best-of `{}` sets concurrency 0",
                        best.id
                    )));
                }
                if let Some(gate) = &best.gate {
                    require_non_empty(&gate.command, "gate command", &best.id)?;
                }
                let limit = self.item_limit(best.max_items);
                if best.items.len() > limit {
                    return Err(WorkflowError::CapExceeded {
                        cap: "max_items",
                        limit,
                        requested: best.items.len(),
                    });
                }
                let mut items = Vec::with_capacity(best.items.len());
                for (index, prompt) in best.items.iter().enumerate() {
                    require_non_empty(
                        prompt,
                        &format!("item prompt {index}"),
                        &best.id,
                    )?;
                    // The step's gate rides on every candidate: each one
                    // is a call the host runs the gate for, so a
                    // candidate that failed its gate is failed for the
                    // selection too.
                    items.push(self.push_call(CallDraft {
                        step_id: &best.id,
                        agent: &best.agent,
                        prompt,
                        gate: best.gate.as_ref(),
                        isolation: false,
                        depth,
                    })?);
                }
                Ok(PlannedStep::BestOf {
                    id: best.id.clone(),
                    concurrency: best.concurrency,
                    items,
                })
            }
            Step::Pipeline(pipeline) => {
                self.claim_id(&pipeline.id, "pipeline")?;
                if pipeline.stages.is_empty() {
                    return Err(invalid(format!(
                        "pipeline `{}` has no stages",
                        pipeline.id
                    )));
                }
                let limit = self.item_limit(None);
                if pipeline.stages.len() > limit {
                    return Err(WorkflowError::CapExceeded {
                        cap: "max_items",
                        limit,
                        requested: pipeline.stages.len(),
                    });
                }
                let level = depth.saturating_add(1);
                if level > self.caps.max_nested {
                    return Err(WorkflowError::CapExceeded {
                        cap: "max_nested",
                        limit: self.caps.max_nested,
                        requested: level,
                    });
                }
                let stages = self.plan_steps(&pipeline.stages, level)?;
                Ok(PlannedStep::Pipeline {
                    id: pipeline.id.clone(),
                    stages,
                })
            }
            Step::Mcts(mcts) => {
                self.claim_id(&mcts.id, "mcts")?;
                require_non_empty(&mcts.agent, "agent name", &mcts.id)?;
                require_non_empty(&mcts.prompt, "prompt", &mcts.id)?;
                if mcts.branches == 0 {
                    return Err(invalid(format!(
                        "mcts `{}` sets branches 0",
                        mcts.id
                    )));
                }
                if mcts.concurrency == Some(0) {
                    return Err(invalid(format!(
                        "mcts `{}` sets concurrency 0",
                        mcts.id
                    )));
                }
                if let Some(gate) = &mcts.gate {
                    require_non_empty(&gate.command, "gate command", &mcts.id)?;
                }
                // Branch count is bound by max_items: the cap that
                // already governs fan-outs and pipelines. One MCTS
                // step is `branches * (max_depth + 1)` items against
                // the same pool, so a count that fits the cap fits
                // the step — and `max_items` is the only place that
                // ceiling is resolved.
                let total_calls = (mcts.branches as usize).saturating_mul(
                    (mcts.max_depth as usize).saturating_add(1),
                );
                let limit = self.item_limit(None);
                if total_calls > limit {
                    return Err(WorkflowError::CapExceeded {
                        cap: "max_items",
                        limit,
                        requested: total_calls,
                    });
                }
                // Depth 0 first: one call per branch, in branch order,
                // so the plan's pre-order walk lays branches side by
                // side. Depths 1..=max_depth chain off the previous
                // depth's text, so the chained prompt is the *plan*
                // prompt; the runtime derives the chain from
                // positions.
                let mut branches = Vec::with_capacity(mcts.branches as usize);
                for branch_index in 0..mcts.branches as usize {
                    branches.push(self.push_call(CallDraft {
                        step_id: &mcts.id,
                        agent: &mcts.agent,
                        prompt: &mcts.prompt,
                        gate: mcts.gate.as_ref(),
                        isolation: false,
                        depth,
                    })?);
                    let _ = branch_index;
                }
                let mut depths = Vec::with_capacity(mcts.max_depth as usize);
                for _ in 1..=mcts.max_depth {
                    let mut row = Vec::with_capacity(mcts.branches as usize);
                    for _ in 0..mcts.branches {
                        row.push(self.push_call(CallDraft {
                            step_id: &mcts.id,
                            agent: &mcts.agent,
                            prompt: &mcts.prompt,
                            gate: mcts.gate.as_ref(),
                            isolation: false,
                            depth,
                        })?);
                    }
                    depths.push(row);
                }
                Ok(PlannedStep::Mcts {
                    id: mcts.id.clone(),
                    concurrency: mcts.concurrency,
                    branches,
                    depths,
                    scorer: mcts.scorer,
                })
            }
        }
    }

    /// The item bound a step runs under: its own limit, tightened by the
    /// run's cap. Items, stages and candidates all count against the
    /// same cap, so this is the one place that ceiling is resolved.
    fn item_limit(&self, step_limit: Option<usize>) -> usize {
        match step_limit {
            Some(limit) => limit.min(self.caps.max_items),
            None => self.caps.max_items,
        }
    }

    fn push_call(
        &mut self,
        draft: CallDraft<'_>,
    ) -> Result<usize, WorkflowError> {
        let position = self.calls.len();
        if position >= self.caps.max_agents {
            return Err(WorkflowError::CapExceeded {
                cap: "max_agents",
                limit: self.caps.max_agents,
                requested: position.saturating_add(1),
            });
        }
        let identity = journal_key(&JournalKeyInput {
            position,
            agent: draft.agent,
            prompt: draft.prompt,
            gate: draft.gate.map(|gate| gate.command.as_str()),
            isolation: draft.isolation,
        });
        self.calls.push(PlannedCall {
            position,
            step_id: draft.step_id.to_owned(),
            phase: None,
            agent: draft.agent.to_owned(),
            prompt: draft.prompt.to_owned(),
            gate: draft.gate.cloned(),
            isolation: draft.isolation,
            depth: draft.depth,
            identity,
        });
        Ok(position)
    }

    fn claim_id(&mut self, id: &str, kind: &str) -> Result<(), WorkflowError> {
        require_non_empty(id, &format!("{kind} id"), kind)?;
        if !self.step_ids.insert(id.to_owned()) {
            return Err(invalid(format!("duplicate step id `{id}`")));
        }
        Ok(())
    }

    /// Copy phase labels onto the calls they cover.
    ///
    /// Runs after the walk, so every id a phase names is known and a
    /// typo is a planning error rather than a silently inert label.
    fn label_phases(&mut self, phases: &[Phase]) -> Result<(), WorkflowError> {
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        let mut phase_ids = BTreeSet::new();
        for phase in phases {
            require_non_empty(&phase.id, "phase id", "phases")?;
            require_non_empty(&phase.label, "phase label", &phase.id)?;
            if !phase_ids.insert(phase.id.clone()) {
                return Err(invalid(format!(
                    "duplicate phase id `{}`",
                    phase.id
                )));
            }
            if phase.steps.is_empty() {
                return Err(invalid(format!(
                    "phase `{}` names no steps",
                    phase.id
                )));
            }
            for step_id in &phase.steps {
                if !self.step_ids.contains(step_id) {
                    return Err(invalid(format!(
                        "phase `{}` names unknown step `{step_id}`",
                        phase.id
                    )));
                }
                if labels
                    .insert(step_id.clone(), phase.label.clone())
                    .is_some()
                {
                    return Err(invalid(format!(
                        "step `{step_id}` is grouped by more than one phase"
                    )));
                }
            }
        }
        for call in &mut self.calls {
            if let Some(label) = labels.get(&call.step_id) {
                call.phase = Some(label.clone());
            }
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> WorkflowError {
    WorkflowError::InvalidSpec {
        message: message.into(),
    }
}

fn require_non_empty(
    value: &str,
    what: &str,
    step_id: &str,
) -> Result<(), WorkflowError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("step `{step_id}` has an empty {what}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{AgentStep, BestOfStep, FanOutStep, PipelineStep};

    fn agent(id: &str) -> Step {
        Step::Agent(AgentStep {
            id: id.to_owned(),
            agent: format!("{id}-agent"),
            prompt: format!("prompt for {id}"),
            gate: None,
            isolation: false,
        })
    }

    fn best_of(id: &str, items: &[&str]) -> BestOfStep {
        BestOfStep {
            id: id.to_owned(),
            agent: "coder".to_owned(),
            items: items.iter().map(|item| (*item).to_owned()).collect(),
            gate: None,
            max_items: None,
            concurrency: None,
        }
    }

    fn spec(steps: Vec<Step>) -> WorkflowSpec {
        WorkflowSpec {
            id: "wf".to_owned(),
            steps,
            phases: Vec::new(),
        }
    }

    fn fan_out(items: usize) -> Step {
        Step::FanOut(FanOutStep {
            id: "fan".to_owned(),
            agent: "fan-agent".to_owned(),
            items: (0..items).map(|index| format!("item {index}")).collect(),
            concurrency: None,
            max_items: None,
        })
    }

    #[test]
    fn positions_follow_the_document_in_pre_order() {
        let document = spec(vec![
            agent("first"),
            Step::Pipeline(PipelineStep {
                id: "pipe".to_owned(),
                stages: vec![agent("stage-one"), fan_out(2)],
            }),
            agent("last"),
        ]);

        let plan = document.plan(&WorkflowCaps::default()).unwrap();
        let ids: Vec<&str> = plan
            .calls()
            .iter()
            .map(|call| call.step_id.as_str())
            .collect();
        assert_eq!(ids, ["first", "stage-one", "fan", "fan", "last"]);
        assert_eq!(plan.call_count(), 5);
        for (position, call) in plan.calls().iter().enumerate() {
            assert_eq!(call.position, position);
        }
        // Only the pipeline's stages are nested.
        let depths: Vec<usize> = plan.calls().iter().map(|c| c.depth).collect();
        assert_eq!(depths, [0, 1, 1, 1, 0]);
    }

    #[test]
    fn identities_change_with_what_the_agent_sees() {
        let baseline = spec(vec![agent("only")])
            .plan(&WorkflowCaps::default())
            .unwrap();
        let renamed = WorkflowSpec {
            steps: vec![Step::Agent(AgentStep {
                id: "only".to_owned(),
                agent: "other-agent".to_owned(),
                prompt: "prompt for only".to_owned(),
                gate: None,
                isolation: false,
            })],
            ..spec(vec![agent("only")])
        }
        .plan(&WorkflowCaps::default())
        .unwrap();
        assert_ne!(baseline.calls()[0].identity, renamed.calls()[0].identity);
    }

    #[test]
    fn a_selection_plans_one_call_per_candidate_under_the_step_gate() {
        let document = spec(vec![
            Step::BestOf(BestOfStep {
                id: "pick".to_owned(),
                agent: "coder".to_owned(),
                items: vec!["one".to_owned(), "two".to_owned()],
                gate: Some(GateRef {
                    command: "cargo test".to_owned(),
                    label: None,
                }),
                max_items: None,
                concurrency: Some(2),
            }),
            agent("after"),
        ]);
        let plan = document.plan(&WorkflowCaps::default()).unwrap();

        assert_eq!(
            plan.steps()[0],
            PlannedStep::BestOf {
                id: "pick".to_owned(),
                concurrency: Some(2),
                items: vec![0, 1],
            }
        );
        assert_eq!(plan.call_count(), 3);
        assert_eq!(
            plan.calls()[0].step_id.as_str(),
            plan.calls()[1].step_id.as_str()
        );
        // The step's gate rides on every candidate, so each one is
        // judged on its own; the step after it declares none.
        for call in &plan.calls()[..2] {
            assert_eq!(
                call.gate.as_ref().map(|gate| gate.command.as_str()),
                Some("cargo test")
            );
        }
        assert!(plan.calls()[2].gate.is_none());
        // Same agent, same gate: the prompt and the position are what
        // tell two candidates apart, in the journal as in the plan.
        assert_ne!(plan.calls()[0].identity, plan.calls()[1].identity);
        assert_eq!(plan.calls()[0].prompt, "one");
        assert_eq!(plan.calls()[1].prompt, "two");
        assert_eq!(plan.calls()[0].depth, 0);
    }

    #[test]
    fn caps_reject_before_anything_runs() {
        let caps = WorkflowCaps {
            max_agents: 2,
            ..WorkflowCaps::default()
        };
        let error = spec(vec![agent("a"), agent("b"), agent("c")])
            .plan(&caps)
            .unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_agents",
                limit: 2,
                requested: 3,
            }
        ));

        let step_capped = Step::FanOut(FanOutStep {
            id: "fan".to_owned(),
            agent: "fan-agent".to_owned(),
            items: vec!["one".to_owned(), "two".to_owned()],
            concurrency: None,
            max_items: Some(1),
        });
        let error = spec(vec![step_capped])
            .plan(&WorkflowCaps::default())
            .unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_items",
                limit: 1,
                requested: 2,
            }
        ));

        // A selection is held to the same item cap as a fan-out: its
        // candidates are what the cap is counting.
        let step_capped = Step::BestOf(BestOfStep {
            max_items: Some(1),
            ..best_of("pick", &["one", "two"])
        });
        let error = spec(vec![step_capped])
            .plan(&WorkflowCaps::default())
            .unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_items",
                limit: 1,
                requested: 2,
            }
        ));

        let run_capped = WorkflowCaps {
            max_items: 1,
            ..WorkflowCaps::default()
        };
        let error = spec(vec![Step::BestOf(best_of("pick", &["one", "two"]))])
            .plan(&run_capped)
            .unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_items",
                limit: 1,
                requested: 2,
            }
        ));

        let nested = Step::Pipeline(PipelineStep {
            id: "outer".to_owned(),
            stages: vec![Step::Pipeline(PipelineStep {
                id: "inner".to_owned(),
                stages: vec![agent("deep")],
            })],
        });
        let caps = WorkflowCaps {
            max_nested: 1,
            ..WorkflowCaps::default()
        };
        let error = spec(vec![nested]).plan(&caps).unwrap_err();
        assert!(matches!(
            error,
            WorkflowError::CapExceeded {
                cap: "max_nested",
                limit: 1,
                requested: 2,
            }
        ));
    }

    #[test]
    fn a_document_that_cannot_run_is_rejected() {
        assert!(spec(Vec::new()).plan(&WorkflowCaps::default()).is_err());

        let duplicated = spec(vec![agent("same"), agent("same")]);
        assert!(matches!(
            duplicated.plan(&WorkflowCaps::default()),
            Err(WorkflowError::InvalidSpec { .. })
        ));

        let empty_prompt = WorkflowSpec {
            steps: vec![Step::Agent(AgentStep {
                id: "a".to_owned(),
                agent: "a-agent".to_owned(),
                prompt: String::new(),
                gate: None,
                isolation: false,
            })],
            ..spec(vec![agent("a")])
        };
        assert!(empty_prompt.plan(&WorkflowCaps::default()).is_err());

        let unknown_phase = WorkflowSpec {
            phases: vec![Phase {
                id: "p".to_owned(),
                label: "Phase".to_owned(),
                steps: vec!["ghost".to_owned()],
            }],
            ..spec(vec![agent("a")])
        };
        assert!(unknown_phase.plan(&WorkflowCaps::default()).is_err());

        // A selection has to have candidates worth choosing between, one
        // prompt each, and a gate that says something.
        let broken = [
            best_of("pick", &[]),
            best_of("pick", &["one", "  "]),
            BestOfStep {
                concurrency: Some(0),
                ..best_of("pick", &["one"])
            },
            BestOfStep {
                gate: Some(GateRef {
                    command: "  ".to_owned(),
                    label: None,
                }),
                ..best_of("pick", &["one"])
            },
        ];
        for step in broken {
            let document = spec(vec![Step::BestOf(step.clone())]);
            assert!(
                matches!(
                    document.plan(&WorkflowCaps::default()),
                    Err(WorkflowError::InvalidSpec { .. })
                ),
                "{step:?} should not plan"
            );
        }
    }

    #[test]
    fn phases_label_calls_without_moving_them() {
        let document = WorkflowSpec {
            phases: vec![Phase {
                id: "p1".to_owned(),
                label: "Review".to_owned(),
                steps: vec!["b".to_owned(), "c".to_owned()],
            }],
            ..spec(vec![agent("a"), agent("b"), agent("c")])
        };
        let plan = document.plan(&WorkflowCaps::default()).unwrap();
        let labels: Vec<Option<&str>> = plan
            .calls()
            .iter()
            .map(|call| call.phase.as_deref())
            .collect();
        assert_eq!(labels, [None, Some("Review"), Some("Review")]);
    }

    #[test]
    fn the_replay_prefix_ends_at_the_first_miss() {
        let plan = spec(vec![agent("a"), agent("b"), agent("c")])
            .plan(&WorkflowCaps::default())
            .unwrap();
        let first = plan.calls()[0].identity.clone();
        let second = plan.calls()[1].identity.clone();
        let third = plan.calls()[2].identity.clone();

        let complete = vec![
            JournalEntry::success(0, first.clone(), "one"),
            JournalEntry::success(1, second.clone(), "two"),
            JournalEntry::success(2, third, "three"),
        ];
        assert_eq!(plan.replay_prefix(&complete), 3);

        // A failure stops the prefix: the run must retry that call.
        let failed = vec![
            JournalEntry::success(0, first.clone(), "one"),
            JournalEntry::failure(1, second.clone(), "boom"),
            JournalEntry::success(2, plan.calls()[2].identity.clone(), "three"),
        ];
        assert_eq!(plan.replay_prefix(&failed), 1);

        // A changed prompt stops it at that position and no further.
        let changed = vec![
            JournalEntry::success(0, first.clone(), "one"),
            JournalEntry::success(1, "not-the-key".to_owned(), "two"),
            JournalEntry::success(2, plan.calls()[2].identity.clone(), "three"),
        ];
        assert_eq!(plan.replay_prefix(&changed), 1);

        // A hole is a miss even when every later entry matches.
        let holed = vec![
            JournalEntry::success(0, first, "one"),
            JournalEntry::success(2, plan.calls()[2].identity.clone(), "three"),
        ];
        assert_eq!(plan.replay_prefix(&holed), 1);

        assert_eq!(plan.replay_prefix(&[]), 0);
    }

    #[test]
    fn a_lost_candidate_replays_once_its_step_has_a_winner() {
        let document = spec(vec![
            Step::BestOf(best_of("pick", &["one", "two"])),
            agent("after"),
        ]);
        let plan = document.plan(&WorkflowCaps::default()).unwrap();
        let first = plan.calls()[0].identity.clone();
        let second = plan.calls()[1].identity.clone();
        let third = plan.calls()[2].identity.clone();

        let loss = JournalEntry::failure(0, first.clone(), "boom");
        let win = JournalEntry::success(1, second.clone(), "answer: two");
        let next = JournalEntry::success(2, third.clone(), "done");

        // The step has a winner on record, so its candidate's failure is
        // a decided loss: the whole document still replays, winner and
        // following step included.
        assert_eq!(
            plan.replay_prefix(&[loss.clone(), win.clone(), next.clone()]),
            3
        );

        // A selection nobody won is not decided: the first failed
        // candidate is where the retry starts.
        let nobody = JournalEntry::failure(1, second.clone(), "boom too");
        assert_eq!(
            plan.replay_prefix(&[loss.clone(), nobody.clone(), next.clone()]),
            0
        );

        // A sibling that no longer matches the plan is not a winner, so
        // it cannot decide anything either.
        let stale = JournalEntry::success(1, "not-the-key".to_owned(), "x");
        assert_eq!(plan.replay_prefix(&[loss.clone(), stale, next.clone()]), 0);

        // Only a selection gets this treatment: a failed fan-out item is
        // still the first call a resume has to make.
        let fan = spec(vec![Step::FanOut(FanOutStep {
            id: "fan".to_owned(),
            agent: "fan-agent".to_owned(),
            items: vec!["one".to_owned(), "two".to_owned()],
            concurrency: None,
            max_items: None,
        })])
        .plan(&WorkflowCaps::default())
        .unwrap();
        let entries = vec![
            JournalEntry::failure(0, fan.calls()[0].identity.clone(), "boom"),
            JournalEntry::success(1, fan.calls()[1].identity.clone(), "two"),
        ];
        assert_eq!(fan.replay_prefix(&entries), 0);
        assert_eq!(fan.replay_prefix(&entries[1..]), 0);
    }
}
