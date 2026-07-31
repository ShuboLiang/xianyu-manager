//! SQLite 仓储实现：标签持久化。
//! 连接时自动创建数据目录与表结构（IF NOT EXISTS），无需外部迁移工具。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
use sqlx::Row;

use crate::domain::error::DomainError;
use crate::domain::repository::TagRepository;
use crate::domain::tag::{NewTag, Tag, TagName};

pub struct SqliteTagRepository {
    pool: SqlitePool,
}

impl SqliteTagRepository {
    /// 打开（必要时创建）数据库并初始化表结构
    pub async fn connect(path: &str) -> Result<Self, DomainError> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
            }
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(to_infra)?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tags (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                enabled    INTEGER NOT NULL DEFAULT 1,
                remark     TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(to_infra)?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl TagRepository for SqliteTagRepository {
    async fn create(&self, tag: &NewTag) -> Result<Tag, DomainError> {
        let now = crate::domain::crawl_task::now_unix();
        let result = sqlx::query(
            "INSERT INTO tags (name, enabled, remark, created_at, updated_at)
             VALUES (?, 1, ?, ?, ?)",
        )
        .bind(tag.name.as_str())
        .bind(&tag.remark)
        .bind(now as i64)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;

        Ok(Tag {
            id: result.last_insert_rowid(),
            name: tag.name.clone(),
            enabled: true,
            remark: tag.remark.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    async fn find(&self, id: i64) -> Result<Option<Tag>, DomainError> {
        let row = sqlx::query("SELECT * FROM tags WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_tag).transpose()
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Tag>, DomainError> {
        let row = sqlx::query("SELECT * FROM tags WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_tag).transpose()
    }

    async fn list(&self) -> Result<Vec<Tag>, DomainError> {
        let rows = sqlx::query("SELECT * FROM tags ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        rows.iter().map(row_to_tag).collect()
    }

    async fn update(&self, tag: &Tag) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE tags SET name = ?, enabled = ?, remark = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(tag.name.as_str())
        .bind(tag.enabled)
        .bind(&tag.remark)
        .bind(tag.updated_at as i64)
        .bind(tag.id)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_tag(row: &SqliteRow) -> Result<Tag, DomainError> {
    Ok(Tag {
        id: row.get("id"),
        name: TagName::new(row.get::<String, _>("name"))?,
        enabled: row.get("enabled"),
        remark: row.get("remark"),
        created_at: row.get::<i64, _>("created_at") as u64,
        updated_at: row.get::<i64, _>("updated_at") as u64,
    })
}

fn to_infra(e: sqlx::Error) -> DomainError {
    DomainError::Infrastructure(format!("sqlite: {e}"))
}
