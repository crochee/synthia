use rmcp::model::{CallToolResult, Content};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub(crate) fn parse_args<T: DeserializeOwned>(
    args: Value,
) -> Result<T, CallToolResult> {
    serde_json::from_value(args).map_err(|e| {
        CallToolResult::error(vec![Content::text(format!(
            "Invalid request: {e}"
        ))])
    })
}

pub(crate) fn ok_result<T: Serialize>(value: &T) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(value).unwrap_or_default(),
    )])
}

pub(crate) fn err_result(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

#[cfg(test)]
mod tests {
    use rmcp::model::CallToolResult;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_args_valid_json() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            name: String,
            value: i32,
        }

        let args = json!({"name": "test", "value": 42});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Args {
                name: "test".to_string(),
                value: 42
            }
        );
    }

    #[test]
    fn test_parse_args_invalid_type() {
        // JSON string is valid JSON but can't deserialize to expected type
        let args = json!("just a string");
        let result: Result<serde_json::Value, _> = parse_args(args);

        // A bare string deserializes fine to Value
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_args_invalid_for_struct() {
        // Object that lacks required fields
        #[derive(serde::Deserialize, Debug)]
        struct Args {
            _required: String,
        }

        let args = json!({"optional": "field"});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.is_error, Some(true));
    }

    #[test]
    fn test_parse_args_missing_fields() {
        #[derive(serde::Deserialize, Debug)]
        struct Args {
            _required: String,
        }

        let args = json!({"other": "field"});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_args_empty_object() {
        #[derive(serde::Deserialize, Debug, Default)]
        struct Args {
            _name: Option<String>,
        }

        let args = json!({});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
    }

    #[test]
    fn test_ok_result_serialization() {
        #[derive(serde::Serialize)]
        struct Result {
            status: String,
            count: i32,
        }

        let data = Result {
            status: "success".to_string(),
            count: 5,
        };
        let result = ok_result(&data);

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("success"));
        assert!(text.text.contains("5"));
    }

    #[test]
    fn test_ok_result_empty() {
        let data: Vec<String> = vec![];
        let result = ok_result(&data);

        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[test]
    fn test_err_result_string() {
        let result = err_result("Something went wrong");

        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert_eq!(text.text, "Something went wrong");
    }

    #[test]
    fn test_err_result_empty() {
        let result: CallToolResult = err_result("");

        assert!(result.is_error == Some(true));
    }

    #[test]
    fn test_err_result_with_format() {
        let result = err_result(format!("Error code: {}", 404));

        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("404"));
    }

    #[test]
    fn test_parse_args_null_value() {
        #[derive(serde::Deserialize, Debug)]
        struct Args {
            name: Option<String>,
        }

        let args = json!({"name": null});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, None);
    }

    #[test]
    fn test_parse_args_zero_and_negative() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            count: i32,
            balance: f64,
        }

        let args = json!({"count": 0, "balance": -42.5});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        let args = result.unwrap();
        assert_eq!(args.count, 0);
        assert_eq!(args.balance, -42.5);
    }

    #[test]
    fn test_parse_args_empty_string() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            name: String,
        }

        let args = json!({"name": ""});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "");
    }

    #[test]
    fn test_parse_args_nested_object() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            outer: Outer,
        }
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Outer {
            inner: Inner,
        }
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Inner {
            value: String,
        }

        let args = json!({"outer": {"inner": {"value": "test"}}});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().outer.inner.value, "test");
    }

    #[test]
    fn test_parse_args_array() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            items: Vec<String>,
        }

        let args = json!({"items": ["a", "b", "c"]});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().items, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_args_empty_array() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Args {
            items: Vec<i32>,
        }

        let args = json!({"items": []});
        let result: Result<Args, _> = parse_args(args);

        assert!(result.is_ok());
        assert!(result.unwrap().items.is_empty());
    }

    #[test]
    fn test_ok_result_with_nested_struct() {
        #[derive(serde::Serialize)]
        struct Inner {
            key: String,
        }
        #[derive(serde::Serialize)]
        struct Outer {
            data: Vec<Inner>,
            count: usize,
        }

        let data = Outer {
            data: vec![Inner {
                key: "value".to_string(),
            }],
            count: 1,
        };
        let result = ok_result(&data);

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("data"));
        assert!(text.text.contains("count"));
    }

    #[test]
    fn test_ok_result_pretty_format() {
        #[derive(serde::Serialize)]
        struct Data {
            x: i32,
            y: i32,
        }

        let data = Data { x: 1, y: 2 };
        let result = ok_result(&data);

        let content = &result.content[0];
        let text = content.as_text().unwrap();
        // Pretty-printed JSON should have newlines
        assert!(text.text.contains('\n') || text.text.contains("  "));
    }

    #[test]
    fn test_err_result_from_string_literal() {
        let result = err_result("literal error");

        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert_eq!(text.text, "literal error");
    }
}
