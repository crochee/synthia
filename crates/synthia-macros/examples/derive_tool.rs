//! `#[derive(Tool)]` end to end: one derive turns an argument struct
//! into a complete `synthia_tool::Tool` impl.
//!
//! What to look at:
//!
//! - `parameters()` — the JSON Schema is generated from the struct's
//!   `schemars::JsonSchema` derive; the `///` doc comments on the
//!   fields become field descriptions.
//! - `name()` / `description()` — `name` comes from
//!   `#[tool(name = "...")]`, `description` from the struct doc
//!   comment (the attribute could override it).
//! - the two `call`s — valid arguments land in the inherent
//!   `execute`, malformed arguments come back as a model-facing
//!   `ToolOutput::error("Invalid arguments: ...")` instead of a
//!   panic, so the model can retry with corrected arguments.
//!
//! Run: cargo run -p synthia-macros --example derive_tool

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use synthia_macros::Tool;
use synthia_tool::{Context, Tool, ToolOutput};

/// Multiplies two integers (description taken from this doc comment).
#[derive(Tool, Deserialize, JsonSchema)]
#[tool(name = "multiply", mode = "sequential")]
struct Multiply {
    /// Left factor.
    left: i64,
    /// Right factor.
    right: i64,
}

impl Multiply {
    async fn execute(&self, _context: &Context) -> ToolOutput {
        ToolOutput::text((self.left * self.right).to_string())
    }
}

/// Text of a `ToolOutput` via its JSON projection (`ContentPart`
/// lives behind `synthia-provider`, which this crate does not depend
/// on directly).
fn text_of(output: &ToolOutput) -> String {
    let value =
        serde_json::to_value(output).expect("ToolOutput is serializable");
    value["content"]
        .as_array()
        .expect("ToolOutput content is an array")
        .iter()
        .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
        .collect()
}

#[tokio::main]
async fn main() {
    let tool = Multiply { left: 6, right: 7 };
    let context =
        Context::new("derive-tool-demo".to_string(), PathBuf::from("."));

    println!("name:        {}", tool.name());
    println!("description: {}", tool.description());
    println!(
        "parameters:\n{}",
        serde_json::to_string_pretty(&tool.parameters())
            .expect("schema serializes")
    );

    let valid = tool
        .call(serde_json::json!({"left": 6, "right": 7}), &context)
        .await;
    println!(
        "call(valid):   is_error={:?} text={:?}",
        valid.is_error,
        text_of(&valid)
    );

    let invalid = tool
        .call(serde_json::json!({"left": "six", "right": 7}), &context)
        .await;
    println!(
        "call(invalid): is_error={:?} text={:?}",
        invalid.is_error,
        text_of(&invalid)
    );

    println!("DERIVE-TOOL: OK");
}
