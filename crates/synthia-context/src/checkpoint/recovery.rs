use std::path::PathBuf;

use super::types::{Checkpoint, CheckpointData, PendingToolCall};
use crate::types::ContextError;

pub fn recover_from_checkpoint(
    checkpoint_dir: PathBuf,
) -> Result<Option<CheckpointData>, ContextError> {
    match Checkpoint::load_latest(checkpoint_dir)? {
        Some(cp) => {
            let mut data = CheckpointData {
                session_id: cp.session_id,
                step: cp.step,
                timestamp: chrono::Utc::now().to_rfc3339(),
                messages: cp.messages,
                pending_tool_calls: cp.pending_tool_calls,
                config_snapshot: cp.config_snapshot,
                guardian_state: cp.guardian_state,
                token_usage: cp.token_usage,
                iteration_count: cp.iteration_count,
            };
            patch_tool_calls(&mut data.pending_tool_calls);
            Ok(Some(data))
        }
        None => Ok(None),
    }
}

pub fn patch_tool_calls(pending_calls: &mut [PendingToolCall]) {
    for call in pending_calls.iter_mut() {
        if call.input.is_null() {
            call.input = serde_json::json!({
                "_error": "incomplete tool call from checkpoint recovery",
                "_id": call.id,
            });
        }
    }
}
