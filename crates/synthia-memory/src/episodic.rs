use std::{path::PathBuf, time::Duration};

use sqlx::SqlitePool;

use crate::types::EpisodicSkill;

pub struct EpisodicMemory {
    pool: SqlitePool,
    _path: PathBuf,
}

impl EpisodicMemory {
    pub async fn new(
        storage_path: PathBuf,
    ) -> Result<Self, synthia_core::Error> {
        let db_url = format!(
            "sqlite://{}",
            storage_path.join("episodic_memory.db").to_string_lossy()
        );

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .acquire_timeout(Duration::from_secs(5))
            .max_connections(3)
            .connect(&db_url)
            .await
            .map_err(|e| {
                synthia_core::Error::Io(std::io::Error::other(format!(
                    "Failed to connect to SQLite: {}",
                    e
                )))
            })?;

        Self::init_schema(&pool).await?;

        Ok(Self {
            pool,
            _path: storage_path,
        })
    }

    /// Create an in-memory instance for testing.
    pub async fn new_in_memory() -> Result<Self, synthia_core::Error> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.map_err(
            |e| {
                synthia_core::Error::Io(std::io::Error::other(format!(
                    "Failed to connect to in-memory SQLite: {}",
                    e
                )))
            },
        )?;

        Self::init_schema(&pool).await?;

        Ok(Self {
            pool,
            _path: PathBuf::from(":memory:"),
        })
    }

    async fn init_schema(pool: &SqlitePool) -> Result<(), synthia_core::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS episodic_skills (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_hint TEXT NOT NULL,
                skill_content TEXT NOT NULL,
                success_rate REAL NOT NULL,
                used_at TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to create episodic_skills table: {}",
                e
            )))
        })?;

        // Index for faster lookups by task_hint
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_episodic_task_hint
            ON episodic_skills(task_hint)
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to create episodic index: {}",
                e
            )))
        })?;

        Ok(())
    }

    pub async fn save(
        &self,
        skill: EpisodicSkill,
    ) -> Result<(), synthia_core::Error> {
        let used_at_str = skill.used_at.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO episodic_skills (task_hint, skill_content, success_rate, used_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&skill.task_hint)
        .bind(&skill.skill_content)
        .bind(skill.success_rate)
        .bind(&used_at_str)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to save episodic skill: {}",
                e
            )))
        })?;

        Ok(())
    }

    pub async fn load(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicSkill>, synthia_core::Error> {
        let rows: Vec<(String, String, f64, String)> = sqlx::query_as(
            r#"
            SELECT task_hint, skill_content, success_rate, used_at
            FROM episodic_skills
            WHERE task_hint = ?
            ORDER BY used_at DESC
            "#,
        )
        .bind(task_hint)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to load episodic skills: {}",
                e
            )))
        })?;

        let skills: Vec<EpisodicSkill> = rows
            .into_iter()
            .map(|(task_hint, skill_content, success_rate, used_at)| {
                EpisodicSkill {
                    task_hint,
                    skill_content,
                    success_rate,
                    used_at: chrono::DateTime::parse_from_rfc3339(&used_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                }
            })
            .collect();

        Ok(skills)
    }

    /// Load all episodic skills, ordered by most recent first.
    pub async fn load_all(
        &self,
        limit: usize,
    ) -> Result<Vec<EpisodicSkill>, synthia_core::Error> {
        let rows: Vec<(String, String, f64, String)> = sqlx::query_as(
            r#"
            SELECT task_hint, skill_content, success_rate, used_at
            FROM episodic_skills
            ORDER BY used_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Failed to load all episodic skills: {}",
                e
            )))
        })?;

        let skills: Vec<EpisodicSkill> = rows
            .into_iter()
            .map(|(task_hint, skill_content, success_rate, used_at)| {
                EpisodicSkill {
                    task_hint,
                    skill_content,
                    success_rate,
                    used_at: chrono::DateTime::parse_from_rfc3339(&used_at)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                }
            })
            .collect();

        Ok(skills)
    }
}

impl Default for EpisodicMemory {
    fn default() -> Self {
        // Default is not available for async initialization; this is a stub
        // that will panic if used without proper async construction.
        panic!(
            "EpisodicMemory::default() is not supported. Use EpisodicMemory::new() or EpisodicMemory::new_in_memory() instead."
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[tokio::test]
    async fn test_save_and_load_episodic() {
        let mem = EpisodicMemory::new_in_memory().await.unwrap();
        let skill = EpisodicSkill {
            task_hint: "refactoring".to_string(),
            skill_content: "extract method".to_string(),
            success_rate: 0.9,
            used_at: Utc::now(),
        };
        mem.save(skill).await.unwrap();
        let skills = mem.load("refactoring").await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill_content, "extract method");
    }

    #[tokio::test]
    async fn test_load_nonexistent_returns_empty() {
        let mem = EpisodicMemory::new_in_memory().await.unwrap();
        let skills = mem.load("nonexistent").await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_saves_same_hint() {
        let mem = EpisodicMemory::new_in_memory().await.unwrap();
        mem.save(EpisodicSkill {
            task_hint: "coding".to_string(),
            skill_content: "first attempt".to_string(),
            success_rate: 0.5,
            used_at: Utc::now(),
        })
        .await
        .unwrap();
        mem.save(EpisodicSkill {
            task_hint: "coding".to_string(),
            skill_content: "second attempt".to_string(),
            success_rate: 0.8,
            used_at: Utc::now(),
        })
        .await
        .unwrap();

        let skills = mem.load("coding").await.unwrap();
        assert_eq!(skills.len(), 2);
        // Most recent first
        assert_eq!(skills[0].skill_content, "second attempt");
    }
}
