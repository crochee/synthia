//! Integration tests for `#[derive(Tool)]`, exercising the generated
//! impl against the real `synthia_tool::Tool` trait.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use synthia_macros::Tool;
use synthia_tool::{Context, ExecutionMode, Tool, ToolOutput};

// ── Fixture: defaults come from the struct ───────────────────────

/// Echoes the session id back to the caller.
#[derive(Tool, Deserialize, JsonSchema)]
struct EchoSession {
    /// Payload echoed alongside the session id.
    payload: String,
}

impl EchoSession {
    async fn execute(&self, context: &Context) -> ToolOutput {
        ToolOutput::text(format!("{}:{}", context.session_id, self.payload))
    }
}

// ── Fixture: every attribute overridden ──────────────────────────

#[derive(Tool, Deserialize, JsonSchema)]
#[tool(
    name = "sum_values",
    description("Adds two integers."),
    mode = "sequential"
)]
struct Sum {
    /// Left operand.
    a: i64,
    /// Right operand.
    b: i64,
}

impl Sum {
    async fn execute(&self, _context: &Context) -> ToolOutput {
        ToolOutput::text((self.a + self.b).to_string())
    }
}

// ── Fixture: enum field proves type-driven schema precision ──────

#[derive(Tool, Deserialize, JsonSchema)]
#[tool(description("Formats a file."))]
struct FormatFile {
    /// Formatting style.
    style: Style,
}

impl FormatFile {
    async fn execute(&self, _context: &Context) -> ToolOutput {
        ToolOutput::text(format!("{:?}", self.style))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Style {
    Compact,
    Pretty,
}

// ── Fixture: keys split across separate #[tool] attributes ───────

/// Merges separate tool attributes.
#[derive(Tool, Deserialize, JsonSchema)]
#[tool(name = "split_attrs")]
#[tool(mode = "sequential")]
struct SplitAttrs {
    value: i32,
}

impl SplitAttrs {
    async fn execute(&self, _context: &Context) -> ToolOutput {
        ToolOutput::text(self.value.to_string())
    }
}

fn context() -> Context {
    Context::new("it-session".to_string(), PathBuf::from("/tmp"))
}

/// Extract the text of a `ToolOutput` via its JSON projection (the
/// crate keeps `ContentPart` behind `synthia-provider`, which is not
/// a dev-dependency here).
fn text_of(output: &ToolOutput) -> String {
    let value = serde_json::to_value(output).expect("ToolOutput serializes");
    value["content"]
        .as_array()
        .expect("content is an array")
        .iter()
        .filter_map(|part| part.get("text")?.as_str())
        .collect()
}

// ── name / description / mode ────────────────────────────────────

#[test]
fn default_name_is_struct_snake_case() {
    let tool = EchoSession {
        payload: "hi".to_string(),
    };
    assert_eq!(tool.name(), "echo_session");
}

#[test]
fn doc_comment_becomes_default_description() {
    let tool = EchoSession {
        payload: "hi".to_string(),
    };
    assert_eq!(
        tool.description(),
        "Echoes the session id back to the caller."
    );
}

#[test]
fn attributes_override_name_description_and_mode() {
    let tool = Sum { a: 0, b: 0 };
    assert_eq!(tool.name(), "sum_values");
    assert_eq!(tool.description(), "Adds two integers.");
    assert_eq!(tool.mode(), ExecutionMode::Sequential);
}

#[test]
fn keys_split_across_attributes_merge() {
    let tool = SplitAttrs { value: 7 };
    assert_eq!(tool.name(), "split_attrs");
    assert_eq!(tool.description(), "Merges separate tool attributes.");
    assert_eq!(tool.mode(), ExecutionMode::Sequential);
}

#[test]
fn mode_defaults_to_parallel() {
    let tool = EchoSession {
        payload: "hi".to_string(),
    };
    assert_eq!(tool.mode(), ExecutionMode::Parallel);
}

// ── parameters() schema shape ────────────────────────────────────

#[test]
fn schema_is_an_object_with_required_fields() {
    let tool = Sum { a: 0, b: 0 };
    let schema = tool.parameters();

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["title"], "Sum");

    let properties = schema["properties"].as_object().expect("properties");
    assert_eq!(properties.len(), 2);
    assert_eq!(properties["a"]["type"], "integer");
    assert_eq!(properties["b"]["type"], "integer");

    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required is an array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(required.contains(&"a"), "required = {required:?}");
    assert!(required.contains(&"b"), "required = {required:?}");
}

#[test]
fn schema_serializes_enum_fields_as_defs_not_any() {
    let tool = FormatFile {
        style: Style::Compact,
    };
    let schema = tool.parameters();

    // Type-driven generation: the enum field is a $ref into `$defs`
    // with both variants enumerated. (Value-driven inference would
    // degrade the field to `true` — accepts anything.)
    assert_eq!(schema["properties"]["style"]["$ref"], "#/$defs/Style");
    let variants: Vec<&str> = schema["$defs"]["Style"]["enum"]
        .as_array()
        .expect("Style enum")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(variants, vec!["compact", "pretty"]);
}

#[test]
fn schema_is_deterministic_across_instances() {
    let with_compact = FormatFile {
        style: Style::Compact,
    };
    let with_pretty = FormatFile {
        style: Style::Pretty,
    };
    // The schema describes the TYPE, so it must not depend on the
    // instance the call happens to run on.
    assert_eq!(
        serde_json::to_string(&with_compact.parameters()).unwrap(),
        serde_json::to_string(&with_pretty.parameters()).unwrap(),
    );
}

// ── call() delegation ────────────────────────────────────────────

#[tokio::test]
async fn call_with_valid_input_executes() {
    let tool = Sum { a: 2, b: 3 };
    let output = tool
        .call(serde_json::json!({"a": 2, "b": 3}), &context())
        .await;
    assert_eq!(output.is_error, None);
    assert_eq!(text_of(&output), "5");
}

#[tokio::test]
async fn call_passes_context_to_execute() {
    let tool = EchoSession {
        payload: "pong".to_string(),
    };
    let output = tool
        .call(serde_json::json!({"payload": "ping"}), &context())
        .await;
    // The deserialized struct (payload from input) and the context
    // (session id from the caller) both reach `execute`.
    assert_eq!(text_of(&output), "it-session:ping");
}

#[tokio::test]
async fn call_with_wrongly_typed_input_is_a_model_facing_error() {
    let tool = Sum { a: 0, b: 0 };
    let output = tool.call(serde_json::json!({"a": "two"}), &context()).await;
    assert_eq!(output.is_error, Some(true));
    assert!(
        text_of(&output).starts_with("Invalid arguments:"),
        "text = {}",
        text_of(&output)
    );
}

#[tokio::test]
async fn call_with_missing_field_is_a_model_facing_error() {
    let tool = Sum { a: 0, b: 0 };
    let output = tool.call(serde_json::json!({"a": 1}), &context()).await;
    assert_eq!(output.is_error, Some(true));
    assert!(
        text_of(&output).contains("missing field `b`"),
        "text = {}",
        text_of(&output)
    );
}

#[tokio::test]
async fn call_deserializes_enum_variants() {
    let tool = FormatFile {
        style: Style::Compact,
    };
    let output = tool
        .call(serde_json::json!({"style": "pretty"}), &context())
        .await;
    assert_eq!(output.is_error, None);
    assert_eq!(text_of(&output), "Pretty");
}

// ── object safety: the impl works through dyn Tool ───────────────

#[tokio::test]
async fn generated_impl_works_through_dyn_tool() {
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(Sum { a: 10, b: 20 }),
        Box::new(EchoSession {
            payload: "go".to_string(),
        }),
    ];
    let names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
    assert_eq!(names, vec!["sum_values", "echo_session"]);

    let output = tools[0]
        .call(serde_json::json!({"a": 1, "b": 2}), &context())
        .await;
    assert_eq!(text_of(&output), "3");
}
