use axum::extract::{Json, State};

use super::dto::{ApiResponse, StatsResponse};
use super::AppState;

/// GET /api/stats：KPI 概览统计（与列表分页解耦，队列指标仍由前端从队列列表推导）
pub async fn get_stats(State(state): State<AppState>) -> Json<ApiResponse<StatsResponse>> {
    match state.stats_service.stats().await {
        Ok(s) => Json(ApiResponse::ok(StatsResponse {
            product_count: s.product_count,
            last_crawled_at: s.last_crawled_at,
            crawled_today: s.crawled_today,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
