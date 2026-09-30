//! Starting one agent run: admitting pending input, composing the
//! agent, and the spawned task that drives it to completion.
//! Method continuation of the inner state's `ControllerInner`.

use std::sync::Arc;

use futures::StreamExt;
use synthia::{
    core::Clock,
    harness::AgentInput,
    provider::{Content, ContentPart, Message, Role, TextContent},
    session::{CompactionCheckpoint, SurfaceLedger},
};

use super::{
    inner::ControllerInner,
    ops::SessionState,
    run_log::{
        RunLog,
        attachment_refs_of_parts,
        prompt_text_of_parts,
        rerun_replace_op,
    },
};

impl ControllerInner {
    /// Start a new agent run if and only if the controller is idle
    /// AND there is pending work — either a multimodal
    /// `pending_multimodal` slot or at least one queued text
    pub(super) async fn maybe_start_run(
        self: &Arc<Self>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.is_startable() {
            return None;
        }

        // Multimodal prompts (with optional agent override) take
        // precedence over text-only queue entries: the per-run
        // `pending_multimodal` slot is the only place the
        // lossless parts (image/audio/file bytes) live.
        let multimodal_take = self.pending_multimodal.lock().take();
        let has_text_pending = self
            .queue
            .has_pending(&self.user_id, &self.session_id)
            .await;
        if multimodal_take.is_none() && !has_text_pending {
            tracing::trace!(
                target: "synthia.session",
                session_id = %self.session_id,
                "maybe_start_run: skipped (no pending inputs)"
            );
            return None;
        }

        let multimodal_active = multimodal_take.is_some();
        self.mark_running(multimodal_active).await;

        // If the multimodal slot carried an explicit agent
        // selection, override the configured default for THIS
        // run only. Reading from the parked deps Mutex keeps
        // the per-dispatch override out of the persistent
        // `default_agent_name` field — a one-shot chat
        // dispatch must not leak into future runs.
        let explicit_agent_name = multimodal_take
            .as_ref()
            .and_then(|parked| parked.agent_name.clone());
        // The turn's provider selection, taken here so it governs
        // exactly this run: `None` keeps the provider on the
        // dependencies, and a selection another op parked afterwards
        // belongs to *its* turn, not this one.
        let provider = self.pending_provider.lock().take();
        // R6-B: create the typed-event channel for this run.
        // The sender goes to the agent loop (structural
        // boundary events); the receiver is drained inside the
        // spawned run task below.
        let (typed_sink, typed_rx) =
            synthia::session::TypedEventSink::channel(256);
        // R34: the run's durable surface ledger and checkpoint. The
        // ledger is filled by the run task (it needs the log read,
        // which happens there); the checkpoint is installed on the
        // composed context manager *now* so the manager the factory
        // builds already carries its emitters.
        let ledger = Arc::new(SurfaceLedger::new());
        let compaction_configured = self.deps.lock().compaction.is_some();
        let checkpoint = compaction_configured.then(|| {
            CompactionCheckpoint::new(typed_sink.clone(), Arc::clone(&ledger))
        });
        let mut config = self.build_run_config_with_explicit_agent(
            explicit_agent_name.as_deref(),
            provider,
            checkpoint.as_ref(),
        );
        config.typed_event_sink = Some(typed_sink);
        let factory = Arc::clone(&self.run_factory);
        let inner = Arc::clone(self);
        let pending_turn =
            multimodal_take.map(|parked| (parked.parts, parked.rerun));

        let span = tracing::info_span!(
            target: "synthia.session",
            "agent.run",
            session.id = %self.session_id,
            user.id = %self.user_id,
            iteration.id = tracing::field::Empty,
            agent.name = tracing::field::Empty,
        );
        Some(tokio::spawn(async move {
            let _entered = span.enter();
            span.record(
                "agent.name",
                config.agent_name.as_deref().unwrap_or("<default>"),
            );
            // When a multimodal payload is parked on `inner`, we
            // skip the queue entirely: the multimodal prompt
            // IS this run's input, and merging it with whatever
            // straggler text the queue holds would conflate two
            // turns into one. The text queue's existence on the
            // controller is preserved for the plain-text path.
            let (pending_text, multimodal_parts, rerun_turn) =
                if let Some((parts, rerun)) = pending_turn {
                    (Vec::new(), Some(parts), rerun)
                } else {
                    let drained = match inner
                        .queue
                        .drain_pending(&inner.user_id, &inner.session_id)
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!(
                                target: "synthia.session",
                                session_id = %inner.session_id,
                                error = %e,
                                "Failed to drain session input queue"
                            );
                            Vec::new()
                        }
                    };
                    (drained, None, false)
                };
            tracing::debug!(
                target: "synthia.session",
                session_id = %inner.session_id,
                drained_count = pending_text.len(),
                "Agent run task: drained input queue"
            );
            let (pending_history, prompt) = pending_text.into_iter().fold(
                (Vec::new(), String::new()),
                |(mut hist, mut last), entry| {
                    if last.is_empty() {
                        last = entry.content;
                    } else {
                        hist.push(Message {
                            role: Role::User,
                            content: Content::Single(ContentPart::Text(
                                TextContent {
                                    text: last,
                                    cache_control: None,
                                },
                            )),
                            tool_call_id: None,
                            name: None,
                            ..Default::default()
                        });
                        last = entry.content;
                    }
                    (hist, last)
                },
            );
            // Reconstruct prior-turn assistant / tool messages from
            // the durable events the previous runs persisted to the
            // session sink. Without this, every run starts with an
            // empty history and the LLM loses all multi-turn
            // memory. The sink is the only durable source of truth
            // here — `InputQueue` is per-run and ephemeral.
            // R34: the same read feeds the durable surface ledger, so
            // every row already on disk is indexed with the ordinal
            // the resume projection will give it. Rows this run
            // appends go through `RunLog`, which keeps the ledger in
            // step.
            let (mut log, sink_history, rerun_replace) = match inner
                .session_store
                .read()
                .await
            {
                Ok(events) => {
                    for (index, row) in events.iter().enumerate() {
                        ledger.record(index as u64 + 1, row);
                    }
                    let last_seq = inner
                        .session_store
                        .snapshot()
                        .await
                        .map(|snapshot| snapshot.last_event_seq)
                        .unwrap_or(events.len() as u64);
                    // A rerun shadows the turn it replaces, so
                    // the prompt row it is about to append
                    // carries replace semantics computed against
                    // THIS fold — the surface as it stands before
                    // the rerun's own rows land.
                    let rerun_replace = if rerun_turn {
                        rerun_replace_op(&events)
                    } else {
                        None
                    };
                    (
                        RunLog::new(
                            Arc::clone(&inner.session_store),
                            Arc::clone(&ledger),
                            last_seq,
                            inner.deps.lock().clock.clone(),
                        ),
                        synthia::context::events_to_messages(&events),
                        rerun_replace,
                    )
                }
                Err(e) => {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to read session sink; starting run with empty history"
                    );
                    (
                        RunLog::new(
                            Arc::clone(&inner.session_store),
                            Arc::clone(&ledger),
                            0,
                            inner.deps.lock().clock.clone(),
                        ),
                        Vec::new(),
                        // No readable log ⇒ nothing to shadow ⇒
                        // the rerun degrades to a plain append.
                        None,
                    )
                }
            };
            // Typed request-header epoch marker (R4 Phase C.3,
            // dsh `EpochHeader` semantics): before the run's user
            // prompts land in the sink, stamp the resolved
            // request config so a later replay can tell exactly
            // which provider/model/tool-set produced each span of
            // the log. Re-emitted only on `initial` (first run)
            // or `change` (config drift between runs).
            {
                let (provider, tool_registry) = (
                    // The provider that *runs* this turn: the config
                    // carries the turn's `model` selection when it had
                    // one, so the header names what actually sampled
                    // rather than the deployment default it was
                    // overridden away from.
                    std::sync::Arc::clone(&config.provider),
                    {
                        let deps = inner.deps.lock();
                        std::sync::Arc::clone(&deps.tool_registry)
                    },
                );
                let provider_name = provider.name().to_string();
                let model = provider.model_config().name.clone();
                let registry = tool_registry.read().await;
                let mut tool_names: Vec<String> = {
                    use synthia::core::{
                        Registry as _,
                        registry::RegistryItem as _,
                    };
                    registry
                        .list(None)
                        .await
                        .map(|entries| {
                            entries
                                .iter()
                                .map(|e| e.name().to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                };
                tool_names.sort();
                let tools_hash = {
                    use std::hash::{Hash, Hasher};
                    let mut h =
                        std::collections::hash_map::DefaultHasher::new();
                    tool_names.hash(&mut h);
                    h.finish()
                };
                let should_emit = {
                    let last = inner.last_request_header.lock();
                    *last
                        != Some((
                            provider_name.clone(),
                            model.clone(),
                            tools_hash,
                        ))
                };
                if should_emit {
                    let reason = if inner.last_request_header.lock().is_some() {
                        "change"
                    } else {
                        "initial"
                    };
                    *inner.last_request_header.lock() = Some((
                        provider_name.clone(),
                        model.clone(),
                        tools_hash,
                    ));
                    let header =
                        synthia::session::SessionEvent::RequestHeader {
                            seq: 0,
                            ts: inner.deps.lock().clock.now().to_rfc3339(),
                            data: serde_json::json!({
                                "reason": reason,
                                "provider": provider_name,
                                "model": model,
                                "tools_hash": format!("{tools_hash:016x}"),
                            }),
                        };
                    if let Ok(v) = serde_json::to_value(&header)
                        && let Err(e) = log.append_typed(v).await
                    {
                        tracing::warn!(
                            target: "synthia.session",
                            session_id = %inner.session_id,
                            error = %e,
                            "Failed to persist typed request_header"
                        );
                    }
                }
            }
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                sink_history_len = sink_history.len(),
                pending_history_len = pending_history.len(),
                "Agent run task: composed history from sink + pending"
            );
            // Persist this run's drained user prompts to the sink so
            // the NEXT run sees them in `sink_history`. We do this
            // AFTER reading the sink (so this run doesn't echo its
            // own prompt back through `sink_history`) and BEFORE
            // invoking the agent (so a fast agent doesn't emit
            // assistant events before the user prompts are
            // recorded — `events_to_history` walks the JSONL in
            // append order, so user prompts must precede the
            // assistant turns they elicited).
            //
            // Multimodal runs persist only the textual component
            // (when one exists). Raw image/audio bytes are NOT
            // serialised into the JSONL sink: the sink is for
            // `UserInput` envelopes whose `data.text` is a plain
            // string, and base64-encoding 5 MB of image data per
            // turn would bloat the per-session log without
            // anything reading it back as image bytes. The
            // durable rehydration only needs the text transcript;
            // the model already saw the bytes for THIS turn.
            for msg in &pending_history {
                let text = match &msg.content {
                    Content::Single(ContentPart::Text(t)) => &t.text,
                    _ => continue,
                };
                if let Err(e) = log
                    .append(&serde_json::json!({
                        "type": "UserInput",
                        "data": { "text": text },
                    }))
                    .await
                {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist drained user prompt to session sink"
                    );
                }
            }
            // The typed text of a multimodal turn never reaches
            // `prompt`: `PromptMulti` parks the whole payload in
            // `parts`, so `pending_history` is empty and `prompt`
            // is `""`. Persist the `Text` parts here so the user's
            // question lands in the durable transcript — without
            // this a multimodal turn writes no `UserInput` row at
            // all, and replay shows the assistant's reply with no
            // question above it.
            let current_turn_text = if prompt.is_empty() {
                multimodal_parts
                    .as_deref()
                    .map(prompt_text_of_parts)
                    .unwrap_or_default()
            } else {
                prompt.clone()
            };
            // A rerun's prompt row carries the replace op computed
            // against the pre-rerun fold, so the durable log folds
            // to the replacement story (one prompt, the new answer)
            // instead of the turn twice — see `rerun_replace_op`.
            //
            // The row also records SMALL references to the turn's
            // attachments (kind / name / mime / byte length) — never
            // their bytes, per the note above. Without a reference a
            // reloaded transcript has no way to show that the turn
            // carried anything, so a file or image turn rendered as an
            // empty bubble on the session-detail page even though the
            // live chat had shown a chip for it. A text-only turn omits
            // the field entirely, keeping the common row shape
            // unchanged.
            let attachment_refs = multimodal_parts
                .as_deref()
                .map(attachment_refs_of_parts)
                .unwrap_or_default();
            if !current_turn_text.is_empty() || !attachment_refs.is_empty() {
                let mut row = serde_json::json!({
                    "type": "UserInput",
                    "data": { "text": current_turn_text },
                });
                if !attachment_refs.is_empty()
                    && let Some(obj) =
                        row.get_mut("data").and_then(|d| d.as_object_mut())
                {
                    obj.insert(
                        "attachments".to_string(),
                        serde_json::Value::Array(attachment_refs),
                    );
                }
                if let (Some(op), Some(obj)) =
                    (rerun_replace.as_ref(), row.as_object_mut())
                    && let Ok(op_value) = serde_json::to_value(op)
                {
                    obj.insert("surface_op".to_string(), op_value);
                }
                if let Err(e) = log.append(&row).await {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist current-turn user prompt to session sink"
                    );
                }
            }
            let mut history = sink_history;
            history.extend(pending_history);
            // Build the AgentInput. Two paths:
            //
            // 1. Multimodal path (`multimodal_parts.is_some()`):
            //    bypass the text-fold and feed the parts
            //    directly via `AgentInput::multi`. The text
            //    preview is still echoed into `history` so the
            //    NEXT turn's agent sees what the user said
            //    alongside the image (the bytes are in-memory
            //    only — the next run re-loads from the sink and
            //    therefore has no access to them).
            //
            // 2. Text path: identical to before.
            let input = if let Some(parts) = multimodal_parts {
                if !prompt.is_empty() {
                    let text_preview = Message {
                        role: Role::User,
                        content: Content::Single(ContentPart::Text(
                            TextContent {
                                text: prompt.clone(),
                                cache_control: None,
                            },
                        )),
                        tool_call_id: None,
                        name: None,
                        ..Default::default()
                    };
                    history.push(text_preview);
                }
                if parts.is_empty() {
                    // Defensive: empty multimodal payload — fall
                    // through to a plain-text input so the agent
                    // sees the typed prompt at minimum.
                    AgentInput::text(prompt)
                } else {
                    AgentInput::multi_with_history(
                        std::mem::take(&mut history),
                        parts,
                    )
                }
            } else if prompt.is_empty() {
                AgentInput::text("")
            } else if history.is_empty() {
                AgentInput::text(prompt)
            } else {
                AgentInput::history(history, prompt)
            };

            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: invoking factory.run_stream"
            );
            let cancel = Arc::new(
                inner
                    .run_cancel
                    .lock()
                    .expect("run_cancel mutex poisoned")
                    .clone()
                    .expect("run_cancel token must be set before run starts"),
            );
            let mut stream = factory.run_stream(config, input, cancel);
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: factory returned; draining events"
            );
            let mut event_count = 0usize;
            while let Some(event) = stream.next().await {
                event_count += 1;
                if let Err(e) =
                    inner.persist_and_broadcast(&event, &mut log).await
                {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        event_kind = event.kind(),
                        error = %e,
                        "Failed to persist or broadcast event"
                    );
                }
            }
            drop(stream);
            // R6-B: the agent stream has ended, which means the
            // agent's spawned drive task already dropped its
            // `TypedEventSink` — every structural event it
            // published is sitting in the channel buffer. Drain
            // synchronously with `try_recv` (no spawned task, no
            // await on channel closure) so the Idle transition
            // can never deadlock. Each record is stamped with the
            // wall clock and the row's live seq — the ordinal the
            // reader will reproduce, which is what a compaction
            // checkpoint's `source_event_seqs` must cite.
            let mut typed_rx = typed_rx;
            while let Ok(Some(record)) = typed_rx.try_recv() {
                if let Err(e) = log.append_typed(record.as_value()).await {
                    tracing::warn!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist typed structural event"
                    );
                }
            }
            // R34: every row this run produced is now in the log, so
            // a compaction record that could not be mapped when it
            // was emitted gets its second chance. Records still
            // without provable provenance are dropped (log-only) by
            // the checkpoint; the surface stays foldable either way.
            let resolved = match checkpoint.as_ref() {
                Some(checkpoint) => checkpoint.resolve_pending(),
                None => Vec::new(),
            };
            for event in resolved {
                match serde_json::to_value(&event) {
                    Ok(value) => {
                        if let Err(e) = log.append_typed(value).await {
                            tracing::warn!(
                                target: "synthia.session",
                                session_id = %inner.session_id,
                                error = %e,
                                "Failed to persist a resolved compaction checkpoint"
                            );
                        }
                    }
                    Err(e) => tracing::warn!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to serialise a resolved compaction checkpoint"
                    ),
                }
            }
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                event_count,
                "Agent run task: factory stream ended"
            );
            let mut state = inner.state.lock().expect("state mutex poisoned");
            if *state == SessionState::Running {
                *state = SessionState::Idle;
            }
            // R11: publish the Completing snapshot.
            let _seq = inner.snapshot_bus.publish(
                synthia::session::OperationSnapshot::new(
                    inner.session_id.clone(),
                    "default".to_string(),
                    synthia::session::OperationState::Completing,
                    0,
                    inner.deps.lock().default_max_iterations,
                    synthia::provider::TokenUsage::default(),
                ),
            );
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: transitioned to Idle"
            );
        }))
    }
}
