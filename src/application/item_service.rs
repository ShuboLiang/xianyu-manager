//! 用例：商品列表查询。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::item::Item;
use crate::domain::repository::ItemRepository;

pub struct ItemService {
    items: Arc<dyn ItemRepository>,
}

impl ItemService {
    pub fn new(items: Arc<dyn ItemRepository>) -> Self {
        Self { items }
    }

    pub async fn list_items(&self) -> Result<Vec<Item>, DomainError> {
        let mut items = self.items.list().await?;
        // 最新抓取的排在前面
        items.sort_by(|a, b| b.crawled_at.cmp(&a.crawled_at));
        Ok(items)
    }
}
