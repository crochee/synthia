//! Pure `AgentCard` construction — no `AppState` dependency.
//!
//! The helpers here take only primitive inputs (name, description,
//! version, URL, skill list) so they can be unit-tested without the
//! HTTP / state stack. Wiring the card to live tool registry
//! snapshots is the responsibility of [`super::card_builder`].
//!
//! Output is compatible with A2A Protocol v1.0 wire format (protojson)
//! as consumed by `@a2a-js/sdk@1.0.0` via `JsonRpcTransportFactory`.

use a2a::{AgentCapabilities, AgentCard, AgentInterface, AgentSkill};

/// Build the default Synthia [`AgentCard`].
///
/// * `name`        — agent name.
/// * `description` — agent description.
/// * `version`     — agent version.
/// * `url`         — A2A endpoint URL (absolute or `/a2a` relative).
/// * `skills`      — AgentSkill list derived from the tool registry.
pub fn build_agent_card(
    name: String,
    description: String,
    version: String,
    url: String,
    skills: Vec<AgentSkill>,
) -> AgentCard {
    AgentCard {
        name,
        description,
        version,
        supported_interfaces: vec![
            AgentInterface {
                url: url.clone(),
                protocol_binding: "JSONRPC".to_string(),
                protocol_version: a2a::VERSION.to_string(),
                tenant: Some(String::new()),
            },
            AgentInterface {
                url: url.clone(),
                protocol_binding: "HTTP+JSON".to_string(),
                protocol_version: a2a::VERSION.to_string(),
                tenant: Some(String::new()),
            },
        ],
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            extensions: None,
            extended_agent_card: None,
        },
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills,
        provider: None,
        documentation_url: None,
        icon_url: None,
        security_schemes: None,
        security_requirements: None,
        signatures: None,
    }
}

/// Map a Synthia tool (`name` + `description`) into an A2A
/// [`AgentSkill`]. Tags default to empty; richer metadata is the
/// caller's responsibility.
pub fn tool_to_skill(
    id: String,
    name: String,
    description: String,
) -> AgentSkill {
    AgentSkill {
        id,
        name,
        description,
        tags: vec![],
        examples: None,
        input_modes: None,
        output_modes: None,
        security_requirements: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_agent_card_produces_valid_card() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "AI coding assistant".to_string(),
            "0.1.0".to_string(),
            "http://localhost:3000/a2a".to_string(),
            vec![tool_to_skill(
                "read_file".to_string(),
                "ReadFile".to_string(),
                "Read a file".to_string(),
            )],
        );
        assert_eq!(card.name, "Synthia");
        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.capabilities.streaming, Some(true));
        assert_eq!(card.default_input_modes, vec!["text/plain"]);
        let json = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(card.name, back.name);
    }

    #[test]
    fn tool_to_skill_maps_correctly() {
        let skill = tool_to_skill(
            "s1".to_string(),
            "Search".to_string(),
            "Search codebase".to_string(),
        );
        assert_eq!(skill.id, "s1");
        assert_eq!(skill.name, "Search");
        assert!(skill.tags.is_empty());
    }

    /// `build_agent_card` MUST emit 2 supported interfaces
    /// (JSON-RPC and HTTP+JSON).
    #[test]
    fn build_agent_card_has_two_supported_interfaces() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        assert_eq!(card.supported_interfaces.len(), 2);
    }

    /// `build_agent_card` MUST use the A2A protocol version
    /// constant `a2a::VERSION`.
    #[test]
    fn build_agent_card_uses_a2a_version() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        for iface in &card.supported_interfaces {
            assert_eq!(iface.protocol_version, a2a::VERSION);
        }
    }

    /// `build_agent_card` MUST set `streaming = Some(true)`.
    #[test]
    fn build_agent_card_streaming_is_true() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        assert_eq!(card.capabilities.streaming, Some(true));
    }

    /// `build_agent_card` MUST set `push_notifications = Some(false)`.
    #[test]
    fn build_agent_card_push_notifications_false() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        assert_eq!(card.capabilities.push_notifications, Some(false));
    }

    /// `build_agent_card` MUST set both input and output modes
    /// to `["text/plain"]`.
    #[test]
    fn build_agent_card_default_modes_are_text_plain() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        assert_eq!(card.default_input_modes, vec!["text/plain"]);
        assert_eq!(card.default_output_modes, vec!["text/plain"]);
    }

    /// `build_agent_card` MUST accept an empty skills list.
    #[test]
    fn build_agent_card_accepts_empty_skills() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        );
        assert!(card.skills.is_empty());
    }

    /// `build_agent_card` MUST round-trip through JSON.
    #[test]
    fn build_agent_card_round_trips_through_json() {
        let card = build_agent_card(
            "Synthia".to_string(),
            "desc".to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![tool_to_skill(
                "bash".to_string(),
                "Bash".to_string(),
                "Run shell".to_string(),
            )],
        );
        let json = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Synthia");
        assert_eq!(back.skills.len(), 1);
        assert_eq!(back.skills[0].id, "bash");
    }

    /// `tool_to_skill` MUST NOT set `examples`/`input_modes`/
    /// `output_modes` (all default to `None`).
    #[test]
    fn tool_to_skill_optional_fields_are_none() {
        let skill = tool_to_skill(
            "id".to_string(),
            "Name".to_string(),
            "desc".to_string(),
        );
        assert!(skill.examples.is_none());
        assert!(skill.input_modes.is_none());
        assert!(skill.output_modes.is_none());
        assert!(skill.security_requirements.is_none());
    }

    /// `tool_to_skill` MUST accept any string for `description`.
    #[test]
    fn tool_to_skill_accepts_any_description() {
        let skill =
            tool_to_skill("id".to_string(), "Name".to_string(), "".to_string());
        assert_eq!(skill.description, "");
    }
}
