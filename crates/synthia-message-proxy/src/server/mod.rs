//! gRPC server for the multi-agent message proxy.
//!
//! Routes `Message`s between agents that have connected
//! over a Unix Domain Socket. Point-to-point delivery
//! uses `Send`; one-to-many uses `Broadcast`. Agents must
//! `Register` before they appear in the routing table and
//! before they can `Subscribe` to inbound messages.
//! Delivery is at-most-once: messages addressed to an
//! agent with no active subscriber are dropped.
//!
//! # Module Layout
//!
//! - [`state`]: the [`state::ProxyState`] struct +
//!   [`state::register`] / [`state::lookup`] methods, plus
//!   the `AGENT_CHANNEL_CAPACITY` constant and the
//!   `AgentSender` type alias.
//! - [`server`]: the [`server::MessageProxyServer`] struct
//!   + 2 methods (`new` / `serve`).
//! - [`service`][]: the
//!   [`service::MessageProxyServiceImpl`] gRPC service impl
//!   — the `MessageProxyService` trait's 4 RPC handlers
//!   (`send` / `broadcast` / `register` / `subscribe`).
//! - [`rpc`]: the [`rpc::unix_millis`] helper.
//! - [`tests`]: 11 unit tests covering register / send /
//!   broadcast / subscribe paths.

mod rpc;
#[allow(clippy::module_inception)]
mod server;
mod service;
mod state;

#[cfg(test)]
mod tests;

pub use server::MessageProxyServer;
