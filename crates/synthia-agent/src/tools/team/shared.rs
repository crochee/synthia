// Re-use err_result from tools::shared
pub(crate) use crate::tools::shared::err_result;

#[cfg(test)]
mod tests {
    use rmcp::model::CallToolResult;

    use super::err_result;

    #[test]
    fn test_err_result_creates_error_response() {
        let result: CallToolResult = err_result("test error");
        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert_eq!(text.text, "test error");
    }

    #[test]
    fn test_err_result_with_format() {
        let result: CallToolResult = err_result(format!("code: {}", 42));
        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("42"));
    }
}
