//! AgentHandle — 无状态推理句柄，可跨 N 个 AgentSession 复用。
//!
//! AgentHandle 持有 agent 的能力（config, provider, tools, hooks），
//! 不持有任何运行时状态（session, loop context, history）。
//! 一个 AgentHandle 可以被多个 AgentSession 共享。

use std::sync::Arc;

use synthia_context::assembler::ContextAssembler;
use synthia_core::registry::RegistryItem;
use synthia_hook::HookRegistry;
use synthia_permission::ApprovalService;
use synthia_provider::{
    registry::ProviderRegistry,
    router::ModelRouter,
    traits::ModelProvider,
};
use synthia_sandbox::SandboxManager;
use synthia_session::Store as SessionStore;
use synthia_tool::registry::ToolRegistry;
use tokio::sync::mpsc;

use crate::config::AgentConfig;

/// 无状态推理句柄 — 跨 N 个 AgentSession 复用。
///
/// 持有 agent 的所有能力（静态配置、LLM provider、工具、钩子），
/// 不持有任何运行时状态。调用 `run()` 时创建 AgentSession，
/// 调用 `resume()` 时基于已有 AgentSession 继续执行。
pub struct AgentHandle {
    /// Agent 唯一标识。
    pub id: String,
    /// Agent 描述（用于 RegistryItem 和 A2A AgentCard）。
    pub description: String,
    /// 静态配置（模型、最大 token、最大迭代等）。
    pub config: AgentConfig,
    /// Provider 注册表（多模型路由，Arc 包装以支持 Clone）。
    pub provider_registry: Arc<ProviderRegistry>,
    /// 当前使用的 LLM provider。
    pub provider: Arc<dyn ModelProvider>,
    /// 工具注册表。
    pub tool_registry: ToolRegistry,
    /// 钩子注册表。
    pub hook_registry: Arc<HookRegistry>,
    /// 上下文组装器。
    pub context_assembler: Arc<ContextAssembler>,
    /// 模型路由器。
    pub model_router: Arc<ModelRouter>,
    /// Session 存储。
    pub session_store: SessionStore,
    /// MCP 管理器（Arc 包装以支持 Clone）。
    pub mcp_manager: Option<Arc<synthia_mcp::McpManager>>,
    /// 审批服务。
    pub approval_service: Option<Arc<dyn ApprovalService>>,
    /// 沙箱管理器。
    pub sandbox_manager: Option<Arc<dyn SandboxManager>>,
    /// Memory 事件发送器。
    pub memory_event_sender:
        Option<mpsc::Sender<synthia_memory::types::MemoryEvent>>,
    /// A2A AgentCard（如果此 agent 暴露为 A2A server）。
    pub a2a_card: Option<serde_json::Value>,
}

impl Clone for AgentHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            description: self.description.clone(),
            config: self.config.clone(),
            provider_registry: self.provider_registry.clone(),
            provider: self.provider.clone(),
            tool_registry: self.tool_registry.clone(),
            hook_registry: self.hook_registry.clone(),
            context_assembler: self.context_assembler.clone(),
            model_router: self.model_router.clone(),
            session_store: self.session_store.clone(),
            mcp_manager: self.mcp_manager.clone(),
            approval_service: self.approval_service.clone(),
            sandbox_manager: self.sandbox_manager.clone(),
            memory_event_sender: self.memory_event_sender.clone(),
            a2a_card: self.a2a_card.clone(),
        }
    }
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHandle")
            .field("id", &self.id)
            .field("description", &self.description)
            .field("config", &self.config)
            .field("a2a_card", &self.a2a_card)
            .finish()
    }
}

impl RegistryItem for AgentHandle {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl AgentHandle {
    /// 创建 AgentHandle 构建器。
    pub fn builder(
        id: impl Into<String>,
        config: AgentConfig,
    ) -> AgentHandleBuilder {
        let id = id.into();
        let description = id.clone();
        AgentHandleBuilder {
            id,
            description,
            config,
            provider_registry: Arc::new(ProviderRegistry::new()),
            provider: None,
            tool_registry: ToolRegistry::new(),
            hook_registry: Arc::new(HookRegistry::new()),
            context_assembler: None,
            model_router: None,
            session_store: None,
            mcp_manager: None,
            approval_service: None,
            sandbox_manager: None,
            memory_event_sender: None,
            a2a_card: None,
        }
    }
}

/// AgentHandle 构建器。
pub struct AgentHandleBuilder {
    id: String,
    description: String,
    config: AgentConfig,
    provider_registry: Arc<ProviderRegistry>,
    provider: Option<Arc<dyn ModelProvider>>,
    tool_registry: ToolRegistry,
    hook_registry: Arc<HookRegistry>,
    context_assembler: Option<Arc<ContextAssembler>>,
    model_router: Option<Arc<ModelRouter>>,
    session_store: Option<SessionStore>,
    mcp_manager: Option<Arc<synthia_mcp::McpManager>>,
    approval_service: Option<Arc<dyn ApprovalService>>,
    sandbox_manager: Option<Arc<dyn SandboxManager>>,
    memory_event_sender:
        Option<mpsc::Sender<synthia_memory::types::MemoryEvent>>,
    a2a_card: Option<serde_json::Value>,
}

impl AgentHandleBuilder {
    pub fn description(mut self, v: impl Into<String>) -> Self {
        self.description = v.into();
        self
    }

    pub fn provider_registry(mut self, v: ProviderRegistry) -> Self {
        self.provider_registry = Arc::new(v);
        self
    }

    pub fn provider(mut self, v: Arc<dyn ModelProvider>) -> Self {
        self.provider = Some(v);
        self
    }

    pub fn tool_registry(mut self, v: ToolRegistry) -> Self {
        self.tool_registry = v;
        self
    }

    pub fn hook_registry(mut self, v: Arc<HookRegistry>) -> Self {
        self.hook_registry = v;
        self
    }

    pub fn context_assembler(mut self, v: Arc<ContextAssembler>) -> Self {
        self.context_assembler = Some(v);
        self
    }

    pub fn model_router(mut self, v: Arc<ModelRouter>) -> Self {
        self.model_router = Some(v);
        self
    }

    pub fn session_store(mut self, v: SessionStore) -> Self {
        self.session_store = Some(v);
        self
    }

    pub fn mcp_manager(mut self, v: Arc<synthia_mcp::McpManager>) -> Self {
        self.mcp_manager = Some(v);
        self
    }

    pub fn approval_service(mut self, v: Arc<dyn ApprovalService>) -> Self {
        self.approval_service = Some(v);
        self
    }

    pub fn sandbox_manager(mut self, v: Arc<dyn SandboxManager>) -> Self {
        self.sandbox_manager = Some(v);
        self
    }

    pub fn memory_event_sender(
        mut self,
        v: mpsc::Sender<synthia_memory::types::MemoryEvent>,
    ) -> Self {
        self.memory_event_sender = Some(v);
        self
    }

    pub fn a2a_card(mut self, v: serde_json::Value) -> Self {
        self.a2a_card = Some(v);
        self
    }

    /// 构建 AgentHandle。provider, context_assembler, model_router, session_store 为必填。
    pub fn build(self) -> Result<AgentHandle, &'static str> {
        let provider = self.provider.ok_or("provider is required")?;
        let context_assembler = self
            .context_assembler
            .ok_or("context_assembler is required")?;
        let model_router =
            self.model_router.ok_or("model_router is required")?;
        let session_store =
            self.session_store.ok_or("session_store is required")?;

        Ok(AgentHandle {
            id: self.id,
            description: self.description,
            config: self.config,
            provider_registry: self.provider_registry,
            provider,
            tool_registry: self.tool_registry,
            hook_registry: self.hook_registry,
            context_assembler,
            model_router,
            session_store,
            mcp_manager: self.mcp_manager,
            approval_service: self.approval_service,
            sandbox_manager: self.sandbox_manager,
            memory_event_sender: self.memory_event_sender,
            a2a_card: self.a2a_card,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_builder_requires_provider() {
        let config = AgentConfig::default();
        let result = AgentHandle::builder("test", config).build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "provider is required");
    }

    #[test]
    fn handle_builder_requires_context_assembler() {
        let config = AgentConfig::default();
        // provider alone is not enough
        let result = AgentHandle::builder("test", config).build();
        assert!(result.is_err());
    }
}
