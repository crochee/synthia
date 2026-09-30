//! R74 JSON-Schema argument validation switch.
//!
//! Five tests exercise:
//! - validation off: arguments pass through to the tool body
//! - validation on: a schema violation synthesises a dotted-path
//!   `is_error` Result
//! - validation on: a well-formed input still runs the tool
//! - `Clone` must carry the `validate_arguments` flag
//! - empty / non-object schemas are permissive
//!
//! `use super::*;` brings in the parent's types (ToolRegistry,
//! ToolEntry, TestEntryTool, collect_results, etc.).

use async_trait;

use super::*;

/// A test tool that takes a typed argument (a string `cmd` and
/// an integer `timeout`), so the validation tests can exercise
/// type / required / nested rules.
#[derive(Debug)]
struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run a shell command with a timeout."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["cmd"],
            "properties": {
                "cmd": {"type": "string"},
                "timeout": {"type": "integer"},
            },
        })
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(format!("ran: {input}"))
    }
}

/// Default behaviour is unchanged: a tool with a non-matching
/// input runs anyway. The `ShellTool` body is the source of
/// truth for argument validity when validation is off.
#[tokio::test]
async fn dispatch_passes_arguments_to_the_tool_when_validation_is_off() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool)));
    assert!(!registry.argument_validation_enabled());

    // Missing required `cmd` and a wrong-typed `timeout` — both
    // would fail validation, but the registry does not validate
    // by default, so the tool body runs.
    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({"timeout": "not an int"}),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_ne!(results[0].1.is_error, Some(true));
}

/// When `with_argument_validation(true)` is set, an input that
/// violates the tool's schema synthesises an `is_error` Result
/// listing the dotted-path violations. The tool body never runs.
#[tokio::test]
async fn dispatch_synthesises_schema_violation_error_when_validation_is_on() {
    let registry = ToolRegistry::new().with_argument_validation(true);
    assert!(registry.argument_validation_enabled());
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool)));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "shell".to_string(),
                // Missing required `cmd` + `timeout` is the
                // wrong type.
                input: serde_json::json!({"timeout": "not an int"}),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    let (call_id, output) = &results[0];
    assert_eq!(call_id, "call-1");
    assert_eq!(
        output.is_error,
        Some(true),
        "a validation failure must mark the result is_error"
    );
    let body = output
        .content
        .iter()
        .find_map(|p| match p {
            synthia_provider::ContentPart::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(body.contains("invalid arguments for tool `shell`"));
    assert!(
        body.contains("cmd:") && body.contains("required"),
        "the missing-`cmd` violation must surface, got: {body}"
    );
    assert!(
        body.contains("timeout:") && body.contains("integer"),
        "the wrong-typed `timeout` violation must surface, got: {body}"
    );
}

/// When validation is on, a well-formed input still runs the
/// tool (the validation must not over-reject).
#[tokio::test]
async fn dispatch_runs_the_tool_when_validation_passes() {
    let registry = ToolRegistry::new().with_argument_validation(true);
    registry.register_entry(ToolEntry::new(Arc::new(ShellTool)));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({
                    "cmd": "ls -la",
                    "timeout": 30,
                }),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0].1.is_error,
        Some(true),
        "a well-formed call must succeed; got: {:?}",
        results[0].1
    );
}

/// The cloning path: `Clone` must carry the validation flag
/// through, otherwise a deployment that builds a child registry
/// from a validated parent would silently lose the guarantee.
#[test]
fn clone_preserves_argument_validation_flag() {
    let registry = ToolRegistry::new().with_argument_validation(true);
    let clone = registry.clone();
    assert!(
        clone.argument_validation_enabled(),
        "Clone must carry the validate_arguments flag"
    );

    let off = ToolRegistry::new();
    let off_clone = off.clone();
    assert!(!off_clone.argument_validation_enabled());
}

/// An empty / non-object schema is permissive (`validate_against_schema`
/// returns `Ok(())`). The registry must not over-reject on an
/// under-specified tool.
#[tokio::test]
async fn validation_passes_through_empty_schemas() {
    // `TestEntryTool` returns `{"type": "object", "properties": {}}`
    // — an object with no `required`, which accepts anything.
    let registry = ToolRegistry::new().with_argument_validation(true);
    registry.register_entry(ToolEntry::new(Arc::new(TestEntryTool)));

    let results = collect_results(
        registry.run_stream(
            vec![synthia_provider::ToolUse {
                id: "call-1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({
                    "anything": "goes",
                    "more": 42,
                }),
            }],
            Context::new("s1".to_string(), PathBuf::from("/tmp")),
        ),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0].1.is_error,
        Some(true),
        "an empty schema must accept extra fields; got: {:?}",
        results[0].1
    );
}
