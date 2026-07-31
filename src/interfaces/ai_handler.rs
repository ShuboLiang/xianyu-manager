//! AI 配置与审计相关 HTTP 接口。

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::application::ai_provider_service::AiProviderPatch;

use super::dto::{
    AiProviderCreateRequest, AiProviderResponse, AiProviderUpdateRequest, AiStatusResponse,
    AiToolCallResponse, ApiResponse, ClassifyProductsRequest, ClassifyProductsResponse,
    ClassifyTaskResponse, PageQuery, PageResponse, TestConnectionResponse,
};
use super::item_handler::normalize_page;
use super::AppState;

/// GET /api/ai/providers
pub async fn list_providers(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<AiProviderResponse>>> {
    match state.ai_provider_service.list_providers().await {
        Ok(list) => Json(ApiResponse::ok(list.into_iter().map(Into::into).collect())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/providers
pub async fn create_provider(
    State(state): State<AppState>,
    Json(req): Json<AiProviderCreateRequest>,
) -> Json<ApiResponse<AiProviderResponse>> {
    let result = state
        .ai_provider_service
        .create_provider(
            req.name,
            req.base_url,
            req.api_key,
            req.model,
            req.timeout_secs,
            req.max_retries,
        )
        .await;
    match result {
        Ok(p) => Json(ApiResponse::ok(p.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/providers/{id}
pub async fn get_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<AiProviderResponse>> {
    match state.ai_provider_service.get_provider(id).await {
        Ok(p) => Json(ApiResponse::ok(p.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/providers/{id}
pub async fn update_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<AiProviderUpdateRequest>,
) -> Json<ApiResponse<AiProviderResponse>> {
    let patch = AiProviderPatch {
        name: req.name,
        base_url: req.base_url,
        api_key: req.api_key,
        model: req.model,
        timeout_secs: req.timeout_secs,
        max_retries: req.max_retries,
    };
    match state.ai_provider_service.update_provider(id, patch).await {
        Ok(p) => Json(ApiResponse::ok(p.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/ai/providers/{id}
pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.ai_provider_service.delete_provider(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/providers/{id}/default
pub async fn set_default_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<AiProviderResponse>> {
    match state.ai_provider_service.set_default(id).await {
        Ok(p) => Json(ApiResponse::ok(p.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/providers/{id}/test
pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<TestConnectionResponse>> {
    match state.ai_provider_service.test_connection(id).await {
        Ok(r) => Json(ApiResponse::ok(r.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/status
pub async fn ai_status(State(state): State<AppState>) -> Json<ApiResponse<AiStatusResponse>> {
    match state.ai_provider_service.status().await {
        Ok(s) => Json(ApiResponse::ok(s.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/tool-calls?page=1&page_size=20：工具调用审计，按时间倒序分页
pub async fn list_tool_calls(
    State(state): State<AppState>,
    Query(q): Query<PageQuery>,
) -> Json<ApiResponse<PageResponse<AiToolCallResponse>>> {
    let (page, page_size) = normalize_page(q.page, q.page_size);
    match state.ai_tool_call_service.list_paginated(page, page_size).await {
        Ok(p) => Json(ApiResponse::ok(PageResponse::new(
            p.items.into_iter().map(Into::into).collect(),
            p.total,
            page,
            page_size,
        ))),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/classify-products：同步 AI 自动打标签（≤50 商品）
pub async fn classify_products_sync(
    State(state): State<AppState>,
    Json(req): Json<ClassifyProductsRequest>,
) -> Json<ApiResponse<ClassifyProductsResponse>> {
    match state.classify_service.classify_sync(req.product_ids).await {
        Ok(result) => Json(ApiResponse::ok(result.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/classify-tasks：创建异步 AI 打标签任务
pub async fn create_classify_task(
    State(state): State<AppState>,
    Json(req): Json<ClassifyProductsRequest>,
) -> Json<ApiResponse<ClassifyTaskResponse>> {
    match state.classify_service.create_classify_task(req.product_ids).await {
        Ok(task) => Json(ApiResponse::ok(task.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/classify-tasks/{id}：查询异步任务
pub async fn get_classify_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ClassifyTaskResponse>> {
    match state.classify_service.get_classify_task(&id).await {
        Ok(task) => Json(ApiResponse::ok(task.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/classify-tasks/{id}/cancel：取消运行中的任务
pub async fn cancel_classify_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<ClassifyTaskResponse>> {
    match state.classify_service.cancel_classify_task(&id).await {
        Ok(task) => Json(ApiResponse::ok(task.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

