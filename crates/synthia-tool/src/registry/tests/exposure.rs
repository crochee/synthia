//! Tests for `ToolExposure` and the related projection.
//!
//! Five tests exercise:
//! - `ToolExposure::default()` is `Direct`
//! - `with_exposure` is read back by `exposure()` and reaches
//!   every `ToolDescriptor`
//! - `descriptors()` reports hidden tools while `snapshot()`
//!   filters them
//! - the `Registry` trait's `get`/`list` preserve exposure
//! - `Hidden` exposure is advertisement-only: dispatch still
//!   runs the tool
//!
//! `use super::*;` brings in the parent block's fixtures
//! (`NamedTool`, `TestEntryTool`), `ToolRegistry`, `ToolEntry`,
//! `ToolExposure`, and the `collect_results` helper.

use super::*;

#[test]
fn exposure_default_is_direct() {
    // Verify that ToolExposure::default() is Direct
    assert_eq!(ToolExposure::default(), ToolExposure::Direct);
}

/// `with_exposure` is read back by `exposure()`, defaults to
/// `Direct`, and reaches every [`ToolDescriptor`] the registry
/// hands out — including the registration-order-independent
/// `descriptors()` list.
#[test]
fn exposure_is_carried_from_entry_to_descriptor() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("read_file"))));
    registry.register_entry(
        ToolEntry::new(Arc::new(NamedTool("query_db")))
            .with_exposure(ToolExposure::Deferred),
    );

    let entry = ToolEntry::new(Arc::new(NamedTool("x")));
    assert_eq!(
        entry.exposure(),
        ToolExposure::Direct,
        "an entry without with_exposure() is Direct"
    );
    assert_eq!(
        entry.with_exposure(ToolExposure::Hidden).exposure(),
        ToolExposure::Hidden
    );

    let descs = registry.descriptors();
    let got: Vec<(&str, ToolExposure)> = descs
        .iter()
        .map(|d| (d.name.as_str(), d.exposure))
        .collect();
    assert_eq!(
        got,
        vec![
            ("query_db", ToolExposure::Deferred),
            ("read_file", ToolExposure::Direct),
        ],
        "descriptors() must carry each entry's exposure, sorted by name"
    );
}

/// `descriptors()` is the projection's input, so it must report the
/// flags as registered instead of pre-filtering like `snapshot()`.
#[test]
fn descriptors_reports_hidden_tools_while_snapshot_filters_them() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(NamedTool("open"))));
    registry.register_entry(
        ToolEntry::new(Arc::new(NamedTool("secret")))
            .with_is_hidden(true)
            .with_exposure(ToolExposure::Hidden),
    );

    assert_eq!(
        registry
            .snapshot()
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["open"],
        "snapshot() keeps its privacy filter"
    );
    let descs = registry.descriptors();
    assert_eq!(
        descs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["open", "secret"]
    );
    assert!(!descs[0].is_hidden);
    assert!(descs[1].is_hidden);
}

/// The `Registry` trait re-materialises entries for `get`/`list`;
/// both paths must hand back the exposure that was registered.
#[tokio::test]
async fn registry_get_and_list_preserve_exposure() {
    let registry = ToolRegistry::new();
    registry.register_entry(
        ToolEntry::new(Arc::new(NamedTool("query_db")))
            .with_exposure(ToolExposure::Deferred),
    );
    registry.register_entry(
        ToolEntry::new(Arc::new(NamedTool("ops")))
            .with_is_hidden(true)
            .with_exposure(ToolExposure::Hidden),
    );

    let fetched = registry
        .get("query_db")
        .await
        .unwrap()
        .expect("query_db is registered");
    assert_eq!(fetched.exposure(), ToolExposure::Deferred);

    let listed = registry.list(None).await.unwrap();
    let ops = listed
        .iter()
        .find(|e| e.name() == "ops")
        .expect("ops is listed");
    assert_eq!(ops.exposure(), ToolExposure::Hidden);
    assert!(ops.is_hidden(), "list() still reports the privacy flag");
}

/// Exposure is advertisement-only: `ToolExposure::Hidden` keeps a
/// tool out of the model-facing list but `run_stream` still runs
/// it. Only the `is_hidden` privacy flag refuses the call — the
/// two levels are pinned apart here and in
/// `test_hidden_tool_not_executed`.
#[tokio::test]
async fn hidden_exposure_tool_stays_executable() {
    let registry = ToolRegistry::new();
    registry.register_entry(
        ToolEntry::new(Arc::new(TestEntryTool))
            .with_exposure(ToolExposure::Hidden),
    );

    let tool_use = synthia_provider::ToolUse {
        id: "1".to_string(),
        name: "test".to_string(),
        input: serde_json::json!({}),
    };
    let ctx = Context::new("s1".to_string(), PathBuf::from("/tmp"));
    let results =
        collect_results(registry.run_stream(vec![tool_use], ctx), 1).await;
    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0].1.is_error,
        Some(true),
        "Hidden exposure must not gate dispatch: {:?}",
        results[0].1
    );
    assert_eq!(
        results[0].1.content[0].text().unwrap_or_default(),
        "test output"
    );
}
