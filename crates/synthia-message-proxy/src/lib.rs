pub mod client;
pub mod server;

mod message_proxy {
    tonic::include_proto!("message_proxy");
}

pub use client::{DEFAULT_PROXY_ADDR, MessageBus, MessageBusProxy, ProxyError};
pub use message_proxy::{
    BroadcastRequest,
    BroadcastResult,
    Message,
    RegisterRequest,
    RegisterResponse,
    SendResult,
    SubscribeRequest,
    message_proxy_service_client::MessageProxyServiceClient,
    message_proxy_service_server::{
        MessageProxyService,
        MessageProxyServiceServer,
    },
};
pub use server::MessageProxyServer;
