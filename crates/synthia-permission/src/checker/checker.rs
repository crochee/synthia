use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use dashmap::DashSet;
use synthia_core::Error;

use crate::{
    level::Permission,
    merged_policy::MergedPolicy,
    rule::{PermissionAction, PermissionRule},
    types::PermissionRequest,
};

pub type Result<T> = std::result::Result<T, Error>;

/// Security checker that resolves `PermissionRequest`s against a
/// [`MergedPolicy`] and runs a built-in security pre-screen
/// (path-traversal / dangerous command).
///
/// # ADR-2026-06-10
///
/// `PermissionChecker` was previously built around a separate
/// `PermissionPolicy` type with its own `Permission` enum. After the
/// 6-expert adversarial review, the codebase unified on `MergedPolicy` as
/// the only permission policy. `PermissionChecker` now wraps a
/// `MergedPolicy` directly; the public `check()` API returns the rich
/// `Permission` enum so downstream callers (e.g. `ToolRegistry`) keep
/// the same `Block` / `RequireConfirm` / `RequireExplicit` / `AutoApprove`
/// distinctions for UX.
pub struct PermissionChecker {
    policy: MergedPolicy,
    /// When `Some(_)`, [`PermissionChecker::check`] short-circuits to
    /// `Err`. This is a per-instance flag (not a global) so multiple
    /// tests in the same binary do not interfere with each other.
    /// The flag is private to test code; production never sets it.
    poison: Option<Arc<()>>,
    /// Session-scoped "always" approval rules keyed by
    /// `(tool_name, normalized_input_json)`. When a request matches an
    /// entry here, [`PermissionChecker::check`] returns
    /// [`Permission::AutoApprove`] without consulting `policy`.
    ///
    /// The key is `(tool_name, normalized_input_json)` where
    /// `normalized_input_json` is produced by
    /// [`crate::approval::normalize`] (recursive key sorting) followed
    /// by `serde_json::to_string`. This ensures key order does not
    /// affect matching, matching `ApprovalScope`'s normalization
    /// strategy.
    ///
    /// Wrapped in `Arc` so that `PermissionChecker::clone()` shares
    /// the same set with the original — clones represent the same
    /// session and must observe each other's `remember_always` /
    /// `forget_always` mutations. The set is never persisted to disk;
    /// dropping the last `Arc` (i.e. ending the session) clears it.
    saved_rules: Arc<DashSet<(String, String)>>,
    /// Workspace root used by [`check_path_traversal`](Self::check_path_traversal)
    /// for the workspace-containment pre-screen. Every file-tool path
    /// MUST normalize to a path inside this root; absolute paths
    /// outside the workspace (e.g. `/etc/passwd`,
    /// `/home/victim/.ssh/id_rsa`) and relative traversals (e.g.
    /// `../../../etc/passwd`) are denied before the policy is consulted.
    ///
    /// Defaults to the process current directory (matching the codex
    /// `cwd`-based containment model in
    /// `codex-rs/core/src/safety.rs`). Callers that know the logical
    /// workspace should override it via
    /// [`with_workspace_root`](Self::with_workspace_root).
    ///
    /// Path normalization is pure arithmetic — it NEVER calls
    /// [`std::fs::canonicalize`] or any other filesystem-I/O method,
    /// so a swapped symlink cannot defeat the check via a TOCTOU race
    /// between check and use.
    workspace_root: PathBuf,
}

impl Clone for PermissionChecker {
    fn clone(&self) -> Self {
        Self {
            policy: self.policy.clone(),
            poison: self.poison.clone(),
            saved_rules: Arc::clone(&self.saved_rules),
            workspace_root: self.workspace_root.clone(),
        }
    }
}

impl PermissionChecker {
    /// JSON keys that may contain file-system paths for file tools.
    /// Strings under any other key (e.g. `write_file.content`) are
    /// intentionally NOT treated as paths, so writing documentation that
    /// mentions `/etc/passwd` does not trigger a false traversal denial.
    const PATH_FIELDS: &[&str] = &[
        "path",
        "file_path",
        "old_path",
        "new_path",
        "from",
        "to",
        "source",
        "destination",
    ];

    /// Construct a checker from a fully resolved [`MergedPolicy`].
    ///
    /// The workspace root defaults to the process current directory
    /// (see [`Self::workspace_root`]); override it with
    /// [`Self::with_workspace_root`] when the logical workspace is
    /// known.
    pub fn new(policy: MergedPolicy) -> Self {
        Self {
            policy,
            poison: None,
            saved_rules: Arc::new(DashSet::new()),
            workspace_root: default_workspace_root(),
        }
    }

    /// Construct a checker from a default-allow policy (every tool
    /// is `AutoApprove`). This is the lenient baseline used by callers
    /// that don't need a custom rule set.
    pub fn allow_all() -> Self {
        let defaults: Vec<PermissionRule> = Vec::new();
        Self::new(MergedPolicy::new(&defaults, &[], &[]))
    }

    /// Override the workspace root used by the path-containment
    /// pre-screen. File-tool paths must normalize to a path inside
    /// `workspace_root`; anything else (absolute paths outside the
    /// workspace, or relative traversals that escape it) is denied.
    ///
    /// Callers that know the agent's logical workspace should set this
    /// explicitly rather than relying on the process-current-directory
    /// default.
    pub fn with_workspace_root(mut self, workspace_root: &Path) -> Self {
        self.workspace_root = workspace_root.to_path_buf();
        self
    }

    /// Test-only constructor: returns a checker that, on every
    /// `check()` call, returns `Err(Error::Internal(...))`. This is
    /// used by integration tests in `synthia-agent` to force the
    /// registry `run_with_context` path to propagate an `Err` so the
    /// L3-L5 recovery cascade wiring in `stream_builder/builder.rs`
    /// can be exercised end-to-end. NOT for production use — a real
    /// policy with `Block` / `Deny` rules is the proper way to refuse
    /// tool execution in production.
    pub fn always_fail_for_test() -> Self {
        Self {
            policy: MergedPolicy::new(&[], &[], &[]),
            poison: Some(Arc::new(())),
            saved_rules: Arc::new(DashSet::new()),
            workspace_root: default_workspace_root(),
        }
    }

    /// Returns `true` if this checker is in "always fail" mode (set
    /// via [`Self::always_fail_for_test`]). Useful for tests that want
    /// to assert the mode without inspecting the poison field.
    pub fn is_always_fail_for_test(&self) -> bool {
        self.poison.is_some()
    }

    /// Access the underlying [`MergedPolicy`].
    pub fn policy(&self) -> &MergedPolicy {
        &self.policy
    }

    /// Mutably access the underlying [`MergedPolicy`].
    pub fn policy_mut(&mut self) -> &mut MergedPolicy {
        &mut self.policy
    }

    /// Resolve each [`PermissionRequest`] to a `Permission` decision.
    pub async fn check(
        &self,
        requests: &[PermissionRequest],
    ) -> Result<HashMap<String, Permission>> {
        // Test-only short-circuit: if this instance was constructed
        // via `always_fail_for_test`, return Err so the registry's
        // `run_with_context` propagates an error to the caller. This
        // is used by the recovery-cascade integration tests to
        // exercise the cascade's `Err` arm.
        if self.poison.is_some() {
            return Err(Error::Internal(
                "permission checker poisoned for test".to_string(),
            ));
        }

        let mut decisions = HashMap::with_capacity(requests.len());

        for req in requests {
            if !req.requires_permission {
                decisions
                    .insert(req.tool_name.clone(), Permission::AutoApprove);
                continue;
            }

            if let Some(deny_reason) =
                self.security_check(&req.tool_name, &req.input)
            {
                decisions.insert(
                    req.tool_name.clone(),
                    Permission::Deny {
                        reason: deny_reason,
                    },
                );
                continue;
            }

            // Check session-scoped "always" rules before evaluating
            // the policy. The resource key is the canonical JSON
            // serialization of `crate::approval::normalize(&req.input)`
            // (recursive key sorting) so callers (e.g.
            // `TerminalApprovalService::record_always_decision`) can
            // reproduce the same key by normalizing `args` before
            // serializing. Matching is exact — no wildcards. See
            // `remember_always` / `forget_always`.
            let resource =
                serde_json::to_string(&crate::approval::normalize(&req.input))
                    .expect("serde_json::Value serialization is infallible");
            if self
                .saved_rules
                .contains(&(req.tool_name.clone(), resource))
            {
                decisions
                    .insert(req.tool_name.clone(), Permission::AutoApprove);
                continue;
            }

            let action = self.policy.evaluate(&req.tool_name);
            decisions
                .insert(req.tool_name.clone(), action_to_permission(action));
        }

        Ok(decisions)
    }

    /// Remember an "always" approval for `(action, resource)` for the
    /// remainder of the session. Subsequent `check()` calls for a
    /// request whose `tool_name == action` and whose serialized
    /// `input == resource` will return `Permission::AutoApprove`
    /// without consulting the policy.
    ///
    /// The rule is session-scoped: it lives in `saved_rules` (an
    /// `Arc<DashSet>`) and is never persisted to disk. When the last
    /// `Arc` reference is dropped (i.e. the session ends) the set is
    /// deallocated and the rule disappears.
    ///
    /// `action` corresponds to `PermissionRequest::tool_name` and
    /// `resource` corresponds to the canonical JSON serialization of
    /// `crate::approval::normalize(&req.input)`. Callers passing a
    /// raw `serde_json::Value` should normalize it first (see
    /// `TerminalApprovalService::record_always_decision`, which
    /// handles normalization internally); callers passing an already
    /// canonical string (e.g. round-tripped from `check()`) may pass
    /// it directly.
    pub fn remember_always(&self, action: String, resource: String) {
        self.saved_rules.insert((action, resource));
    }

    /// Remove a previously-saved "always" rule. After this call,
    /// matching requests will once again be evaluated by the policy.
    /// Forgetting a rule that was never saved is a no-op (returns
    /// without error).
    pub fn forget_always(&self, action: &str, resource: &str) {
        self.saved_rules
            .remove(&(action.to_string(), resource.to_string()));
    }

    fn security_check(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<String> {
        match tool_name {
            "read_file" | "write_file" | "edit_file" | "create_file"
            | "delete_file" | "move_file" | "copy_file" => {
                self.check_path_traversal(input)
            }
            "bash" | "shell" => self.check_dangerous_command(input),
            _ => None,
        }
    }

    fn check_path_traversal(
        &self,
        input: &serde_json::Value,
    ) -> Option<String> {
        let paths = self.extract_path_strings(input);
        for path in &paths {
            let p = Path::new(path);
            // Resolve relative paths against the workspace root BEFORE
            // normalization so that `..` segments can fold against the
            // workspace segments (matching codex's `resolve_path(cwd, p)`
            // + `normalize()` pipeline in `core/src/safety.rs`). This
            // is pure arithmetic — no filesystem I/O.
            let resolved = if p.is_relative() {
                self.workspace_root.join(p)
            } else {
                p.to_path_buf()
            };
            if !is_path_in_workspace(&resolved, &self.workspace_root) {
                return Some(format!("path outside workspace: {}", path));
            }
        }
        None
    }

    fn check_dangerous_command(
        &self,
        input: &serde_json::Value,
    ) -> Option<String> {
        let commands = self.extract_string_values(input);
        for cmd in &commands {
            if self.is_dangerous_command(cmd) {
                return Some(format!("dangerous command detected: {}", cmd));
            }
        }
        None
    }

    fn is_dangerous_command(&self, cmd: &str) -> bool {
        let patterns = [
            "rm -rf /",
            "rm -rf /*",
            "chmod 777 /",
            "chmod 777 /*",
            "> /dev/sd",
            "dd if=/dev/zero",
            ":(){ :|:& };:",
            "mkfs",
            "fdisk",
            "parted",
            ">/dev/sd",
            "sudo rm -rf",
            "sudo chmod 777",
            "sudo dd",
            "curl *|bash",
            "curl*|sh",
            "wget*|bash",
            "wget*|sh",
        ];
        let cmd_lower = cmd.to_lowercase();
        patterns.iter().any(|p| cmd_lower.contains(p))
    }

    fn extract_string_values(&self, value: &serde_json::Value) -> Vec<String> {
        let mut results = Vec::new();
        self.extract_strings_recursive(value, &mut results);
        results
    }

    fn extract_strings_recursive(
        &self,
        value: &serde_json::Value,
        results: &mut Vec<String>,
    ) {
        match value {
            serde_json::Value::String(s) => results.push(s.clone()),
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    self.extract_strings_recursive(v, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    self.extract_strings_recursive(v, results);
                }
            }
            _ => {}
        }
    }

    /// Extract string values that live under keys known to carry file
    /// paths (`path`, `file_path`, `old_path`, `new_path`, `from`, `to`,
    /// `source`, `destination`). Recurses into nested objects/arrays so
    /// multi-step patch inputs are still covered, but ignores strings
    /// under unrelated keys such as `content` or `description`.
    fn extract_path_strings(&self, value: &serde_json::Value) -> Vec<String> {
        let mut results = Vec::new();
        self.extract_path_strings_recursive(value, &mut results);
        results
    }

    fn extract_path_strings_recursive(
        &self,
        value: &serde_json::Value,
        results: &mut Vec<String>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map.iter() {
                    if Self::PATH_FIELDS.contains(&k.as_str()) {
                        self.extract_strings_recursive(v, results);
                    } else {
                        self.extract_path_strings_recursive(v, results);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    self.extract_path_strings_recursive(v, results);
                }
            }
            _ => {}
        }
    }
}

/// Translate a `PermissionAction` (the merged-policy output) into the
/// richer `Permission` enum (the public `check()` output).
///
/// `PermissionAction::Allow` becomes `Permission::AutoApprove` so the
/// tool can run silently. `Ask` becomes `RequireConfirm` — the user is
/// asked once and the decision is remembered. `Deny` becomes `Block`.
fn action_to_permission(action: PermissionAction) -> Permission {
    match action {
        PermissionAction::Allow => Permission::AutoApprove,
        PermissionAction::Ask => Permission::RequireConfirm,
        PermissionAction::Deny => Permission::Block,
    }
}

/// Default workspace root: the process current directory, mirroring
/// the codex `cwd`-based containment model in
/// `codex-rs/core/src/safety.rs`.
///
/// If the current directory cannot be read (e.g. cwd deleted, container
/// environment), this returns `.` as a sentinel. `normalize_path(".")`
/// produces an empty `PathBuf`, and [`is_path_in_workspace`] treats an
/// empty normalized workspace as **deny-all** (fail-closed). Without
/// that guard, `Path::starts_with("")` returns `true` for any path,
/// which would be a fail-OPEN security bypass.
fn default_workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Normalize a path by folding `.` and `..` segments using pure path
/// arithmetic — **no filesystem I/O**.
///
/// This is the synthia port of codex's `normalize()` in
/// `codex-rs/core/src/safety.rs` (lines 145-157). It iterates
/// [`Path::components`] and maintains a [`PathBuf`] accumulator:
/// - [`Component::CurDir`] (`.`) is skipped.
/// - [`Component::ParentDir`] (`..`) pops the last segment if there is
///   one to pop ([`PathBuf::pop`] is a no-op at the root, so a `..`
///   above the root is effectively dropped rather than escaping).
/// - All other components (root, prefix, normal segments) are pushed.
///
/// # Why not `canonicalize`
///
/// [`std::fs::canonicalize`] resolves symlinks and requires the path to
/// exist on disk, which would (a) introduce a TOCTOU race — a symlink
/// can be swapped between this check and the subsequent file operation —
/// and (b) fail on non-existent paths. This function performs no I/O
/// and works on any path, existent or not.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => { /* skip */ }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Decide whether `path` is inside `workspace_root` using pure path
/// arithmetic (no filesystem I/O).
///
/// Both `path` and `workspace_root` are normalized via
/// [`normalize_path`] first, then a prefix-containment check
/// ([`Path::starts_with`]) is applied. Relative `path` callers should
/// resolve against the workspace root (e.g.
/// `workspace_root.join(path)`) BEFORE calling this function so that
/// `..` segments fold against the workspace segments — see
/// [`PermissionChecker::check_path_traversal`].
pub(crate) fn is_path_in_workspace(path: &Path, workspace_root: &Path) -> bool {
    let normalized = normalize_path(path);
    let workspace_normalized = normalize_path(workspace_root);
    if workspace_normalized.as_os_str().is_empty() {
        // fail-closed: empty workspace (e.g. `default_workspace_root()`
        // falling back to `.` when cwd is unavailable) means deny all.
        // Without this, `Path::starts_with("")` returns `true` for any
        // path, which would be a fail-OPEN security bypass.
        return false;
    }
    normalized.starts_with(workspace_normalized)
}
