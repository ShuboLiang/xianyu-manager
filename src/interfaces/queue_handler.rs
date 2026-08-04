use axum::extract::{Json, Path, State};

use crate::application::queue_service::{EnqueueTarget, PreviewResult, QueuePurgeCriteria};
use crate::domain::error::DomainError;
use crate::domain::product::Product;

use super::dto::{
    ApiResponse, EnqueueRequest, EnqueueResponse, PreviewRequest, PreviewResponse,
    ProductBriefResponse, QueuePurgeOutcomeResponse, QueuePurgeRequest, QueueResponse,
    RenameQueueRequest,
};
use super::AppState;

/// POST /api/queues/preview：入队前预览（不落库）
pub async fn preview(
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Json<ApiResponse<PreviewResponse>> {
    let target = match into_target(req.selector, req.product_ids) {
        Ok(t) => t,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state.queue_service.preview(target).await {
        Ok(result) => Json(ApiResponse::ok(result.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues：创建队列
pub async fn enqueue(
    State(state): State<AppState>,
    Json(req): Json<EnqueueRequest>,
) -> Json<ApiResponse<EnqueueResponse>> {
    let target = match into_target(req.selector, req.product_ids) {
        Ok(t) => t,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state
        .queue_service
        .enqueue(target, req.interval_secs.unwrap_or(3))
        .await
    {
        Ok((queue, result)) => Json(ApiResponse::ok(EnqueueResponse {
            queue_id: queue.id,
            status: queue.status.as_str().to_string(),
            added: briefs(result.to_add),
            skipped: briefs(result.skipped),
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/{id}/entries：向运行中/暂停中/排队中队列追加条目
pub async fn append_entries(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<EnqueueRequest>,
) -> Json<ApiResponse<EnqueueResponse>> {
    let target = match into_target(req.selector, req.product_ids) {
        Ok(t) => t,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state.queue_service.append_entries(id, target).await {
        Ok(result) => Json(ApiResponse::ok(EnqueueResponse {
            queue_id: id,
            status: "appended".to_string(),
            added: briefs(result.to_add),
            skipped: briefs(result.skipped),
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/queues：队列列表（含进度计数）
pub async fn list_queues(State(state): State<AppState>) -> Json<ApiResponse<Vec<QueueResponse>>> {
    match state.queue_service.list_progress().await {
        Ok(list) => Json(ApiResponse::ok(list.into_iter().map(Into::into).collect())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/queues/{id}：队列详情
pub async fn get_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<QueueResponse>> {
    match state.queue_service.get_progress(id).await {
        Ok(p) => Json(ApiResponse::ok(p.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/{id}/pause
pub async fn pause_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<QueueResponse>> {
    match state.queue_service.pause(id).await {
        Ok(_) => get_queue(State(state), Path(id)).await,
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/{id}/resume
pub async fn resume_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<QueueResponse>> {
    match state.queue_service.resume(id).await {
        Ok(_) => get_queue(State(state), Path(id)).await,
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/{id}/cancel
pub async fn cancel_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<QueueResponse>> {
    match state.queue_service.cancel(id).await {
        Ok(_) => get_queue(State(state), Path(id)).await,
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/queues/{id}：删除已结束（done/cancelled）的队列及其条目
pub async fn delete_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.queue_service.delete(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/queues/{id}/name：队列手动改名
pub async fn rename_queue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<RenameQueueRequest>,
) -> Json<ApiResponse<()>> {
    match state.queue_service.rename(id, req.name).await {
        Ok(_) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/purge/preview：预览历史队列清理（不落库）
pub async fn purge_preview(
    State(state): State<AppState>,
    Json(req): Json<QueuePurgeRequest>,
) -> Json<ApiResponse<QueuePurgeOutcomeResponse>> {
    let criteria = match QueuePurgeCriteria::new(req.before_days, req.keep_latest) {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state.queue_service.purge_preview(criteria).await {
        Ok(o) => Json(ApiResponse::ok(QueuePurgeOutcomeResponse {
            queues: o.queues,
            entries: o.entries,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/purge：执行历史队列清理（只作用于 done/cancelled）
pub async fn purge(
    State(state): State<AppState>,
    Json(req): Json<QueuePurgeRequest>,
) -> Json<ApiResponse<QueuePurgeOutcomeResponse>> {
    let criteria = match QueuePurgeCriteria::new(req.before_days, req.keep_latest) {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state.queue_service.purge(criteria).await {
        Ok(o) => Json(ApiResponse::ok(QueuePurgeOutcomeResponse {
            queues: o.queues,
            entries: o.entries,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/pause-all：全部暂停，返回影响数量
pub async fn pause_all(State(state): State<AppState>) -> Json<ApiResponse<usize>> {
    match state.queue_service.pause_all().await {
        Ok(n) => Json(ApiResponse::ok(n)),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/queues/resume-all：全部恢复，返回影响数量
pub async fn resume_all(State(state): State<AppState>) -> Json<ApiResponse<usize>> {
    match state.queue_service.resume_all().await {
        Ok(n) => Json(ApiResponse::ok(n)),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// selector / product_ids 二选一
fn into_target(
    selector: Option<super::dto::SelectorDto>,
    product_ids: Option<Vec<i64>>,
) -> Result<EnqueueTarget, DomainError> {
    match (selector, product_ids) {
        (Some(s), None) => Ok(EnqueueTarget::Selector(s.into())),
        (None, Some(ids)) => Ok(EnqueueTarget::ProductIds(ids)),
        (Some(_), Some(_)) => Err(DomainError::InvalidInput(
            "selector 与 product_ids 只能二选一".into(),
        )),
        (None, None) => Err(DomainError::InvalidInput(
            "必须提供 selector 或 product_ids".into(),
        )),
    }
}

fn briefs(products: Vec<Product>) -> Vec<ProductBriefResponse> {
    products
        .into_iter()
        .map(|p| ProductBriefResponse {
            id: p.id,
            name: p.name.as_str().to_string(),
        })
        .collect()
}

impl From<PreviewResult> for PreviewResponse {
    fn from(r: PreviewResult) -> Self {
        Self {
            to_add: briefs(r.to_add),
            skipped: briefs(r.skipped),
        }
    }
}
