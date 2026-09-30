//! # synthia-server
//!
//! The application crate: an axum REST + SSE surface over the library
//! pieces, and the only crate in the workspace allowed to reach for a
//! concrete runtime, `anyhow`, or a database driver.
//!
//! It is deliberately **not** part of the assembly story for library
//! consumers — `synthia` (the facade) plus `crates/synthia-harness` are.
//! What this crate is good for is showing how the pieces are wired end
//! to end: one process clock, one tool registry, one steering policy,
//! one agent registry, and a run factory that every chat dispatch goes
//! through.
//!
//! ## The boot
//!
//! ```text
//! clap args ─┐
//!            ├─ resolve_server_config ──► AppState ◄── routes
//! env vars  ─┘        (one read)             │
//!                                     [agents]/[tools]/auth/cors/
//!                                     mcp_servers applied
//!
//! [`config::resolve_server_config`] resolves the deployment file once
//! (`--config`, then `<dir>/config.toml`, then `<dir>/.synthia/config.toml`)
//! and everything read at boot comes from that value; the bind address
//! resolves from `--host`/`--port`, `SYNTHIA_HOST`/`SYNTHIA_PORT`, and
//! that config, in that order. `server::boot_server` returns the router
//! and the resolved address together, which is what `main` binds.
//!
//! ## Surfaces
//!
//! - `POST /api/v1/chat/sessions/…` — the chat surface: create, send,
//!   stream (SSE), cancel, regenerate, feedback.
//! - `/api/v1/{agents,skills,tools,memory,sessions,models}` — management,
//!   behind the API-key layer (`[auth]`).
//! - `/livez`, `/readyz`, `/metrics` — probes and Prometheus, mounted
//!   outside the auth layer because an orchestrator hits them every
//!   second.
//!
//! ## What is here
//!
//! - [`state::AppState`] — the assembled process state and the one
//!   `SessionController` per `(user, session)`.
//! - [`config`] — the deployment types, the YAML bridge the CLI's
//!   `--config` file goes through, and the resolver above.
//! - [`session`] — the controller: input queue, run factory, event
//!   stream, durability, compaction checkpoints.
//! - [`routes`] — the HTTP handlers, thin by design.
//! - [`middleware`] — auth, tracing, trace-context propagation, error
//!   envelope, response headers, RED metrics.
//!
//! See `DEPLOYMENT.md` for the deployment contract (config keys,
//! variables, probes, Nginx notes).

pub mod api;
pub mod build_info;
pub mod config;
pub mod event_stream;
pub mod middleware;
pub mod routes;
pub mod search;
pub mod server;
pub mod session;
pub mod state;

pub use config::server::ServerConfig;
pub use event_stream::EventBroadcaster;
pub use server::{BootedServer, boot_server, create_router};
pub use state::{AppState, AppliedToolSurface, UsageMetrics, UsageSnapshot};
