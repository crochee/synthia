use std::sync::Arc;

use synthia_permission::ApprovalService;
use synthia_sandbox::SandboxManager;

use crate::{
    agent::Agent,
    config_watcher::MultiConfigWatcher,
    steering::SteeringChannel,
};

impl Agent {
    pub fn with_mcp_manager(
        mut self,
        mcp_manager: Arc<synthia_mcp::McpManager>,
    ) -> Self {
        self.mcp_manager = Some(mcp_manager);
        self
    }

    pub fn with_steering_channel(
        mut self,
        channel: Arc<dyn SteeringChannel>,
    ) -> Self {
        self.steering_channel = Some(channel);
        self
    }

    pub fn with_config_watcher(mut self, watcher: MultiConfigWatcher) -> Self {
        self.config_watcher = Some(watcher);
        self
    }

    pub fn with_approval_service(
        mut self,
        service: Arc<dyn ApprovalService>,
    ) -> Self {
        self.approval_service = Some(service);
        self
    }

    pub fn with_sandbox_manager(
        mut self,
        manager: Arc<dyn SandboxManager>,
    ) -> Self {
        self.sandbox_manager = Some(manager);
        self
    }
}
