//! Free function [`resolve_attribute`] — translates a dotted
//! `subject.id` / `resource.name` / `environment.risk_score`
//! path into a `serde_json::Value`. Used by `AttributeEquals`
//! and `AttributeCompare` variants of
//! [`super::ConditionDefinition`].

use super::super::context::AccessRequest;

pub fn resolve_attribute(
    path: &str,
    request: &AccessRequest,
) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["subject", "id"] => Some(serde_json::json!(request.subject.id)),
        ["subject", "user_id"] => request
            .subject
            .user_id
            .as_ref()
            .map(|s| serde_json::json!(s)),
        ["subject", "session_id"] => request
            .subject
            .session_id
            .as_ref()
            .map(|s| serde_json::json!(s)),
        ["resource", "name"] => Some(serde_json::json!(request.resource.name)),
        ["resource", "type"] => request
            .resource
            .resource_type
            .as_ref()
            .map(|s| serde_json::json!(s)),
        ["resource", "owner"] => request
            .resource
            .owner
            .as_ref()
            .map(|s| serde_json::json!(s)),
        ["action", "name"] => Some(serde_json::json!(request.action.name)),
        ["action", "type"] => request
            .action
            .action_type
            .as_ref()
            .map(|s| serde_json::json!(s)),
        ["environment", "risk_score"] => {
            request.environment.risk_score.map(|s| serde_json::json!(s))
        }
        ["environment", "ip"] => request
            .environment
            .ip_address
            .as_ref()
            .map(|s| serde_json::json!(s)),
        _ => request.context.get(path).cloned(),
    }
}
