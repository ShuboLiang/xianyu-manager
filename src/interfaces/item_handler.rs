use axum::extract::{Json, State};

use super::dto::{ApiResponse, ItemResponse};
use super::AppState;

/// GET /api/items：商品列表（管理端数据源）
pub async fn list_items(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<ItemResponse>>> {
    match state.item_service.list_items().await {
        Ok(items) => Json(ApiResponse::ok(items.into_iter().map(Into::into).collect())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
