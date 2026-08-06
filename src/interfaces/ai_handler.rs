//! AI 配置与审计相关 HTTP 接口。

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::application::ai::tool_approval::{ApprovalDecision, ApprovalMode};
use crate::application::ai_provider_service::AiProviderPatch;
use crate::application::ai_tool_call_service::PurgeCriteria;

use super::dto::{
    AiChatRequest, AiChatResponse, AiProviderCreateRequest, AiProviderResponse,
    AiProviderUpdateRequest, AiStatusResponse, AiToolCallListQuery, AiToolCallPurgePreviewResponse,
    AiToolCallPurgeRequest, AiToolCallPurgeResponse, AiToolCallResponse, AiToolInfoResponse,
    ApiResponse, ApprovalDecideRequest, ApprovalModeRequest, ApprovalModeResponse,
    ClassifyProductsRequest, ClassifyProductsResponse, ClassifyTaskResponse,
    ConversationDetailResponse, ConversationMessageResponse, ConversationResponse,
    CrawlModeResponse, CrawlPromptRequest, CrawlPromptResponse, PageResponse,
    PendingApprovalQuery, PendingApprovalResponse, RenameConversationRequest,
    TestConnectionResponse, ToolAvailabilityRequest, UpdateCrawlModeRequest,
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
        .list_paginated(page, page_size, q.tool_name.as_deref(), q.failed, q.source.as_deref())
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

/// GET /api/ai/tools：当前 AI 工具清单（名称/描述/参数 Schema + 启停状态），供外部智能体发现能力
pub async fn list_admin_tools(State(state): State<AppState>) -> Json<ApiResponse<Vec<AiToolInfoResponse>>> {
    match state.admin_tools_service.tool_manifest().await {
        Ok(manifest) => Json(ApiResponse::ok(
            manifest
                .into_iter()
                .map(|m| AiToolInfoResponse {
                    name: m.name,
                    description: m.description,
                    parameters: m.parameters,
                    enabled: m.enabled,
                    is_write: m.is_write,
                })
                .collect(),
        )),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/tools：整体替换全局禁用工具名单，返回更新后的完整清单
pub async fn update_admin_tools(
    State(state): State<AppState>,
    Json(req): Json<ToolAvailabilityRequest>,
) -> Json<ApiResponse<Vec<AiToolInfoResponse>>> {
    if let Err(e) = state
        .admin_tools_service
        .set_disabled_tools(req.disabled_tools)
        .await
    {
        return Json(ApiResponse::err(e.to_string()));
    }
    match state.admin_tools_service.tool_manifest().await {
        Ok(manifest) => Json(ApiResponse::ok(
            manifest
                .into_iter()
                .map(|m| AiToolInfoResponse {
                    name: m.name,
                    description: m.description,
                    parameters: m.parameters,
                    enabled: m.enabled,
                    is_write: m.is_write,
                })
                .collect(),
        )),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
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

/// GET /api/ai/chat/sessions：全部会话（按最近更新倒序，含消息数）
pub async fn list_conversations(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<ConversationResponse>>> {
    match state.chat_session_service.list_with_counts().await {
        Ok(list) => {
            let mut resp = Vec::with_capacity(list.len());
            for (c, count) in list {
                let mode = state.tool_approval.get_mode(c.id).await;
                resp.push(ConversationResponse::from_conversation(
                    c,
                    count,
                    mode.as_str().to_string(),
                ));
            }
            Json(ApiResponse::ok(resp))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/chat/sessions：新建会话
pub async fn create_conversation(
    State(state): State<AppState>,
) -> Json<ApiResponse<ConversationResponse>> {
    match state.chat_session_service.create().await {
        Ok(c) => {
            let mode = state.tool_approval.get_mode(c.id).await;
            Json(ApiResponse::ok(ConversationResponse::from_conversation(
                c,
                0,
                mode.as_str().to_string(),
            )))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/ai/chat/sessions/{id}：会话详情 + 全部消息
pub async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<ConversationDetailResponse>> {
    match state.chat_session_service.get(id).await {
        Ok((conversation, messages)) => {
            let mode = state.tool_approval.get_mode(conversation.id).await;
            Json(ApiResponse::ok(ConversationDetailResponse {
                conversation: ConversationResponse::from_conversation(
                    conversation,
                    messages.len() as u64,
                    mode.as_str().to_string(),
                ),
                messages: messages.into_iter().map(Into::into).collect(),
            }))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/chat/sessions/{id}/title：会话改名
pub async fn rename_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<RenameConversationRequest>,
) -> Json<ApiResponse<ConversationResponse>> {
    match state.chat_session_service.rename(id, req.title).await {
        Ok(c) => {
            let count = state.chat_session_service.message_count(c.id).await.unwrap_or(0);
            let mode = state.tool_approval.get_mode(c.id).await;
            Json(ApiResponse::ok(ConversationResponse::from_conversation(
                c,
                count,
                mode.as_str().to_string(),
            )))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/ai/chat/sessions/{id}：删除会话及其消息
pub async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.chat_session_service.delete(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/ai/chat/sessions/{id}/messages：会话内发消息（AI 带历史回复）
pub async fn chat_in_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<AiChatRequest>,
) -> Json<ApiResponse<AiChatResponse>> {
    match state.chat_session_service.chat(id, &req.message).await {
        Ok(reply) => Json(ApiResponse::ok(AiChatResponse { reply })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/ai/chat/sessions/{id}/messages：清空会话全部消息（保留会话本身）
pub async fn clear_conversation_messages(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.chat_session_service.clear(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/ai/chat/sessions/{id}/mode：切换会话写操作确认模式（normal / yolo）
pub async fn set_conversation_mode(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ApprovalModeRequest>,
) -> Json<ApiResponse<ApprovalModeResponse>> {
    let mode = match ApprovalMode::from_str(&req.mode) {
        Some(m) => m,
        None => {
            return Json(ApiResponse::err(
                "mode 必须是 normal 或 yolo".to_string(),
            ))
        }
    };
    state.tool_approval.set_mode(id, mode).await;
    tracing::info!("AI 助手会话 #{id} 切换为 {} 模式", mode.as_str());
    Json(ApiResponse::ok(ApprovalModeResponse {
        mode: mode.as_str().to_string(),
    }))
}

/// GET /api/ai/approvals/pending?conversation_id={id}：某会话待用户确认的写操作审批
pub async fn list_pending_approvals(
    State(state): State<AppState>,
    Query(q): Query<PendingApprovalQuery>,
) -> Json<ApiResponse<Vec<PendingApprovalResponse>>> {
    let pendings = state.tool_approval.list_pending(q.conversation_id).await;
    Json(ApiResponse::ok(
        pendings
            .into_iter()
            .map(|p| PendingApprovalResponse {
                id: p.id,
                conversation_id: p.conversation_id,
                tool_name: p.tool_name,
                arguments: p.arguments,
                created_at: p.created_at,
            })
            .collect(),
    ))
}

/// POST /api/ai/approvals/{id}/decide：用户对写操作审批作出决策（allow_once / allow_always / deny）
pub async fn decide_approval(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<ApprovalDecideRequest>,
) -> Json<ApiResponse<()>> {
    let decision = match ApprovalDecision::from_str(&req.decision) {
        Some(d) => d,
        None => {
            return Json(ApiResponse::err(
                "decision 必须是 allow_once / allow_always / deny 之一".to_string(),
            ))
        }
    };
    match state.tool_approval.decide(id, decision).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// 辅助：消息列表响应（供前端会话详情直接复用）
#[allow(dead_code)]
fn messages_response(messages: Vec<crate::domain::ai_conversation::ConversationMessage>) -> Vec<ConversationMessageResponse> {
    messages.into_iter().map(Into::into).collect()
}

