//! 仓储端口（trait）：domain 定义契约，infrastructure 提供实现。

use async_trait::async_trait;

use super::ai_classify_task::AiClassifyTask;
use super::ai_provider::{AiProvider, NewAiProvider};
use super::ai_tool_call::{AiToolCall, NewAiToolCall};
use super::crawl_queue::{CrawlEntry, CrawlQueue, QueueStatus};
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

#[async_trait]
pub trait QueueRepository: Send + Sync {
    async fn create_queue(&self, queue: &CrawlQueue) -> Result<CrawlQueue, DomainError>;
    async fn add_entries(&self, queue_id: i64, product_ids: &[i64]) -> Result<(), DomainError>;
    async fn find_queue(&self, id: i64) -> Result<Option<CrawlQueue>, DomainError>;
    async fn list_queues(&self) -> Result<Vec<CrawlQueue>, DomainError>;
    async fn update_queue(&self, queue: &CrawlQueue) -> Result<(), DomainError>;
    async fn list_entries(&self, queue_id: i64) -> Result<Vec<CrawlEntry>, DomainError>;
    async fn next_pending_entry(&self, queue_id: i64) -> Result<Option<CrawlEntry>, DomainError>;
    async fn update_entry(&self, entry: &CrawlEntry) -> Result<(), DomainError>;
    /// 未结束队列（waiting/running/paused）中 pending/running 条目的商品 id（全局去重）
    async fn queued_product_ids(&self) -> Result<Vec<i64>, DomainError>;
    /// 当前 running 的队列；启动恢复时数据库里可能有多个，取创建最早的
    async fn current_running_queue(&self) -> Result<Option<CrawlQueue>, DomainError>;
    /// 创建最早的 waiting 队列（worker 提升用）
    async fn oldest_waiting_queue(&self) -> Result<Option<CrawlQueue>, DomainError>;
    /// 按状态筛选队列（全部暂停/恢复用），按创建时间升序
    async fn list_by_status(&self, statuses: &[QueueStatus]) -> Result<Vec<CrawlQueue>, DomainError>;
    /// 重启恢复：所有 running 状态的条目重置为 pending
    async fn reset_running_entries(&self) -> Result<(), DomainError>;
    /// 删除队列及其全部条目，返回是否真的删除了记录
    async fn delete_queue(&self, id: i64) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait AiProviderRepository: Send + Sync {
    /// 创建配置并返回带 id 的完整实体
    async fn create(&self, provider: &NewAiProvider) -> Result<AiProvider, DomainError>;
    async fn find(&self, id: i64) -> Result<Option<AiProvider>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<AiProvider>, DomainError>;
    async fn list(&self) -> Result<Vec<AiProvider>, DomainError>;
    async fn update(&self, provider: &AiProvider) -> Result<(), DomainError>;
    /// 返回是否真的删除了记录
    async fn delete(&self, id: i64) -> Result<bool, DomainError>;
    /// 当前默认配置（is_default = 1），最多一条
    async fn find_default(&self) -> Result<Option<AiProvider>, DomainError>;
    /// 清掉全部默认标记（set_default 前先清后设）
    async fn clear_default(&self) -> Result<(), DomainError>;
}

#[async_trait]
pub trait AiToolCallRepository: Send + Sync {
    /// 落一条工具调用审计记录并返回带 id 的完整实体
    async fn create(&self, call: &NewAiToolCall) -> Result<AiToolCall, DomainError>;
    /// 最近的调用记录，按时间倒序
    async fn list_recent(&self, limit: u32) -> Result<Vec<AiToolCall>, DomainError>;
}

#[async_trait]
pub trait AiClassifyTaskRepository: Send + Sync {
    async fn save(&self, task: &AiClassifyTask) -> Result<(), DomainError>;
    async fn find(&self, id: &str) -> Result<Option<AiClassifyTask>, DomainError>;
}
