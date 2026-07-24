//! All write-path operations on [`super::core::ColdMemory`].

use crate::types::ColdEntry;

impl super::core::ColdMemory {
    /// Append a new entry to cold memory. Triggers eviction when the
    /// total entry count exceeds `max_entries`.
    pub async fn append(
        &self,
        entry: ColdEntry,
    ) -> Result<(), synthia_core::Error> {
        let metadata_str =
            serde_json::to_string(&entry.metadata).map_err(|e| {
                synthia_core::Error::Parse(format!(
                    "failed to serialize metadata: {}",
                    e
                ))
            })?;

        let created_at_str = entry.created_at.to_rfc3339();

        // Insert into FTS5 table
        sqlx::query(
            r#"
            INSERT INTO cold_entries_fts (entry_id, content) VALUES (?, ?)
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.content)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to insert into FTS5: {}",
                e
            )))
        })?;

        // Insert metadata
        sqlx::query(
            r#"
            INSERT INTO cold_entries_meta (entry_id, metadata, created_at, importance_score, access_count) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry.id)
        .bind(&metadata_str)
        .bind(&created_at_str)
        .bind(entry.importance_score)
        .bind(entry.access_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!("Failed to insert metadata: {}", e)))
        })?;

        // Check if we need to evict old entries
        self.evict_low_importance_entries().await?;

        Ok(())
    }

    /// Increment access count and boost importance score for a specific entry.
    pub async fn increment_access(
        &self,
        entry_id: &str,
    ) -> Result<(), synthia_core::Error> {
        sqlx::query(
            r#"
            UPDATE cold_entries_meta
            SET access_count = access_count + 1,
                importance_score = MIN(1.0, importance_score + 0.1 * (1.0 - importance_score))
            WHERE entry_id = ?
            "#,
        )
        .bind(entry_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to increment access count: {}",
                e
            )))
        })?;
        Ok(())
    }

    /// Decay importance scores for all entries based on decay factor.
    pub async fn decay_importance_scores(
        &self,
    ) -> Result<(), synthia_core::Error> {
        sqlx::query(
            r#"
            UPDATE cold_entries_meta
            SET importance_score = MAX(0.01, importance_score * ?)
            "#,
        )
        .bind(self.importance_decay_factor)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to decay importance scores: {}",
                e
            )))
        })?;
        Ok(())
    }

    /// Evict entries with lowest importance scores if we exceed
    /// `max_entries`. Returns the number of entries removed.
    ///
    /// Visible to `super` only (called from [`super::core::ColdMemory::append`]).
    pub(super) async fn evict_low_importance_entries(
        &self,
    ) -> Result<usize, synthia_core::Error> {
        let current_count = self.entry_count().await?;

        if current_count <= self.max_entries {
            return Ok(0);
        }

        let entries_to_remove = current_count - self.max_entries;

        // Find and delete entries with lowest importance scores
        let ids_to_delete: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT entry_id
            FROM cold_entries_meta
            ORDER BY importance_score ASC, created_at ASC
            LIMIT ?
            "#,
        )
        .bind(entries_to_remove as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to select entries for eviction: {}",
                e
            )))
        })?;

        let removed = self
            .delete_entries(
                &ids_to_delete.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )
            .await?;

        Ok(removed)
    }
}
