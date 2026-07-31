//! 仓储端口（trait）：domain 定义契约，infrastructure 提供实现。

use async_trait::async_trait;

use super::crawl_task::CrawlTask;
use super::error::DomainError;
use super::item::Item;
use super::product::{NewProduct, Product};
use super::tag::{NewTag, Tag};

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

#[async_trait]
pub trait TagRepository: Send + Sync {
    /// 创建标签并返回带 id 的完整实体
    async fn create(&self, tag: &NewTag) -> Result<Tag, DomainError>;
    async fn find(&self, id: i64) -> Result<Option<Tag>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Tag>, DomainError>;
    async fn list(&self) -> Result<Vec<Tag>, DomainError>;
    async fn update(&self, tag: &Tag) -> Result<(), DomainError>;
    /// 返回是否真的删除了记录
    async fn delete(&self, id: i64) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait ProductRepository: Send + Sync {
    /// 创建商品并返回带 id 的完整实体
    async fn create(&self, product: &NewProduct) -> Result<Product, DomainError>;
    async fn find(&self, id: i64) -> Result<Option<Product>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Product>, DomainError>;
    async fn list(&self) -> Result<Vec<Product>, DomainError>;
    /// 使用某个标签的全部商品（用于删除标签前的影响提示）
    async fn list_by_tag(&self, tag_id: i64) -> Result<Vec<Product>, DomainError>;
    async fn update(&self, product: &Product) -> Result<(), DomainError>;
    /// 返回是否真的删除了记录
    async fn delete(&self, id: i64) -> Result<bool, DomainError>;
}
