use serde_json::Value;
use std::collections::HashMap;

pub fn build_code_agent_prompt(
    tools: &[crate::clients::ToolDefinition],
    system_prompt: Option<String>,
) -> String {
    let tools_section = if tools.is_empty() {
        "You have no tools available.".to_string()
    } else {
        let tool_descriptions: Vec<String> = tools
            .iter()
            .map(|t| {
                format!(
                    "- {}: {}",
                    t.name,
                    t.description
                )
            })
            .collect();
        format!(
            "You have access to the following tools:\n{}\n\nWhen you need to use a tool, respond with a JSON object in the following format:\n{{\"tool\": \"<tool_name>\", \"parameters\": <parameters_json>}}",
            tool_descriptions.join("\n")
        )
    };

    let default_prompt = format!(
        r#"You are an expert AI programming assistant that helps with software development tasks.

## Your Capabilities
- Reading, writing, and analyzing code
- Running shell commands
- File system operations
- Code search and analysis

## Guidelines
1. Think step by step before taking action
2. Use tools efficiently - read files before writing, search before creating
3. Maintain code quality and follow best practices
4. Explain your reasoning and the actions you're taking

## Workflow
1. Understand the user's request
2. Plan your approach
3. Execute actions using tools
4. Verify results
5. Iterate as needed

## Important Notes
- Always use absolute paths for file operations
- Check file existence before reading
- Create necessary directories before writing files
- Handle errors gracefully and provide informative messages

{}

## Response Format
You should think about the problem step by step, then take action using tools when needed. After receiving tool results, analyze them and continue until the task is complete.

When you need to use a tool, respond with:
```
TOOL_CALL: <tool_name>: <arguments_json>
```

When you have completed the task or need to respond to the user:
```
FINAL: <your response>
```"#,
        tools_section
    );

    match system_prompt {
        Some(custom) if !custom.is_empty() => custom,
        _ => default_prompt,
    }
}

pub fn build_step_prompt(step_number: usize, total_steps: usize) -> String {
    format!(
        r#"Step {}/{}: What is your next thought and action?"#,
        step_number, total_steps
    )
}

pub fn build_observation_prompt(observation: &str) -> String {
    format!(
        r#"Observation: {}

Based on this observation, what is your next thought and action?"#,
        observation
    )
}

pub fn build_tool_result_prompt(tool_name: &str, result: &Value) -> String {
    format!(
        r#"Tool '{}' returned:
{}

What is your next thought and action?"#,
        tool_name,
        serde_json::to_string_pretty(result).unwrap_or_default()
    )
}

pub fn build_final_response_prompt(completed: bool) -> String {
    if completed {
        r#"Task completed! Please provide a summary of what was accomplished."#
    } else {
        r#"Please provide your final response or next action."#
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_code_agent_prompt_with_tools() {
        let tools = vec![
            crate::clients::ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({}),
            },
            crate::clients::ToolDefinition {
                name: "write_file".to_string(),
                description: "Write a file".to_string(),
                parameters: serde_json::json!({}),
            },
        ];

        let prompt = build_code_agent_prompt(&tools, None);

        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("TOOL_CALL:"));
        assert!(prompt.contains("FINAL:"));
    }

    #[test]
    fn test_build_code_agent_prompt_empty_tools() {
        let tools = vec![];
        let prompt = build_code_agent_prompt(&tools, None);

        assert!(prompt.contains("no tools available"));
    }

    #[test]
    fn test_build_code_agent_prompt_custom_system() {
        let tools = vec![];
        let custom_prompt = "You are a custom assistant.".to_string();
        let prompt = build_code_agent_prompt(&tools, Some(custom_prompt.clone()));

        assert_eq!(prompt, custom_prompt);
    }
}
