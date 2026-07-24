//! Errors that can occur during plugin loading.

/// Errors that can occur during plugin loading.
#[derive(Debug, thiserror::Error)]
pub enum PluginLoaderError {
    #[error("Failed to discover plugins: {0}")]
    DiscoveryFailed(String),

    #[error("Failed to load hooks: {0}")]
    HookLoadFailed(String),

    #[error("Cannot find HOME environment variable")]
    HomeDirectoryNotFound,
}
