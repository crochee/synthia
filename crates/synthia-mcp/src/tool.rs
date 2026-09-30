//! [`McpTool`] — a remote MCP tool exposed as a local
//! [`synthia_tool::Tool`].

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use synthia_core::Error;
use synthia_tool::{
    Context,
    Tool,
    ToolOutput,
    output::{RenderKind, ToolOutputDefinition},
    traits::ExecutionMode,
};

use crate::client::{McpCallResult, McpClient, McpToolSpec};

/// One remote MCP tool, adapted to the local tool contract.
///
/// `description` / `parameters` come from the server's
/// `tools/list` entry; `call` forwards to `tools/call` and maps
/// the result blocks to a [`ToolOutput`].
///
/// The registry-facing name is the *raw* server name unless a
/// public (namespaced) name was set via
/// [`McpTool::with_public_name`] — `tools/call` always targets
/// the raw name on the wire.
pub struct McpTool {
    spec: McpToolSpec,
    client: Arc<McpClient>,
    /// Namespaced registry name (`None` → use the raw spec name).
    public_name: Option<String>,
}

impl McpTool {
    /// Wrap `spec` with the client that owns its server; the
    /// tool is registered under its raw name.
    #[must_use]
    pub fn new(spec: McpToolSpec, client: Arc<McpClient>) -> Self {
        Self {
            spec,
            client,
            public_name: None,
        }
    }

    /// Wrap `spec`, registering it under `public_name` (see
    /// [`crate::public_tool_name`]) while `tools/call` keeps
    /// targeting the raw wire name.
    #[must_use]
    pub fn with_public_name(
        spec: McpToolSpec,
        client: Arc<McpClient>,
        public_name: String,
    ) -> Self {
        Self {
            spec,
            client,
            public_name: Some(public_name),
        }
    }

    /// The remote spec this tool was built from.
    #[must_use]
    pub fn spec(&self) -> &McpToolSpec {
        &self.spec
    }

    /// The registry-facing name (public name when set, else the
    /// raw server name).
    #[must_use]
    pub fn public_name(&self) -> &str {
        self.public_name.as_deref().unwrap_or(&self.spec.name)
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        self.public_name()
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn parameters(&self) -> Value {
        self.spec.input_schema.clone()
    }

    fn mode(&self) -> ExecutionMode {
        // Remote calls go out over the server's transport; treat
        // as sequential so the orchestrator does not interleave
        // them with sibling mutating tools.
        ExecutionMode::Sequential
    }

    fn output_definition(&self) -> ToolOutputDefinition {
        // Remote tools have no local render contract; the JSON
        // kind tells a UI to fall back to structured rendering.
        ToolOutputDefinition::passthrough(self.public_name())
            .with_kind(RenderKind::Json)
            .with_title(format!("MCP: {}", self.public_name()))
            .with_presentation(
                "mcp_server",
                Value::String(self.client.server_name()),
            )
    }

    async fn call(&self, input: Value, _context: &Context) -> ToolOutput {
        let result = match self.client.tools_call(&self.spec.name, input).await
        {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!(
                    "MCP tool `{}` failed: {e}",
                    self.public_name()
                ));
            }
        };
        output_from_result(
            &result,
            self.public_name(),
            &self.client.server_name(),
        )
    }
}

/// Render one `tools/call` outcome for the model.
///
/// The single implementation of the remote-result → [`ToolOutput`]
/// projection. [`McpTool`] (one remote tool pre-registered as a local
/// tool) and [`crate::McpControlTool`] (the dynamic `mcp` tool) differ
/// only in how they *address* a call, so the multimodal mapping — and
/// the `isError` handling — lives here once.
///
/// `display` names the call the way the model wrote it (a local public
/// name, or `<server>/<tool>`); `server` is stamped as the
/// `mcp_server` presentation hint.
pub(crate) fn output_from_result(
    result: &McpCallResult,
    display: &str,
    server: &str,
) -> ToolOutput {
    let text = result.text();
    if result.is_error {
        return ToolOutput::error(if text.is_empty() {
            format!("MCP tool `{display}` reported an error")
        } else {
            text
        });
    }
    let parts = result.parts();
    let all_text = parts
        .iter()
        .all(|p| matches!(p, synthia_provider::types::ContentPart::Text(_)));
    // A text-only reply keeps the compact string shape, but the
    // string is derived from the *projected* parts so an enriched
    // placeholder (a binary kind we do not model) survives; anything
    // multimodal travels as parts so the image/audio payload reaches
    // the provider.
    let output = if all_text {
        let joined = parts
            .iter()
            .filter_map(|p| p.text())
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutput::text(joined)
    } else {
        ToolOutput::from_parts(parts)
    };
    output.with_metadata("mcp_server", Value::String(server.to_string()))
}

/// Convenience: an [`Error`] for a tool-name collision (used by
/// callers that want to refuse to shadow a local tool).
#[must_use]
pub fn collision_error(name: &str) -> Error {
    Error::ToolExecution {
        message: format!("MCP tool `{name}` collides with an existing tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_flattens_blocks_with_placeholders() {
        let result = crate::client::McpCallResult {
            content: vec![
                crate::client::McpContentBlock {
                    kind: "text".into(),
                    text: Some("hello".into()),
                    extra: serde_json::Map::new(),
                },
                crate::client::McpContentBlock {
                    kind: "image".into(),
                    text: None,
                    extra: serde_json::Map::new(),
                },
            ],
            is_error: false,
        };
        assert_eq!(result.text(), "hello\n[image content]");
    }

    #[test]
    fn text_of_empty_content_is_empty() {
        let result = crate::client::McpCallResult {
            content: Vec::new(),
            is_error: false,
        };
        assert_eq!(result.text(), "");
    }

    #[test]
    fn collision_error_names_the_tool() {
        let e = collision_error("read");
        assert!(e.to_string().contains("read"));
    }
}
