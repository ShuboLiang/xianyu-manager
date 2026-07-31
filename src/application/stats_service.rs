//! 用例：KPI 概览统计。与列表分页解耦——列表分页后前端无法再从列表数据推导全局统计。

use std::sync::Arc;

use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::repository::{ItemRepository, ProductRepository};

/// 全局统计（队列相关指标仍由前端从队列全量列表推导，不在此处）
pub struct Stats {
    pub product_count: u64,
    pub last_crawled_at: Option<u64>,
    /// 近 24 小时抓取数量（无法跨平台取本地零点，用滚动 24h 窗口）
    pub crawled_today: u64,
}

pub struct StatsService {
    items: Arc<dyn ItemRepository>,
    products: Arc<dyn ProductRepository>,
}

impl StatsService {
    pub fn new(items: Arc<dyn ItemRepository>, products: Arc<dyn ProductRepository>) -> Self {
        Self { items, products }
    }

    pub async fn stats(&self) -> Result<Stats, DomainError> {
        let day_ago = now_unix().saturating_sub(86_400);
        Ok(Stats {
            product_count: self.products.count().await?,
            last_crawled_at: self.products.max_last_crawled_at().await?,
            crawled_today: self.items.count_since(day_ago).await?,
        })
    }
}
