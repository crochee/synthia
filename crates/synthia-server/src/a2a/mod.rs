//! A2A 协议集成模块。
//!
//! 将 synthia 的 `SessionController` 桥接到 A2A 协议的 `AgentExecutor` trait，
//! 使得外部 agent 可以通过标准 A2A 端点与 synthia 交互。

mod card_builder;
mod executor;
mod service;

pub use service::A2aService;
