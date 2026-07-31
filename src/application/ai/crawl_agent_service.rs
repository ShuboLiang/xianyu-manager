//! AI 驱动的商品抓取用例：一个商品 = 一轮 ReAct agent。
//!
//! 流程：AI 调用 `xianyu_search` 搜索候选（底层走 WebBridge 真实浏览器）
//! → AI 从候选中挑选最多 8 个「描述最匹配、质量最高」的有效商品
//! → AI 调用 `save_crawl_result` 提交，工具内计算中位数/均价/回收价（中位数 × 系数）
//! 并写库。工具调用全程落 ai_tool_calls 审计表（由 AiGateway 实现方负责）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::application::ports::{AiGateway, AiTool, XianYuGateway};
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::product::Product;
use crate::domain::repository::{ItemRepository, ProductRepository};

/// AI 单次最多提交的有效商品数
pub const MAX_SELECTED: usize = 8;
/// agent 最大轮数：搜索（含换关键词重试）+ 提交 + 总结，留足容错
const MAX_ROUNDS: u32 = 10;

/// 一次抓取落库的统计结果（由 save_crawl_result 工具写入，供 worker 读取）
#[derive(Debug, Clone, Copy)]
pub struct CrawlOutcome {
    pub median_price: f64,
    pub avg_price: f64,
    pub count: u32,
    pub recycle_price: f64,
}

pub struct CrawlAgentService {
    gateway: Arc<dyn XianYuGateway>,
    products: Arc<dyn ProductRepository>,
    items: Arc<dyn ItemRepository>,
    ai: Arc<dyn AiGateway>,
    recycle_factor: f64,
}

impl CrawlAgentService {
    pub fn new(
        gateway: Arc<dyn XianYuGateway>,
        products: Arc<dyn ProductRepository>,
        items: Arc<dyn ItemRepository>,
        ai: Arc<dyn AiGateway>,
        recycle_factor: f64,
    ) -> Self {
        Self {
            gateway,
            products,
            items,
            ai,
            recycle_factor,
        }
    }

    /// 抓取一个商品：跑一轮 AI agent，返回落库的统计结果
    pub async fn crawl_product(&self, product: &Product) -> Result<CrawlOutcome, DomainError> {
        // 工具执行与 agent 循环在同一线程串行，std Mutex 即可（不在 await 间持锁）
        let outcome: Arc<Mutex<Option<CrawlOutcome>>> = Arc::new(Mutex::new(None));

        let tools: Vec<Arc<dyn AiTool>> = vec![
            Arc::new(XianyuSearchTool::new(
                self.gateway.clone(),
                product.name.as_str().to_string(),
            )),
            Arc::new(SaveCrawlResultTool::new(
                self.products.clone(),
                self.items.clone(),
                product.id,
                self.recycle_factor,
                outcome.clone(),
            )),
        ];

        let system = "你是二手行情采集助手。你的任务是为指定商品估算二手行情：\n\
            1. 调用 xianyu_search 搜索商品（可修正关键词使其更贴近真实在售标题，如去掉多余空格、补充型号）；\n\
            2. 从返回的候选中挑选最多 8 个有效商品：标题描述必须与目标商品高度匹配（同型号/同规格），\
               剔除配件、求购帖、明显不相关、价格异常（远高于或低于正常行情）的条目；\
               同等匹配度下优先选描述信息更完整、卖家信息更全的；\n\
            3. 调用 save_crawl_result 提交你选中的商品（原样回传 title/price/seller/url）。\n\
            必须用 save_crawl_result 提交后才算完成，不要只输出文字总结。";

        let user = format!(
            "目标商品：{}\n请搜索、筛选并提交最多 {MAX_SELECTED} 个有效在售商品。",
            product.name.as_str()
        );

        self.ai.run_agent(system, &user, &tools, MAX_ROUNDS).await?;

        let taken = outcome.lock().expect("outcome 锁中毒").take();
        taken.ok_or_else(|| {
            DomainError::InvalidState("AI 未提交抓取结果（save_crawl_result 未被调用）".into())
        })
    }
}

// ---------- AI 工具 ----------

/// `xianyu_search`：按关键词搜索闲鱼在售商品（经 WebBridge 真实浏览器）
struct XianyuSearchTool {
    gateway: Arc<dyn XianYuGateway>,
    default_keyword: String,
}

impl XianyuSearchTool {
    fn new(gateway: Arc<dyn XianYuGateway>, default_keyword: String) -> Self {
        Self {
            gateway,
            default_keyword,
        }
    }
}

#[async_trait]
impl AiTool for XianyuSearchTool {
    fn name(&self) -> &str {
        "xianyu_search"
    }

    fn description(&self) -> &str {
        "搜索闲鱼在售商品，返回候选列表（title/price/seller/url）。\
         不传 keyword 时用目标商品名搜索；候选质量差时可修正关键词再搜一次。"
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "keyword": {
                    "type": "string",
                    "description": "搜索关键词，省略时用目标商品名"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let keyword = args
            .get("keyword")
            .and_then(|k| k.as_str())
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .unwrap_or(&self.default_keyword);
        let items = self.gateway.search(keyword, 1).await?;
        let list: Vec<JsonValue> = items
            .iter()
            .map(|i| {
                serde_json::json!({
                    "title": i.title,
                    "price": i.price,
                    "seller": i.seller,
                    "url": i.url,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "keyword": keyword,
            "count": list.len(),
            "items": list,
        }))
    }
}

/// `save_crawl_result`：提交 AI 选中的有效商品，计算统计并落库
struct SaveCrawlResultTool {
    products: Arc<dyn ProductRepository>,
    items: Arc<dyn ItemRepository>,
    product_id: i64,
    recycle_factor: f64,
    outcome: Arc<Mutex<Option<CrawlOutcome>>>,
}

impl SaveCrawlResultTool {
    fn new(
        products: Arc<dyn ProductRepository>,
        items: Arc<dyn ItemRepository>,
        product_id: i64,
        recycle_factor: f64,
        outcome: Arc<Mutex<Option<CrawlOutcome>>>,
    ) -> Self {
        Self {
            products,
            items,
            product_id,
            recycle_factor,
            outcome,
        }
    }
}

#[async_trait]
impl AiTool for SaveCrawlResultTool {
    fn name(&self) -> &str {
        "save_crawl_result"
    }

    fn description(&self) -> &str {
        "提交你筛选出的有效商品（1-8 个），系统会计算价格中位数/均价/回收价并写入数据库。\
         items 中每条必须原样包含 title/price/seller/url。提交后本商品抓取即完成。"
    }

    fn parameters_schema(&self) -> JsonValue {
        serde_json::json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_SELECTED,
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "price": { "type": "number", "exclusiveMinimum": 0 },
                            "seller": { "type": "string" },
                            "url": { "type": "string" }
                        },
                        "required": ["title", "price", "url"]
                    }
                }
            },
            "required": ["items"]
        })
    }

    async fn execute(&self, args: JsonValue) -> Result<JsonValue, DomainError> {
        let arr = args
            .get("items")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DomainError::InvalidInput("items 必须是数组".into()))?;
        if arr.is_empty() || arr.len() > MAX_SELECTED {
            return Err(DomainError::InvalidInput(format!(
                "items 数量需在 1..={MAX_SELECTED} 之间，收到 {}",
                arr.len()
            )));
        }

        let now = now_unix();
        let mut items: Vec<Item> = Vec::with_capacity(arr.len());
        for (i, v) in arr.iter().enumerate() {
            let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").trim();
            let price = v.get("price").and_then(|p| p.as_f64()).unwrap_or(0.0);
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or("").trim();
            let seller = v
                .get("seller")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() || price <= 0.0 || url.is_empty() {
                return Err(DomainError::InvalidInput(format!(
                    "第 {} 条数据无效（title/url 不能为空，price 必须 > 0）",
                    i + 1
                )));
            }
            items.push(Item {
                id: url.to_string(),
                title: title.to_string(),
                price,
                seller,
                url: url.to_string(),
                crawled_at: now,
            });
        }

        // 统计：中位数 / 均价 / 回收价（中位数 × 系数，默认 0.9）
        let mut prices: Vec<f64> = items.iter().map(|i| i.price).collect();
        prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = prices.len();
        let median = if count % 2 == 1 {
            prices[count / 2]
        } else {
            (prices[count / 2 - 1] + prices[count / 2]) / 2.0
        };
        let avg = prices.iter().sum::<f64>() / count as f64;
        let recycle = median * self.recycle_factor;

        let mut product = self
            .products
            .find(self.product_id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("商品 {}", self.product_id)))?;
        product.record_crawl_result(median, avg, count as u32, recycle);
        self.products.update(&product).await?;
        let _ = self.items.save_all(&items).await;

        let result = CrawlOutcome {
            median_price: median,
            avg_price: avg,
            count: count as u32,
            recycle_price: recycle,
        };
        *self.outcome.lock().expect("outcome 锁中毒") = Some(result);

        tracing::info!(
            "商品 {} 抓取完成：{count} 条有效，中位数 {median:.2}，回收价 {recycle:.2}",
            self.product_id
        );
        Ok(serde_json::json!({
            "saved": true,
            "count": count,
            "median_price": median,
            "avg_price": avg,
            "recycle_price": recycle,
        }))
    }
}
