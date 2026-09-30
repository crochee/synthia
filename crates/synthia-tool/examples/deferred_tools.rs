//! `deferred_tools` — the transcript decides how much of a tool the
//! model is shown.
//!
//! Seam: `synthia_tool::project_tool_definitions` +
//! `synthia_tool::called_tool_names`
//! (`crates/synthia-tool/src/surface.rs`), the one pure projection
//! from registered `ToolDescriptor`s to the `ToolDefinition`s a
//! provider request carries.
//!
//! Look at: the same registry advertised three ways — eager (every
//! schema in full), deferred before the first call (the `Deferred`
//! tool's schema replaced by a permissive placeholder, so the
//! model-facing payload shrinks), and deferred after a simulated call
//! (the transcript mentions the name, so the full schema is back).
//! Between them the `Deferred` tool is dispatched through the
//! registry's real stream path: the placeholder is a prompt-economy
//! choice, never a capability change, and the tool's own validation
//! still rejects bad arguments.
//!
//! Run (no network, no API key):
//!
//! ```bash
//! cargo run -p synthia-tool --example deferred_tools
//! ```

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};
use synthia_provider::{Content, ContentPart, Message, Role, ToolResult};
use synthia_tool::{
    Context,
    Tool,
    ToolDescriptor,
    ToolEntry,
    ToolExposure,
    ToolOutput,
    ToolRegistry,
    called_tool_names,
    project_tool_definitions,
};

/// One tool that refuses to run without its single required argument.
/// That refusal is the point: the model may see only a placeholder
/// schema, but the tool still validates its own input on dispatch.
struct SchemaTool {
    name: &'static str,
    description: &'static str,
    argument: &'static str,
}

#[async_trait]
impl Tool for SchemaTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { self.argument: {"type": "string"} },
            "required": [self.argument],
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        match input.get(self.argument).and_then(Value::as_str) {
            Some(value) if !value.is_empty() => ToolOutput::text(format!(
                "{} ran with {}={value}",
                self.name, self.argument
            )),
            _ => ToolOutput::error(format!(
                "`{}` must be a non-empty string",
                self.argument
            )),
        }
    }
}

fn context() -> Context {
    Context::new("deferred-tools".to_string(), PathBuf::from("/tmp"))
}

/// The same catalog with every tool advertised in full — the payload
/// the model list would carry without deferral.
fn eager_baseline(descriptors: &[ToolDescriptor]) -> Vec<ToolDescriptor> {
    descriptors
        .iter()
        .cloned()
        .map(|mut descriptor| {
            descriptor.exposure = ToolExposure::Direct;
            descriptor
        })
        .collect()
}

/// Serialized size of one model-facing tool list.
fn payload_bytes(defs: &[synthia_provider::ToolDefinition]) -> usize {
    serde_json::to_string(defs)
        .expect("tool definitions are JSON values")
        .len()
}

/// Print one model-facing list, one tool per line, and its size.
fn print_surface(step: &str, defs: &[synthia_provider::ToolDefinition]) {
    println!("\n{step} ({} bytes):", payload_bytes(defs));
    for def in defs {
        println!(
            "  {:<10} {}",
            def.name,
            serde_json::to_string(&def.input_schema)
                .expect("schemas are JSON values")
        );
    }
}

/// Dispatch one tool through the registry's real stream path.
async fn dispatch(
    registry: &ToolRegistry,
    name: &str,
    input: Value,
) -> ToolOutput {
    let tool_use = synthia_provider::ToolUse {
        id: format!("call-{name}"),
        name: name.to_string(),
        input,
    };
    synthia_test_support::collect_results(
        registry.run_stream(vec![tool_use], context()),
        1,
    )
    .await
    .into_iter()
    .map(|(_call_id, output)| output)
    .next()
    .expect("run_stream yields exactly one Result per call")
}

/// The transcript a real loop would have after the model called
/// `query_db` once: the assistant's `tool_use` and the tool's result.
fn transcript_after_query_db() -> Vec<Message> {
    let call = synthia_provider::ToolUse {
        id: "call-1".to_string(),
        name: "query_db".to_string(),
        input: json!({"sql": "select 1"}),
    };
    let mut result = ToolResult::new("call-1", "1 row");
    result.tool_name = Some("query_db".to_string());
    vec![
        Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(call)),
        ),
        Message::new(
            Role::Tool,
            Content::Single(ContentPart::ToolResult(result)),
        ),
    ]
}

#[tokio::main]
async fn main() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(SchemaTool {
        name: "read_file",
        description: "read a file by path",
        argument: "path",
    })));
    registry.register_entry(
        ToolEntry::new(Arc::new(SchemaTool {
            name: "query_db",
            description: "run a SQL query against the workspace database",
            argument: "sql",
        }))
        .with_exposure(ToolExposure::Deferred),
    );

    println!("== synthia deferred tools ==\n");
    println!("registered: read_file (Direct), query_db (Deferred)");

    let descriptors = registry.descriptors();
    let eager = eager_baseline(&descriptors);
    let eager_bytes =
        payload_bytes(&project_tool_definitions(&eager, &HashSet::new(), None));

    // 1. Before the first call: `query_db` is advertised by name and
    //    description only. Its real schema is withheld.
    let before =
        project_tool_definitions(&descriptors, &called_tool_names(&[]), None);
    print_surface("before the first call", &before);

    // 2. The model-facing payload is smaller than the eager baseline,
    //    which is the entire point of deferring.
    let before_bytes = payload_bytes(&before);
    assert!(
        before_bytes < eager_bytes,
        "deferral must shrink the model-facing payload; \
         before={before_bytes} eager={eager_bytes}"
    );
    println!(
        "\nmodel-facing payload: {before_bytes} bytes vs {eager_bytes} \
         bytes eager ({} fewer)",
        eager_bytes - before_bytes
    );

    // 3. Dispatch still works with the placeholder in place — and the
    //    tool's own validation still runs.
    let ok = dispatch(&registry, "query_db", json!({"sql": "select 1"})).await;
    assert!(
        !ok.is_error.unwrap_or(false),
        "deferred tool must execute; got {ok:?}"
    );
    println!(
        "dispatch query_db            : {}",
        ok.content[0].text().unwrap_or_default()
    );
    let rejected = dispatch(&registry, "query_db", json!({"sql": ""})).await;
    assert_eq!(
        rejected.is_error,
        Some(true),
        "tool-side validation must reject the empty argument"
    );
    println!(
        "dispatch query_db {{sql: \"\"}} : error: {}",
        rejected.content[0].text().unwrap_or_default()
    );

    // 4. After the simulated call: the transcript mentions the name,
    //    so the projection promotes the tool to its full schema.
    let transcript = transcript_after_query_db();
    let called = called_tool_names(&transcript);
    let after = project_tool_definitions(&descriptors, &called, None);
    print_surface("after a simulated call", &after);
    assert_eq!(after.len(), before.len(), "promotion adds no tool");
    let promoted = after
        .iter()
        .find(|d| d.name == "query_db")
        .expect("query_db is advertised");
    let real = descriptors
        .iter()
        .find(|d| d.name == "query_db")
        .expect("query_db is registered");
    assert_eq!(
        promoted.input_schema, real.parameters,
        "the called tool must be promoted to its real schema"
    );
    assert_eq!(
        after
            .iter()
            .find(|d| d.name == "read_file")
            .expect("read_file is advertised")
            .input_schema,
        before
            .iter()
            .find(|d| d.name == "read_file")
            .expect("read_file was advertised")
            .input_schema,
        "a Direct tool is unaffected by promotion"
    );

    println!("\nDEFERRED-TOOLS: OK");
}
