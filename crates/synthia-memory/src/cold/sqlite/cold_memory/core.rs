//! The [`ColdMemory`] struct and its constructors/builders.

use std::{path::PathBuf, time::Duration};

use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

use super::schema::init_schema;
use crate::types::RetrievalMode;

/// SQLite-backed cold memory with FTS5 + importance scoring.
///
/// Fields are `pub(super)` so the sibling [`super`] submodules
/// (`mutate`, `search`, `admin`, `schema`) can access them.
pub struct ColdMemory {
    pub(super) pool: SqlitePool,
    pub(super) _path: PathBuf,
    pub(super) default_mode: RetrievalMode,
    pub(super) max_entries: usize,
    pub(super) importance_decay_factor: f64,
}

impl ColdMemory {
    /// Open or create a file-backed SQLite database at
    /// `storage_path/cold_memory.db`. Runs schema migration.
    pub async fn new(
        storage_path: PathBuf,
    ) -> Result<Self, synthia_core::Error> {
        let db_url = format!(
            "sqlite://{}",
            storage_path.join("cold_memory.db").to_string_lossy()
        );

        let pool = SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs(5))
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| {
                synthia_core::Error::Io(std::io::Error::other(format!(
                    "Failed to connect to SQLite: {}",
                    e
                )))
            })?;

        // Create tables if they don't exist
        init_schema(&pool).await?;

        Ok(Self {
            pool,
            _path: storage_path,
            default_mode: RetrievalMode::Bm25,
            max_entries: 1000,
            importance_decay_factor: 0.99,
        })
    }

    /// Open an in-memory SQLite database (for tests). Same schema
    /// as [`ColdMemory::new`].
    pub async fn new_in_memory() -> Result<Self, synthia_core::Error> {
        let pool =
            SqlitePool::connect("sqlite::memory:").await.map_err(|e| {
                synthia_core::Error::Io(std::io::Error::other(format!(
                    "Failed to connect to in-memory SQLite: {}",
                    e
                )))
            })?;

        init_schema(&pool).await?;

        Ok(Self {
            pool,
            _path: PathBuf::from(":memory:"),
            default_mode: RetrievalMode::Bm25,
            max_entries: 1000,
            importance_decay_factor: 0.99,
        })
    }

    /// Set the maximum number of entries kept before eviction kicks in.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Set the importance-score decay factor (0.01..=1.0). Values
    /// outside this range are clamped.
    pub fn with_importance_decay_factor(mut self, factor: f64) -> Self {
        self.importance_decay_factor = factor.clamp(0.01, 1.0);
        self
    }
}
