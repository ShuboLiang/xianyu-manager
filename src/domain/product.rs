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
    /// 常见价位（元）：自适应档宽分档众数的档位下界，区间上界 = 值 + mode_bucket_width(值)
    pub mode_price: Option<f64>,
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
        mode_price: Option<f64>,
        crawled_count: u32,
        recycle_price: f64,
    ) {
        self.median_price = Some(median_price);
        self.avg_price = Some(avg_price);
        self.mode_price = mode_price;
        self.crawled_count = Some(crawled_count);
        self.recycle_price = Some(recycle_price.floor());
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

/// 档宽按价格量级自适应：便宜商品需要细档位（几十块的商品给 0–100 没意义），
/// 贵商品用粗档位避免样本被打散。档宽是下界的纯函数，展示层可直接推出区间上界。
pub fn mode_bucket_width(lower: f64) -> i64 {
    if lower < 100.0 {
        10
    } else if lower < 1000.0 {
        50
    } else if lower < 10000.0 {
        100
    } else {
        500
    }
}

/// 常见价位（自适应档宽分档众数）：价格按 mode_bucket_width 向下取整分档
/// （95 与 92 同属 90–100 档，1299 与 1201 同属 1200–1300 档），
/// 取商品数最多的档（并列时取较低档），返回该档的**下界**——
/// 展示区间为 [返回值, 返回值 + mode_bucket_width(返回值))。
/// 原始众数对连续价格没有意义（每个价格只出现一次），分档后才是
/// 「这类商品大家都挂多少钱」的有效近似。空输入返回 None。
pub fn mode_price(prices: &[f64]) -> Option<f64> {
    if prices.is_empty() {
        return None;
    }
    let mut buckets: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for p in prices {
        if !p.is_finite() || *p <= 0.0 {
            continue;
        }
        let w = mode_bucket_width(*p);
        *buckets.entry((p / w as f64).floor() as i64 * w).or_default() += 1;
    }
    buckets
        .into_iter()
        // 数量降序、档值升序：先比数量，并列取较低档
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(bucket, _)| bucket as f64)
}

#[cfg(test)]
mod tests {
    use super::{mode_bucket_width, mode_price};

    #[test]
    fn empty_prices_yield_none() {
        assert_eq!(mode_price(&[]), None);
    }

    #[test]
    fn cheap_items_use_ten_yuan_buckets() {
        // 95 / 92 同属 90–100 档（数量 2），胜过 80–90 档（数量 1）
        assert_eq!(mode_price(&[95.0, 92.0, 88.0]), Some(90.0));
    }

    #[test]
    fn floors_to_magnitude_bucket() {
        // 1299 / 1201 同属 1200–1300 档（数量 2），胜过 1400–1500 档（数量 1）
        assert_eq!(mode_price(&[1299.0, 1201.0, 1420.0]), Some(1200.0));
        // 千元以下是 50 元档：680 → 650–700 档
        assert_eq!(mode_price(&[680.0]), Some(650.0));
    }

    #[test]
    fn boundary_price_belongs_to_upper_bucket() {
        assert_eq!(mode_price(&[1300.0]), Some(1300.0));
        assert_eq!(mode_price(&[100.0]), Some(100.0));
    }

    #[test]
    fn tie_breaks_to_lower_bucket() {
        // 1200–1300 档与 1500–1600 档各 1 个，并列取较低档
        assert_eq!(mode_price(&[1250.0, 1580.0]), Some(1200.0));
    }

    #[test]
    fn ignores_non_positive_and_non_finite() {
        assert_eq!(mode_price(&[0.0, f64::NAN]), None);
    }

    #[test]
    fn bucket_width_follows_magnitude() {
        assert_eq!(mode_bucket_width(90.0), 10);
        assert_eq!(mode_bucket_width(100.0), 50);
        assert_eq!(mode_bucket_width(650.0), 50);
        assert_eq!(mode_bucket_width(1200.0), 100);
        assert_eq!(mode_bucket_width(10500.0), 500);
    }
}
