//! HTTP 请求/响应 DTO：与 domain 模型解耦，serde 注解只属于这一层。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::application::ai_provider_service::{AiStatus, TestConnectionResult};
use crate::application::ai::classify_service::{ClassifySuggestion, ClassifySyncResult};
use crate::application::queue_service::QueueProgress;
use crate::domain::ai_classify_task::{AiClassifyTask, ClassifyWarning};
use crate::domain::ai_provider::AiProvider;
use crate::domain::ai_tool_call::AiToolCall;
use crate::domain::crawl_queue::Selector;
use crate::domain::crawl_task::{CrawlTask, TaskStatus};
use crate::domain::item::Item;
use crate::domain::product::Product;
use crate::domain::tag::Tag;

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CrawlRequest {
    pub keyword: String,
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
}

fn default_max_pages() -> u32 {
    1
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TaskResponse {
    pub id: String,
    pub keyword: String,
    pub max_pages: u32,
    pub status: String,
    pub item_count: usize,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub created_at: u64,
}

impl From<CrawlTask> for TaskResponse {
    fn from(t: CrawlTask) -> Self {
        Self {
            id: t.id,
            keyword: t.keyword.as_str().to_string(),
            max_pages: t.max_pages.value(),
            status: match t.status {
                TaskStatus::Pending => "pending",
                TaskStatus::Running => "running",
                TaskStatus::Done => "done",
                TaskStatus::Failed => "failed",
            }
            .to_string(),
            item_count: t.item_count,
            error: t.error,
            created_at: t.created_at,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ItemResponse {
    pub id: String,
    pub title: String,
    pub price: f64,
    pub seller: String,
    pub url: String,
    #[ts(type = "number")]
    pub crawled_at: u64,
}

impl From<Item> for ItemResponse {
    fn from(it: Item) -> Self {
        Self {
            id: it.id,
            title: it.title,
            price: it.price,
            seller: it.seller,
            url: it.url,
            crawled_at: it.crawled_at,
        }
    }
}

// ---------- 标签 ----------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TagCreateRequest {
    pub name: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TagUpdateRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TagResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub remark: Option<String>,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
}

impl From<Tag> for TagResponse {
    fn from(t: Tag) -> Self {
        Self {
            id: t.id,
            name: t.name.as_str().to_string(),
            enabled: t.enabled,
            remark: t.remark,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

// ---------- 待爬取商品 ----------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductCreateRequest {
    pub name: String,
    /// 不传或空数组 = 无标签
    #[ts(type = "Array<number> | null")]
    pub tag_ids: Option<Vec<i64>>,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductUpdateRequest {
    pub name: Option<String>,
    /// 不传=不修改，空数组=清空全部标签，非空数组=整体替换
    #[ts(type = "Array<number> | null")]
    pub tag_ids: Option<Vec<i64>>,
    pub remark: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "Array<number>")]
    pub tag_ids: Vec<i64>,
    /// 标签名列表，与 tag_ids 一一对应；无标签时为空数组
    pub tag_names: Vec<String>,
    pub remark: Option<String>,
    pub median_price: Option<f64>,
    pub avg_price: Option<f64>,
    pub crawled_count: Option<u32>,
    #[ts(type = "number | null")]
    pub last_crawled_at: Option<u64>,
    pub recycle_price: Option<f64>,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
}

impl ProductResponse {
    /// tag_names 由 handler 通过 TagService 解析后传入
    pub fn from_product(p: Product, tag_names: Vec<String>) -> Self {
        Self {
            id: p.id,
            name: p.name.as_str().to_string(),
            tag_ids: p.tag_ids,
            tag_names,
            remark: p.remark,
            median_price: p.median_price,
            avg_price: p.avg_price,
            crawled_count: p.crawled_count,
            last_crawled_at: p.last_crawled_at,
            recycle_price: p.recycle_price,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductBriefResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
}

// ---------- 抓取队列 ----------

#[derive(Debug, Deserialize, Default, TS)]
#[ts(export)]
pub struct SelectorDto {
    #[serde(default)]
    #[ts(type = "Array<number>")]
    pub tag_all: Vec<i64>,
    #[serde(default)]
    #[ts(type = "Array<number>")]
    pub tag_any: Vec<i64>,
    #[serde(default)]
    #[ts(type = "Array<number>")]
    pub tag_exclude: Vec<i64>,
    pub stale_days: Option<u32>,
}

impl From<SelectorDto> for Selector {
    fn from(d: SelectorDto) -> Self {
        Self {
            tag_all: d.tag_all,
            tag_any: d.tag_any,
            tag_exclude: d.tag_exclude,
            stale_days: d.stale_days,
        }
    }
}

/// 预览请求：选择器或商品 id 列表，二选一
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PreviewRequest {
    pub selector: Option<SelectorDto>,
    #[ts(type = "Array<number> | null")]
    pub product_ids: Option<Vec<i64>>,
}

/// 入队/追加请求：目标 + 间隔（追加时间隔忽略）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct EnqueueRequest {
    pub selector: Option<SelectorDto>,
    #[ts(type = "Array<number> | null")]
    pub product_ids: Option<Vec<i64>>,
    pub interval_secs: Option<u32>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PreviewResponse {
    pub to_add: Vec<ProductBriefResponse>,
    pub skipped: Vec<ProductBriefResponse>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct QueueResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub status: String,
    pub interval_secs: u32,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number | null")]
    pub finished_at: Option<u64>,
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl From<QueueProgress> for QueueResponse {
    fn from(p: QueueProgress) -> Self {
        Self {
            id: p.queue.id,
            status: p.queue.status.as_str().to_string(),
            interval_secs: p.queue.interval_secs,
            created_at: p.queue.created_at,
            finished_at: p.queue.finished_at,
            total: p.total,
            pending: p.pending,
            running: p.running,
            done: p.done,
            failed: p.failed,
            skipped: p.skipped,
        }
    }
}

/// 入队/追加响应：队列 + 明细
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct EnqueueResponse {
    #[ts(type = "number")]
    pub queue_id: i64,
    pub status: String,
    pub added: Vec<ProductBriefResponse>,
    pub skipped: Vec<ProductBriefResponse>,
}

// ---------- AI 配置 ----------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AiProviderCreateRequest {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

#[derive(Debug, Deserialize, Default, TS)]
#[ts(export)]
pub struct AiProviderUpdateRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub timeout_secs: Option<u32>,
    pub max_retries: Option<u32>,
}

fn default_timeout_secs() -> u32 {
    60
}

fn default_max_retries() -> u32 {
    2
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiProviderResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub base_url: String,
    /// 掩码后的密钥，未设置时为 null
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u32,
    pub max_retries: u32,
    pub is_default: bool,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
}

impl From<AiProvider> for AiProviderResponse {
    fn from(p: AiProvider) -> Self {
        let api_key = p.masked_key();
        Self {
            id: p.id,
            name: p.name,
            base_url: p.base_url,
            api_key,
            model: p.model,
            timeout_secs: p.timeout_secs,
            max_retries: p.max_retries,
            is_default: p.is_default,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TestConnectionResponse {
    #[ts(type = "number")]
    pub latency_ms: u64,
    pub reply: String,
}

impl From<TestConnectionResult> for TestConnectionResponse {
    fn from(r: TestConnectionResult) -> Self {
        Self {
            latency_ms: r.latency_ms,
            reply: r.reply,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiStatusResponse {
    pub configured: bool,
    pub source: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
}

impl From<AiStatus> for AiStatusResponse {
    fn from(s: AiStatus) -> Self {
        Self {
            configured: s.configured,
            source: s.source,
            name: s.name,
            model: s.model,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiToolCallResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub tool_name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    #[ts(type = "number")]
    pub created_at: u64,
}

impl From<AiToolCall> for AiToolCallResponse {
    fn from(c: AiToolCall) -> Self {
        Self {
            id: c.id,
            tool_name: c.tool_name,
            arguments: c.arguments,
            result: c.result,
            error: c.error,
            duration_ms: c.duration_ms,
            created_at: c.created_at,
        }
    }
}

/// 统一 API 响应结构（泛型包装，TS 侧在 web/src/types/api.ts 手写，不经 ts-rs 导出）
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            message: "ok".into(),
            data: Some(data),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            code: -1,
            message: message.into(),
            data: None,
        }
    }
}

// ---------- 分页与全局统计 ----------

/// 通用分页查询参数（page 从 1 开始）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PageQuery {
    #[ts(type = "number | null")]
    pub page: Option<u64>,
    #[ts(type = "number | null")]
    pub page_size: Option<u64>,
}

/// 商品列表查询参数：分页 + 服务端排序
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductListQuery {
    #[ts(type = "number | null")]
    pub page: Option<u64>,
    #[ts(type = "number | null")]
    pub page_size: Option<u64>,
    /// 白名单：median_price / avg_price / crawled_count / last_crawled_at / recycle_price
    pub sort_by: Option<String>,
    /// asc / desc（默认 desc）
    pub sort_dir: Option<String>,
}

/// 分页响应（泛型包装，TS 侧在 web/src/types/api.ts 手写，不经 ts-rs 导出）
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(items: Vec<T>, total: u64, page: u64, page_size: u64) -> Self {
        Self {
            items,
            total,
            page,
            page_size,
        }
    }
}

/// KPI 概览统计（与列表分页解耦）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct StatsResponse {
    #[ts(type = "number")]
    pub product_count: u64,
    #[ts(type = "number | null")]
    pub last_crawled_at: Option<u64>,
    /// 近 24 小时抓取数量
    #[ts(type = "number")]
    pub crawled_today: u64,
}

// ---------- 批量导入 ----------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductBatchCreateRequest {
    pub names: Vec<String>,
    #[ts(type = "Array<number> | null")]
    pub tag_ids: Option<Vec<i64>>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductBatchCreateResponse {
    pub created: Vec<ProductResponse>,
    pub skipped: Vec<BatchSkippedItem>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct BatchSkippedItem {
    pub name: String,
    pub reason: String,
}

// ---------- AI 自动打标签 ----------

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ClassifyProductsRequest {
    #[ts(type = "Array<number>")]
    pub product_ids: Vec<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClassifyProductsResponse {
    pub summary: String,
    pub suggestions: Vec<ClassifySuggestionDto>,
    pub warnings: Vec<String>,
}

impl From<ClassifySyncResult> for ClassifyProductsResponse {
    fn from(r: ClassifySyncResult) -> Self {
        Self {
            summary: r.summary,
            suggestions: r.suggestions.into_iter().map(Into::into).collect(),
            warnings: r.warnings,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClassifySuggestionDto {
    #[ts(type = "number")]
    pub product_id: i64,
    #[ts(type = "Array<number>")]
    pub tag_ids: Vec<i64>,
}

impl From<ClassifySuggestion> for ClassifySuggestionDto {
    fn from(s: ClassifySuggestion) -> Self {
        Self {
            product_id: s.product_id,
            tag_ids: s.tag_ids,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClassifyTaskResponse {
    pub id: String,
    pub status: String,
    pub total: usize,
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub progress_pct: f64,
    pub warnings: Vec<ClassifyWarningDto>,
    pub current_batch: usize,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number | null")]
    pub finished_at: Option<u64>,
}

impl From<AiClassifyTask> for ClassifyTaskResponse {
    fn from(t: AiClassifyTask) -> Self {
        let pct = t.progress_pct();
        let id = t.id;
        let status = t.status.as_str().to_string();
        let total = t.total;
        let processed = t.processed;
        let succeeded = t.succeeded;
        let failed = t.failed;
        let warnings = t.warnings.into_iter().map(Into::into).collect();
        let current_batch = t.current_batch;
        let error = t.error;
        let created_at = t.created_at;
        let finished_at = t.finished_at;
        Self {
            id,
            status,
            total,
            processed,
            succeeded,
            failed,
            progress_pct: pct,
            warnings,
            current_batch,
            error,
            created_at,
            finished_at,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ClassifyWarningDto {
    #[ts(type = "number")]
    pub product_id: i64,
    pub message: String,
}

impl From<ClassifyWarning> for ClassifyWarningDto {
    fn from(w: ClassifyWarning) -> Self {
        Self {
            product_id: w.product_id,
            message: w.message,
        }
    }
}
