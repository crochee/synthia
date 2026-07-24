// ── Tool Definitions ──────────────────────────────────────────────────────

pub fn load_skill_tool_definition() -> synthia_provider::types::ToolDefinition {
    synthia_provider::types::ToolDefinition::new(
        "load_skill",
        "Load a skill's full instructions into context. Use when you need detailed guidance from a skill listed in Available Skills.",
        serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name (must appear in Available Skills list)"
                }
            }
        }),
    )
}

pub fn unload_skill_tool_definition() -> synthia_provider::types::ToolDefinition
{
    synthia_provider::types::ToolDefinition::new(
        "unload_skill",
        "Remove a previously loaded skill from context to free up tokens.",
        serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The skill name to unload"
                }
            }
        }),
    )
}
