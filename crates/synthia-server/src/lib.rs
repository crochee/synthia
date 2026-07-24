// TODO: Migrate ContextAssembler → FragmentRegistry::render_active() in
// session/controller.rs and state/agent_factory.rs, then remove this allow.
#![allow(deprecated)]

pub mod a2a;
pub mod api;
pub mod approval;
pub mod cleanup;
pub mod config;
pub mod error;
pub mod event_stream;
pub mod mcp;
pub mod middleware;
pub mod routes;
pub mod scheduler;
pub mod server;
pub mod session;
pub mod sse;
pub mod state;
pub mod workspace;

pub use approval::*;
pub use cleanup::{CleanupConfig, CleanupDaemon, CleanupMetrics};
pub use config::server::ServerConfig;
pub use event_stream::{EventBroadcaster, SseEventStream};
pub use mcp::{McpService, types::McpServerConfig};
pub use scheduler::{JobRegistry, JobScheduler};
pub use server::*;
pub use sse::agent_output_to_sse;
pub use state::*;
