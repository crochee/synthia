//! Shared server state: lifecycle types, `AgentRegistry`, `AppState`,
//! and the `AgentFactory` builder.

mod agent_factory;
mod app_state;
mod registry;
mod subagent_factory;
mod types;

pub use agent_factory::AgentFactory;
pub use app_state::AppState;
pub use registry::AgentRegistry;
pub use subagent_factory::AppStateSubagentFactory;
pub use types::AgentSessionState;
