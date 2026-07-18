//! Tool category enumeration.
//!
//! Re-exports [`synthia_core::tool::descriptor::ToolCategory`] when the
//! `unified-registry` feature is enabled, otherwise provides a standalone
//! enum with the same shape.

#[cfg(feature = "unified-registry")]
pub use synthia_core::tool::descriptor::ToolCategory;

#[cfg(not(feature = "unified-registry"))]
mod standalone {
    /// Tool category for routing and permission decisions.
    ///
    /// Mirrors `synthia_core::tool::descriptor::ToolCategory` so that
    /// the sub-traits can reference a category without pulling in the
    /// full unified-registry dependency.
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
}

#[cfg(not(feature = "unified-registry"))]
pub use standalone::ToolCategory;
