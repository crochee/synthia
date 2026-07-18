pub mod builtin;
pub mod events;
pub mod registry;
pub mod scoped_registry;
pub mod sub_traits;
pub mod traits;
pub mod types;
#[cfg(feature = "unified-registry")]
pub mod unified_adapter;

#[cfg(test)]
mod tool_test;
#[cfg(test)]
mod types_test;

pub use events::FileChangeEvent;
pub use registry::{ToolEntry, ToolRegistry};
pub use scoped_registry::{
    LayeredToolRegistry,
    ScopeGuard,
    ScopedRegistration,
    ScopedToolRegistry,
    Token,
    ToolScope,
};
pub use sub_traits::{
    PermissionAlwaysAllow,
    PermissionAlwaysDeny,
    PermissionContext,
    PermissionDecision,
    ToolCategory,
    ToolDefinition,
    ToolExecution,
    ToolLifecycle,
    ToolMetadataSnapshot,
    ToolPermission,
};
pub use traits::*;
pub use types::*;

/// Backward-compatible alias aggregating the three sub-traits.
///
/// `ToolV1` is a supertrait combining `ToolDefinition + ToolExecution +
/// ToolLifecycle`, providing the full tool interface in a single trait
/// object. Existing callers that need all three aspects can use this
/// trait instead of spelling out the compound bound.
///
/// This trait will be retained for at least 2 minor versions after the
/// sub-traits become the primary interface.
pub trait ToolV1: ToolDefinition + ToolExecution + ToolLifecycle {}
impl<T: ToolDefinition + ToolExecution + ToolLifecycle> ToolV1 for T {}
