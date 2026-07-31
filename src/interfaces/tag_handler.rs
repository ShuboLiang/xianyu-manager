use axum::extract::{Json, Path, State};

use crate::application::tag_service::TagPatch;

use super::dto::{ApiResponse, TagCreateRequest, TagResponse, TagUpdateRequest};
use super::AppState;

/// GET /api/tags：标签列表
pub async fn list_tags(State(state): State<AppState>) -> Json<ApiResponse<Vec<TagResponse>>> {
    match state.tag_service.list_tags().await {
        Ok(tags) => Json(ApiResponse::ok(tags.into_iter().map(Into::into).collect())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/tags：创建标签
pub async fn create_tag(
    State(state): State<AppState>,
    Json(req): Json<TagCreateRequest>,
) -> Json<ApiResponse<TagResponse>> {
    match state
        .tag_service
        .create_tag(req.name, req.remark)
        .await
    {
        Ok(tag) => Json(ApiResponse::ok(tag.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/tags/{id}：标签详情
pub async fn get_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<TagResponse>> {
    match state.tag_service.get_tag(id).await {
        Ok(tag) => Json(ApiResponse::ok(tag.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/tags/{id}：更新标签（部分字段）
pub async fn update_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<TagUpdateRequest>,
) -> Json<ApiResponse<TagResponse>> {
    let patch = TagPatch {
        name: req.name,
        enabled: req.enabled,
        remark: req.remark,
    };
    match state.tag_service.update_tag(id, patch).await {
        Ok(tag) => Json(ApiResponse::ok(tag.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/tags/{id}：删除标签
pub async fn delete_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.tag_service.delete_tag(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
