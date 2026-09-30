//! The MCTS step walkthrough: branch columns of chained depths, and
//! the scorer's winner.

use futures::future::join_all;

use super::{super::execution::Execution, Executor};
use crate::{
    concurrency::Semaphore,
    error::WorkflowError,
    host::{GateVerdict, WorkflowHost},
    result::{BranchScore, CallRun, CallStatus},
    scorer::{BuiltinScorer, ScoredBranch},
};

impl<H: WorkflowHost> Executor<'_, H> {
    /// Run an MCTS step: each branch is a column of calls (depth 0 ..
    /// max_depth), all chained off the previous depth's text. The
    /// scorer reads the last depth's text; the highest-scoring branch
    /// wins, ties broken by `branch_id`.
    ///
    /// Depth rows run concurrently among themselves, but depth `d + 1`
    /// waits for depth `d` to settle so the chain input is the same
    /// branch's previous text. The step's `concurrency` cap bounds
    /// every row's in-flight count; the run's semaphore bounds the
    /// whole run.
    pub(super) async fn run_mcts(
        &self,
        step_id: &str,
        concurrency: Option<usize>,
        branches: &[usize],
        depths: &[Vec<usize>],
        scorer: &BuiltinScorer,
        pipeline_input: Option<&str>,
    ) -> Result<Execution, WorkflowError> {
        let branch_count = branches.len();
        let mut all_calls: Vec<CallRun> = Vec::with_capacity(
            branch_count + depths.iter().map(Vec::len).sum::<usize>(),
        );
        // `branch_text[b]` is the depth-d text of branch b. After
        // depth d completes we read it to feed depth d + 1; `None`
        // means the branch has nothing to chain (it failed) and the
        // next depth runs unchained.
        let mut branch_text: Vec<Option<String>> = vec![None; branch_count];
        // First depth: no chain input. Each call's chain is the
        // pipeline's previous step's output, exactly like a fan-out's
        // first round.
        let row0 = self
            .run_items(branches, concurrency, pipeline_input)
            .await?;
        for (branch_index, run) in row0.iter().enumerate() {
            if let Some(text) = run.text.as_deref().filter(|t| !t.is_empty()) {
                branch_text[branch_index] = Some(text.to_owned());
            }
        }
        all_calls.extend(row0);
        for row in depths {
            if self.control.is_aborted() {
                break;
            }
            let chain_inputs: Vec<Option<String>> = branch_text.clone();
            let runs = self
                .run_mcts_row_chained(row, concurrency, &chain_inputs)
                .await?;
            for (branch_index, run) in runs.iter().enumerate() {
                if let Some(text) =
                    run.text.as_deref().filter(|t| !t.is_empty())
                    && let Some(slot) = branch_text.get_mut(branch_index)
                {
                    *slot = Some(text.to_owned());
                }
            }
            all_calls.extend(runs);
        }
        let winner_index = self.pick_mcts_winner(
            step_id,
            branches,
            depths,
            &branch_text,
            scorer,
            &mut all_calls,
        );
        if let Some(index) = winner_index {
            let winner_pos = depths
                .last()
                .and_then(|row| row.get(index))
                .copied()
                .unwrap_or(branches[index]);
            if let Some(call) =
                all_calls.iter_mut().find(|c| c.position == winner_pos)
            {
                call.select();
                if let Some(score) = call.branch_score.as_mut() {
                    score.winner = true;
                }
            }
        }
        let output = winner_index
            .and_then(|index| branch_text.get(index).and_then(Clone::clone))
            .filter(|text| !text.is_empty());
        // Losing branches get re-marked as superseded, mirroring the
        // best_of step's behavior. A skipped or aborted call keeps
        // its status because those are facts about the run, not the
        // selection.
        let winner_pos = winner_index.and_then(|index| {
            depths
                .last()
                .and_then(|row| row.get(index))
                .or_else(|| branches.get(index))
                .copied()
        });
        for call in &mut all_calls {
            if Some(call.position) == winner_pos {
                continue;
            }
            if matches!(call.status, CallStatus::Succeeded | CallStatus::Failed)
            {
                call.supersede();
            }
        }
        let _ = step_id;
        Ok(Execution {
            calls: all_calls,
            output,
        })
    }

    /// Run a depth row with per-branch chain input.
    async fn run_mcts_row_chained(
        &self,
        items: &[usize],
        concurrency: Option<usize>,
        chain_inputs: &[Option<String>],
    ) -> Result<Vec<CallRun>, WorkflowError> {
        let bound = concurrency.map(Semaphore::new);
        let runs =
            join_all(items.iter().enumerate().map(|(index, position)| {
                let call = self.call(*position);
                let bound = bound.as_ref();
                let chain = chain_inputs
                    .get(index)
                    .and_then(Option::as_deref)
                    .map(str::to_owned);
                async move {
                    let _step_permit = match bound {
                        Some(semaphore) => match semaphore.acquire().await {
                            Ok(permit) => Some(permit),
                            Err(_drained) => return Ok(CallRun::aborted(call)),
                        },
                        None => None,
                    };
                    self.run_call(call, chain.as_deref()).await
                }
            }))
            .await;
        let mut calls = Vec::with_capacity(runs.len());
        for run in runs {
            calls.push(run?);
        }
        Ok(calls)
    }

    /// Score every branch's last-depth text and pick a winner.
    ///
    /// Writes a [`BranchScore`] onto the branch's last-depth call so
    /// the run reports why the winner won. The tie-break is `branch_id`
    /// ascending — the lower position in the plan wins.
    fn pick_mcts_winner(
        &self,
        _step_id: &str,
        branches: &[usize],
        depths: &[Vec<usize>],
        branch_text: &[Option<String>],
        scorer: &BuiltinScorer,
        calls: &mut [CallRun],
    ) -> Option<usize> {
        let last_row = depths.last().map(Vec::as_slice).unwrap_or(branches);
        let mut best: Option<(usize, f64)> = None; // (branch_index, score)
        let mut best_branch_id: usize = usize::MAX;
        for (branch_index, &branch_pos) in branches.iter().enumerate() {
            let last_pos =
                last_row.get(branch_index).copied().unwrap_or(branch_pos);
            let text = branch_text.get(branch_index).and_then(Option::as_ref);
            let gate = calls
                .iter()
                .find(|c| c.position == last_pos)
                .map(|c| c.gate)
                .unwrap_or(GateVerdict::Absent);
            let agent = self.call(branch_pos).agent.as_str();
            let prompt = self.call(branch_pos).prompt.as_str();
            let scored = ScoredBranch {
                branch_id: branch_pos,
                agent,
                prompt,
                text: text.map(String::as_str),
                gate,
                branch_index,
            };
            let score = scorer.score(&scored);
            if let Some(call) =
                calls.iter_mut().find(|c| c.position == last_pos)
            {
                call.branch_score = Some(BranchScore {
                    branch_id: branch_pos,
                    branch_index,
                    text: text.cloned(),
                    score: Some(score),
                    gate,
                    winner: false,
                });
            }
            // Tie-break: equal scores go to the smaller `branch_id`
            // (the lower position in the plan), exactly the rule the
            // scorer module documents.
            let is_better = match best {
                None => true,
                Some((_, best_score)) => {
                    score > best_score
                        || (score == best_score && branch_pos < best_branch_id)
                }
            };
            if is_better {
                best = Some((branch_index, score));
                best_branch_id = branch_pos;
            }
        }
        best.map(|(branch_index, _)| branch_index)
    }
}
