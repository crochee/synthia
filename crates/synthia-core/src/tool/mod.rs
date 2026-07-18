//! Unified Tool trait + supporting types (feature-gated behind `unified-registry`).

pub mod bound_output;
pub mod builtin;
pub mod capability;
pub mod delegate_providers;
pub mod descriptor;
pub mod mcp_provider;
pub mod mcp_types;
pub mod output_bound;
pub mod plugin;
pub mod provider;
pub mod registry;
pub mod types;

pub use bound_output::*;
pub use builtin::*;
pub use capability::*;
pub use delegate_providers::*;
pub use descriptor::*;
pub use mcp_provider::*;
pub use mcp_types::*;
pub use output_bound::*;
pub use plugin::*;
pub use provider::*;
pub use registry::*;
pub use types::*;
