//! 接口层：axum 路由与 handler，只做 HTTP 协议 ↔ 应用层的翻译。

pub mod crawl_handler;
pub mod dto;
pub mod item_handler;
pub mod product_handler;
pub mod tag_handler;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::crawl_service::CrawlService;
use crate::application::item_service::ItemService;
use crate::application::product_service::ProductService;
use crate::application::tag_service::TagService;

use dto::ApiResponse;

/// 注入给 handler 的应用服务句柄
#[derive(Clone)]
pub struct AppState {
    pub crawl_service: Arc<CrawlService>,
    pub item_service: Arc<ItemService>,
    pub tag_service: Arc<TagService>,
    pub product_service: Arc<ProductService>,
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
        .route("/tags", get(tag_handler::list_tags).post(tag_handler::create_tag))
        .route(
            "/tags/{id}",
            get(tag_handler::get_tag)
                .put(tag_handler::update_tag)
                .delete(tag_handler::delete_tag),
        )
        .route("/tags/{id}/products", get(tag_handler::tag_products))
        .route(
            "/products",
            get(product_handler::list_products).post(product_handler::create_product),
        )
        .route(
            "/products/{id}",
            get(product_handler::get_product)
                .put(product_handler::update_product)
                .delete(product_handler::delete_product),
        )
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
