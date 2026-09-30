//! Tests for `create_session_scope` token allocation,
//! drop semantics, and registry-drop ordering.
//!
//! Four tests exercise:
//! - token allocation is non-zero and monotonic
//! - the scope is a no-op when no tools were registered
//! - dropping the registry first does not panic the
//!   scope's own Drop impl (Weak::upgrade returns None)
//!
//! `use super::*;` brings in the parent block's `Arc`,
//! `ToolRegistry`, `RegistrationScope` (the public type
//! under test), and any helper functions / fixtures.

use super::*;

#[test]
fn create_session_scope_returns_valid_token() {
    let registry = Arc::new(ToolRegistry::new());
    let scope = registry.create_session_scope();
    // Token should be non-zero (first allocation)
    assert_ne!(scope.token().0, 0);
}

#[test]
fn create_session_scope_drop_is_noop_when_no_tools_registered() {
    let registry = Arc::new(ToolRegistry::new());
    let tool_count_before = registry.tool_count();
    {
        let _scope = registry.create_session_scope();
    }
    // Tool count unchanged after scope drop (no tools were registered)
    assert_eq!(registry.tool_count(), tool_count_before);
}

#[test]
fn create_session_scope_subsequent_tokens_are_monotonic() {
    let registry = Arc::new(ToolRegistry::new());
    let scope1 = registry.create_session_scope();
    let scope2 = registry.create_session_scope();
    assert!(
        scope2.token().0 > scope1.token().0,
        "tokens should be monotonically increasing"
    );
}

#[test]
fn create_session_scope_noop_when_registry_dropped_first() {
    let registry = Arc::new(ToolRegistry::new());
    let scope = registry.create_session_scope();
    // Drop the registry first
    drop(registry);
    // Now drop the scope — should not panic (Weak::upgrade returns None)
    drop(scope);
}
