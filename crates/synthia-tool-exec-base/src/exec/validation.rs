use serde_json::Value;

/// Full JSON Schema validation using the jsonschema crate.
/// Validates the complete input against the schema including types, enums, ranges, etc.
#[cfg(feature = "jsonschema-validation")]
pub fn validate_parameters(
    input: &Value,
    schema: &Value,
) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| format!("Invalid JSON schema: {e}"))?;

    let result = validator.validate(input);
    if result.is_ok() {
        Ok(())
    } else {
        let messages: Vec<_> = validator
            .iter_errors(input)
            .map(|e| format!("{} at {}", e, e.instance_path))
            .collect();
        Err(format!("Validation failed: {}", messages.join("; ")))
    }
}

/// Fallback validation: only checks for required field presence.
/// Used when the jsonschema-validation feature is disabled.
#[cfg(not(feature = "jsonschema-validation"))]
pub fn validate_parameters(
    input: &Value,
    schema: &Value,
) -> Result<(), String> {
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for field in required {
            if let Some(name) = field.as_str()
                && input.get(name).is_none()
            {
                return Err(format!("Missing required parameter: {}", name));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_required_parameter() {
        let input = serde_json::json!({"name": "test"});
        let schema = serde_json::json!({"required": ["name", "count"]});
        let result = validate_parameters(&input, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("count"));
    }

    #[test]
    fn test_validate_all_parameters_present() {
        let input = serde_json::json!({"name": "test", "count": 5});
        let schema = serde_json::json!({"required": ["name", "count"]});
        assert!(validate_parameters(&input, &schema).is_ok());
    }

    #[test]
    fn test_type_validation_mismatch() {
        let _schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string" }
            }
        });
        let _input = serde_json::json!({ "command": 123 });
        #[cfg(feature = "jsonschema-validation")]
        {
            let result = validate_parameters(&_input, &_schema);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("command"));
        }
    }

    #[test]
    fn test_enum_validation_invalid() {
        let _schema = serde_json::json!({
            "type": "object",
            "required": ["mode"],
            "properties": {
                "mode": { "type": "string", "enum": ["read", "write"] }
            }
        });
        let _input = serde_json::json!({ "mode": "delete" });
        #[cfg(feature = "jsonschema-validation")]
        {
            let result = validate_parameters(&_input, &_schema);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_range_validation() {
        let _schema = serde_json::json!({
            "type": "object",
            "required": ["count"],
            "properties": {
                "count": { "type": "integer", "minimum": 1, "maximum": 100 }
            }
        });
        let _input = serde_json::json!({ "count": 0 });
        #[cfg(feature = "jsonschema-validation")]
        {
            let result = validate_parameters(&_input, &_schema);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_additional_properties_rejected() {
        let _schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "additionalProperties": false,
            "properties": {
                "command": { "type": "string" }
            }
        });
        let _input = serde_json::json!({ "command": "echo", "extra": "field" });
        #[cfg(feature = "jsonschema-validation")]
        {
            let result = validate_parameters(&_input, &_schema);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_valid_input_passes() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string" }
            }
        });
        let input = serde_json::json!({ "command": "echo hello" });
        let result = validate_parameters(&input, &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_required_field_missing() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string" }
            }
        });
        let input = serde_json::json!({ "other": "field" });
        let result = validate_parameters(&input, &schema);
        assert!(result.is_err());
    }
}
