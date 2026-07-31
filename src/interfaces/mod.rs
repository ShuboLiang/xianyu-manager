//! 接口层：axum 路由与 handler，只做 HTTP 协议 ↔ 应用层的翻译。

pub mod crawl_handler;
pub mod dto;
pub mod item_handler;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::crawl_service::CrawlService;
use crate::application::item_service::ItemService;

use dto::ApiResponse;

/// 注入给 handler 的应用服务句柄
#[derive(Clone)]
pub struct AppState {
    pub crawl_service: Arc<CrawlService>,
    pub item_service: Arc<ItemService>,
}

/// 组装整个应用的路由：
/// - `/api/*` 后端接口
/// - 其余路径回退到前端静态文件（index.html）
pub fn build_router(state: AppState, static_dir: &str) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/crawl", post(crawl_handler::start_crawl))
        .route("/crawl/{id}", get(crawl_handler::get_task))
        .route("/items", get(item_handler::list_items))
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(static_dir.to_string()))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<ApiResponse<&'static str>> {
    Json(ApiResponse::ok("healthy"))
}
