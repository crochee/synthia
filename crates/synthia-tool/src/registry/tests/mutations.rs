//! Post-registration mutation concern (`set_exposure` /
//! `set_hidden`).
//!
//! Two tests exercise:
//! - `set_exposure` reaches the projection immediately,
//!   bumps the version, reports unknown names, and skips
//!   the bump on a no-op set
//! - `set_hidden` gates listing *and* dispatch both ways,
//!   pinned apart from `Hidden` exposure (dispatchable)
//!
//! `use super::*;` brings in the parent block's `Arc`,
//! `ToolRegistry`, `ToolEntry`, `ToolExposure`, `NamedTool`,
//! `TestEntryTool`, `Context`, `PathBuf`, and the
//! `collect_results` helper.

use super::*;

/// `set_exposure` writes where the projection reads: the change
/// reaches `descriptors()` (and therefore
/// `project_tool_definitions`) immediately, bumps the version so a
/// version-keyed cache cannot serve the stale exposure, reports
/// `false` for an unknown name, and leaves the version alone when
/// the value already matches.
#[test]
fn set_exposure_reaches_the_projection_and_reports_unknown_names() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("query_db"))));
    let before = registry.version();
    assert_eq!(registry.descriptors()[0].exposure, ToolExposure::Direct);

    assert!(registry.set_exposure("query_db", ToolExposure::Deferred));
    assert_eq!(registry.exposure("query_db"), Some(ToolExposure::Deferred));
    let descriptors = registry.descriptors();
    assert_eq!(descriptors[0].exposure, ToolExposure::Deferred);
    let defs = crate::project_tool_definitions(
        &descriptors,
        &std::collections::HashSet::new(),
        None,
    );
    assert_eq!(
        defs[0].input_schema,
        serde_json::json!({
            "type": "object",
            "additionalProperties": true,
        }),
        "the projection must see the new exposure"
    );
    assert!(
        registry.version() > before,
        "a real change must invalidate version-keyed caches"
    );

    let after_change = registry.version();
    assert!(
        registry.set_exposure("query_db", ToolExposure::Deferred),
        "setting the stored value still reports the tool exists"
    );
    assert_eq!(
        registry.version(),
        after_change,
        "a no-op set need not invalidate the cache"
    );

    assert!(!registry.set_exposure("ghost", ToolExposure::Hidden));
    assert_eq!(registry.exposure("ghost"), None);
    assert_eq!(registry.version(), after_change);
}

/// `set_hidden` flips the privacy flag both ways: hidden tools
/// leave `snapshot()` and `run_stream` refuses them, unhiding
/// restores both; `Hidden` *exposure* keeps dispatch, so the two
/// levels stay pinned apart through the mutation path too.
#[tokio::test]
async fn set_hidden_gates_listing_and_dispatch_unlike_hidden_exposure() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    assert!(registry.set_hidden("test", true));
    assert!(
        registry.snapshot().is_empty(),
        "a hidden tool leaves the snapshot"
    );
    let tool_use = || synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({}),
    };
    let ctx = || Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let results =
        collect_results(registry.run_stream(vec![tool_use()], ctx()), 1).await;
    assert!(
        results[0].1.is_error.unwrap_or(false),
        "a hidden tool must be refused: {:?}",
        results[0].1
    );

    assert!(registry.set_hidden("test", false));
    assert_eq!(registry.snapshot().len(), 1);
    let results =
        collect_results(registry.run_stream(vec![tool_use()], ctx()), 1).await;
    assert_ne!(results[0].1.is_error, Some(true), "unhiding restores it");

    assert!(registry.set_exposure("test", ToolExposure::Hidden));
    let results =
        collect_results(registry.run_stream(vec![tool_use()], ctx()), 1).await;
    assert_ne!(
        results[0].1.is_error,
        Some(true),
        "Hidden exposure must stay dispatcheable"
    );
}
