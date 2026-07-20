//! 从 AppState 构建 A2A AgentCard。
//!
//! 读取 tool_registry 中的工具列表，转换为 A2A AgentSkill，
//! 调用 `synthia_a2a::build_agent_card` 构建标准 AgentCard。

use a2a::AgentCard;
use synthia_a2a::card::{build_agent_card, tool_to_skill};

use crate::state::AppState;

/// 从 AppState 构建默认的 A2A AgentCard。
///
/// 使用 tool_registry 中的工具列表作为 AgentSkill，
/// 并使用固定名称 "Synthia" 和描述。
pub async fn build_card_from_state(state: &AppState, url: String) -> AgentCard {
    let skills = collect_skills_from_registry(state).await;
    build_agent_card(
        "Synthia".to_string(),
        "AI coding assistant powered by Synthia".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        url,
        skills,
    )
}

/// 从 tool_registry 收集所有工具并转换为 A2A AgentSkill。
async fn collect_skills_from_registry(
    state: &AppState,
) -> Vec<a2a::AgentSkill> {
    let registry = state.tool_registry.read().await;
    let snapshots = registry.snapshot();
    snapshots
        .iter()
        .map(|snap| {
            tool_to_skill(
                snap.name.clone(),
                snap.name.clone(),
                snap.description.clone(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_to_skill_creates_valid_skill() {
        let skill = tool_to_skill(
            "read_file".to_string(),
            "ReadFile".to_string(),
            "Read a file".to_string(),
        );
        assert_eq!(skill.id, "read_file");
        assert_eq!(skill.name, "ReadFile");
    }
}
