//! 用例：商品列表查询（分页）。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::repository::{ItemRepository, Page};

pub struct ItemService {
    items: Arc<dyn ItemRepository>,
}

impl ItemService {
    pub fn new(items: Arc<dyn ItemRepository>) -> Self {
        Self { items }
    }

    /// 按抓取时间倒序分页（page 从 1 开始，调用方已完成钳制）；search 为可选商品名/标题模糊搜索
    pub async fn list_paginated(
        &self,
        page: u64,
        page_size: u64,
        search: Option<String>,
    ) -> Result<Page<Item>, DomainError> {
        let search_str = search.as_deref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        });
        self.items
            .list_paginated((page - 1) * page_size, page_size, search_str)
            .await
    }

    /// 某商品最后一轮抓取的明细（前端「查看明细」弹窗用）
    pub async fn latest_for_product(&self, product_id: i64) -> Result<Vec<Item>, DomainError> {
        self.items.list_latest_for_product(product_id).await
    }

    pub async fn delete_item(&self, id: &str) -> Result<(), DomainError> {
        if !self.items.delete(id).await? {
            return Err(DomainError::NotFound(format!("抓取记录 {id}")));
        }
        Ok(())
    }

    /// 预览「按搜索条件批量删除」：命中总数 + 前几条标题样本
    pub async fn preview_delete_matching(
        &self,
        search: Option<String>,
    ) -> Result<ItemDeletePreview, DomainError> {
        let search_str = normalize_search(search.as_deref());
        let page = self.items.list_paginated(0, 10, search_str).await?;
        Ok(ItemDeletePreview {
            total: page.total,
            sample: page.items.into_iter().map(|i| i.title).collect(),
        })
    }

    /// 按搜索条件批量删除（None = 清空全部），返回删除条数
    pub async fn delete_matching(&self, search: Option<String>) -> Result<u64, DomainError> {
        self.items
            .delete_matching(normalize_search(search.as_deref()))
            .await
    }

    /// 预览「勾选批量删除」：实际存在的记录数 + 前 10 条标题样本
    pub async fn preview_delete_by_ids(&self, ids: &[String]) -> Result<ItemDeletePreview, DomainError> {
        if ids.is_empty() {
            return Err(DomainError::InvalidInput("请先勾选要删除的记录".into()));
        }
        let found = self.items.list_by_ids(ids).await?;
        Ok(ItemDeletePreview {
            total: found.len() as u64,
            sample: found.iter().take(10).map(|i| i.title.clone()).collect(),
        })
    }

    /// 勾选批量删除：按 id 列表删除，返回实际删除条数
    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<u64, DomainError> {
        if ids.is_empty() {
            return Err(DomainError::InvalidInput("请先勾选要删除的记录".into()));
        }
        let deleted = self.items.delete_by_ids(ids).await?;
        tracing::info!("勾选批量删除抓取记录：{deleted} 条");
        Ok(deleted)
    }
}

/// 批量删除预览：命中总数 + 标题样本
#[derive(Debug)]
pub struct ItemDeletePreview {
    pub total: u64,
    pub sample: Vec<String>,
}

/// 空白的搜索词视为没有搜索条件
fn normalize_search(search: Option<&str>) -> Option<&str> {
    search.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    })
}
