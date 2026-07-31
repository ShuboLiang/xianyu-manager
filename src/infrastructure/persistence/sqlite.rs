//! SQLite 仓储实现：标签与待爬取商品的持久化。
//! 连接时自动创建数据目录与表结构（IF NOT EXISTS），无需外部迁移工具。
//! 各仓储共享同一个连接池，由 `connect` 在启动时创建。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteRow};
use sqlx::Row;

use crate::domain::crawl_queue::{CrawlEntry, CrawlQueue, EntryStatus, QueueStatus};
use crate::domain::error::DomainError;
use crate::domain::product::{NewProduct, Product, ProductName};
use crate::domain::repository::{ProductRepository, QueueRepository, TagRepository};
use crate::domain::tag::{NewTag, Tag, TagName};

/// 打开（必要时创建）数据库，初始化全部表结构，返回共享连接池。
/// 开启外键约束：删除商品或标签时自动清理 product_tags 关联（ON DELETE CASCADE）。
pub async fn connect(path: &str) -> Result<SqlitePool, DomainError> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        }
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .pragma("foreign_keys", "ON");
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

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS products (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT NOT NULL UNIQUE,
            remark          TEXT,
            median_price    REAL,
            avg_price       REAL,
            crawled_count   INTEGER,
            last_crawled_at INTEGER,
            recycle_price   REAL,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .map_err(to_infra)?;

    // 商品 ↔ 标签 多对多关联表
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS product_tags (
            product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
            tag_id     INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (product_id, tag_id)
        )",
    )
    .execute(&pool)
    .await
    .map_err(to_infra)?;

    // 抓取队列与队列条目（条目不带外键：商品删除后条目保留 → skipped）
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS crawl_queues (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            status        TEXT NOT NULL,
            interval_secs INTEGER NOT NULL,
            created_at    INTEGER NOT NULL,
            finished_at   INTEGER
        )",
    )
    .execute(&pool)
    .await
    .map_err(to_infra)?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS crawl_entries (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            queue_id   INTEGER NOT NULL,
            product_id INTEGER NOT NULL,
            status     TEXT NOT NULL,
            error      TEXT,
            crawled_at INTEGER
        )",
    )
    .execute(&pool)
    .await
    .map_err(to_infra)?;

    Ok(pool)
}

// ---------- 标签 ----------

pub struct SqliteTagRepository {
    pool: SqlitePool,
}

impl SqliteTagRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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

// ---------- 待爬取商品 ----------

pub struct SqliteProductRepository {
    pool: SqlitePool,
}

impl SqliteProductRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProductRepository for SqliteProductRepository {
    async fn create(&self, product: &NewProduct) -> Result<Product, DomainError> {
        let now = crate::domain::crawl_task::now_unix();
        let result = sqlx::query(
            "INSERT INTO products (name, remark, created_at, updated_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(product.name.as_str())
        .bind(&product.remark)
        .bind(now as i64)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        let id = result.last_insert_rowid();

        self.save_tag_links(id, &product.tag_ids).await?;

        Ok(Product {
            id,
            name: product.name.clone(),
            tag_ids: product.tag_ids.clone(),
            remark: product.remark.clone(),
            median_price: None,
            avg_price: None,
            crawled_count: None,
            last_crawled_at: None,
            recycle_price: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn find(&self, id: i64) -> Result<Option<Product>, DomainError> {
        let row = sqlx::query(&format!("{PRODUCT_SELECT} WHERE p.id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_product).transpose()
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Product>, DomainError> {
        let row = sqlx::query(&format!("{PRODUCT_SELECT} WHERE p.name = ?"))
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_product).transpose()
    }

    async fn list(&self) -> Result<Vec<Product>, DomainError> {
        let rows = sqlx::query(&format!("{PRODUCT_SELECT} ORDER BY p.created_at ASC"))
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        rows.iter().map(row_to_product).collect()
    }

    async fn list_by_tag(&self, tag_id: i64) -> Result<Vec<Product>, DomainError> {
        let rows = sqlx::query(&format!(
            "{PRODUCT_SELECT} WHERE EXISTS (
                SELECT 1 FROM product_tags pt WHERE pt.product_id = p.id AND pt.tag_id = ?
            ) ORDER BY p.created_at ASC"
        ))
        .bind(tag_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_infra)?;
        rows.iter().map(row_to_product).collect()
    }

    async fn update(&self, product: &Product) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE products SET name = ?, remark = ?,
                median_price = ?, avg_price = ?, crawled_count = ?,
                last_crawled_at = ?, recycle_price = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(product.name.as_str())
        .bind(&product.remark)
        .bind(product.median_price)
        .bind(product.avg_price)
        .bind(product.crawled_count.map(|c| c as i64))
        .bind(product.last_crawled_at.map(|t| t as i64))
        .bind(product.recycle_price)
        .bind(product.updated_at as i64)
        .bind(product.id)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;

        // 关联全量重建（商品-标签数量很小，先删后插最简单可靠）
        sqlx::query("DELETE FROM product_tags WHERE product_id = ?")
            .bind(product.id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        self.save_tag_links(product.id, &product.tag_ids).await
    }

    async fn delete(&self, id: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM products WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }
}

impl SqliteProductRepository {
    async fn save_tag_links(&self, product_id: i64, tag_ids: &[i64]) -> Result<(), DomainError> {
        for tag_id in tag_ids {
            sqlx::query("INSERT OR IGNORE INTO product_tags (product_id, tag_id) VALUES (?, ?)")
                .bind(product_id)
                .bind(tag_id)
                .execute(&self.pool)
                .await
                .map_err(to_infra)?;
        }
        Ok(())
    }
}

/// 商品基础查询：携带该商品全部标签 id（逗号拼接，无标签时为 NULL）
const PRODUCT_SELECT: &str = "SELECT p.*, (
        SELECT GROUP_CONCAT(pt.tag_id) FROM product_tags pt WHERE pt.product_id = p.id
    ) AS tag_ids
    FROM products p";

fn row_to_product(row: &SqliteRow) -> Result<Product, DomainError> {
    let tag_ids = row
        .get::<Option<String>, _>("tag_ids")
        .map(|s| s.split(',').filter_map(|p| p.parse().ok()).collect())
        .unwrap_or_default();
    Ok(Product {
        id: row.get("id"),
        name: ProductName::new(row.get::<String, _>("name"))?,
        tag_ids,
        remark: row.get("remark"),
        median_price: row.get("median_price"),
        avg_price: row.get("avg_price"),
        crawled_count: row.get::<Option<i64>, _>("crawled_count").map(|c| c as u32),
        last_crawled_at: row.get::<Option<i64>, _>("last_crawled_at").map(|t| t as u64),
        recycle_price: row.get("recycle_price"),
        created_at: row.get::<i64, _>("created_at") as u64,
        updated_at: row.get::<i64, _>("updated_at") as u64,
    })
}

fn to_infra(e: sqlx::Error) -> DomainError {
    DomainError::Infrastructure(format!("sqlite: {e}"))
}

// ---------- 抓取队列 ----------

pub struct SqliteQueueRepository {
    pool: SqlitePool,
}

impl SqliteQueueRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueueRepository for SqliteQueueRepository {
    async fn create_queue(&self, queue: &CrawlQueue) -> Result<CrawlQueue, DomainError> {
        let result = sqlx::query(
            "INSERT INTO crawl_queues (status, interval_secs, created_at, finished_at)
             VALUES (?, ?, ?, NULL)",
        )
        .bind(queue.status.as_str())
        .bind(queue.interval_secs as i64)
        .bind(queue.created_at as i64)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        let mut q = queue.clone();
        q.id = result.last_insert_rowid();
        Ok(q)
    }

    async fn add_entries(&self, queue_id: i64, product_ids: &[i64]) -> Result<(), DomainError> {
        for product_id in product_ids {
            sqlx::query(
                "INSERT INTO crawl_entries (queue_id, product_id, status, error, crawled_at)
                 VALUES (?, ?, 'pending', NULL, NULL)",
            )
            .bind(queue_id)
            .bind(product_id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        }
        Ok(())
    }

    async fn find_queue(&self, id: i64) -> Result<Option<CrawlQueue>, DomainError> {
        let row = sqlx::query("SELECT * FROM crawl_queues WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_queue).transpose()
    }

    async fn list_queues(&self) -> Result<Vec<CrawlQueue>, DomainError> {
        let rows = sqlx::query("SELECT * FROM crawl_queues ORDER BY created_at DESC, id DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        rows.iter().map(row_to_queue).collect()
    }

    async fn update_queue(&self, queue: &CrawlQueue) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE crawl_queues SET status = ?, interval_secs = ?, finished_at = ?
             WHERE id = ?",
        )
        .bind(queue.status.as_str())
        .bind(queue.interval_secs as i64)
        .bind(queue.finished_at.map(|t| t as i64))
        .bind(queue.id)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(())
    }

    async fn list_entries(&self, queue_id: i64) -> Result<Vec<CrawlEntry>, DomainError> {
        let rows = sqlx::query("SELECT * FROM crawl_entries WHERE queue_id = ? ORDER BY id ASC")
            .bind(queue_id)
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        rows.iter().map(row_to_entry).collect()
    }

    async fn next_pending_entry(&self, queue_id: i64) -> Result<Option<CrawlEntry>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM crawl_entries WHERE queue_id = ? AND status = 'pending'
             ORDER BY id ASC LIMIT 1",
        )
        .bind(queue_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_infra)?;
        row.as_ref().map(row_to_entry).transpose()
    }

    async fn update_entry(&self, entry: &CrawlEntry) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE crawl_entries SET status = ?, error = ?, crawled_at = ? WHERE id = ?",
        )
        .bind(entry.status.as_str())
        .bind(&entry.error)
        .bind(entry.crawled_at.map(|t| t as i64))
        .bind(entry.id)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(())
    }

    async fn queued_product_ids(&self) -> Result<Vec<i64>, DomainError> {
        let rows = sqlx::query(
            "SELECT DISTINCT e.product_id FROM crawl_entries e
             JOIN crawl_queues q ON q.id = e.queue_id
             WHERE q.status IN ('waiting', 'running', 'paused')
               AND e.status IN ('pending', 'running')",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(rows.iter().map(|r| r.get("product_id")).collect())
    }

    async fn current_running_queue(&self) -> Result<Option<CrawlQueue>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM crawl_queues WHERE status = 'running'
             ORDER BY created_at ASC, id ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(to_infra)?;
        row.as_ref().map(row_to_queue).transpose()
    }

    async fn oldest_waiting_queue(&self) -> Result<Option<CrawlQueue>, DomainError> {
        let row = sqlx::query(
            "SELECT * FROM crawl_queues WHERE status = 'waiting'
             ORDER BY created_at ASC, id ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(to_infra)?;
        row.as_ref().map(row_to_queue).transpose()
    }

    async fn list_by_status(
        &self,
        statuses: &[QueueStatus],
    ) -> Result<Vec<CrawlQueue>, DomainError> {
        if statuses.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = statuses.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM crawl_queues WHERE status IN ({placeholders})
             ORDER BY created_at ASC, id ASC"
        );
        let mut query = sqlx::query(&sql);
        for s in statuses {
            query = query.bind(s.as_str());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(to_infra)?;
        rows.iter().map(row_to_queue).collect()
    }

    async fn reset_running_entries(&self) -> Result<(), DomainError> {
        sqlx::query("UPDATE crawl_entries SET status = 'pending' WHERE status = 'running'")
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(())
    }

    async fn delete_queue(&self, id: i64) -> Result<bool, DomainError> {
        sqlx::query("DELETE FROM crawl_entries WHERE queue_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        let result = sqlx::query("DELETE FROM crawl_queues WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_queue(row: &SqliteRow) -> Result<CrawlQueue, DomainError> {
    Ok(CrawlQueue {
        id: row.get("id"),
        status: QueueStatus::from_str(row.get::<String, _>("status").as_str())?,
        interval_secs: row.get::<i64, _>("interval_secs") as u32,
        created_at: row.get::<i64, _>("created_at") as u64,
        finished_at: row.get::<Option<i64>, _>("finished_at").map(|t| t as u64),
    })
}

fn row_to_entry(row: &SqliteRow) -> Result<CrawlEntry, DomainError> {
    Ok(CrawlEntry {
        id: row.get("id"),
        queue_id: row.get("queue_id"),
        product_id: row.get("product_id"),
        status: EntryStatus::from_str(row.get::<String, _>("status").as_str())?,
        error: row.get("error"),
        crawled_at: row.get::<Option<i64>, _>("crawled_at").map(|t| t as u64),
    })
}
