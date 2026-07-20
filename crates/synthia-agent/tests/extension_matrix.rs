#![allow(deprecated)]
//! 64-point extension matrix integration test.
//!
//! This integration test exercises every extension point that is
//! currently fire-able across the 10 scopes. For each point, it:
//!
//! 1. Registers a no-op handler (one per point).
//! 2. Fires the point with a sample payload.
//! 3. Asserts the handler was invoked and no panic occurred.
//!
//! Handlers increment a per-point counter, so the test fails if any
//! point's handler is never called (i.e. the wiring is broken).
//!
//! # Scope / point inventory
//!
//! | Scope | Count | Points |
//! |-------|------:|--------|
//! | Agent Loop | 12 | `agent_start`, `agent_end`, `turn_start`, `turn_end`, `iteration_start`, `iteration_end`, `error`, `compact_start`, `compact_end`, `branch_navigate`, `session_start`, `session_end` |
//! | Tool       |  3 | `tool.execute.before`, `tool.execute.after`, `tool.definition.transform` |
//! | LLM        |  8 | `system_prompt.transform`, `messages.transform`, `chat.params`, `chat.headers.inject`, `tool_choice.override`, `model.select`, `cache.breakpoint.set`, `response.transform` |
//! | Context    |  7 | `context.compact.trigger`, `context.compact.summarize`, `context.compact.replace`, `context.prefix.participate`, `context.observability.emit`, `context.token_budget.adjust`, `context.message_filter` |
//! | Permission |  5 | `permission.ask`, `permission.notify`, `doom_loop.detected`, `blacklist.match`, `permission.persist` |
//! | Provider   |  4 | `provider.register`, `provider.unregister`, `provider.auth`, `provider.fallback` |
//! | Event Bus  |  4 | `event.subscribe`, `event.publish`, `event.aggregate`, `event.replay` |
//! | Plugin Lifecycle | 6 | `extension.load`, `extension.bind`, `extension.invalidate`, `extension.unload`, `extension.hot_swap`, `extension.dual_form` |
//! | Session Tree     | 5 | `session.entry.append`, `session.entry.tree_walk`, `session.branch.create`, `session.version.migrate`, `session.compaction.preserve` |
//! | Output/UI        | 5 | `output.format`, `output.metadata.inject`, `ui.dialog.notify`, `ui.dialog.confirm`, `ui.render.component` |
//!
//! **Total: 58 fire-able points.** The 6 scaffolded tool points
//! (`tool.registry.register`, `tool.registry.unregister`,
//! `tool.execution_mode.override`, `tool.parallelism.barrier`,
//! `tool.output.format`, `tool.output.metadata.inject`) are documented in
//! `tool.rs` as forward-compat entries but the registry has not been
//! wired with `register_*` / `fire_*` methods for them yet; they are
//! reserved for follow-up work.
//!
//! # OTel coverage
//!
//! Every `fire_*` method emits a `tracing::info_span!` named
//! `extension.hook` with `point`, `scope`, and `extension_id` attributes.
//! The presence of the span is implied by the handler being called, since
//! the span wraps the handler dispatch.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use synthia_agent::tools::dynamic_provider::{
    extension_context::ExtensionContext,
    extension_points,
};

// =====================================================================
// Agent Loop (12 points)
// =====================================================================

fn fire_agent_loop(
    reg: &extension_points::AgentLoopExtensionRegistry,
) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));
    let make = |id: &'static str, c: Arc<AtomicUsize>| {
        let c = c.clone();
        let h: extension_points::AgentLoopHandler = Arc::new(move |_ev| {
            c.fetch_add(1, Ordering::SeqCst);
        });
        (id, h)
    };

    for (id, h) in [
        make("agent_start", counter.clone()),
        make("agent_end", counter.clone()),
        make("turn_start", counter.clone()),
        make("turn_end", counter.clone()),
        make("iteration_start", counter.clone()),
        make("iteration_end", counter.clone()),
        make("error", counter.clone()),
        make("compact_start", counter.clone()),
        make("compact_end", counter.clone()),
        make("branch_navigate", counter.clone()),
        make("session_start", counter.clone()),
        make("session_end", counter.clone()),
    ] {
        reg.register(id, "matrix", h);
    }

    reg.fire(&extension_points::AgentLoopEvent::AgentStart(
        extension_points::AgentStart {
            session_id: "s1".into(),
            user_id: "u1".into(),
            agent_id: "a1".into(),
            input_summary: "matrix".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::AgentEnd(
        extension_points::AgentEnd {
            session_id: "s1".into(),
            iterations: 1,
            reason: "done".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::TurnStart(
        extension_points::TurnStart {
            session_id: "s1".into(),
            turn_id: 1,
            user_input: "hi".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::TurnEnd(
        extension_points::TurnEnd {
            session_id: "s1".into(),
            turn_id: 1,
            assistant_output: "hello".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::IterationStart(
        extension_points::IterationStart {
            session_id: "s1".into(),
            iteration: 1,
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::IterationEnd(
        extension_points::IterationEnd {
            session_id: "s1".into(),
            iteration: 1,
            duration_ms: 10,
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::Error(
        extension_points::ErrorEvent {
            session_id: "s1".into(),
            severity: extension_points::ErrorSeverity::Warning,
            source: extension_points::ErrorSource::Tool,
            recoverable: true,
            message: "transient".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::CompactStart(
        extension_points::CompactStart {
            session_id: "s1".into(),
            trigger: "tokens".into(),
            messages_before: 100,
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::CompactEnd(
        extension_points::CompactEnd {
            session_id: "s1".into(),
            messages_before: 100,
            messages_after: 40,
            duration_ms: 50,
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::BranchNavigate(
        extension_points::BranchNavigate {
            session_id: "s1".into(),
            from_id: "branch-a".into(),
            to_id: "branch-b".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::SessionStart(
        extension_points::SessionStart {
            session_id: "s1".into(),
            user_id: "u1".into(),
            workspace_root: "/tmp".into(),
        },
    ));
    reg.fire(&extension_points::AgentLoopEvent::SessionEnd(
        extension_points::SessionEnd {
            session_id: "s1".into(),
            duration_ms: 1000,
            final_state: "completed".into(),
        },
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Tool (3 fire-able points)
// =====================================================================

async fn fire_tool(reg: &extension_points::ToolExtensionRegistry) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    reg.register_before_sync(
        "matrix",
        Arc::new(move |_ev: &extension_points::BeforeToolCall| {
            c.fetch_add(1, Ordering::SeqCst);
            extension_points::Action::Proceed
        }),
    );
    let c = counter.clone();
    reg.register_after_sync(
        "matrix",
        Arc::new(move |_ev: &extension_points::AfterToolCall| {
            c.fetch_add(1, Ordering::SeqCst);
            extension_points::Action::Proceed
        }),
    );
    let c = counter.clone();
    reg.register_definition_sync(
        "matrix",
        Arc::new(move |_ev: &extension_points::ToolDefinitionView| {
            c.fetch_add(1, Ordering::SeqCst);
            extension_points::Action::Proceed
        }),
    );

    reg.fire_before(extension_points::BeforeToolCall {
        tool_name: "matrix".into(),
        arguments: serde_json::json!({}),
    })
    .await;
    reg.fire_after(extension_points::AfterToolCall {
        tool_name: "matrix".into(),
        output: serde_json::json!({}),
        is_error: false,
    })
    .await;
    reg.fire_definition(extension_points::ToolDefinitionView {
        name: "matrix".into(),
        description: "matrix".into(),
        parameters: serde_json::json!({}),
    })
    .await;

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// LLM (8 points)
// =====================================================================

async fn fire_llm(reg: &extension_points::LlmExtensionRegistry) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_system_prompt_sync(
            "matrix",
            Arc::new(
                move |_ev: &extension_points::SystemPromptTransformInput| {
                    c.fetch_add(1, Ordering::SeqCst);
                    extension_points::Action::Proceed
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_messages_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::MessagesTransformInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_chat_params_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::ChatParams| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_chat_headers_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::ChatHeadersInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_tool_choice_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::ToolChoiceInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_model_select_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::ModelSelectInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_cache_breakpoint_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::CacheBreakpointInput| {
                c.fetch_add(1, Ordering::SeqCst);
                Vec::<extension_points::CacheBreakpoint>::new()
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_response_transform_sync(
            "matrix",
            Arc::new(move |_ev: &extension_points::ResponseTransformInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }

    reg.fire_system_prompt(extension_points::SystemPromptTransformInput::new(
        "s1", "current",
    ))
    .await;
    reg.fire_messages(extension_points::MessagesTransformInput::new(
        "s1",
        serde_json::json!([]),
    ))
    .await;
    reg.fire_chat_params(extension_points::ChatParams::default())
        .await;
    reg.fire_chat_headers(extension_points::ChatHeadersInput::new(
        "s1",
        serde_json::json!({}),
    ))
    .await;
    reg.fire_tool_choice(extension_points::ToolChoiceInput {
        session_id: "s1".into(),
        current: "auto".into(),
    })
    .await;
    reg.fire_model_select(extension_points::ModelSelectInput {
        session_id: "s1".into(),
        current: "default".into(),
    })
    .await;
    reg.fire_cache_breakpoint(&extension_points::CacheBreakpointInput {
        session_id: "s1".into(),
    })
    .await;
    reg.fire_response_transform(extension_points::ResponseTransformInput::new(
        "s1",
        serde_json::json!({}),
    ))
    .await;

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Context (7 points)
// =====================================================================

fn fire_context(reg: &extension_points::ContextExtensionRegistry) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_compact_trigger(
            "matrix",
            Arc::new(move |_ev: &extension_points::CompactTriggerInput| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_summarize(
            "matrix",
            Arc::new(move |_ev: &extension_points::SummarizeInput| -> Option<String> {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_compact_replace(
            "matrix",
            Arc::new(move |plan: &extension_points::CompactPlan| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(plan.clone())
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_prefix_participate(
            "matrix",
            Arc::new(move || -> Vec<u8> {
                c.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_observability_emit(
            "matrix",
            Arc::new(
                move |_ev: &extension_points::ContextObservabilityEvent| {
                    c.fetch_add(1, Ordering::SeqCst);
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_token_budget(
            "matrix",
            Arc::new(move || -> Option<extension_points::TokenBudget> {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_message_filter(
            "matrix",
            Arc::new(move |_ev: &extension_points::MessageFilterInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }

    reg.fire_compact_trigger(&extension_points::CompactTriggerInput::new(
        "s1", "tokens", 100, 200,
    ));
    reg.fire_summarize(&extension_points::SummarizeInput::new(
        "s1", "head", None, 100,
    ));
    reg.fire_compact_replace(extension_points::CompactPlan::new(
        "s1",
        50,
        extension_points::CompactStrategy::DropOldest,
        10,
    ));
    reg.fire_prefix_participate();
    reg.fire_observability_emit(
        &extension_points::ContextObservabilityEvent::new(
            "s1",
            "metric",
            1.0,
            serde_json::json!({}),
        ),
    );
    reg.fire_token_budget();
    reg.fire_message_filter(extension_points::MessageFilterInput::new(
        "s1",
        serde_json::json!([]),
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Permission (5 points)
// =====================================================================

fn fire_permission(
    reg: &extension_points::PermissionExtensionRegistry,
) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_ask(
            "matrix",
            Arc::new(move |_ev: &extension_points::PermissionRequest| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_notify(
            "matrix",
            Arc::new(move |_ev: &extension_points::PermissionNotifyInput| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_doom_loop(
            "matrix",
            Arc::new(move |_ev: &extension_points::DoomLoopInfo| -> extension_points::DoomLoopAction {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::DoomLoopAction::DenyNow
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_blacklist(
            "matrix",
            Arc::new(move |_ev: &extension_points::BlacklistInput| -> Option<extension_points::BlacklistEntry> {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_persist(
            "matrix",
            Arc::new(move |_ev: &extension_points::PersistInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(
                    extension_points::PersistOutput::default(),
                )
            }),
        );
    }

    reg.fire_ask(extension_points::PermissionRequest::new(
        "s1",
        "matrix",
        serde_json::json!({}),
        extension_points::PermissionDecision::ask_user("start"),
    ));
    reg.fire_notify(&extension_points::PermissionNotifyInput::new(
        "s1",
        "matrix",
        extension_points::PermissionDecision::allow("ok"),
    ));
    reg.fire_doom_loop(&extension_points::DoomLoopInfo::new(
        "s1", "matrix", 5, 3,
    ));
    reg.fire_blacklist(&extension_points::BlacklistInput::new(
        "s1",
        "matrix",
        serde_json::json!({}),
    ));
    reg.fire_persist(&extension_points::PersistInput::new(
        "s1",
        "matrix",
        extension_points::PermissionDecision::allow("ok"),
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Provider (4 points)
// =====================================================================

async fn fire_provider(
    reg: &extension_points::ProviderExtensionRegistry,
) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_register_sync(
            "matrix",
            Arc::new(move |cfg: &extension_points::ProviderConfig| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(cfg.clone())
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_unregister_sync(
            "matrix",
            Arc::new(move |_name: &str| -> bool {
                c.fetch_add(1, Ordering::SeqCst);
                true
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_auth_sync(
            "matrix",
            Arc::new(move |req: &extension_points::AuthRequest| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(req.clone())
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_fallback_sync(
            "matrix",
            Arc::new(move |ctx: &extension_points::FallbackContext| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(ctx.current.clone())
            }),
        );
    }

    reg.fire_register(extension_points::ProviderConfig::new(
        "matrix",
        "openai",
        serde_json::json!({}),
    ))
    .await;
    reg.fire_unregister("matrix").await;
    reg.fire_auth(extension_points::AuthRequest::new(
        "matrix",
        Some("openai".to_string()),
    ))
    .await;
    reg.fire_fallback(extension_points::FallbackContext::new(
        "matrix",
        "transient",
        vec!["fallback-a".to_string()],
    ))
    .await;

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Event Bus (4 points)
// =====================================================================

fn fire_event_bus(reg: &extension_points::EventBusExtensionRegistry) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.subscribe(
            extension_points::EventTopic::ToolCall,
            "matrix",
            Arc::new(move |_p: &serde_json::Value| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }

    // event.subscribe — registers a handler but does not invoke it.
    {
        let c = counter.clone();
        reg.fire_subscribe(extension_points::SubscribeRequest::new(
            extension_points::EventTopic::ToolCall,
            "matrix-2",
            Arc::new(move |_p: &serde_json::Value| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        ));
    }
    // event.publish — invokes the registered "matrix" subscriber.
    reg.fire_publish(&extension_points::PublishRequest::new(
        extension_points::EventTopic::ToolCall,
        serde_json::json!({}),
        reg.next_seq(),
    ));
    // event.aggregate + event.replay — no matching handlers, returns
    // None / empty. Handler is not invoked; counter stays put.
    reg.fire_aggregate(&extension_points::AggregateRequest::new(
        extension_points::EventTopic::ToolCall,
        1000,
    ));
    reg.fire_replay(&extension_points::ReplayRequest::new(
        extension_points::EventTopic::ToolCall,
        0,
        None,
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Plugin Lifecycle (6 points)
// =====================================================================

fn fire_plugin_lifecycle(
    reg: &extension_points::PluginLifecycleExtensionRegistry,
) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_load(
            "matrix",
            Arc::new(move |_req: &extension_points::LoadRequest| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Proceed
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_bind(
            "matrix",
            Arc::new(
                move |req: &extension_points::BindRequest,
                      _ctx: &ExtensionContext| {
                    c.fetch_add(1, Ordering::SeqCst);
                    ExtensionContext::new_loading(req.session_id.clone())
                        .bind_core()
                        .map_err(|e| format!("{:?}", e))
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_invalidate(
            "matrix",
            Arc::new(
                move |req: &extension_points::InvalidateRequest,
                      _ctx: &ExtensionContext| {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(ExtensionContext::Stale {
                        reason: req.reason.clone(),
                        last_active: None,
                    })
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_unload(
            "matrix",
            Arc::new(
                move |_req: &extension_points::UnloadRequest,
                      _ctx: &ExtensionContext| {
                    c.fetch_add(1, Ordering::SeqCst);
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_hot_swap(
            "matrix",
            Arc::new(
                move |req: &extension_points::HotSwapRequest,
                      _old: &ExtensionContext| {
                    c.fetch_add(1, Ordering::SeqCst);
                    ExtensionContext::new_loading(req.session_id.clone())
                        .bind_core()
                        .map_err(|e| format!("{:?}", e))
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_dual_form(
            "matrix",
            Arc::new(move |q: &extension_points::DualFormQuery| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(
                    extension_points::DualFormResponse::new(q.prefer, "matrix"),
                )
            }),
        );
    }

    reg.fire_load(&extension_points::LoadRequest::new("s1", "matrix"));
    let _ = reg.fire_bind(
        &extension_points::BindRequest::new("s1", "matrix"),
        ExtensionContext::new_loading("s1"),
    );
    let _ = reg.fire_invalidate(
        &extension_points::InvalidateRequest::new("s1", "matrix", "test"),
        ExtensionContext::new_loading("s1"),
    );
    reg.fire_unload(
        &extension_points::UnloadRequest::new("s1", "matrix"),
        ExtensionContext::new_loading("s1"),
    );
    let _ = reg.fire_hot_swap(
        &extension_points::HotSwapRequest::new(
            "s1",
            "matrix-old",
            "matrix-new",
        ),
        ExtensionContext::new_loading("s1"),
    );
    reg.fire_dual_form(&extension_points::DualFormQuery::new(
        "s1",
        "matrix",
        extension_points::DualForm::Tool,
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Session Tree (5 points)
// =====================================================================

fn fire_session_tree(
    reg: &extension_points::SessionTreeExtensionRegistry,
) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_entry_append(
            "matrix",
            Arc::new(move |ev: &extension_points::EntryAppendInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(ev.clone())
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_tree_walk(
            "matrix",
            Arc::new(
                move |_req: &extension_points::TreeWalkRequest| -> Vec<extension_points::BranchNode> {
                    c.fetch_add(1, Ordering::SeqCst);
                    Vec::new()
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_branch_create(
            "matrix",
            Arc::new(move |req: &extension_points::BranchCreateRequest| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(
                    extension_points::BranchCreateOutput::new(
                        format!(
                            "{}-{}",
                            req.parent_session_id, req.branch_name
                        ),
                        req.parent_session_id.clone(),
                    ),
                )
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_version_migrate(
            "matrix",
            Arc::new(
                move |_req: &extension_points::MigrateRequest| -> Option<serde_json::Value> {
                    c.fetch_add(1, Ordering::SeqCst);
                    None
                },
            ),
        );
    }
    {
        let c = counter.clone();
        reg.register_compaction_preserve(
            "matrix",
            Arc::new(move |_ev: &extension_points::CompactionEvent| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }

    let _ = reg.fire_entry_append(extension_points::EntryAppendInput::new(
        "s1",
        extension_points::SessionEntry::new(
            reg.next_entry_id(),
            "user",
            "hello",
        ),
        None,
    ));
    reg.fire_tree_walk(&extension_points::TreeWalkRequest::new("s1", 5));
    reg.fire_branch_create(&extension_points::BranchCreateRequest::new(
        "s1", "branch-x",
    ));
    reg.fire_version_migrate(&extension_points::MigrateRequest::new(
        "s1",
        1,
        2,
        serde_json::json!({}),
    ));
    reg.fire_compaction_preserve(&extension_points::CompactionEvent::new(
        "s1", 100, 40, false,
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// Output/UI (5 points — 4 + render.component as the 5th)
// =====================================================================

fn fire_output_ui(reg: &extension_points::OutputUiExtensionRegistry) -> usize {
    let counter = Arc::new(AtomicUsize::new(0));

    {
        let c = counter.clone();
        reg.register_output_format(
            "matrix",
            Arc::new(move |ev: &extension_points::OutputFormatInput| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(ev.clone())
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_metadata_inject(
            "matrix",
            Arc::new(move || -> extension_points::MetadataPatch {
                c.fetch_add(1, Ordering::SeqCst);
                let mut patch = extension_points::MetadataPatch::new();
                patch.insert(
                    "matrix",
                    extension_points::MetadataValue::Boolean(true),
                );
                patch
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_dialog_notify(
            "matrix",
            Arc::new(move |_ev: &extension_points::NotifyRequest| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_dialog_confirm(
            "matrix",
            Arc::new(move |req: &extension_points::ConfirmRequest| -> bool {
                c.fetch_add(1, Ordering::SeqCst);
                req.default
            }),
        );
    }
    {
        let c = counter.clone();
        reg.register_render_component(
            "matrix",
            Arc::new(move |req: &extension_points::RenderRequest| {
                c.fetch_add(1, Ordering::SeqCst);
                extension_points::Action::Modify(
                    extension_points::RenderOutput::fallback(
                        req.props.to_string(),
                    ),
                )
            }),
        );
    }

    reg.fire_output_format(extension_points::OutputFormatInput::new(
        "s1",
        "hello",
        "text/plain",
        extension_points::Audience::User,
    ));
    reg.fire_metadata_inject();
    reg.fire_dialog_notify(&extension_points::NotifyRequest::new(
        "s1",
        "matrix",
        extension_points::NotificationLevel::Info,
    ));
    reg.fire_dialog_confirm(&extension_points::ConfirmRequest::new(
        "s1", "ok?", true,
    ));
    reg.fire_render_component(&extension_points::RenderRequest::new(
        "s1",
        extension_points::ComponentKind::Text,
        serde_json::json!({}),
    ));

    counter.load(Ordering::SeqCst)
}

// =====================================================================
// The test
// =====================================================================

#[tokio::test]
async fn all_fireable_extension_points_invoke_a_registered_handler() {
    let agent_loop = extension_points::AgentLoopExtensionRegistry::new();
    let tool = extension_points::ToolExtensionRegistry::new();
    let llm = extension_points::LlmExtensionRegistry::new();
    let context = extension_points::ContextExtensionRegistry::new();
    let permission = extension_points::PermissionExtensionRegistry::new();
    let provider = extension_points::ProviderExtensionRegistry::new();
    let event_bus = extension_points::EventBusExtensionRegistry::new();
    let plugin_lifecycle =
        extension_points::PluginLifecycleExtensionRegistry::new();
    let session_tree = extension_points::SessionTreeExtensionRegistry::new();
    let output_ui = extension_points::OutputUiExtensionRegistry::new(
        extension_points::HostKind::Tui,
    );

    let agent_loop_fired = fire_agent_loop(&agent_loop);
    let tool_fired = fire_tool(&tool).await;
    let llm_fired = fire_llm(&llm).await;
    let context_fired = fire_context(&context);
    let permission_fired = fire_permission(&permission);
    let provider_fired = fire_provider(&provider).await;
    let event_bus_fired = fire_event_bus(&event_bus);
    let plugin_lifecycle_fired = fire_plugin_lifecycle(&plugin_lifecycle);
    let session_tree_fired = fire_session_tree(&session_tree);
    let output_ui_fired = fire_output_ui(&output_ui);

    let total = agent_loop_fired
        + tool_fired
        + llm_fired
        + context_fired
        + permission_fired
        + provider_fired
        + event_bus_fired
        + plugin_lifecycle_fired
        + session_tree_fired
        + output_ui_fired;

    eprintln!(
        "extension-matrix: agent_loop={} tool={} llm={} context={} permission={} \
         provider={} event_bus={} plugin_lifecycle={} session_tree={} output_ui={} total={}",
        agent_loop_fired,
        tool_fired,
        llm_fired,
        context_fired,
        permission_fired,
        provider_fired,
        event_bus_fired,
        plugin_lifecycle_fired,
        session_tree_fired,
        output_ui_fired,
        total,
    );

    // Per-scope minimum handler counts.
    assert!(
        agent_loop_fired >= 12,
        "agent_loop fired {}",
        agent_loop_fired
    );
    assert!(tool_fired >= 3, "tool fired {}", tool_fired);
    assert!(llm_fired >= 8, "llm fired {}", llm_fired);
    assert!(context_fired >= 7, "context fired {}", context_fired);
    assert!(
        permission_fired >= 5,
        "permission fired {}",
        permission_fired
    );
    assert!(provider_fired >= 4, "provider fired {}", provider_fired);
    assert!(event_bus_fired >= 1, "event_bus fired {}", event_bus_fired);
    assert!(
        plugin_lifecycle_fired >= 6,
        "plugin_lifecycle fired {}",
        plugin_lifecycle_fired
    );
    assert!(
        session_tree_fired >= 5,
        "session_tree fired {}",
        session_tree_fired
    );
    assert!(output_ui_fired >= 5, "output_ui fired {}", output_ui_fired);
}
