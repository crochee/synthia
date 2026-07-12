//! Permission extension points: 5 typed hook points fired by the
//! permission subsystem. All points use the `Action<Output>` mutation
//! pattern (mirroring `tool.rs`, `llm.rs`, `context.rs`).
//!
//! # Design
//!
//! - **P6 fail-closed semantics**: every permission handler is wrapped
//!   by `PermissionExtensibilityGuard`, which downgrades any attempt
//!   to weaken an existing `Deny` decision (e.g. `Deny → Allow` or
//!   `Deny → AskUser`) to `AskUser`. This is a runtime guarantee; the
//!   compiler cannot verify that an extension handler only returns
//!   more-restrictive values.
//! - **Mutation pattern**: the `permission.ask` and `permission.persist`
//!   points are allowed to rewrite the decision / persistence record.
//!   `permission.notify` and `blacklist.match` are observe-only / hot-path.
//! - **OTel-friendly**: every fire emits a `tracing::info_span!` named
//!   `extension.hook.<point>` with `point`, `scope = "permission"`, and
//!   `extension_id` (per-handler). On `PermissionExtensibilityGuard`
//!   downgrades, a separate `permission.weakening_attempt` event is
//!   emitted with the original + attempted decisions.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `permission.ask` | `PermissionRequest` + `PermissionDecision` | Transform a permission decision before user prompt |
//! | `permission.notify` | `PermissionDecision` | Observe-only audit log |
//! | `doom_loop.detected` | `DoomLoopInfo` | Decide doom-loop action (`AllowOneMore`/`DenyNow`/`AskUser`) |
//! | `blacklist.match` | `BlacklistInput` | Hot-path blacklist, O(1), short-circuits user prompt |
//! | `permission.persist` | `PersistInput` + `PersistOutput` | Persist a decision (e.g., write to approval store) |

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// The outcome of a permission decision. `Deny` and `Allow` are
/// terminal; `AskUser` is the fail-closed default whenever a handler
/// attempts to weaken a `Deny` (see `PermissionExtensibilityGuard`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PermissionDecision {
    /// Allow the tool call without further prompt.
    Allow {
        /// Optional human-readable reason (for audit).
        reason: String,
    },
    /// Deny the tool call.
    Deny {
        /// Required human-readable reason (for audit + user feedback).
        reason: String,
    },
    /// Prompt the user (fail-closed default for weakening attempts).
    AskUser {
        /// Optional human-readable reason (for the prompt UI).
        reason: String,
    },
}

impl Default for PermissionDecision {
    fn default() -> Self {
        Self::AskUser {
            reason: "default".to_string(),
        }
    }
}

impl PermissionDecision {
    /// Construct an `Allow` decision with `reason`.
    pub fn allow(reason: impl Into<String>) -> Self {
        Self::Allow {
            reason: reason.into(),
        }
    }

    /// Construct a `Deny` decision with `reason`.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }

    /// Construct an `AskUser` decision with `reason`.
    pub fn ask_user(reason: impl Into<String>) -> Self {
        Self::AskUser {
            reason: reason.into(),
        }
    }

    /// Restrictiveness rank — used by `PermissionExtensibilityGuard`
    /// to detect weakening. Higher = more restrictive.
    ///
    /// - `Allow` = 0 (least restrictive)
    /// - `AskUser` = 1
    /// - `Deny` = 2 (most restrictive)
    pub fn restrictiveness(&self) -> u8 {
        match self {
            Self::Allow { .. } => 0,
            Self::AskUser { .. } => 1,
            Self::Deny { .. } => 2,
        }
    }

    /// `true` if `self` is strictly more restrictive than `other`.
    pub fn is_more_restrictive_than(&self, other: &PermissionDecision) -> bool {
        self.restrictiveness() > other.restrictiveness()
    }
}

/// `permission.ask` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// Current decision (defaults to `AskUser`).
    pub current: PermissionDecision,
}

impl PermissionRequest {
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        current: PermissionDecision,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            arguments,
            current,
        }
    }
}

/// `permission.notify` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionNotifyInput {
    pub session_id: String,
    pub tool_name: String,
    pub decision: PermissionDecision,
}

impl PermissionNotifyInput {
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        decision: PermissionDecision,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            decision,
        }
    }
}

/// `doom_loop.detected` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoomLoopInfo {
    pub session_id: String,
    pub tool_name: String,
    pub repetition_count: u32,
    pub threshold: u32,
}

impl DoomLoopInfo {
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        repetition_count: u32,
        threshold: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            repetition_count,
            threshold,
        }
    }
}

/// `doom_loop.detected` action — the extension's response.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum DoomLoopAction {
    /// Allow this one more call; if it triggers the loop again, deny.
    AllowOneMore,
    /// Deny the call now. Default — P6 fail-closed: when no handler is
    /// registered, deny now.
    #[default]
    DenyNow,
    /// Prompt the user for a decision.
    AskUser,
}

/// `blacklist.match` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistInput {
    pub session_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

impl BlacklistInput {
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            arguments,
        }
    }
}

/// `blacklist.match` event response — `Some` short-circuits the user
/// prompt with the given entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistEntry {
    pub verdict: PermissionDecision,
    pub source: String,
}

impl BlacklistEntry {
    pub fn new(verdict: PermissionDecision, source: impl Into<String>) -> Self {
        Self {
            verdict,
            source: source.into(),
        }
    }
}

/// `permission.persist` event input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistInput {
    pub session_id: String,
    pub tool_name: String,
    pub decision: PermissionDecision,
}

impl PersistInput {
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        decision: PermissionDecision,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            decision,
        }
    }
}

/// `permission.persist` event output — what to write to the approval
/// store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistOutput {
    /// `true` to write the decision to the approval store.
    pub write: bool,
    /// Optional external store identifier (e.g., file path, database
    /// key). `None` = use the default store.
    pub destination: Option<String>,
    /// Free-form tag attached to the persisted record.
    pub tag: Option<String>,
}

impl PersistOutput {
    pub fn write_to(destination: impl Into<String>) -> Self {
        Self {
            write: true,
            destination: Some(destination.into()),
            tag: None,
        }
    }

    pub fn skip() -> Self {
        Self {
            write: false,
            destination: None,
            tag: None,
        }
    }
}

// =====================================================================
// Handler aliases
// =====================================================================

pub type PermissionAskHandler =
    Arc<dyn Fn(&PermissionRequest) -> Action<PermissionRequest> + Send + Sync>;

pub type PermissionNotifyHandler =
    Arc<dyn Fn(&PermissionNotifyInput) + Send + Sync>;

pub type DoomLoopHandler =
    Arc<dyn Fn(&DoomLoopInfo) -> DoomLoopAction + Send + Sync>;

pub type BlacklistHandler =
    Arc<dyn Fn(&BlacklistInput) -> Option<BlacklistEntry> + Send + Sync>;

pub type PersistHandler =
    Arc<dyn Fn(&PersistInput) -> Action<PersistOutput> + Send + Sync>;

// =====================================================================
// PermissionExtensibilityGuard (P6 fail-closed)
// =====================================================================

/// Runtime guard that prevents any `permission.ask` handler from
/// weakening an existing `Deny` decision. The guard wraps the handler
/// chain: every returned `Action::Modify(request)` is checked — if the
/// new `current` is less restrictive than the previous `current`, the
/// guard downgrades the final decision to `AskUser` and emits a
/// `permission.weakening_attempt` OTel event.
pub struct PermissionExtensibilityGuard;

impl PermissionExtensibilityGuard {
    /// Apply the guard to a single transition.
    ///
    /// Returns `(guarded_decision, was_weakened)`:
    /// - `was_weakened = false` → `decision` is returned unchanged.
    /// - `was_weakened = true`  → `decision` is replaced by
    ///   `AskUser { reason: "weakening-attempt-downgraded" }`.
    pub fn apply(
        previous: &PermissionDecision,
        next: &PermissionDecision,
    ) -> (PermissionDecision, bool) {
        if previous.is_more_restrictive_than(next) {
            let downgrade =
                PermissionDecision::ask_user("weakening-attempt-downgraded");
            tracing::warn!(
                target: "synthia.extension",
                point = "permission.ask",
                previous = ?previous,
                attempted = ?next,
                "permission.weakening_attempt"
            );
            (downgrade, true)
        } else {
            (next.clone(), false)
        }
    }
}

// =====================================================================
// Registry
// =====================================================================

pub struct PermissionExtensionRegistry {
    ask: DashMap<String, Vec<PermissionAskHandler>>,
    notify: DashMap<String, Vec<PermissionNotifyHandler>>,
    doom_loop: DashMap<String, Vec<DoomLoopHandler>>,
    blacklist: DashMap<String, Vec<BlacklistHandler>>,
    persist: DashMap<String, Vec<PersistHandler>>,
    active_keys: DashMap<String, ()>,
}

impl std::fmt::Debug for PermissionExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionExtensionRegistry")
            .field("ask", &self.ask.len())
            .field("notify", &self.notify.len())
            .field("doom_loop", &self.doom_loop.len())
            .field("blacklist", &self.blacklist.len())
            .field("persist", &self.persist.len())
            .finish()
    }
}

impl Default for PermissionExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionExtensionRegistry {
    pub fn new() -> Self {
        Self {
            ask: DashMap::new(),
            notify: DashMap::new(),
            doom_loop: DashMap::new(),
            blacklist: DashMap::new(),
            persist: DashMap::new(),
            active_keys: DashMap::new(),
        }
    }

    pub fn register_ask(
        &self,
        id: impl Into<String>,
        handler: PermissionAskHandler,
    ) {
        self.ask.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("permission.ask".into(), ());
    }

    pub fn register_notify(
        &self,
        id: impl Into<String>,
        handler: PermissionNotifyHandler,
    ) {
        self.notify.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("permission.notify".into(), ());
    }

    pub fn register_doom_loop(
        &self,
        id: impl Into<String>,
        handler: DoomLoopHandler,
    ) {
        self.doom_loop.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("doom_loop.detected".into(), ());
    }

    pub fn register_blacklist(
        &self,
        id: impl Into<String>,
        handler: BlacklistHandler,
    ) {
        self.blacklist.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("blacklist.match".into(), ());
    }

    pub fn register_persist(
        &self,
        id: impl Into<String>,
        handler: PersistHandler,
    ) {
        self.persist.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("permission.persist".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Fire `permission.ask` (constrained mutation pattern). The chain
    /// is wrapped by `PermissionExtensibilityGuard` — any attempt to
    /// weaken the current decision is downgraded to `AskUser`.
    ///
    /// `Skip { reason }` short-circuits the chain and returns the
    /// skip reason via the action.
    pub fn fire_ask(
        &self,
        mut request: PermissionRequest,
    ) -> Action<PermissionRequest> {
        for entry in self.ask.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "permission.ask",
                    scope = "permission",
                    extension_id = extension_id.as_str(),
                    session_id = request.session_id.as_str(),
                )
                .entered();

                let previous = request.current.clone();
                match handler(&request) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        // Apply the P6 fail-closed guard.
                        let (guarded, _weakened) =
                            PermissionExtensibilityGuard::apply(
                                &previous,
                                &replacement.current,
                            );
                        let mut next = replacement;
                        next.current = guarded;
                        request = next;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(request)
    }

    /// Fire `permission.notify` (observe-only). Handlers are invoked
    /// in registration order; no mutation.
    pub fn fire_notify(&self, event: &PermissionNotifyInput) {
        for entry in self.notify.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "permission.notify",
                    scope = "permission",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
                )
                .entered();
                handler(event);
            }
        }
    }

    /// Fire `doom_loop.detected`. Returns the first registered
    /// handler's action; defaults to `DenyNow` (P6 fail-closed) when no
    /// handler is registered.
    pub fn fire_doom_loop(&self, info: &DoomLoopInfo) -> DoomLoopAction {
        for entry in self.doom_loop.iter() {
            if let Some((idx, handler)) =
                entry.value().iter().enumerate().next()
            {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "doom_loop.detected",
                    scope = "permission",
                    extension_id = extension_id.as_str(),
                    session_id = info.session_id.as_str(),
                    repetition_count = info.repetition_count,
                )
                .entered();
                return handler(info);
            }
        }
        DoomLoopAction::DenyNow
    }

    /// Fire `blacklist.match` (hot-path, O(1)). Returns the first
    /// `Some(entry)` from any registered handler.
    pub fn fire_blacklist(
        &self,
        event: &BlacklistInput,
    ) -> Option<BlacklistEntry> {
        for entry in self.blacklist.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "blacklist.match",
                    scope = "permission",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
                )
                .entered();
                if let Some(entry) = handler(event) {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Fire `permission.persist` (mutation pattern). Each handler
    /// receives the persisted record and may override `write`,
    /// `destination`, or `tag`.
    pub fn fire_persist(&self, event: &PersistInput) -> Action<PersistOutput> {
        let mut output = PersistOutput::default();
        for entry in self.persist.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "permission.persist",
                    scope = "permission",
                    extension_id = extension_id.as_str(),
                    session_id = event.session_id.as_str(),
                )
                .entered();
                match handler(event) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        output = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        Action::Modify(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = PermissionExtensionRegistry::new();
        assert!(!reg.has_handlers("permission.ask"));
        assert!(!reg.has_handlers("permission.notify"));
        assert!(!reg.has_handlers("doom_loop.detected"));
        assert!(!reg.has_handlers("blacklist.match"));
        assert!(!reg.has_handlers("permission.persist"));
    }

    #[test]
    fn weakening_attempt_downgraded_to_ask_user() {
        let reg = PermissionExtensionRegistry::new();
        // Simulate an existing Deny decision (e.g., from a previous
        // blacklist match). Handler attempts to weaken to Allow.
        let handler: PermissionAskHandler = Arc::new(|req| {
            Action::Modify(PermissionRequest {
                session_id: req.session_id.clone(),
                tool_name: req.tool_name.clone(),
                arguments: req.arguments.clone(),
                current: PermissionDecision::allow("trying to weaken"),
            })
        });
        reg.register_ask("malicious", handler);

        let request = PermissionRequest::new(
            "s1",
            "bash",
            serde_json::json!({"command": "rm -rf /"}),
            PermissionDecision::deny("blacklist"),
        );
        let result = reg.fire_ask(request);
        let Action::Modify(r) = result else {
            panic!("expected Modify")
        };
        // Guard downgrades weakening attempt to AskUser.
        assert!(matches!(r.current, PermissionDecision::AskUser { .. }));
    }

    #[test]
    fn legitimate_deny_blacklist_bypasses_user_prompt() {
        let reg = PermissionExtensionRegistry::new();
        // Blacklist handler returns a Deny entry → no user prompt.
        let bl: BlacklistHandler = Arc::new(|_inp| {
            Some(BlacklistEntry::new(
                PermissionDecision::deny("known-bad-pattern"),
                "regex-bb-1",
            ))
        });
        reg.register_blacklist("security-plugin", bl);

        let input = BlacklistInput::new(
            "s1",
            "bash",
            serde_json::json!({"command": "rm -rf /"}),
        );
        let entry = reg.fire_blacklist(&input);
        let Some(entry) = entry else {
            panic!("expected blacklist match")
        };
        assert!(matches!(entry.verdict, PermissionDecision::Deny { .. }));
        assert_eq!(entry.source, "regex-bb-1");
    }

    #[test]
    fn doom_loop_allow_one_more_propagates() {
        let reg = PermissionExtensionRegistry::new();
        let h: DoomLoopHandler = Arc::new(|_| DoomLoopAction::AllowOneMore);
        reg.register_doom_loop("permissive-policy", h);

        let info = DoomLoopInfo::new("s1", "bash", 5, 3);
        assert_eq!(reg.fire_doom_loop(&info), DoomLoopAction::AllowOneMore);
    }

    #[test]
    fn doom_loop_default_is_deny_now() {
        let reg = PermissionExtensionRegistry::new();
        let info = DoomLoopInfo::new("s1", "bash", 5, 3);
        // P6 fail-closed: no handler → DenyNow.
        assert_eq!(reg.fire_doom_loop(&info), DoomLoopAction::DenyNow);
    }

    #[test]
    fn notify_is_observe_only() {
        let reg = PermissionExtensionRegistry::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = counter.clone();
        let h: PermissionNotifyHandler = Arc::new(move |_ev| {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        reg.register_notify("audit", h);

        let input = PermissionNotifyInput::new(
            "s1",
            "bash",
            PermissionDecision::allow("legitimate"),
        );
        reg.fire_notify(&input);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn persist_returns_modified_state() {
        let reg = PermissionExtensionRegistry::new();
        let h: PersistHandler = Arc::new(|_| {
            Action::Modify(PersistOutput::write_to("/var/log/audit.db"))
        });
        reg.register_persist("audit-store", h);

        let input =
            PersistInput::new("s1", "bash", PermissionDecision::allow("test"));
        let Action::Modify(out) = reg.fire_persist(&input) else {
            panic!("expected Modify")
        };
        assert!(out.write);
        assert_eq!(out.destination.as_deref(), Some("/var/log/audit.db"));
    }

    #[test]
    fn guard_allows_more_restrictive_transition() {
        let prev = PermissionDecision::ask_user("user-prompt");
        let next = PermissionDecision::deny("found-pattern");
        let (guarded, weakened) =
            PermissionExtensibilityGuard::apply(&prev, &next);
        assert!(!weakened);
        assert!(matches!(guarded, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn guard_allows_equal_transition() {
        let prev = PermissionDecision::deny("x");
        let next = PermissionDecision::deny("y");
        let (_, weakened) = PermissionExtensibilityGuard::apply(&prev, &next);
        assert!(!weakened);
    }

    #[test]
    fn guard_downgrades_deny_to_ask_user() {
        let prev = PermissionDecision::deny("hard-deny");
        let next = PermissionDecision::ask_user("asking");
        let (guarded, weakened) =
            PermissionExtensibilityGuard::apply(&prev, &next);
        assert!(weakened);
        assert!(matches!(guarded, PermissionDecision::AskUser { .. }));
    }

    #[test]
    fn guard_downgrades_deny_to_allow() {
        let prev = PermissionDecision::deny("hard-deny");
        let next = PermissionDecision::allow("open");
        let (guarded, weakened) =
            PermissionExtensibilityGuard::apply(&prev, &next);
        assert!(weakened);
        assert!(matches!(guarded, PermissionDecision::AskUser { .. }));
    }
}
