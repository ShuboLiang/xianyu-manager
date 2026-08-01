//! 待爬取商品实体：管理「要爬哪些商品」。
//! 基础信息（名称/标签/备注）由用户维护；统计字段（中位数、均价、爬取数量、
//! 最后爬取时间）只在爬取完成后写入，未爬取时为空。
//! 回收价默认由爬取结果计算，也允许用户手动设置/清空（下一轮爬取会覆盖）。

use super::crawl_task::now_unix;
use super::error::DomainError;

/// 商品名（值对象）：非空、去除首尾空白、不超过 64 字符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductName(String);

impl ProductName {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("商品名不能为空".into()));
        }
        if s.chars().count() > 64 {
            return Err(DomainError::InvalidInput("商品名过长（>64 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 创建商品的入参（尚无 id）
#[derive(Debug, Clone)]
pub struct NewProduct {
    pub name: ProductName,
    /// 所属标签 id 列表，空 Vec 表示无标签（默认）
    pub tag_ids: Vec<i64>,
    pub remark: Option<String>,
}

/// 待爬取商品（实体）
#[derive(Debug, Clone)]
pub struct Product {
    pub id: i64,
    pub name: ProductName,
    /// 所属标签 id 列表，空 Vec 表示无标签（多对多，存 product_tags 关联表）
    pub tag_ids: Vec<i64>,
    pub remark: Option<String>,
    // ---- 以下字段由爬取结果填充，未爬取时为 None ----
    /// 价格中位数（元）
    pub median_price: Option<f64>,
    /// 价格均值（元）
    pub avg_price: Option<f64>,
    /// 最近一次爬取到的商品数量
    pub crawled_count: Option<u32>,
    /// 最后爬取时间，Unix 秒
    pub last_crawled_at: Option<u64>,
    /// 回收价格（元）
    pub recycle_price: Option<f64>,
    // ----
    /// Unix 秒
    pub created_at: u64,
    pub updated_at: u64,
}

impl Product {
    pub fn update_info(
        &mut self,
        name: ProductName,
        tag_ids: Vec<i64>,
        remark: Option<String>,
    ) {
        self.name = name;
        self.tag_ids = tag_ids;
        self.remark = remark;
        self.touch();
    }

    /// 写入一次爬取的统计结果（爬虫流程对接时调用）
    #[allow(dead_code)]
    pub fn record_crawl_result(
        &mut self,
        median_price: f64,
        avg_price: f64,
        crawled_count: u32,
        recycle_price: f64,
    ) {
        self.median_price = Some(median_price);
        self.avg_price = Some(avg_price);
        self.crawled_count = Some(crawled_count);
        self.recycle_price = Some(recycle_price);
        self.last_crawled_at = Some(now_unix());
        self.touch();
    }

    /// 手动设置/清空回收价（元）：Some 必须是正的有限值，None = 清空。
    /// 下一轮爬取完成时会按计算结果覆盖手动值。
    pub fn set_recycle_price(&mut self, price: Option<f64>) -> Result<(), DomainError> {
        if let Some(p) = price {
            if !p.is_finite() || p <= 0.0 {
                return Err(DomainError::InvalidInput("回收价必须为正数".into()));
            }
        }
        self.recycle_price = price;
        self.touch();
        Ok(())
    }

    fn touch(&mut self) {
        self.updated_at = now_unix();
    }
}
