//! A2A 协议集成模块。
//!
//! Bridges the agent's internal [`SessionController`] to the A2A
//! protocol's `AgentExecutor` trait so external agents can talk to
//! Synthia over the standard A2A endpoints.
//!
//! ## Module layout
//!
//! | Module        | Responsibility                                                   |
//! |---------------|-------------------------------------------------------------------|
//! | `card`        | Pure `AgentCard` construction (no `AppState` dependency).         |
//! | `mapping`     | `AgentEvent` → A2A `StreamResponse` translation.                  |
//! | `card_builder`| Snapshot `AppState.tool_registry` into the default `AgentCard`.   |
//! | `executor`    | `SynthiaExecutor`: bridges `AgentExecutor` ↔ `SessionController`. |
//! | `service`     | `A2aService`: assembles handler, card producer, and merged router.|
//! | `wrapper`     | `SynthiaHandler`: decorates `RequestHandler` for post-completion `:subscribe`. |
//! | `shared_store`| `SharedTaskStore`: clone-friendly `TaskStore` for the wrapper fallback. |
//! | `task_history`| `TaskHistoryBuilder` — A2A-faithful `Task.history` writer. |
//! | `serde_sse`   | `#[cfg(test)]` — verifies `StreamResponse` protojson serialization. |

pub mod card;
pub mod card_builder;
mod executor;
pub mod mapping;
mod service;
mod shared_store;
mod task_history;
mod wrapper;

#[cfg(test)]
mod serde_sse;

pub use service::A2aService;
