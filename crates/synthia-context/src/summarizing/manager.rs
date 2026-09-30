//! [`SummarizingContextManager`] — the pluggable, async,
//! LLM-summarising context-window manager. See the module docs
//! on [`super`] for the design rationale and failure modes.

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use synthia_provider::{Message, Role};
use synthia_session::{CompactionOutcome, SurfaceToken};

use super::{
    SummariseFn,
    archive::{ToolCallArchive, archive_record_for},
    lifecycle::{CompactionLifecycle, CompactionRecord},
    prompt::{extract_tool_result_text, serialise_batch_for_summariser},
    refs::{allocate_short_refs, build_summary_message},
};
use crate::{
    compaction_settings::{CompactionDetails, CompactionSettings},
    context_manager::{AgentState, ContextManager},
};

///
/// See the `summarizing` module docs for the full design rationale. The
/// default implementation is held in
/// [`SummarizingContextManager::new`]; pass your own summariser
/// to swap the LLM.
pub struct SummarizingContextManager {
    /// Async LLM call that takes a serialised tool-batch and
    /// returns a summary. Cheap / fast models work best here;
    /// the calling loop already pays for the orchestrating model.
    summarise: SummariseFn,
    /// In-memory archive of every pruned tool call. Shared with
    /// the `context_tree_query` tool the agent registers so the
    /// model can recover any pruned output on demand.
    archive: Arc<ToolCallArchive>,
    /// R5-8: optional callback invoked on every replace with the
    /// `(start, end, source_indices, summary_text)` tuple. Wired
    /// by the agent loop to translate the splice into a typed
    /// `SessionEvent::Compaction { surface_op: Replace, … }` so
    /// replay shows the durable compaction log instead of
    /// silently mutating `messages` in place.
    compaction_emitter: Option<Arc<dyn Fn(CompactionRecord) + Send + Sync>>,
    /// R29-Phase-J: optional compaction *lifecycle* emitter.
    /// Fires Start before the summariser call, Summary after a
    /// usable result, and End on every exit path. The pair of
    /// Start/End is what makes a crash mid-compaction detectable
    /// in the durable log.
    compaction_lifecycle:
        Option<Arc<dyn Fn(CompactionLifecycle) + Send + Sync>>,
    /// R15-1: optional compaction policy. When `Some`, the
    /// batch pruning is gated on
    /// [`crate::should_compact`] (utilisation threshold +
    /// enabled switch + message-count throttle) instead of
    /// pruning every batch unconditionally. `None` keeps the
    /// legacy always-prune behaviour.
    settings: Option<CompactionSettings>,
    /// R29-C: file activity recorded by the previous compaction.
    /// When `Some`, the serialised batch handed to the summariser
    /// names the paths read/modified since then so the summary
    /// carries that context forward. Per-compaction data (not a
    /// policy knob); `None` leaves the prompt unchanged.
    compaction_details: Option<CompactionDetails>,
    /// Messages observed since the last prune. Feeds the
    /// `min_messages_between_compaction` throttle; reset to 0
    /// after a successful splice. Interior atomic because
    /// `prepare` takes `&self`.
    pub(super) messages_since_compaction: std::sync::atomic::AtomicU32,
    /// R29-Phase-K: the file activity the *next* `prepare`
    /// should hand the summariser. Installed by the agent loop
    /// through [`ContextManager::set_compaction_details`]; takes
    /// precedence over the builder-set
    /// [`Self::with_compaction_details`] value once written.
    /// `Mutex` because `prepare` and `set_compaction_details`
    /// both take `&self`.
    runtime_details: Mutex<Option<CompactionDetails>>,
}

impl SummarizingContextManager {
    pub fn new(summarise: SummariseFn) -> Self {
        Self {
            summarise,
            archive: Arc::new(ToolCallArchive::new()),
            compaction_emitter: None,
            compaction_lifecycle: None,
            settings: None,
            compaction_details: None,
            messages_since_compaction: std::sync::atomic::AtomicU32::new(0),
            runtime_details: Mutex::new(None),
        }
    }

    /// Construct with a caller-provided archive (e.g. a
    /// pre-populated index from session resume). The default
    /// archive is empty.
    pub fn with_archive(
        summarise: SummariseFn,
        archive: Arc<ToolCallArchive>,
    ) -> Self {
        Self {
            summarise,
            archive,
            compaction_emitter: None,
            compaction_lifecycle: None,
            settings: None,
            compaction_details: None,
            messages_since_compaction: std::sync::atomic::AtomicU32::new(0),
            runtime_details: Mutex::new(None),
        }
    }

    /// R15-1: install the compaction policy. When set,
    /// `prepare` consults [`crate::should_compact`] (utilisation
    /// threshold + enabled + message throttle) before pruning;
    /// unset keeps the legacy always-prune behaviour.
    #[must_use]
    pub fn with_settings(mut self, settings: CompactionSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// R29-C: attach the file activity recorded by the previous
    /// compaction. When set, the serialised batch handed to the
    /// summariser names the paths read/modified since then.
    /// Unset (the default) leaves the prompt unchanged.
    #[must_use]
    pub fn with_compaction_details(
        mut self,
        details: CompactionDetails,
    ) -> Self {
        self.compaction_details = Some(details);
        self
    }

    /// Borrow the attached compaction details, if any.
    pub fn compaction_details(&self) -> Option<&CompactionDetails> {
        self.compaction_details.as_ref()
    }

    /// Install the R5-8 compaction emitter. Called from the
    /// agent loop after constructing the manager. The callback
    /// fires synchronously from inside `prepare`; if the
    /// callback panics, the splice is left intact (the
    /// callback MUST NOT mutate `messages`).
    pub fn with_compaction_emitter<F>(mut self, f: F) -> Self
    where
        F: Fn(CompactionRecord) + Send + Sync + 'static,
    {
        self.compaction_emitter = Some(Arc::new(f));
        self
    }

    /// R29-Phase-J: install the compaction *lifecycle* emitter.
    ///
    /// Fires `Start` before the summariser call, `Summary` after a
    /// usable result, and `End` on every exit path (committed or
    /// failed). A log that pairs them can distinguish a crashed
    /// compaction (unmatched `Start`) from a completed one — see
    /// `synthia_session::orphaned_compactions`.
    ///
    /// Like [`Self::with_compaction_emitter`], the callback fires
    /// synchronously from inside `prepare` and MUST NOT mutate
    /// `messages`.
    #[must_use]
    pub fn with_compaction_lifecycle_emitter<F>(mut self, f: F) -> Self
    where
        F: Fn(CompactionLifecycle) + Send + Sync + 'static,
    {
        self.compaction_lifecycle = Some(Arc::new(f));
        self
    }

    /// Fire one lifecycle step when an emitter is installed.
    fn emit_lifecycle(&self, step: CompactionLifecycle) {
        if let Some(emitter) = self.compaction_lifecycle.as_ref() {
            emitter(step);
        }
    }

    /// Borrow the underlying archive. Callers wire this into the
    /// `context_tree_query` tool so the model can query by short
    /// alias or canonical id.
    pub fn archive(&self) -> &Arc<ToolCallArchive> {
        &self.archive
    }
}

#[async_trait]
impl ContextManager for SummarizingContextManager {
    async fn prepare(
        &self,
        messages: &mut Vec<Message>,
        state: &mut AgentState,
    ) {
        // Reset the per-call flag and refresh the estimate before
        // we mutate anything, so downstream callers can rely on
        // `state.estimated_tokens` reflecting the *input* to this
        // call.
        state.last_truncated = false;
        state.estimated_tokens = self.estimate_tokens(messages);

        if messages.is_empty() {
            return;
        }

        // R15-1: settings-driven trigger. When a policy is
        // installed, prune only when the utilisation threshold,
        // the enabled switch, and the message-count throttle all
        // agree. The throttle counts messages accumulated since
        // the last successful prune, including this batch.
        if let Some(settings) = self.settings {
            let count = self
                .messages_since_compaction
                .fetch_add(
                    messages.len() as u32,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .saturating_add(messages.len() as u32);
            if !crate::should_compact(
                state.estimated_tokens as u64,
                state.context_window as u64,
                count,
                &settings,
            ) {
                return;
            }
        }

        // Walk once: collect every tool-result message and the
        // assistant turn that *contains* the corresponding tool
        // calls. We only prune the tool-result messages; the
        // assistant turn stays verbatim so the conversation
        // surface still shows "model asked → tool answered".
        let mut batches: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();

        for (i, msg) in messages.iter().enumerate() {
            if msg.role == Role::Tool {
                current.push(i);
            } else if !current.is_empty() {
                batches.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            batches.push(current);
        }

        if batches.is_empty() {
            return;
        }

        // Walk each batch back-to-front. Reverse order keeps
        // earlier indices stable as we splice.
        let mut did_prune = false;
        for batch in batches.into_iter().rev() {
            // Serialise the batch as a user-message for the
            // summariser LLM. Tool-name + result-text per call,
            // wrapped in XML fences so a malicious / hijacked tool
            // cannot inject instructions into the summariser
            // (mirrors pi-lcm's `prompts.ts` XML-fence mitigation).
            // R29-C: the previous compaction's file activity is
            // appended so the summary can name what was touched.
            // R29-K: the runtime-installed value (written by the
            // agent loop each iteration) wins over the
            // builder-set one when present.
            let effective_details = self
                .runtime_details
                .lock()
                .clone()
                .or_else(|| self.compaction_details.clone());
            let serialised = serialise_batch_for_summariser(
                &batch,
                messages,
                effective_details.as_ref(),
            );

            let raw_size: usize = batch
                .iter()
                .filter_map(|&i| messages.get(i))
                .filter_map(extract_tool_result_text)
                .map(|t| t.len())
                .sum();

            // R29-Phase-J: open the lifecycle before the LLM
            // call so a crash inside the summariser leaves an
            // unmatched `Start` in the log.
            let token = SurfaceToken::new();
            self.emit_lifecycle(CompactionLifecycle::Start {
                token: token.clone(),
            });

            let summary = (self.summarise)(&serialised).await;
            let summary_text = match summary {
                Some(s) => s,
                None => {
                    // Summariser failed — leave the batch in
                    // place. Mirrors pi-context-prune's
                    // "skip-oversized" + "summarizer-failed"
                    // branches: never lose information when the
                    // recovery path itself is broken.
                    self.emit_lifecycle(CompactionLifecycle::End {
                        token,
                        outcome: CompactionOutcome::Failed,
                    });
                    continue;
                }
            };

            // Skip when the summary is *larger* than what it
            // replaces. Same heuristic as pi-context-prune's
            // `shouldSkipOversized` check.
            if summary_text.len() > raw_size {
                self.emit_lifecycle(CompactionLifecycle::End {
                    token,
                    outcome: CompactionOutcome::Failed,
                });
                continue;
            }

            self.emit_lifecycle(CompactionLifecycle::Summary {
                token: token.clone(),
                summary: summary_text.clone(),
                shadowed_positions: batch.iter().map(|i| *i as u64).collect(),
            });

            // Build the replacement summary message as an
            // assistant turn carrying the short-ref footer so
            // `context_tree_query` can resolve the aliases.
            let short_refs = allocate_short_refs(&batch);
            let summary_msg = build_summary_message(&short_refs, &summary_text);

            // Archive each tool call so `context_tree_query` can
            // recover the original output.
            for &i in &batch {
                if let Some(msg) = messages.get(i)
                    && let Some(record) = archive_record_for(msg)
                {
                    self.archive.insert(record);
                }
            }

            // Splice: replace the contiguous run of tool-result
            // messages with the single summary message. The
            // assistant turn that contained the original
            // tool_use blocks stays untouched.
            let first = batch[0];
            let last = *batch.last().expect("batch is non-empty");
            // R5-8: emit a `CompactionRecord` describing the splice
            // to the registered emitter (wired by the agent loop
            // to a typed `SessionEvent::Compaction`). Pre-splice
            // indices so the durable event references the
            // historical messages that were replaced.
            if let Some(emitter) = self.compaction_emitter.as_ref() {
                emitter(CompactionRecord {
                    start: first,
                    end: last + 1,
                    source_indices: batch.clone(),
                    source_keys: batch
                        .iter()
                        .map(|&i| {
                            messages
                                .get(i)
                                .and_then(|msg| msg.tool_call_id.clone())
                        })
                        .collect(),
                    summary_text: summary_text.clone(),
                });
            }
            // Drop indices in reverse so each removal does not
            // shift the ones still to drop.
            for &i in batch.iter().skip(1).collect::<Vec<_>>().iter().rev() {
                messages.remove(*i);
            }
            messages[first] = summary_msg;
            did_prune = true;
            // R29-Phase-J: the splice landed — close the lifecycle
            // as committed. Paired with the `Start` above, this is
            // what lets a log distinguish a finished compaction
            // from a crashed one.
            self.emit_lifecycle(CompactionLifecycle::End {
                token,
                outcome: CompactionOutcome::Committed,
            });
        }

        if did_prune {
            state.last_truncated = true;
            // R15-1: a successful splice resets the
            // message-count throttle.
            self.messages_since_compaction
                .store(0, std::sync::atomic::Ordering::Relaxed);
        }
        state.estimated_tokens = self.estimate_tokens(messages);
    }

    /// R29-Phase-K: the agent loop installs the files touched
    /// since the last iteration. Stored (not merged) so each
    /// iteration reports only its own activity — the summariser
    /// prompt describes the most recent run, not the whole
    /// session.
    fn set_compaction_details(&self, details: CompactionDetails) {
        *self.runtime_details.lock() = Some(details);
    }
}
