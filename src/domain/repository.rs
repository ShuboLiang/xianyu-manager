//! 仓储端口（trait）：domain 定义契约，infrastructure 提供实现。

use async_trait::async_trait;

use super::crawl_task::CrawlTask;
use super::error::DomainError;
use super::item::Item;

#[async_trait]
pub trait ItemRepository: Send + Sync {
    async fn save_all(&self, items: &[Item]) -> Result<(), DomainError>;
    async fn list(&self) -> Result<Vec<Item>, DomainError>;
}

#[async_trait]
pub trait CrawlTaskRepository: Send + Sync {
    async fn save(&self, task: &CrawlTask) -> Result<(), DomainError>;
    async fn find(&self, id: &str) -> Result<Option<CrawlTask>, DomainError>;
}
