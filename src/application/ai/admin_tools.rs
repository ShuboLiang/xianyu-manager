//! 后台管理 Agent 工具集：把前端各接口的能力（商品/标签/抓取记录/队列/统计）暴露成 AiTool，
//! 供通用管理助手（POST /api/ai/chat）或外部智能体调用。
//!
//! 设计要点：
//! - 工具直接复用 application 层的 service（业务规则不重复实现），不走 HTTP 层；
//! - 读工具尽量收敛结果（分页、字段裁剪），避免 AI 上下文被海量数据淹没；
//! - 写工具通过 description 明确副作用，删除类工具描述中要求先确认。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

use crate::application::item_service::ItemService;
use crate::application::ports::{AiGateway, AiTool};
use crate::application::product_service::{ProductPatch, ProductService};
use crate::application::queue_service::{EnqueueTarget, QueueProgress, QueueService};
use crate::application::stats_service::StatsService;
use crate::application::tag_service::{TagPatch, TagService};
use crate::application::trend_service::TrendService;
use crate::domain::crawl_queue::Selector;
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::product::Product;
use crate::domain::repository::Page;
use crate::domain::tag::Tag;

/// 管理 agent 最大轮数（读写链式操作可能较多，放宽到 12）
const MAX_ROUNDS: u32 = 12;

/// 管理助手系统提示词：角色 + 工具使用规则
const SYSTEM_PROMPT: &str = r#"你是一个闲鱼二手商品管理后台的 AI 助手，可以查询和操作后台数据（商品、标签、抓取记录、抓取队列、统计）。

使用规则：
1. 查询优先：需要任何数据时先调用对应的 list_/get_ 工具获取真实数据，不要凭空编造。
2. 写操作会真实落库：创建/更新/删除/入队前先想清楚，必要时先用查询工具确认对象 id 和现状。
3. 删除是破坏性操作：删除前必须先用 get_/list_ 工具确认对象存在，删除后如实说明结果。
4. 入队（enqueue）需要 selector（tag_all/tag_any/tag_exclude/stale_days）或 product_ids 之一。
5. 工具返回的是 JSON 数据；回复用户时用中文总结要点即可，不要原样复述大段 JSON。
6. 工具调用失败时如实说明错误原因，不要假装成功。"#;

// ---------- 管理服务 ----------

pub struct AdminToolsService {
    products: Arc<ProductService>,
    tags: Arc<TagService>,
    items: Arc<ItemService>,
    queues: Arc<QueueService>,
    stats: Arc<StatsService>,
    trend: Arc<TrendService>,
    ai_gateway: Arc<dyn AiGateway>,
}

impl AdminToolsService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        products: Arc<ProductService>,
        tags: Arc<TagService>,
        items: Arc<ItemService>,
        queues: Arc<QueueService>,
        stats: Arc<StatsService>,
        trend: Arc<TrendService>,
        ai_gateway: Arc<dyn AiGateway>,
    ) -> Self {
        Self {
            products,
            tags,
            items,
            queues,
            stats,
            trend,
            ai_gateway,
        }
    }

    /// 全部管理工具（一次构造，供 chat / 清单共用）
    pub fn tools(&self) -> Vec<Arc<dyn AiTool>> {
        vec![
            Arc::new(ListProductsTool::new(self.products.clone(), self.tags.clone())),
            Arc::new(GetProductTool::new(self.products.clone(), self.tags.clone())),
            Arc::new(CreateProductTool::new(self.products.clone(), self.tags.clone())),
            Arc::new(UpdateProductTool::new(self.products.clone(), self.tags.clone())),
            Arc::new(DeleteProductTool::new(self.products.clone())),
            Arc::new(BatchCreateProductsTool::new(self.products.clone())),
            Arc::new(ListTagsTool::new(self.tags.clone())),
            Arc::new(CreateTagTool::new(self.tags.clone())),
            Arc::new(UpdateTagTool::new(self.tags.clone())),
            Arc::new(DeleteTagTool::new(self.tags.clone())),
            Arc::new(ListItemsTool::new(self.items.clone(), self.products.clone())),
            Arc::new(GetStatsTool::new(self.stats.clone())),
            Arc::new(ListQueuesTool::new(self.queues.clone())),
            Arc::new(GetQueueTool::new(self.queues.clone())),
            Arc::new(EnqueueTool::new(self.queues.clone())),
            Arc::new(QueueActionTool::new(self.queues.clone(), QueueAction::Pause)),
            Arc::new(QueueActionTool::new(self.queues.clone(), QueueAction::Resume)),
            Arc::new(QueueActionTool::new(self.queues.clone(), QueueAction::Cancel)),
            Arc::new(GetPriceTrendTool::new(self.trend.clone())),
        ]
    }

    /// 工具清单（name/description/schema），供外部智能体发现能力
    pub fn tool_manifest(&self) -> Vec<ToolManifest> {
        self.tools()
            .iter()
            .map(|t| ToolManifest {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    /// 跑一轮通用管理 agent：用户指令 + 全部工具
    pub async fn chat(&self, user: &str) -> Result<String, DomainError> {
        if user.trim().is_empty() {
            return Err(DomainError::InvalidInput("消息不能为空".into()));
        }
        let tools = self.tools();
        self.ai_gateway
            .run_agent(SYSTEM_PROMPT, user, &tools, MAX_ROUNDS)
            .await
    }
}

/// 工具清单条目（供接口层组装 /api/ai/tools 响应）
#[derive(Debug, Clone)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

// ---------- 参数解析辅助 ----------

fn arg_i64(args: &JsonValue, key: &str) -> Option<i64> {
    args.get(key).and_then(|v| v.as_i64())
}

fn arg_u64(args: &JsonValue, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| v.as_u64())
}

fn arg_string(args: &JsonValue, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn arg_bool(args: &JsonValue, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn arg_i64_array(args: &JsonValue, key: &str) -> Option<Vec<i64>> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
}

fn arg_string_array(args: &JsonValue, key: &str) -> Option<Vec<String>> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
}

/// 分页钳制（与接口层同语义：page ≥ 1，page_size ∈ [1, 100]，默认 20）
fn clamp_page(page: Option<u64>, page_size: Option<u64>) -> (u64, u64) {
    (
        page.unwrap_or(1).max(1),
        page_size.unwrap_or(20).clamp(1, 100),
    )
}

fn product_to_json(p: &Product, tag_names: &HashMap<i64, String>) -> JsonValue {
    json!({
        "id": p.id,
        "name": p.name.as_str(),
        "tag_ids": p.tag_ids,
        "tag_names": p.tag_ids.iter().filter_map(|id| tag_names.get(id)).collect::<Vec<_>>(),
        "remark": p.remark,
        "median_price": p.median_price,
        "avg_price": p.avg_price,
        "mode_price": p.mode_price,
        "crawled_count": p.crawled_count,
        "last_crawled_at": p.last_crawled_at,
        "recycle_price": p.recycle_price,
    })
}

fn tag_to_json(t: &Tag) -> JsonValue {
    json!({
        "id": t.id,
        "name": t.name.as_str(),
        "enabled": t.enabled,
        "remark": t.remark,
    })
}

fn queue_to_json(p: &QueueProgress) -> JsonValue {
    json!({
        "id": p.queue.id,
        "status": p.queue.status.as_str(),
        "name": p.queue.name,
        "interval_secs": p.queue.interval_secs,
        "total": p.total,
        "pending": p.pending,
        "running": p.running,
        "done": p.done,
        "failed": p.failed,
        "skipped": p.skipped,
        "created_at": p.queue.created_at,
        "finished_at": p.queue.finished_at,
    })
}

fn item_to_json(it: &Item, product_names: &HashMap<i64, String>) -> JsonValue {
    json!({
        "id": it.id,
        "title": it.title,
        "price": it.price,
        "seller": it.seller,
        "url": it.url,
        "crawled_at": it.crawled_at,
        "product_id": it.product_id,
        "product_name": it.product_id.and_then(|pid| product_names.get(&pid)).cloned(),
    })
}

// ---------- 商品工具 ----------

pub struct ListProductsTool {
    products: Arc<ProductService>,
    tags: Arc<TagService>,
}

impl ListProductsTool {
    pub fn new(products: Arc<ProductService>, tags: Arc<TagService>) -> Self {
        Self { products, tags }
    }
}

#[async_trait]
impl AiTool for ListProductsTool {
    fn name(&self) -> &str {
        "list_products"
    }

    fn description(&self) -> &str {
        "分页查询待爬取商品列表。支持按名称模糊搜索（search）、按标签过滤（tag_id）、按字段排序（sort_by=median_price/avg_price/mode_price/crawled_count/last_crawled_at/recycle_price，sort_dir=asc/desc）。返回 total 与每页最多 page_size 条商品（含标签名与价格统计）。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "page": { "type": "integer", "description": "页码，从 1 开始，默认 1" },
                "page_size": { "type": "integer", "description": "每页条数，1-100，默认 20" },
                "search": { "type": "string", "description": "商品名模糊搜索" },
                "tag_id": { "type": "integer", "description": "按标签 id 过滤" },
                "sort_by": { "type": "string", "description": "排序字段，如 median_price" },
                "sort_dir": { "type": "string", "enum": ["asc", "desc"], "description": "排序方向，默认 desc" }
            }
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let (page, page_size) = clamp_page(arg_u64(&args, "page"), arg_u64(&args, "page_size"));
        let p: Page<Product> = self
            .products
            .list_paginated(
                page,
                page_size,
                arg_string(&args, "sort_by"),
                arg_string(&args, "sort_dir"),
                arg_string(&args, "search"),
                arg_i64(&args, "tag_id"),
            )
            .await?;
        let tag_names = tag_name_map(&self.tags).await;
        Ok(json!({
            "total": p.total,
            "page": page,
            "page_size": page_size,
            "items": p.items.iter().map(|prod| product_to_json(prod, &tag_names)).collect::<Vec<_>>(),
        }))
    }
}

pub struct GetProductTool {
    products: Arc<ProductService>,
    tags: Arc<TagService>,
}

impl GetProductTool {
    pub fn new(products: Arc<ProductService>, tags: Arc<TagService>) -> Self {
        Self { products, tags }
    }
}

#[async_trait]
impl AiTool for GetProductTool {
    fn name(&self) -> &str {
        "get_product"
    }

    fn description(&self) -> &str {
        "按 id 查询单个商品的完整信息（含标签、备注、价格统计、回收价）。商品不存在时返回错误。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "商品 id" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        let product = self.products.get_product(id).await?;
        let tag_names = tag_name_map(&self.tags).await;
        Ok(product_to_json(&product, &tag_names))
    }
}

pub struct CreateProductTool {
    products: Arc<ProductService>,
    tags: Arc<TagService>,
}

impl CreateProductTool {
    pub fn new(products: Arc<ProductService>, tags: Arc<TagService>) -> Self {
        Self { products, tags }
    }
}

#[async_trait]
impl AiTool for CreateProductTool {
    fn name(&self) -> &str {
        "create_product"
    }

    fn description(&self) -> &str {
        "创建待爬取商品。name 必填（全局唯一，重复会报错），tag_ids 可填标签 id 列表（必须来自 list_tags），remark 可选备注。返回创建后的商品信息。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "商品名称" },
                "tag_ids": { "type": "array", "items": { "type": "integer" }, "description": "标签 id 列表" },
                "remark": { "type": "string", "description": "备注" }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let name = arg_string(&args, "name").ok_or_else(|| DomainError::InvalidInput("缺少 name".into()))?;
        let tag_ids = arg_i64_array(&args, "tag_ids").unwrap_or_default();
        let product = self.products.create_product(name, tag_ids, arg_string(&args, "remark")).await?;
        let tag_names = tag_name_map(&self.tags).await;
        Ok(product_to_json(&product, &tag_names))
    }
}

pub struct UpdateProductTool {
    products: Arc<ProductService>,
    tags: Arc<TagService>,
}

impl UpdateProductTool {
    pub fn new(products: Arc<ProductService>, tags: Arc<TagService>) -> Self {
        Self { products, tags }
    }
}

#[async_trait]
impl AiTool for UpdateProductTool {
    fn name(&self) -> &str {
        "update_product"
    }

    fn description(&self) -> &str {
        "更新商品。id 必填；name/tag_ids/remark 只更新提供的字段；tag_ids 传空数组=清空全部标签，不传=不改；recycle_price 传数值=手动设定回收价，传 null=清空，不传=不改。返回更新后的商品信息。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "商品 id" },
                "name": { "type": "string", "description": "新名称" },
                "tag_ids": { "type": "array", "items": { "type": "integer" }, "description": "标签 id 列表（空数组=清空）" },
                "remark": { "type": "string", "description": "备注" },
                "recycle_price": { "type": ["number", "null"], "description": "手动回收价（元），null=清空" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        // recycle_price 三重语义：缺省=不修改，null=清空，数值=设定
        let recycle_price = match args.get("recycle_price") {
            None => None,
            Some(v) if v.is_null() => Some(None),
            Some(v) => Some(Some(v.as_f64().ok_or_else(|| {
                DomainError::InvalidInput("recycle_price 必须是数字或 null".into())
            })?)),
        };
        let patch = ProductPatch {
            name: arg_string(&args, "name"),
            tag_ids: arg_i64_array(&args, "tag_ids"),
            remark: arg_string(&args, "remark"),
            recycle_price,
        };
        let product = self.products.update_product(id, patch).await?;
        let tag_names = tag_name_map(&self.tags).await;
        Ok(product_to_json(&product, &tag_names))
    }
}

pub struct DeleteProductTool {
    products: Arc<ProductService>,
}

impl DeleteProductTool {
    pub fn new(products: Arc<ProductService>) -> Self {
        Self { products }
    }
}

#[async_trait]
impl AiTool for DeleteProductTool {
    fn name(&self) -> &str {
        "delete_product"
    }

    fn description(&self) -> &str {
        "删除商品（破坏性操作，删除前请先用 get_product 确认）。删除后该商品的抓取历史保留但解除归属。返回删除是否成功。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "商品 id" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        self.products.delete_product(id).await?;
        Ok(json!({ "deleted": true, "id": id }))
    }
}

pub struct BatchCreateProductsTool {
    products: Arc<ProductService>,
}

impl BatchCreateProductsTool {
    pub fn new(products: Arc<ProductService>) -> Self {
        Self { products }
    }
}

#[async_trait]
impl AiTool for BatchCreateProductsTool {
    fn name(&self) -> &str {
        "batch_create_products"
    }

    fn description(&self) -> &str {
        "批量导入商品。names 为商品名数组（每行一个），tag_ids 可选统一标签。返回 created（成功创建数）与 skipped（被跳过的名称及原因，如重名/校验失败）。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "names": { "type": "array", "items": { "type": "string" }, "description": "商品名列表" },
                "tag_ids": { "type": "array", "items": { "type": "integer" }, "description": "统一标签 id 列表" }
            },
            "required": ["names"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let names = arg_string_array(&args, "names")
            .ok_or_else(|| DomainError::InvalidInput("缺少 names".into()))?;
        let tag_ids = arg_i64_array(&args, "tag_ids").unwrap_or_default();
        let result = self.products.batch_create(names, tag_ids).await?;
        Ok(json!({
            "created": result.created.len(),
            "skipped": result.skipped.iter().map(|(name, reason)| json!({ "name": name, "reason": reason })).collect::<Vec<_>>(),
        }))
    }
}

// ---------- 标签工具 ----------

pub struct ListTagsTool {
    tags: Arc<TagService>,
}

impl ListTagsTool {
    pub fn new(tags: Arc<TagService>) -> Self {
        Self { tags }
    }
}

#[async_trait]
impl AiTool for ListTagsTool {
    fn name(&self) -> &str {
        "list_tags"
    }

    fn description(&self) -> &str {
        "查询全部商品标签（含启用状态与备注）。创建/更新商品时 tag_ids 必须从这里取。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _args: JsonValue) -> Result<JsonValue, DomainError> {
        let tags = self.tags.list_tags().await?;
        Ok(json!({
            "count": tags.len(),
            "tags": tags.iter().map(tag_to_json).collect::<Vec<_>>(),
        }))
    }
}

pub struct CreateTagTool {
    tags: Arc<TagService>,
}

impl CreateTagTool {
    pub fn new(tags: Arc<TagService>) -> Self {
        Self { tags }
    }
}

#[async_trait]
impl AiTool for CreateTagTool {
    fn name(&self) -> &str {
        "create_tag"
    }

    fn description(&self) -> &str {
        "创建商品标签。name 必填（全局唯一），remark 可选备注。新标签默认启用。返回创建后的标签。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "标签名称" },
                "remark": { "type": "string", "description": "备注" }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let name = arg_string(&args, "name").ok_or_else(|| DomainError::InvalidInput("缺少 name".into()))?;
        let tag = self.tags.create_tag(name, arg_string(&args, "remark")).await?;
        Ok(tag_to_json(&tag))
    }
}

pub struct UpdateTagTool {
    tags: Arc<TagService>,
}

impl UpdateTagTool {
    pub fn new(tags: Arc<TagService>) -> Self {
        Self { tags }
    }
}

#[async_trait]
impl AiTool for UpdateTagTool {
    fn name(&self) -> &str {
        "update_tag"
    }

    fn description(&self) -> &str {
        "更新标签。id 必填；name/enabled/remark 只更新提供的字段。enabled=false 表示停用该标签（不再参与抓取）。返回更新后的标签。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "标签 id" },
                "name": { "type": "string", "description": "新名称" },
                "enabled": { "type": "boolean", "description": "是否启用" },
                "remark": { "type": "string", "description": "备注" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        let patch = TagPatch {
            name: arg_string(&args, "name"),
            enabled: arg_bool(&args, "enabled"),
            remark: arg_string(&args, "remark"),
        };
        let tag = self.tags.update_tag(id, patch).await?;
        Ok(tag_to_json(&tag))
    }
}

pub struct DeleteTagTool {
    tags: Arc<TagService>,
}

impl DeleteTagTool {
    pub fn new(tags: Arc<TagService>) -> Self {
        Self { tags }
    }
}

#[async_trait]
impl AiTool for DeleteTagTool {
    fn name(&self) -> &str {
        "delete_tag"
    }

    fn description(&self) -> &str {
        "删除标签（破坏性操作）。删除后该标签与商品的关联被清除，商品本身不受影响。返回是否删除成功。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "标签 id" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        self.tags.delete_tag(id).await?;
        Ok(json!({ "deleted": true, "id": id }))
    }
}

// ---------- 抓取记录工具 ----------

pub struct ListItemsTool {
    items: Arc<ItemService>,
    products: Arc<ProductService>,
}

impl ListItemsTool {
    pub fn new(items: Arc<ItemService>, products: Arc<ProductService>) -> Self {
        Self { items, products }
    }
}

#[async_trait]
impl AiTool for ListItemsTool {
    fn name(&self) -> &str {
        "list_items"
    }

    fn description(&self) -> &str {
        "分页查询已抓取的闲鱼商品记录（按抓取时间倒序）。支持按标题/商品名模糊搜索（search）、按标签过滤（tag_id）。返回 total 与每页最多 page_size 条记录（含价格、卖家、链接、所属商品）。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "page": { "type": "integer", "description": "页码，从 1 开始，默认 1" },
                "page_size": { "type": "integer", "description": "每页条数，1-100，默认 20" },
                "search": { "type": "string", "description": "标题或商品名模糊搜索" },
                "tag_id": { "type": "integer", "description": "按标签 id 过滤" }
            }
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let (page, page_size) = clamp_page(arg_u64(&args, "page"), arg_u64(&args, "page_size"));
        let p: Page<Item> = self
            .items
            .list_paginated(page, page_size, arg_string(&args, "search"), arg_i64(&args, "tag_id"))
            .await?;
        let product_ids: Vec<i64> = p.items.iter().filter_map(|it| it.product_id).collect();
        let product_names = product_name_map(&self.products, &product_ids).await;
        Ok(json!({
            "total": p.total,
            "page": page,
            "page_size": page_size,
            "items": p.items.iter().map(|it| item_to_json(it, &product_names)).collect::<Vec<_>>(),
        }))
    }
}

// ---------- 统计工具 ----------

pub struct GetStatsTool {
    stats: Arc<StatsService>,
}

impl GetStatsTool {
    pub fn new(stats: Arc<StatsService>) -> Self {
        Self { stats }
    }
}

#[async_trait]
impl AiTool for GetStatsTool {
    fn name(&self) -> &str {
        "get_stats"
    }

    fn description(&self) -> &str {
        "查询后台 KPI 概览统计：商品总数、近 24 小时抓取数量、最后爬取时间。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _args: JsonValue) -> Result<JsonValue, DomainError> {
        let s = self.stats.stats().await?;
        Ok(json!({
            "product_count": s.product_count,
            "crawled_today": s.crawled_today,
            "last_crawled_at": s.last_crawled_at,
        }))
    }
}

// ---------- 队列工具 ----------

pub struct ListQueuesTool {
    queues: Arc<QueueService>,
}

impl ListQueuesTool {
    pub fn new(queues: Arc<QueueService>) -> Self {
        Self { queues }
    }
}

#[async_trait]
impl AiTool for ListQueuesTool {
    fn name(&self) -> &str {
        "list_queues"
    }

    fn description(&self) -> &str {
        "查询全部抓取队列及其进度（状态、名称、总条数、完成/失败/跳过数）。状态包括 waiting/running/paused/done/cancelled。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _args: JsonValue) -> Result<JsonValue, DomainError> {
        let list = self.queues.list_progress().await?;
        Ok(json!({
            "count": list.len(),
            "queues": list.iter().map(queue_to_json).collect::<Vec<_>>(),
        }))
    }
}

pub struct GetQueueTool {
    queues: Arc<QueueService>,
}

impl GetQueueTool {
    pub fn new(queues: Arc<QueueService>) -> Self {
        Self { queues }
    }
}

#[async_trait]
impl AiTool for GetQueueTool {
    fn name(&self) -> &str {
        "get_queue"
    }

    fn description(&self) -> &str {
        "按 id 查询单个抓取队列的进度。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "队列 id" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        let p = self.queues.get_progress(id).await?;
        Ok(queue_to_json(&p))
    }
}

pub struct EnqueueTool {
    queues: Arc<QueueService>,
}

impl EnqueueTool {
    pub fn new(queues: Arc<QueueService>) -> Self {
        Self { queues }
    }
}

#[async_trait]
impl AiTool for EnqueueTool {
    fn name(&self) -> &str {
        "enqueue"
    }

    fn description(&self) -> &str {
        "创建抓取队列并开始抓取。入队目标二选一：selector（按标签圈选：tag_all=必须全部包含，tag_any=至少包含其一，tag_exclude=排除，stale_days=距今未爬天数）或 product_ids（商品 id 列表）。interval_secs 为条间间隔（秒，默认 3）。已在队列中的商品会自动跳过。返回队列 id、状态、新增与跳过商品数。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "object",
                    "properties": {
                        "tag_all": { "type": "array", "items": { "type": "integer" } },
                        "tag_any": { "type": "array", "items": { "type": "integer" } },
                        "tag_exclude": { "type": "array", "items": { "type": "integer" } },
                        "stale_days": { "type": "integer", "description": "距今未爬天数" }
                    }
                },
                "product_ids": { "type": "array", "items": { "type": "integer" } },
                "interval_secs": { "type": "integer", "description": "条间间隔秒数，默认 3" }
            }
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let target = if let Some(sel) = args.get("selector") {
            EnqueueTarget::Selector(selector_from_json(sel))
        } else if let Some(ids) = arg_i64_array(&args, "product_ids") {
            EnqueueTarget::ProductIds(ids)
        } else {
            return Err(DomainError::InvalidInput(
                "必须提供 selector 或 product_ids 之一".into(),
            ));
        };
        let interval = arg_u64(&args, "interval_secs").unwrap_or(3) as u32;
        let (queue, preview) = self.queues.enqueue(target, interval).await?;
        Ok(json!({
            "queue_id": queue.id,
            "status": queue.status.as_str(),
            "added": preview.to_add.len(),
            "skipped": preview.skipped.len(),
        }))
    }
}

/// 队列动作：暂停/恢复/取消共用一个工具实现
pub enum QueueAction {
    Pause,
    Resume,
    Cancel,
}

pub struct QueueActionTool {
    queues: Arc<QueueService>,
    action: QueueAction,
}

impl QueueActionTool {
    pub fn new(queues: Arc<QueueService>, action: QueueAction) -> Self {
        Self { queues, action }
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            QueueAction::Pause => "pause_queue",
            QueueAction::Resume => "resume_queue",
            QueueAction::Cancel => "cancel_queue",
        }
    }
}

#[async_trait]
impl AiTool for QueueActionTool {
    fn name(&self) -> &str {
        self.action_name()
    }

    fn description(&self) -> &str {
        match self.action {
            QueueAction::Pause => "暂停队列（优雅暂停：当前条目跑完才停）。id 必填，返回暂停后的队列状态。",
            QueueAction::Resume => "恢复已暂停的队列（回到排队等待执行）。id 必填，返回恢复后的队列状态。",
            QueueAction::Cancel => "取消队列（终止执行并标记为已取消）。id 必填，返回取消后的队列状态。",
        }
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": "队列 id" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let id = arg_i64(&args, "id").ok_or_else(|| DomainError::InvalidInput("缺少 id".into()))?;
        let queue = match self.action {
            QueueAction::Pause => self.queues.pause(id).await?,
            QueueAction::Resume => self.queues.resume(id).await?,
            QueueAction::Cancel => self.queues.cancel(id).await?,
        };
        Ok(json!({
            "id": queue.id,
            "status": queue.status.as_str(),
            "name": queue.name,
        }))
    }
}

// ---------- 价格趋势工具 ----------

pub struct GetPriceTrendTool {
    trend: Arc<TrendService>,
}

impl GetPriceTrendTool {
    pub fn new(trend: Arc<TrendService>) -> Self {
        Self { trend }
    }
}

#[async_trait]
impl AiTool for GetPriceTrendTool {
    fn name(&self) -> &str {
        "get_price_trend"
    }

    fn description(&self) -> &str {
        "查询一个或多个商品的价格趋势：product_ids 传商品 id 列表，返回每个商品按抓取批次聚合的价格点（中位数/最低/最高/均价/样本数）。"
    }

    fn parameters_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "product_ids": { "type": "array", "items": { "type": "integer" }, "description": "商品 id 列表" }
            },
            "required": ["product_ids"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let ids = arg_i64_array(&args, "product_ids")
            .ok_or_else(|| DomainError::InvalidInput("缺少 product_ids".into()))?;
        let series = self.trend.compute(&ids).await?;
        Ok(json!({
            "series": series.iter().map(|s| json!({
                "product_id": s.product_id,
                "product_name": s.product_name,
                "points": s.points.iter().map(|p| json!({
                    "crawled_at": p.crawled_at,
                    "median_price": p.median_price,
                    "min_price": p.min_price,
                    "max_price": p.max_price,
                    "avg_price": p.avg_price,
                    "count": p.count,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))
    }
}

// ---------- 辅助函数 ----------

async fn tag_name_map(tags: &Arc<TagService>) -> HashMap<i64, String> {
    tags.list_tags()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|t| (t.id, t.name.as_str().to_string()))
        .collect()
}

async fn product_name_map(
    products: &Arc<ProductService>,
    ids: &[i64],
) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    for &id in ids {
        if let Ok(p) = products.get_product(id).await {
            map.insert(id, p.name.as_str().to_string());
        }
    }
    map
}

fn selector_from_json(v: &JsonValue) -> Selector {
    Selector {
        tag_all: arg_i64_array(v, "tag_all").unwrap_or_default(),
        tag_any: arg_i64_array(v, "tag_any").unwrap_or_default(),
        tag_exclude: arg_i64_array(v, "tag_exclude").unwrap_or_default(),
        stale_days: arg_u64(v, "stale_days").map(|n| n as u32),
    }
}
