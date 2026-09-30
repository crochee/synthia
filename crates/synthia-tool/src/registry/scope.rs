//! Scoped registration: tokens, the auto-unregistering
//! `RegistrationScope`, and the registry methods that manage them.

use std::sync::Arc;

use super::{ToolEntry, ToolRegistry};

/// Registration token for unregistration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationToken(pub u64);

impl ToolRegistry {
    /// Remove all `ProviderEntry` instances that were registered with
    /// the given token.
    pub fn unregister_by_token(&self, token: RegistrationToken) {
        let mut inner = self.inner.write();
        let mut removed_count = 0usize;

        // Drain entries matching the token from each tool bucket.
        inner.tools.retain(|_name, entries| {
            let before = entries.len();
            entries.retain(|e| e.provider_token != token);
            let removed = before - entries.len();
            removed_count += removed;
            !entries.is_empty()
        });

        if removed_count > 0 {
            self.version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tracing::info!(
                token = token.0,
                count = removed_count,
                "unregistered tools by token"
            );
        } else {
            tracing::debug!(
                token = token.0,
                "unregister_by_token: no matching entries"
            );
        }
    }

    /// Register a single [`ToolEntry`] directly and return a
    /// `RegistrationScope` that auto-unregisters on drop.
    pub async fn register_scoped_arc(
        self: &Arc<Self>,
        entry: ToolEntry,
    ) -> RegistrationScope {
        let mut inner = self.inner.write();
        let token = self.register_entry_inner(&mut inner, entry);
        if token.is_some() {
            self.version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let token = token.unwrap_or_else(|| {
            RegistrationToken(inner.next_registration.wrapping_sub(1))
        });
        RegistrationScope {
            token,
            registry: Arc::downgrade(self),
        }
    }

    /// Create an empty session scope with a fresh registration token.
    ///
    /// Unlike [`register_scoped_arc`](Self::register_scoped_arc), this does not
    /// register any tools immediately. The returned scope carries a
    /// unique [`RegistrationToken`] that future code can associate with
    /// tools registered during a session. When the scope is dropped, all
    /// tools registered under its token are automatically unregistered
    /// from the registry (or the cleanup is a no-op if the registry has
    /// already been dropped).
    pub fn create_session_scope(self: &Arc<Self>) -> RegistrationScope {
        let token = {
            let mut inner = self.inner.write();
            let token = RegistrationToken(inner.next_registration);
            inner.next_registration += 1;
            token
        };

        RegistrationScope {
            token,
            registry: Arc::downgrade(self),
        }
    }
}

/// RAII scope that automatically unregisters tools when dropped.
///
/// Created by [`ToolRegistry::register_scoped_arc`] or
/// [`ToolRegistry::create_session_scope`]. When the scope
/// goes out of scope, all tools that were registered under its token
/// are removed from the registry. If the registry itself has already
/// been dropped, cleanup is a no-op.
#[derive(Debug)]
pub struct RegistrationScope {
    token: RegistrationToken,
    registry: std::sync::Weak<ToolRegistry>,
}

impl RegistrationScope {
    /// The registration token for this scope.
    pub fn token(&self) -> &RegistrationToken {
        &self.token
    }

    /// Perform cleanup: upgrade `Weak` → `Arc`, call
    /// `unregister_by_token`.
    fn cleanup(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            tracing::info!(
                token = self.token.0,
                "RegistrationScope dropped — unregistering tools"
            );
            registry.unregister_by_token(self.token.clone());
        } else {
            tracing::debug!(
                token = self.token.0,
                "RegistrationScope dropped but registry already gone — no-op"
            );
        }
    }
}

impl Drop for RegistrationScope {
    fn drop(&mut self) {
        self.cleanup();
    }
}
