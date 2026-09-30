//! Minimal JSON-Schema subset validator for tool arguments.
//!
//! R12-3 (pi-subagents `src/structured-output.ts` parity).
//! The goal is NOT a full JSON-Schema implementation — the
//! LLM-facing tool schemas synthia generates (via `schemars`)
//! use a small, well-defined subset:
//!
//! - `type` (`"object"` | `"string"` | `"number"` |
//!   `"integer"` | `"boolean"` | `"array"`),
//! - `required` (array of property names, objects only),
//! - `properties` (nested schemas, objects only),
//! - `items` (element schema, arrays only).
//!
//! Anything else in the schema is ignored (permissive), because
//! unknown keywords must not reject valid input (JSON-Schema
//! spec: unknown keywords are annotations, not assertions).
//!
//! The validator returns field-level errors so the model gets
//! actionable feedback in the tool-result error path
//! (`ToolOutput::error`) and can self-correct in the same turn.

use serde_json::Value;

/// One validation failure. `path` is a dotted pointer to the
/// offending value (`"foo.bar[2]"` style) so the model can find
/// the exact field to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaViolation {
    /// Dotted path to the offending value ("" for the root).
    pub path: String,
    /// What was expected.
    pub expected: String,
    /// What was found (short form; never the full value — that
    /// could leak large payloads into the error path).
    pub found: String,
}

impl std::fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: expected {}, found {}",
            if self.path.is_empty() {
                "(root)"
            } else {
                &self.path
            },
            self.expected,
            self.found
        )
    }
}

/// Validate `value` against the JSON-Schema `schema`.
///
/// Returns `Ok(())` when the value satisfies every recognised
/// assertion; `Err(violations)` otherwise (all violations, not
/// just the first, so the model can fix everything in one turn).
///
/// An empty / non-object schema accepts anything (permissive
/// default — matches how the tool registry treats schemas).
pub fn validate_against_schema(
    schema: &Value,
    value: &Value,
) -> Result<(), Vec<SchemaViolation>> {
    let mut violations = Vec::new();
    validate_node(schema, value, "", &mut violations);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn validate_node(
    schema: &Value,
    value: &Value,
    path: &str,
    out: &mut Vec<SchemaViolation>,
) {
    let Some(obj) = schema.as_object() else {
        return; // Non-object schema = no assertions.
    };

    // `type` assertion.
    if let Some(Value::String(expected)) = obj.get("type")
        && !type_matches(expected, value)
    {
        out.push(SchemaViolation {
            path: path.to_string(),
            expected: format!("type {expected}"),
            found: type_of(value).to_string(),
        });
        // Type mismatch short-circuits further checks on
        // this node — nested checks assume the parent type.
        return;
    }

    // `required` + `properties` (object only).
    if value.is_object()
        && let Some(props) = obj.get("properties").and_then(Value::as_object)
    {
        let value_obj = value.as_object().expect("checked is_object");
        if let Some(Value::Array(required)) = obj.get("required") {
            for req in required {
                if let Some(name) = req.as_str()
                    && !value_obj.contains_key(name)
                {
                    out.push(SchemaViolation {
                        path: join_path(path, name),
                        expected: "required property present".to_string(),
                        found: "missing".to_string(),
                    });
                }
            }
        }
        for (name, sub_schema) in props {
            if let Some(sub_value) = value_obj.get(name) {
                validate_node(
                    sub_schema,
                    sub_value,
                    &join_path(path, name),
                    out,
                );
            }
            // Absent optional properties are fine; missing
            // required ones were flagged above.
        }
    }

    // `items` (array only).
    if value.is_array()
        && let Some(items_schema) = obj.get("items")
    {
        for (i, elem) in value
            .as_array()
            .expect("checked is_array")
            .iter()
            .enumerate()
        {
            validate_node(items_schema, elem, &format!("{}[{i}]", path), out);
        }
    }
}

fn join_path(base: &str, leaf: &str) -> String {
    if base.is_empty() {
        leaf.to_string()
    } else {
        format!("{base}.{leaf}")
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true, // Unknown type keyword: permissive.
    }
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_schema_accepts_anything() {
        assert!(validate_against_schema(&json!({}), &json!(42)).is_ok());
        assert!(
            validate_against_schema(&json!(null), &json!("anything")).is_ok()
        );
    }

    #[test]
    fn type_mismatch_reports_both_types() {
        let err =
            validate_against_schema(&json!({"type": "string"}), &json!(42))
                .unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].expected, "type string");
        assert_eq!(err[0].found, "integer");
    }

    #[test]
    fn missing_required_property_is_reported() {
        let schema = json!({
            "type": "object",
            "required": ["cmd"],
            "properties": {"cmd": {"type": "string"}}
        });
        let err = validate_against_schema(&schema, &json!({})).unwrap_err();
        assert!(err.iter().any(|v| v.path == "cmd"));
    }

    #[test]
    fn nested_property_paths_are_dotted() {
        let schema = json!({
            "type": "object",
            "properties": {
                "opts": {
                    "type": "object",
                    "properties": {"depth": {"type": "integer"}}
                }
            }
        });
        let err = validate_against_schema(
            &schema,
            &json!({"opts": {"depth": "not a number"}}),
        )
        .unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].path, "opts.depth");
    }

    #[test]
    fn array_items_validated_per_element() {
        let schema = json!({
            "type": "array",
            "items": {"type": "string"}
        });
        let err = validate_against_schema(
            &schema,
            &json!(["ok", 42, "also ok", false]),
        )
        .unwrap_err();
        assert_eq!(err.len(), 2);
        assert_eq!(err[0].path, "[1]");
        assert_eq!(err[1].path, "[3]");
    }

    #[test]
    fn integer_accepts_integers_not_floats() {
        assert!(
            validate_against_schema(&json!({"type": "integer"}), &json!(7))
                .is_ok()
        );
        assert!(
            validate_against_schema(&json!({"type": "integer"}), &json!(7.5))
                .is_err()
        );
        assert!(
            validate_against_schema(&json!({"type": "number"}), &json!(7.5))
                .is_ok()
        );
    }

    #[test]
    fn all_violations_reported_at_once() {
        let schema = json!({
            "type": "object",
            "required": ["a", "b"],
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "string"},
                "c": {"type": "integer"}
            }
        });
        let err = validate_against_schema(
            &schema,
            &json!({"a": 1, "c": "not an int"}),
        )
        .unwrap_err();
        // a has wrong type, b is missing, c has wrong type.
        assert_eq!(err.len(), 3);
    }

    #[test]
    fn valid_payload_passes() {
        let schema = json!({
            "type": "object",
            "required": ["cmd"],
            "properties": {
                "cmd": {"type": "string"},
                "timeout": {"type": "integer"}
            }
        });
        assert!(
            validate_against_schema(
                &schema,
                &json!({"cmd": "ls -la", "timeout": 30})
            )
            .is_ok()
        );
    }

    #[test]
    fn unknown_type_keyword_is_permissive() {
        assert!(
            validate_against_schema(
                &json!({"type": "custom-type"}),
                &json!("anything")
            )
            .is_ok()
        );
    }

    #[test]
    fn violation_display_includes_path_and_expectation() {
        let v = SchemaViolation {
            path: "opts.depth".to_string(),
            expected: "type integer".to_string(),
            found: "string".to_string(),
        };
        assert_eq!(
            v.to_string(),
            "opts.depth: expected type integer, found string"
        );
        let root = SchemaViolation {
            path: String::new(),
            expected: "type object".to_string(),
            found: "array".to_string(),
        };
        assert_eq!(
            root.to_string(),
            "(root): expected type object, found array"
        );
    }
}
