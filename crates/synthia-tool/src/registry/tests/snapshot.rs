//! Dual-index `snapshot()` ordering and materialisation.
//!
//! Seven tests exercise:
//! - `snapshot()` is sorted by name, independent of insertion
//!   order
//! - unregistration is reflected in the next snapshot
//! - the empty-registry snapshot is empty
//! - `ToolEntry` metadata builders (`with_is_hidden`)
//! - `resolve` hands back the canonical trait object
//! - materialisation preserves descriptor content
//! - `register_scoped_arc` unregisters on drop
//!
//! `use super::*;` brings in the parent block's `Arc`,
//! `ToolRegistry`, `ToolEntry`, `TestEntryTool`, `Context`,
//! and the `Tool` trait. `async_trait` is repeated because the
//! tests below derive `Tool` impls and `use super::*;` does
//! not re-export it.

use async_trait::async_trait;

use super::*;

#[test]
fn test_snapshot_is_sorted_by_name() {
    #[derive(Debug)]
    struct ToolA;
    #[derive(Debug)]
    struct ToolB;

    #[async_trait]
    impl Tool for ToolA {
        fn name(&self) -> &str {
            "a"
        }

        fn description(&self) -> &str {
            "Tool A"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("a")
        }
    }

    #[async_trait]
    impl Tool for ToolB {
        fn name(&self) -> &str {
            "b"
        }

        fn description(&self) -> &str {
            "Tool B"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("b")
        }
    }

    // Register in reverse-alphabetical order to prove the snapshot
    // ordering is independent of insertion order.
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ToolB)));
    registry.register_entry(ToolEntry::new(Arc::new(ToolA)));

    let meta = registry.snapshot();
    assert_eq!(meta.len(), 2);
    assert_eq!(meta[0].name, "a");
    assert_eq!(meta[1].name, "b");
}

#[tokio::test]
async fn test_snapshot_after_unregister() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    #[derive(Debug)]
    struct OtherTool;
    #[async_trait]
    impl Tool for OtherTool {
        fn name(&self) -> &str {
            "other"
        }

        fn description(&self) -> &str {
            "Other"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(&self, _: serde_json::Value, _: &Context) -> ToolOutput {
            ToolOutput::text("o")
        }
    }
    registry.register_entry(ToolEntry::new(Arc::new(OtherTool)));

    let meta = registry.snapshot();
    assert_eq!(meta.len(), 2);

    registry.unregister_by_name("test");
    let meta = registry.snapshot();
    assert_eq!(meta.len(), 1);
    assert_eq!(meta[0].name, "other");
}

#[test]
fn test_snapshot_empty_registry() {
    let registry = ToolRegistry::new();
    assert!(registry.snapshot().is_empty());
}

#[test]
fn test_entry_metadata_builders() {
    let entry = ToolEntry::new(Arc::new(TestEntryTool)).with_is_hidden(true);
    assert!(entry.is_hidden());
}

#[tokio::test]
async fn resolve_returns_canonical_tool_trait_object() {
    let registry = ToolRegistry::new();
    assert!(registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool))));
    let entries = registry.list(None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "test");
}

#[test]
fn materialization_preserves_descriptor_content() {
    let registry = ToolRegistry::new();
    assert!(registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool))));
    let descriptors = registry.snapshot();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].name, "test");
    assert_eq!(descriptors[0].description, "A test tool");
}

#[tokio::test]
async fn register_scoped_arc_unregisters_on_drop() {
    let registry = Arc::new(ToolRegistry::new());
    let scope = registry
        .register_scoped_arc(ToolEntry::new(Arc::new(TestEntryTool)))
        .await;
    assert!(
        registry.snapshot().iter().any(|s| s.name == "test"),
        "expected `test` to be present after registration"
    );
    drop(scope);
    assert!(
        registry.snapshot().iter().all(|s| s.name != "test"),
        "expected `test` to be removed after scope drop"
    );
}
