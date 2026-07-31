//! 内存仓储实现：骨架阶段使用，重启即丢失。
//! 后续接 SQLite 时新增 SqliteItemRepository 实现同样的 trait 即可。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::ai_classify_task::AiClassifyTask;
use crate::domain::crawl_task::CrawlTask;
use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::repository::{AiClassifyTaskRepository, CrawlTaskRepository, ItemRepository};

#[derive(Default)]
pub struct InMemoryItemRepository {
    items: Mutex<Vec<Item>>,
}

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

    async fn list(&self) -> Result<Vec<Item>, DomainError> {
        let guard = self
            .items
            .lock()
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(guard.clone())
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
