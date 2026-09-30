//! Feature-gate proofs for the facade.
//!
//! Gating is proven from both sides:
//!
//! - **Absence in a default build** — the `compile_fail` doc tests in
//!   `src/lib.rs`. Each names an item from a module its feature gates,
//!   so `cargo test -p synthia --doc` fails the moment a feature stops
//!   gating its module (and the doc tests are not emitted at all under
//!   `--all-features`, where the modules do exist).
//! - **Presence once enabled** — the tests below. Each is compiled only
//!   when its feature is on, and fails to compile if the module is
//!   missing, so `cargo test -p synthia --all-features` proves every
//!   opt-in module is reachable.
//!
//! The names are imported with `as _` deliberately: the property under
//! test is *reachability* of the facade path, not any behaviour of the
//! item behind it (the underlying crates own those tests), so the
//! imports are the assertion and nothing else in the test body uses
//! them.
#![allow(unused_imports)]

/// The default feature set exposes every module it promises.
///
/// This is the compile-time half of the acceptance criterion "default
/// features are limited to the assemble-a-basic-agent set": if a module
/// listed here stops being part of the default set, this test does not
/// compile.
#[cfg(all(
    feature = "core",
    feature = "provider",
    feature = "context",
    feature = "tool",
    feature = "session",
    feature = "steering",
    feature = "harness",
    feature = "macros",
))]
#[test]
fn default_set_modules_are_reachable() {
    use synthia::{
        context::TruncatingContextManager as _,
        core::AtomicCancelToken as _,
        harness::ReActAgent as _,
        macros::Tool as _,
        provider::ModelProvider as _,
        session::TypedEventSink as _,
        steering::Steering as _,
        tool::ToolRegistry as _,
    };
}

/// The prelude exposes exactly the names the tutorial promises.
///
/// A name disappearing from the curated set is a compile error here
/// (the collision decisions documented on `synthia::prelude` pin what
/// each of those names *means*).
#[cfg(all(
    feature = "core",
    feature = "provider",
    feature = "context",
    feature = "tool",
    feature = "session",
    feature = "steering",
    feature = "harness",
))]
#[test]
fn prelude_exposes_the_assembly_surface() {
    use synthia::prelude::{
        Agent as _,
        AgentEvent as _,
        AgentInput as _,
        AtomicCancelToken as _,
        CancelToken as _,
        CompletionRequest as _,
        CompletionResponse as _,
        Content as _,
        ContentPart as _,
        Context as _,
        ContextManager as _,
        Error as _,
        Message as _,
        ModelConfig as _,
        ModelProvider as _,
        ProviderConfig as _,
        ReActAgent as _,
        Result as _,
        Steering as _,
        Tool as _,
        ToolEntry as _,
        ToolOutput as _,
        ToolRegistry as _,
        TruncatingContextManager as _,
        TypedEventSink as _,
        async_trait as _,
    };
}

/// `synthia::skill` exists only under the `skill` feature.
#[cfg(feature = "skill")]
#[test]
fn skill_module_is_reachable_when_enabled() {
    use synthia::skill::Skill as _;
}

/// The Anthropic adapter is reachable only under `provider-anthropic`.
#[cfg(feature = "provider-anthropic")]
#[test]
fn anthropic_adapter_is_reachable_when_enabled() {
    use synthia::provider::AnthropicProvider as _;
}

/// The OpenAI-compatible adapter is reachable only under
/// `provider-openai`.
#[cfg(feature = "provider-openai")]
#[test]
fn openai_adapter_is_reachable_when_enabled() {
    use synthia::provider::OpenAICompatibleProvider as _;
}

/// The `web_fetch` tool plugin is reachable only under `tool-web`.
#[cfg(feature = "tool-web")]
#[test]
fn web_fetch_builtin_is_reachable_when_enabled() {
    use synthia::tool_web::WebFetchTool as _;
}

/// Each offline tool plugin is reachable under its own feature.
#[cfg(all(
    feature = "tool-read",
    feature = "tool-write",
    feature = "tool-shell",
    feature = "tool-todo"
))]
#[test]
fn offline_tool_plugins_are_reachable_when_enabled() {
    use synthia::{
        tool_read::ReadTool as _,
        tool_shell::ShellTool as _,
        tool_todo::TodoWriteTool as _,
        tool_write::WriteTool as _,
    };
}

/// The `task` tool plugin is reachable under `tool-task`.
#[cfg(feature = "tool-task")]
#[test]
fn tool_task_plugin_is_reachable_when_enabled() {
    use synthia::tool_task::{TaskDelegator as _, TaskSpec as _};
}

/// The `schedule` tool plugin is reachable under `tool-scheduler` —
/// and, unlike the other tool plugins, it is opt-in: the host must
/// build the `ScheduleStore` it adapts.
#[cfg(feature = "tool-scheduler")]
#[test]
fn tool_scheduler_plugin_is_reachable_when_enabled() {
    use synthia::tool_scheduler::{
        SCHEDULE_TOOL_NAME as _,
        SchedulerTool as _,
        register_scheduler_tool as _,
    };
}

/// The `search` tool plugin is reachable under `tool-search` — and, like
/// `tool-scheduler`, it is opt-in: the host must build the
/// `synthia-search::Registry` it searches.
#[cfg(feature = "tool-search")]
#[test]
fn tool_search_plugin_is_reachable_when_enabled() {
    use synthia::tool_search::{
        SEARCH_TOOL_NAME as _,
        SearchTool as _,
        register_search_tool as _,
    };
}

/// `synthia::attachment` exists only under the `attachment` feature.
#[cfg(feature = "attachment")]
#[test]
fn attachment_module_is_reachable_when_enabled() {
    use synthia::attachment::AttachmentStore as _;
}

/// `synthia::mcp` exists only under the `mcp` feature.
#[cfg(feature = "mcp")]
#[test]
fn mcp_module_is_reachable_when_enabled() {
    use synthia::mcp::McpTool as _;
}

/// `synthia::scheduler` exists only under the `scheduler` feature.
#[cfg(feature = "scheduler")]
#[test]
fn scheduler_module_is_reachable_when_enabled() {
    use synthia::scheduler::Scheduler as _;
}

/// The `cron` feature is the whole cron path through the facade, not
/// half of it: it reaches the parser (`synthia-scheduler/cron`, behind
/// `synthia::scheduler::CronTrigger`) *and* the tool layer's forwarding
/// feature (`synthia-tool-scheduler/cron`, which is what makes the
/// `schedule` tool accept `kind: "cron"` instead of refusing it).
///
/// Reachability alone would not prove the second half — under
/// `--all-features` the `tool-scheduler` feature is on anyway — so the
/// assertion is the tool's own schema: it advertises the `cron` kind
/// exactly when *its* `cron` feature is on, and nothing else in this
/// workspace enables that feature. A facade `cron` that pulled only the
/// parser would leave the model's `create kind=cron` refused, and this
/// test would see `["interval", "once"]`.
#[cfg(feature = "cron")]
#[test]
fn cron_reaches_parser_and_tool_acceptance_through_the_facade() {
    use std::sync::Arc;

    use synthia::{
        scheduler::{CronTrigger as _, ScheduleStore},
        tool::Tool as _,
        tool_scheduler::{SchedulerTool, register_scheduler_tool as _},
    };

    // Never touched by `parameters()`; the store is the tool's handle
    // on the schedule it edits.
    let store = Arc::new(ScheduleStore::new(std::env::temp_dir()));
    let tool = SchedulerTool::new(store);
    assert_eq!(
        tool.parameters()["properties"]["kind"]["enum"],
        serde_json::json!(["interval", "once", "cron"]),
        "the facade's `cron` must reach the tool's acceptance too: {}",
        tool.parameters(),
    );
    assert_eq!(
        tool.parameters()["properties"]["cron_expr"]["type"],
        serde_json::json!("string"),
    );
}

/// `synthia::eval` exists only under the `eval` feature.
#[cfg(feature = "eval")]
#[test]
fn eval_module_is_reachable_when_enabled() {
    use synthia::eval::EvalSuite as _;
}

/// `synthia::workflow` exists only under the `workflow` feature.
#[cfg(feature = "workflow")]
#[test]
fn workflow_module_is_reachable_when_enabled() {
    use synthia::workflow::WorkflowSpec as _;
}

/// `synthia::telemetry` exists only under the `telemetry` feature.
#[cfg(feature = "telemetry")]
#[test]
fn telemetry_module_is_reachable_when_enabled() {
    use synthia::telemetry::init_tracing as _;
}

// (The facade no longer carries a `server` feature: the application
// crate `synthia-server` depends on `synthia` — re-exporting it
// here would be a dependency cycle. Consumers add
// `synthia-server` alongside `synthia` when they want the
// deployment.)
/// `synthia::test_support` exists only under the `test-support` feature.
#[cfg(feature = "test-support")]
#[test]
fn test_support_module_is_reachable_when_enabled() {
    use synthia::test_support::ReplayProvider as _;
}

/// The `sqlite` feature forwards to `synthia-context/sqlite`, so the
/// SQLite memory tier is reachable through the facade's `context`
/// module only when it is enabled.
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_memory_is_reachable_when_enabled() {
    use synthia::context::memory::SqliteMemory as _;
}
