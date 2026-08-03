//! 内存仓储实现：抓取任务、AI 分类任务（重启即丢失，可接受）。
//! ItemRepository 的内存实现保留为备选（默认已切到 SqliteItemRepository）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::ai_classify_task::AiClassifyTask;
use crate::domain::crawl_task::CrawlTask;
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::repository::{
    AiClassifyTaskRepository, CrawlTaskRepository, ItemRepository, Page,
};

#[allow(dead_code)]
#[derive(Default)]
pub struct InMemoryItemRepository {
    items: Mutex<Vec<Item>>,
}

#[allow(dead_code)]
#[async_trait]
impl ItemRepository for InMemoryItemRepository {
    async fn save_all(&self, items: &[Item]) -> Result<(), DomainError> {
        let mut guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        guard.extend_from_slice(items);
        Ok(())
    }

    async fn list_paginated(&self, offset: u64, limit: u64, search: Option<&str>) -> Result<Page<Item>, DomainError> {
        let guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let mut all: Vec<Item> = match search {
            Some(q) => guard
                .iter()
                .filter(|i| i.title.contains(q) || i.product_id.is_some_and(|_| false))
                .cloned()
                .collect(),
            None => guard.clone(),
        };
        all.sort_by(|a, b| b.crawled_at.cmp(&a.crawled_at));
        let total = all.len() as u64;
        let items = all
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok(Page { items, total })
    }

    async fn count_since(&self, unix_ts: u64) -> Result<u64, DomainError> {
        let guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(guard.iter().filter(|i| i.crawled_at >= unix_ts).count() as u64)
    }

    async fn list_latest_for_product(&self, product_id: i64) -> Result<Vec<Item>, DomainError> {
        let guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let latest = guard
            .iter()
            .filter(|i| i.product_id == Some(product_id))
            .map(|i| i.crawled_at)
            .max();
        let Some(ts) = latest else {
            return Ok(Vec::new());
        };
        let mut items: Vec<Item> = guard
            .iter()
            .filter(|i| i.product_id == Some(product_id) && i.crawled_at == ts)
            .cloned()
            .collect();
        items.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal));
        Ok(items)
    }

    async fn list_by_product(&self, product_id: i64) -> Result<Vec<Item>, DomainError> {
        let guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let mut items: Vec<Item> = guard
            .iter()
            .filter(|i| i.product_id == Some(product_id))
            .cloned()
            .collect();
        items.sort_by_key(|i| i.crawled_at);
        Ok(items)
    }

    async fn detach_product(&self, product_ids: &[i64]) -> Result<(), DomainError> {
        let mut guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        for item in guard.iter_mut() {
            if item.product_id.is_some_and(|pid| product_ids.contains(&pid)) {
                item.product_id = None;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryCrawlTaskRepository {
    tasks: Mutex<HashMap<String, CrawlTask>>,
}

#[async_trait]
impl CrawlTaskRepository for InMemoryCrawlTaskRepository {
    async fn save(&self, task: &CrawlTask) -> Result<(), DomainError> {
        let mut guard = self
            .tasks
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        guard.insert(task.id.clone(), task.clone());
        Ok(())
    }

    async fn find(&self, id: &str) -> Result<Option<CrawlTask>, DomainError> {
        let guard = self
            .tasks
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(guard.get(id).cloned())
    }
}

#[derive(Default)]
pub struct InMemoryAiClassifyTaskRepository {
    tasks: Mutex<HashMap<String, AiClassifyTask>>,
}

#[async_trait]
impl AiClassifyTaskRepository for InMemoryAiClassifyTaskRepository {
    async fn save(&self, task: &AiClassifyTask) -> Result<(), DomainError> {
        let mut guard = self
            .tasks
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        guard.insert(task.id.clone(), task.clone());
        Ok(())
    }

    async fn find(&self, id: &str) -> Result<Option<AiClassifyTask>, DomainError> {
        let guard = self
            .tasks
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(guard.get(id).cloned())
    }
}
