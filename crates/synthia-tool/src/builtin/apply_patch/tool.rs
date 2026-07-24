//! The [`ApplyPatchTool`] struct itself and its
//! [`crate::traits::Tool`] impl.
//!
//! The `call` method is organized in 5 stages:
//!
//! 1. **Parse** — `v4a::parse_v4a` converts the patch text into
//!    `Vec<PatchOp>`. Pure, no filesystem mutation possible.
//! 2. **Reject Move** — if `enable_move` is `false` (the default),
//!    reject any `Update` op that has a `*** Move to:` line
//!    up-front to prevent partial state.
//! 3. **Resolve** — `check_path_safety` + `resolve_path` for every
//!    op's path and `*** Move to:` destination, with early return
//!    on any failure.
//! 4. **Sequential apply** — call [`super::apply::apply_one`] for
//!    each op in source order. On the first failure, stop and
//!    return the applied + failed summary so the LLM can re-plan
//!    (mirrors codex scenario 015 + opencode's "atomic rollback
//!    not supported yet" stance).
//! 5. **Summarize** — format the final tool output as
//!    `"Applied N operations: A x, M y, D z"`.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{
    apply::{apply_one, apply_one_with_events},
    input::ApplyPatchInput,
    op_summary::op_summary,
};
use crate::{
    builtin::{
        path::{check_path_safety, resolve_path},
        v4a::{self, PatchOp},
    },
    traits::{FileChangeCallback, Tool},
    types::*,
};

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ApplyPatchTool {
    /// Whether to honor `*** Move to:` hunk lines. Defaults to `false` (matches
    /// opencode's behavior). Set to `true` only after atomic-rollback support
    /// is designed.
    pub enable_move: bool,
}

impl ApplyPatchTool {
    /// Shared implementation for `call` and `call_with_progress`.
    ///
    /// When `on_event` is `Some`, each successfully applied hunk emits a
    /// [`FileChangeEvent::HunkApplied`] and each completed op emits the
    /// appropriate `FileAdded` / `FileUpdated` / `FileDeleted` event.
    fn execute_ops(
        &self,
        input: ToolInput,
        on_event: Option<FileChangeCallback>,
    ) -> ToolOutput {
        let workspace_root = &input.context.workspace_root;
        let parsed: ApplyPatchInput = match serde_json::from_value(input.input)
        {
            Ok(v) => v,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {}", e));
            }
        };

        // Stage 1: parse (no filesystem mutation possible)
        let ops = match v4a::parse_v4a(&parsed.patch) {
            Ok(ops) => ops,
            Err(e) => {
                return ToolOutput::error(format!("Parse error: {}", e));
            }
        };

        // Stage 1.5: reject Move ops up-front if disabled (prevents partial state)
        if !self.enable_move {
            for op in &ops {
                if let PatchOp::Update {
                    move_to: Some(m), ..
                } = op
                {
                    return ToolOutput::error(format!(
                        "apply_patch moves are not supported yet (Move to: {})",
                        m.display()
                    ));
                }
            }
        }

        // Stage 2: resolve all paths and check safety
        let mut resolved: Vec<(PatchOp, PathBuf, Option<PathBuf>)> =
            Vec::with_capacity(ops.len());
        for op in ops {
            let (raw_path, op_after_extract) = match &op {
                PatchOp::Add { path, .. } => (path.clone(), op.clone()),
                PatchOp::Update { path, .. } => (path.clone(), op.clone()),
                PatchOp::Delete { path } => (path.clone(), op.clone()),
            };
            let path_str = match raw_path.to_str() {
                Some(s) => s,
                None => {
                    return ToolOutput::error(format!(
                        "Invalid path encoding: {}",
                        raw_path.display()
                    ));
                }
            };
            if let Some(err) = check_path_safety(workspace_root, path_str) {
                return ToolOutput::error(err);
            }
            // For Update+Move, also check the destination path
            let move_to_abs = if let PatchOp::Update {
                move_to: Some(m), ..
            } = &op_after_extract
            {
                let m_str = match m.to_str() {
                    Some(s) => s,
                    None => {
                        return ToolOutput::error(format!(
                            "Invalid Move path encoding: {}",
                            m.display()
                        ));
                    }
                };
                if let Some(err) = check_path_safety(workspace_root, m_str) {
                    return ToolOutput::error(format!(
                        "Move destination unsafe: {}",
                        err
                    ));
                }
                Some(resolve_path(workspace_root, m_str))
            } else {
                None
            };
            let abs = resolve_path(workspace_root, path_str);
            resolved.push((op_after_extract, abs, move_to_abs));
        }

        // Stage 3: sequential apply
        let mut applied: Vec<PatchOp> = Vec::new();
        for (op, abs_path, move_to_abs) in resolved {
            let result = if let Some(ref callback) = on_event {
                apply_one_with_events(
                    &op,
                    &abs_path,
                    move_to_abs.as_ref(),
                    callback.as_ref(),
                )
            } else {
                apply_one(&op, &abs_path, move_to_abs.as_ref())
            };
            match result {
                Ok(()) => applied.push(op),
                Err(reason) => {
                    // Stop at first failure; return applied + failed
                    let applied_summary: Vec<String> =
                        applied.iter().map(op_summary).collect();
                    return ToolOutput::error(format!(
                        "Applied {} of {} operations. Succeeded: [{}]. Failed: {} — {}",
                        applied.len(),
                        applied.len() + 1,
                        applied_summary.join(", "),
                        op_summary(&op),
                        reason
                    ));
                }
            }
        }

        // All succeeded
        let summary: Vec<String> = applied.iter().map(op_summary).collect();
        ToolOutput::text(format!(
            "Applied {} operations: {}",
            applied.len(),
            summary.join(", ")
        ))
    }
}

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply an Anthropic V4A multi-file patch. Operations apply sequentially; if a later \
         operation fails, earlier operations remain applied and the failure reports them \
         explicitly. Moves are not supported yet."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["patch"],
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "V4A patch text starting with '*** Begin Patch'"
                }
            }
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        false
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        self.execute_ops(input, None)
    }

    async fn call_with_progress(
        &self,
        input: ToolInput,
        on_event: FileChangeCallback,
        _token: &CancellationToken,
    ) -> ToolOutput {
        self.execute_ops(input, Some(on_event))
    }
}
