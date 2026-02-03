use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::Value;

use super::{
    data::MessageType,
    file_store::TeamStorage,
    shared::err_result,
    tool_base::{json_result, text_result},
    types::{PlanApprovalRequest, ShutdownRequest, ShutdownResponseRequest},
};
use crate::tools::Tool;

#[derive(Clone)]
pub(crate) struct ShutdownRequestTool {
    storage: TeamStorage,
}

impl ShutdownRequestTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for ShutdownRequestTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShutdownRequestTool {
    fn name(&self) -> &str {
        "shutdown_request"
    }

    fn description(&self) -> &str {
        "Request teammate graceful shutdown."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(ShutdownRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ShutdownRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => return err_result(format!("Invalid request: {e}")),
        };

        let request_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        if let Err(e) = self
            .storage
            .protocol_store
            .create_shutdown_request(&request_id, &request.teammate)
            .await
        {
            return err_result(format!(
                "Failed to create shutdown request: {e}"
            ));
        }

        if let Err(e) = self
            .storage
            .message_store
            .send_message(
                &request.teammate,
                MessageType::ShutdownRequest,
                "lead",
                "Please shut down gracefully.",
                Some(&request_id),
            )
            .await
        {
            return err_result(format!("Failed to send message: {e}"));
        }

        text_result(format!(
            "Shutdown request {request_id} sent to '{}' (status: pending)",
            request.teammate
        ))
    }
}

#[derive(Clone)]
pub(crate) struct ShutdownResponseTool {
    storage: TeamStorage,
}

impl ShutdownResponseTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for ShutdownResponseTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShutdownResponseTool {
    fn name(&self) -> &str {
        "shutdown_response"
    }

    fn description(&self) -> &str {
        "Check shutdown request status."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(ShutdownResponseRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ShutdownResponseRequest =
            match serde_json::from_value(args) {
                Ok(r) => r,
                Err(e) => return err_result(format!("Invalid request: {e}")),
            };

        let result = match self
            .storage
            .protocol_store
            .get_shutdown_request(&request.request_id)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return err_result(format!(
                    "Failed to get shutdown request: {e}"
                ));
            }
        };

        match result {
            Some(shutdown) => {
                let output = serde_json::json!({
                    "request_id": shutdown.request_id,
                    "target": shutdown.target,
                    "status": shutdown.status
                });
                json_result(&output)
            }
            None => err_result(format!(
                "Shutdown request not found: {}",
                request.request_id
            )),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PlanApprovalTool {
    storage: TeamStorage,
}

impl PlanApprovalTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for PlanApprovalTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PlanApprovalTool {
    fn name(&self) -> &str {
        "plan_approval"
    }

    fn description(&self) -> &str {
        "Approve or reject teammate plan."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(PlanApprovalRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: PlanApprovalRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => return err_result(format!("Invalid request: {e}")),
        };

        let plan = match self
            .storage
            .protocol_store
            .get_plan_request(&request.request_id)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return err_result(format!("Failed to get plan request: {e}"));
            }
        };

        match plan {
            Some(plan) => {
                let new_status = if request.approve {
                    "approved"
                } else {
                    "rejected"
                };

                if let Err(e) = self
                    .storage
                    .protocol_store
                    .update_plan_status(&request.request_id, new_status)
                    .await
                {
                    return err_result(format!(
                        "Failed to update plan status: {e}"
                    ));
                }

                let feedback = request.feedback.unwrap_or_default();

                if let Err(e) = self
                    .storage
                    .message_store
                    .send_message(
                        &plan.sender,
                        MessageType::PlanApprovalResponse,
                        "lead",
                        &feedback,
                        Some(&request.request_id),
                    )
                    .await
                {
                    return err_result(format!("Failed to send message: {e}"));
                }

                text_result(format!("Plan {new_status} for '{}'", plan.sender))
            }
            None => err_result(format!(
                "Plan request not found: {}",
                request.request_id
            )),
        }
    }
}
