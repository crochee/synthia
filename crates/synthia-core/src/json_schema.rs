use schemars::{JsonSchema, schema_for};
use serde_json::Value;

pub fn generate_tool_schema<T: JsonSchema>() -> Value {
    let schema = schema_for!(T);
    serde_json::to_value(schema).unwrap_or_default()
}

pub fn generate_properties_schema<T: JsonSchema>() -> Value {
    let schema = generate_tool_schema::<T>();
    schema
        .get("properties")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;

    #[derive(Debug, Serialize, JsonSchema)]
    struct TestToolInput {
        name: String,
        count: i32,
    }

    #[test]
    fn test_generate_tool_schema() {
        let schema = generate_tool_schema::<TestToolInput>();
        assert!(schema.get("properties").is_some());
        let props = schema.get("properties").unwrap();
        assert!(props.get("name").is_some());
        assert!(props.get("count").is_some());
    }

    #[test]
    fn test_generate_properties_schema() {
        let props = generate_properties_schema::<TestToolInput>();
        assert!(props.get("name").is_some());
        assert!(props.get("count").is_some());
    }
}
