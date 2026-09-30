//! Tests for the R34 `[tools]` boot application — the
//! `apply_tools_config` step that translates the
//! deployment's `groups` / `deferred` / `hidden` /
//! `max_visible` config section into registry writes
//! plus a [`ToolSurfacePolicy`] the agent installs.

use crate::state::{
    AppliedToolSurface,
    apply_tools_config,
    plugin_tool_registry,
};

/// The boot contract: unknown names are skipped (warned)
/// instead of failing boot, the rest lands on the
/// registry.
#[tokio::test]
async fn apply_tools_config_skips_unknown_names_and_reports_the_surface() {
    let registry = plugin_tool_registry();
    let config = crate::config::ToolsConfig {
        deferred: vec!["shell".to_string(), "ghost".to_string()],
        hidden: vec!["TodoWrite".to_string()],
        groups: std::collections::BTreeMap::from([
            (
                "files".to_string(),
                vec!["read".to_string(), "write".to_string()],
            ),
            ("net".to_string(), vec!["web_fetch".to_string()]),
        ]),
        active_groups: vec!["files".to_string(), "nope".to_string()],
        max_visible: Some(3),
    };

    let applied = apply_tools_config(&registry, &config);

    assert_eq!(applied.deferred, vec!["shell"]);
    assert_eq!(applied.hidden, vec!["TodoWrite"]);
    assert_eq!(applied.active_groups, vec!["files"]);
    assert_eq!(
        applied.skipped,
        vec!["nope", "ghost"],
        "unknown names are recorded in application order, not fatal"
    );
    assert_eq!(
        registry.exposure("read"),
        Some(synthia::tool::ToolExposure::Direct)
    );
    assert_eq!(
        registry.exposure("shell"),
        Some(synthia::tool::ToolExposure::Deferred)
    );
    assert_eq!(
        registry.exposure("web_fetch"),
        Some(synthia::tool::ToolExposure::Hidden),
        "an inactive group withholds without unregistering"
    );
    assert!(
        registry.contains("web_fetch"),
        "the withheld tool stays registered and dispatcheable"
    );
    assert!(
        registry.snapshot().iter().all(|s| s.name != "TodoWrite"),
        "the hidden list flips the privacy flag"
    );

    let policy = applied.policy().expect("groups + cap were configured");
    assert_eq!(policy.max_visible, Some(3));
    assert_eq!(policy.active_groups, vec!["files"]);
    assert_eq!(policy.groups["files"], vec!["read", "write"]);
}

/// Write order is `groups` → `deferred` → `hidden`: a
/// later key wins for a tool named by several of them.
#[tokio::test]
async fn apply_tools_config_write_order_gives_later_keys_precedence() {
    let registry = plugin_tool_registry();
    let config = crate::config::ToolsConfig {
        deferred: vec!["read".to_string()],
        hidden: vec!["write".to_string()],
        groups: std::collections::BTreeMap::from([(
            "files".to_string(),
            vec!["read".to_string(), "write".to_string(), "shell".to_string()],
        )]),
        active_groups: vec!["files".to_string()],
        max_visible: None,
    };

    apply_tools_config(&registry, &config);

    assert_eq!(
        registry.exposure("read"),
        Some(synthia::tool::ToolExposure::Deferred),
        "deferred overrides the active-group Direct verdict"
    );
    assert_eq!(
        registry.exposure("shell"),
        Some(synthia::tool::ToolExposure::Direct),
        "an untouched active-group member keeps the group verdict"
    );
    assert!(
        registry.snapshot().iter().all(|s| s.name != "write"),
        "hidden wins over deferred and over the group verdict"
    );
}

/// A deployment with no `[tools]` section is a no-op.
#[tokio::test]
async fn apply_tools_config_without_a_section_is_a_noop() {
    let registry = plugin_tool_registry();
    let version = registry.version();

    let applied =
        apply_tools_config(&registry, &crate::config::ToolsConfig::default());

    assert_eq!(applied, AppliedToolSurface::default());
    assert!(applied.policy().is_none());
    assert_eq!(registry.version(), version, "nothing was written");
}
