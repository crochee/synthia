use sqlx::sqlite::SqlitePoolOptions;

use crate::types::ColdEntry;

pub struct SqliteStore {
    pub(crate) pool: sqlx::SqlitePool,
}

impl SqliteStore {
    pub async fn new(database_url: &str) -> Result<Self, synthia_core::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(database_url)
            .await
            .map_err(|e| {
                synthia_core::Error::Memory(format!(
                    "failed to connect to SQLite: {}",
                    e
                ))
            })?;

        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    async fn init_schema(&self) -> Result<(), synthia_core::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cold_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                timestamp TEXT,
                importance_score REAL NOT NULL DEFAULT 0.5,
                session_id TEXT,
                summary TEXT,
                tools_used TEXT,
                outcome TEXT,
                access_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Memory(format!(
                "failed to create schema: {}",
                e
            ))
        })?;

        Ok(())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn entry_to_row(
        entry: &ColdEntry,
    ) -> (
        String,
        String,
        String,
        String,
        f64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    ) {
        let metadata =
            serde_json::to_string(&entry.metadata).unwrap_or_default();
        let summary = entry.summary.clone();
        let tools_used = entry.tools_used.as_ref().map(|v| {
            serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string())
        });
        let outcome = entry.outcome.clone();
        let created_at = entry.created_at.to_rfc3339();
        let timestamp = entry.timestamp.map(|t| t.to_rfc3339());
        (
            entry.id.clone(),
            entry.content.clone(),
            metadata,
            created_at,
            entry.importance_score,
            timestamp,
            summary,
            tools_used,
            outcome,
            entry.access_count as i64,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn row_to_entry(
        id: String,
        content: String,
        metadata: String,
        created_at: String,
        importance_score: f64,
        timestamp: Option<String>,
        summary: Option<String>,
        tools_used: Option<String>,
        outcome: Option<String>,
        access_count: i64,
    ) -> ColdEntry {
        use chrono::{DateTime, Utc};

        let parsed_metadata: serde_json::Value =
            serde_json::from_str(&metadata).unwrap_or(serde_json::Value::Null);
        let parsed_created_at = DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let parsed_timestamp = timestamp
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let parsed_tools_used = tools_used
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());

        ColdEntry {
            id,
            content,
            metadata: parsed_metadata,
            created_at: parsed_created_at,
            timestamp: parsed_timestamp,
            importance_score,
            session_id: None,
            summary,
            tools_used: parsed_tools_used,
            outcome,
            access_count: access_count as u64,
        }
    }
}
