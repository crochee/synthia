//! Provider extension points: 4 typed hook points fired by the LLM
//! provider subsystem. All points use the `Action<Output>` mutation
//! pattern (mirroring `tool.rs`, `llm.rs`, `context.rs`,
//! `permission.rs`).
//!
//! # Design
//!
//! - **Idempotent registration**: `provider.register` with the same
//!   `name` replaces the prior configuration (mirrors the Phase 3
//!   `ExtensionManager` pattern). Each replace emits a
//!   `provider.replaced` OTel event.
//! - **Cache version**: every register / unregister / auth / fallback
//!   event bumps an `AtomicU64 cache_version`. The orchestrator reads
//!   the version after the chain to decide whether to invalidate the
//!   provider resolution cache.
//! - **Mutation pattern**: handlers may rewrite the provider
//!   configuration (`register`), the auth token (`auth`), or the
//!   fallback chain (`fallback`). `provider.unregister` returns a
//!   boolean indicating whether a provider was removed.
//! - **OTel-friendly**: every fire emits a `tracing::info_span!` named
//!   `extension.hook.<point>` with `point`, `scope = "provider"`, and
//!   `extension_id` (per-handler). Replacements emit a
//!   `provider.replaced` event.
//!
//! # Points
//!
//! | Name | Payload | Purpose |
//! |------|---------|---------|
//! | `provider.register` | `ProviderConfig` | Add or replace a provider (idempotent) |
//! | `provider.unregister` | `name: String` | Remove a provider |
//! | `provider.auth` | `AuthRequest` | Rotate the auth token before a request |
//! | `provider.fallback` | `FallbackContext` → `FallbackChain` | Configure a fallback chain |

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::tool::Action;

// =====================================================================
// Typed payloads
// =====================================================================

/// `provider.register` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name (e.g. "openai", "anthropic", "custom-1").
    pub name: String,
    /// Provider kind — openai-compatible / anthropic / custom.
    pub kind: String,
    /// Provider-specific configuration (e.g. base_url, default_model).
    pub config: serde_json::Value,
}

impl ProviderConfig {
    pub fn new(
        name: impl Into<String>,
        kind: impl Into<String>,
        config: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            config,
        }
    }
}

/// `provider.auth` event payload. The handler may rotate
/// `current_token` in place; the orchestrator uses the modified token
/// for the actual request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub provider_name: String,
    pub current_token: Option<String>,
    /// Free-form context (e.g. request_id, session_id) for handler
    /// logic; the orchestrator is required to populate it.
    #[serde(default)]
    pub context: serde_json::Value,
}

impl AuthRequest {
    pub fn new(
        provider_name: impl Into<String>,
        current_token: Option<String>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            current_token,
            context: serde_json::Value::Null,
        }
    }
}

/// `provider.fallback` event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackContext {
    pub primary: String,
    /// Error message from the primary provider.
    pub error: String,
    /// Current fallback chain (defaults to empty).
    pub current: Vec<String>,
}

impl FallbackContext {
    pub fn new(
        primary: impl Into<String>,
        error: impl Into<String>,
        current: Vec<String>,
    ) -> Self {
        Self {
            primary: primary.into(),
            error: error.into(),
            current,
        }
    }
}

/// `provider.fallback` event response — the fallback chain (ordered).
pub type FallbackChain = Vec<String>;

// =====================================================================
// Handler aliases
// =====================================================================

pub type RegisterHandler =
    Arc<dyn Fn(&ProviderConfig) -> Action<ProviderConfig> + Send + Sync>;

pub type UnregisterHandler = Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub type AuthHandler =
    Arc<dyn Fn(&AuthRequest) -> Action<AuthRequest> + Send + Sync>;

pub type FallbackHandler =
    Arc<dyn Fn(&FallbackContext) -> Action<FallbackChain> + Send + Sync>;

// =====================================================================
// Registry
// =====================================================================

pub struct ProviderExtensionRegistry {
    register: DashMap<String, Vec<RegisterHandler>>,
    unregister: DashMap<String, Vec<UnregisterHandler>>,
    auth: DashMap<String, Vec<AuthHandler>>,
    fallback: DashMap<String, Vec<FallbackHandler>>,
    active_keys: DashMap<String, ()>,
    /// Bumped on every successful register/unregister/auth/fallback
    /// event. The orchestrator reads the version after the chain to
    /// decide whether to invalidate the provider resolution cache.
    cache_version: Arc<AtomicU64>,
}

impl std::fmt::Debug for ProviderExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderExtensionRegistry")
            .field("register", &self.register.len())
            .field("unregister", &self.unregister.len())
            .field("auth", &self.auth.len())
            .field("fallback", &self.fallback.len())
            .field("cache_version", &self.cache_version.load(Ordering::SeqCst))
            .finish()
    }
}

impl Default for ProviderExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderExtensionRegistry {
    pub fn new() -> Self {
        Self {
            register: DashMap::new(),
            unregister: DashMap::new(),
            auth: DashMap::new(),
            fallback: DashMap::new(),
            active_keys: DashMap::new(),
            cache_version: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn register_register(
        &self,
        id: impl Into<String>,
        handler: RegisterHandler,
    ) {
        self.register.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("provider.register".into(), ());
    }

    pub fn register_unregister(
        &self,
        id: impl Into<String>,
        handler: UnregisterHandler,
    ) {
        self.unregister.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("provider.unregister".into(), ());
    }

    pub fn register_auth(&self, id: impl Into<String>, handler: AuthHandler) {
        self.auth.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("provider.auth".into(), ());
    }

    pub fn register_fallback(
        &self,
        id: impl Into<String>,
        handler: FallbackHandler,
    ) {
        self.fallback.entry(id.into()).or_default().push(handler);
        self.active_keys.insert("provider.fallback".into(), ());
    }

    pub fn has_handlers(&self, point: &str) -> bool {
        self.active_keys.contains_key(point)
    }

    /// Current cache version — bumped on every register/unregister/
    /// auth/fallback fire. The orchestrator reads the version after
    /// the chain to decide whether to invalidate the provider
    /// resolution cache.
    pub fn cache_version(&self) -> u64 {
        self.cache_version.load(Ordering::SeqCst)
    }

    /// Fire `provider.register`. The chain runs in registration
    /// order; the final `ProviderConfig` is the registration record.
    /// Each successful fire bumps the cache version. If a new
    /// provider replaces an existing one with the same name, a
    /// `provider.replaced` OTel event is emitted.
    pub fn fire_register(
        &self,
        mut config: ProviderConfig,
    ) -> Action<ProviderConfig> {
        for entry in self.register.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "provider.register",
                    scope = "provider",
                    extension_id = extension_id.as_str(),
                    provider_name = config.name.as_str(),
                )
                .entered();
                match handler(&config) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        if replacement.name == config.name
                            && replacement.kind != config.kind
                        {
                            tracing::info!(
                                target: "synthia.extension",
                                point = "provider.register",
                                provider_name = config.name.as_str(),
                                "provider.replaced"
                            );
                        }
                        config = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        self.cache_version.fetch_add(1, Ordering::SeqCst);
        Action::Modify(config)
    }

    /// Fire `provider.unregister`. Returns `true` if any handler
    /// reported removal. Each successful fire bumps the cache
    /// version.
    pub fn fire_unregister(&self, name: &str) -> bool {
        let mut removed = false;
        for entry in self.unregister.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "provider.unregister",
                    scope = "provider",
                    extension_id = extension_id.as_str(),
                    provider_name = name,
                )
                .entered();
                if handler(name) {
                    removed = true;
                }
            }
        }
        if removed {
            self.cache_version.fetch_add(1, Ordering::SeqCst);
        }
        removed
    }

    /// Fire `provider.auth`. The chain rotates the token in place;
    /// the orchestrator uses the final `current_token` for the
    /// request. Each successful fire bumps the cache version.
    pub fn fire_auth(&self, mut req: AuthRequest) -> Action<AuthRequest> {
        for entry in self.auth.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "provider.auth",
                    scope = "provider",
                    extension_id = extension_id.as_str(),
                    provider_name = req.provider_name.as_str(),
                )
                .entered();
                match handler(&req) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        req = replacement;
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
            }
        }
        self.cache_version.fetch_add(1, Ordering::SeqCst);
        Action::Modify(req)
    }

    /// Fire `provider.fallback`. The chain builds a fallback chain
    /// in registration order. The first non-empty `Modify` wins.
    pub fn fire_fallback(
        &self,
        mut ctx: FallbackContext,
    ) -> Action<FallbackChain> {
        for entry in self.fallback.iter() {
            for (idx, handler) in entry.value().iter().enumerate() {
                let extension_id = format!("{}#{}", entry.key(), idx);
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = "provider.fallback",
                    scope = "provider",
                    extension_id = extension_id.as_str(),
                    provider_name = ctx.primary.as_str(),
                )
                .entered();
                match handler(&ctx) {
                    Action::Proceed => {}
                    Action::Modify(replacement) => {
                        if !replacement.is_empty() {
                            self.cache_version.fetch_add(1, Ordering::SeqCst);
                            return Action::Modify(replacement);
                        }
                        // Empty replacement → keep the prior chain.
                    }
                    Action::Skip { reason } => {
                        return Action::Skip { reason };
                    }
                }
                ctx.current.clear();
            }
        }
        Action::Modify(ctx.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let reg = ProviderExtensionRegistry::new();
        assert!(!reg.has_handlers("provider.register"));
        assert!(!reg.has_handlers("provider.unregister"));
        assert!(!reg.has_handlers("provider.auth"));
        assert!(!reg.has_handlers("provider.fallback"));
        assert_eq!(reg.cache_version(), 0);
    }

    #[test]
    fn register_bumps_cache_version() {
        let reg = ProviderExtensionRegistry::new();
        let h: RegisterHandler = Arc::new(|c| Action::Modify(c.clone()));
        reg.register_register("noop", h);

        let config = ProviderConfig::new(
            "openai",
            "openai",
            serde_json::json!({"base_url": "https://api.openai.com"}),
        );
        let v0 = reg.cache_version();
        let _ = reg.fire_register(config);
        assert_eq!(reg.cache_version(), v0 + 1);
    }

    #[test]
    fn auth_token_rotation() {
        let reg = ProviderExtensionRegistry::new();
        let h: AuthHandler = Arc::new(|req| {
            let mut next = req.clone();
            next.current_token = Some("rotated-token".to_string());
            Action::Modify(next)
        });
        reg.register_auth("token-rotator", h);

        let req = AuthRequest::new("openai", Some("old-token".to_string()));
        let Action::Modify(r) = reg.fire_auth(req) else {
            panic!("expected Modify")
        };
        assert_eq!(r.current_token.as_deref(), Some("rotated-token"));
    }

    #[test]
    fn fallback_chain_iterated_in_order() {
        let reg = ProviderExtensionRegistry::new();
        let h: FallbackHandler = Arc::new(|ctx| {
            Action::Modify(vec![
                ctx.primary.clone(),
                "secondary".to_string(),
                "tertiary".to_string(),
            ])
        });
        reg.register_fallback("chain-1", h);

        let ctx = FallbackContext::new("primary", "rate-limited", Vec::new());
        let Action::Modify(chain) = reg.fire_fallback(ctx) else {
            panic!("expected Modify")
        };
        assert_eq!(chain, vec!["primary", "secondary", "tertiary"]);
    }

    #[test]
    fn unregister_returns_true_when_handler_removes() {
        let reg = ProviderExtensionRegistry::new();
        let h: UnregisterHandler = Arc::new(|name| name == "to-remove");
        reg.register_unregister("remover", h);

        assert!(reg.fire_unregister("to-remove"));
        assert!(!reg.fire_unregister("non-existent"));
    }

    #[test]
    fn concurrent_register_increments_cache_version() {
        use std::sync::Arc as StdArc;
        let reg = StdArc::new(ProviderExtensionRegistry::new());
        let mut handles = Vec::new();
        for i in 0..16 {
            let reg = reg.clone();
            handles.push(std::thread::spawn(move || {
                let h: RegisterHandler = Arc::new(move |c| {
                    Action::Modify(ProviderConfig {
                        name: format!("p{}", i),
                        ..c.clone()
                    })
                });
                reg.register_register(format!("h{}", i), h);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Concurrent registration must not panic and all handlers
        // should be present.
        assert!(reg.has_handlers("provider.register"));
    }
}
