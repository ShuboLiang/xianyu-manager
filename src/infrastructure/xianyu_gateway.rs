//! 闲鱼网关实现：Http（真实接口，待实现）与 Mock（开发用假数据）。

use async_trait::async_trait;
use tokio::time::Duration;

use crate::application::ports::XianYuGateway;
use crate::domain::crawl_task::now_unix;
use crate::domain::error::DomainError;
use crate::domain::item::Item;

/// 真实闲鱼接口实现。闲鱼接口需要登录态 Cookie 与请求签名（mtop），
/// 具体抓取逻辑后续在此实现，上层不受影响。
pub struct HttpXianYuGateway {
    #[allow(dead_code)]
    client: reqwest::Client,
    #[allow(dead_code)]
    cookie: Option<String>,
}

impl HttpXianYuGateway {
    pub fn new(cookie: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            cookie,
        }
    }
}

#[async_trait]
impl XianYuGateway for HttpXianYuGateway {
    async fn search(&self, _keyword: &str, _page: u32) -> Result<Vec<Item>, DomainError> {
        // TODO: 实现真实抓取（mtop 签名、列表解析）
        Err(DomainError::Infrastructure(
            "真实闲鱼接口尚未实现，请先用 GATEWAY=mock 运行".into(),
        ))
    }
}

/// 开发用假数据网关：按关键词生成示例商品，便于打通全链路。
pub struct MockXianYuGateway;

#[async_trait]
impl XianYuGateway for MockXianYuGateway {
    async fn search(&self, keyword: &str, page: u32) -> Result<Vec<Item>, DomainError> {
        let now = now_unix();
        let items = (0..3)
            .map(|i| {
                let n = (page - 1) * 3 + i + 1;
                Item {
                    id: format!("mock-{keyword}-{n}"),
                    title: format!("{keyword} 示例商品 {n}"),
                    price: 99.0 + n as f64,
                    seller: format!("卖家{n}"),
                    url: format!("https://www.goofish.com/item?id=mock-{n}"),
                    crawled_at: now,
                    product_id: None,
                }
            })
            .collect();
        // 模拟单次抓取耗时约 5 秒
        tokio::time::sleep(Duration::from_secs(5)).await;
        Ok(items)
    }
}
