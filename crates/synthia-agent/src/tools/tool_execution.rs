use synthia_provider::types::ToolUse;
use synthia_tool::ToolOutput;

pub struct ToolExecution {
    pub tool_use: ToolUse,
    pub output: ToolOutput,
    pub is_error: bool,
}

pub fn normalize_tool_outputs(
    tool_calls: &[ToolUse],
    outputs: Vec<ToolOutput>,
) -> Vec<ToolExecution> {
    tool_calls
        .iter()
        .zip(outputs.iter())
        .map(|(tu, output)| ToolExecution {
            tool_use: tu.clone(),
            output: output.clone(),
            is_error: output.is_error.unwrap_or(false),
        })
        .collect()
}

pub fn extract_output_text(output: &ToolOutput) -> String {
    output
        .content
        .iter()
        .filter_map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use synthia_provider::types::{ContentPart, TextContent};

    use super::*;

    #[test]
    fn test_extract_output_text() {
        let output = ToolOutput {
            content: vec![ContentPart::Text(TextContent {
                text: "Hello, World!".to_string(),
                cache_control: None,
            })],
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        };
        assert_eq!(extract_output_text(&output), "Hello, World!");
    }

    #[test]
    fn test_extract_output_text_multiple_parts() {
        let output = ToolOutput {
            content: vec![
                ContentPart::Text(TextContent {
                    text: "Line 1".to_string(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "Line 2".to_string(),
                    cache_control: None,
                }),
            ],
            is_error: Some(false),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        };
        assert_eq!(extract_output_text(&output), "Line 1\nLine 2");
    }

    #[test]
    fn test_normalize_tool_outputs() {
        let tool_calls = vec![
            ToolUse {
                id: "call-1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "/tmp/test.txt"}),
            },
            ToolUse {
                id: "call-2".to_string(),
                name: "write_file".to_string(),
                input: serde_json::json!({"path": "/tmp/out.txt"}),
            },
        ];
        let outputs = vec![
            ToolOutput {
                content: vec![ContentPart::Text(TextContent {
                    text: "file content".to_string(),
                    cache_control: None,
                })],
                is_error: Some(false),
                metadata: serde_json::Map::new(),
                truncated_by: None,
            },
            ToolOutput {
                content: vec![ContentPart::Text(TextContent {
                    text: "written".to_string(),
                    cache_control: None,
                })],
                is_error: Some(true),
                metadata: serde_json::Map::new(),
                truncated_by: None,
            },
        ];

        let executions = normalize_tool_outputs(&tool_calls, outputs);
        assert_eq!(executions.len(), 2);
        assert!(!executions[0].is_error);
        assert!(executions[1].is_error);
    }
}
