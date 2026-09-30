//! [`ProviderRegistry`] — the named in-memory profile map.

use std::collections::HashMap;

use synthia_core::Error;

use super::enum_::ProviderProfile;
use crate::traits::ModelProvider;

/// A typed registry of named `ProviderProfile`s — the
/// lego-composition primitive that replaces the legacy
/// `WorkspaceConfig.providers` `HashMap<String, ProviderEntry>` for
/// in-memory assembly.
///
/// `WorkspaceConfig` is preserved for the on-disk
/// `.agents/config.toml` workflow; `ProviderRegistry` is the
/// pure-in-memory variant for lib consumers that want to wire
/// providers programmatically.
#[derive(Clone, Debug, Default)]
pub struct ProviderRegistry {
    profiles: HashMap<String, ProviderProfile>,
    default_name: Option<String>,
}

impl ProviderRegistry {
    /// Empty registry — callers `insert` profiles individually.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a profile under its [`ProviderProfile::name()`]. If a
    /// profile with the same name already exists it is replaced.
    pub fn insert(mut self, profile: ProviderProfile) -> Self {
        let name = profile.name().to_string();
        if self.default_name.is_none() {
            self.default_name = Some(name.clone());
        }
        self.profiles.insert(name, profile);
        self
    }

    /// Set the profile returned by [`Self::default_profile`].
    /// Idempotent — the most recent call wins.
    pub fn with_default(mut self, name: &str) -> Self {
        self.default_name = Some(name.to_string());
        self
    }

    /// Resolve the named profile (returns the typed enum, not a
    /// provider).
    pub fn get(&self, name: &str) -> Option<&ProviderProfile> {
        self.profiles.get(name)
    }

    /// Resolve the default profile (the first inserted, or whatever
    /// `with_default` last set).
    pub fn default_profile(&self) -> Option<&ProviderProfile> {
        self.default_name
            .as_ref()
            .and_then(|n| self.profiles.get(n))
    }

    /// Build the [`ModelProvider`] for the named profile.
    pub fn build(&self, name: &str) -> Result<Box<dyn ModelProvider>, Error> {
        self.get(name)
            .ok_or_else(|| Error::not_found(name))?
            .build_provider()
    }

    /// Build the default profile's [`ModelProvider`].
    pub fn build_default(&self) -> Result<Box<dyn ModelProvider>, Error> {
        let profile = self.default_profile().ok_or_else(|| {
            Error::config("ProviderRegistry: no default profile set")
        })?;
        profile.build_provider()
    }

    /// Names of every registered profile.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.profiles.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered profiles.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// True when no profile is registered.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}
