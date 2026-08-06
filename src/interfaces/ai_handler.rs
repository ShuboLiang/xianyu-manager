//! AI 配置与审计相关 HTTP 接口。

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::application::ai_provider_service::AiProviderPatch;
use crate::application::ai_tool_call_service::PurgeCriteria;

use super::dto::{
    AiChatRequest, AiChatResponse, AiProviderCreateRequest, AiProviderResponse,
    AiProviderUpdateRequest, AiStatusResponse, AiToolCallListQuery, AiToolCallPurgePreviewResponse,
    AiToolCallPurgeRequest, AiToolCallPurgeResponse, AiToolCallResponse, AiToolInfoResponse,
    ApiResponse, ClassifyProductsRequest, ClassifyProductsResponse, ClassifyTaskResponse,
    CrawlModeResponse, CrawlPromptRequest, CrawlPromptResponse, PageResponse, TestConnectionResponse,
    UpdateCrawlModeRequest,
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
            req.extra_params,
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
        extra_params: req.extra_params,
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

/// GET /api/ai/crawl-prompt：读取用户自定义抓取提示词
pub async fn get_crawl_prompt(
    State(state): State<AppState>,
) -> Json<ApiResponse<CrawlPromptResponse>> {
    match state.ai_settings_service.get_crawl_prompt().await {
        Ok(p) => Json(ApiResponse::ok(CrawlPromptResponse {
            custom_prompt: p,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/crawl-prompt：保存（整体覆盖；空串 = 清空）
pub async fn update_crawl_prompt(
    State(state): State<AppState>,
    Json(req): Json<CrawlPromptRequest>,
) -> Json<ApiResponse<CrawlPromptResponse>> {
    match state.ai_settings_service.save_crawl_prompt(req.custom_prompt).await {
        Ok(p) => Json(ApiResponse::ok(CrawlPromptResponse {
            custom_prompt: p,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/crawl-mode：读取当前生效的抓取模式（DB 设置 > 环境变量兜底）
pub async fn get_crawl_mode(
    State(state): State<AppState>,
) -> Json<ApiResponse<CrawlModeResponse>> {
    match state.ai_settings_service.get_crawl_mode().await {
        Ok(mode) => Json(ApiResponse::ok(CrawlModeResponse { mode })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/crawl-mode：切换抓取模式（direct/agent，下一轮抓取生效）
pub async fn update_crawl_mode(
    State(state): State<AppState>,
    Json(req): Json<UpdateCrawlModeRequest>,
) -> Json<ApiResponse<CrawlModeResponse>> {
    match state.ai_settings_service.save_crawl_mode(req.mode).await {
        Ok(mode) => Json(ApiResponse::ok(CrawlModeResponse { mode })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/tool-calls?page=1&page_size=20：工具调用审计，按时间倒序分页
pub async fn list_tool_calls(
    State(state): State<AppState>,
    Query(q): Query<AiToolCallListQuery>,
) -> Json<ApiResponse<PageResponse<AiToolCallResponse>>> {
    let (page, page_size) = normalize_page(q.page, q.page_size);
    match state
        .ai_tool_call_service
        .list_paginated(page, page_size, q.tool_name.as_deref(), q.failed)
        .await
    {
        Ok(p) => Json(ApiResponse::ok(PageResponse::new(
            p.items.into_iter().map(Into::into).collect(),
            p.total,
            page,
            page_size,
        ))),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/tool-calls/names：库中出现过的工具名（筛选下拉用）
pub async fn list_tool_call_names(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<String>>> {
    match state.ai_tool_call_service.list_tool_names().await {
        Ok(names) => Json(ApiResponse::ok(names)),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/tool-calls/purge/preview：预览将清理的记录数
pub async fn preview_purge_tool_calls(
    State(state): State<AppState>,
    Json(req): Json<AiToolCallPurgeRequest>,
) -> Json<ApiResponse<AiToolCallPurgePreviewResponse>> {
    let result = match PurgeCriteria::new(req.before_days, req.keep_latest) {
        Ok(criteria) => state.ai_tool_call_service.purge_preview(criteria).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(matched) => Json(ApiResponse::ok(AiToolCallPurgePreviewResponse { matched })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/tool-calls/purge：执行清理（保留期管理，非单条删除）
pub async fn purge_tool_calls(
    State(state): State<AppState>,
    Json(req): Json<AiToolCallPurgeRequest>,
) -> Json<ApiResponse<AiToolCallPurgeResponse>> {
    let result = match PurgeCriteria::new(req.before_days, req.keep_latest) {
        Ok(criteria) => state.ai_tool_call_service.purge(criteria).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(deleted) => Json(ApiResponse::ok(AiToolCallPurgeResponse { deleted })),
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

/// GET /api/ai/tools：当前可用的 AI 工具清单（名称/描述/参数 Schema），供外部智能体发现能力
pub async fn list_admin_tools(State(state): State<AppState>) -> Json<ApiResponse<Vec<AiToolInfoResponse>>> {
    let manifest = state.admin_tools_service.tool_manifest();
    let infos: Vec<AiToolInfoResponse> = manifest
        .into_iter()
        .map(|m| AiToolInfoResponse {
            name: m.name,
            description: m.description,
            parameters: m.parameters,
        })
        .collect();
    Json(ApiResponse::ok(infos))
}

/// POST /api/ai/chat：通用管理助手，接收自然语言指令，AI 自主调用工具完成查询/操作
pub async fn ai_chat(
    State(state): State<AppState>,
    Json(req): Json<AiChatRequest>,
) -> Json<ApiResponse<AiChatResponse>> {
    match state.admin_tools_service.chat(&req.message).await {
        Ok(reply) => Json(ApiResponse::ok(AiChatResponse { reply })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

