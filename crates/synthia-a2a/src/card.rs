//! AgentCard 构建函数。
//!
//! 从 Synthia 元数据构建 A2A 标准的 `AgentCard`，
//! 不依赖 `AppState`，仅接收参数。

use a2a::{AgentCapabilities, AgentCard, AgentInterface, AgentSkill};

/// 从 Synthia 元数据构建 A2A `AgentCard` (v1.0)。
///
/// 输出兼容 A2A Protocol v1.0 wire format (protojson)，
/// 即被 `@a2a-js/sdk@1.0` 通过 `JsonRpcTransportFactory` 消费。
///
/// # Arguments
/// * `name` — Agent 名称
/// * `description` — Agent 描述
/// * `version` — Agent 版本
/// * `url` — A2A endpoint URL（如 `http://localhost:3000/a2a` 或 `/a2a` 相对路径）
/// * `skills` — Agent 提供的技能列表
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

/// 从工具名称和描述构建 A2A `AgentSkill`。
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
}
