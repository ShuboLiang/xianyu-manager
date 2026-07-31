use axum::extract::{Json, Path, State};

use crate::application::tag_service::TagPatch;

use super::dto::{ApiResponse, ProductBriefResponse, TagCreateRequest, TagResponse, TagUpdateRequest};
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

/// GET /api/tags/{id}/products：使用该标签的商品（删除前的影响提示）
pub async fn tag_products(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<Vec<ProductBriefResponse>>> {
    // 先确认标签存在，避免对不存在的标签返回空列表造成误解
    if let Err(e) = state.tag_service.get_tag(id).await {
        return Json(ApiResponse::err(e.to_string()));
    }
    match state.product_service.list_by_tag(id).await {
        Ok(products) => Json(ApiResponse::ok(
            products
                .into_iter()
                .map(|p| ProductBriefResponse {
                    id: p.id,
                    name: p.name.as_str().to_string(),
                })
                .collect(),
        )),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
