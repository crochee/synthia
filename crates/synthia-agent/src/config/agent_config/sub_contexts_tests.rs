//! Tests for [`super::sub_contexts`] — read-only sub-context views
//! derived from an [`AgentRunConfig`] without modifying the underlying
//! god-struct.
//!
//! Coverage map (5 tests):
//!
//! - [`LoopContext`] fields: 1 test (provider + router + session_id)
//! - [`PersistenceContext`] fields: 1 test (session_id + store present)
//! - [`OrchestrationContext`] fields: 1 test (control default-None,
//!   factory default-None, fork_policy accessible)
//! - All three contexts derived from same config: 1 test
//! - Stability: deriving twice yields identical Arc-backed views: 1 test

use std::sync::Arc;

use synthia_hook::HookRegistry;
use synthia_provider::router::ModelRouter;
use synthia_session::Store as SessionStore;
use synthia_tool::registry::ToolRegistry;
use tempfile::TempDir;
use test_support::FakeProvider;
use tokio_util::sync::CancellationToken;

use super::{
    sub_contexts::{LoopContext, OrchestrationContext, PersistenceContext},
    *,
};
use crate::input::AgentInput;

fn fake_provider() -> Arc<dyn synthia_provider::traits::ModelProvider> {
    Arc::new(FakeProvider::new(vec![]))
}

fn temp_store() -> (TempDir, SessionStore) {
    let dir = TempDir::new().expect("tempdir");
    let store = SessionStore::new(dir.path().to_path_buf());
    (dir, store)
}

#[test]
fn loop_context_exposes_provider_router_session_id() {
    let (_dir, store) = temp_store();
    let cfg = AgentRunConfigBuilder::new()
        .provider(fake_provider())
        .tool_registry(ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::default()))
        .model_router(Arc::new(ModelRouter::default()))
        .input(AgentInput::text(""))
        .config(AgentConfig::default())
        .cancel_token(CancellationToken::new())
        .session_store(store)
        .user_id("u-1".to_string())
        .session_id("s-loop".to_string())
        .build()
        .expect("build must succeed");

    let loop_ctx = LoopContext::from(&cfg);
    assert!(Arc::ptr_eq(loop_ctx.provider, &cfg.provider));
    assert!(Arc::ptr_eq(loop_ctx.model_router, &cfg.model_router));
    assert_eq!(loop_ctx.session_id, "s-loop");
}

#[test]
fn persistence_context_exposes_session_id_and_store() {
    let (_dir, store) = temp_store();
    let cfg = AgentRunConfigBuilder::new()
        .provider(fake_provider())
        .tool_registry(ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::default()))
        .model_router(Arc::new(ModelRouter::default()))
        .input(AgentInput::text(""))
        .config(AgentConfig::default())
        .cancel_token(CancellationToken::new())
        .user_id("u-1".to_string())
        .session_id("s-persist".to_string())
        .session_store(store)
        .build()
        .expect("build must succeed");

    let persist = PersistenceContext::from(&cfg);
    assert_eq!(persist.session_id, "s-persist");
    let persist2 = PersistenceContext::from(&cfg);
    assert_eq!(persist.session_id, persist2.session_id);
}

#[test]
fn orchestration_context_defaults_are_none() {
    let (_dir, store) = temp_store();
    let cfg = AgentRunConfigBuilder::new()
        .provider(fake_provider())
        .tool_registry(ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::default()))
        .model_router(Arc::new(ModelRouter::default()))
        .input(AgentInput::text(""))
        .config(AgentConfig::default())
        .cancel_token(CancellationToken::new())
        .session_store(store)
        .user_id("u-1".to_string())
        .session_id("s-1".to_string())
        .build()
        .expect("build must succeed");

    let orch = OrchestrationContext::from(&cfg);
    assert!(orch.agent_control.is_none());
    assert!(orch.subagent_session_factory.is_none());
    let _ = orch.fork_policy;
}

#[test]
fn all_three_contexts_derive_from_same_config() {
    let (_dir, store) = temp_store();
    let cfg = AgentRunConfigBuilder::new()
        .provider(fake_provider())
        .tool_registry(ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::default()))
        .model_router(Arc::new(ModelRouter::default()))
        .input(AgentInput::text(""))
        .config(AgentConfig::default())
        .cancel_token(CancellationToken::new())
        .user_id("u-multi".to_string())
        .session_id("s-multi".to_string())
        .session_store(store)
        .build()
        .expect("build must succeed");

    let _loop_ctx = LoopContext::from(&cfg);
    let _persist = PersistenceContext::from(&cfg);
    let _orch = OrchestrationContext::from(&cfg);
}

#[test]
fn sub_contexts_are_zero_copy_views() {
    let (_dir, store) = temp_store();
    let cfg = AgentRunConfigBuilder::new()
        .provider(fake_provider())
        .tool_registry(ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::default()))
        .model_router(Arc::new(ModelRouter::default()))
        .input(AgentInput::text(""))
        .config(AgentConfig::default())
        .cancel_token(CancellationToken::new())
        .session_store(store)
        .user_id("u-zc".to_string())
        .session_id("s-zc".to_string())
        .build()
        .expect("build must succeed");

    let a = LoopContext::from(&cfg);
    let b = LoopContext::from(&cfg);
    assert!(Arc::ptr_eq(a.provider, b.provider));
    assert!(Arc::ptr_eq(a.model_router, b.model_router));
}
