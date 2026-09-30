//! [`Action`] — the tool-agnostic vocabulary every [`Guard`] sees.
//!
//! The agent loop classifies a raw [`ToolUse`] into one of these
//! variants before the guard pipeline runs, so a guard never has to
//! know individual tool schemas: a workspace-boundary guard matches
//! [`Action::FileWrite`] whether the write came from `write`,
//! a future `edit` tool, or anything else that mutates files.
//!
//! Classification is name-based (see [`Action::from_tool_use`]) and
//! deliberately lossy: unknown tools stay [`Action::ToolCall`] with
//! their raw arguments, so guards that only inspect names/arguments
//! still run.
//!
//! [`Guard`]: crate::Guard

use std::path::PathBuf;

use serde_json::{Value, json};
use synthia_provider::ToolUse;

/// One outgoing agent action, classified by effect rather than by
/// tool identity.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A tool invocation whose effect class is unknown.
    ToolCall { name: String, arguments: Value },
    /// Execution of a shell command.
    ShellCommand {
        command: String,
        timeout_secs: Option<u64>,
    },
    /// Creation or mutation of a file.
    FileWrite { path: PathBuf, content: String },
    /// An outbound HTTP request.
    HttpRequest { url: String, method: String },
    /// Delegation of work to a sub-agent.
    AgentDelegation { agent: String, prompt: String },
    /// Raw content about to enter the model context (e.g. a tool
    /// result). Reserved for output-side guards.
    RawOutput { content: String },
}

impl Action {
    /// Classify a wire tool call into an [`Action`].
    ///
    /// Known built-in tool names (`shell`, `write`, `web_fetch`,
    /// `task`) map to their specific variant with the interesting
    /// argument promoted to a typed field; everything else stays a
    /// generic [`Action::ToolCall`].
    pub fn from_tool_use(call: &ToolUse) -> Self {
        match call.name.as_str() {
            "shell" | "bash" => Self::ShellCommand {
                command: string_field(&call.input, "command")
                    .unwrap_or_default(),
                timeout_secs: u64_field(&call.input, "timeout_secs"),
            },
            "write" | "write_file" | "edit" => Self::FileWrite {
                path: string_field(&call.input, "file_path")
                    .or_else(|| string_field(&call.input, "path"))
                    .unwrap_or_default()
                    .into(),
                content: string_field(&call.input, "content")
                    .or_else(|| string_field(&call.input, "new_string"))
                    .unwrap_or_default(),
            },
            "web_fetch" | "fetch" | "http_request" => Self::HttpRequest {
                url: string_field(&call.input, "url").unwrap_or_default(),
                method: string_field(&call.input, "method")
                    .unwrap_or_else(|| "GET".to_string())
                    .to_ascii_uppercase(),
            },
            "task" => Self::AgentDelegation {
                agent: string_field(&call.input, "agent").unwrap_or_default(),
                prompt: string_field(&call.input, "prompt").unwrap_or_default(),
            },
            _ => Self::ToolCall {
                name: call.name.clone(),
                arguments: call.input.clone(),
            },
        }
    }

    /// Rebuild a wire tool call from this action, preserving the
    /// original call's identity (`id`). Used by the agent loop to
    /// adopt a guard-sanitized action: the sanitized name +
    /// arguments replace the original, everything else is copied.
    pub fn apply_to(&self, call: &ToolUse) -> ToolUse {
        let (name, arguments) = self.to_name_arguments();
        ToolUse {
            id: call.id.clone(),
            name,
            input: arguments,
        }
    }

    /// Stable fingerprint of this action: tool name plus
    /// key-canonicalised arguments. Two calls with the same name
    /// and semantically equal arguments (regardless of key order)
    /// produce the same fingerprint — this is what loop detection
    /// compares.
    pub fn fingerprint(&self) -> String {
        let (name, arguments) = self.to_name_arguments();
        tool_fingerprint(&name, &arguments)
    }

    /// Short human-readable summary for log lines and deny
    /// messages. Long payloads are elided so a denial stays
    /// readable in the transcript.
    pub fn summary(&self) -> String {
        const MAX: usize = 120;
        let raw = match self {
            Self::ToolCall { name, arguments } => {
                format!("{name} {arguments}")
            }
            Self::ShellCommand { command, .. } => format!("shell {command}"),
            Self::FileWrite { path, content } => {
                format!("write {} ({} bytes)", path.display(), content.len())
            }
            Self::HttpRequest { url, method } => format!("{method} {url}"),
            Self::AgentDelegation { agent, prompt } => {
                format!("task -> {agent}: {prompt}")
            }
            Self::RawOutput { content } => format!("output {content}"),
        };
        if raw.chars().count() <= MAX {
            raw
        } else {
            let truncated: String = raw.chars().take(MAX).collect();
            format!("{truncated}…")
        }
    }

    /// Decompose into the wire shape (tool name + argument object).
    fn to_name_arguments(&self) -> (String, Value) {
        match self {
            Self::ToolCall { name, arguments } => {
                (name.clone(), arguments.clone())
            }
            Self::ShellCommand {
                command,
                timeout_secs,
            } => (
                "shell".to_string(),
                json!({"command": command, "timeout_secs": timeout_secs}),
            ),
            Self::FileWrite { path, content } => (
                "write".to_string(),
                json!({"file_path": path.display().to_string(), "content": content}),
            ),
            Self::HttpRequest { url, method } => (
                "web_fetch".to_string(),
                json!({"url": url, "method": method}),
            ),
            Self::AgentDelegation { agent, prompt } => (
                "task".to_string(),
                json!({"agent": agent, "prompt": prompt}),
            ),
            Self::RawOutput { content } => {
                ("raw_output".to_string(), json!({"content": content}))
            }
        }
    }
}

/// Canonical fingerprint of a tool call: `name` + argument object
/// serialised with recursively sorted keys. Key-order independence
/// matters because providers do not guarantee argument key order
/// across retries.
pub fn tool_fingerprint(name: &str, arguments: &Value) -> String {
    format!("{name}:{}", canonical_json(arguments))
}

/// Serialise a [`Value`] with recursively sorted object keys so
/// equal-by-semantics values produce equal strings.
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{:?}:{}", k, canonical_json(&map[k])))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", body.join(","))
        }
        Value::String(s) => format!("{s:?}"),
        other => other.to_string(),
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, input: Value) -> ToolUse {
        ToolUse {
            id: "t1".to_string(),
            name: name.to_string(),
            input,
        }
    }

    /// Known tool names MUST classify into their specific variant
    /// with the typed field promoted.
    #[test]
    fn classifies_known_tools() {
        let shell = Action::from_tool_use(&call(
            "shell",
            json!({"command": "ls", "timeout_secs": 5}),
        ));
        assert_eq!(
            shell,
            Action::ShellCommand {
                command: "ls".to_string(),
                timeout_secs: Some(5),
            }
        );

        let write = Action::from_tool_use(&call(
            "write",
            json!({"file_path": "a.txt", "content": "hi"}),
        ));
        assert_eq!(
            write,
            Action::FileWrite {
                path: PathBuf::from("a.txt"),
                content: "hi".to_string(),
            }
        );

        let task = Action::from_tool_use(&call(
            "task",
            json!({"agent": "coder", "prompt": "do it"}),
        ));
        assert_eq!(
            task,
            Action::AgentDelegation {
                agent: "coder".to_string(),
                prompt: "do it".to_string(),
            }
        );
    }

    /// Unknown tools MUST stay generic `ToolCall` with raw args.
    #[test]
    fn unknown_tool_stays_generic() {
        let a = Action::from_tool_use(&call("custom", json!({"x": 1})));
        assert_eq!(
            a,
            Action::ToolCall {
                name: "custom".to_string(),
                arguments: json!({"x": 1}),
            }
        );
    }

    /// Fingerprints MUST be key-order independent and name
    /// sensitive.
    #[test]
    fn fingerprint_is_key_order_independent() {
        let a =
            tool_fingerprint("t", &json!({"a": 1, "b": [2, {"y": 1, "x": 2}]}));
        let b =
            tool_fingerprint("t", &json!({"b": [2, {"x": 2, "y": 1}], "a": 1}));
        assert_eq!(a, b);
        assert_ne!(a, tool_fingerprint("u", &json!({"a": 1, "b": 2})));
    }

    /// `apply_to` MUST preserve the original id and adopt the
    /// action's name/arguments.
    #[test]
    fn apply_to_preserves_id_and_adopts_action() {
        let action = Action::ShellCommand {
            command: "ls -la".to_string(),
            timeout_secs: None,
        };
        let rebuilt =
            action.apply_to(&call("shell", json!({"command": "rm -rf /"})));
        assert_eq!(rebuilt.id, "t1");
        assert_eq!(rebuilt.name, "shell");
        assert_eq!(rebuilt.input["command"], "ls -la");
    }

    /// Summaries MUST elide long payloads.
    #[test]
    fn summary_elides_long_content() {
        let action = Action::RawOutput {
            content: "x".repeat(500),
        };
        let summary = action.summary();
        assert!(summary.chars().count() <= 122);
        assert!(summary.ends_with('…'));
    }
}
