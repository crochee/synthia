//! `OperationRequest` — the discriminated-union wire type for the
//! operation endpoint.
//!
//! Adopted from pi's `OperationRequest` sum type
//! (`prompt | skill | prompt_template | compaction | navigation`).
//! Synthia ships the same vocabulary on a single endpoint
//! (`POST /api/v1/chat/sessions/{id}/operation`) so a client can
//! express “do this thing with the session” without the server
//! growing one route per operation kind.
//!
//! ## Status (R29)
//!
//! Only [`OperationRequest::Prompt`] is implemented end-to-end —
//! it dispatches to the same code path the existing
//! `POST .../messages` route uses. The other three variants are
//! declared so the wire contract is stable and clients can be
//! written against it; the handler answers `501 Not Implemented`
//! for them until the backing features land.
//!
//! ## Gating
//!
//! The endpoint is off by default. Operators opt in with
//! `[operations] enabled = true` in `config.toml`; the router
//! simply does not register the route when the flag is off, so
//! the path falls through to the standard 404 envelope.

use serde::{Deserialize, Serialize};

/// One operation a client can ask a session to perform.
///
/// Wire shape is internally tagged by `kind` (snake_case), e.g.
/// `{"kind": "prompt", "text": "hi"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationRequest {
    /// Queue a plain-text turn. Implemented.
    Prompt {
        /// The user text.
        text: String,
    },
    /// Run a named skill. Declared for wire compatibility;
    /// the server answers 501.
    Skill {
        /// Skill name (as registered under `.agents/skills`).
        name: String,
        /// Skill arguments.
        #[serde(default)]
        args: serde_json::Value,
    },
    /// Render a named prompt template and queue it. Declared
    /// for wire compatibility; the server answers 501.
    PromptTemplate {
        /// Template name.
        name: String,
        /// Template variable bindings.
        #[serde(default)]
        vars: serde_json::Value,
    },
    /// Force a context compaction. Declared for wire
    /// compatibility; the server answers 501.
    Compaction {
        /// When `true`, compact even if the utilisation
        /// threshold has not been crossed.
        #[serde(default)]
        force: bool,
    },
    /// Navigate the session (rewind / branch). Declared for
    /// wire compatibility; the server answers 501.
    Navigation {
        /// Navigation target (e.g. a message id).
        target: String,
        /// Navigation arguments.
        #[serde(default)]
        args: serde_json::Value,
    },
}

impl OperationRequest {
    /// Stable snake_case wire tag for this operation. Useful for
    /// logging and for the `501` error message.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Prompt { .. } => "prompt",
            Self::Skill { .. } => "skill",
            Self::PromptTemplate { .. } => "prompt_template",
            Self::Compaction { .. } => "compaction",
            Self::Navigation { .. } => "navigation",
        }
    }

    /// True when the server implements this operation today.
    #[must_use]
    pub const fn is_implemented(&self) -> bool {
        matches!(self, Self::Prompt { .. })
    }
}

/// `AppJson<T>` requires `T: validator::Validate`. The operation
/// union has no field-level rules — every variant's payload is
/// either used as-is (`Prompt.text`) or rejected wholesale with
/// `501` — so the check is a no-op. An explicit impl keeps that
/// decision visible instead of hiding it behind a derive.
impl validator::Validate for OperationRequest {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_round_trips_with_kind_tag() {
        let req = OperationRequest::Prompt {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["kind"], "prompt");
        assert_eq!(json["text"], "hello");
        let parsed: OperationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn skill_round_trips_with_default_args() {
        let json = serde_json::json!({ "kind": "skill", "name": "summarize" });
        let parsed: OperationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            parsed,
            OperationRequest::Skill {
                name: "summarize".to_string(),
                args: serde_json::Value::Null,
            }
        );
    }

    #[test]
    fn prompt_template_round_trips_with_vars() {
        let json = serde_json::json!({
            "kind": "prompt_template",
            "name": "review",
            "vars": { "lang": "rust" }
        });
        let parsed: OperationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.kind(), "prompt_template");
        assert!(!parsed.is_implemented());
    }

    #[test]
    fn compaction_defaults_force_to_false() {
        let json = serde_json::json!({ "kind": "compaction" });
        let parsed: OperationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, OperationRequest::Compaction { force: false });
    }

    #[test]
    fn navigation_round_trips() {
        let json = serde_json::json!({
            "kind": "navigation",
            "target": "msg-7"
        });
        let parsed: OperationRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.kind(), "navigation");
    }

    #[test]
    fn only_prompt_is_implemented() {
        assert!(
            OperationRequest::Prompt {
                text: String::new()
            }
            .is_implemented()
        );
        assert!(
            !OperationRequest::Compaction { force: false }.is_implemented()
        );
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let json = serde_json::json!({ "kind": "teleport" });
        assert!(serde_json::from_value::<OperationRequest>(json).is_err());
    }
}
