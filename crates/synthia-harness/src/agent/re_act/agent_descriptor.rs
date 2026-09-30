//! Descriptor-mutation setters on [`super::ReActAgent`].
//!
//! Every setter in this file rewrites a field on the agent's
//! [`AgentDescriptor`]. They do not change the loop, the tools,
//! the steering, or the runtime — just the wire-level metadata
//! the harness advertises to the model and to clients.
//!
//! [`AgentDescriptor`]: crate::agent::AgentDescriptor

use std::sync::Arc;

use super::ReActAgent;
use crate::prompt::PromptContext;

impl ReActAgent {
    /// Replace the agent's display name (the registry / routing
    /// slug and the `<identity>` block in the system prompt).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.descriptor.name = name.into();
        self
    }

    /// Replace the agent's instructions (the body of the
    /// `<instructions>` block in the system prompt).
    #[must_use]
    pub fn with_instructions(
        mut self,
        instructions: impl Into<String>,
    ) -> Self {
        self.descriptor.instructions = instructions.into();
        self
    }

    /// Replace the agent's model hint (the `<model_hint>` block
    /// the model sees, and the value a deployment may echo back
    /// to clients on the `AgentCard`).
    #[must_use]
    pub fn with_model_hint(mut self, hint: Option<String>) -> Self {
        self.descriptor.model_hint = hint;
        self
    }

    /// Declare a structured-output JSON Schema for this agent.
    /// Auto-registers the
    /// [`synthia_tool::structured_output_tool`] (name
    /// `structured_output`) into the tool registry so the model
    /// can submit a schema-validated final answer; the schema
    /// string also rides on the descriptor for the wire surface.
    ///
    /// Auto-injection is LIFO so the caller's pre-registered
    /// `structured_output` (if any) still wins name lookups.
    #[must_use]
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.tool_registry
            .register_entry(synthia_tool::ToolEntry::new(
                synthia_tool::structured_output_tool(schema.clone()),
            ));
        self.descriptor.output_schema = Some(schema.to_string());
        self
    }

    /// Replace the prompt context. Useful when registries are
    /// populated after construction.
    pub fn set_prompt_context(&mut self, ctx: PromptContext) {
        self.prompt_context = Arc::new(ctx);
    }

    /// Borrow the current prompt context.
    pub fn prompt_context(&self) -> &PromptContext {
        &self.prompt_context
    }

    /// Replace the descriptor used by the prompt assembler.
    pub fn descriptor_mut(
        &mut self,
        descriptor: crate::agent::AgentDescriptor,
    ) {
        self.descriptor = descriptor;
    }
}
