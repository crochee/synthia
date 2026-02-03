use rmcp::model::{CallToolResult, Content};
use serde::Serialize;

pub(crate) fn json_result<T: Serialize>(value: &T) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(value).unwrap_or_default(),
    )])
}

pub(crate) fn text_result(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_result_success() {
        #[derive(serde::Serialize)]
        struct Data<'a> {
            name: &'a str,
            value: i32,
        }

        let data = Data {
            name: "test",
            value: 42,
        };
        let result = json_result(&data);

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("test"));
        assert!(text.text.contains("42"));
    }

    #[test]
    fn test_json_result_empty() {
        let data: Vec<String> = vec![];
        let result = json_result(&data);

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("[]"));
    }

    #[test]
    fn test_text_result_string() {
        let result = text_result("hello world");

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert_eq!(text.text, "hello world");
    }

    #[test]
    fn test_text_result_from_str() {
        let result = text_result(String::from("dynamic"));

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert_eq!(text.text, "dynamic");
    }

    #[test]
    fn test_text_result_empty() {
        let result = text_result("");

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.is_empty());
    }
}
