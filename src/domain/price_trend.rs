#[derive(Debug, Clone)]
pub struct PriceTrendPoint {
    pub crawled_at: u64,
    pub median_price: f64,
    pub min_price: f64,
    pub max_price: f64,
    pub avg_price: f64,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct PriceTrendSeries {
    pub product_id: i64,
    pub product_name: String,
    pub points: Vec<PriceTrendPoint>,
}
