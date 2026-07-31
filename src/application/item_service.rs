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

    /// 按抓取时间倒序分页（page 从 1 开始，调用方已完成钳制）
    pub async fn list_paginated(&self, page: u64, page_size: u64) -> Result<Page<Item>, DomainError> {
        self.items
            .list_paginated((page - 1) * page_size, page_size)
            .await
    }
}
