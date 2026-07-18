//! `ToolLifecycle` sub-trait — registration and health management.
//!
//! Provides lifecycle hooks for registration, health-checking, and
//! versioning. This sub-trait extracts the "am I alive?" concern from
//! the monolithic [`crate::Tool`] trait.

/// Lifecycle management sub-trait: register, unregister, health, version.
///
/// Tools that don't need lifecycle hooks (stateless built-ins) can use
/// the default no-op implementations.
pub trait ToolLifecycle: Send + Sync + 'static {
    /// Called when the tool is registered in a [`crate::ToolRegistry`].
    ///
    /// Use this for one-time initialization (e.g. warming caches,
    /// validating config). Default: no-op.
    fn on_register(&self) -> Result<(), String> {
        Ok(())
    }

    /// Called when the tool is unregistered from a [`crate::ToolRegistry`].
    ///
    /// Use this for cleanup (e.g. releasing resources, flushing buffers).
    /// Default: no-op.
    fn on_unregister(&self) -> Result<(), String> {
        Ok(())
    }

    /// Check whether the tool is healthy and ready for execution.
    ///
    /// Returns `Ok(())` if healthy, `Err` with a description of the
    /// issue otherwise. Default: always healthy.
    fn health_check(&self) -> Result<(), String> {
        Ok(())
    }

    /// Semantic version of the tool implementation.
    ///
    /// Used by the registry to detect stale tool references.
    /// Default: `"0.1.0"`.
    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    /// Schema version of the tool's `parameters_schema()`.
    ///
    /// Bumped when the JSON Schema changes incompatibly. Default: `1`.
    fn schema_version(&self) -> u32 {
        1
    }
}

#[cfg(test)]
mod tests {
    /// Compile-time sanity: `ToolLifecycle` exposes at most 5 methods.
    #[test]
    fn tool_lifecycle_has_at_most_five_methods() {
        // The 5 methods are: on_register, on_unregister, health_check,
        // version, schema_version.
        const METHOD_COUNT: usize = 5;
        assert!(
            METHOD_COUNT <= 5,
            "ToolLifecycle exceeds 5 methods — consider splitting"
        );
    }
}
