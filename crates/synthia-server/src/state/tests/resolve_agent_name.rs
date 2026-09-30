//! Tests for `AppState::resolve_agent_name` — the
//! three-tier fallback ladder every dispatch flows
//! through (`explicit > configured default > first
//! registered`). A regression in the priority ordering
//! would surface as silent "wrong agent" responses, so
//! every branch is pinned here.

use synthia::{core::registry::Registry, harness::AgentRegistry};

use crate::state::{AppState, tests::support::stub_entry};

#[tokio::test]
async fn explicit_name_wins_over_default() {
    let reg = AgentRegistry::new();
    reg.put(stub_entry("a")).await.unwrap();
    reg.put(stub_entry("default-one")).await.unwrap();
    let got =
        AppState::resolve_agent_name(&reg, Some("default-one"), Some("a"));
    assert_eq!(got, Some("a".to_string()));
}

#[tokio::test]
async fn explicit_name_missing_falls_back_to_default() {
    let reg = AgentRegistry::new();
    reg.put(stub_entry("default-one")).await.unwrap();
    let got = AppState::resolve_agent_name(
        &reg,
        Some("default-one"),
        Some("does-not-exist"),
    );
    assert_eq!(got, Some("default-one".to_string()));
}

#[tokio::test]
async fn no_explicit_no_default_falls_back_to_first_registered() {
    let reg = AgentRegistry::new();
    reg.put(stub_entry("first")).await.unwrap();
    reg.put(stub_entry("second")).await.unwrap();
    let got = AppState::resolve_agent_name(&reg, None, None);
    // HashMap iteration order is unspecified — we
    // accept any registered name. The point is: not
    // None.
    assert!(got.is_some());
    assert!(reg.names().contains(&got.unwrap()));
}

#[tokio::test]
async fn empty_registry_returns_none() {
    let reg = AgentRegistry::new();
    assert_eq!(AppState::resolve_agent_name(&reg, None, None), None);
    assert_eq!(
        AppState::resolve_agent_name(&reg, Some("default"), None),
        None
    );
    assert_eq!(
        AppState::resolve_agent_name(&reg, Some("default"), Some("explicit")),
        None
    );
}

/// `explicit = Some("")` (empty string) MUST be treated
/// the same as `None` by the caller (`extract_agent_name`
/// the chat layer already converts "" to None), but
/// defensively, if it ever leaks through, the registry
/// lookup must also produce `None` rather than silently
/// match a (non-existent) ""-named agent.
#[tokio::test]
async fn empty_explicit_string_does_not_match_a_registry_default() {
    let reg = AgentRegistry::new();
    reg.put(stub_entry("only-one")).await.unwrap();
    let got = AppState::resolve_agent_name(&reg, None, Some(""));
    // "" is not registered → fall back to first
    // registered. If the registry had an "" entry, this
    // test would still return Some(""), but we don't
    // register one.
    assert_eq!(got, Some("only-one".to_string()));
}
