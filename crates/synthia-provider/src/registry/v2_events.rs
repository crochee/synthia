//! Extension events emitted by `ProviderRegistry` v2.
//!
//! These are the canonical wire shape consumers can listen on to react
//! to registration changes and source-scoped hot-swaps.

use serde::{Deserialize, Serialize};

use super::v2::SourceId;

/// Lifecycle events from `ProviderRegistry` v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProviderEvent {
    ProviderRegister { name: String, source_id: SourceId },
    ProviderUnregister { name: String, source_id: SourceId },
    ExtensionHotSwap { source_id: SourceId, count: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_event_serializes_with_tag() {
        let event = ProviderEvent::ProviderRegister {
            name: "gpt-4".to_string(),
            source_id: SourceId("ext-a".to_string()),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("\"event\":\"provider_register\""),
            "got: {json}"
        );
        assert!(json.contains("\"name\":\"gpt-4\""), "got: {json}");
    }

    #[test]
    fn hot_swap_event_serializes_with_tag() {
        let event = ProviderEvent::ExtensionHotSwap {
            source_id: SourceId("ext-a".to_string()),
            count: 3,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("\"event\":\"extension_hot_swap\""),
            "got: {json}",
        );
        assert!(json.contains("\"count\":3"), "got: {json}");
    }

    #[test]
    fn round_trip_register() {
        let original = ProviderEvent::ProviderRegister {
            name: "gpt-4".to_string(),
            source_id: SourceId("ext-b".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: ProviderEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }
}
