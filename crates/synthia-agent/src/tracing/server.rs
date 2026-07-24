//! The [`MetricsServer`] — Prometheus scrape endpoint.
//!
//! ## Two implementations
//!
//! - With `--features observability` (the real one): uses
//!   `metrics-exporter-prometheus` to install the global
//!   recorder, spawns a tokio TCP listener on
//!   `0.0.0.0:{metrics_port}`, and serves the
//!   `text/plain; version=0.0.4` Prometheus body on every
//!   accepted connection. The `Drop` impl signals a
//!   oneshot shutdown channel so `stop()` can wait for the
//!   task to finish.
//! - Without the feature (stub): `start` / `stop` are
//!   no-ops that log a warning; `port` returns 0.
//!
//! In both cases, the `MetricsServer` value is returned by
//! `start` so the caller can later `stop().await`.

#[cfg(feature = "observability")]
mod server_impl {
    use std::sync::atomic::Ordering;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::oneshot,
    };
    use tracing::{info, warn};

    use super::super::{
        config::ObservabilityConfig,
        flags::RECORDER_INSTALLED,
    };

    /// A Prometheus metrics server that serves scrape requests on a configurable
    /// TCP port.
    ///
    /// The server runs in a background tokio task. Dropping the `MetricsServer`
    /// signals the task to shut down.
    pub struct MetricsServer {
        port: u16,
        shutdown_tx: Option<oneshot::Sender<()>>,
        handle: Option<tokio::task::JoinHandle<()>>,
    }

    impl MetricsServer {
        /// Start the Prometheus metrics server on the configured port.
        ///
        /// This installs the global Prometheus recorder and spawns a background
        /// HTTP listener. Returns immediately after spawning the task.
        pub async fn start(
            config: &ObservabilityConfig,
        ) -> std::io::Result<Self> {
            let port = config.metrics_port;

            info!(%port, "Starting Prometheus metrics server");

            // Install the global recorder for the metrics crate.
            let recorder =
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .build_recorder();
            let recorder_handle = recorder.handle();

            if !RECORDER_INSTALLED.swap(true, Ordering::SeqCst) {
                metrics::set_global_recorder(recorder).map_err(|e| {
                    std::io::Error::other(format!(
                        "Failed to install metrics recorder: {e}"
                    ))
                })?;
            }

            let (tx, rx) = oneshot::channel::<()>();

            let handle = tokio::spawn(async move {
                Self::run_server(port, rx, recorder_handle).await;
            });

            Ok(Self {
                port,
                shutdown_tx: Some(tx),
                handle: Some(handle),
            })
        }

        /// Stop the metrics server gracefully.
        pub async fn stop(mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.await;
            }
            info!(port = %self.port, "Prometheus metrics server stopped");
        }

        async fn run_server(
            port: u16,
            mut shutdown: oneshot::Receiver<()>,
            recorder_handle: metrics_exporter_prometheus::PrometheusHandle,
        ) {
            let listener = match tokio::net::TcpListener::bind(format!(
                "0.0.0.0:{port}"
            ))
            .await
            {
                Ok(l) => l,
                Err(e) => {
                    warn!(%e, %port, "Failed to bind Prometheus metrics listener");
                    return;
                }
            };

            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown => {
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((mut stream, _addr)) => {
                                // Read the HTTP request (we ignore it, just serve metrics)
                                let mut buf = [0u8; 1024];
                                let _ = stream.read(&mut buf).await;

                                // Serve Prometheus metrics from the shared handle
                                let metrics_body = recorder_handle.render();
                                let response = format!(
                                    "HTTP/1.1 200 OK\r\n\
                                     Content-Type: text/plain; version=0.0.4\r\n\
                                     Content-Length: {}\r\n\
                                     Connection: close\r\n\r\n\
                                     {}",
                                    metrics_body.len(),
                                    metrics_body
                                );
                                let _ = stream.write_all(response.as_bytes()).await;
                                let _ = stream.flush().await;
                            }
                            Err(e) => {
                                warn!(%e, "Error accepting Prometheus scrape connection");
                            }
                        }
                    }
                }
            }
        }

        /// Returns the port this server is listening on.
        pub fn port(&self) -> u16 {
            self.port
        }
    }

    impl Drop for MetricsServer {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }
}

#[cfg(feature = "observability")]
pub use server_impl::MetricsServer;

#[cfg(not(feature = "observability"))]
/// Stub implementation when observability feature is not enabled.
pub struct MetricsServer;

#[cfg(not(feature = "observability"))]
impl MetricsServer {
    /// No-op when observability feature is not enabled.
    pub async fn start(
        _config: &super::config::ObservabilityConfig,
    ) -> std::io::Result<Self> {
        tracing::info!(
            "MetricsServer::start called but observability feature is not enabled"
        );
        Ok(MetricsServer)
    }

    /// No-op when observability feature is not enabled.
    pub async fn stop(self) {
        tracing::info!(
            "MetricsServer::stop called but observability feature is not enabled"
        );
    }

    /// Returns default port 0 when observability feature is not enabled.
    pub fn port(&self) -> u16 {
        0
    }
}
