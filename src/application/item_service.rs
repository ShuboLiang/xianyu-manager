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
}
