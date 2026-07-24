//! [`HookEvent`] enum: external injection and custom events.

use serde::{Deserialize, Serialize};

/// External injection and extension custom events reported via
/// [`AgentEvent::Hook`](super::AgentEvent::Hook).
///
/// Spec:
/// - `Message` — A human-injected message (user steering, hotkeys).
/// - `ConfirmRequest` — A guardian/pre-execution confirmation
///   request for a tool call.
/// - `ConfirmResponse` — The user's response (approve / deny).
/// - `Custom` — A free-form custom event from an extension plugin;
///   `kind` is the plugin event name, `data` is arbitrary JSON.
///
/// The serde format uses `#[serde(tag = "type")]` so the wire JSON is
/// `{"type": "<variant>", ...payload}`. The A2A adapter translates
/// this to the documented `kind` discriminator format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookEvent {
    /// A user-injected message (e.g. steering).
    Message { priority: i32, message: String },
    /// A confirmation request for a tool call.
    ConfirmRequest {
        tool_use_id: String,
        tool_name: String,
        reason: String,
    },
    /// The user's response to a [`HookEvent::ConfirmRequest`].
    ConfirmResponse { approved: bool, tool_use_id: String },
    /// A free-form extension custom event.
    ///
    /// Mirrors pi-mono `extensions/types.ts` `CustomEvent` support.
    /// `kind` identifies the plugin event type; `data` carries
    /// arbitrary JSON.
    Custom {
        kind: String,
        data: serde_json::Value,
    },
}
