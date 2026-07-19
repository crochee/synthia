//! `EventRenderer` trait + `EventRendererRegistry` + builtin `JsonEventRenderer`.
//!
//! PR-7.2: Custom events need a rendering surface so that consumers
//! (UI, protocol layer, telemetry) can transform `Custom { event_type, data }`
//! into human-readable or structured output. The registry is keyed by
//! `event_type`; the wildcard key `"*"` is the builtin JSON fallback.

use std::fmt;

use dashmap::DashMap;
use serde_json::Value;

/// Trait for rendering a custom event's data payload.
///
/// Implementors receive the `data` field of an `AgentEvent::Custom` and
/// return a rendered string. The builtin `JsonEventRenderer` simply
/// pretty-prints the JSON value.
pub trait EventRenderer: Send + Sync + 'static {
    /// Render the event data to a string.
    fn render(&self, event_type: &str, data: &Value) -> String;
}

/// Builtin JSON renderer — the wildcard fallback.
///
/// Produces `serde_json::to_string_pretty` output. If that fails,
/// falls back to `Debug` formatting.
pub struct JsonEventRenderer;

impl EventRenderer for JsonEventRenderer {
    fn render(&self, _event_type: &str, data: &Value) -> String {
        serde_json::to_string_pretty(data)
            .unwrap_or_else(|_| format!("{data:?}"))
    }
}

/// Error returned by [`EventRendererRegistry::register`].
#[derive(Debug, thiserror::Error)]
pub enum EventRendererError {
    /// A renderer is already registered for this event type.
    #[error("renderer already registered for event type: {0}")]
    AlreadyRegistered(String),
}

/// Thread-safe registry of [`EventRenderer`] instances, keyed by `event_type`.
///
/// Lookup order:
/// 1. Exact match on `event_type`.
/// 2. Wildcard `"*"`.
/// 3. If neither exists, a default `JsonEventRenderer` is used.
pub struct EventRendererRegistry {
    renderers: DashMap<String, Box<dyn EventRenderer>>,
}

impl fmt::Debug for EventRendererRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<_> =
            self.renderers.iter().map(|r| r.key().clone()).collect();
        f.debug_struct("EventRendererRegistry")
            .field("registered_types", &keys)
            .finish()
    }
}

impl Default for EventRendererRegistry {
    fn default() -> Self {
        let registry = Self {
            renderers: DashMap::new(),
        };
        // Always install the wildcard JSON fallback.
        registry
            .renderers
            .insert("*".to_string(), Box::new(JsonEventRenderer));
        registry
    }
}

impl EventRendererRegistry {
    /// Create a new registry with the builtin JSON wildcard renderer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a renderer for a specific `event_type`.
    ///
    /// Returns `Err` if a renderer (other than the wildcard) is already
    /// registered for this type. To replace, call [`unregister`] first.
    pub fn register(
        &self,
        event_type: impl Into<String>,
        renderer: Box<dyn EventRenderer>,
    ) -> Result<(), EventRendererError> {
        let key = event_type.into();
        if self.renderers.contains_key(&key) {
            return Err(EventRendererError::AlreadyRegistered(key));
        }
        self.renderers.insert(key, renderer);
        Ok(())
    }

    /// Remove a previously registered renderer.
    ///
    /// Returns `true` if a renderer was removed.
    pub fn unregister(&self, event_type: &str) -> bool {
        self.renderers.remove(event_type).is_some()
    }

    /// Render a custom event using the best matching renderer.
    ///
    /// Lookup: exact match → wildcard `"*"` → builtin `JsonEventRenderer`.
    pub fn render(&self, event_type: &str, data: &Value) -> String {
        // 1. Exact match
        if let Some(renderer) = self.renderers.get(event_type) {
            return renderer.render(event_type, data);
        }
        // 2. Wildcard
        if let Some(renderer) = self.renderers.get("*") {
            return renderer.render(event_type, data);
        }
        // 3. Fallback (should not happen — default always installs wildcard)
        JsonEventRenderer.render(event_type, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial test renderer that returns a fixed prefix + the `event_type`.
    struct PrefixRenderer;

    impl EventRenderer for PrefixRenderer {
        fn render(&self, event_type: &str, _data: &Value) -> String {
            format!("[prefix] {event_type}")
        }
    }

    #[test]
    fn wildcard_renders_json_by_default() {
        let registry = EventRendererRegistry::new();
        let data = serde_json::json!({"hello": "world"});
        let rendered = registry.render("any_type", &data);
        assert!(rendered.contains("hello"));
        assert!(rendered.contains("world"));
    }

    #[test]
    fn custom_renderer_overrides_for_specific_type() {
        let registry = EventRendererRegistry::new();
        registry
            .register("my_event", Box::new(PrefixRenderer))
            .unwrap();

        let data = serde_json::json!({});
        let rendered = registry.render("my_event", &data);
        assert_eq!(rendered, "[prefix] my_event");
    }

    #[test]
    fn wildcard_still_used_for_unregistered_types() {
        let registry = EventRendererRegistry::new();
        registry
            .register("my_event", Box::new(PrefixRenderer))
            .unwrap();

        let data = serde_json::json!({"key": 42});
        let rendered = registry.render("other_event", &data);
        // Should fall through to JSON wildcard
        assert!(rendered.contains("42"));
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let registry = EventRendererRegistry::new();
        registry
            .register("my_event", Box::new(PrefixRenderer))
            .unwrap();

        let result = registry.register("my_event", Box::new(PrefixRenderer));
        assert!(result.is_err());
        if let Err(EventRendererError::AlreadyRegistered(key)) = result {
            assert_eq!(key, "my_event");
        } else {
            panic!("expected AlreadyRegistered error");
        }
    }

    #[test]
    fn unregister_removes_renderer() {
        let registry = EventRendererRegistry::new();
        registry
            .register("my_event", Box::new(PrefixRenderer))
            .unwrap();

        assert!(registry.unregister("my_event"));
        // After unregister, falls back to JSON wildcard
        let data = serde_json::json!({"x": 1});
        let rendered = registry.render("my_event", &data);
        assert!(rendered.contains('1'));
    }

    #[test]
    fn unregister_nonexistent_returns_false() {
        let registry = EventRendererRegistry::new();
        assert!(!registry.unregister("no_such_type"));
    }
}
