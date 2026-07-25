//! PluginRegistry — dynamic discovery and loading of third-party extension packages.

use std::{collections::HashMap, path::Path, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing;

use super::{
    fragment::{ContextFragment, FragmentRegistry},
    provider::ToolProvider,
    registry::{RegistrationScope, ToolRegistry},
    skill_registry::{Skill, SkillRegistry},
};

/// Plugin capability summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilitySummary {
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub fragments: Vec<String>,
}

/// Plugin trait.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Plugin ID (unique).
    fn id(&self) -> &str;
    /// Plugin version.
    fn version(&self) -> &str;
    /// Plugin description.
    fn description(&self) -> &str;
    /// Plugin capability summary.
    fn capabilities(&self) -> PluginCapabilitySummary;
    /// Initialize the plugin (register tools, skills, fragments, etc.).
    async fn initialize(&self) -> Result<(), PluginError>;
    /// Shutdown the plugin (cleanup resources).
    async fn shutdown(&self) -> Result<(), PluginError>;
    /// Tool providers exposed by this plugin.
    async fn tools(&self) -> Vec<Arc<dyn ToolProvider>> {
        vec![]
    }
    /// Skills exposed by this plugin.
    async fn skills(&self) -> Vec<Arc<dyn Skill>> {
        vec![]
    }
    /// Context fragments exposed by this plugin.
    async fn fragments(&self) -> Vec<Arc<dyn ContextFragment>> {
        vec![]
    }
}

/// Plugin error type.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),
    #[error("Plugin initialization failed: {0}")]
    InitFailed(String),
    #[error("Plugin shutdown failed: {0}")]
    ShutdownFailed(String),
    #[error("Plugin discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("Plugin descriptor error: {0}")]
    DescriptorError(String),
}

/// Plugin state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    Discovered,
    Loaded,
    Initialized,
    Failed,
    Unloaded,
}

/// Tracks per-plugin registration state for cross-dimension cleanup.
struct PluginRegState {
    /// Registration scopes for tool providers — dropped on unload to auto-unregister.
    tool_scopes: Vec<RegistrationScope>,
    /// Skill names registered by this plugin.
    skill_names: Vec<String>,
    /// Fragment names registered by this plugin.
    fragment_names: Vec<String>,
}

/// Plugin descriptor parsed from `plugin.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: PluginDescriptorId,
}

/// The `[id]` section of a `plugin.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDescriptorId {
    pub name: String,
    pub version: String,
}

/// Discovered plugin path (directory containing a `plugin.toml`).
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Directory containing the `plugin.toml`.
    pub dir: std::path::PathBuf,
    /// Parsed descriptor.
    pub descriptor: PluginDescriptor,
}

/// Registry for dynamic discovery and loading of third-party plugin packages.
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
    states: RwLock<HashMap<String, PluginState>>,
    reg_states: RwLock<HashMap<String, PluginRegState>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    skill_registry: Option<Arc<SkillRegistry>>,
    fragment_registry: Option<Arc<FragmentRegistry>>,
}

impl PluginRegistry {
    /// Create a new empty registry (no cross-dimension coordination).
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            reg_states: RwLock::new(HashMap::new()),
            tool_registry: None,
            skill_registry: None,
            fragment_registry: None,
        }
    }

    /// Create a registry with cross-dimension coordination references.
    pub fn with_registries(
        tool_registry: Arc<ToolRegistry>,
        skill_registry: Arc<SkillRegistry>,
        fragment_registry: Arc<FragmentRegistry>,
    ) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            reg_states: RwLock::new(HashMap::new()),
            tool_registry: Some(tool_registry),
            skill_registry: Some(skill_registry),
            fragment_registry: Some(fragment_registry),
        }
    }

    /// Load a plugin into the registry and register its tools/skills/fragments.
    pub async fn load(
        &self,
        plugin: Arc<dyn Plugin>,
    ) -> Result<(), PluginError> {
        let id = plugin.id().to_string();
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&id) {
                return Err(PluginError::AlreadyLoaded(id));
            }
        }

        // Register cross-dimension items before inserting the plugin.
        let reg_state = self.register_plugin_dimensions(&*plugin).await?;

        self.plugins.write().await.insert(id.clone(), plugin);
        self.states
            .write()
            .await
            .insert(id.clone(), PluginState::Loaded);
        self.reg_states.write().await.insert(id, reg_state);
        Ok(())
    }

    /// Initialize a loaded plugin by ID.
    pub async fn initialize(&self, id: &str) -> Result<(), PluginError> {
        let plugin = {
            let plugins = self.plugins.read().await;
            plugins
                .get(id)
                .cloned()
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?
        };
        if let Err(e) = plugin.initialize().await {
            self.states
                .write()
                .await
                .insert(id.to_string(), PluginState::Failed);
            return Err(PluginError::InitFailed(e.to_string()));
        }
        self.states
            .write()
            .await
            .insert(id.to_string(), PluginState::Initialized);
        Ok(())
    }

    /// Initialize all loaded but not yet initialized plugins.
    pub async fn initialize_all(&self) -> Vec<Result<(), PluginError>> {
        let ids: Vec<String> = {
            let states = self.states.read().await;
            states
                .iter()
                .filter(|(_, state)| **state == PluginState::Loaded)
                .map(|(id, _)| id.clone())
                .collect()
        };
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            results.push(self.initialize(&id).await);
        }
        results
    }

    /// Unload a plugin (call shutdown, unregister cross-dimension items, then remove).
    pub async fn unload(&self, id: &str) -> Result<(), PluginError> {
        let plugin = {
            let plugins = self.plugins.read().await;
            plugins
                .get(id)
                .cloned()
                .ok_or_else(|| PluginError::NotFound(id.to_string()))?
        };
        if let Err(e) = plugin.shutdown().await {
            self.states
                .write()
                .await
                .insert(id.to_string(), PluginState::Failed);
            return Err(PluginError::ShutdownFailed(e.to_string()));
        }

        // Unregister cross-dimension items by dropping the reg_state.
        self.unregister_plugin_dimensions(id).await;

        self.plugins.write().await.remove(id);
        self.states.write().await.remove(id);
        Ok(())
    }

    /// Get a plugin by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.read().await.get(id).cloned()
    }

    /// Get the state of a plugin by ID.
    pub async fn state(&self, id: &str) -> Option<PluginState> {
        self.states.read().await.get(id).copied()
    }

    /// List all plugin IDs.
    pub async fn list(&self) -> Vec<String> {
        self.plugins.read().await.keys().cloned().collect()
    }

    /// Get capability summaries for all plugins.
    pub async fn capability_summaries(
        &self,
    ) -> HashMap<String, PluginCapabilitySummary> {
        let plugins = self.plugins.read().await;
        plugins
            .iter()
            .map(|(id, p)| (id.clone(), p.capabilities()))
            .collect()
    }

    /// Get the number of plugins.
    pub async fn plugin_count(&self) -> usize {
        self.plugins.read().await.len()
    }

    /// Shutdown all plugins.
    pub async fn shutdown_all(&self) -> Vec<Result<(), PluginError>> {
        let ids: Vec<String> =
            self.plugins.read().await.keys().cloned().collect();
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            let plugin = {
                let plugins = self.plugins.read().await;
                plugins.get(&id).cloned()
            };
            if let Some(plugin) = plugin {
                if let Err(e) = plugin.shutdown().await {
                    self.states
                        .write()
                        .await
                        .insert(id.clone(), PluginState::Failed);
                    results
                        .push(Err(PluginError::ShutdownFailed(e.to_string())));
                } else {
                    self.unregister_plugin_dimensions(&id).await;
                    self.states
                        .write()
                        .await
                        .insert(id.clone(), PluginState::Unloaded);
                    results.push(Ok(()));
                }
            }
        }
        results
    }

    /// Scan a directory for plugin descriptor files (`plugin.toml`).
    ///
    /// Returns a list of discovered plugin paths. Does NOT auto-load — the
    /// caller decides whether to load them.
    pub fn discover(dir: &Path) -> Result<Vec<DiscoveredPlugin>, PluginError> {
        if !dir.exists() {
            return Err(PluginError::DiscoveryFailed(format!(
                "directory does not exist: {}",
                dir.display()
            )));
        }
        if !dir.is_dir() {
            return Err(PluginError::DiscoveryFailed(format!(
                "path is not a directory: {}",
                dir.display()
            )));
        }

        let mut discovered = Vec::new();

        let entries = std::fs::read_dir(dir).map_err(|e| {
            PluginError::DiscoveryFailed(format!(
                "failed to read directory {}: {e}",
                dir.display()
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                PluginError::DiscoveryFailed(format!(
                    "failed to read directory entry: {e}"
                ))
            })?;
            let path = entry.path();
            let toml_path = path.join("plugin.toml");

            if toml_path.is_file() {
                match Self::parse_plugin_descriptor(&toml_path) {
                    Ok(descriptor) => {
                        tracing::info!(
                            path = %path.display(),
                            name = %descriptor.id.name,
                            version = %descriptor.id.version,
                            "discovered plugin"
                        );
                        discovered.push(DiscoveredPlugin {
                            dir: path,
                            descriptor,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %toml_path.display(),
                            error = %e,
                            "skipping invalid plugin descriptor"
                        );
                    }
                }
            }
        }

        Ok(discovered)
    }

    // ── Private helpers ────────────────────────────────────────────────────

    /// Register a plugin's tools, skills, and fragments in their respective
    /// registries. Returns the per-plugin reg state for later cleanup.
    async fn register_plugin_dimensions(
        &self,
        plugin: &dyn Plugin,
    ) -> Result<PluginRegState, PluginError> {
        let plugin_id = plugin.id().to_string();
        let mut tool_scopes = Vec::new();
        let mut skill_names = Vec::new();
        let mut fragment_names = Vec::new();

        // Register tool providers.
        if let Some(tool_reg) = &self.tool_registry {
            let providers = plugin.tools().await;
            for provider in providers {
                let scope = tool_reg
                    .register_scoped_with_namespace(provider, &plugin_id)
                    .await
                    .map_err(|e| {
                        PluginError::InitFailed(format!(
                            "failed to register tools for plugin {plugin_id}: {e}"
                        ))
                    })?;
                tracing::info!(
                    plugin = %plugin_id,
                    tool_count = scope.tool_names().len(),
                    "registered plugin tools via scoped registration"
                );
                tool_scopes.push(scope);
            }
        }

        // Register skills.
        if let Some(skill_reg) = &self.skill_registry {
            let skills = plugin.skills().await;
            for skill in &skills {
                let name = skill.name().to_string();
                if let Err(e) = skill_reg.register(skill.clone()).await {
                    tracing::warn!(
                        plugin = %plugin_id,
                        skill = %name,
                        error = %e,
                        "failed to register plugin skill (may already exist)"
                    );
                } else {
                    tracing::info!(
                        plugin = %plugin_id,
                        skill = %name,
                        "registered plugin skill"
                    );
                    skill_names.push(name);
                }
            }
        }

        // Register fragments.
        if let Some(frag_reg) = &self.fragment_registry {
            let fragments = plugin.fragments().await;
            for fragment in &fragments {
                let name = fragment.name().to_string();
                if let Err(e) = frag_reg.register(fragment.clone()).await {
                    tracing::warn!(
                        plugin = %plugin_id,
                        fragment = %name,
                        error = %e,
                        "failed to register plugin fragment (may already exist)"
                    );
                } else {
                    tracing::info!(
                        plugin = %plugin_id,
                        fragment = %name,
                        "registered plugin fragment"
                    );
                    fragment_names.push(name);
                }
            }
        }

        Ok(PluginRegState {
            tool_scopes,
            skill_names,
            fragment_names,
        })
    }

    /// Unregister all cross-dimension items for a plugin.
    ///
    /// For tools, dropping the `RegistrationScope` auto-unregisters.
    /// For skills and fragments, explicit unregistration is needed.
    async fn unregister_plugin_dimensions(&self, id: &str) {
        let reg_state = self.reg_states.write().await.remove(id);

        let Some(reg_state) = reg_state else {
            return;
        };

        // Drop tool scopes — RAII auto-unregisters from ToolRegistry.
        for scope in reg_state.tool_scopes {
            scope.close();
        }

        // Unregister skills.
        if let Some(skill_reg) = &self.skill_registry {
            for name in &reg_state.skill_names {
                if let Err(e) = skill_reg.unregister(name).await {
                    tracing::warn!(
                        plugin = %id,
                        skill = %name,
                        error = %e,
                        "failed to unregister plugin skill on unload"
                    );
                } else {
                    tracing::info!(
                        plugin = %id,
                        skill = %name,
                        "unregistered plugin skill"
                    );
                }
            }
        }

        // Unregister fragments.
        if let Some(frag_reg) = &self.fragment_registry {
            for name in &reg_state.fragment_names {
                if frag_reg.unregister(name).await {
                    tracing::info!(
                        plugin = %id,
                        fragment = %name,
                        "unregistered plugin fragment"
                    );
                } else {
                    tracing::warn!(
                        plugin = %id,
                        fragment = %name,
                        "failed to unregister plugin fragment on unload (not found)"
                    );
                }
            }
        }
    }

    /// Parse a `plugin.toml` file into a `PluginDescriptor`.
    fn parse_plugin_descriptor(
        path: &Path,
    ) -> Result<PluginDescriptor, PluginError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            PluginError::DescriptorError(format!(
                "failed to read {}: {e}",
                path.display()
            ))
        })?;
        let descriptor: PluginDescriptor =
            toml::from_str(&content).map_err(|e| {
                PluginError::DescriptorError(format!(
                    "failed to parse {}: {e}",
                    path.display()
                ))
            })?;
        Ok(descriptor)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{
        descriptor::{Tool, ToolDescriptor, ToolExposure, ToolProvenance},
        fragment::{ContextFragment, FragmentContext, FragmentError},
        provider::ToolProvider,
        skill_registry::{Skill, SkillProvenance},
        tool_name::ToolName,
        types::{ToolContext, ToolError, ToolInput, ToolOutput},
    };

    // ── Test helpers ───────────────────────────────────────────────────────

    /// A simple test plugin for unit testing.
    struct TestPlugin {
        id: String,
        version: String,
        description: String,
        capabilities: PluginCapabilitySummary,
    }

    impl TestPlugin {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                version: "0.1.0".to_string(),
                description: format!("Test plugin {id}"),
                capabilities: PluginCapabilitySummary {
                    tools: vec![format!("{id}:tool1")],
                    skills: vec![format!("{id}:skill1")],
                    fragments: vec![format!("{id}:fragment1")],
                },
            }
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn id(&self) -> &str {
            &self.id
        }

        fn version(&self) -> &str {
            &self.version
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn capabilities(&self) -> PluginCapabilitySummary {
            self.capabilities.clone()
        }

        async fn initialize(&self) -> Result<(), PluginError> {
            Ok(())
        }

        async fn shutdown(&self) -> Result<(), PluginError> {
            Ok(())
        }
    }

    /// A minimal Tool for testing.
    struct StubTool {
        tool_name: String,
    }

    #[async_trait]
    impl Tool for StubTool {
        fn name(&self) -> &str {
            &self.tool_name
        }

        async fn execute(
            &self,
            _input: ToolInput,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::default())
        }

        fn descriptor(&self) -> &ToolDescriptor {
            static DESC: std::sync::OnceLock<ToolDescriptor> =
                std::sync::OnceLock::new();
            DESC.get_or_init(|| ToolDescriptor {
                name: ToolName::plain("stub"),
                description: "stub tool".to_string(),
                parameters: serde_json::Value::Null,
                category: crate::tool::descriptor::ToolCategory::Utility,
                provenance: ToolProvenance::Plugin {
                    id: "test".to_string(),
                },
                execution_mode:
                    crate::tool::descriptor::ExecutionMode::Parallel,
                cancel_behavior:
                    crate::tool::descriptor::CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: ToolExposure::Direct,
            })
        }
    }

    /// A minimal ToolProvider for testing plugin tool registration.
    struct StubToolProvider {
        id: String,
        tool_name: String,
    }

    impl StubToolProvider {
        fn new(id: &str, tool_name: &str) -> Self {
            Self {
                id: id.to_string(),
                tool_name: tool_name.to_string(),
            }
        }
    }

    #[async_trait]
    impl ToolProvider for StubToolProvider {
        fn id(&self) -> &str {
            &self.id
        }

        async fn list_tools(&self) -> Vec<ToolDescriptor> {
            vec![ToolDescriptor {
                name: ToolName::plain(&self.tool_name),
                description: "stub provider tool".to_string(),
                parameters: serde_json::Value::Null,
                category: crate::tool::descriptor::ToolCategory::Utility,
                provenance: ToolProvenance::Plugin {
                    id: self.id.clone(),
                },
                execution_mode:
                    crate::tool::descriptor::ExecutionMode::Parallel,
                cancel_behavior:
                    crate::tool::descriptor::CancelBehavior::Cooperative,
                examples: vec![],
                permission_required: false,
                prompt_visible_provenance: true,
                is_hidden: false,
                is_user_invocable: true,
                exposure: ToolExposure::Direct,
            }]
        }

        async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
            if name == self.tool_name {
                Some(Arc::new(StubTool {
                    tool_name: self.tool_name.clone(),
                }))
            } else {
                None
            }
        }
    }

    /// A minimal Skill for testing plugin skill registration.
    struct StubSkill {
        name: String,
        provenance: SkillProvenance,
    }

    impl StubSkill {
        fn new(name: &str, plugin_id: &str) -> Self {
            Self {
                name: name.to_string(),
                provenance: SkillProvenance::Plugin {
                    id: plugin_id.to_string(),
                },
            }
        }
    }

    #[async_trait]
    impl Skill for StubSkill {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "stub skill"
        }

        fn instructions(&self) -> &str {
            "stub instructions"
        }

        fn tools(&self) -> Vec<ToolName> {
            vec![]
        }

        fn provenance(&self) -> &SkillProvenance {
            &self.provenance
        }

        async fn detect_invocation(&self, _user_input: &str) -> f64 {
            0.0
        }
    }

    /// A minimal ContextFragment for testing plugin fragment registration.
    struct StubFragment {
        name: String,
    }

    impl StubFragment {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl ContextFragment for StubFragment {
        fn name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> u32 {
            100
        }

        fn is_active(&self) -> bool {
            true
        }

        async fn render(
            &self,
            _ctx: &FragmentContext,
        ) -> Result<String, FragmentError> {
            Ok(format!("fragment {}", self.name))
        }
    }

    /// A test plugin that provides tools, skills, and fragments.
    struct FullPlugin {
        id: String,
        version: String,
        description: String,
        capabilities: PluginCapabilitySummary,
    }

    impl FullPlugin {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                version: "0.1.0".to_string(),
                description: format!("Full plugin {id}"),
                capabilities: PluginCapabilitySummary {
                    tools: vec![format!("{id}-tool")],
                    skills: vec![format!("{id}-skill")],
                    fragments: vec![format!("{id}-frag")],
                },
            }
        }
    }

    #[async_trait]
    impl Plugin for FullPlugin {
        fn id(&self) -> &str {
            &self.id
        }

        fn version(&self) -> &str {
            &self.version
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn capabilities(&self) -> PluginCapabilitySummary {
            self.capabilities.clone()
        }

        async fn initialize(&self) -> Result<(), PluginError> {
            Ok(())
        }

        async fn shutdown(&self) -> Result<(), PluginError> {
            Ok(())
        }

        async fn tools(&self) -> Vec<Arc<dyn ToolProvider>> {
            vec![Arc::new(StubToolProvider::new(
                &self.id,
                &format!("{}-tool", self.id),
            ))]
        }

        async fn skills(&self) -> Vec<Arc<dyn Skill>> {
            vec![Arc::new(StubSkill::new(
                &format!("{}-skill", self.id),
                &self.id,
            ))]
        }

        async fn fragments(&self) -> Vec<Arc<dyn ContextFragment>> {
            vec![Arc::new(StubFragment::new(&format!("{}-frag", self.id)))]
        }
    }

    // ── Basic PluginRegistry tests (existing) ──────────────────────────────

    #[tokio::test]
    async fn new_registry_is_empty() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count().await, 0);
        assert!(registry.list().await.is_empty());
    }

    #[tokio::test]
    async fn load_plugin_adds_to_registry() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("test-plugin"));
        let result = registry.load(plugin).await;
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count().await, 1);
        assert_eq!(
            registry.state("test-plugin").await,
            Some(PluginState::Loaded)
        );
    }

    #[tokio::test]
    async fn load_duplicate_returns_error() {
        let registry = PluginRegistry::new();
        let plugin1 = Arc::new(TestPlugin::new("dup-plugin"));
        let plugin2 = Arc::new(TestPlugin::new("dup-plugin"));
        assert!(registry.load(plugin1).await.is_ok());
        let result = registry.load(plugin2).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::AlreadyLoaded(id) => assert_eq!(id, "dup-plugin"),
            other => panic!("Expected AlreadyLoaded, got: {other}"),
        }
    }

    #[tokio::test]
    async fn initialize_plugin() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("init-plugin"));
        registry.load(plugin).await.unwrap();
        let result = registry.initialize("init-plugin").await;
        assert!(result.is_ok());
        assert_eq!(
            registry.state("init-plugin").await,
            Some(PluginState::Initialized)
        );
    }

    #[tokio::test]
    async fn initialize_nonexistent_returns_error() {
        let registry = PluginRegistry::new();
        let result = registry.initialize("no-such-plugin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::NotFound(id) => assert_eq!(id, "no-such-plugin"),
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn unload_plugin_removes_from_registry() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin::new("unload-plugin"));
        registry.load(plugin).await.unwrap();
        let result = registry.unload("unload-plugin").await;
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count().await, 0);
        assert!(registry.get("unload-plugin").await.is_none());
        assert!(registry.state("unload-plugin").await.is_none());
    }

    #[tokio::test]
    async fn unload_nonexistent_returns_error() {
        let registry = PluginRegistry::new();
        let result = registry.unload("ghost-plugin").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::NotFound(id) => assert_eq!(id, "ghost-plugin"),
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn list_returns_all_ids() {
        let registry = PluginRegistry::new();
        registry.load(Arc::new(TestPlugin::new("a"))).await.unwrap();
        registry.load(Arc::new(TestPlugin::new("b"))).await.unwrap();
        registry.load(Arc::new(TestPlugin::new("c"))).await.unwrap();
        let mut ids = registry.list().await;
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn capability_summaries() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("cap-plugin")))
            .await
            .unwrap();
        let summaries = registry.capability_summaries().await;
        assert!(summaries.contains_key("cap-plugin"));
        let summary = &summaries["cap-plugin"];
        assert_eq!(summary.tools, vec!["cap-plugin:tool1"]);
        assert_eq!(summary.skills, vec!["cap-plugin:skill1"]);
        assert_eq!(summary.fragments, vec!["cap-plugin:fragment1"]);
    }

    #[tokio::test]
    async fn plugin_count() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count().await, 0);
        registry
            .load(Arc::new(TestPlugin::new("p1")))
            .await
            .unwrap();
        assert_eq!(registry.plugin_count().await, 1);
        registry
            .load(Arc::new(TestPlugin::new("p2")))
            .await
            .unwrap();
        assert_eq!(registry.plugin_count().await, 2);
        registry.unload("p1").await.unwrap();
        assert_eq!(registry.plugin_count().await, 1);
    }

    #[tokio::test]
    async fn initialize_all_initializes_loaded_plugins() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("ia1")))
            .await
            .unwrap();
        registry
            .load(Arc::new(TestPlugin::new("ia2")))
            .await
            .unwrap();
        let results = registry.initialize_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(registry.state("ia1").await, Some(PluginState::Initialized));
        assert_eq!(registry.state("ia2").await, Some(PluginState::Initialized));
    }

    #[tokio::test]
    async fn shutdown_all_shuts_down_all_plugins() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("sa1")))
            .await
            .unwrap();
        registry
            .load(Arc::new(TestPlugin::new("sa2")))
            .await
            .unwrap();
        let results = registry.shutdown_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(registry.state("sa1").await, Some(PluginState::Unloaded));
        assert_eq!(registry.state("sa2").await, Some(PluginState::Unloaded));
    }

    #[tokio::test]
    async fn get_returns_plugin() {
        let registry = PluginRegistry::new();
        registry
            .load(Arc::new(TestPlugin::new("get-plugin")))
            .await
            .unwrap();
        let plugin = registry.get("get-plugin").await;
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().id(), "get-plugin");
        assert!(registry.get("nonexistent").await.is_none());
    }

    // ── Cross-dimension registration tests ─────────────────────────────────

    #[tokio::test]
    async fn with_registries_creates_coordinated_registry() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        assert_eq!(plugin_reg.plugin_count().await, 0);
    }

    #[tokio::test]
    async fn load_registers_tools_in_tool_registry() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("tool-test"));
        plugin_reg.load(plugin).await.unwrap();

        // Tool should be registered in the ToolRegistry
        assert!(tool_reg.resolve_now("tool-test-tool").is_some());
        assert_eq!(tool_reg.tool_count(), 1);
    }

    #[tokio::test]
    async fn load_registers_skills_in_skill_registry() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("skill-test"));
        plugin_reg.load(plugin).await.unwrap();

        // Skill should be registered in the SkillRegistry
        let skill = skill_reg.get("skill-test-skill").await;
        assert!(skill.is_some());
        assert_eq!(skill_reg.skill_count().await, 1);
    }

    #[tokio::test]
    async fn load_registers_fragments_in_fragment_registry() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("frag-test"));
        plugin_reg.load(plugin).await.unwrap();

        // Fragment should be registered in the FragmentRegistry
        let fragment = frag_reg.get("frag-test-frag").await;
        assert!(fragment.is_some());
        assert_eq!(frag_reg.fragment_count().await, 1);
    }

    #[tokio::test]
    async fn load_without_registries_still_works() {
        // PluginRegistry::new() has no cross-dimension registries — should not fail.
        let plugin_reg = PluginRegistry::new();
        let plugin = Arc::new(FullPlugin::new("no-reg"));
        let result = plugin_reg.load(plugin).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn load_plugin_with_no_dimensions() {
        // TestPlugin has no tools/skills/fragments — load should still work.
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(TestPlugin::new("bare-plugin"));
        plugin_reg.load(plugin).await.unwrap();
        assert_eq!(tool_reg.tool_count(), 0);
        assert_eq!(skill_reg.skill_count().await, 0);
        assert_eq!(frag_reg.fragment_count().await, 0);
    }

    // ── Scope Drop auto-cleanup tests ──────────────────────────────────────

    #[tokio::test]
    async fn unload_auto_unregisters_tools() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("unload-tool"));
        plugin_reg.load(plugin).await.unwrap();

        // Tool is registered
        assert!(tool_reg.resolve_now("unload-tool-tool").is_some());

        // Unload the plugin — tool should be auto-unregistered
        plugin_reg.unload("unload-tool").await.unwrap();

        assert!(tool_reg.resolve_now("unload-tool-tool").is_none());
    }

    #[tokio::test]
    async fn unload_auto_unregisters_skills() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("unload-skill"));
        plugin_reg.load(plugin).await.unwrap();

        // Skill is registered
        assert!(skill_reg.get("unload-skill-skill").await.is_some());

        // Unload the plugin — skill should be unregistered
        plugin_reg.unload("unload-skill").await.unwrap();

        assert!(skill_reg.get("unload-skill-skill").await.is_none());
    }

    #[tokio::test]
    async fn unload_auto_unregisters_fragments() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let plugin = Arc::new(FullPlugin::new("unload-frag"));
        plugin_reg.load(plugin).await.unwrap();

        // Fragment is registered
        assert!(frag_reg.get("unload-frag-frag").await.is_some());

        // Unload the plugin — fragment should be unregistered
        plugin_reg.unload("unload-frag").await.unwrap();

        assert!(frag_reg.get("unload-frag-frag").await.is_none());
    }

    #[tokio::test]
    async fn shutdown_all_auto_unregisters_all_dimensions() {
        let tool_reg = Arc::new(ToolRegistry::new());
        let skill_reg = Arc::new(SkillRegistry::new());
        let frag_reg = Arc::new(FragmentRegistry::new());

        let plugin_reg = PluginRegistry::with_registries(
            tool_reg.clone(),
            skill_reg.clone(),
            frag_reg.clone(),
        );

        let p1 = Arc::new(FullPlugin::new("sa-x"));
        let p2 = Arc::new(FullPlugin::new("sa-y"));
        plugin_reg.load(p1).await.unwrap();
        plugin_reg.load(p2).await.unwrap();

        // Both registered
        assert!(tool_reg.resolve_now("sa-x-tool").is_some());
        assert!(tool_reg.resolve_now("sa-y-tool").is_some());
        assert!(skill_reg.get("sa-x-skill").await.is_some());
        assert!(skill_reg.get("sa-y-skill").await.is_some());
        assert!(frag_reg.get("sa-x-frag").await.is_some());
        assert!(frag_reg.get("sa-y-frag").await.is_some());

        let results = plugin_reg.shutdown_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));

        // All unregistered
        assert!(tool_reg.resolve_now("sa-x-tool").is_none());
        assert!(tool_reg.resolve_now("sa-y-tool").is_none());
        assert!(skill_reg.get("sa-x-skill").await.is_none());
        assert!(skill_reg.get("sa-y-skill").await.is_none());
        assert!(frag_reg.get("sa-x-frag").await.is_none());
        assert!(frag_reg.get("sa-y-frag").await.is_none());
    }

    // ── discover() tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn discover_finds_plugin_toml() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let toml_content = r#"
[id]
name = "my-plugin"
version = "0.1.0"
"#;
        std::fs::write(plugin_dir.join("plugin.toml"), toml_content).unwrap();

        let discovered = PluginRegistry::discover(dir.path()).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].descriptor.id.name, "my-plugin");
        assert_eq!(discovered[0].descriptor.id.version, "0.1.0");
        assert_eq!(discovered[0].dir, plugin_dir);
    }

    #[tokio::test]
    async fn discover_skips_dirs_without_plugin_toml() {
        let dir = tempfile::tempdir().unwrap();
        let empty_dir = dir.path().join("no-plugin");
        std::fs::create_dir_all(&empty_dir).unwrap();

        let discovered = PluginRegistry::discover(dir.path()).unwrap();
        assert!(discovered.is_empty());
    }

    #[tokio::test]
    async fn discover_skips_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("bad-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), "not valid toml {{{")
            .unwrap();

        let discovered = PluginRegistry::discover(dir.path()).unwrap();
        assert!(discovered.is_empty());
    }

    #[tokio::test]
    async fn discover_returns_error_for_nonexistent_dir() {
        let result =
            PluginRegistry::discover(Path::new("/no/such/directory/ever"));
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::DiscoveryFailed(msg) => {
                assert!(msg.contains("directory does not exist"));
            }
            other => panic!("Expected DiscoveryFailed, got: {other}"),
        }
    }

    #[tokio::test]
    async fn discover_returns_error_for_file_path() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let result = PluginRegistry::discover(file.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            PluginError::DiscoveryFailed(msg) => {
                assert!(msg.contains("not a directory"));
            }
            other => panic!("Expected DiscoveryFailed, got: {other}"),
        }
    }

    #[tokio::test]
    async fn discover_multiple_plugins() {
        let dir = tempfile::tempdir().unwrap();

        for name in &["alpha", "beta", "gamma"] {
            let plugin_dir = dir.path().join(name);
            std::fs::create_dir_all(&plugin_dir).unwrap();
            let toml_content = format!(
                r#"
[id]
name = "{name}"
version = "0.1.0"
"#
            );
            std::fs::write(plugin_dir.join("plugin.toml"), toml_content)
                .unwrap();
        }

        let mut discovered = PluginRegistry::discover(dir.path()).unwrap();
        discovered.sort_by_key(|d| d.descriptor.id.name.clone());
        assert_eq!(discovered.len(), 3);
        assert_eq!(discovered[0].descriptor.id.name, "alpha");
        assert_eq!(discovered[1].descriptor.id.name, "beta");
        assert_eq!(discovered[2].descriptor.id.name, "gamma");
    }

    #[tokio::test]
    async fn discover_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let discovered = PluginRegistry::discover(dir.path()).unwrap();
        assert!(discovered.is_empty());
    }

    #[tokio::test]
    async fn plugin_default_trait_methods() {
        /// A plugin that uses the default (empty) trait methods.
        struct MinimalPlugin {
            id: String,
        }

        #[async_trait]
        impl Plugin for MinimalPlugin {
            fn id(&self) -> &str {
                &self.id
            }

            fn version(&self) -> &str {
                "0.0.1"
            }

            fn description(&self) -> &str {
                "minimal"
            }

            fn capabilities(&self) -> PluginCapabilitySummary {
                PluginCapabilitySummary {
                    tools: vec![],
                    skills: vec![],
                    fragments: vec![],
                }
            }

            async fn initialize(&self) -> Result<(), PluginError> {
                Ok(())
            }

            async fn shutdown(&self) -> Result<(), PluginError> {
                Ok(())
            }
        }

        let p = MinimalPlugin {
            id: "min".to_string(),
        };
        // Default trait methods should return empty vecs.
        assert!(p.tools().await.is_empty());
        assert!(p.skills().await.is_empty());
        assert!(p.fragments().await.is_empty());
    }
}
