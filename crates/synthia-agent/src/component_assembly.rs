//! Agent component assembly.
//! Builder pattern for constructing fully-configured Agent instances.

use std::sync::Arc;

use synthia_command::CommandRegistry;
use synthia_context::ContextAssembler;
use synthia_core::tool::{
    extension_registry::{
        CommandStore,
        ExtensionRegistry,
        McpStore,
        ProviderStore,
    },
    fragment::FragmentRegistry,
};
use synthia_hook::HookRegistry;
use synthia_mcp::McpToolAdapter;
use synthia_provider::{
    registry::ProviderRegistry,
    router::ModelRouter,
    traits::ModelProvider,
};
use synthia_session::manager::SessionManager;
use synthia_tool::{Tool, ToolEntry, ToolRegistry};
use tracing::info;

use crate::{agent::Agent, steering::SteeringChannel, types::AgentConfig};

pub struct ComponentAssembler {
    config: AgentConfig,
    hook_registry: HookRegistry,
    tool_registry: Option<ToolRegistry>,
    command_registry: Option<CommandRegistry>,
    session_manager: Option<SessionManager>,
    context_assembler: Option<ContextAssembler>,
    model_router: Option<ModelRouter>,
    mcp_server_configs: Vec<synthia_mcp::McpServerConfig>,
    steering_channel: Option<Box<dyn SteeringChannel>>,
    provider_registry: Option<Arc<ProviderRegistry>>,
    provider: Option<Arc<dyn ModelProvider>>,
}

impl ComponentAssembler {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            hook_registry: HookRegistry::new(),
            tool_registry: None,
            command_registry: None,
            session_manager: None,
            context_assembler: None,
            model_router: None,
            mcp_server_configs: Vec::new(),
            steering_channel: None,
            provider_registry: None,
            provider: None,
        }
    }

    pub fn with_hooks(mut self, registry: HookRegistry) -> Self {
        self.hook_registry = registry;
        self
    }

    pub fn with_tools(mut self, registry: ToolRegistry) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_commands(mut self, registry: CommandRegistry) -> Self {
        self.command_registry = Some(registry);
        self
    }

    pub fn with_session_manager(mut self, manager: SessionManager) -> Self {
        self.session_manager = Some(manager);
        self
    }

    pub fn with_context_assembler(
        mut self,
        assembler: ContextAssembler,
    ) -> Self {
        self.context_assembler = Some(assembler);
        self
    }

    pub fn with_model_router(mut self, router: ModelRouter) -> Self {
        self.model_router = Some(router);
        self
    }

    /// Add MCP server configurations for discovery at build time.
    pub fn with_mcp_server_config(
        mut self,
        config: synthia_mcp::McpServerConfig,
    ) -> Self {
        self.mcp_server_configs.push(config);
        self
    }

    /// Set the steering channel for mid-loop agent control.
    pub fn with_steering_channel(
        mut self,
        channel: Box<dyn SteeringChannel>,
    ) -> Self {
        self.steering_channel = Some(channel);
        self
    }

    /// Set the provider registry.
    pub fn with_provider_registry(
        mut self,
        registry: ProviderRegistry,
    ) -> Self {
        self.provider_registry = Some(Arc::new(registry));
        self
    }

    /// Set the active model provider.
    pub fn with_provider(mut self, provider: Arc<dyn ModelProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    fn into_agent(
        self,
        session_manager: SessionManager,
        context_assembler: Arc<ContextAssembler>,
        model_router: Arc<ModelRouter>,
        tool_registry: ToolRegistry,
    ) -> Agent {
        // Build an ExtensionRegistry with its own synthia-core ToolRegistry
        // and a fresh FragmentRegistry. The legacy synthia-tool ToolRegistry
        // (held in `Agent::tool_registry`) coexists during the migration
        // period. The ExtensionRegistry provides the new Registry-First path
        // for fragments, skills, and plugins; the legacy registry handles
        // tool dispatch in the main loop.
        let ext_tool_registry =
            Arc::new(synthia_core::tool::registry::ToolRegistry::new());
        let fragment_registry = Arc::new(FragmentRegistry::new());
        let mut extension_registry =
            ExtensionRegistry::new(ext_tool_registry, fragment_registry);

        // Migrate provider_registry into ExtensionRegistry via the
        // ProviderStore trait object. The Arc is shared between
        // Agent::provider_registry and ExtensionRegistry so both
        // paths access the same data during the migration period.
        let provider_registry = self
            .provider_registry
            .unwrap_or_else(|| Arc::new(ProviderRegistry::default()));
        extension_registry.set_provider_store(
            Arc::clone(&provider_registry) as Arc<dyn ProviderStore>
        );

        // Migrate command_registry into ExtensionRegistry via the
        // CommandStore trait object. The Arc is shared between
        // Agent::command_registry and ExtensionRegistry so both
        // paths access the same data during the migration period.
        let command_registry = self.command_registry.unwrap_or_default();
        extension_registry.set_command_store(
            Arc::new(command_registry.clone()) as Arc<dyn CommandStore>,
        );

        Agent {
            config: self.config,
            provider_registry,
            provider: self.provider.expect(
                "ComponentAssembler: provider is required; call with_provider() before build()",
            ),
            tool_registry,
            hook_registry: Arc::new(self.hook_registry),
            command_registry: command_registry.clone(),
            session_manager,
            context_assembler,
            model_router,
            session_store: synthia_session::Store::new(std::path::PathBuf::from(
                ".synthia/sessions",
            )),
            mcp_manager: None,
            steering_channel: self.steering_channel.map(std::sync::Arc::from),
            config_watcher: None,
            memory_event_sender: None,
            approval_service: None,
            sandbox_manager: None,
            extension_registry: Some(extension_registry),
        }
    }

    /// Build the Agent instance, performing MCP discovery if server configs were provided.
    pub fn build(self) -> Agent {
        info!(
            model = self.config.model,
            max_iterations = self.config.max_iterations,
            "Assembling agent components"
        );

        // Extract all Option fields first to avoid partial-move issues
        let config = &self.config;
        let session_manager = self.session_manager.unwrap_or_else(|| {
            SessionManager::new(std::path::PathBuf::from(".synthia/sessions"))
        });
        let context_assembler = Arc::new(
            self.context_assembler
                .unwrap_or_else(|| ContextAssembler::new(4096)),
        );
        let model_router = Arc::new(self.model_router.unwrap_or_default());
        let tool_registry = self.tool_registry.unwrap_or_default();

        // Now consume remaining fields - the ones above are already moved
        let consumed = ComponentAssembler {
            config: config.clone(),
            hook_registry: self.hook_registry,
            tool_registry: None,
            command_registry: self.command_registry,
            session_manager: None,
            context_assembler: None,
            model_router: None,
            mcp_server_configs: Vec::new(),
            steering_channel: self.steering_channel,
            provider_registry: self.provider_registry,
            provider: self.provider,
        };

        consumed.into_agent(
            session_manager,
            context_assembler,
            model_router,
            tool_registry,
        )
    }

    /// Async build that performs MCP tool discovery and registers tools.
    pub async fn build_with_discovery(self) -> Agent {
        let tool_registry = self.tool_registry.unwrap_or_default();
        let mut mcp_manager_arc: Option<Arc<synthia_mcp::McpManager>> = None;

        if !self.mcp_server_configs.is_empty() {
            let manager = Arc::new(synthia_mcp::McpManager::new());
            for config in &self.mcp_server_configs {
                manager.register_config(config.clone()).await;
            }

            match manager.discover_tools().await {
                Ok(discovered) => {
                    for (server_name, tools) in &discovered {
                        info!(
                            server = %server_name,
                            count = tools.len(),
                            "MCP server discovered tools"
                        );
                    }

                    // Create adapters and register them
                    for (server_name, tools) in discovered {
                        for tool_summary in tools {
                            let tool_definition = synthia_mcp::ToolDefinition {
                                name: tool_summary.name.clone(),
                                description: tool_summary.description.clone(),
                                input_schema: serde_json::json!({}),
                            };
                            let adapter = Arc::new(McpToolAdapter::new(
                                server_name.clone(),
                                tool_definition,
                                manager.clone(),
                            ));
                            let name = adapter.name().to_string();
                            tool_registry.register(ToolEntry::new(
                                adapter as Arc<dyn Tool>,
                            ));
                            info!(tool = %name, "Registered MCP tool in ToolRegistry");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "MCP tool discovery failed");
                }
            }

            mcp_manager_arc = Some(manager);
        }

        info!(
            model = self.config.model,
            max_iterations = self.config.max_iterations,
            "Assembling agent components with MCP discovery"
        );

        // Extract all Option fields first to avoid partial-move issues
        let config = &self.config;
        let session_manager = self.session_manager.unwrap_or_else(|| {
            SessionManager::new(std::path::PathBuf::from(".synthia/sessions"))
        });
        let context_assembler = Arc::new(
            self.context_assembler
                .unwrap_or_else(|| ContextAssembler::new(4096)),
        );
        let model_router = Arc::new(self.model_router.unwrap_or_default());

        // Now consume remaining fields - the ones above are already moved
        let consumed = ComponentAssembler {
            config: config.clone(),
            hook_registry: self.hook_registry,
            tool_registry: None,
            command_registry: self.command_registry,
            session_manager: None,
            context_assembler: None,
            model_router: None,
            mcp_server_configs: Vec::new(),
            steering_channel: self.steering_channel,
            provider_registry: self.provider_registry,
            provider: self.provider,
        };

        let mut agent = consumed.into_agent(
            session_manager,
            context_assembler,
            model_router,
            tool_registry,
        );

        // Migrate mcp_manager into Agent and ExtensionRegistry.
        if let Some(manager) = mcp_manager_arc {
            agent.mcp_manager = Some(manager.clone());
            if let Some(ref mut ext_reg) = agent.extension_registry {
                ext_reg.set_mcp_store(manager as Arc<dyn McpStore>);
            }
        }

        agent
    }
}
