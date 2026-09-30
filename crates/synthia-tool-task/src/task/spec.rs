//! What one `task` tool call means: the parsed arguments and
//! the constants that bound the seam.

use serde_json::Value;

use crate::{
    gate::GateSpec,
    worktree::{self, WorktreeSpec},
};

/// The tool name the model calls to delegate to a peer agent.
pub const TASK_TOOL_NAME: &str = "task";

/// Maximum sub-agent nesting depth (0 = top-level session).
pub const MAX_SUBAGENT_DEPTH: usize = 3;

/// Parsed arguments of one `task` tool call.
///
/// The two core fields — `agent` + `prompt` — are required;
/// `gate` and `isolation` are optional knobs (R30 Slice D).
/// `gate` is always available; `isolation` is feature-gated at
/// the schema level via
/// `ToolFeatures::is_enabled("worktree_isolation")` so
/// deployments that opt out of the capability never advertise
/// it to the model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskSpec {
    /// Registry name of the peer agent to run.
    pub agent: String,
    /// The prompt handed to the child as its user input.
    pub prompt: String,
    /// Optional verification command run against the child's
    /// working tree after the child finishes.
    pub gate: Option<GateSpec>,
    /// Optional worktree-isolation marker. When `Some`, the
    /// child runs in a fresh detached worktree off the parent's
    /// base cwd.
    pub isolation: Option<WorktreeSpec>,
}
impl TaskSpec {
    /// Parse the tool-call JSON arguments.
    ///
    /// `agent` must be a non-empty string; `prompt` defaults to
    /// an empty string (a child may legitimately be asked to act
    /// on its own instructions). `gate` and `isolation` are
    /// parsed when present; missing / null leaves the
    /// corresponding field as `None`.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let Some(agent) = value.get("agent").and_then(Value::as_str) else {
            return Err("task tool requires a string `agent` field".to_string());
        };
        if agent.is_empty() {
            return Err("`agent` must be a non-empty agent name".to_string());
        };
        let prompt = value
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let gate = parse_gate_field(value.get("gate"))?;
        let isolation = worktree::parse_worktree_field(value.get("isolation"));
        Ok(Self {
            agent: agent.to_string(),
            prompt: prompt.to_string(),
            gate,
            isolation,
        })
    }
}

/// Parse the optional `gate` object out of a tool-call payload.
///
/// Accepts three shapes:
/// - `null` / missing ⇒ `Ok(None)` (no gate).
/// - `{"command": ["cargo", "test"], "timeout_secs": 60}` ⇒
///   `Ok(Some(GateSpec { … }))`.
/// - `"make test"` (bare string) ⇒ `Ok(Some(GateSpec::from(...)))`
///   for backwards compatibility with the simpler shape.
/// - Anything else ⇒ `Err("gate: …")`.
fn parse_gate_field(value: Option<&Value>) -> Result<Option<GateSpec>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    if let Some(s) = value.as_str() {
        return Ok(Some(GateSpec::from(s)));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| "gate must be a string or object".to_string())?;
    let command = obj
        .get("command")
        .ok_or_else(|| "gate object requires a `command` field".to_string())?;
    let argv: Vec<String> = match command {
        Value::String(s) => vec![s.clone()],
        Value::Array(arr) => arr
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    "gate command entries must be strings".to_string()
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("gate.command must be string or array".to_string()),
    };
    if argv.is_empty() {
        return Err(
            "gate.command must contain at least one argument".to_string()
        );
    }
    let timeout_secs = obj
        .get("timeout_secs")
        .and_then(Value::as_u64)
        .map(std::time::Duration::from_secs);
    let mut spec = GateSpec {
        command: argv,
        timeout: timeout_secs,
    };
    if let Some(t) = timeout_secs {
        spec = spec.with_timeout(t);
    }
    Ok(Some(spec))
}
