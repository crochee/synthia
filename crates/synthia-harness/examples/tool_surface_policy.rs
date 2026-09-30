//! The tool surface policy: what the model is told about, and what
//! still runs.
//!
//! Seam shown: `ToolSurfacePolicy` decides the **advertising** half of
//! the tool catalog — which groups are offered, in how much schema
//! detail, and how many — while the registry keeps deciding what can
//! actually execute. The two are deliberately different questions: a
//! tool nobody advertises still runs when a skill, a workflow step or
//! a peer agent calls it.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-harness --example tool_surface_policy
//! ```
//!
//! Look at: the same registry, projected three ways (full catalog,
//! groups applied, capped), and the last section dispatching a tool
//! the policy deliberately hides from the model.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_provider::{ContentPart, Message, ToolUse};
use synthia_test_support::collect_results;
use synthia_tool::{
    Context,
    Tool,
    ToolEntry,
    ToolExposure,
    ToolOutput,
    ToolRegistry,
    ToolSurfacePolicy,
    called_tool_names,
    project_tool_definitions,
};

struct EchoTool(&'static str);

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        self.0
    }

    fn description(&self) -> &str {
        "echoes its arguments"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
            "additionalProperties": false,
        })
    }

    async fn call(&self, input: Value, _context: &Context) -> ToolOutput {
        ToolOutput::text(format!("{} ran with {input}", self.0))
    }
}

fn describe(registry: &ToolRegistry, messages: &[Message], label: &str) {
    let called = called_tool_names(messages);
    let defs = project_tool_definitions(&registry.descriptors(), &called, None);
    println!("\n-- {label}");
    for def in defs {
        let open = def
            .input_schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        println!(
            "   {:<11} schema: {}",
            def.name,
            if open { "open placeholder" } else { "full" }
        );
    }
}

/// A transcript in which the model already called `name`.
fn after_calling(name: &str) -> Vec<Message> {
    let mut message = Message::assistant("");
    message.content =
        synthia_provider::Content::parts(vec![ContentPart::ToolUse(ToolUse {
            id: "call-1".to_string(),
            name: name.to_string(),
            input: json!({ "path": "src/lib.rs" }),
        })]);
    vec![message]
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let registry = Arc::new(ToolRegistry::new());
    for name in ["read_file", "write_file", "deploy", "status"] {
        registry.register_entry(ToolEntry::new(Arc::new(EchoTool(name))));
    }

    // 1. No policy: the whole catalog, every schema full.
    describe(&registry, &[], "no policy (the registry as registered)");

    // 2. A policy declares two groups and activates one. Members of an
    //    inactive group are no longer advertised at all; a tool in no
    //    group is untouched.
    let mut groups = BTreeMap::new();
    groups.insert(
        "files".to_string(),
        vec!["read_file".to_string(), "write_file".to_string()],
    );
    groups.insert("ops".to_string(), vec!["deploy".to_string()]);
    let policy = ToolSurfacePolicy {
        max_visible: None,
        groups,
        active_groups: vec!["files".to_string()],
    };
    policy
        .apply(&registry)
        .expect("every group member is registered");

    println!(
        "\nexposures after apply: read_file={:?} deploy={:?} status={:?}",
        registry.exposure("read_file"),
        registry.exposure("deploy"),
        registry.exposure("status"),
    );
    describe(&registry, &[], "groups applied (ops inactive)");

    // 3. Deferred exposure is the other lever: the tool is offered by
    //    name and description only until the transcript shows it was
    //    called, then it promotes itself.
    registry.set_exposure("read_file", ToolExposure::Deferred);
    describe(&registry, &[], "read_file deferred, not yet called");
    describe(
        &registry,
        &after_calling("read_file"),
        "read_file deferred, after a call",
    );

    // 4. The cap is deterministic (descriptors are name-sorted).
    let capped = ToolSurfacePolicy {
        max_visible: Some(2),
        groups: BTreeMap::new(),
        active_groups: Vec::new(),
    };
    let visible = capped.visible_tool_names(&registry.descriptors());
    let mut visible: Vec<&str> = visible.iter().map(String::as_str).collect();
    visible.sort_unstable();
    println!("\n-- capped to 2\n   {visible:?}");

    // 5. The point of the split: the policy wrote `Hidden` onto
    //    `deploy`, so no projection advertises it — and it still
    //    executes.
    let advertised = ToolSurfacePolicy::default()
        .visible_tool_names(&registry.descriptors());
    println!(
        "\ndeploy advertised after the policy: {}",
        advertised.contains("deploy")
    );
    let outputs = collect_results(
        registry.run_stream(
            vec![ToolUse {
                id: "call-2".to_string(),
                name: "deploy".to_string(),
                input: json!({ "path": "src/lib.rs" }),
            }],
            Context::new("surface".to_string(), std::env::temp_dir()),
        ),
        1,
    )
    .await;
    let text: String = outputs
        .into_iter()
        .flat_map(|(_id, output)| output.content)
        .filter_map(|part| part.text().map(str::to_owned))
        .collect();
    println!("deploy dispatched anyway: {text}");

    // A policy that names a tool nobody registered is a typed error,
    // not a silent no-op.
    let mut bad_groups = BTreeMap::new();
    bad_groups.insert("files".to_string(), vec!["nope".to_string()]);
    let bad = ToolSurfacePolicy {
        max_visible: None,
        groups: bad_groups,
        active_groups: vec!["files".to_string()],
    };
    println!(
        "unknown tool in a policy: {}",
        bad.apply(&registry).is_err()
    );

    println!("TOOL-SURFACE-POLICY: OK");
}
