//! HTTP 请求/响应 DTO：与 domain 模型解耦，serde 注解只属于这一层。

use serde::{Deserialize, Serialize};

use crate::domain::crawl_task::{CrawlTask, TaskStatus};
use crate::domain::item::Item;

#[derive(Debug, Deserialize)]
pub struct CrawlRequest {
    pub keyword: String,
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
}

fn default_max_pages() -> u32 {
    1
}

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub keyword: String,
    pub max_pages: u32,
    pub status: String,
    pub item_count: usize,
    pub error: Option<String>,
    pub created_at: u64,
}

impl From<CrawlTask> for TaskResponse {
    fn from(t: CrawlTask) -> Self {
        Self {
            id: t.id,
            keyword: t.keyword.as_str().to_string(),
            max_pages: t.max_pages.value(),
            status: match t.status {
                TaskStatus::Pending => "pending",
                TaskStatus::Running => "running",
                TaskStatus::Done => "done",
                TaskStatus::Failed => "failed",
            }
            .to_string(),
            item_count: t.item_count,
            error: t.error,
            created_at: t.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ItemResponse {
    pub id: String,
    pub title: String,
    pub price: f64,
    pub seller: String,
    pub url: String,
    pub crawled_at: u64,
}

impl From<Item> for ItemResponse {
    fn from(it: Item) -> Self {
        Self {
            id: it.id,
            title: it.title,
            price: it.price,
            seller: it.seller,
            url: it.url,
            crawled_at: it.crawled_at,
        }
    }
}

/// 统一 API 响应结构
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            message: "ok".into(),
            data: Some(data),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            code: -1,
            message: message.into(),
            data: None,
        }
    }
}
