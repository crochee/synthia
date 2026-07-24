use async_trait::async_trait;

use super::r#struct::SqliteStore;
use crate::{
    cold::store::{MemoryHit, MemoryStore, SearchQuery},
    types::ColdEntry,
};

#[async_trait]
impl MemoryStore for SqliteStore
where
    Self: Send + Sync,
{
    async fn insert(
        &self,
        entry: &ColdEntry,
    ) -> Result<(), synthia_core::Error> {
        let (
            id,
            content,
            metadata,
            created_at,
            importance_score,
            timestamp,
            summary,
            tools_used,
            outcome,
            access_count,
        ) = SqliteStore::entry_to_row(entry);
        sqlx::query(
            "INSERT OR REPLACE INTO cold_entries (id, content, metadata, created_at, importance_score, timestamp, summary, tools_used, outcome, access_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(&content).bind(&metadata).bind(&created_at).bind(importance_score).bind(&timestamp).bind(&summary).bind(&tools_used).bind(&outcome).bind(access_count)
        .execute(&self.pool).await.map_err(|e| synthia_core::Error::Memory(format!("insert failed: {}", e)))?;
        Ok(())
    }

    async fn insert_batch(
        &self,
        entries: &[&ColdEntry],
    ) -> Result<(), synthia_core::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            synthia_core::Error::Memory(format!(
                "transaction start failed: {}",
                e
            ))
        })?;
        for entry in entries {
            let (
                id,
                content,
                metadata,
                created_at,
                importance_score,
                timestamp,
                summary,
                tools_used,
                outcome,
                access_count,
            ) = SqliteStore::entry_to_row(entry);
            sqlx::query(
                "INSERT OR REPLACE INTO cold_entries (id, content, metadata, created_at, importance_score, timestamp, summary, tools_used, outcome, access_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&id).bind(&content).bind(&metadata).bind(&created_at).bind(importance_score).bind(&timestamp).bind(&summary).bind(&tools_used).bind(&outcome).bind(access_count)
            .execute(&mut *tx).await.map_err(|e| synthia_core::Error::Memory(format!("batch insert failed: {}", e)))?;
        }
        tx.commit().await.map_err(|e| {
            synthia_core::Error::Memory(format!(
                "transaction commit failed: {}",
                e
            ))
        })?;
        Ok(())
    }

    async fn get(
        &self,
        id: &str,
    ) -> Result<Option<ColdEntry>, synthia_core::Error> {
        let row: Option<(String, String, String, String, f64, Option<String>, Option<String>, Option<String>, Option<String>, i64)> = sqlx::query_as(
            "SELECT id, content, metadata, created_at, importance_score, timestamp, summary, tools_used, outcome, access_count FROM cold_entries WHERE id = ?"
        )
        .bind(id).fetch_optional(&self.pool).await.map_err(|e| synthia_core::Error::Memory(format!("get failed: {}", e)))?;
        Ok(row.map(
            |(
                id,
                content,
                metadata,
                created_at,
                importance_score,
                timestamp,
                summary,
                tools_used,
                outcome,
                access_count,
            )| {
                SqliteStore::row_to_entry(
                    id,
                    content,
                    metadata,
                    created_at,
                    importance_score,
                    timestamp,
                    summary,
                    tools_used,
                    outcome,
                    access_count,
                )
            },
        ))
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<MemoryHit>, synthia_core::Error> {
        let pattern = format!("%{}%", query.query.to_lowercase());
        let rows: Vec<(String, String, String, String, f64, Option<String>, Option<String>, Option<String>, Option<String>, i64)> = sqlx::query_as(
            "SELECT id, content, metadata, created_at, importance_score, timestamp, summary, tools_used, outcome, access_count FROM cold_entries WHERE content LIKE ? ORDER BY importance_score DESC LIMIT ?"
        )
        .bind(&pattern).bind(query.limit as i64).fetch_all(&self.pool).await.map_err(|e| synthia_core::Error::Memory(format!("search failed: {}", e)))?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    content,
                    metadata,
                    created_at,
                    importance_score,
                    timestamp,
                    summary,
                    tools_used,
                    outcome,
                    access_count,
                )| MemoryHit {
                    entry: SqliteStore::row_to_entry(
                        id,
                        content,
                        metadata,
                        created_at,
                        importance_score,
                        timestamp,
                        summary,
                        tools_used,
                        outcome,
                        access_count,
                    ),
                    score: importance_score,
                },
            )
            .collect())
    }

    async fn delete(&self, id: &str) -> Result<(), synthia_core::Error> {
        sqlx::query("DELETE FROM cold_entries WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                synthia_core::Error::Memory(format!("delete failed: {}", e))
            })?;
        Ok(())
    }

    async fn delete_batch(
        &self,
        ids: &[&str],
    ) -> Result<(), synthia_core::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            synthia_core::Error::Memory(format!(
                "transaction start failed: {}",
                e
            ))
        })?;
        for id in ids {
            sqlx::query("DELETE FROM cold_entries WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    synthia_core::Error::Memory(format!(
                        "batch delete failed: {}",
                        e
                    ))
                })?;
        }
        tx.commit().await.map_err(|e| {
            synthia_core::Error::Memory(format!(
                "transaction commit failed: {}",
                e
            ))
        })?;
        Ok(())
    }
}
