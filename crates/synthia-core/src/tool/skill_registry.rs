//! SkillRegistry — 技能注册表，提供提示模板 + 工具组合 + 隐式调用检测。

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::tool_name::ToolName;

/// 技能来源
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillProvenance {
    Core,
    Plugin { id: String },
    User,
    File { path: String },
}

/// Skill trait — 技能定义
#[async_trait]
pub trait Skill: Send + Sync + 'static {
    /// 技能名称
    fn name(&self) -> &str;
    /// 技能描述
    fn description(&self) -> &str;
    /// 技能激活时注入到 system prompt 的指令
    fn instructions(&self) -> &str;
    /// 技能需要的工具列表
    fn tools(&self) -> Vec<ToolName>;
    /// 技能来源
    fn provenance(&self) -> &SkillProvenance;
    /// 检测用户输入是否隐式触发此技能，返回 [0.0, 1.0] 的置信度
    async fn detect_invocation(&self, user_input: &str) -> f64;
}

/// 技能错误类型
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill not found: {0}")]
    NotFound(String),
    #[error("Skill already registered: {0}")]
    AlreadyRegistered(String),
    #[error("Skill activation failed: {0}")]
    ActivationFailed(String),
}

/// 技能激活结果
#[derive(Debug, Clone)]
pub struct SkillActivation {
    pub name: String,
    pub instructions: String,
    pub tools: Vec<ToolName>,
}

/// 技能注册表
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Arc<dyn Skill>>>,
    active_skills: RwLock<Vec<String>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            active_skills: RwLock::new(Vec::new()),
        }
    }

    /// 注册技能
    pub async fn register(
        &self,
        skill: Arc<dyn Skill>,
    ) -> Result<(), SkillError> {
        let name = skill.name().to_owned();
        let mut skills = self.skills.write().await;
        if skills.contains_key(&name) {
            return Err(SkillError::AlreadyRegistered(name));
        }
        skills.insert(name, skill);
        Ok(())
    }

    /// 反注册技能
    pub async fn unregister(&self, name: &str) -> Result<(), SkillError> {
        {
            let mut skills = self.skills.write().await;
            if skills.remove(name).is_none() {
                return Err(SkillError::NotFound(name.to_owned()));
            }
        }
        // 同时从活跃列表中移除
        let mut active = self.active_skills.write().await;
        active.retain(|n| n != name);
        Ok(())
    }

    /// 获取技能
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        let skills = self.skills.read().await;
        skills.get(name).cloned()
    }

    /// 列出所有技能名称
    pub async fn list(&self) -> Vec<String> {
        let skills = self.skills.read().await;
        skills.keys().cloned().collect()
    }

    /// 检测用户输入最可能触发的技能，返回 (skill_name, confidence) 列表
    /// 按置信度降序排列，仅返回超过 threshold 的
    pub async fn detect_skills(
        &self,
        user_input: &str,
        threshold: f64,
    ) -> Vec<(String, f64)> {
        let skills = self.skills.read().await;
        let mut results: Vec<(String, f64)> = Vec::new();
        for skill in skills.values() {
            let confidence = skill.detect_invocation(user_input).await;
            if confidence > threshold {
                results.push((skill.name().to_owned(), confidence));
            }
        }
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// 激活技能（将技能添加到活跃列表，返回其 instructions + tools）
    pub async fn activate(
        &self,
        name: &str,
    ) -> Result<SkillActivation, SkillError> {
        let skills = self.skills.read().await;
        let skill = skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound(name.to_owned()))?;

        let activation = SkillActivation {
            name: skill.name().to_owned(),
            instructions: skill.instructions().to_owned(),
            tools: skill.tools(),
        };

        // 释放读锁再获取写锁，避免死锁
        drop(skills);

        let mut active = self.active_skills.write().await;
        if !active.contains(&name.to_owned()) {
            active.push(name.to_owned());
        }

        Ok(activation)
    }

    /// 停用技能
    pub async fn deactivate(&self, name: &str) {
        let mut active = self.active_skills.write().await;
        active.retain(|n| n != name);
    }

    /// 获取所有活跃技能的 instructions
    pub async fn active_instructions(&self) -> Vec<String> {
        let active = self.active_skills.read().await;
        let skills = self.skills.read().await;
        active
            .iter()
            .filter_map(|name| {
                skills.get(name).map(|s| s.instructions().to_owned())
            })
            .collect()
    }

    /// 获取所有活跃技能需要的工具
    pub async fn active_tools(&self) -> Vec<ToolName> {
        let active = self.active_skills.read().await;
        let skills = self.skills.read().await;
        let mut tools: Vec<ToolName> = active
            .iter()
            .flat_map(|name| {
                skills.get(name).map(|s| s.tools()).unwrap_or_default()
            })
            .collect();
        tools.dedup();
        tools
    }

    /// 技能数量
    pub async fn skill_count(&self) -> usize {
        let skills = self.skills.read().await;
        skills.len()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助结构体
    struct EchoSkill {
        name: String,
        description: String,
        instructions: String,
        tools: Vec<ToolName>,
        provenance: SkillProvenance,
        keywords: Vec<String>,
    }

    impl EchoSkill {
        fn new(name: &str, keywords: Vec<&str>) -> Self {
            Self {
                name: name.to_owned(),
                description: format!("Description of {name}"),
                instructions: format!("Instructions for {name}"),
                tools: vec![ToolName::plain(name)],
                provenance: SkillProvenance::Core,
                keywords: keywords.into_iter().map(String::from).collect(),
            }
        }

        fn with_tools(
            name: &str,
            keywords: Vec<&str>,
            tools: Vec<ToolName>,
        ) -> Self {
            Self {
                name: name.to_owned(),
                description: format!("Description of {name}"),
                instructions: format!("Instructions for {name}"),
                tools,
                provenance: SkillProvenance::Core,
                keywords: keywords.into_iter().map(String::from).collect(),
            }
        }
    }

    #[async_trait]
    impl Skill for EchoSkill {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn instructions(&self) -> &str {
            &self.instructions
        }

        fn tools(&self) -> Vec<ToolName> {
            self.tools.clone()
        }

        fn provenance(&self) -> &SkillProvenance {
            &self.provenance
        }

        async fn detect_invocation(&self, user_input: &str) -> f64 {
            let lower = user_input.to_lowercase();
            if self
                .keywords
                .iter()
                .any(|kw| lower.contains(&kw.to_lowercase()))
            {
                0.8
            } else {
                0.0
            }
        }
    }

    fn make_skill(name: &str, keywords: Vec<&str>) -> Arc<dyn Skill> {
        Arc::new(EchoSkill::new(name, keywords))
    }

    #[tokio::test]
    async fn new_registry_is_empty() {
        let registry = SkillRegistry::new();
        assert_eq!(registry.skill_count().await, 0);
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn register_and_get() {
        let registry = SkillRegistry::new();
        let skill = make_skill("search", vec!["search", "find"]);
        registry.register(skill).await.unwrap();

        let got = registry.get("search").await.unwrap();
        assert_eq!(got.name(), "search");
        assert_eq!(registry.skill_count().await, 1);
    }

    #[tokio::test]
    async fn register_duplicate_returns_error() {
        let registry = SkillRegistry::new();
        let skill1 = make_skill("search", vec!["search"]);
        let skill2 = make_skill("search", vec!["search"]);
        registry.register(skill1).await.unwrap();
        let err = registry.register(skill2).await.unwrap_err();
        assert!(
            matches!(err, SkillError::AlreadyRegistered(ref s) if s == "search")
        );
    }

    #[tokio::test]
    async fn unregister_removes_skill() {
        let registry = SkillRegistry::new();
        let skill = make_skill("search", vec!["search"]);
        registry.register(skill).await.unwrap();
        registry.unregister("search").await.unwrap();
        assert_eq!(registry.skill_count().await, 0);
        assert!(registry.get("search").await.is_none());
    }

    #[tokio::test]
    async fn unregister_nonexistent_returns_error() {
        let registry = SkillRegistry::new();
        let err = registry.unregister("nonexistent").await.unwrap_err();
        assert!(
            matches!(err, SkillError::NotFound(ref s) if s == "nonexistent")
        );
    }

    #[tokio::test]
    async fn list_returns_all_names() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search"]))
            .await
            .unwrap();
        registry
            .register(make_skill("debug", vec!["debug"]))
            .await
            .unwrap();

        let mut names = registry.list().await;
        names.sort();
        assert_eq!(names, vec!["debug", "search"]);
    }

    #[tokio::test]
    async fn detect_skills_returns_matching() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search", "find"]))
            .await
            .unwrap();
        registry
            .register(make_skill("debug", vec!["debug", "bug"]))
            .await
            .unwrap();

        let results = registry
            .detect_skills("I want to search for something", 0.5)
            .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "search");
        assert!((results[0].1 - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn detect_skills_threshold_filters() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search"]))
            .await
            .unwrap();

        // threshold 0.9 应过滤掉 0.8 的匹配
        let results = registry.detect_skills("search something", 0.9).await;
        assert!(results.is_empty());

        // threshold 0.5 应保留 0.8 的匹配
        let results = registry.detect_skills("search something", 0.5).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn activate_adds_to_active_list() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search"]))
            .await
            .unwrap();

        let activation = registry.activate("search").await.unwrap();
        assert_eq!(activation.name, "search");
        assert_eq!(activation.instructions, "Instructions for search");
        assert_eq!(activation.tools, vec![ToolName::plain("search")]);
    }

    #[tokio::test]
    async fn deactivate_removes_from_active_list() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search"]))
            .await
            .unwrap();
        registry.activate("search").await.unwrap();
        registry.deactivate("search").await;

        let instructions = registry.active_instructions().await;
        assert!(instructions.is_empty());
    }

    #[tokio::test]
    async fn active_instructions_returns_instructions() {
        let registry = SkillRegistry::new();
        registry
            .register(make_skill("search", vec!["search"]))
            .await
            .unwrap();
        registry
            .register(make_skill("debug", vec!["debug"]))
            .await
            .unwrap();
        registry.activate("search").await.unwrap();
        registry.activate("debug").await.unwrap();

        let mut instructions = registry.active_instructions().await;
        instructions.sort();
        assert_eq!(
            instructions,
            vec!["Instructions for debug", "Instructions for search"]
        );
    }

    #[tokio::test]
    async fn active_tools_returns_tools() {
        let registry = SkillRegistry::new();
        registry
            .register(Arc::new(EchoSkill::with_tools(
                "search",
                vec!["search"],
                vec![ToolName::plain("search_tool")],
            )))
            .await
            .unwrap();
        registry
            .register(Arc::new(EchoSkill::with_tools(
                "debug",
                vec!["debug"],
                vec![ToolName::plain("debug_tool")],
            )))
            .await
            .unwrap();
        registry.activate("search").await.unwrap();
        registry.activate("debug").await.unwrap();

        let mut tools = registry.active_tools().await;
        tools.sort();
        assert_eq!(
            tools,
            vec![
                ToolName::plain("debug_tool"),
                ToolName::plain("search_tool")
            ]
        );
    }
}
