#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{
        checker::{PermissionChecker, is_path_in_workspace, normalize_path},
        level::Permission,
        merged_policy::MergedPolicy,
        rule::{PermissionAction, PermissionRule},
        types::PermissionRequest,
    };

    fn req(name: &str, requires: bool) -> PermissionRequest {
        PermissionRequest::new(
            name.to_string(),
            serde_json::json!({}),
            requires,
        )
    }

    fn req_with_input(
        name: &str,
        requires: bool,
        input: serde_json::Value,
    ) -> PermissionRequest {
        PermissionRequest::new(name.to_string(), input, requires)
    }

    fn rule(
        pattern: &str,
        action: PermissionAction,
        forced: bool,
    ) -> PermissionRule {
        PermissionRule {
            pattern: pattern.to_string(),
            action,
            forced,
        }
    }

    /// Build a checker whose `read_file` policy is `Allow` and whose
    /// workspace root is pinned to `/workspace` (deterministic, independent
    /// of the test process's current directory).
    fn checker_in_workspace() -> PermissionChecker {
        let policy = MergedPolicy::new(
            &[rule("read_file", PermissionAction::Allow, false)],
            &[],
            &[],
        );
        PermissionChecker::new(policy)
            .with_workspace_root(Path::new("/workspace"))
    }

    #[tokio::test]
    async fn test_check_no_permission_required() {
        let checker = PermissionChecker::allow_all();
        let decisions =
            checker.check(&[req("no_perm_tool", false)]).await.unwrap();
        assert!(matches!(
            decisions.get("no_perm_tool"),
            Some(Permission::AutoApprove)
        ));
    }

    #[tokio::test]
    async fn test_check_blocked_tool() {
        // ADR-2026-06-10: explicit Deny rule → Block decision.
        let policy = MergedPolicy::new(
            &[rule("req_perm_tool", PermissionAction::Deny, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);
        let decisions =
            checker.check(&[req("req_perm_tool", true)]).await.unwrap();
        assert!(matches!(
            decisions.get("req_perm_tool"),
            Some(Permission::Block)
        ));
    }

    #[tokio::test]
    async fn test_check_auto_approve() {
        let policy = MergedPolicy::new(
            &[rule("req_perm_tool", PermissionAction::Allow, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);
        let decisions =
            checker.check(&[req("req_perm_tool", true)]).await.unwrap();
        assert!(matches!(
            decisions.get("req_perm_tool"),
            Some(Permission::AutoApprove)
        ));
    }

    #[tokio::test]
    async fn test_check_require_confirm() {
        let policy = MergedPolicy::new(
            &[rule("req_perm_tool", PermissionAction::Ask, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);
        let decisions =
            checker.check(&[req("req_perm_tool", true)]).await.unwrap();
        assert!(matches!(
            decisions.get("req_perm_tool"),
            Some(Permission::RequireConfirm)
        ));
    }

    #[tokio::test]
    async fn test_check_unknown_tool_asks() {
        // ADR-2026-06-10: fail-closed default → RequireConfirm.
        let checker = PermissionChecker::allow_all();
        let decisions =
            checker.check(&[req("nonexistent", true)]).await.unwrap();
        assert!(matches!(
            decisions.get("nonexistent"),
            Some(Permission::RequireConfirm)
        ));
    }

    #[tokio::test]
    async fn test_absolute_path_outside_workspace_denied() {
        // Spec: Absolute Path Outside Workspace Denied.
        // `read_file("/etc/passwd")` MUST Deny regardless of policy,
        // because `/etc/passwd` is an absolute path outside the
        // workspace root.
        let checker = checker_in_workspace();
        let input = serde_json::json!({"path": "/etc/passwd"});
        let decisions = checker
            .check(&[req_with_input("read_file", true, input)])
            .await
            .unwrap();
        match decisions.get("read_file") {
            Some(Permission::Deny { reason }) => {
                assert!(
                    reason.contains("outside workspace"),
                    "unexpected deny reason: {reason}"
                );
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_absolute_path_to_ssh_key_denied() {
        // Spec: Absolute Path Outside Workspace Denied (ssh key case).
        let checker = checker_in_workspace();
        let input = serde_json::json!({"path": "/home/victim/.ssh/id_rsa"});
        let decisions = checker
            .check(&[req_with_input("read_file", true, input)])
            .await
            .unwrap();
        assert!(matches!(
            decisions.get("read_file"),
            Some(Permission::Deny { .. })
        ));
    }

    #[tokio::test]
    async fn test_relative_traversal_denied() {
        // Spec: Relative Traversal Denied.
        // `read_file("../../../etc/passwd")` resolves against the
        // workspace root, folds to `/etc/passwd`, and is denied by
        // the prefix-containment check.
        let checker = checker_in_workspace();
        let input = serde_json::json!({"path": "../../../etc/passwd"});
        let decisions = checker
            .check(&[req_with_input("read_file", true, input)])
            .await
            .unwrap();
        assert!(matches!(
            decisions.get("read_file"),
            Some(Permission::Deny { .. })
        ));
    }

    #[tokio::test]
    async fn test_security_dangerous_command() {
        let checker = PermissionChecker::allow_all();
        let input = serde_json::json!({"command": "rm -rf /tmp/test"});
        let decisions = checker
            .check(&[req_with_input("bash", true, input)])
            .await
            .unwrap();
        assert!(matches!(
            decisions.get("bash"),
            Some(Permission::Deny { .. })
        ));
    }

    #[tokio::test]
    async fn test_path_in_write_file_content_is_not_a_traversal() {
        // `write_file` content that mentions an absolute path must not
        // be treated as a path-traversal attempt. Only keys that carry
        // actual file paths (e.g. `path`) should be inspected.
        let checker = PermissionChecker::allow_all()
            .with_workspace_root(Path::new("/workspace"));
        let input = serde_json::json!({
            "path": "docs/example.md",
            "content": "See also /etc/passwd and /home/victim/.ssh/id_rsa"
        });
        let decisions = checker
            .check(&[req_with_input("write_file", true, input)])
            .await
            .unwrap();
        assert!(
            !matches!(
                decisions.get("write_file"),
                Some(Permission::Deny { .. })
            ),
            "content strings must not be parsed as paths"
        );
    }

    #[tokio::test]
    async fn test_write_file_path_still_checked() {
        // The `path` key of `write_file` is still subject to workspace
        // containment even though `content` is ignored.
        let checker = PermissionChecker::allow_all()
            .with_workspace_root(Path::new("/workspace"));
        let input = serde_json::json!({
            "path": "/etc/passwd",
            "content": " harmless content "
        });
        let decisions = checker
            .check(&[req_with_input("write_file", true, input)])
            .await
            .unwrap();
        assert!(
            matches!(
                decisions.get("write_file"),
                Some(Permission::Deny { .. })
            ),
            "path key must still be checked"
        );
    }

    #[tokio::test]
    async fn test_workspace_internal_path_allowed() {
        // Spec: Workspace Internal Path Allowed.
        // `read_file("src/main.rs")` is relative, resolves to
        // `/workspace/src/main.rs`, and passes the containment check;
        // the Allow policy then yields AutoApprove.
        let checker = checker_in_workspace();
        let input = serde_json::json!({"path": "src/main.rs"});
        let decisions = checker
            .check(&[req_with_input("read_file", true, input)])
            .await
            .unwrap();
        assert!(matches!(
            decisions.get("read_file"),
            Some(Permission::AutoApprove)
        ));
    }

    #[test]
    fn test_normalize_folds_dot_segments() {
        // Spec: Normalize Handles Dot Segments.
        // `workspace/src/../src/main.rs` MUST fold to
        // `workspace/src/main.rs`.
        let normalized =
            normalize_path(Path::new("workspace/src/../src/main.rs"));
        assert_eq!(normalized, PathBuf::from("workspace/src/main.rs"));
    }

    #[test]
    fn test_normalize_does_not_call_canonicalize() {
        // Spec: Symbolic Link Not Resolved.
        // `canonicalize()` fails on non-existent paths and performs
        // filesystem I/O. `normalize_path` MUST succeed on a path that
        // does not exist on disk (and contains a `..` segment), which
        // proves it does not consult the filesystem.
        let path = Path::new("/this/surely/does/not/exist/anywhere/target.txt");
        let normalized = normalize_path(path);
        assert_eq!(normalized, path.to_path_buf());

        // A `..` segment under a non-existent directory would make
        // `canonicalize` error; `normalize_path` folds it instead
        // (the `..` pops the preceding `exist` segment).
        let folded = normalize_path(Path::new(
            "/this/surely/does/not/exist/../target.txt",
        ));
        assert_eq!(folded, PathBuf::from("/this/surely/does/not/target.txt"));

        // Sanity: the folded path is recognized as in-workspace.
        assert!(is_path_in_workspace(
            Path::new("/this/surely/does/not/target.txt"),
            Path::new("/this/surely/does/not")
        ));
    }

    #[test]
    fn test_empty_workspace_root_denies_all_paths() {
        // When `workspace_root` normalizes to empty (e.g.
        // `default_workspace_root()` returns `.` because cwd is
        // unavailable, or a caller passes `.`), ALL paths MUST be denied.
        //
        // Without the fail-closed guard in `is_path_in_workspace`,
        // `Path::starts_with("")` returns `true` for any path, which
        // would be a fail-OPEN security bypass allowing access to
        // `/etc/passwd`, `~/.ssh/id_rsa`, etc.
        assert!(!is_path_in_workspace(
            Path::new("/etc/passwd"),
            Path::new(".")
        ));
        assert!(!is_path_in_workspace(
            Path::new("src/main.rs"),
            Path::new(".")
        ));
        assert!(!is_path_in_workspace(
            Path::new("../../../etc/passwd"),
            Path::new(".")
        ));
        // Also covers the empty `PathBuf` directly, in case the sentinel
        // changes shape in the future.
        assert!(!is_path_in_workspace(
            Path::new("/etc/passwd"),
            Path::new("")
        ));
    }

    #[tokio::test]
    async fn test_security_clean_command_passes() {
        let policy = MergedPolicy::new(
            &[rule("bash", PermissionAction::Allow, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);

        let input = serde_json::json!({"command": "ls -la"});
        let decisions = checker
            .check(&[req_with_input("bash", true, input)])
            .await
            .unwrap();
        assert!(matches!(
            decisions.get("bash"),
            Some(Permission::AutoApprove)
        ));
    }

    #[tokio::test]
    async fn test_policy_accessor_returns_underlying_policy() {
        let policy = MergedPolicy::new(
            &[rule("read_file", PermissionAction::Allow, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);
        assert_eq!(checker.policy().len(), 1);
        assert_eq!(
            checker.policy().evaluate("read_file"),
            PermissionAction::Allow
        );
    }

    // ---- F19: saved "always" rule persistence ----

    fn resource_for(input: &serde_json::Value) -> String {
        serde_json::to_string(input).expect("JSON serialization is infallible")
    }

    #[tokio::test]
    async fn test_saved_rule_auto_approve() {
        // Policy says bash -> Ask (RequireConfirm). After
        // `remember_always`, the same (bash, args) pair must
        // short-circuit to AutoApprove without consulting the policy.
        let policy = MergedPolicy::new(
            &[rule("bash", PermissionAction::Ask, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);

        let input = serde_json::json!({"command": "ls"});
        let req = req_with_input("bash", true, input.clone());

        // Without saved rule: policy is consulted → RequireConfirm.
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::RequireConfirm));

        // Save rule and re-check.
        checker.remember_always("bash".to_string(), resource_for(&input));

        let decisions = checker.check(&[req]).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
    }

    #[tokio::test]
    async fn test_saved_rule_no_match_evaluates_policy() {
        // Saved rule for `cargo build` should NOT match a request
        // with `cargo test` — policy must be consulted.
        let policy = MergedPolicy::new(
            &[rule("bash", PermissionAction::Ask, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);

        let saved_input = serde_json::json!({"command": "cargo build"});
        let other_input = serde_json::json!({"command": "cargo test"});

        checker.remember_always("bash".to_string(), resource_for(&saved_input));

        let req = req_with_input("bash", true, other_input);
        let decisions = checker.check(&[req]).await.unwrap();
        // Different resource → no saved-rule match → Ask → RequireConfirm.
        assert_eq!(decisions.get("bash"), Some(&Permission::RequireConfirm));
    }

    #[tokio::test]
    async fn test_remember_always_inserts() {
        // `remember_always` must actually insert into `saved_rules`
        // such that the next `check()` for the same pair returns
        // AutoApprove even when the policy would otherwise Deny.
        let policy = MergedPolicy::new(
            &[rule("bash", PermissionAction::Deny, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);

        let input = serde_json::json!({"command": "cargo build"});
        let req = req_with_input("bash", true, input.clone());

        // Pre-condition: policy alone → Block.
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::Block));

        checker.remember_always("bash".to_string(), resource_for(&input));

        // Post-condition: saved rule overrides policy → AutoApprove.
        let decisions = checker.check(&[req]).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
    }

    #[tokio::test]
    async fn test_forget_always_removes() {
        // After `forget_always`, a previously-saved rule must no
        // longer match — the policy is consulted again.
        let policy = MergedPolicy::new(
            &[rule("bash", PermissionAction::Ask, false)],
            &[],
            &[],
        );
        let checker = PermissionChecker::new(policy);

        let input = serde_json::json!({"command": "ls"});
        let resource = resource_for(&input);
        checker.remember_always("bash".to_string(), resource.clone());

        let req = req_with_input("bash", true, input.clone());
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));

        checker.forget_always("bash", &resource);

        let decisions = checker.check(&[req]).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::RequireConfirm));
    }

    #[tokio::test]
    async fn test_forget_nonexistent_is_noop() {
        // Forgetting a pair that was never saved must not panic and
        // must leave `saved_rules` (and thus `check()`) unaffected.
        let checker = PermissionChecker::allow_all();

        // Should not panic.
        checker.forget_always("never", "saved");

        // `check()` still works as before — `allow_all()` policy +
        // unknown tool defaults to Ask → RequireConfirm.
        let req = req("unknown_tool", true);
        let decisions = checker.check(&[req]).await.unwrap();
        assert_eq!(
            decisions.get("unknown_tool"),
            Some(&Permission::RequireConfirm)
        );
    }
}
