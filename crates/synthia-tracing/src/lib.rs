//! Synthia distributed tracing and structured logging
//!
//! This module provides:
//! - Structured logging with tracing
//! - OpenTelemetry integration for distributed tracing
//! - Convenient macros for adding trace context

use std::{env, sync::Once};

use thiserror::Error;
use tracing::info;
use tracing_subscriber::{
    EnvFilter,
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static INIT: Once = Once::new();

#[derive(Debug, Error)]
pub enum TracingError {
    #[error("Tracing already initialized")]
    AlreadyInitialized,
    #[error("Tracing not initialized")]
    NotInitialized,
    #[error("Invalid trace context: {0}")]
    TraceContext(String),
}

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub service_name: String,
    pub log_level: String,
    pub otlp_endpoint: Option<String>,
    pub service_version: Option<String>,
    pub environment: Option<String>,
}

impl TracingConfig {
    pub fn from_env() -> Self {
        Self {
            service_name: env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "synthia".to_string()),
            log_level: env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".to_string()),
            otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            service_version: env::var("SERVICE_VERSION").ok(),
            environment: env::var("ENVIRONMENT").ok(),
        }
    }

    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        self.service_version = Some(version.into());
        self
    }

    pub fn with_environment(mut self, env: impl Into<String>) -> Self {
        self.environment = Some(env.into());
        self
    }
}

pub fn init_tracing(config: TracingConfig) -> Result<(), TracingError> {
    let result: Result<(), TracingError> = Ok(());

    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();

        info!(
            service_name = %config.service_name,
            log_level = %config.log_level,
            "Tracing initialized"
        );

        if config.otlp_endpoint.is_some() {
            // OTLP endpoint configured but OpenTelemetry not yet implemented
            // TODO: Implement OpenTelemetry integration when dependencies are available
        }

        if let Some(ref version) = config.service_version {
            info!(service_version = %version, "Service version set");
        }

        if let Some(ref env) = config.environment {
            info!(environment = %env, "Environment set");
        }
    });

    result
}

pub fn shutdown() {
    info!("Tracing shutdown requested");
}
