//! Sub-agent delegation — the `task` tool seam.
//!
//! Fills in the multi-agent delegation contract documented on
//! the crate root: a parent agent's model invokes the `task`
//! tool once per sub-agent; `ReActLoop`
//! intercepts those calls before normal tool dispatch, resolves
//! the named peer from the [`synthia_harness::AgentRegistry`],
//! runs it as a child, forwards every child event wrapped in
//! [`synthia_harness::AgentEvent::Agent`] +
//! [`synthia_harness::AgentMeta`], and commits the child's final text as the tool
//! result.
//!
//! # Why interception instead of a registered tool
//!
//! The `task` primitive is not a tool: it needs the parent
//! loop's event sink (to forward the child trace), the parent's
//! cancellation token, and per-run depth bookkeeping — none of
//! which the [`synthia_tool::Tool`] contract carries. Routing it
//! through `execute_tool_inner` keeps the `ToolRegistry`
//! untouched while giving the model one uniform surface (it
//! just sees a `task` tool definition in its tool list).
//!
//! # Depth guard
//!
//! Child runs receive `AgentInput::subagent_depth = parent + 1`.
//! Delegation refuses to spawn at
//! [`MAX_SUBAGENT_DEPTH`] — a misbehaving planner cannot
//! recurse forever.
//!
//! # Verification gate + worktree isolation
//!
//! (Slice D, R30.) `TaskSpec` optionally carries a
//! [`crate::gate::GateSpec`] and / or a [`crate::worktree::WorktreeSpec`]. When either is
//! present, `run_subagent` takes responsibility for the
//! lifecycle:
//!
//! - With a worktree: create a temp git worktree off the base
//!   cwd at `HEAD`, run the child against the worktree's path,
//!   clean up after the child (clean ⇒ remove; dirty ⇒ commit
//!   on the reserved `synthia/agent-<ulid>` branch + remove).
//! - With a gate: run a verification command from the
//!   worktree (when present) or the base cwd otherwise; pass ⇒
//!   child text becomes the tool result; anything else ⇒
//!   `ToolOutput::error("failed gate: …")`.
//!
//! All subprocess execution goes through the
//! [`crate::gate::CommandRunner`] trait
//! so the production path uses `std::process::Command` while
//! the integration tests use a recording fake.
//!
//! ## Module layout
//!
//! - `spec` — [`TaskSpec`] parsing (+ [`TASK_TOOL_NAME`] /
//!   [`MAX_SUBAGENT_DEPTH`]): what a `task` call means.
//! - `schema` — [`task_tool_definition`] /
//!   [`task_tool_definition_with_features`]: the LLM-facing
//!   wire definition, feature-gated per deployment.
//! - `delegator` — [`TaskDelegator`]: the interceptor that
//!   turns a `task` tool call into a depth-capped child run.
//! - `runner` — `run_subagent`(+`_with_runner`): drives one
//!   child, forwards its events, applies the gate + worktree
//!   lifecycle, and commits the final text.

mod delegator;
mod runner;
mod schema;
mod spec;

pub use delegator::TaskDelegator;
pub use schema::{task_tool_definition, task_tool_definition_with_features};
pub use spec::{MAX_SUBAGENT_DEPTH, TASK_TOOL_NAME, TaskSpec};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};
    use synthia_harness::{AgentRegistry, ToolInterceptor};
    use synthia_tool::ToolFeatures;

    use super::*;

    #[test]
    fn task_spec_parses_agent_and_prompt() {
        let spec = TaskSpec::from_value(
            &json!({"agent": "coder", "prompt": "fix it"}),
        )
        .expect("parse");
        assert_eq!(spec.agent, "coder");
        assert_eq!(spec.prompt, "fix it");
        assert!(spec.gate.is_none());
        assert!(spec.isolation.is_none());
    }

    #[test]
    fn task_spec_prompt_defaults_to_empty() {
        let spec =
            TaskSpec::from_value(&json!({"agent": "coder"})).expect("parse");
        assert_eq!(spec.prompt, "");
    }

    #[test]
    fn task_spec_rejects_missing_and_empty_agent() {
        assert!(TaskSpec::from_value(&json!({"prompt": "x"})).is_err());
        assert!(TaskSpec::from_value(&json!({"agent": ""})).is_err());
        assert!(TaskSpec::from_value(&json!({})).is_err());
    }

    #[test]
    fn task_spec_parses_gate_string_shape() {
        let spec = TaskSpec::from_value(&json!({
            "agent": "coder",
            "prompt": "fix it",
            "gate": "make test"
        }))
        .expect("parse");
        let gate = spec.gate.expect("gate");
        assert_eq!(gate.command, vec!["make test".to_string()]);
    }

    #[test]
    fn task_spec_parses_gate_object_shape() {
        let spec = TaskSpec::from_value(&json!({
            "agent": "coder",
            "prompt": "fix it",
            "gate": {
                "command": ["cargo", "test", "-p", "synthia-harness"],
                "timeout_secs": 60
            }
        }))
        .expect("parse");
        let gate = spec.gate.expect("gate");
        assert_eq!(
            gate.command,
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "synthia-harness".to_string()
            ]
        );
        assert_eq!(gate.timeout, Some(std::time::Duration::from_secs(60)));
    }

    #[test]
    fn task_spec_parses_isolation_flag() {
        let spec = TaskSpec::from_value(&json!({
            "agent": "coder",
            "prompt": "fix it",
            "isolation": true
        }))
        .expect("parse");
        assert!(spec.isolation.is_some());
    }

    #[test]
    fn task_spec_omits_optional_when_null() {
        let spec = TaskSpec::from_value(&json!({
            "agent": "coder",
            "prompt": "fix it",
            "gate": null,
            "isolation": null
        }))
        .expect("parse");
        assert!(spec.gate.is_none());
        assert!(spec.isolation.is_none());
    }

    #[test]
    fn task_spec_rejects_gate_with_bad_command() {
        let err = TaskSpec::from_value(&json!({
            "agent": "coder",
            "gate": {"command": []}
        }))
        .expect_err("bad");
        assert!(err.contains("gate.command"));
    }

    #[test]
    fn task_tool_definition_has_required_schema_shape() {
        let def = task_tool_definition();
        assert_eq!(def.name, "task");
        let required = def
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required array");
        assert!(required.contains(&json!("agent")));
        assert!(required.contains(&json!("prompt")));
        let props = def
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("properties");
        assert!(props.contains_key("agent"));
        assert!(props.contains_key("prompt"));
        assert!(props.contains_key("gate"));
        // Default (no features): `isolation` is dropped from the
        // schema so the model never sees a refused capability.
        assert!(!props.contains_key("isolation"));
    }

    #[test]
    fn task_tool_definition_feature_on_advertises_isolation() {
        let features = ToolFeatures::none().with("worktree_isolation");
        let def = task_tool_definition_with_features(&features);
        let props = def
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("properties");
        assert!(
            props.contains_key("isolation"),
            "feature-on must keep isolation property"
        );
        let required = def
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required");
        assert!(!required.contains(&json!("isolation")));
    }

    #[test]
    fn task_tool_definition_feature_off_drops_isolation() {
        let features = ToolFeatures::none();
        let def = task_tool_definition_with_features(&features);
        let props = def
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("properties");
        assert!(
            !props.contains_key("isolation"),
            "feature-off must drop isolation property"
        );
        let required = def
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required");
        assert!(!required.contains(&json!("isolation")));
    }

    #[test]
    fn delegator_claims_only_task_and_advertises_one_definition() {
        let delegator = TaskDelegator::new(Arc::new(AgentRegistry::new()));
        assert!(delegator.claims("task"));
        assert!(!delegator.claims("shell"));
        let defs = delegator.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "task");
    }
}
