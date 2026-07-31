//! 应用层的端口（port）：闲鱼数据网关。
//! 由 application 定义契约，infrastructure 提供实现（防腐层）。

use async_trait::async_trait;

use crate::domain::error::DomainError;
use crate::domain::item::Item;

/// 闲鱼数据网关：抽象「按关键词搜索一页商品」的能力。
/// 实现方负责登录态、签名、HTML/JSON 解析等易变细节。
#[async_trait]
pub trait XianYuGateway: Send + Sync {
    async fn search(&self, keyword: &str, page: u32) -> Result<Vec<Item>, DomainError>;
}
