//! A2aTransport — A2A 通信层。
//!
//! 每个 AgentHandle 可同时作为 A2A Client 和 Server。
//! 本地 agent 走 in-process call，远程 agent 走 HTTP/gRPC A2A。

use std::{net::SocketAddr, sync::Arc};

use dashmap::DashMap;
use thiserror::Error;
use url::Url;

/// A2A 通信错误。
#[derive(Debug, Error)]
pub enum A2aError {
    #[error("invalid agent URL: {0}")]
    InvalidUrl(String),
    #[error("discovery failed for {url}: {reason}")]
    DiscoveryFailed { url: String, reason: String },
    #[error("server already running")]
    ServerAlreadyRunning,
}

/// A2A AgentCard（能力名片）。
///
/// 从 AgentHandle 自动构建，暴露给其他 agent 做能力发现。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentCard {
    /// Agent 名称。
    pub name: String,
    /// Agent 描述。
    pub description: String,
    /// Agent 版本。
    pub version: String,
    /// Agent URL（A2A endpoint）。
    pub url: Option<String>,
    /// Agent 能力。
    pub capabilities: AgentCapabilities,
    /// Agent 提供的技能（= tool_registry 工具列表）。
    pub skills: Vec<AgentSkill>,
}

/// Agent 能力声明。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentCapabilities {
    /// 是否支持流式响应。
    pub streaming: bool,
    /// 是否支持推送通知。
    pub push_notifications: bool,
}

/// Agent 技能（= 一个 Tool 的描述）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentSkill {
    /// 技能 ID。
    pub id: String,
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
}

/// A2A 通信层 — 每个 AgentHandle 可同时作为 A2A Client 和 Server。
pub struct A2aTransport {
    /// 此 agent 的 AgentCard。
    card: AgentCard,
    /// 已发现的远程 agent client 缓存（url → AgentCard）。
    discovered: Arc<DashMap<String, AgentCard>>,
    /// A2A Server 地址（如果已启动）。
    server_addr: Option<SocketAddr>,
}

impl A2aTransport {
    /// 从 AgentHandle 信息构建 A2aTransport。
    pub fn from_handle_info(
        name: String,
        description: String,
        skills: Vec<AgentSkill>,
    ) -> Self {
        let card = AgentCard {
            name,
            description,
            version: env!("CARGO_PKG_VERSION").to_string(),
            url: None,
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: false,
            },
            skills,
        };

        Self {
            card,
            discovered: Arc::new(DashMap::new()),
            server_addr: None,
        }
    }

    /// 获取此 agent 的 AgentCard。
    pub fn card(&self) -> &AgentCard {
        &self.card
    }

    /// 获取已发现的远程 agent。
    pub fn discovered(&self) -> &Arc<DashMap<String, AgentCard>> {
        &self.discovered
    }

    /// 发现远程 agent — GET /.well-known/agent.json → 缓存。
    ///
    /// 当前返回占位 AgentCard。完整实现需要 A2A HTTP client 调用。
    pub async fn discover(&self, url: &str) -> Result<AgentCard, A2aError> {
        // 验证 URL 格式
        let _parsed =
            Url::parse(url).map_err(|e| A2aError::InvalidUrl(e.to_string()))?;

        // TODO: 实际 A2A discovery — GET /.well-known/agent.json
        // 当前返回占位 card，Phase 2 对接 a2a-client-lf
        let placeholder_card = AgentCard {
            name: format!("remote-agent-{url}"),
            description: "Discovered remote agent (placeholder)".to_string(),
            version: "0.1.0".to_string(),
            url: Some(url.to_string()),
            capabilities: AgentCapabilities::default(),
            skills: Vec::new(),
        };

        self.discovered
            .insert(url.to_string(), placeholder_card.clone());

        Ok(placeholder_card)
    }

    /// 启动 A2A Server — 其他 agent 可通过 A2A 协议发现和调用此 agent。
    ///
    /// 当前为占位实现。完整实现需要 a2a-server-lf 集成。
    pub async fn serve(&mut self, addr: SocketAddr) -> Result<(), A2aError> {
        if self.server_addr.is_some() {
            return Err(A2aError::ServerAlreadyRunning);
        }

        // TODO: 实际 A2A server 启动 — 使用 a2a-server-lf
        // 当前只记录地址
        self.card.url = Some(format!("http://{addr}"));
        self.server_addr = Some(addr);

        tracing::info!(
            "A2A server started at {addr} for agent {}",
            self.card.name
        );
        Ok(())
    }

    /// 获取 A2A Server 地址。
    pub fn server_addr(&self) -> Option<SocketAddr> {
        self.server_addr
    }

    /// 将远程 agent URL 注册为 SendMessage/SendMessageStream Tool 到 registry。
    ///
    /// 遍历 remote_urls，为每个 URL 创建 SendMessageTool 和 SendMessageStreamTool
    /// 并注册到给定的 ToolRegistry。AgentHandle 初始化时可调用此方法。
    #[allow(deprecated)]
    pub fn register_remote_tools(
        &self,
        remote_urls: &[String],
        registry: &synthia_tool::registry::ToolRegistry,
    ) {
        use synthia_tool::{ToolEntry, traits::Tool};

        for url in remote_urls {
            let send_msg = crate::tools::SendMessageTool::for_url(
                url.clone(),
                Arc::new(self.clone_for_registration()),
            );
            let send_stream = crate::tools::SendMessageStreamTool::for_url(
                url.clone(),
                Arc::new(self.clone_for_registration()),
            );
            registry
                .register(ToolEntry::new(Arc::new(send_msg) as Arc<dyn Tool>));
            registry.register(ToolEntry::new(
                Arc::new(send_stream) as Arc<dyn Tool>
            ));
            tracing::info!(url = %url, "Registered SendMessage/SendMessageStream tools for remote agent");
        }
    }

    /// 克隆 self 用于工具注册（需要独立的 transport 引用）。
    fn clone_for_registration(&self) -> Self {
        Self {
            card: self.card.clone(),
            discovered: self.discovered.clone(),
            server_addr: self.server_addr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_construction() {
        let card = AgentCard {
            name: "test-agent".to_string(),
            description: "A test agent".to_string(),
            version: "0.1.0".to_string(),
            url: None,
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: false,
            },
            skills: vec![AgentSkill {
                id: "read_file".to_string(),
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
            }],
        };
        assert_eq!(card.name, "test-agent");
        assert!(card.capabilities.streaming);
        assert_eq!(card.skills.len(), 1);
    }

    #[tokio::test]
    async fn transport_discover() {
        let transport = A2aTransport::from_handle_info(
            "test".to_string(),
            "test agent".to_string(),
            vec![],
        );
        let card = transport.discover("http://localhost:8080").await.unwrap();
        assert!(card.url.is_some());
        assert!(transport.discovered().contains_key("http://localhost:8080"));
    }

    #[tokio::test]
    async fn transport_serve() {
        let mut transport = A2aTransport::from_handle_info(
            "test".to_string(),
            "test agent".to_string(),
            vec![],
        );
        let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        transport.serve(addr).await.unwrap();
        assert_eq!(transport.server_addr(), Some(addr));
        // Second serve should fail
        assert!(transport.serve(addr).await.is_err());
    }
}
