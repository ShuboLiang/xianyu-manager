//! SQLite 仓储实现：标签、待爬取商品、抓取队列、AI 配置的持久化。
//! 连接时自动创建数据目录与表结构（IF NOT EXISTS），无需外部迁移工具。
//! 各仓储共享同一个连接池，由 `connect` 在启动时创建。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;

use crate::domain::ai_provider::{AiProvider, BaseUrl, ModelName, NewAiProvider, ProviderName};
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
            mode_price      REAL,
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

    // 老库 products 表可能缺 mode_price 列，手动迁移（同 items.product_id 的做法）
    let cols = sqlx::query("PRAGMA table_info(products)")
        .fetch_all(&*pool)
        .await
        .map_err(to_infra)?;
    if !cols.iter().any(|r| r.get::<String, _>("name") == "mode_price") {
        sqlx::query("ALTER TABLE products ADD COLUMN mode_price REAL")
            .execute(&*pool)
            .await
            .map_err(to_infra)?;
        tracing::info!("products 表迁移：补充 mode_price 列");
    }

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
            name          TEXT NOT NULL DEFAULT '',
            name_custom   INTEGER NOT NULL DEFAULT 0,
            created_at    INTEGER NOT NULL,
            finished_at   INTEGER
        )",
    )
    .execute(&*pool)
    .await
    .map_err(to_infra)?;

    // 老库 crawl_queues 表可能缺 name/name_custom 列，手动迁移（同 items.product_id 的做法）
    let cols = sqlx::query("PRAGMA table_info(crawl_queues)")
        .fetch_all(&*pool)
        .await
        .map_err(to_infra)?;
    let col_names: Vec<String> = cols.iter().map(|r| r.get("name")).collect();
    if !col_names.iter().any(|c| c == "name") {
        sqlx::query("ALTER TABLE crawl_queues ADD COLUMN name TEXT NOT NULL DEFAULT ''")
            .execute(&*pool)
            .await
            .map_err(to_infra)?;
        tracing::info!("crawl_queues 表迁移：补充 name 列");
    }
    if !col_names.iter().any(|c| c == "name_custom") {
        sqlx::query("ALTER TABLE crawl_queues ADD COLUMN name_custom INTEGER NOT NULL DEFAULT 0")
            .execute(&*pool)
            .await
            .map_err(to_infra)?;
        tracing::info!("crawl_queues 表迁移：补充 name_custom 列");
    }

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
    // 库中数据理论上都经过值对象校验；读回时非法数据归为基础设施错误
    Ok(Tag {
        id: row.get("id"),
        name: TagName::new(row.get::<String, _>("name"))
            .map_err(|e| DomainError::Infrastructure(format!("tags 行数据非法: {e}")))?,
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
        // 商品行 + 标签关联是一个原子写入：任一失败整体回滚
        let mut tx = self.pool.begin().await.map_err(to_infra)?;
        let result = sqlx::query(
            "INSERT INTO products (name, remark, created_at, updated_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(product.name.as_str())
        .bind(&product.remark)
        .bind(now as i64)
        .bind(now as i64)
        .execute(&mut *tx)
        .await
        .map_err(to_infra)?;
        let id = result.last_insert_rowid();

        save_tag_links(&mut tx, id, &product.tag_ids).await?;
        tx.commit().await.map_err(to_infra)?;

        Ok(Product {
            id,
            name: product.name.clone(),
            tag_ids: product.tag_ids.clone(),
            remark: product.remark.clone(),
            median_price: None,
            avg_price: None,
            mode_price: None,
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
                col = product_sort_column_sql(col),
                dir = if desc { "DESC" } else { "ASC" },
            ),
            None => "ORDER BY p.created_at ASC".to_string(),
        };
        // 搜索词走 bind 参数（防注入/通配符错乱）；tag_id 是 i64 数值，拼入安全
        let mut where_parts: Vec<String> = Vec::new();
        let escaped = search.map(like_escape);
        if escaped.is_some() {
            where_parts.push("p.name LIKE '%' || ? || '%' ESCAPE '\\'".to_string());
        }
        if let Some(tid) = tag_id {
            if tid == -1 {
                where_parts.push(
                    "NOT EXISTS (SELECT 1 FROM product_tags pt WHERE pt.product_id = p.id)"
                        .to_string(),
                );
            } else {
                where_parts.push(format!(
                    "EXISTS (SELECT 1 FROM product_tags pt WHERE pt.product_id = p.id AND pt.tag_id = {tid})"
                ));
            }
        }
        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let count_sql = format!("SELECT COUNT(*) AS c FROM products p {where_clause}");

        let list_sql = format!("{PRODUCT_SELECT} {where_clause} {order_by} LIMIT ? OFFSET ?");
        let mut list_q = sqlx::query(&list_sql);
        if let Some(e) = &escaped {
            list_q = list_q.bind(e.clone());
        }
        let rows = list_q
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        let items = rows
            .iter()
            .map(row_to_product)
            .collect::<Result<Vec<_>, _>>()?;

        let mut count_q = sqlx::query(&count_sql);
        if let Some(e) = &escaped {
            count_q = count_q.bind(e.clone());
        }
        let count_row = count_q
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
        // 基本信息 UPDATE + 关联全量重建是一个原子写入：
        // 中途失败不会留下「标签被清空」的半状态
        let mut tx = self.pool.begin().await.map_err(to_infra)?;
        sqlx::query(
            "UPDATE products SET name = ?, remark = ?,
                median_price = ?, avg_price = ?, mode_price = ?, crawled_count = ?,
                last_crawled_at = ?, recycle_price = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(product.name.as_str())
        .bind(&product.remark)
        .bind(product.median_price)
        .bind(product.avg_price)
        .bind(product.mode_price)
        .bind(product.crawled_count.map(|c| c as i64))
        .bind(product.last_crawled_at.map(|t| t as i64))
        .bind(product.recycle_price)
        .bind(product.updated_at as i64)
        .bind(product.id)
        .execute(&mut *tx)
        .await
        .map_err(to_infra)?;

        // 关联全量重建（商品-标签数量很小，先删后插最简单可靠）
        sqlx::query("DELETE FROM product_tags WHERE product_id = ?")
            .bind(product.id)
            .execute(&mut *tx)
            .await
            .map_err(to_infra)?;
        save_tag_links(&mut tx, product.id, &product.tag_ids).await?;
        tx.commit().await.map_err(to_infra)
    }

    /// 按 id 列表批量查询（勾选批删的预览样本用），按创建时间升序
    async fn list_by_ids(&self, ids: &[i64]) -> Result<Vec<Product>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("{PRODUCT_SELECT} WHERE p.id IN ({placeholders}) ORDER BY p.created_at ASC");
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(to_infra)?;
        rows.iter().map(row_to_product).collect()
    }

    async fn delete(&self, id: i64) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM products WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_by_ids(&self, ids: &[i64]) -> Result<u64, DomainError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("DELETE FROM products WHERE id IN ({placeholders})");
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let result = query.execute(&self.pool).await.map_err(to_infra)?;
        Ok(result.rows_affected())
    }
}

/// 写入商品-标签关联（在调用方的事务内执行，保证与商品写入同生共死）
async fn save_tag_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    product_id: i64,
    tag_ids: &[i64],
) -> Result<(), DomainError> {
    for tag_id in tag_ids {
        sqlx::query("INSERT OR IGNORE INTO product_tags (product_id, tag_id) VALUES (?, ?)")
            .bind(product_id)
            .bind(tag_id)
            .execute(&mut **tx)
            .await
            .map_err(to_infra)?;
    }
    Ok(())
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
        name: ProductName::new(row.get::<String, _>("name"))
            .map_err(|e| DomainError::Infrastructure(format!("products 行数据非法: {e}")))?,
        tag_ids,
        remark: row.get("remark"),
        median_price: row.get("median_price"),
        avg_price: row.get("avg_price"),
        mode_price: row.get("mode_price"),
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

/// LIKE 模糊匹配的通配符转义（配合 ESCAPE '\'）：
/// 用户输入中的 \ % _ 不再具有通配含义，只按字面匹配
fn like_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// ProductSortColumn（domain 白名单枚举）→ SQL 列名。
/// 映射属于持久化细节，只活在 infra 层；返回值全部来自此白名单，可安全拼入 SQL。
fn product_sort_column_sql(col: ProductSortColumn) -> &'static str {
    match col {
        ProductSortColumn::MedianPrice => "p.median_price",
        ProductSortColumn::AvgPrice => "p.avg_price",
        ProductSortColumn::ModePrice => "p.mode_price",
        ProductSortColumn::CrawledCount => "p.crawled_count",
        ProductSortColumn::LastCrawledAt => "p.last_crawled_at",
        ProductSortColumn::RecyclePrice => "p.recycle_price",
    }
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
            "INSERT INTO crawl_queues (status, interval_secs, name, name_custom, created_at, finished_at)
             VALUES (?, ?, ?, ?, ?, NULL)",
        )
        .bind(queue.status.as_str())
        .bind(queue.interval_secs as i64)
        .bind(&queue.name)
        .bind(queue.name_custom)
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
            "UPDATE crawl_queues SET status = ?, interval_secs = ?, name = ?, name_custom = ?, finished_at = ?
             WHERE id = ?",
        )
        .bind(queue.status.as_str())
        .bind(queue.interval_secs as i64)
        .bind(&queue.name)
        .bind(queue.name_custom)
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
        name: row.get("name"),
        name_custom: row.get::<i64, _>("name_custom") != 0,
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
        .bind(provider.name.as_str())
        .bind(provider.base_url.as_str())
        .bind(&provider.api_key)
        .bind(provider.model.as_str())
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
        .bind(provider.name.as_str())
        .bind(provider.base_url.as_str())
        .bind(&provider.api_key)
        .bind(provider.model.as_str())
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
    // 库中数据理论上都经过值对象校验；读回时仍走构造函数兜底，非法数据报基础设施错误
    let to_infra_err = |e: DomainError| {
        DomainError::Infrastructure(format!("ai_providers 行数据非法: {e}"))
    };
    Ok(AiProvider {
        id: row.get("id"),
        name: ProviderName::new(row.get::<String, _>("name")).map_err(to_infra_err)?,
        base_url: BaseUrl::new(row.get::<String, _>("base_url")).map_err(to_infra_err)?,
        api_key: row.get("api_key"),
        model: ModelName::new(row.get::<String, _>("model")).map_err(to_infra_err)?,
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

    async fn list_paginated(
        &self,
        offset: u64,
        limit: u64,
        tool_name: Option<&str>,
        failed_only: Option<bool>,
    ) -> Result<Page<AiToolCall>, DomainError> {
        let where_clause = ai_tool_call_where(tool_name.is_some(), failed_only);
        let list_sql = format!(
            "SELECT * FROM ai_tool_calls{where_clause} ORDER BY created_at DESC, id DESC LIMIT {limit} OFFSET {offset}"
        );
        let mut list_q = sqlx::query(&list_sql);
        if let Some(name) = tool_name {
            list_q = list_q.bind(name);
        }
        let rows = list_q
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        let items = rows
            .iter()
            .map(row_to_ai_tool_call)
            .collect::<Result<Vec<_>, _>>()?;

        let count_sql = format!("SELECT COUNT(*) AS c FROM ai_tool_calls{where_clause}");
        let mut count_q = sqlx::query(&count_sql);
        if let Some(name) = tool_name {
            count_q = count_q.bind(name);
        }
        let count_row = count_q
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        let total = count_row.get::<i64, _>("c") as u64;

        Ok(Page { items, total })
    }

    async fn list_tool_names(&self) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query("SELECT DISTINCT tool_name FROM ai_tool_calls ORDER BY tool_name ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(rows.iter().map(|r| r.get::<String, _>("tool_name")).collect())
    }

    async fn purge_preview(
        &self,
        before_ts: Option<u64>,
        keep_latest: Option<u64>,
    ) -> Result<u64, DomainError> {
        let sql = format!(
            "SELECT COUNT(*) AS c FROM ai_tool_calls{}",
            purge_where(before_ts, keep_latest)
        );
        let row = sqlx::query(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(row.get::<i64, _>("c") as u64)
    }

    async fn purge(&self, before_ts: Option<u64>, keep_latest: Option<u64>) -> Result<u64, DomainError> {
        let sql = format!(
            "DELETE FROM ai_tool_calls{}",
            purge_where(before_ts, keep_latest)
        );
        let result = sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected())
    }
}

/// 列表筛选的 WHERE 子句：tool_name 以占位符形式出现（调用方负责 bind），
/// failed_only 是布尔值，拼入安全。
fn ai_tool_call_where(has_tool_name: bool, failed_only: Option<bool>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if has_tool_name {
        parts.push("tool_name = ?".to_string());
    }
    if let Some(failed) = failed_only {
        parts.push(if failed {
            "error IS NOT NULL".to_string()
        } else {
            "error IS NULL".to_string()
        });
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", parts.join(" AND "))
    }
}

/// 清理条件的 WHERE 子句：before_ts 与 keep_latest 恰有一个生效（service 层已校验）。
fn purge_where(before_ts: Option<u64>, keep_latest: Option<u64>) -> String {
    if let Some(ts) = before_ts {
        return format!(" WHERE created_at < {ts}");
    }
    match keep_latest {
        Some(n) => format!(
            " WHERE id NOT IN (SELECT id FROM ai_tool_calls ORDER BY created_at DESC, id DESC LIMIT {n})"
        ),
        None => String::new(),
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
        tag_id: Option<i64>,
    ) -> Result<Page<Item>, DomainError> {
        // 搜索词走 bind 参数（防注入/通配符错乱）：标题或所属商品名模糊匹配；
        // 标签筛选 = 记录所属商品挂在该标签下（product_id 为 NULL 的记录不归属任何标签，永不命中）
        let escaped = search.map(like_escape);
        let join_clause = if escaped.is_some() {
            "LEFT JOIN products p ON i.product_id = p.id"
        } else {
            ""
        };
        let mut conditions: Vec<String> = Vec::new();
        if escaped.is_some() {
            conditions.push(
                "(i.title LIKE '%' || ? || '%' ESCAPE '\\' OR p.name LIKE '%' || ? || '%' ESCAPE '\\')"
                    .to_string(),
            );
        }
        if tag_id.is_some() {
            conditions.push(
                "EXISTS (SELECT 1 FROM product_tags pt WHERE pt.product_id = i.product_id AND pt.tag_id = ?)"
                    .to_string(),
            );
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let list_sql = format!(
            "SELECT i.* FROM items i {join_clause} {where_clause} ORDER BY i.crawled_at DESC, i.id ASC LIMIT ? OFFSET ?"
        );
        let mut list_q = sqlx::query(&list_sql);
        if let Some(e) = &escaped {
            list_q = list_q.bind(e.clone()).bind(e.clone());
        }
        if let Some(tid) = tag_id {
            list_q = list_q.bind(tid);
        }
        let rows = list_q
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(to_infra)?;
        let items = rows.iter().map(row_to_item).collect();

        let count_sql =
            format!("SELECT COUNT(*) AS c FROM items i {join_clause} {where_clause}");
        let mut count_q = sqlx::query(&count_sql);
        if let Some(e) = &escaped {
            count_q = count_q.bind(e.clone()).bind(e.clone());
        }
        if let Some(tid) = tag_id {
            count_q = count_q.bind(tid);
        }
        let count_row = count_q
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

    async fn detach_product(&self, product_ids: &[i64]) -> Result<(), DomainError> {
        if product_ids.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; product_ids.len()].join(",");
        let sql = format!(
            "UPDATE items SET product_id = NULL WHERE product_id IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for id in product_ids {
            query = query.bind(id);
        }
        query.execute(&self.pool).await.map_err(to_infra)?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM items WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_infra)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_by_ids(&self, ids: &[String]) -> Result<Vec<Item>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("SELECT * FROM items WHERE id IN ({placeholders}) ORDER BY id ASC");
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(to_infra)?;
        Ok(rows.iter().map(row_to_item).collect())
    }

    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64, DomainError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("DELETE FROM items WHERE id IN ({placeholders})");
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let result = query.execute(&self.pool).await.map_err(to_infra)?;
        Ok(result.rows_affected())
    }

    async fn delete_matching(&self, search: Option<&str>) -> Result<u64, DomainError> {
        // WHERE 语义与 list_paginated 的 search 一致（标题或所属商品名模糊匹配），
        // SQLite 的 DELETE 不支持 JOIN，商品名条件改用子查询表达；搜索词走 bind 参数
        let result = match search {
            Some(q) => {
                let escaped = like_escape(q);
                sqlx::query(
                    "DELETE FROM items WHERE title LIKE '%' || ? || '%' ESCAPE '\\'
                     OR product_id IN (
                         SELECT id FROM products WHERE name LIKE '%' || ? || '%' ESCAPE '\\'
                     )",
                )
                .bind(&escaped)
                .bind(&escaped)
                .execute(&self.pool)
                .await
                .map_err(to_infra)?
            }
            None => sqlx::query("DELETE FROM items")
                .execute(&self.pool)
                .await
                .map_err(to_infra)?,
        };
        Ok(result.rows_affected())
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

        let page = repo.list_paginated(0, 10, None, None).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);
        // 按抓取时间倒序：a（刚刷新）在最前
        assert_eq!(page.items[0].id, "a");
        assert_eq!(page.items[0].price, 150.0);

        let page = repo.list_paginated(1, 1, None, None).await.unwrap();
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

    #[tokio::test]
    async fn search_with_quote_and_wildcards_matches_literally() {
        let pool = connect(":memory:").await.unwrap();
        let repo = SqliteItemRepository::new(pool);
        let now = now_unix();

        let mut special = sample("sp", 100.0, now);
        special.title = "Men's 100% 全新".into();
        repo.save_all(&[sample("plain", 50.0, now), special]).await.unwrap();

        // 单引号不破坏 SQL，且能按字面命中
        let page = repo.list_paginated(0, 10, Some("Men's"), None).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "sp");

        // % 不再作为通配符：只命中标题里真的含 "100%" 的记录
        let page = repo.list_paginated(0, 10, Some("100%"), None).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "sp");

        // 普通关键词仍能模糊命中（"商品 plain" 的标题含「商品」）
        let page = repo.list_paginated(0, 10, Some("商品"), None).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "plain");

        // 批删与列表同一 WHERE 语义：通配符按字面处理，不会误删
        assert_eq!(repo.delete_matching(Some("100%")).await.unwrap(), 1);
        assert_eq!(repo.list_paginated(0, 10, None, None).await.unwrap().total, 1);
    }

    #[tokio::test]
    async fn tag_filter_matches_only_products_under_tag() {
        let pool = connect(":memory:").await.unwrap();
        let repo = SqliteItemRepository::new(pool.clone());
        let now = now_unix();

        // 两个商品：商品 1 挂标签 1，商品 2 无标签
        for (pid, name) in [(1, "显卡 A"), (2, "主板 B")] {
            sqlx::query("INSERT INTO products (id, name, created_at, updated_at) VALUES (?, ?, 0, 0)")
                .bind(pid)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO tags (id, name, created_at, updated_at) VALUES (1, '显卡', 0, 0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_tags (product_id, tag_id) VALUES (1, 1)")
            .execute(&pool)
            .await
            .unwrap();

        let mut tagged = sample("t1", 100.0, now);
        tagged.product_id = Some(1);
        let mut untagged = sample("u1", 200.0, now);
        untagged.product_id = Some(2);
        let detached = sample("d1", 300.0, now); // product_id 为 NULL（商品已删）
        repo.save_all(&[tagged, untagged, detached]).await.unwrap();

        // 按标签筛选：只命中挂在该标签下的商品的记录
        let page = repo.list_paginated(0, 10, None, Some(1)).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "t1");

        // 标签筛选与搜索叠加
        let page = repo
            .list_paginated(0, 10, Some("显卡"), Some(1))
            .await
            .unwrap();
        assert_eq!(page.total, 1);

        // 不存在的标签：零命中
        assert_eq!(repo.list_paginated(0, 10, None, Some(999)).await.unwrap().total, 0);
    }

    #[test]
    fn like_escape_escapes_wildcards() {
        assert_eq!(like_escape("100%"), "100\\%");
        assert_eq!(like_escape("a_b"), "a\\_b");
        assert_eq!(like_escape("c\\d"), "c\\\\d");
        assert_eq!(like_escape("普通文本"), "普通文本");
    }
}
