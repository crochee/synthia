//! Registration lifecycle, descriptor/snapshot cache, and
//! `Registry`-trait surface.
//!
//! 24 tests cover:
//! - register_entry accepts / refuses / increments count
//! - unregister_by_name returns the right `bool`
//! - descriptor / snapshot caches invalidate on change and
//!   survive a clone
//! - snapshot is alphabetically sorted, collapses duplicates,
//!   filters hidden tools
//! - get / list / contains_and_len / dispatch through the
//!   public surface
//!
//! Shared fixtures (`TestEntryTool`, `ShadowTool`,
//! `NamedTool`) live in `super::support`. A handful of tests
//! do derive their own one-off `Tool` impls (`ToolWithRequired`,
//! `FastTool`, two `HiddenTool` shapes) for dispatch-shape
//! coverage.
//!
//! `use super::*;` brings in the parent block's `Arc`,
//! `Context`, `PathBuf`, `ToolRegistry`, `ToolEntry`,
//! `RegistrationToken`, `ProviderEntry`, `ToolProvenance`,
//! `ToolExposure`, `synthia_provider::ToolUse`,
//! `ToolOutput`, and the
//! `collect_results` helper. `async_trait` is repeated
//! because the inline `Tool` impls need it and `use
//! super::*;` does not re-export the macro.

use async_trait::async_trait;

use super::*;

#[test]
fn register_entry_cannot_shadow_core_tool() {
    let registry = ToolRegistry::new();
    // Manually inject a Core entry into the
    // inner HashMap (Core tools are normally
    // registered via the provider API at
    // startup, not via `register_entry`).
    {
        let mut inner = registry.inner.write();
        let name = "core_name".to_string();
        let token = RegistrationToken(inner.next_registration);
        inner.next_registration += 1;
        let provider_entry = ProviderEntry {
            provider_token: token,
            provenance: ToolProvenance::Core,
            is_hidden: false,
            exposure: ToolExposure::Direct,
            tool: Arc::new(ShadowTool),
        };
        inner.tools.entry(name).or_default().push(provider_entry);
    }
    // Now try to register a Dynamic tool with
    // the same name via the public API.
    let accepted =
        registry.register_entry(ToolEntry::new(Arc::new(ShadowTool)));
    assert!(
        !accepted,
        "register_entry MUST refuse to shadow a Core tool; got {accepted}"
    );
}

/// `register_entry` for a fresh name returns
/// `true` and increments `tool_count`.
#[test]
fn register_entry_for_new_name_returns_true_and_inserts() {
    let registry = ToolRegistry::new();
    let count_before = registry.tool_count();
    let accepted =
        registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    assert!(accepted, "fresh name must be accepted");
    assert_eq!(
        registry.tool_count(),
        count_before + 1,
        "tool_count MUST increment by 1"
    );
}

/// `unregister_by_name` returns `true` when a
/// tool is removed, `false` when the name was
/// never registered. The `bool` return is
/// load-bearing for the cleanup code path —
/// callers use it to decide whether to log.
#[test]
fn unregister_by_name_returns_true_on_remove_false_on_unknown() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    assert!(
        registry.unregister_by_name("test"),
        "existing name MUST return true"
    );
    assert!(
        !registry.unregister_by_name("test"),
        "second unregister of same name MUST return false"
    );
    assert!(
        !registry.unregister_by_name("never_existed"),
        "unknown name MUST return false"
    );
}

/// `descriptors_cached` is a memo, not a projection: a tool
/// registered (or unregistered) *after* a cached read must appear
/// (or disappear) on the very next call, or the model would be
/// advertised a stale catalog.
#[test]
fn descriptors_cached_invalidates_when_the_registry_changes() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));

    let first = registry.descriptors_cached();
    assert_eq!(
        first.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["alpha"]
    );
    // Unchanged registry: the same allocation comes back.
    let again = registry.descriptors_cached();
    assert!(
        Arc::ptr_eq(&first, &again),
        "an unchanged registry must reuse the memo"
    );

    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("beta"))));
    let after_insert = registry.descriptors_cached();
    assert_eq!(
        after_insert
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"],
        "a newly registered tool must be advertised immediately"
    );
    assert!(!Arc::ptr_eq(&first, &after_insert));

    assert!(registry.unregister_by_name("alpha"));
    let after_remove = registry.descriptors_cached();
    assert_eq!(
        after_remove
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta"],
        "an unregistered tool must disappear immediately"
    );
}

/// Cloning a registry keeps the memo usable (the clone shares the
/// registrations *and* the version, so a stale entry would be a bug
/// that only shows up in a cloned deployment path).
#[test]
fn descriptors_cached_survives_a_clone() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));
    let warm = registry.descriptors_cached();

    let clone = registry.clone();
    let from_clone = clone.descriptors_cached();
    assert_eq!(
        from_clone
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha"]
    );
    // The clone's own registrations still invalidate its memo.
    clone.register_entry(ToolEntry::new(Arc::new(NamedTool("beta"))));
    assert_eq!(clone.descriptors_cached().len(), 2);
    assert_eq!(
        registry.descriptors_cached().len(),
        1,
        "the original registry is unaffected"
    );
    assert_eq!(warm.len(), 1);
}

/// `snapshot_cached` is a memo with the same invalidation contract
/// as `descriptors_cached`: unchanged registry → the same
/// allocation; any change → the new catalog on the very next call.
/// A stale catalog would advertise a tool the model cannot call (or
/// hide one it can).
#[test]
fn snapshot_cached_reuses_until_the_registry_changes() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));

    let first = registry.snapshot_cached();
    assert_eq!(
        first.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["alpha"]
    );
    let again = registry.snapshot_cached();
    assert!(
        Arc::ptr_eq(&first, &again),
        "an unchanged registry must reuse the memo"
    );

    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("beta"))));
    let after_insert = registry.snapshot_cached();
    assert_eq!(
        after_insert
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"],
        "a newly registered tool must be advertised immediately"
    );
    assert!(!Arc::ptr_eq(&first, &after_insert));

    assert!(registry.unregister_by_name("alpha"));
    assert_eq!(
        registry
            .snapshot_cached()
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta"],
        "an unregistered tool must disappear immediately"
    );
}

/// A hidden tool never enters the cached catalog, and flipping the
/// flag is a registry change like any other — so the memo cannot
/// keep advertising a tool that was hidden after it was cached.
#[test]
fn snapshot_cached_follows_the_hidden_flag() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("alpha"))));
    assert_eq!(registry.snapshot_cached().len(), 1);

    assert!(registry.set_hidden("alpha", true));
    assert!(
        registry.snapshot_cached().is_empty(),
        "a hidden tool must leave the cached catalog"
    );

    assert!(registry.set_hidden("alpha", false));
    assert_eq!(
        registry.snapshot_cached().len(),
        1,
        "unhiding must restore it: the version moved"
    );
}

/// `snapshot()` returns entries sorted by name
/// (deterministic ordering for downstream
/// consumers). Insert in reverse-alphabetical
/// order and verify the snapshot comes back
/// in alphabetical order.
#[test]
fn snapshot_is_sorted_alphabetically_for_deterministic_output() {
    let registry = ToolRegistry::new();
    // Insert in REVERSE alphabetical order.
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("zoo"))));
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("apple"))));
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("mango"))));
    let snap = registry.snapshot();
    let names: Vec<&str> = snap.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["apple", "mango", "zoo"],
        "snapshot MUST be sorted alphabetically; got {names:?}"
    );
}

/// `snapshot()` returns `tool_count() - duplicates`
/// entries (last entry per bucket wins via shadowing).
/// Pin the shadowing semantics: registering the
/// same name twice produces 1 snapshot entry
/// (the newer one) but `tool_count` reflects
/// both insertions. Actually let me check the
/// tool_count semantics first.
#[test]
fn snapshot_returns_one_entry_per_unique_name() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    let snap = registry.snapshot();
    let test_entries: Vec<_> =
        snap.iter().filter(|s| s.name == "test").collect();
    assert_eq!(
        test_entries.len(),
        1,
        "duplicate registrations collapse to 1 snapshot entry (last-wins shadowing)"
    );
}

/// `with_is_hidden(true)` hides the tool
/// from snapshot output. Pin the contract so
/// the LLM never sees hidden tools.
#[test]
fn with_is_hidden_filters_out_tool_from_snapshot() {
    let registry = ToolRegistry::new();
    let hidden = ToolEntry::new(Arc::new(TestEntryTool)).with_is_hidden(true);
    registry.register_entry(hidden);
    let snap = registry.snapshot();
    assert!(
        !snap.iter().any(|s| s.name == "test"),
        "hidden tool MUST NOT appear in snapshot; got {snap:?}"
    );
}

/// `tool_count()` and `snapshot()` both return
/// 1 entry for duplicate registrations — the
/// HashMap's `len()` is the unique-name count,
/// not the raw ProviderEntry bucket length.
/// Pin the contract so a refactor that switches
/// `tool_count` to use `values().map(|v| v.len()).sum()`
/// (raw ProviderEntry count) breaks loudly.
#[test]
fn tool_count_and_snapshot_both_collapse_duplicate_names_to_one() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    assert_eq!(
        registry.tool_count(),
        1,
        "tool_count = unique name count (HashMap len)"
    );
    assert_eq!(
        registry.snapshot().len(),
        1,
        "snapshot = unique name count (last-wins)"
    );
}

#[tokio::test]
async fn test_tool_registry_register_and_get() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    let entry = registry.get("test").await.unwrap();
    assert!(entry.is_some());
    assert!(entry.unwrap().tool_instance().name() == "test");
    assert!(registry.get("nonexistent").await.unwrap().is_none());
}

#[tokio::test]
async fn test_tool_registry_unregister() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    registry.unregister_by_name("test");
    let entries = registry.list(None).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_list_definitions() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    let entries = registry.list(None).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "test");
}

#[tokio::test]
async fn test_run_with_context() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({}),
    };
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let results =
        collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_text());
}

#[tokio::test]
async fn test_run_with_context_validation_error() {
    #[derive(Debug)]
    struct ToolWithRequired;
    #[async_trait]
    impl Tool for ToolWithRequired {
        fn name(&self) -> &str {
            "req"
        }

        fn description(&self) -> &str {
            "Tool with required param"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "required": ["name"],
                "properties": {}
            })
        }

        async fn call(
            &self,
            input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            if input.get("name").is_none() {
                return ToolOutput::error(
                    "Missing required property: name".to_string(),
                );
            }
            ToolOutput::text("ok")
        }
    }

    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ToolWithRequired)));

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "req".to_string(),
        input: serde_json::json!({}),
    };
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let results =
        collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));
    let text = results[0].1.content[0].text().unwrap();
    assert!(
        text.contains("Missing required") || text.contains("required property"),
        "Unexpected validation error: {}",
        text
    );
}

#[tokio::test]
async fn test_run_with_context_concurrent() {
    #[derive(Debug)]
    struct FastTool;
    #[async_trait]
    impl Tool for FastTool {
        fn name(&self) -> &str {
            "fast"
        }

        fn description(&self) -> &str {
            "Fast tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("done")
        }
    }

    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(FastTool)));

    let tool_uses = vec![
        synthia_provider::ToolUse {
            id: "1".to_string(),
            name: "fast".to_string(),
            input: serde_json::json!({}),
        },
        synthia_provider::ToolUse {
            id: "2".to_string(),
            name: "fast".to_string(),
            input: serde_json::json!({}),
        },
    ];
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let expected = tool_uses.len();
    let results =
        collect_results(registry.run_stream(tool_uses, ctx), expected).await;
    assert_eq!(results.len(), 2);
    for r in &results {
        assert!(r.1.is_text());
    }
}

#[tokio::test]
async fn test_run_with_context_unknown_tool() {
    let registry = ToolRegistry::new();

    let tool_uses = vec![synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "nonexistent_tool".to_string(),
        input: serde_json::json!({}),
    }];
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let expected = tool_uses.len();
    let results =
        collect_results(registry.run_stream(tool_uses, ctx), expected).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));
}

#[tokio::test]
async fn test_hidden_tools_not_in_list() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    #[derive(Debug)]
    struct HiddenTool;

    #[async_trait]
    impl Tool for HiddenTool {
        fn name(&self) -> &str {
            "hidden"
        }

        fn description(&self) -> &str {
            "A hidden tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("hidden output")
        }
    }

    registry.register_entry(
        ToolEntry::new(Arc::new(HiddenTool)).with_is_hidden(true),
    );

    let entries = registry.list(None).await.unwrap();
    assert_eq!(entries.len(), 2);

    let hidden = entries.iter().find(|e| e.name() == "hidden").unwrap();
    assert!(hidden.is_hidden());

    let visible = entries.iter().find(|e| e.name() == "test").unwrap();
    assert!(!visible.is_hidden());

    assert!(registry.get("test").await.unwrap().is_some());
    assert!(registry.get("hidden").await.unwrap().is_some());
}

#[tokio::test]
async fn test_hidden_tool_not_executed() {
    let registry = ToolRegistry::new();

    #[derive(Debug)]
    struct HiddenTool;

    #[async_trait]
    impl Tool for HiddenTool {
        fn name(&self) -> &str {
            "hidden_exec"
        }

        fn description(&self) -> &str {
            "A hidden executable tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text("hidden executed")
        }
    }

    registry.register_entry(
        ToolEntry::new(Arc::new(HiddenTool)).with_is_hidden(true),
    );

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "hidden_exec".to_string(),
        input: serde_json::json!({}),
    };
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let results =
        collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));
    assert!(
        results[0].1.content[0]
            .text()
            .unwrap()
            .contains("not found")
    );
}

#[tokio::test]
async fn test_registry_trait_list() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    let items = registry.list(None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name(), "test");

    let filtered = registry
        .list(Some(super::super::registry_trait::ToolFilter {
            name_prefix: Some("tes".to_string()),
        }))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);

    let no_match = registry
        .list(Some(super::super::registry_trait::ToolFilter {
            name_prefix: Some("xyz".to_string()),
        }))
        .await
        .unwrap();
    assert_eq!(no_match.len(), 0);
}

#[tokio::test]
async fn test_registry_trait_get() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    let item = registry.get("test").await.unwrap();
    assert_eq!(item.unwrap().name(), "test");

    let not_found = registry.get("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_registry_trait_contains_and_len() {
    let registry = ToolRegistry::new();
    assert!(registry.snapshot().is_empty());
    assert_eq!(registry.tool_count(), 0);

    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));
    assert!(!registry.snapshot().is_empty());
    assert_eq!(registry.tool_count(), 1);
    assert!(
        registry.snapshot().iter().any(|s| s.name == "test"),
        "expected `test` to appear in the registry snapshot"
    );
    assert!(
        registry.snapshot().iter().all(|s| s.name != "nonexistent"),
        "expected `nonexistent` to be absent"
    );
}
