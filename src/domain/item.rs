//! 商品实体与相关值对象。

use super::error::DomainError;

/// 搜索关键词（值对象）：非空、去除首尾空白
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword(String);

impl Keyword {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            return Err(DomainError::InvalidInput("关键词不能为空".into()));
        }
        if s.chars().count() > 64 {
            return Err(DomainError::InvalidInput("关键词过长（>64 字符）".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 抓取页数范围（值对象）：1..=50
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange(u32);

impl PageRange {
    pub const MAX_PAGES: u32 = 50;

    pub fn new(n: u32) -> Result<Self, DomainError> {
        if n == 0 || n > Self::MAX_PAGES {
            return Err(DomainError::InvalidInput(format!(
                "页数需在 1..={} 之间，收到 {n}",
                Self::MAX_PAGES
            )));
        }
        Ok(Self(n))
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

/// 闲鱼商品条目（实体）
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub title: String,
    /// 单位：元
    pub price: f64,
    pub seller: String,
    pub url: String,
    /// 抓取时间，Unix 秒（同一轮抓取的条目共享同一时间戳，可作为批次标识）
    pub crawled_at: u64,
    /// 本次抓取服务的待爬取商品 id（队列/AI 抓取路径写入；关键词直搜等无归属场景为 None）
    pub product_id: Option<i64>,
}
