//! `tool_groups` — named tool groups shape what the *model* is
//! offered, never what the runtime can execute.
//!
//! Seam: `synthia_tool::GroupedRegistry`
//! (`crates/synthia-tool/src/grouped.rs`), a read-side wrapper over a
//! shared `ToolRegistry`.
//!
//! Look at: `visible_tool_names()` before any declaration, after
//! `declare`, and across `activate` / `activate_only` /
//! `deactivate`; then the dispatch of a tool whose group is
//! **deactivated** through `inner().run_stream(..)` — it still
//! answers, because narrowing the advertised list is a prompt-budget
//! decision and never a security boundary.
//!
//! Run (no network, no API key):
//!
//! ```bash
//! cargo run -p synthia-tool --example tool_groups
//! ```

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_tool::{
    Context,
    GroupedRegistry,
    Tool,
    ToolEntry,
    ToolOutput,
    ToolRegistry,
};

/// Registration order for the catalog; the visible list is sorted by
/// name, the registry's own order.
const TOOL_NAMES: [&str; 4] = ["read_file", "write_file", "deploy", "status"];

/// Minimal echo tool: its reply names the tool that ran, which is the
/// only proof this example needs.
struct EchoTool {
    name: String,
}

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "echo the tool's own name back to the caller"
    }

    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(format!("{} echoed", self.name))
    }
}

fn context() -> Context {
    Context::new("tool-groups".to_string(), PathBuf::from("/tmp"))
}

/// Dispatch one tool through the registry's real stream path and
/// concatenate the text it produced.
async fn dispatch(registry: &ToolRegistry, name: &str) -> String {
    let tool_use = synthia_provider::ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input: json!({}),
    };
    let outputs = synthia_test_support::collect_results(
        registry.run_stream(vec![tool_use], context()),
        1,
    )
    .await;
    outputs
        .into_iter()
        .flat_map(|(_call_id, output)| output.content)
        .filter_map(|part| part.text().map(str::to_owned))
        .collect()
}

/// Print the model-facing list for one step of the story.
fn visible(grouped: &GroupedRegistry, step: &str) {
    println!("{step:<28}: [{}]", grouped.visible_tool_names().join(", "));
}

#[tokio::main]
async fn main() {
    let registry = ToolRegistry::new();
    for name in TOOL_NAMES {
        registry.register_entry(ToolEntry::new(Arc::new(EchoTool {
            name: name.to_string(),
        })));
    }
    let registry = Arc::new(registry);
    let grouped = GroupedRegistry::new(Arc::clone(&registry));

    println!("== synthia tool groups ==\n");
    println!("registered tools            : [{}]", TOOL_NAMES.join(", "));

    // 1. No declaration yet: grouping is opt-in, so the catalog is
    //    advertised unchanged.
    visible(&grouped, "before any declaration");

    // 2. Declaring is already a narrowing: a grouped tool is hidden
    //    until its group is activated. `status` is in no group, so it
    //    stays visible throughout.
    grouped
        .declare("files", &["read_file", "write_file"])
        .expect("files group names registered tools exactly once");
    grouped
        .declare("ops", &["deploy"])
        .expect("ops group names registered tools exactly once");
    println!("declared groups             : {:?}", grouped.group_names());
    visible(&grouped, "after declare (no activate)");

    // 3. Activating a group is the only thing that re-advertises its
    //    members — and it touches nothing else.
    grouped.activate("files");
    visible(&grouped, "activate(files)");
    grouped.activate("ops");
    visible(&grouped, "activate(ops)");

    // 4. `activate_only` replaces the active set atomically: an
    //    unknown name would leave it untouched (a typo cannot
    //    half-apply a switch).
    grouped
        .activate_only(&["ops"])
        .expect("both group names are declared");
    println!(
        "active groups               : {:?}",
        grouped.active_group_names()
    );
    visible(&grouped, "activate_only([ops])");

    grouped.deactivate("ops");
    visible(&grouped, "deactivate(ops)");

    // 5. Visibility ≠ executability. `deploy` is gone from the
    //    advertised list but its provider is untouched, so a skill,
    //    workflow step, or subagent can still run it through the
    //    registry — the dispatch authority.
    println!(
        "\ndeploy visible to the model  : {}",
        grouped.is_visible("deploy")
    );
    println!(
        "deploy still registered      : {}",
        registry.contains("deploy")
    );
    println!(
        "group_of(deploy)             : {:?}",
        grouped.group_of("deploy")
    );
    let text = dispatch(grouped.inner(), "deploy").await;
    println!("dispatch through inner()     : {text}");

    println!("\nTOOL-GROUPS: OK");
}
