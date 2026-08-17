use async_trait::async_trait;
use synthia_tool::{traits::Tool, types::*};

#[derive(Debug)]
pub struct FakeTool {
    pub name: String,
    pub description: String,
    pub output: String,
    pub call_count: std::sync::Arc<tokio::sync::Mutex<usize>>,
    pub should_fail: bool,
}

impl FakeTool {
    pub fn new(name: &str, output: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Fake tool: {}", name),
            output: output.to_string(),
            call_count: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
            should_fail: false,
        }
    }

    pub fn failing(name: &str, error_msg: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Fake tool: {}", name),
            output: error_msg.to_string(),
            call_count: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
            should_fail: true,
        }
    }
}

#[async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let mut count = self.call_count.lock().await;
        *count += 1;
        if self.should_fail {
            ToolOutput::error(&self.output)
        } else {
            ToolOutput::text(&self.output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_tool_call_count() {
        let tool = FakeTool::new("test", "output");
        let ctx =
            Context::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
        tool.call(serde_json::json!({}), &ctx).await;
        assert_eq!(*tool.call_count.lock().await, 1);
    }
}
