//! The LLM-facing `task` tool wire definition.
//!
//! `agent` + `prompt` are always required. `gate` is always
//! visible; `isolation` is feature-gated via
//! [`ToolSchemaBuilder::with_feature`] so a deployment that
//! disables `worktree_isolation` in its [`ToolFeatures`]
//! removes the property entirely from the LLM-facing schema —
//! the model never sees a description for a refused
//! capability.

use serde_json::{Map, Value, json};
use synthia_provider::ToolDefinition;
use synthia_tool::{ToolFeatures, ToolSchemaBuilder};

use super::spec::TASK_TOOL_NAME;

/// The `task` tool definition with every feature off:
/// `isolation` is dropped so the model never sees a refused
/// capability.
#[must_use]
pub fn task_tool_definition() -> ToolDefinition {
    task_tool_definition_with_features(&ToolFeatures::none())
}

/// Schema-aware variant of [`task_tool_definition`]: when
/// `features` enables `worktree_isolation`, the `isolation`
/// property is kept; otherwise it is dropped (it is never in
/// `required`, so nothing else shifts) so the model never
/// sees it.
#[must_use]
pub fn task_tool_definition_with_features(
    features: &ToolFeatures,
) -> ToolDefinition {
    // `agent` + `prompt` only: `gate` and `isolation` are
    // optional knobs and never land in `required`.
    let schema =
        build_final_schema(features, vec![json!("agent"), json!("prompt")]);
    ToolDefinition {
        name: TASK_TOOL_NAME.to_string(),
        description: "Delegate a sub-task to a registered peer agent \
                      and return its final answer. Use for work that \
                      belongs to a specialist (per each agent's \
                      handoff hint) or that should run in isolation."
            .to_string(),
        input_schema: schema,
        cache_control: None,
        annotations: None,
    }
}

fn build_final_schema(features: &ToolFeatures, required: Vec<Value>) -> Value {
    let mut props = Map::new();
    props.insert(
        "agent".to_string(),
        json!({
            "type": "string",
            "description": "Name of the registered peer agent to run."
        }),
    );
    props.insert(
        "prompt".to_string(),
        json!({
            "type": "string",
            "description": "The complete, self-contained task \
                            for the sub-agent."
        }),
    );
    props.insert(
        "gate".to_string(),
        json!({
            "type": "object",
            "description": "Optional verification command run \
                            against the child's working tree after \
                            it finishes.",
            "properties": {
                "command": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}}
                    ]
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1
                }
            },
            "required": ["command"]
        }),
    );
    if features.is_enabled("worktree_isolation") {
        props.insert(
            "isolation".to_string(),
            json!({
                "type": "boolean",
                "description": "Run the child in a fresh detached \
                                git worktree. Edits are committed to \
                                `synthia/agent-<ulid>` on cleanup when \
                                the worktree is dirty."
            }),
        );
    }

    ToolSchemaBuilder::new()
        .with("type", json!("object"))
        .with("properties", Value::Object(props))
        .with("required", Value::Array(required))
        .build()
}
