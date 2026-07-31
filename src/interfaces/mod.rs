//! 接口层：axum 路由与 handler，只做 HTTP 协议 ↔ 应用层的翻译。

pub mod ai_handler;
pub mod crawl_handler;
pub mod dto;
pub mod item_handler;
pub mod product_handler;
pub mod queue_handler;
pub mod stats_handler;
pub mod tag_handler;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::ai::classify_service::ClassifyService;
use crate::application::ai_provider_service::AiProviderService;
use crate::application::ai_tool_call_service::AiToolCallService;
use crate::application::crawl_service::CrawlService;
use crate::application::item_service::ItemService;
use crate::application::product_service::ProductService;
use crate::application::queue_service::QueueService;
use crate::application::stats_service::StatsService;
use crate::application::tag_service::TagService;

use dto::ApiResponse;

/// 注入给 handler 的应用服务句柄
#[derive(Clone)]
pub struct AppState {
    pub crawl_service: Arc<CrawlService>,
    pub item_service: Arc<ItemService>,
    pub tag_service: Arc<TagService>,
    pub product_service: Arc<ProductService>,
    pub queue_service: Arc<QueueService>,
    pub ai_provider_service: Arc<AiProviderService>,
    pub ai_tool_call_service: Arc<AiToolCallService>,
    pub classify_service: Arc<ClassifyService>,
    pub stats_service: Arc<StatsService>,
}

/// 组装整个应用的路由：
/// - `/api/*` 后端接口
/// - 其余路径回退到前端静态文件（index.html）
pub fn build_router(state: AppState, static_dir: &str) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats_handler::get_stats))
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
        .route("/products/batch", post(product_handler::batch_create_products))
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
        .route("/queues/preview", post(queue_handler::preview))
        .route(
            "/queues",
            get(queue_handler::list_queues).post(queue_handler::enqueue),
        )
        .route("/queues/pause-all", post(queue_handler::pause_all))
        .route("/queues/resume-all", post(queue_handler::resume_all))
        .route("/queues/{id}", get(queue_handler::get_queue).delete(queue_handler::delete_queue))
        .route("/queues/{id}/entries", post(queue_handler::append_entries))
        .route("/queues/{id}/pause", post(queue_handler::pause_queue))
        .route("/queues/{id}/resume", post(queue_handler::resume_queue))
        .route("/queues/{id}/cancel", post(queue_handler::cancel_queue))
        // AI 配置
        .route("/ai/status", get(ai_handler::ai_status))
        .route("/ai/tool-calls", get(ai_handler::list_tool_calls))
        .route(
            "/ai/providers",
            get(ai_handler::list_providers).post(ai_handler::create_provider),
        )
        .route(
            "/ai/providers/{id}",
            get(ai_handler::get_provider)
                .put(ai_handler::update_provider)
                .delete(ai_handler::delete_provider),
        )
        .route("/ai/providers/{id}/default", post(ai_handler::set_default_provider))
        .route("/ai/providers/{id}/test", post(ai_handler::test_provider))
        .route("/ai/classify-products", post(ai_handler::classify_products_sync))
        .route("/ai/classify-tasks", post(ai_handler::create_classify_task))
        .route("/ai/classify-tasks/{id}", get(ai_handler::get_classify_task))
        .route("/ai/classify-tasks/{id}/cancel", post(ai_handler::cancel_classify_task))
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
