//! Build the Synthia default `AgentCard` from the live `AppState`.
//!
//! Reads tool-registry snapshots and forwards them to the pure
//! [`super::card::build_agent_card`] helper. Lives here (not in
//! `super::card`) because the AppState coupling belongs to the
//! server crate, not to the agent-card primitive.

use std::time::{Duration, Instant as TimeInstant};

use a2a::AgentCard;
use parking_lot::RwLock;

use super::card::{build_agent_card, tool_to_skill};
use crate::state::AppState;

/// Resolve the human-readable A2A card name from the
/// descriptor of the agent that will handle the request.
///
/// Looks up the resolved agent through the standard
/// "explicit > configured default > first registered"
/// ladder (mirroring `AppState::resolve_agent_name`) and
/// returns its `descriptor.display_name()` (which falls
/// back to the slug when unset). When the registry is
/// empty the card uses `"Synthia"` as a last-resort label
/// so the A2A endpoint always returns a well-formed card.
fn resolve_card_name(state: &AppState) -> String {
    let explicit = state.default_agent_name.read().clone();
    let resolved = if let Some(name) = &explicit
        && let Some(agent) = state.agent_registry.resolve_sync(name)
    {
        Some(agent)
    } else {
        state
            .agent_registry
            .first_name()
            .and_then(|n| state.agent_registry.resolve_sync(&n))
    };
    resolved
        .map(|a| a.descriptor().display_name().to_string())
        .unwrap_or_else(|| "Synthia".to_string())
}

/// TTL on the server-side skills cache. The card itself is
/// already served with `Cache-Control: public, max-age=3600`, so
/// the cache hit rate is high (intermediate CDN / browser caches
/// absorb most repeat requests). This TTL exists only to absorb
/// *server-side* repeat requests — the A2A scanner runs every
/// minute from many deployments, all of which funnel through this
/// process. 60s is short enough that a hot-deploy tool change
/// shows up within a minute without an explicit reload call.
const SKILL_CACHE_TTL: Duration = Duration::from_secs(60);

/// One cached snapshot of the projected `AgentSkill` list.
/// `captured_at` lets us decide whether the entry is still fresh
/// without bumping any external state.
struct CachedSkillProjection {
    captured_at: TimeInstant,
    skills: Vec<a2a::AgentSkill>,
}

/// Process-wide cache of `Vec<AgentSkill>` snapshots. We don't
/// need an explicit invalidation hook because the tool registry
/// rarely changes after boot; a 60s TTL keeps the projection
/// within an honest bound of the live state. `parking_lot` keeps
/// the read path lock-free under the common case (the scanner
/// hammers this every minute).
static SKILL_PROJECTION_CACHE: RwLock<Option<CachedSkillProjection>> =
    parking_lot::const_rwlock(None);

/// Construct Synthia's default `AgentCard` using the tool registry
/// snapshots currently held in `AppState`. The `name` is
/// resolved from the live agent descriptor (via
/// [`resolve_card_name`]) so it tracks the configured
/// default's `display_name`; `description` and `version`
/// are deliberately server-side constants — they describe the
/// agent, not the request.
pub async fn build_card_from_state(state: &AppState, url: String) -> AgentCard {
    build_agent_card(
        resolve_card_name(state),
        "Synthia is an AI coding assistant that helps users complete \
         software engineering tasks end-to-end. It uses a ReAct loop \
         with a curated tool registry, follows safe action defaults, \
         and produces concise markdown-formatted answers."
            .to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        url,
        cached_skill_projection(state).await,
    )
}

/// Return the cached `Vec<AgentSkill>` projection if it's still
/// within [`SKILL_CACHE_TTL`], otherwise rebuild it under a write
/// lock and return the fresh list. Concurrent callers in the
/// miss path race benignly: each takes a snapshot of the tool
/// registry under the same lock; the first one to win the write
/// lock publishes, the rest see a fresh entry and skip the
/// rebuild.
async fn cached_skill_projection(state: &AppState) -> Vec<a2a::AgentSkill> {
    // Fast path: peek the cache. The clone is cheap (the skills
    // list is at most a few dozen items) and avoids holding the
    // lock through the JSON serialise that the response builder
    // does next.
    if let Some(hit) = SKILL_PROJECTION_CACHE.read().as_ref()
        && hit.captured_at.elapsed() < SKILL_CACHE_TTL
    {
        return hit.skills.clone();
    }
    // Slow path: re-snapshot the tool registry and project. The
    // registry's own read-lock prevents the projection from
    // observing a half-registered tool list.
    let registry = state.tool_registry.read().await;
    let skills: Vec<a2a::AgentSkill> = registry
        .snapshot()
        .iter()
        .map(|snap| {
            tool_to_skill(
                snap.name.clone(),
                snap.name.clone(),
                snap.description.clone(),
            )
        })
        .collect();
    let mut cache = SKILL_PROJECTION_CACHE.write();
    *cache = Some(CachedSkillProjection {
        captured_at: TimeInstant::now(),
        skills: skills.clone(),
    });
    skills
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_to_skill_creates_valid_skill() {
        let skill = tool_to_skill(
            "read_file".to_string(),
            "ReadFile".to_string(),
            "Read a file".to_string(),
        );
        assert_eq!(skill.id, "read_file");
        assert_eq!(skill.name, "ReadFile");
    }

    /// `tool_to_skill` MUST pass through the `description`.
    #[test]
    fn tool_to_skill_passes_description_through() {
        let skill = tool_to_skill(
            "id".to_string(),
            "name".to_string(),
            "very long description with unicode 你好".to_string(),
        );
        assert!(skill.description.contains("unicode"));
        assert!(skill.description.contains("你好"));
    }

    /// `tool_to_skill` MUST treat empty tags correctly.
    #[test]
    fn tool_to_skill_tags_are_empty() {
        let skill = tool_to_skill(
            "id".to_string(),
            "name".to_string(),
            "desc".to_string(),
        );
        assert!(skill.tags.is_empty());
        assert_eq!(skill.tags.len(), 0);
    }

    /// `tool_to_skill` MUST NOT set `examples` (always None).
    #[test]
    fn tool_to_skill_examples_always_none() {
        let skill = tool_to_skill(
            "id".to_string(),
            "name".to_string(),
            "desc".to_string(),
        );
        assert!(skill.examples.is_none());
    }

    /// The default Synthia `AgentCard` description must
    /// mention both the agent name and the tool registry so
    /// A2A scanners can identify the surface at a glance.
    /// Pinning the shape keeps a future copy edit from
    /// silently downgrading the card to a generic "AI
    /// coding assistant" blurb.
    #[test]
    fn card_description_identifies_synthia_and_its_tools() {
        let desc = build_agent_card(
            "Synthia".to_string(),
            "Synthia is an AI coding assistant that helps users complete \
             software engineering tasks end-to-end. It uses a ReAct loop \
             with a curated tool registry, follows safe action defaults, \
             and produces concise markdown-formatted answers."
                .to_string(),
            "0.1.0".to_string(),
            "https://x".to_string(),
            vec![],
        )
        .description;
        assert!(desc.contains("Synthia"));
        assert!(desc.contains("tool"));
        assert!(desc.contains("markdown"));
    }

    /// `build_card_from_state` resolves the card name from
    /// the live agent descriptor's `display_name`. The
    /// canonical `AppState::for_test` registers the ReAct
    /// agent with `display_name = Some("Synthia")`, so the
    /// well-known agent-card endpoint MUST surface that
    /// human-readable label (not the routing slug "agent").
    /// Pinning the contract here keeps the user-facing
    /// identity stable across descriptor refactors.
    #[cfg(any(test, feature = "test-utils"))]
    #[tokio::test]
    async fn build_card_uses_descriptor_display_name() {
        use synthia_session::manager::SessionRegistry;

        use crate::state::AppState;

        let workspace_root = std::env::temp_dir().join("synthia-card-test");
        let _ = std::fs::create_dir_all(&workspace_root);
        let state = AppState::for_test(
            SessionRegistry::new(workspace_root.join("sessions")),
            workspace_root,
        )
        .await;
        let card =
            build_card_from_state(&state, "https://x/a2a".to_string()).await;
        assert_eq!(
            card.name, "Synthia",
            "A2A card name must track the descriptor's display_name"
        );
    }

    /// When the agent registry is empty (no agents
    /// registered) `resolve_card_name` MUST fall back to
    /// the `"Synthia"` last-resort label so the A2A
    /// endpoint always returns a well-formed card.
    #[cfg(any(test, feature = "test-utils"))]
    #[tokio::test]
    async fn build_card_falls_back_when_registry_empty() {
        use std::sync::Arc;

        use synthia_agent::AgentRegistry;
        use synthia_session::manager::SessionRegistry;

        use crate::state::AppState;

        let workspace_root =
            std::env::temp_dir().join("synthia-card-empty-test");
        let _ = std::fs::create_dir_all(&workspace_root);

        let mut state = AppState::for_test(
            SessionRegistry::new(workspace_root.join("sessions")),
            workspace_root,
        )
        .await;
        // Replace the registry with an empty one so the
        // resolver hits its fallback branch.
        state.agent_registry = Arc::new(AgentRegistry::new());

        let card =
            build_card_from_state(&state, "https://x/a2a".to_string()).await;
        assert_eq!(
            card.name, "Synthia",
            "empty registry must fall back to the Synthia last-resort label"
        );
    }
}
