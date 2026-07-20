//! synthia-a2a — A2A 协议通信层。
//!
//! 集成 a2a-lf crate 族，每个 AgentHandle 可同时作为 A2A Client 和 Server。
//! 提供 A2aTransport、SynthiaA2aHandler、SendMessage/SendMessageStream Tool。
//!
//! ## 类型映射
//!
//! - `mapping` — AgentEvent → A2A StreamResponse 转换
//! - `card` — 构建 A2A AgentCard

pub mod card;
pub mod handler;
pub mod mapping;
#[allow(deprecated)]
pub mod tools;
pub mod transport;

pub use card::{build_agent_card, tool_to_skill};
pub use handler::SynthiaA2aHandler;
pub use mapping::{
    agent_event_to_stream_responses,
    extract_text_from_message,
    task_with_state,
};
#[allow(deprecated)]
pub use tools::{SendMessageStreamTool, SendMessageTool};
pub use transport::A2aTransport;
