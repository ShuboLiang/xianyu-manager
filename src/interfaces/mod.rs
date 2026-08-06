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

use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::ai::admin_tools::AdminToolsService;
use crate::application::ai::chat_session_service::ChatSessionService;
use crate::application::ai::classify_service::ClassifyService;
use crate::application::ai_provider_service::AiProviderService;
use crate::application::ai_settings_service::AiSettingsService;
use crate::application::ai_tool_call_service::AiToolCallService;
use crate::application::crawl_service::CrawlService;
use crate::application::item_service::ItemService;
use crate::application::product_service::ProductService;
use crate::application::queue_service::QueueService;
use crate::application::stats_service::StatsService;
use crate::application::tag_service::TagService;
use crate::application::trend_service::TrendService;

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
    pub ai_settings_service: Arc<AiSettingsService>,
    pub ai_tool_call_service: Arc<AiToolCallService>,
    pub classify_service: Arc<ClassifyService>,
    pub admin_tools_service: Arc<AdminToolsService>,
    pub chat_session_service: Arc<ChatSessionService>,
    pub stats_service: Arc<StatsService>,
    pub trend_service: Arc<TrendService>,
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
        .route(
            "/items/batch-delete/preview",
            post(item_handler::preview_batch_delete),
        )
        .route("/items/batch-delete", post(item_handler::batch_delete_items))
        .route(
            "/items/batch-delete-ids/preview",
            post(item_handler::preview_batch_delete_items_by_ids),
        )
        .route(
            "/items/batch-delete-ids",
            post(item_handler::batch_delete_items_by_ids),
        )
        .route("/items/{id}", delete(item_handler::delete_item))
        .route("/tags", get(tag_handler::list_tags).post(tag_handler::create_tag))
        .route(
            "/tags/{id}",
            get(tag_handler::get_tag)
                .put(tag_handler::update_tag)
                .delete(tag_handler::delete_tag),
        )
        .route("/tags/{id}/products", get(tag_handler::tag_products))
        .route("/products/price-trend", get(product_handler::price_trend))
        .route("/products/export", get(product_handler::export_products))
        .route("/products/batch", post(product_handler::batch_create_products))
        .route(
            "/products/batch-delete/preview",
            post(product_handler::preview_batch_delete),
        )
        .route(
            "/products/batch-delete",
            post(product_handler::batch_delete_products),
        )
        .route(
            "/products/batch-delete-ids/preview",
            post(product_handler::preview_batch_delete_products_by_ids),
        )
        .route(
            "/products/batch-delete-ids",
            post(product_handler::batch_delete_products_by_ids),
        )
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
        .route(
            "/products/{id}/latest-items",
            get(product_handler::latest_product_items),
        )
        .route("/queues/preview", post(queue_handler::preview))
        .route(
            "/queues",
            get(queue_handler::list_queues).post(queue_handler::enqueue),
        )
        .route("/queues/pause-all", post(queue_handler::pause_all))
        .route("/queues/resume-all", post(queue_handler::resume_all))
        .route("/queues/purge", post(queue_handler::purge))
        .route("/queues/purge/preview", post(queue_handler::purge_preview))
        .route("/queues/{id}", get(queue_handler::get_queue).delete(queue_handler::delete_queue))
        .route("/queues/{id}/entries", post(queue_handler::append_entries))
        .route("/queues/{id}/pause", post(queue_handler::pause_queue))
        .route("/queues/{id}/resume", post(queue_handler::resume_queue))
        .route("/queues/{id}/cancel", post(queue_handler::cancel_queue))
        .route("/queues/{id}/name", put(queue_handler::rename_queue))
        // AI 配置
        .route("/ai/status", get(ai_handler::ai_status))
        .route(
            "/ai/crawl-prompt",
            get(ai_handler::get_crawl_prompt).put(ai_handler::update_crawl_prompt),
        )
        .route(
            "/ai/crawl-mode",
            get(ai_handler::get_crawl_mode).put(ai_handler::update_crawl_mode),
        )
        .route("/ai/tool-calls", get(ai_handler::list_tool_calls))
        .route("/ai/tool-calls/names", get(ai_handler::list_tool_call_names))
        .route(
            "/ai/tool-calls/purge/preview",
            post(ai_handler::preview_purge_tool_calls),
        )
        .route("/ai/tool-calls/purge", post(ai_handler::purge_tool_calls))
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
        .route("/ai/tools", get(ai_handler::list_admin_tools))
        .route("/ai/chat", post(ai_handler::ai_chat))
        .route(
            "/ai/chat/sessions",
            get(ai_handler::list_conversations).post(ai_handler::create_conversation),
        )
        .route(
            "/ai/chat/sessions/{id}",
            get(ai_handler::get_conversation).delete(ai_handler::delete_conversation),
        )
        .route(
            "/ai/chat/sessions/{id}/title",
            put(ai_handler::rename_conversation),
        )
        .route(
            "/ai/chat/sessions/{id}/messages",
            post(ai_handler::chat_in_conversation),
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
