//! Integration proof for the group contract: a tool whose group is
//! deactivated leaves the model-facing list but stays **executable**.
//!
//! The unit tests in `src/grouped.rs` pin the read-side logic; these
//! drive the real dispatch path (`ToolRegistry::run_stream`) to show
//! the wrapper never gates execution.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_test_support::collect_results;
use synthia_tool::{
    Context,
    GroupError,
    GroupedRegistry,
    Tool,
    ToolEntry,
    ToolOutput,
    ToolRegistry,
};

struct MockTool {
    name: String,
    description: String,
}

impl MockTool {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(format!("{} returned", self.name))
    }
}

fn make_context() -> Context {
    Context::new("groups-session".to_string(), PathBuf::from("/tmp"))
}

fn make_tool_use(name: &str) -> synthia_provider::ToolUse {
    synthia_provider::ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input: json!({}),
    }
}

fn registry_with(names: &[&str]) -> Arc<ToolRegistry> {
    let registry = ToolRegistry::new();
    for name in names {
        registry.register_entry(ToolEntry::new(Arc::new(MockTool::new(
            name,
            "grouped test tool",
        ))));
    }
    Arc::new(registry)
}

async fn dispatch(registry: &ToolRegistry, name: &str) -> String {
    let outputs = collect_results(
        registry.run_stream(vec![make_tool_use(name)], make_context()),
        1,
    )
    .await;
    outputs
        .into_iter()
        .flat_map(|(_call_id, output)| output.content)
        .filter_map(|part| part.text().map(str::to_owned))
        .collect()
}

/// The whole point of groups: deactivating one narrows what the model
/// is offered, and nothing else. The tool runs.
#[tokio::test]
async fn deactivated_group_tools_stay_executable() {
    let registry = registry_with(&["read", "deploy"]);
    let grouped = GroupedRegistry::new(Arc::clone(&registry));
    grouped.declare("ops", &["deploy"]).unwrap();
    grouped.activate("ops");
    assert_eq!(grouped.visible_tool_names(), vec!["deploy", "read"]);

    grouped.deactivate("ops");
    assert_eq!(grouped.visible_tool_names(), vec!["read"]);
    assert!(!grouped.is_visible("deploy"));

    // Still registered, still dispatchable — the wrapper only shaped
    // the advertised list.
    assert!(registry.contains("deploy"));
    let text = dispatch(&registry, "deploy").await;
    assert!(
        text.contains("deploy returned"),
        "deactivated tool must still execute: {text}"
    );
}

/// Activating a group is a read-side switch: the registry's catalog
/// and version are untouched, so cached wiring stays valid.
#[tokio::test]
async fn activation_is_read_side_only() {
    let registry = registry_with(&["a", "b"]);
    let grouped = GroupedRegistry::new(Arc::clone(&registry));
    grouped.declare("only_a", &["a"]).unwrap();
    grouped.declare("only_b", &["b"]).unwrap();

    let full_before = registry.snapshot();
    let version_before = registry.version();

    grouped.activate_only(&["only_b"]).unwrap();
    assert_eq!(grouped.visible_tool_names(), vec!["b"]);
    assert_eq!(grouped.visible_metadata_snapshots()[0].name, "b");

    grouped.activate_only(&["only_a"]).unwrap();
    assert_eq!(grouped.visible_tool_names(), vec!["a"]);

    assert_eq!(registry.snapshot().len(), full_before.len());
    assert_eq!(registry.version(), version_before);
    assert_eq!(registry.tool_count(), 2);
}

/// A typo in a switch does not half-apply, and a typo in a
/// declaration does not silently drop a tool from the catalog.
#[tokio::test]
async fn wiring_typos_are_rejected_loudly() {
    let registry = registry_with(&["a"]);
    let grouped = GroupedRegistry::new(registry);
    grouped.declare("g", &["a"]).unwrap();

    assert_eq!(
        grouped.activate_only(&["g", "grup"]),
        Err(GroupError::UnknownGroup {
            name: "grup".to_owned(),
        })
    );
    assert_eq!(grouped.active_group_names(), Vec::<String>::new());

    assert_eq!(
        grouped.declare("h", &["b"]),
        Err(GroupError::UnknownTool {
            group: "h".to_owned(),
            tool: "b".to_owned(),
        })
    );
    assert_eq!(grouped.group_names(), vec!["g".to_owned()]);
}
