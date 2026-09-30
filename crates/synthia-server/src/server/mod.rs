mod router;

pub use router::{BootedServer, boot_server, create_router};

#[cfg(test)]
mod tests {
    use super::*;

    /// Router assembly over a hermetic `AppState`.
    ///
    /// The production `create_server` loads the workspace configuration and
    /// builds a real provider, so it fails without ambient provider
    /// credentials by design. This test pins that the router *assembles*,
    /// which has nothing to do with which provider is configured, so it
    /// builds the router over the in-memory `for_test` state instead.
    #[tokio::test]
    async fn test_server_creation() {
        let dir = tempfile::tempdir().expect("temp workspace");
        let sessions = synthia::session::manager::SessionRegistry::new(
            dir.path().join("sessions"),
        );
        let state = std::sync::Arc::new(
            crate::state::AppState::for_test(
                sessions,
                dir.path().to_path_buf(),
            )
            .await,
        );
        let router = create_router(state).await;
        assert!(std::mem::size_of_val(&router) > 0);
    }
}
