use axum::extract::{Json, Path, State};

use super::dto::{ApiResponse, CrawlRequest, TaskResponse};
use super::AppState;

/// POST /api/crawl：创建抓取任务，立即返回任务句柄，抓取在后台执行
pub async fn start_crawl(
    State(state): State<AppState>,
    Json(req): Json<CrawlRequest>,
) -> Json<ApiResponse<TaskResponse>> {
    match state.crawl_service.start_crawl(req.keyword, req.max_pages).await {
        Ok(task) => Json(ApiResponse::ok(task.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/crawl/{id}：查询任务状态，供前端轮询
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<TaskResponse>> {
    match state.crawl_service.get_task(&id).await {
        Ok(task) => Json(ApiResponse::ok(task.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
