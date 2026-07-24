use std::{collections::HashMap, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_core::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub priority: i32,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub exec: Option<Vec<String>>,
    pub version: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub levels: SkillLevels,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub body: String,
    pub source: SkillSource,
    pub level: SkillLevel,
    pub token_count: SkillTokenCount,
    pub state: SkillState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillTokenCount {
    pub level0: usize,
    pub level1: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillState {
    Loaded,
    Activated,
    Disabled,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum SkillSource {
    BuiltIn,
    Project,
    User,
}

impl SkillSource {
    pub fn from_path(
        path: &std::path::Path,
        _builtin_root: &std::path::Path,
        project_root: &std::path::Path,
        user_root: &std::path::Path,
    ) -> Self {
        if path.starts_with(user_root) {
            SkillSource::User
        } else if path.starts_with(project_root) {
            SkillSource::Project
        } else {
            SkillSource::BuiltIn
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SkillLevel {
    #[default]
    Level0 = 0,
    Level1 = 1,
    Level2 = 2,
}

impl SkillLevel {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SkillLevel::Level0),
            1 => Some(SkillLevel::Level1),
            2 => Some(SkillLevel::Level2),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            SkillLevel::Level0 => 0,
            SkillLevel::Level1 => 1,
            SkillLevel::Level2 => 2,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillLevels {
    #[serde(default)]
    pub level0: Option<String>,
    #[serde(default)]
    pub level1: Option<String>,
    #[serde(default)]
    pub level2: Option<String>,
}

impl SkillLevels {
    pub fn new() -> Self {
        Self {
            level0: Some("name + description".to_string()),
            level1: Some("detailed instructions".to_string()),
            level2: Some("reference code snippets".to_string()),
        }
    }

    pub fn get_level(&self, level: SkillLevel) -> Option<String> {
        match level {
            SkillLevel::Level0 => self.level0.clone(),
            SkillLevel::Level1 => self.level1.clone(),
            SkillLevel::Level2 => self.level2.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkillMatch {
    pub skill: Skill,
    pub final_score: f64,
    pub bm25_score: f64,
    pub matched_by: MatchStrategy,
}

#[derive(Clone, Debug)]
pub enum MatchStrategy {
    Keyword,
    BM25,
    Vector,
}

#[derive(Clone, Debug, Default)]
pub struct MatchConfig {
    pub max_level0_inject: usize,
    pub bm25_weight: f64,
    pub priority_coefficient: f64,
    pub min_match_score: f64,
}

impl MatchConfig {
    pub fn new() -> Self {
        Self {
            max_level0_inject: 5,
            bm25_weight: 1.0,
            priority_coefficient: 0.1,
            min_match_score: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkillPaths {
    pub user_dir: PathBuf,
    pub project_dir: PathBuf,
    pub builtin_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillStateStore {
    #[serde(default)]
    pub disabled_skills: std::collections::HashSet<String>,
}

impl SkillStateStore {
    pub fn load(path: &std::path::Path) -> Result<Self, Error> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self {
                disabled_skills: std::collections::HashSet::new(),
            })
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SkillUsageStats {
    pub skill_name: String,
    pub use_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub last_used: DateTime<Utc>,
    pub context_summary: Option<String>,
}

/// Aggregated usage statistics for a skill including match and activation counts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillUsageRecord {
    pub skill_name: String,
    pub match_count: usize,
    pub activation_count: usize,
    pub estimated_token_cost: usize,
    pub last_matched: Option<DateTime<Utc>>,
    pub last_activated: Option<DateTime<Utc>>,
}

/// Aggregated global skill statistics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillGlobalStats {
    pub total_skills: usize,
    pub active_skills: usize,
    pub total_token_usage: usize,
    pub total_matches: usize,
    pub total_activations: usize,
}
