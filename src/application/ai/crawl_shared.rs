//! AI 驱动抓取的共享件：抓取器端口、统计结果、统计计算与落库。
//!
//! 两种抓取实现共用同一套「算统计 + 写库」逻辑：
//! - crawl_agent_service：ReAct agent 循环（AI 调 xianyu_search / save_crawl_result 工具）
//! - crawl_direct_service：单轮调用（Rust 直搜 → AI 一次筛选 → Rust 落库，省 token）

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::product::{mode_price, Product};
use crate::domain::repository::{ItemRepository, ProductRepository};

/// AI 单次最多提交的有效商品数
pub const MAX_SELECTED: usize = 8;

/// 一次抓取落库的统计结果（供 worker 读取）
#[derive(Debug, Clone, Copy)]
pub struct CrawlOutcome {
    pub median_price: f64,
    pub avg_price: f64,
    /// 常见价位（自适应档宽分档众数）
    pub mode_price: Option<f64>,
    pub count: u32,
    pub recycle_price: f64,
}

/// 商品抓取器端口：一个商品 = 一次抓取，返回落库的统计结果。
/// QueueService 只依赖该 trait，两种实现（agent / direct）可配置切换。
#[async_trait]
pub trait ProductCrawler: Send + Sync {
    /// AI 是否已配置（入队前预检）
    async fn check_ai_available(&self) -> bool;
    /// 抓取一个商品并落库统计
    async fn crawl_product(&self, product: &Product) -> Result<CrawlOutcome, DomainError>;
}

/// 统计计算 + 落库：中位数 / 均价 / 常见价位 / 回收价（中位数 × 系数）。
/// items 必须非空；调用方负责先把 product_id / crawled_at 填好。
pub async fn finalize_crawl(
    products: &Arc<dyn ProductRepository>,
    items_repo: &Arc<dyn ItemRepository>,
    product_id: i64,
    items: &[Item],
    recycle_factor: f64,
) -> Result<CrawlOutcome, DomainError> {
    let mut prices: Vec<f64> = items.iter().map(|i| i.price).collect();
    prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = prices.len();
    if count == 0 {
        return Err(DomainError::InvalidInput("抓取结果为空，无法计算统计".into()));
    }
    let median = if count % 2 == 1 {
        prices[count / 2]
    } else {
        (prices[count / 2 - 1] + prices[count / 2]) / 2.0
    };
    let avg = prices.iter().sum::<f64>() / count as f64;
    // 常见价位：自适应档宽分档众数（原始众数对连续价格没有意义）
    let mode = mode_price(&prices);
    let recycle = (median * recycle_factor).floor();

    let mut product = products
        .find(product_id)
        .await?
        .ok_or_else(|| DomainError::NotFound(format!("商品 {product_id}")))?;
    product.record_crawl_result(median, avg, mode, count as u32, recycle);
    products.update(&product).await?;
    let _ = items_repo.save_all(items).await;

    Ok(CrawlOutcome {
        median_price: median,
        avg_price: avg,
        mode_price: mode,
        count: count as u32,
        recycle_price: recycle,
    })
}
