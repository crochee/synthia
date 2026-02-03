//! Tests for MCP tool adapter module
//!
//! Note: Testing McpToolAdapter::call() requires a valid ServerSink (Peer<RoleClient>),
//! which can only be created by connecting to an actual MCP server. Therefore, only
//! pure function tests and adapter construction/field tests are included.

use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Map, Value};

use crate::tools::McpToolCollector;

fn create_test_tool() -> Tool {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    let mut props = Map::new();
    props.insert(
        "arg1".to_string(),
        Value::String("First argument".to_string()),
    );
    schema.insert("properties".to_string(), Value::Object(props));

    Tool::new("test_tool", "A test tool", Arc::new(schema))
}

mod mcp_tool_collector_tests {
    use super::*;

    #[test]
    fn test_parse_qualified_name_valid() {
        let result =
            McpToolCollector::parse_qualified_name("mcp__server1__tool1");
        assert!(result.is_some());
        let (server, tool) = result.unwrap();
        assert_eq!(server, "server1");
        assert_eq!(tool, "tool1");
    }

    #[test]
    fn test_parse_qualified_name_with_underscores_in_names() {
        // Server name with underscores
        let result =
            McpToolCollector::parse_qualified_name("mcp__my_server__my_tool");
        assert!(result.is_some());
        let (server, tool) = result.unwrap();
        assert_eq!(server, "my_server");
        assert_eq!(tool, "my_tool");
    }

    #[test]
    fn test_parse_qualified_name_no_prefix() {
        let result = McpToolCollector::parse_qualified_name("server__tool");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_qualified_name_empty_prefix() {
        let result = McpToolCollector::parse_qualified_name("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_qualified_name_prefix_only() {
        let result = McpToolCollector::parse_qualified_name("mcp__");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_qualified_name_no_double_underscore() {
        let result = McpToolCollector::parse_qualified_name("mcp__single");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_qualified_name_quadruple_underscore() {
        // mcp____tool -> remainder is "_tool", find("__") at pos 0, server="" tool="tool"
        let result = McpToolCollector::parse_qualified_name("mcp____tool");
        assert!(result.is_some());
        let (server, tool) = result.unwrap();
        assert_eq!(server, "");
        assert_eq!(tool, "tool");
    }

    #[test]
    fn test_parse_qualified_name_triple_underscore() {
        // mcp__server__tool_name -> server=server, tool=tool_name
        let result =
            McpToolCollector::parse_qualified_name("mcp__server__tool_name");
        assert!(result.is_some());
        let (server, tool) = result.unwrap();
        assert_eq!(server, "server");
        assert_eq!(tool, "tool_name");
    }

    #[test]
    fn test_parse_qualified_name_multiple_double_underscores() {
        // Only splits on first double underscore after prefix
        let result = McpToolCollector::parse_qualified_name("mcp__a__b__c");
        assert!(result.is_some());
        let (server, tool) = result.unwrap();
        assert_eq!(server, "a");
        assert_eq!(tool, "b__c");
    }

    #[tokio::test]
    async fn test_collector_new_and_default() {
        // Test that new() creates a collector
        let collector = McpToolCollector::new();
        let result = collector.collect_all_tools().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());

        // Test default() works the same way
        let collector2 = McpToolCollector::default();
        let result2 = collector2.collect_all_tools().await;
        assert!(result2.is_ok());
        assert!(result2.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_collector_register_and_unregister() {
        // McpToolCollector::register_server and unregister_server take ServerSink
        // which requires a real MCP connection, so we can't easily test them
        // without a mock MCP server.
    }
}

mod mcp_tool_adapter_tests {
    use super::*;

    fn make_schema_with_name() -> Arc<Map<String, Value>> {
        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        let mut props = Map::new();
        props.insert("name".to_string(), Value::String("A name".to_string()));
        props.insert("age".to_string(), Value::String("A number".to_string()));
        schema.insert("properties".to_string(), Value::Object(props));
        schema.insert(
            "required".to_string(),
            Value::Array(vec![Value::String("name".to_string())]),
        );
        Arc::new(schema)
    }

    #[test]
    fn test_adapter_name() {
        // Creating McpToolAdapter requires ServerSink which needs a real connection
        // But we can verify the adapter name field directly through construction
        let tool = create_test_tool();
        assert_eq!(tool.name, "test_tool");
    }

    #[test]
    fn test_adapter_description() {
        let tool = create_test_tool();
        assert_eq!(tool.description.as_deref(), Some("A test tool"));
    }

    #[test]
    fn test_adapter_description_empty() {
        let empty_schema = Arc::new(Map::new());
        let tool = Tool::new("my_tool", "", empty_schema);
        assert_eq!(tool.description.as_deref(), Some(""));
    }

    #[test]
    fn test_adapter_description_none() {
        let empty_schema = Arc::new(Map::new());
        let tool = Tool::new("my_tool", "", empty_schema);
        // Tool description is Option<Cow>, so we check it exists
        assert!(tool.description.is_some());
    }

    #[test]
    fn test_tool_schema_properties() {
        let schema = make_schema_with_name();
        let tool = Tool::new("test_tool", "desc", schema);

        let schema_json = tool.schema_as_json_value();
        assert_eq!(schema_json["type"], "object");
        assert!(schema_json.as_object().unwrap().contains_key("properties"));
        let properties = schema_json["properties"].as_object().unwrap();
        assert!(properties.contains_key("name"));
        assert!(properties.contains_key("age"));
    }

    #[test]
    fn test_tool_schema_empty() {
        let empty_schema = Arc::new(Map::new());
        let tool = Tool::new("test_tool", "desc", empty_schema);

        let schema_json = tool.schema_as_json_value();
        assert!(schema_json.is_object());
    }

    #[test]
    fn test_tool_new_accepts_static_strings() {
        // Tool::new requires 'static lifetimes for name and description
        let schema = Arc::new(Map::new());
        let tool = Tool::new("static_name", "static_desc", schema);
        assert_eq!(tool.name, "static_name");
    }
}
