use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::{
    approval::{
        ApprovalError,
        ApprovalOutcome,
        ApprovalPolicy,
        ApprovalScope,
        normalize,
    },
    checker::PermissionChecker,
    permission_future::PermissionFuture,
    types::PermissionRequest,
};

/// An approval service that prompts the user on the terminal and records the
/// decision in an [`ApprovalStore`](crate::ApprovalStore).
///
/// The prompt accepts the following responses:
/// - `y`, `yes` -> approve once
/// - `n`, `no`, `reject` -> deny and cache the rejection
/// - `a`, `always` -> approve for the remainder of the session
///
/// When constructed via [`TerminalApprovalService::new_with_checker`], an
/// "always" reply is also forwarded to the bundled [`PermissionChecker`] via
/// `remember_always`, so subsequent `PermissionChecker::check` calls for the
/// same `(tool, args)` pair return `Permission::AutoApprove` without
/// prompting again. The legacy [`TerminalApprovalService::new`] constructor
/// keeps `checker = None` for backward compatibility (the approval store's
/// own `AlwaysForSession` cache still works for repeat lookups within the
/// same process).
#[derive(Clone)]
pub struct TerminalApprovalService {
    pub(crate) store: crate::ApprovalStore,
    pub(crate) checker: Option<Arc<PermissionChecker>>,
}

impl TerminalApprovalService {
    /// Create a new terminal approval service backed by `store` and no
    /// `PermissionChecker`. This preserves the pre-F19 behavior: "always"
    /// replies are cached in the [`ApprovalStore`](crate::ApprovalStore) but not propagated to
    /// any `PermissionChecker`.
    pub fn new(store: crate::ApprovalStore) -> Self {
        Self {
            store,
            checker: None,
        }
    }

    /// Create a new terminal approval service backed by `store` and
    /// `checker`. When the user replies "always" / "a" /
    /// "always-for-session", `checker.remember_always(tool, args)` is
    /// invoked in addition to caching the decision in `store`. The
    /// `PermissionChecker` (and any clones sharing its `saved_rules`)
    /// will then auto-approve matching requests for the remainder of
    /// the session.
    ///
    /// **Production wiring note**: The CLI
    /// (`synthia-cli/src/repl_core/repl/agent_message.rs`) currently
    /// uses [`TerminalApprovalService::new`] (without checker) because
    /// the CLI context does not yet expose a `PermissionChecker`
    /// handle. Once the CLI wiring exposes the checker, switch to
    /// `new_with_checker` to enable F19 end-to-end.
    pub fn new_with_checker(
        store: crate::ApprovalStore,
        checker: Arc<PermissionChecker>,
    ) -> Self {
        Self {
            store,
            checker: Some(checker),
        }
    }

    /// Record an "always" approval decision to both the [`ApprovalStore`](crate::ApprovalStore)
    /// (for prompt suppression) and the bundled [`PermissionChecker`] (for
    /// `check()` short-circuit). Extracted from `request_approval` so
    /// the F19 wiring can be unit-tested without mocking stdin.
    ///
    /// `tool` and `args` mirror the parameters passed to
    /// [`Self::request_approval`]. The `args` value is normalized via
    /// [`normalize`] before being forwarded to
    /// [`PermissionChecker::remember_always`], so the derived resource
    /// key matches the one computed by
    /// [`PermissionChecker::check`](crate::checker::PermissionChecker::check).
    pub(super) fn record_always_decision(
        &self,
        tool: &str,
        args: &serde_json::Value,
    ) {
        let scope = ApprovalScope::new(tool, args);
        self.store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );
        if let Some(checker) = &self.checker {
            checker
                .remember_always(tool.to_string(), normalize(args).to_string());
        }
    }

    /// Internal helper that accepts arbitrary async reader/writer pairs so
    /// the interactive prompt can be unit-tested without touching real
    /// stdin/stdout.
    ///
    /// The parameter count is intentionally high: this is the single internal
    /// entry point shared by the real `ApprovalService` implementation and the
    /// test harness. Splitting it would not improve readability.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn request_approval_with_io<R, W>(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
        reader: R,
        mut writer: W,
    ) -> Result<ApprovalOutcome, ApprovalError>
    where
        R: AsyncBufReadExt + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        if cancel.is_cancelled() {
            return Err(ApprovalError::Cancelled);
        }

        let scope = ApprovalScope::new(tool, args);
        if let Some(outcome) = self.store.get(&scope, policy) {
            return Ok(outcome);
        }

        let prompt = format!(
            "Approve tool call?\n  tool: {tool}\n  args: {args}\n[y/n/a/always/reject]: ",
        );
        if writer.write_all(prompt.as_bytes()).await.is_err() {
            return Err(ApprovalError::Unavailable);
        }
        if writer.flush().await.is_err() {
            return Err(ApprovalError::Unavailable);
        }

        let mut lines = reader.lines();
        let line = tokio::select! {
            _ = cancel.cancelled() => return Err(ApprovalError::Cancelled),
            _ = tokio::time::sleep(timeout) => return Err(ApprovalError::Timeout),
            result = lines.next_line() => result.map_err(|_| ApprovalError::Unavailable)?,
        };

        let line = line.ok_or(ApprovalError::Timeout)?;
        let outcome = match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => {
                self.store.set(
                    &scope,
                    ApprovalOutcome::Approve,
                    ApprovalPolicy::Once,
                );
                ApprovalOutcome::Approve
            }
            "a" | "always" | "always-for-session" => {
                self.record_always_decision(tool, args);
                ApprovalOutcome::Approve
            }
            "n" | "no" | "reject" => {
                self.store.set(
                    &scope,
                    ApprovalOutcome::Deny,
                    ApprovalPolicy::Reject,
                );
                ApprovalOutcome::Deny
            }
            _ => ApprovalOutcome::Deny,
        };

        Ok(outcome)
    }
}

#[async_trait]
impl crate::ApprovalService for TerminalApprovalService {
    async fn request_approval(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        let stdin = tokio::io::BufReader::new(tokio::io::stdin());
        let stdout = tokio::io::stdout();
        self.request_approval_with_io(
            tool, args, policy, timeout, cancel, stdin, stdout,
        )
        .await
    }

    fn ask(&self, _request: PermissionRequest) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use crate::{
        approval::{
            ApprovalError,
            ApprovalOutcome,
            ApprovalPolicy,
            ApprovalScope,
            ApprovalService,
            ApprovalStore,
        },
        approval_ui::TerminalApprovalService,
    };

    #[tokio::test]
    async fn terminal_approval_service_uses_cached_decision_without_prompt() {
        let store = ApprovalStore::new();
        let scope = ApprovalScope::new(
            "write",
            &serde_json::json!({ "path": "cached.txt" }),
        );
        store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        let service = TerminalApprovalService::new(store);
        let outcome = service
            .request_approval(
                "write",
                &serde_json::json!({ "path": "cached.txt" }),
                ApprovalPolicy::AlwaysForSession,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Ok(ApprovalOutcome::Approve));
    }

    // ---- F19: TerminalApprovalService <-> PermissionChecker wiring ----

    #[tokio::test]
    async fn test_terminal_approval_records_always_to_checker() {
        use std::sync::Arc;

        use crate::{
            checker::PermissionChecker,
            level::Permission,
            merged_policy::MergedPolicy,
            rule::{PermissionAction, PermissionRule},
            types::PermissionRequest,
        };

        let rule = PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Ask,
            forced: false,
        };
        let checker = Arc::new(PermissionChecker::new(MergedPolicy::new(
            &[rule],
            &[],
            &[],
        )));
        let store = ApprovalStore::new();
        let service =
            TerminalApprovalService::new_with_checker(store, checker.clone());

        let req = PermissionRequest::new(
            "bash".to_string(),
            serde_json::json!({"command": "cargo build"}),
            true,
        );
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::RequireConfirm));

        service.record_always_decision(
            "bash",
            &serde_json::json!({"command": "cargo build"}),
        );

        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
    }

    #[tokio::test]
    async fn test_terminal_approval_always_wiring_is_key_order_independent() {
        use std::sync::Arc;

        use crate::{
            checker::PermissionChecker,
            level::Permission,
            merged_policy::MergedPolicy,
            rule::{PermissionAction, PermissionRule},
            types::PermissionRequest,
        };

        let rule = PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Ask,
            forced: false,
        };
        let checker = Arc::new(PermissionChecker::new(MergedPolicy::new(
            &[rule],
            &[],
            &[],
        )));
        let store = ApprovalStore::new();
        let service =
            TerminalApprovalService::new_with_checker(store, checker.clone());

        service.record_always_decision(
            "bash",
            &serde_json::json!({"command": "ls", "cwd": "/tmp"}),
        );

        let req = PermissionRequest::new(
            "bash".to_string(),
            serde_json::json!({"cwd": "/tmp", "command": "ls"}),
            true,
        );
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
    }

    // ---- TerminalApprovalService interactive input handling ----

    fn terminal_service() -> (TerminalApprovalService, serde_json::Value) {
        (
            TerminalApprovalService::new(ApprovalStore::new()),
            serde_json::json!({"command": "ls"}),
        )
    }

    async fn terminal_with_input(
        service: &TerminalApprovalService,
        args: &serde_json::Value,
        input: &str,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        terminal_with_input_and_policy(
            service,
            args,
            input,
            ApprovalPolicy::Once,
        )
        .await
    }

    async fn terminal_with_input_and_policy(
        service: &TerminalApprovalService,
        args: &serde_json::Value,
        input: &str,
        policy: ApprovalPolicy,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        let reader = tokio::io::BufReader::new(input.as_bytes());
        let writer: Vec<u8> = Vec::new();
        service
            .request_approval_with_io(
                "bash",
                args,
                policy,
                Duration::from_secs(30),
                CancellationToken::new(),
                reader,
                writer,
            )
            .await
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_once_on_y() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "y\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_once_on_yes() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "yes\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_n() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "n\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_no() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "no\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_reject() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "reject\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_a() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "a\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_always() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "always\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_always_for_session() {
        let (service, args) = terminal_service();
        let result =
            terminal_with_input(&service, &args, "always-for-session\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_unknown_input() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "maybe\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_once_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "y\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Deny)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_always_for_session_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "a\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        assert_eq!(
            terminal_with_input_and_policy(
                &service,
                &args,
                "n\n",
                ApprovalPolicy::AlwaysForSession
            )
            .await,
            Ok(ApprovalOutcome::Approve)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_reject_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Deny)
        );
        assert_eq!(
            terminal_with_input_and_policy(
                &service,
                &args,
                "y\n",
                ApprovalPolicy::Reject
            )
            .await,
            Ok(ApprovalOutcome::Deny)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_times_out() {
        let (service, args) = terminal_service();
        let reader = tokio::io::BufReader::new(tokio::io::empty());
        let writer: Vec<u8> = Vec::new();
        let result = service
            .request_approval_with_io(
                "bash",
                &args,
                ApprovalPolicy::Once,
                Duration::from_millis(10),
                CancellationToken::new(),
                reader,
                writer,
            )
            .await;
        assert_eq!(result, Err(ApprovalError::Timeout));
    }

    #[tokio::test]
    async fn terminal_approval_service_cancels() {
        let (service, args) = terminal_service();
        let reader = tokio::io::BufReader::new("y\n".as_bytes());
        let writer: Vec<u8> = Vec::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = service
            .request_approval_with_io(
                "bash",
                &args,
                ApprovalPolicy::Once,
                Duration::from_secs(30),
                cancel,
                reader,
                writer,
            )
            .await;
        assert_eq!(result, Err(ApprovalError::Cancelled));
    }
}
