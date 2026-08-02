//! SQLite 仓储实现：标签、待爬取商品、抓取队列、AI 配置的持久化。
//! 连接时自动创建数据目录与表结构（IF NOT EXISTS），无需外部迁移工具。
//! 各仓储共享同一个连接池，由 `connect` 在启动时创建。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;

use crate::domain::ai_provider::{AiProvider, NewAiProvider};
use crate::domain::ai_tool_call::{AiToolCall, NewAiToolCall};
use crate::domain::crawl_queue::{CrawlEntry, CrawlQueue, EntryStatus, QueueStatus};
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::product::{NewProduct, Product, ProductName};
use crate::domain::repository::{
    AiProviderRepository, AiToolCallRepository, ItemRepository, Page, ProductRepository,
    ProductSortColumn, QueueRepository, SettingsRepository, TagRepository,
};
use crate::domain::tag::{NewTag, Tag, TagName};

/// 打开（必要时创建）数据库，初始化全部表结构，返回共享连接池。
/// 开启外键约束：删除商品或标签时自动清理 product_tags 关联（ON DELETE CASCADE）。
/// 传 ":memory:" 使用内存库（测试用）：整个池限制为一条连接，
/// 否则每条连接会各开一个独立的空库。
pub async fn connect(path: &str) -> Result<SqlitePool, DomainError> {
    if path == ":memory:" {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .pragma("foreign_keys", "ON");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(to_infra)?;
        create_tables(&pool).await?;
        return Ok(pool);
    }

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

    create_tables(&pool).await?;
    Ok(pool)
}

/// 初始化全部表结构（IF NOT EXISTS，可重复执行）
async fn create_tables(pool: &SqlitePool) -> Result<(), DomainError> {
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
    .execute(&*pool)
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
    .execute(&*pool)
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
    .execute(&*pool)
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
    .execute(&*pool)
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
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // AI 供应商配置（密钥明文存本地库，见 domain/ai_provider.rs 说明）
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ai_providers (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            name         TEXT NOT NULL UNIQUE,
            base_url     TEXT NOT NULL,
            api_key      TEXT,
            model        TEXT NOT NULL,
            timeout_secs INTEGER NOT NULL DEFAULT 60,
            max_retries  INTEGER NOT NULL DEFAULT 2,
            is_default   INTEGER NOT NULL DEFAULT 0,
            created_at   INTEGER NOT NULL,
            updated_at   INTEGER NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // AI 工具调用审计（只增不改）
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ai_tool_calls (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name   TEXT NOT NULL,
            arguments   TEXT NOT NULL,
            result      TEXT,
            error       TEXT,
            duration_ms INTEGER NOT NULL,
            created_at  INTEGER NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // 应用级 KV 设置（用户自定义抓取提示词等）
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // 抓取到的原始商品数据：id = 详情页 URL（同一链接重复抓取时更新价格与时间）
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id         TEXT PRIMARY KEY,
            title      TEXT NOT NULL,
            price      REAL NOT NULL,
            seller     TEXT NOT NULL DEFAULT '',
            url        TEXT NOT NULL,
            crawled_at INTEGER NOT NULL,
            product_id INTEGER
        )",
    )
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // 老库 items 表可能缺 product_id 列（CREATE TABLE IF NOT EXISTS 不会补列），手动迁移
    let cols = sqlx::query("PRAGMA table_info(items)")
        .fetch_all(&*pool)
        .await
        .map_err(to_infra)?;
    let has_product_id = cols
        .iter()
        .any(|r| r.get::<String, _>("name") == "product_id");
    if !has_product_id {
        sqlx::query("ALTER TABLE items ADD COLUMN product_id INTEGER")
            .execute(&*pool)
            .await
            .map_err(to_infra)?;
        tracing::info!("items 表迁移：补充 product_id 列");
    }

    Ok(())
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

    async fn count(&self) -> Result<u64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM products")
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(row.get::<i64, _>("c") as u64)
    }

    async fn max_last_crawled_at(&self) -> Result<Option<u64>, DomainError> {
        let row = sqlx::query("SELECT MAX(last_crawled_at) AS m FROM products")
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(row.get::<Option<i64>, _>("m").map(|t| t as u64))
    }

    async fn list_paginated(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(ProductSortColumn, bool)>,
        search: Option<&str>,
        tag_id: Option<i64>,
    ) -> Result<Page<Product>, DomainError> {
        let order_by = match sort {
            Some((col, desc)) => format!(
                "ORDER BY ({col} IS NULL) ASC, {col} {dir}, p.id ASC",
                col = col.as_sql(),
                dir = if desc { "DESC" } else { "ASC" },
            ),
            None => "ORDER BY p.created_at ASC".to_string(),
        };
        let mut where_parts: Vec<String> = Vec::new();
        if let Some(q) = search {
            where_parts.push(format!("p.name LIKE '%{q}%'"));
        }
        if let Some(tid) = tag_id {
            where_parts.push(format!(
                "EXISTS (SELECT 1 FROM product_tags pt WHERE pt.product_id = p.id AND pt.tag_id = {tid})"
            ));
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let count_sql = format!("SELECT COUNT(*) AS c FROM products p {where_clause}");
        let rows = sqlx::query(&format!(
            "{PRODUCT_SELECT} {where_clause} {order_by} LIMIT ? OFFSET ?"
        ))
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        let items = rows
            .iter()
            .map(row_to_product)
            .collect::<Result<Vec<_>, _>>()?;
        let count_row = sqlx::query(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        let total = count_row.get::<i64, _>("c") as u64;
        Ok(Page { items, total })
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

// ---------- AI 配置 ----------

pub struct SqliteAiProviderRepository {
    pool: SqlitePool,
}

impl SqliteAiProviderRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiProviderRepository for SqliteAiProviderRepository {
    async fn create(&self, provider: &NewAiProvider) -> Result<AiProvider, DomainError> {
        let now = crate::domain::crawl_task::now_unix();
        let result = sqlx::query(
            "INSERT INTO ai_providers
             (name, base_url, api_key, model, timeout_secs, max_retries, is_default, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&provider.name)
        .bind(&provider.base_url)
        .bind(&provider.api_key)
        .bind(&provider.model)
        .bind(provider.timeout_secs as i64)
        .bind(provider.max_retries as i64)
        .bind(now as i64)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;

        Ok(AiProvider {
            id: result.last_insert_rowid(),
            name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            api_key: provider.api_key.clone(),
            model: provider.model.clone(),
            timeout_secs: provider.timeout_secs,
            max_retries: provider.max_retries,
            is_default: false,
            created_at: now,
            updated_at: now,
        })
    }

    async fn find(&self, id: i64) -> Result<Option<AiProvider>, DomainError> {
        let row = sqlx::query("SELECT * FROM ai_providers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_ai_provider).transpose()
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<AiProvider>, DomainError> {
        let row = sqlx::query("SELECT * FROM ai_providers WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_ai_provider).transpose()
    }

    async fn list(&self) -> Result<Vec<AiProvider>, DomainError> {
        let rows = sqlx::query("SELECT * FROM ai_providers ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        rows.iter().map(row_to_ai_provider).collect()
    }

    async fn update(&self, provider: &AiProvider) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE ai_providers SET
             name = ?, base_url = ?, api_key = ?, model = ?,
             timeout_secs = ?, max_retries = ?, is_default = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&provider.name)
        .bind(&provider.base_url)
        .bind(&provider.api_key)
        .bind(&provider.model)
        .bind(provider.timeout_secs as i64)
        .bind(provider.max_retries as i64)
        .bind(provider.is_default)
        .bind(provider.updated_at as i64)
        .bind(provider.id)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM ai_providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_default(&self) -> Result<Option<AiProvider>, DomainError> {
        let row = sqlx::query("SELECT * FROM ai_providers WHERE is_default = 1 LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        row.as_ref().map(row_to_ai_provider).transpose()
    }

    async fn clear_default(&self) -> Result<(), DomainError> {
        sqlx::query("UPDATE ai_providers SET is_default = 0")
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(())
    }
}

fn row_to_ai_provider(row: &SqliteRow) -> Result<AiProvider, DomainError> {
    Ok(AiProvider {
        id: row.get("id"),
        name: row.get("name"),
        base_url: row.get("base_url"),
        api_key: row.get("api_key"),
        model: row.get("model"),
        timeout_secs: row.get::<i64, _>("timeout_secs") as u32,
        max_retries: row.get::<i64, _>("max_retries") as u32,
        is_default: row.get("is_default"),
        created_at: row.get::<i64, _>("created_at") as u64,
        updated_at: row.get::<i64, _>("updated_at") as u64,
    })
}

// ---------- AI 工具调用审计 ----------

pub struct SqliteAiToolCallRepository {
    pool: SqlitePool,
}

impl SqliteAiToolCallRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AiToolCallRepository for SqliteAiToolCallRepository {
    async fn create(&self, call: &NewAiToolCall) -> Result<AiToolCall, DomainError> {
        let now = crate::domain::crawl_task::now_unix();
        let result = sqlx::query(
            "INSERT INTO ai_tool_calls (tool_name, arguments, result, error, duration_ms, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&call.tool_name)
        .bind(&call.arguments)
        .bind(&call.result)
        .bind(&call.error)
        .bind(call.duration_ms as i64)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .map_err(to_infra)?;

        Ok(AiToolCall {
            id: result.last_insert_rowid(),
            tool_name: call.tool_name.clone(),
            arguments: call.arguments.clone(),
            result: call.result.clone(),
            error: call.error.clone(),
            duration_ms: call.duration_ms,
            created_at: now,
        })
    }

    async fn list_paginated(&self, offset: u64, limit: u64) -> Result<Page<AiToolCall>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM ai_tool_calls ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(to_infra)?;
        let items = rows
            .iter()
            .map(row_to_ai_tool_call)
            .collect::<Result<Vec<_>, _>>()?;

        let count_row = sqlx::query("SELECT COUNT(*) AS c FROM ai_tool_calls")
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        let total = count_row.get::<i64, _>("c") as u64;

        Ok(Page { items, total })
    }
}

fn row_to_ai_tool_call(row: &SqliteRow) -> Result<AiToolCall, DomainError> {
    Ok(AiToolCall {
        id: row.get("id"),
        tool_name: row.get("tool_name"),
        arguments: row.get("arguments"),
        result: row.get("result"),
        error: row.get("error"),
        duration_ms: row.get::<i64, _>("duration_ms") as u64,
        created_at: row.get::<i64, _>("created_at") as u64,
    })
}

// ---------- 抓取到的原始商品数据 ----------

pub struct SqliteItemRepository {
    pool: SqlitePool,
}

impl SqliteItemRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ItemRepository for SqliteItemRepository {
    /// 同一链接重复抓取时覆盖（价格/标题可能变化，crawled_at 刷新）
    async fn save_all(&self, items: &[Item]) -> Result<(), DomainError> {
        for item in items {
            sqlx::query(
                "INSERT OR REPLACE INTO items (id, title, price, seller, url, crawled_at, product_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&item.id)
            .bind(&item.title)
            .bind(item.price)
            .bind(&item.seller)
            .bind(&item.url)
            .bind(item.crawled_at as i64)
            .bind(item.product_id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        }
        Ok(())
    }

    async fn list_paginated(
        &self,
        offset: u64,
        limit: u64,
        search: Option<&str>,
    ) -> Result<Page<Item>, DomainError> {
        let (join_clause, where_clause, count_sql) = match search {
            Some(q) => {
                let w = format!("WHERE i.title LIKE '%{q}%' OR p.name LIKE '%{q}%'");
                let c = format!(
                    "SELECT COUNT(*) AS c FROM items i LEFT JOIN products p ON i.product_id = p.id {w}"
                );
                (
                    "LEFT JOIN products p ON i.product_id = p.id".to_string(),
                    w,
                    c,
                )
            }
            None => (
                String::new(),
                String::new(),
                "SELECT COUNT(*) AS c FROM items".to_string(),
            ),
        };
        let sql = format!(
            "SELECT i.* FROM items i {join_clause} {where_clause} ORDER BY i.crawled_at DESC, i.id ASC LIMIT ? OFFSET ?"
        );
        let rows = sqlx::query(&sql)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        let items = rows.iter().map(row_to_item).collect();

        let count_row = sqlx::query(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        let total = count_row.get::<i64, _>("c") as u64;

        Ok(Page { items, total })
    }

    async fn count_since(&self, unix_ts: u64) -> Result<u64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM items WHERE crawled_at >= ?")
            .bind(unix_ts as i64)
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(row.get::<i64, _>("c") as u64)
    }

    async fn list_latest_for_product(&self, product_id: i64) -> Result<Vec<Item>, DomainError> {
        // 同一轮抓取的条目共享 crawled_at：取该商品最新一轮的全部明细，按价格升序
        let rows = sqlx::query(
            "SELECT * FROM items
             WHERE product_id = ?
               AND crawled_at = (SELECT MAX(crawled_at) FROM items WHERE product_id = ?)
             ORDER BY price ASC, id ASC",
        )
        .bind(product_id)
        .bind(product_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(rows.iter().map(row_to_item).collect())
    }

    async fn list_by_product(&self, product_id: i64) -> Result<Vec<Item>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM items WHERE product_id = ? ORDER BY crawled_at ASC",
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_infra)?;
        Ok(rows.iter().map(row_to_item).collect())
    }
}

fn row_to_item(row: &SqliteRow) -> Item {
    Item {
        id: row.get("id"),
        title: row.get("title"),
        price: row.get("price"),
        seller: row.get("seller"),
        url: row.get("url"),
        crawled_at: row.get::<i64, _>("crawled_at") as u64,
        product_id: row.get("product_id"),
    }
}

// ---------- 应用级 KV 设置 ----------

pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn get(&self, key: &str) -> Result<Option<String>, DomainError> {
        let row = sqlx::query("SELECT value FROM app_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(row.map(|r| r.get("value")))
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), DomainError> {
        sqlx::query("INSERT OR REPLACE INTO app_settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(())
    }
}

#[cfg(test)]
mod item_repo_tests {
    use super::*;
    use crate::domain::crawl_task::now_unix;

    fn sample(id: &str, price: f64, crawled_at: u64) -> Item {
        Item {
            id: id.into(),
            title: format!("商品 {id}"),
            price,
            seller: "卖家".into(),
            url: format!("https://www.goofish.com/item?id={id}"),
            crawled_at,
            product_id: None,
        }
    }

    #[tokio::test]
    async fn save_dedup_paginate_and_count_since() {
        let pool = connect(":memory:").await.unwrap();
        let repo = SqliteItemRepository::new(pool);
        let now = now_unix();

        repo.save_all(&[sample("a", 100.0, now - 10), sample("b", 200.0, now)])
            .await
            .unwrap();
        // 同 id 再抓：覆盖而不是新增
        repo.save_all(&[sample("a", 150.0, now)]).await.unwrap();

        let page = repo.list_paginated(0, 10, None).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);
        // 按抓取时间倒序：a（刚刷新）在最前
        assert_eq!(page.items[0].id, "a");
        assert_eq!(page.items[0].price, 150.0);

        let page = repo.list_paginated(1, 1, None).await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "b");

        assert_eq!(repo.count_since(now - 5).await.unwrap(), 2);
        assert_eq!(repo.count_since(now + 5).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_latest_for_product_returns_only_latest_round() {
        let pool = connect(":memory:").await.unwrap();
        let repo = SqliteItemRepository::new(pool);
        let now = now_unix();

        // 商品 1 的两轮抓取 + 商品 2 的一轮
        let mut round1 = vec![sample("r1-a", 100.0, now - 100), sample("r1-b", 200.0, now - 100)];
        let mut round2 = vec![
            sample("r2-a", 300.0, now),
            sample("r2-b", 250.0, now),
            sample("r2-c", 280.0, now),
        ];
        let mut other = vec![sample("o-a", 999.0, now)];
        for (i, pid) in [(&mut round1, 1), (&mut round2, 1), (&mut other, 2)] {
            for it in i.iter_mut() {
                it.product_id = Some(pid);
            }
        }
        repo.save_all(&round1).await.unwrap();
        repo.save_all(&round2).await.unwrap();
        repo.save_all(&other).await.unwrap();

        let latest = repo.list_latest_for_product(1).await.unwrap();
        assert_eq!(latest.len(), 3);
        assert!(latest.iter().all(|i| i.crawled_at == now));
        // 按价格升序
        assert_eq!(latest[0].id, "r2-b");
        assert_eq!(latest[2].id, "r2-a");

        assert!(repo.list_latest_for_product(999).await.unwrap().is_empty());
    }
}
