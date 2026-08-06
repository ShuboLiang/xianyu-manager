//! HTTP 请求/响应 DTO：与 domain 模型解耦，serde 注解只属于这一层。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::application::ai_provider_service::{AiStatus, TestConnectionResult};
use crate::application::ai::classify_service::{ClassifySuggestion, ClassifySyncResult};
use crate::application::queue_service::QueueProgress;
use crate::domain::ai_classify_task::{AiClassifyTask, ClassifyWarning};
use crate::domain::ai_conversation::{Conversation, ConversationMessage};
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
    #[ts(type = "number | null")]
    pub product_id: Option<i64>,
    pub product_name: Option<String>,
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
            product_id: it.product_id,
            product_name: None,
        }
    }
}

/// 抓取数据批量删除请求：search 与列表搜索同语义，空 = 清空全部
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ItemBatchDeleteRequest {
    pub search: Option<String>,
}

/// 抓取数据批量删除预览：命中总数 + 标题样本（前 10 条）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ItemBatchDeletePreviewResponse {
    #[ts(type = "number")]
    pub total: u64,
    pub sample: Vec<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ItemBatchDeleteResponse {
    #[ts(type = "number")]
    pub deleted: u64,
}

/// 抓取数据勾选批量删除请求（按 id 列表；id 是详情页 URL）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ItemBatchDeleteIdsRequest {
    pub ids: Vec<String>,
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
    /// 回收价（元）：不传=不修改，null=清空，数值=手动设定（下一轮爬取会覆盖）
    #[serde(default)]
    #[ts(type = "number | null")]
    pub recycle_price: Option<Option<f64>>,
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
    /// 常见价位（自适应档宽分档众数的档位下界；档宽按量级：<100→10，<1000→50，<10000→100，≥10000→500）
    pub mode_price: Option<f64>,
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
            mode_price: p.mode_price,
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
    pub name: String,
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
            name: p.queue.name,
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

/// 队列改名请求
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RenameQueueRequest {
    pub name: String,
}

/// 历史队列清理请求：before_days 与 keep_latest 恰填一个（service 层校验）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct QueuePurgeRequest {
    /// 清理 N 天前结束的队列（0 = 清空全部历史）
    #[ts(type = "number | null")]
    pub before_days: Option<u32>,
    /// 仅保留最近结束的 N 条
    #[ts(type = "number | null")]
    pub keep_latest: Option<u64>,
}

/// 历史队列清理预览/结果：命中（或已删）的队列数与条目总数
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct QueuePurgeOutcomeResponse {
    #[ts(type = "number")]
    pub queues: u64,
    #[ts(type = "number")]
    pub entries: u64,
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
    /// 透传给 OpenAI 兼容端点的额外请求参数（JSON 对象字符串），
    /// 如 DeepSeek 关思考 `{"thinking": {"type": "disabled"}}`
    pub extra_params: Option<String>,
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
    /// None=不修改，空串=清空，非空=替换（必须是合法 JSON 对象）
    pub extra_params: Option<String>,
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
    /// 额外请求参数（JSON 对象字符串），未设置为 null
    pub extra_params: Option<String>,
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
            name: p.name.as_str().to_string(),
            base_url: p.base_url.as_str().to_string(),
            api_key,
            model: p.model.as_str().to_string(),
            timeout_secs: p.timeout_secs,
            max_retries: p.max_retries,
            extra_params: p.extra_params.as_ref().map(|v| v.as_str().to_string()),
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

/// 用户自定义抓取提示词（作用于 AI 抓取的筛选与回收价定价）
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CrawlPromptRequest {
    pub custom_prompt: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CrawlPromptResponse {
    pub custom_prompt: String,
}

/// AI 抓取模式响应：direct = 单轮调用（省 token），agent = ReAct 工具循环
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CrawlModeResponse {
    pub mode: String,
}

/// AI 抓取模式更新请求（service 层校验只允许 direct/agent）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateCrawlModeRequest {
    pub mode: String,
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
    /// LLM 调用的输入 token 数（纯工具行/供应商未上报为 null）
    #[ts(type = "number | null")]
    pub input_tokens: Option<u64>,
    /// LLM 调用的输出 token 数
    #[ts(type = "number | null")]
    pub output_tokens: Option<u64>,
    /// 命中供应商缓存的输入 token 数
    #[ts(type = "number | null")]
    pub cached_input_tokens: Option<u64>,
    #[ts(type = "number")]
    pub created_at: u64,
    /// 来源：assistant（AI 助手）/ crawl（AI 抓取）/ classify（AI 打标签）；老记录为 null
    pub source: Option<String>,
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
            input_tokens: c.input_tokens,
            output_tokens: c.output_tokens,
            cached_input_tokens: c.cached_input_tokens,
            created_at: c.created_at,
            source: c.source,
        }
    }
}

/// AI 工具调用记录列表查询参数：分页 + 工具名/来源/成败筛选
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AiToolCallListQuery {
    #[ts(type = "number | null")]
    pub page: Option<u64>,
    #[ts(type = "number | null")]
    pub page_size: Option<u64>,
    /// 工具名精确匹配，缺省 = 全部
    pub tool_name: Option<String>,
    /// true = 只看失败，false = 只看成功，缺省 = 全部
    #[ts(type = "boolean | null")]
    pub failed: Option<bool>,
    /// 来源精确匹配（assistant/crawl/classify），缺省 = 全部
    pub source: Option<String>,
}

/// AI 工具调用记录清理请求：before_days 与 keep_latest 恰填一个（service 层校验）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AiToolCallPurgeRequest {
    /// 删除 N 天前的记录（0 = 清空全部）
    #[ts(type = "number | null")]
    pub before_days: Option<u32>,
    /// 仅保留最新 N 条
    #[ts(type = "number | null")]
    pub keep_latest: Option<u64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiToolCallPurgePreviewResponse {
    /// 命中清理条件的记录数
    #[ts(type = "number")]
    pub matched: u64,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiToolCallPurgeResponse {
    /// 实际删除条数
    #[ts(type = "number")]
    pub deleted: u64,
}

/// 通用管理助手聊天请求
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AiChatRequest {
    pub message: String,
}

/// 通用管理助手聊天响应
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiChatResponse {
    pub reply: String,
}

/// 单个 AI 工具的清单（供外部智能体发现能力 + 前端「可用工具」抽屉展示）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AiToolInfoResponse {
    pub name: String,
    pub description: String,
    /// 参数的 JSON Schema
    #[ts(type = "any")]
    pub parameters: serde_json::Value,
    /// 当前是否启用（未被全局禁用）
    pub enabled: bool,
    /// 是否写操作（会真实改库）
    pub is_write: bool,
}

/// 全局禁用工具名单（整体替换；空数组 = 全部恢复）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ToolAvailabilityRequest {
    /// 禁用的工具名列表（未知工具名会被拒绝并列出）
    pub disabled_tools: Vec<String>,
}

/// AI 助手会话摘要（列表用）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConversationResponse {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    /// 消息数（用于列表展示）
    #[ts(type = "number")]
    pub message_count: u64,
    #[ts(type = "number")]
    pub created_at: u64,
    #[ts(type = "number")]
    pub updated_at: u64,
    /// 写操作确认模式：normal（需确认）/ yolo（全部放行）
    pub mode: String,
}

/// AI 助手会话消息
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConversationMessageResponse {
    #[ts(type = "number")]
    pub id: i64,
    /// "user" | "assistant"
    pub role: String,
    pub content: String,
    #[ts(type = "number")]
    pub created_at: u64,
}

/// AI 助手会话详情（会话 + 全部消息）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ConversationDetailResponse {
    pub conversation: ConversationResponse,
    pub messages: Vec<ConversationMessageResponse>,
}

/// AI 助手会话改名请求
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RenameConversationRequest {
    pub title: String,
}

/// 待用户确认的写操作审批（前端弹框展示）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PendingApprovalResponse {
    #[ts(type = "number")]
    pub id: u64,
    #[ts(type = "number")]
    pub conversation_id: i64,
    pub tool_name: String,
    /// 调用参数（JSON 字符串）
    pub arguments: String,
    #[ts(type = "number")]
    pub created_at: u64,
}

/// 待确认审批查询：某会话的待确认审批
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PendingApprovalQuery {
    #[ts(type = "number")]
    pub conversation_id: i64,
}

/// 用户对某条审批的决策
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalDecideRequest {
    /// allow_once（允许本次）/ allow_always（该对话全部允许）/ deny（拒绝）
    pub decision: String,
}

/// 会话确认模式切换请求
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ApprovalModeRequest {
    /// normal / yolo
    pub mode: String,
}

/// 会话确认模式响应
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ApprovalModeResponse {
    /// normal / yolo
    pub mode: String,
}

impl ConversationResponse {
    pub fn from_conversation(c: Conversation, message_count: u64, mode: String) -> Self {
        Self {
            id: c.id,
            title: c.title,
            message_count,
            created_at: c.created_at,
            updated_at: c.updated_at,
            mode,
        }
    }
}

impl From<ConversationMessage> for ConversationMessageResponse {
    fn from(m: ConversationMessage) -> Self {
        Self {
            id: m.id,
            role: m.role.as_str().to_string(),
            content: m.content,
            created_at: m.created_at,
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

/// 抓取数据列表查询参数：分页 + 商品名/标题模糊搜索
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ItemListQuery {
    #[ts(type = "number | null")]
    pub page: Option<u64>,
    #[ts(type = "number | null")]
    pub page_size: Option<u64>,
    /// 标题或商品名模糊搜索
    pub search: Option<String>,
    /// 标签筛选：只看挂在该标签下的商品的记录
    #[ts(type = "number | null")]
    pub tag_id: Option<i64>,
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
    /// 商品名模糊搜索
    pub search: Option<String>,
    /// 按标签过滤（标签 id）
    #[ts(type = "number | null")]
    pub tag_id: Option<i64>,
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

/// 按标签批量删除商品请求
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductBatchDeleteRequest {
    #[ts(type = "number")]
    pub tag_id: i64,
}

/// 批量删除预览：命中商品 + 其中处于活跃队列的数量（仅提示，不阻止）
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductBatchDeletePreviewResponse {
    pub total: usize,
    pub products: Vec<ProductBriefResponse>,
    #[ts(type = "number")]
    pub in_active_queues: u64,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductBatchDeleteResponse {
    #[ts(type = "number")]
    pub deleted: u64,
}

/// 商品勾选批量删除请求（按 id 列表）
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ProductBatchDeleteIdsRequest {
    #[ts(type = "Array<number>")]
    pub ids: Vec<i64>,
}

/// 商品勾选批量删除预览：实际存在的商品数 + 名称样本（前 10 条）+ 活跃队列占用数
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProductBatchDeleteIdsPreviewResponse {
    #[ts(type = "number")]
    pub total: u64,
    pub sample: Vec<String>,
    #[ts(type = "number")]
    pub in_active_queues: u64,
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

// ---------- 价格趋势图 ----------

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PriceTrendPoint {
    #[ts(type = "number")]
    pub crawled_at: u64,
    pub median_price: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub avg_price: f64,
    #[ts(type = "number")]
    pub count: u32,
}

impl From<crate::domain::price_trend::PriceTrendPoint> for PriceTrendPoint {
    fn from(p: crate::domain::price_trend::PriceTrendPoint) -> Self {
        Self {
            crawled_at: p.crawled_at,
            median_price: p.median_price,
            min_price: p.min_price,
            max_price: p.max_price,
            avg_price: p.avg_price,
            count: p.count,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PriceTrendSeries {
    #[ts(type = "number")]
    pub product_id: i64,
    pub product_name: String,
    pub points: Vec<PriceTrendPoint>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PriceTrendQuery {
    #[serde(default, deserialize_with = "deserialize_comma_separated_i64")]
    #[ts(type = "Array<number>")]
    pub product_ids: Vec<i64>,
}

fn deserialize_comma_separated_i64<'de, D>(deserializer: D) -> Result<Vec<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|part| part.trim().parse::<i64>().map_err(serde::de::Error::custom))
        .collect()
}
