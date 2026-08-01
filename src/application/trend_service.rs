use std::sync::Arc;

use crate::domain::error::DomainError;
use crate::domain::price_trend::{PriceTrendPoint, PriceTrendSeries};
use crate::domain::repository::{ItemRepository, ProductRepository};

pub struct TrendService {
    items: Arc<dyn ItemRepository>,
    products: Arc<dyn ProductRepository>,
}

impl TrendService {
    pub fn new(items: Arc<dyn ItemRepository>, products: Arc<dyn ProductRepository>) -> Self {
        Self { items, products }
    }

    pub async fn compute(
        &self,
        product_ids: &[i64],
    ) -> Result<Vec<PriceTrendSeries>, DomainError> {
        let mut series = Vec::new();

        for &pid in product_ids {
            let product = match self.products.find(pid).await? {
                Some(p) => p,
                None => continue,
            };
            let items = self.items.list_by_product(pid).await?;
            let points = compute_product_trend(&items);
            series.push(PriceTrendSeries {
                product_id: pid,
                product_name: product.name.as_str().to_string(),
                points,
            });
        }

        Ok(series)
    }
}

fn compute_product_trend(items: &[crate::domain::item::Item]) -> Vec<PriceTrendPoint> {
    let mut groups: Vec<(u64, Vec<f64>)> = Vec::new();
    for item in items {
        match groups.last_mut() {
            Some((ts, prices)) if *ts == item.crawled_at => {
                prices.push(item.price);
            }
            _ => {
                groups.push((item.crawled_at, vec![item.price]));
            }
        }
    }

    groups
        .into_iter()
        .map(|(ts, mut prices)| {
            prices.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let count = prices.len() as u32;
            let median = if count % 2 == 1 {
                prices[count as usize / 2]
            } else if count > 0 {
                let mid = count as usize / 2;
                (prices[mid - 1] + prices[mid]) / 2.0
            } else {
                0.0
            };
            let sum: f64 = prices.iter().sum();
            PriceTrendPoint {
                crawled_at: ts,
                median_price: median,
                min_price: prices.first().copied().unwrap_or(0.0),
                max_price: prices.last().copied().unwrap_or(0.0),
                avg_price: if count > 0 { sum / count as f64 } else { 0.0 },
                count,
            }
        })
        .collect()
}