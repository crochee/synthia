//! [`HookMap`] — typed, name-keyed hook registry (pi `HookMap` parity).
//!
//! R6-4 — typed `name → Vec<Handler<Name>>` registry that lets
//! lib consumers register named lifecycle hooks with typed event
//! payloads, rather than the positional
//! `Vec<Arc<dyn AgentHook>>` Synthia shipped through R3.
//!
//! ## Why alongside `Steering::hooks`?
//!
//! The positional `Vec<Arc<dyn AgentHook>>` registry is fine for
//! the 5-6 builtin hooks the agent loop has shipped with
//! (loop / context / tool / error / done). It does **not** scale
//! to the 13 named structural events dsh / pi expose
//! (`before_run` / `before_drive` / `transform_context` /
//! `before_request` / `before_payload` / `after_response` /
//! `before_tool` / `after_tool` / `before_compaction` /
//! `before_navigation` / …). Each of those has a *different*
//! event payload; typing them under a single
//! `async fn on_event(...)` would force every hook to pattern
//! match a sum-type.
//!
//! `HookMap` keeps the registry keyed by event name; the
//! handler type is parameterised by the event payload via the
//! [`HookEvent`] enum. A consumer that only needs
//! `before_tool` registers one typed `Handler<BeforeToolEvent>`;
//! a consumer that needs both `before_tool` and `after_tool`
//! registers two typed handlers.
//!
//! ## Migration
//!
//! The existing `Steering::hooks: Vec<Arc<dyn AgentHook>>`
//! stays, and it is what the agent loop reads: the loop walks
//! `Steering::hooks` / `guards` / `hints` directly and never
//! consults a `HookMap`. Build one with [`HookMap::new`] /
//! [`HookMap::on`] (or `on_with_state`) and dispatch through it
//! from your own code — `assemble_from_scratch` and
//! `assemble_with_provider_profile` show that shape — when you
//! want name-keyed, state-aware dispatch instead of one closure
//! per lifecycle point.

use std::{collections::HashMap, sync::Arc};

use synthia_context::AgentState;

/// are deliberately a closed enum: adding a new event forces
/// every existing handler to update its match, which is the
/// design goal (no silent payload drift).
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// `before_run` — fired once per `Agent::run` invocation,
    /// after the agent has resolved its descriptor but before
    /// the first iteration.
    BeforeRun {
        /// Agent descriptor name (e.g. `"default"`).
        agent_name: String,
        /// Run ordinal within the session.
        run_idx: u32,
    },
    /// `after_run` — fired once per `Agent::run` invocation,
    /// after the final `SessionEnded` event.
    AfterRun {
        agent_name: String,
        run_idx: u32,
        /// End reason string (e.g. `"completed"`,
        /// `"max_iterations"`, `"cancelled"`).
        reason: String,
    },
    /// `before_tool` — fired immediately before a tool dispatch
    /// (after the guard pipeline has accepted the call). A
    /// handler that returns `HookDecision::Block { reason }`
    /// vetoes the call.
    BeforeTool {
        tool_name: String,
        call_id: String,
        /// Raw arguments JSON for the call.
        arguments: serde_json::Value,
    },
    /// `after_tool` — fired once a tool has returned. A handler
    /// that returns `HookDecision::Terminate` ends the agent
    /// run with the supplied reason.
    AfterTool {
        tool_name: String,
        call_id: String,
        /// `true` when the tool reported an error result.
        is_error: bool,
    },
    /// `before_provider_call` — fired right before the LLM
    /// dispatch. Handlers can mutate the request before it
    /// goes on the wire (e.g. for red-team prompt rewriting
    /// in tests).
    BeforeProviderCall {
        provider: String,
        model: String,
        /// Token estimate of the request body.
        estimated_tokens: usize,
    },
    /// `after_provider_call` — fired once the streaming
    /// response has been fully assembled.
    AfterProviderCall {
        provider: String,
        model: String,
        /// Total usage reported by the provider.
        prompt_tokens: usize,
        completion_tokens: usize,
        /// Provider stop reason.
        stop_reason: Option<String>,
    },
    /// `on_error` — fired when the agent run terminates via
    /// `SessionEndReason::Error`. The reason string is the
    /// `Display` form of the error.
    OnError { agent_name: String, reason: String },
    /// `on_compaction` — fired when the
    /// `SummarizingContextManager` fires its emitter (R5-8).
    /// The `start` / `end` indices are the pre-splice range.
    OnCompaction {
        start: usize,
        end: usize,
        /// Summary text the manager produced.
        summary: String,
    },
}

/// Handler decision. A handler can veto a call (`Block`),
/// terminate the run (`Terminate`), or stay silent (`Allow`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HookDecision {
    /// The call proceeds unchanged.
    #[default]
    Allow,
    /// The call is vetoed; the agent loop synthesises an
    /// `is_error=true` tool result carrying the reason.
    Block {
        /// Human-readable reason; surfaces in the wire-visible
        /// tool result so the model can react.
        reason: String,
    },
    /// The agent run ends with the given reason. Distinct from
    /// `Block` (which only affects one tool call).
    Terminate {
        /// End reason string passed to `SessionEndReason`.
        reason: String,
    },
}

/// One handler registered for one [`HookEvent`] variant. The
/// handler is `Send + Sync` so the registry can be shared
/// across tasks; the closure returns a [`HookDecision`].
pub type HookHandler =
    Arc<dyn Fn(&HookEvent) -> HookDecision + Send + Sync + 'static>;

/// State-aware variant of [`HookHandler`]. Receives the live
/// [`AgentState`] (read-only) alongside the event so handlers
/// can make decisions off the runtime state, not just the
/// event payload.
///
/// Adopted from pi `packages/agent/src/harness/hooks.ts`
/// (handlers receive harness state). Use this when the
/// decision depends on counters (iteration index, tool-call
/// count, context utilisation, …); use the plain
/// [`HookHandler`] when the decision depends only on the
/// event payload.
pub type HookHandlerWithState = Arc<
    dyn for<'a> Fn(&'a HookEvent, &'a AgentState) -> HookDecision
        + Send
        + Sync
        + 'static,
>;
/// Typed, name-keyed hook registry. Construct one with
/// [`HookMap::new`], populate it via [`HookMap::on`], and
/// dispatch through it from your own code: the agent loop reads
/// [`Steering`](crate::Steering)'s positional hooks, so a map
/// is for callers that want name-keyed, state-aware dispatch.
///
/// The map is internally `HashMap<event_name, Vec<HookHandler>>`
/// keyed by the [`event_name`] of each [`HookEvent`]. A handler
/// registered for `BeforeTool` fires only for `BeforeTool`
/// events, never for `AfterTool` — payload type safety is
/// enforced by the variant, not by a string match.
///
/// The state-aware registry ([`HookMap::on_with_state`]) is
/// layered alongside the plain registry; both fire during a
/// single [`HookMap::dispatch`] / [`HookMap::dispatch_with_state`]
/// pass, in registration order. Plain handlers fire first so
/// pure-payload vetoes win before any state-dependent veto
/// runs.
#[derive(Default, Clone)]
pub struct HookMap {
    inner: HashMap<&'static str, Vec<HookHandler>>,
    inner_state: HashMap<&'static str, Vec<HookHandlerWithState>>,
}

impl HookMap {
    /// Build an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one handler for the [`HookEvent`] variant named
    /// by `name`. The handler is appended to the existing list
    /// (registration order = invocation order). Returns `self`
    /// for fluent chaining.
    ///
    /// # Panics
    ///
    /// The function does not panic; it is a pure builder.
    pub fn on<F>(mut self, name: &'static str, handler: F) -> Self
    where
        F: Fn(&HookEvent) -> HookDecision + Send + Sync + 'static,
    {
        self.inner.entry(name).or_default().push(Arc::new(handler));
        self
    }

    /// Register one **state-aware** handler. Receives the live
    /// [`AgentState`] alongside the event. Use when the
    /// decision depends on counters (iteration index, tool-call
    /// count, context utilisation, …).
    pub fn on_with_state<F>(mut self, name: &'static str, handler: F) -> Self
    where
        F: for<'a> Fn(&'a HookEvent, &'a AgentState) -> HookDecision
            + Send
            + Sync
            + 'static,
    {
        self.inner_state
            .entry(name)
            .or_default()
            .push(Arc::new(handler));
        self
    }

    /// True when no handlers are registered (counts both
    /// plain and state-aware handlers).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.values().all(Vec::is_empty)
            && self.inner_state.values().all(Vec::is_empty)
    }

    /// Number of distinct event names with at least one handler
    /// (counts both plain and state-aware handlers).
    #[must_use]
    pub fn len(&self) -> usize {
        let plain = self
            .inner
            .values()
            .filter(|handlers| !handlers.is_empty())
            .count();
        let state = self
            .inner_state
            .values()
            .filter(|handlers| !handlers.is_empty())
            .count();
        plain + state
    }

    /// Fire one event. Every handler registered for the
    /// event's [`event_name`] runs in registration order. The
    /// first non-`Allow` decision short-circuits the rest and
    /// is returned; if every handler returns `Allow` the
    /// function returns `Allow`.
    ///
    /// Handler panics are caught and treated as `Allow` (the
    /// registry never poisons the agent run on a user code
    /// bug; the `tracing` layer logs the panic).
    pub fn dispatch(&self, event: &HookEvent) -> HookDecision {
        let name = event_name(event);
        let Some(handlers) = self.inner.get(name) else {
            return HookDecision::Allow;
        };
        for handler in handlers {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler(event)
                }));
            match result {
                Ok(decision) if decision != HookDecision::Allow => {
                    return decision;
                }
                Ok(_) => continue,
                Err(_) => {
                    // Handler panicked — swallow + log so the
                    // agent run continues.
                    tracing::warn!(
                        event = name,
                        "HookMap handler panicked; treating as Allow"
                    );
                    continue;
                }
            }
        }
        HookDecision::Allow
    }

    /// Fire one event with the live [`AgentState`]. Plain
    /// handlers fire first; state-aware handlers fire next.
    /// The first non-`Allow` decision short-circuits the rest
    /// and is returned.
    ///
    /// Handler panics are caught and treated as `Allow` (the
    /// registry never poisons the agent run on a user code
    /// bug; the `tracing` layer logs the panic).
    pub fn dispatch_with_state(
        &self,
        event: &HookEvent,
        state: &AgentState,
    ) -> HookDecision {
        // 1. Plain handlers first (pure-payload vetoes win).
        let plain = self.dispatch(event);
        if plain != HookDecision::Allow {
            return plain;
        }
        // 2. State-aware handlers next.
        let name = event_name(event);
        let Some(handlers) = self.inner_state.get(name) else {
            return HookDecision::Allow;
        };
        for handler in handlers {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handler(event, state)
                }));
            match result {
                Ok(decision) if decision != HookDecision::Allow => {
                    return decision;
                }
                Ok(_) => continue,
                Err(_) => {
                    tracing::warn!(
                        event = name,
                        "HookMap state-aware handler panicked; treating as Allow"
                    );
                    continue;
                }
            }
        }
        HookDecision::Allow
    }
}

/// Wire tag for a [`HookEvent`] variant. Mirrors the variant
/// name in `lower_snake_case` so the registry key matches the
/// event name a lib consumer registers.
#[must_use]
pub fn event_name(event: &HookEvent) -> &'static str {
    match event {
        HookEvent::BeforeRun { .. } => "before_run",
        HookEvent::AfterRun { .. } => "after_run",
        HookEvent::BeforeTool { .. } => "before_tool",
        HookEvent::AfterTool { .. } => "after_tool",
        HookEvent::BeforeProviderCall { .. } => "before_provider_call",
        HookEvent::AfterProviderCall { .. } => "after_provider_call",
        HookEvent::OnError { .. } => "on_error",
        HookEvent::OnCompaction { .. } => "on_compaction",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_map_dispatches_allow() {
        let map = HookMap::new();
        let decision = map.dispatch(&HookEvent::BeforeRun {
            agent_name: "x".to_string(),
            run_idx: 0,
        });
        assert_eq!(decision, HookDecision::Allow);
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn before_tool_handler_vetoes_dispatch() {
        let map = HookMap::new().on("before_tool", |event| match event {
            HookEvent::BeforeTool { tool_name, .. } if tool_name == "shell" => {
                HookDecision::Block {
                    reason: "shell vetoed by test".to_string(),
                }
            }
            _ => HookDecision::Allow,
        });
        let decision = map.dispatch(&HookEvent::BeforeTool {
            tool_name: "shell".to_string(),
            call_id: "c1".to_string(),
            arguments: json!({}),
        });
        assert_eq!(
            decision,
            HookDecision::Block {
                reason: "shell vetoed by test".to_string(),
            }
        );
    }

    #[test]
    fn handler_runs_only_for_matching_event_name() {
        // A handler registered for `before_tool` must NOT fire
        // on `after_tool`. Pin the variant isolation.
        let map = HookMap::new().on("before_tool", |_| HookDecision::Block {
            reason: "should not fire here".to_string(),
        });
        let decision = map.dispatch(&HookEvent::AfterTool {
            tool_name: "shell".to_string(),
            call_id: "c1".to_string(),
            is_error: false,
        });
        assert_eq!(decision, HookDecision::Allow);
    }

    #[test]
    fn first_non_allow_decision_wins() {
        // Two handlers; the first returns Allow, the second
        // returns Terminate. The result is Terminate — first
        // non-allow short-circuits.
        let map = HookMap::new().on("after_tool", |_| HookDecision::Allow).on(
            "after_tool",
            |_| HookDecision::Terminate {
                reason: "second wins".to_string(),
            },
        );
        let decision = map.dispatch(&HookEvent::AfterTool {
            tool_name: "t".to_string(),
            call_id: "c".to_string(),
            is_error: false,
        });
        assert_eq!(
            decision,
            HookDecision::Terminate {
                reason: "second wins".to_string(),
            }
        );
    }

    #[test]
    fn handler_panic_is_treated_as_allow() {
        let map = HookMap::new().on("before_run", |_| {
            panic!("handler bug");
        });
        let decision = map.dispatch(&HookEvent::BeforeRun {
            agent_name: "x".to_string(),
            run_idx: 0,
        });
        assert_eq!(decision, HookDecision::Allow);
    }

    #[test]
    fn multiple_handlers_for_distinct_events_coexist() {
        let map = HookMap::new()
            .on("before_tool", |event| match event {
                HookEvent::BeforeTool { tool_name, .. }
                    if tool_name == "read" =>
                {
                    HookDecision::Block {
                        reason: "no reads".to_string(),
                    }
                }
                _ => HookDecision::Allow,
            })
            .on("after_tool", |event| match event {
                HookEvent::AfterTool { is_error, .. } if *is_error => {
                    HookDecision::Terminate {
                        reason: "tool error".to_string(),
                    }
                }
                _ => HookDecision::Allow,
            });
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());

        let read = map.dispatch(&HookEvent::BeforeTool {
            tool_name: "read".to_string(),
            call_id: "c".to_string(),
            arguments: json!({}),
        });
        assert_eq!(
            read,
            HookDecision::Block {
                reason: "no reads".to_string(),
            }
        );

        let errored = map.dispatch(&HookEvent::AfterTool {
            tool_name: "t".to_string(),
            call_id: "c".to_string(),
            is_error: true,
        });
        assert_eq!(
            errored,
            HookDecision::Terminate {
                reason: "tool error".to_string(),
            }
        );

        // Unrelated event name — no handler registered.
        let unrelated = map.dispatch(&HookEvent::OnError {
            agent_name: "x".to_string(),
            reason: "y".to_string(),
        });
        assert_eq!(unrelated, HookDecision::Allow);
    }

    #[test]
    fn event_name_matches_variant() {
        assert_eq!(
            event_name(&HookEvent::BeforeRun {
                agent_name: "a".to_string(),
                run_idx: 0,
            }),
            "before_run"
        );
        assert_eq!(
            event_name(&HookEvent::AfterRun {
                agent_name: "a".to_string(),
                run_idx: 0,
                reason: "x".to_string(),
            }),
            "after_run"
        );
        assert_eq!(
            event_name(&HookEvent::BeforeTool {
                tool_name: "t".to_string(),
                call_id: "c".to_string(),
                arguments: json!({}),
            }),
            "before_tool"
        );
        assert_eq!(
            event_name(&HookEvent::AfterTool {
                tool_name: "t".to_string(),
                call_id: "c".to_string(),
                is_error: false,
            }),
            "after_tool"
        );
        assert_eq!(
            event_name(&HookEvent::OnError {
                agent_name: "a".to_string(),
                reason: "x".to_string(),
            }),
            "on_error"
        );
        assert_eq!(
            event_name(&HookEvent::OnCompaction {
                start: 0,
                end: 1,
                summary: "x".to_string(),
            }),
            "on_compaction"
        );
    }

    #[test]
    fn on_registration_preserves_order() {
        // Three handlers in registration order; the third
        // returns Terminate. The dispatch must reach the third
        // (no early short-circuit on Allow).
        let map = HookMap::new()
            .on("after_run", |_| HookDecision::Allow)
            .on("after_run", |_| HookDecision::Allow)
            .on("after_run", |_| HookDecision::Terminate {
                reason: "third".to_string(),
            });
        let decision = map.dispatch(&HookEvent::AfterRun {
            agent_name: "x".to_string(),
            run_idx: 0,
            reason: "y".to_string(),
        });
        assert_eq!(
            decision,
            HookDecision::Terminate {
                reason: "third".to_string(),
            }
        );
    }

    #[test]
    fn state_aware_handler_receives_agent_state() {
        let map =
            HookMap::new().on_with_state("before_run", |_event, state| {
                if state.iteration_count >= 3 {
                    HookDecision::Block {
                        reason: "too many iterations".to_string(),
                    }
                } else {
                    HookDecision::Allow
                }
            });
        let mut state = AgentState::with_window(100);
        state.iteration_count = 0;
        assert_eq!(
            map.dispatch_with_state(
                &HookEvent::BeforeRun {
                    agent_name: "x".to_string(),
                    run_idx: 0
                },
                &state
            ),
            HookDecision::Allow
        );
        state.iteration_count = 3;
        let decision = map.dispatch_with_state(
            &HookEvent::BeforeRun {
                agent_name: "x".to_string(),
                run_idx: 0,
            },
            &state,
        );
        assert!(matches!(decision, HookDecision::Block { .. }));
    }

    #[test]
    fn state_aware_handler_panic_is_swallowed() {
        let map = HookMap::new().on_with_state("before_run", |_, _| {
            panic!("intentional panic for test");
        });
        let state = AgentState::with_window(100);
        // Panicking handler must not poison dispatch — the
        let decision = map.dispatch_with_state(
            &HookEvent::BeforeRun {
                agent_name: "x".to_string(),
                run_idx: 0,
            },
            &state,
        );
        assert_eq!(decision, HookDecision::Allow);
    }

    #[test]
    fn plain_veto_wins_before_state_aware_check() {
        let map = HookMap::new()
            .on("before_run", |_| HookDecision::Terminate {
                reason: "plain".to_string(),
            })
            .on_with_state("before_run", |_, _| HookDecision::Allow);
        let state = AgentState::with_window(100);
        let decision = map.dispatch_with_state(
            &HookEvent::BeforeRun {
                agent_name: "x".to_string(),
                run_idx: 0,
            },
            &state,
        );
        // Plain handler vetoes before state-aware handlers even
        // run.
        assert!(matches!(decision, HookDecision::Terminate { .. }));
    }

    #[test]
    fn on_with_state_does_not_affect_plain_dispatch() {
        let map = HookMap::new().on_with_state("before_run", |_, _| {
            HookDecision::Terminate {
                reason: "state-aware".to_string(),
            }
        });
        // Plain dispatch ignores state-aware handlers entirely.
        let decision = map.dispatch(&HookEvent::BeforeRun {
            agent_name: "x".to_string(),
            run_idx: 0,
        });
        assert_eq!(decision, HookDecision::Allow);
    }
}
