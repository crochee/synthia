//! Tool category enumeration.
//!
//! Provides a standalone enum with the same shape as
//! `synthia_core::tool::descriptor::ToolCategory`.

/// Tool category for routing and permission decisions.
///
/// Mirrors `synthia_core::tool::descriptor::ToolCategory` so that
/// the sub-traits can reference a category without pulling in the
/// full unified tool infrastructure.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Filesystem,
    Search,
    Shell,
    Edit,
    Memory,
    Agent,
    Skill,
    Network,
    Utility,
    Custom,
}
