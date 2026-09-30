//! `SkillApplicationContext` — the runtime hook a skill uses to
//! return a **structured** response to the model.
//!
//! ## Motivation
//!
//! The [`crate::skill::format_skill_content`] path returns the skill
//! body as a wrapped markdown string. That's correct for
//! *declarative* skills (workflow descriptions the model reads and
//! follows), but it's the wrong shape for *procedural* skills
//! (skills that run code, query an API, or compose multiple tool
//! calls before producing a final answer).
//!
//! R9-3 adds an opt-in `apply()` lifecycle: a skill may declare
//! that it wants to *execute* on a structured `SkillRequest` and
//! return a structured [`SkillResponse`]. The runtime wraps the
//! response in the existing `<skill_content>` envelope so the
//! LLM-facing surface is unchanged; only the in-process shape
//! becomes richer.
//!
//! ## Layering
//!
//! ```text
//! SkillTool::execute({"name": "...", "args": {...}})
//!   │
//!   ▼
//! SkillApplicationContext::apply(skill, request, ctx)
//!   │
//!   ▼
//! SkillResponse { content: ContentPart, structured: Option<Value> }
//! ```
//!
//! ## Reference
//!
//! Adopted from dsh `packages/skill/skill/src/index.ts`:
//! `SkillDefinition` carries an `invocation` policy that decides
//! model-vs-user invocation; `SkillRegistration` lets runtime
//! callers register ad-hoc skills with structured I/O. Synthia's
//! port preserves the conservative `Skill` value type (declarative
//! skills still work untouched) and adds a parallel
//! `SkillApplication` opt-in layer that runtime-registered
//! procedural skills can plug into.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One content part the skill can return to the model.
///
/// Mirrors `synthia_provider::ContentPart` semantically (text /
/// image / tool-use / tool-result) but stays a **lib-local enum**
/// so `synthia-skill` doesn't pull in `synthia-provider`. The
/// runtime layer is responsible for converting
/// `SkillResponseContent::Text(s)` into the provider's
/// `TextContent` before sending the next prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillResponseContent {
    /// Plain text returned to the model (most common path).
    Text {
        /// The text body.
        text: String,
    },
    /// A code-block-formatted text payload. Renders identically
    /// to `Text` but signals "this is a structured data block".
    Json {
        /// Pre-formatted JSON text (the runtime does NOT
        /// re-validate; the skill is responsible for emitting
        /// well-formed JSON).
        json: String,
    },
    /// Composite response: multiple text blocks the model should
    /// treat as one logical response. Used by skills that want to
    /// separate "I read X" / "I did Y" / "the answer is Z" without
    /// returning a single glued string.
    Blocks {
        /// Ordered list of text blocks.
        blocks: Vec<String>,
    },
}

impl SkillResponseContent {
    /// Flatten any variant into a single string the existing
    /// `<skill_content>` envelope can wrap. Mirrors
    /// `synthia_provider::ContentPart`'s `as_text` semantics.
    pub fn as_text(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Json { json } => format!("```json\n{json}\n```"),
            Self::Blocks { blocks } => blocks.join("\n\n"),
        }
    }
}

impl From<String> for SkillResponseContent {
    fn from(text: String) -> Self {
        Self::Text { text }
    }
}

impl From<&str> for SkillResponseContent {
    fn from(text: &str) -> Self {
        Self::Text {
            text: text.to_string(),
        }
    }
}

/// A structured request the runtime hands to a procedural skill.
///
/// `name` mirrors the `Skill::name` lookup; `args` is the
/// caller-supplied argument bag (typically the JSON object the
/// model passes alongside the skill invocation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRequest {
    /// Skill name (matches the registered `Skill::name`).
    pub name: String,
    /// Caller-supplied argument bag.
    #[serde(default)]
    pub args: HashMap<String, Value>,
}

impl SkillRequest {
    /// Convenience constructor for the common "no args" case.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: HashMap::new(),
        }
    }

    /// Builder — adds (or overwrites) one argument.
    pub fn with_arg(
        mut self,
        key: impl Into<String>,
        value: impl Into<Value>,
    ) -> Self {
        self.args.insert(key.into(), value.into());
        self
    }
}

/// A structured response from a procedural skill.
///
/// `content` is what the model reads (always wrapped in the
/// `<skill_content>` envelope by the runtime). `structured` is an
/// optional machine-readable payload the host process can consume
/// without parsing the text body — useful for skills that emit
/// numbers / IDs / status flags alongside their prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillResponse {
    /// Model-facing content. Required.
    pub content: SkillResponseContent,
    /// Optional machine-readable payload (skipped by the runtime
    /// when `None`).
    pub structured: Option<Value>,
}

impl SkillResponse {
    /// Build a text-only response.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: text.into().into(),
            structured: None,
        }
    }

    /// Build a structured response with both human + machine parts.
    pub fn structured(
        content: impl Into<SkillResponseContent>,
        structured: impl Into<Value>,
    ) -> Self {
        Self {
            content: content.into(),
            structured: Some(structured.into()),
        }
    }
}

/// Errors a procedural skill can surface.
#[derive(Debug, thiserror::Error)]
pub enum SkillApplicationError {
    /// The skill is registered as a procedural skill but the
    /// application function returned `None` (the trait doesn't
    /// declare an `apply` body for this name).
    #[error("skill '{0}' has no apply() handler")]
    NotHandled(String),
    /// The skill received a malformed `SkillRequest` (missing
    /// required arg, wrong type, …).
    #[error("invalid skill request: {0}")]
    InvalidRequest(String),
    /// The application itself raised an error (network failure,
    /// panic, …). The runtime wraps the message into a
    /// `SkillResponse::text(error_message)` so the model sees a
    /// graceful error rather than a crash.
    #[error("skill '{skill}' execution failed: {message}")]
    Execution {
        /// Skill name.
        skill: String,
        /// Underlying error message.
        message: String,
    },
}

/// A typed handle for a single procedural skill.
///
/// `SkillApplication` is a thin pointer to an async function that
/// takes a [`SkillRequest`] and a [`SkillApplicationContext`] and
/// returns a [`SkillResponse`] (or an error the runtime converts
/// to a graceful model-facing response).
///
/// `SkillApplication` is `Send + Sync + 'static` so the registry
/// can hold it next to the value-type [`crate::skill::Skill`].
pub type SkillApplication = Arc<
    dyn for<'a> Fn(
            &'a SkillRequest,
            &'a SkillApplicationContext,
        ) -> std::pin::Pin<
            Box<
                dyn Future<
                        Output = Result<SkillResponse, SkillApplicationError>,
                    > + Send
                    + 'a,
            >,
        > + Send
        + Sync,
>;

/// Per-invocation context the runtime hands to a procedural skill.
///
/// `SkillApplicationContext` carries the runtime-supplied
/// dependencies a skill needs (cancel token, scoped settings) but
/// **not** the model / provider / tool registry. Skills that need
/// to call back into the model should use a dedicated MCP-style
/// bridge registered alongside the skill, not the
/// `SkillApplicationContext` (which exists to keep skills lego-style
/// pluggable without dragging the agent runtime into their
/// dependency surface).
#[derive(Clone)]
pub struct SkillApplicationContext {
    /// Optional cancel token the runtime may set when a session
    /// is being torn down (lib consumers building their own
    /// runtime should pass `Some(cancel_token)` from the
    /// `AgentRunConfig`).
    cancel: Option<Arc<dyn crate_synthia_core_cancel::CancelToken>>,
    /// Skill-scoped settings (key-value bag the caller can hand
    /// to the skill without changing its public API).
    settings: HashMap<String, Value>,
}

/// Local re-export of the cancel-token trait so the skill crate
/// doesn't pull in `synthia-core` directly. `crate_synthia_core_cancel`
/// is a private module that re-exports `synthia_core::CancelToken`.
mod crate_synthia_core_cancel {
    // Re-export the trait so lib consumers can compose against
    // it without importing synthia-core just to write a skill.
    pub use synthia_core::CancelToken;
}

impl std::fmt::Debug for SkillApplicationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillApplicationContext")
            .field("cancel", &self.cancel.as_ref().map(|_| "CancelToken"))
            .field("settings", &self.settings.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for SkillApplicationContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillApplicationContext {
    /// Empty context — used by tests and by callers that don't
    /// need cancel / settings plumbing.
    pub fn new() -> Self {
        Self {
            cancel: None,
            settings: HashMap::new(),
        }
    }

    /// Attach a cancel token (typically the
    /// `Arc<dyn CancelToken>` from `AgentRunConfig`).
    pub fn with_cancel(
        mut self,
        cancel: Arc<dyn crate_synthia_core_cancel::CancelToken>,
    ) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Attach a settings key-value bag.
    pub fn with_settings(mut self, settings: HashMap<String, Value>) -> Self {
        self.settings = settings;
        self
    }

    /// Add (or overwrite) a single settings key.
    pub fn set(
        mut self,
        key: impl Into<String>,
        value: impl Into<Value>,
    ) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }

    /// True if a cancel token is attached and the token reports
    /// cancelled. Skills should poll this between long-running
    /// steps and bail out with `SkillApplicationError::Execution`
    /// when it returns `true`.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|t| t.is_cancelled())
    }

    /// Look up a settings key.
    pub fn get_setting(&self, key: &str) -> Option<&Value> {
        self.settings.get(key)
    }

    /// Look up a settings key as `&str`. Returns `None` if the
    /// key is absent or its value isn't a string.
    pub fn get_setting_str(&self, key: &str) -> Option<&str> {
        self.settings.get(key).and_then(|v| v.as_str())
    }
}

/// Builder for [`SkillApplication`] that closes over the
/// request-handling async closure without lifetime gymnastics.
///
/// ```ignore
/// use synthia_skill::{SkillApplication, SkillRequest, SkillApplicationContext, SkillResponse};
///
/// let apply = SkillApplication::new(|req: &SkillRequest, _ctx| async move {
///     SkillResponse::text(format!("hello {}", req.name))
/// });
/// ```
pub struct SkillApplicationBuilder;

impl SkillApplicationBuilder {
    /// Wrap a `Fn(&SkillRequest, &SkillApplicationContext) -> impl Future`
    /// closure in a `SkillApplication` (boxed, `Send + Sync`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new<F>(handler: F) -> SkillApplication
    where
        F: for<'a> Fn(
                &'a SkillRequest,
                &'a SkillApplicationContext,
            ) -> std::pin::Pin<
                Box<
                    dyn Future<
                            Output = Result<
                                SkillResponse,
                                SkillApplicationError,
                            >,
                        > + Send
                        + 'a,
                >,
            > + Send
            + Sync
            + 'static,
    {
        Arc::new(handler)
    }
}

/// A procedural-skill handle: a `Skill` value paired with an
/// [`SkillApplication`] handler.
///
/// Bundling the two lets the runtime route
/// `SkillTool::execute({"name": ...})` through either path:
///
/// - If the registry finds a [`crate::skill::Skill`] but no
///   [`RegisteredSkillApplication`], it falls back to the legacy
///   `<skill_content>` envelope (declarative skill).
/// - If the registry finds a `RegisteredSkillApplication`, it
///   invokes the closure and wraps the structured response in the
///   same envelope.
#[derive(Clone)]
pub struct RegisteredSkillApplication {
    /// Skill name (matches the handler it ships with).
    pub name: String,
    /// Human-readable description surfaced through the
    /// `<available_skills>` prompt block.
    pub description: Option<String>,
    /// The async handler.
    pub apply: SkillApplication,
}

impl std::fmt::Debug for RegisteredSkillApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisteredSkillApplication")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("apply", &"SkillApplication")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_request_with_arg_builds_a_bag() {
        let req = SkillRequest::new("summarize")
            .with_arg("path", "/tmp/x.md")
            .with_arg("max_words", 100);
        assert_eq!(
            req.args.get("path").unwrap(),
            &Value::String("/tmp/x.md".to_string())
        );
        assert_eq!(req.args.get("max_words").unwrap(), &Value::from(100));
    }

    #[test]
    fn skill_response_text_renders_through_as_text() {
        let resp = SkillResponse::text("hi");
        assert_eq!(resp.content.as_text(), "hi");
        assert!(resp.structured.is_none());
    }

    #[test]
    fn skill_response_json_wraps_in_code_fence() {
        let resp = SkillResponse::text("```json\n{\"k\":1}\n```");
        // The as_text path just returns the text verbatim.
        assert_eq!(resp.content.as_text(), "```json\n{\"k\":1}\n```");
    }

    #[test]
    fn blocks_response_joins_with_blank_line() {
        let content = SkillResponseContent::Blocks {
            blocks: vec!["first".to_string(), "second".to_string()],
        };
        assert_eq!(content.as_text(), "first\n\nsecond");
    }

    #[test]
    fn application_context_defaults_have_no_cancel() {
        let ctx = SkillApplicationContext::new();
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn application_context_settings_round_trip() {
        let ctx = SkillApplicationContext::new()
            .set("model", "stub")
            .set("max_tokens", 256);
        assert_eq!(ctx.get_setting_str("model"), Some("stub"));
        assert_eq!(ctx.get_setting("max_tokens"), Some(&Value::from(256)));
    }

    #[tokio::test]
    async fn skill_application_invokes_the_closure() {
        let apply: SkillApplication =
            SkillApplicationBuilder::new(|req, _ctx| {
                Box::pin(async move {
                    Ok(SkillResponse::structured(
                        SkillResponseContent::Text {
                            text: format!("hello {}", req.name),
                        },
                        serde_json::json!({ "ok": true }),
                    ))
                })
            });
        let req = SkillRequest::new("summarize");
        let ctx = SkillApplicationContext::new();
        let resp = (apply)(&req, &ctx).await.expect("apply succeeds");
        assert_eq!(resp.content.as_text(), "hello summarize");
        assert_eq!(resp.structured, Some(serde_json::json!({ "ok": true })));
    }

    #[tokio::test]
    async fn skill_application_can_surface_an_execution_error() {
        let apply: SkillApplication =
            SkillApplicationBuilder::new(|req, _ctx| {
                Box::pin(async move {
                    Err(SkillApplicationError::Execution {
                        skill: req.name.clone(),
                        message: "simulated failure".to_string(),
                    })
                })
            });
        let req = SkillRequest::new("broken");
        let ctx = SkillApplicationContext::new();
        let err = (apply)(&req, &ctx).await.expect_err("must error");
        let formatted = format!("{err}");
        assert!(formatted.contains("broken"));
        assert!(formatted.contains("simulated failure"));
    }

    #[test]
    fn registered_skill_application_debug_redacts_closure() {
        let apply: SkillApplication = SkillApplicationBuilder::new(|_, _| {
            Box::pin(async { Ok(SkillResponse::text("ok")) })
        });
        let reg = RegisteredSkillApplication {
            name: "x".to_string(),
            description: Some("desc".to_string()),
            apply,
        };
        let formatted = format!("{reg:?}");
        assert!(formatted.contains("x"));
        assert!(formatted.contains("desc"));
        // The closure is redacted as a marker string, not a
        // pointer / function pointer.
        assert!(formatted.contains("SkillApplication"));
    }
}
