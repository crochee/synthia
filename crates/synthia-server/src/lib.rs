// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

pub mod api;
pub mod approval;
pub mod auth;
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
pub mod sse_stream;
pub mod state;
pub mod workspace;

pub use approval::*;
pub use auth::auth_middleware;
pub use cleanup::{CleanupConfig, CleanupDaemon, CleanupMetrics};
pub use config::server::ServerConfig;
pub use event_stream::{EventBroadcaster, SseEventStream};
pub use mcp::{McpService, types::McpServerConfig};
pub use scheduler::{JobRegistry, JobScheduler};
pub use server::*;
pub use sse::agent_output_to_sse;
pub use state::*;
