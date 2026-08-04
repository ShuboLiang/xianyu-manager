//! 用例：待爬取商品的增删改查。

use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::product::{NewProduct, Product, ProductName};
use crate::domain::repository::{
    ItemRepository, Page, ProductRepository, ProductSortColumn, QueueRepository, TagRepository,
};

/// 单次批量导入的数量上限
const BATCH_CREATE_LIMIT: usize = 1000;

/// 批量创建结果：创建成功的商品 + 被跳过的条目（名称, 原因）
#[derive(Debug)]
pub struct BatchCreateResult {
    pub created: Vec<Product>,
    pub skipped: Vec<(String, String)>,
}

/// 更新商品的补丁：None 表示不修改该字段。
/// `tag_ids` 传 Some(vec![]) 表示清空全部标签，Some(ids) 表示整体替换。
/// `recycle_price` 是双层 Option：None=不修改，Some(None)=清空，Some(Some(v))=手动设定。
#[derive(Debug, Default)]
pub struct ProductPatch {
    pub name: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
    pub remark: Option<String>,
    pub recycle_price: Option<Option<f64>>,
}

/// 按标签批量删除的预览结果：命中商品 + 其中处于活跃队列的数量
#[derive(Debug)]
pub struct BatchDeletePreview {
    pub products: Vec<Product>,
    pub in_active_queues: u64,
}

/// 勾选批量删除的预览结果：实际存在的商品数 + 前 10 条名称样本 + 活跃队列占用数
#[derive(Debug)]
pub struct BatchDeleteByIdsPreview {
    pub total: u64,
    pub sample: Vec<String>,
    pub in_active_queues: u64,
}

pub struct ProductService {
    products: Arc<dyn ProductRepository>,
    tags: Arc<dyn TagRepository>,
    items: Arc<dyn ItemRepository>,
    queues: Arc<dyn QueueRepository>,
}

impl ProductService {
    pub fn new(
        products: Arc<dyn ProductRepository>,
        tags: Arc<dyn TagRepository>,
        items: Arc<dyn ItemRepository>,
        queues: Arc<dyn QueueRepository>,
    ) -> Self {
        Self {
            products,
            tags,
            items,
            queues,
        }
    }

    pub async fn create_product(
        &self,
        name: String,
        tag_ids: Vec<i64>,
        remark: Option<String>,
    ) -> Result<Product, DomainError> {
        let name = ProductName::new(name)?;
        self.ensure_name_available(name.as_str(), None).await?;
        self.ensure_tags_exist(&tag_ids).await?;
        let new_product = NewProduct {
            name,
            tag_ids,
            remark: normalize_remark(remark),
        };
        self.products.create(&new_product).await
    }

    pub async fn batch_create(
        &self,
        names: Vec<String>,
        tag_ids: Vec<i64>,
    ) -> Result<BatchCreateResult, DomainError> {
        if names.len() > BATCH_CREATE_LIMIT {
            return Err(DomainError::InvalidInput(format!(
                "单次最多 {} 条，当前 {} 条",
                BATCH_CREATE_LIMIT,
                names.len()
            )));
        }

        self.ensure_tags_exist(&tag_ids).await?;

        let mut created: Vec<Product> = Vec::new();
        let mut skipped: Vec<(String, String)> = Vec::new();

        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for raw in &names {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }

            if seen.contains(trimmed) {
                skipped.push((trimmed.to_string(), "本批次内重复".into()));
                continue;
            }
            seen.insert(trimmed.to_string());

            let name = match ProductName::new(trimmed) {
                Ok(n) => n,
                Err(e) => {
                    skipped.push((trimmed.to_string(), format!("校验失败: {e}")));
                    continue;
                }
            };

            if let Some(existing) = self.products.find_by_name(name.as_str()).await? {
                skipped.push((name.as_str().to_string(), "商品名已存在".into()));
                drop(existing);
                continue;
            }

            let new_product = NewProduct {
                name,
                tag_ids: tag_ids.clone(),
                remark: None,
            };

            match self.products.create(&new_product).await {
                Ok(p) => created.push(p),
                Err(e) => {
                    skipped.push((new_product.name.as_str().to_string(), format!("创建失败: {e}")));
                }
            }
        }

        Ok(BatchCreateResult { created, skipped })
    }

    pub async fn get_product(&self, id: i64) -> Result<Product, DomainError> {
        self.products
            .find(id)
            .await?
            .ok_or_else(|| DomainError::NotFound(format!("商品 {id}")))
    }

    /// 分页查询（page 从 1 开始，调用方已完成钳制）；sort_by 非法时返回 InvalidInput；search 为可选模糊匹配；tag_id 可选按标签过滤
    pub async fn list_paginated(
        &self,
        page: u64,
        page_size: u64,
        sort_by: Option<String>,
        sort_dir: Option<String>,
        search: Option<String>,
        tag_id: Option<i64>,
    ) -> Result<Page<Product>, DomainError> {
        let sort = match sort_by {
            Some(s) => {
                let col = ProductSortColumn::parse(&s)
                    .ok_or_else(|| DomainError::InvalidInput(format!("不支持的排序字段: {s}")))?;
                let desc = sort_dir.as_deref() != Some("asc");
                Some((col, desc))
            }
            None => None,
        };
        let search_str = search.as_deref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        });
        self.products
            .list_paginated((page - 1) * page_size, page_size, sort, search_str, tag_id)
            .await
    }

    /// 导出全部商品（不分页，用于 Excel 导出）
    pub async fn list_all(&self) -> Result<Vec<Product>, DomainError> {
        self.products.list().await
    }

    /// 使用某个标签的全部商品（删除标签前的影响提示）
    pub async fn list_by_tag(&self, tag_id: i64) -> Result<Vec<Product>, DomainError> {
        self.products.list_by_tag(tag_id).await
    }

    pub async fn update_product(
        &self,
        id: i64,
        patch: ProductPatch,
    ) -> Result<Product, DomainError> {
        let mut product = self.get_product(id).await?;

        if patch.name.is_some() || patch.tag_ids.is_some() || patch.remark.is_some() {
            let name = match patch.name {
                Some(n) => ProductName::new(n)?,
                None => product.name.clone(),
            };
            // 改名时校验新名字不与其他商品冲突
            if name != product.name {
                self.ensure_name_available(name.as_str(), Some(id)).await?;
            }
            let tag_ids = match patch.tag_ids {
                Some(ids) => {
                    self.ensure_tags_exist(&ids).await?;
                    ids
                }
                None => product.tag_ids.clone(),
            };
            let remark = match patch.remark {
                Some(r) => normalize_remark(Some(r)),
                None => product.remark.clone(),
            };
            product.update_info(name, tag_ids, remark);
        }

        // 回收价手动设置/清空（校验在实体方法里）
        if let Some(recycle_price) = patch.recycle_price {
            product.set_recycle_price(recycle_price)?;
        }

        self.products.update(&product).await?;
        Ok(product)
    }

    pub async fn delete_product(&self, id: i64) -> Result<(), DomainError> {
        // 先解除抓取记录的归属，避免 items.product_id 悬空
        self.items.detach_product(&[id]).await?;
        if !self.products.delete(id).await? {
            return Err(DomainError::NotFound(format!("商品 {id}")));
        }
        Ok(())
    }

    /// 预览「删除某标签下全部商品」：命中商品 + 其中处于活跃队列的数量（仅提示，不阻止）
    pub async fn preview_batch_delete_by_tag(
        &self,
        tag_id: i64,
    ) -> Result<BatchDeletePreview, DomainError> {
        if self.tags.find(tag_id).await?.is_none() {
            return Err(DomainError::NotFound(format!("标签 {tag_id}")));
        }
        let products = self.products.list_by_tag(tag_id).await?;
        let queued: std::collections::HashSet<i64> =
            self.queues.queued_product_ids().await?.into_iter().collect();
        let in_active_queues = products.iter().filter(|p| queued.contains(&p.id)).count() as u64;
        Ok(BatchDeletePreview {
            products,
            in_active_queues,
        })
    }

    /// 删除某标签下全部商品，返回删除条数。
    /// 抓取历史（items）保留，仅解除归属；活跃队列中的条目由 worker 标记 skipped 兜底。
    pub async fn batch_delete_by_tag(&self, tag_id: i64) -> Result<u64, DomainError> {
        if self.tags.find(tag_id).await?.is_none() {
            return Err(DomainError::NotFound(format!("标签 {tag_id}")));
        }
        let products = self.products.list_by_tag(tag_id).await?;
        let ids: Vec<i64> = products.iter().map(|p| p.id).collect();
        if ids.is_empty() {
            return Ok(0);
        }
        self.items.detach_product(&ids).await?;
        self.products.delete_by_ids(&ids).await
    }

    /// 预览「勾选批量删除」：实际存在的商品数 + 前 10 条名称样本 + 其中处于活跃队列的数量
    pub async fn preview_batch_delete_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<BatchDeleteByIdsPreview, DomainError> {
        if ids.is_empty() {
            return Err(DomainError::InvalidInput("请先勾选要删除的商品".into()));
        }
        let products = self.products.list_by_ids(ids).await?;
        let queued: std::collections::HashSet<i64> =
            self.queues.queued_product_ids().await?.into_iter().collect();
        let in_active_queues = products.iter().filter(|p| queued.contains(&p.id)).count() as u64;
        Ok(BatchDeleteByIdsPreview {
            total: products.len() as u64,
            sample: products.iter().take(10).map(|p| p.name.as_str().to_string()).collect(),
            in_active_queues,
        })
    }

    /// 勾选批量删除：按 id 列表删除，返回实际删除条数。
    /// 抓取历史（items）保留，仅解除归属；活跃队列中的条目由 worker 标记 skipped 兜底。
    pub async fn batch_delete_by_ids(&self, ids: &[i64]) -> Result<u64, DomainError> {
        if ids.is_empty() {
            return Err(DomainError::InvalidInput("请先勾选要删除的商品".into()));
        }
        self.items.detach_product(ids).await?;
        let deleted = self.products.delete_by_ids(ids).await?;
        tracing::info!("勾选批量删除商品：{deleted} 个");
        Ok(deleted)
    }

    /// 校验商品名未被占用；exclude_id 用于更新时排除自身
    async fn ensure_name_available(
        &self,
        name: &str,
        exclude_id: Option<i64>,
    ) -> Result<(), DomainError> {
        if let Some(existing) = self.products.find_by_name(name).await? {
            if Some(existing.id) != exclude_id {
                return Err(DomainError::Conflict(format!("商品名「{name}」已存在")));
            }
        }
        Ok(())
    }

    /// 校验所有指定的标签都存在
    async fn ensure_tags_exist(&self, tag_ids: &[i64]) -> Result<(), DomainError> {
        for id in tag_ids {
            if self.tags.find(*id).await?.is_none() {
                return Err(DomainError::NotFound(format!("标签 {id}")));
            }
        }
        Ok(())
    }
}

/// 空白的备注视为没有备注
fn normalize_remark(remark: Option<String>) -> Option<String> {
    remark.and_then(|r| {
        let r = r.trim().to_string();
        if r.is_empty() { None } else { Some(r) }
    })
}
