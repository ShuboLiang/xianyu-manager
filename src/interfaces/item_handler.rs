use std::collections::HashSet;

use axum::extract::{Json, Path, Query, State};

use super::dto::{
    ApiResponse, ItemBatchDeleteIdsRequest, ItemBatchDeletePreviewResponse,
    ItemBatchDeleteRequest, ItemBatchDeleteResponse, ItemListQuery, ItemResponse, PageResponse,
};
use super::AppState;

/// 分页参数钳制：page ≥ 1，page_size ∈ [1, 100]，默认 20
pub(crate) fn normalize_page(page: Option<u64>, page_size: Option<u64>) -> (u64, u64) {
    (
        page.unwrap_or(1).max(1),
        page_size.unwrap_or(20).clamp(1, 100),
    )
}

/// GET /api/items?page=1&page_size=20&search=关键词&tag_id=1：已抓取原始数据，按抓取时间倒序分页
pub async fn list_items(
    State(state): State<AppState>,
    Query(q): Query<ItemListQuery>,
) -> Json<ApiResponse<PageResponse<ItemResponse>>> {
    let (page, page_size) = normalize_page(q.page, q.page_size);
    match state.item_service.list_paginated(page, page_size, q.search, q.tag_id).await {
        Ok(p) => {
            let product_ids: HashSet<i64> = p
                .items
                .iter()
                .filter_map(|it| it.product_id)
                .collect();
            let product_names = resolve_product_names(&state, product_ids).await;
            let items = p
                .items
                .into_iter()
                .map(|it| {
                    let mut resp: ItemResponse = it.into();
                    if let Some(pid) = resp.product_id {
                        resp.product_name = product_names.get(&pid).cloned();
                    }
                    resp
                })
                .collect();
            Json(ApiResponse::ok(PageResponse::new(items, p.total, page, page_size)))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

async fn resolve_product_names(
    state: &AppState,
    ids: HashSet<i64>,
) -> std::collections::HashMap<i64, String> {
    let mut map = std::collections::HashMap::new();
    for id in ids {
        if let Ok(p) = state.product_service.get_product(id).await {
            map.insert(id, p.name.as_str().to_string());
        }
    }
    map
}

/// DELETE /api/items/{id}：删除单条抓取记录
pub async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.item_service.delete_item(&id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/items/batch-delete/preview：预览按搜索条件批量删除（空 search = 全部）
pub async fn preview_batch_delete(
    State(state): State<AppState>,
    Json(req): Json<ItemBatchDeleteRequest>,
) -> Json<ApiResponse<ItemBatchDeletePreviewResponse>> {
    match state.item_service.preview_delete_matching(req.search).await {
        Ok(preview) => Json(ApiResponse::ok(ItemBatchDeletePreviewResponse {
            total: preview.total,
            sample: preview.sample,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/items/batch-delete：按搜索条件批量删除抓取记录（空 search = 清空全部）
pub async fn batch_delete_items(
    State(state): State<AppState>,
    Json(req): Json<ItemBatchDeleteRequest>,
) -> Json<ApiResponse<ItemBatchDeleteResponse>> {
    match state.item_service.delete_matching(req.search).await {
        Ok(deleted) => Json(ApiResponse::ok(ItemBatchDeleteResponse { deleted })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/items/batch-delete-ids/preview：勾选批量删除预览（数量 + 前 10 条标题样本）
pub async fn preview_batch_delete_items_by_ids(
    State(state): State<AppState>,
    Json(req): Json<ItemBatchDeleteIdsRequest>,
) -> Json<ApiResponse<ItemBatchDeletePreviewResponse>> {
    match state.item_service.preview_delete_by_ids(&req.ids).await {
        Ok(p) => Json(ApiResponse::ok(ItemBatchDeletePreviewResponse {
            total: p.total,
            sample: p.sample,
        })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/items/batch-delete-ids：勾选批量删除执行（按 id 列表）
pub async fn batch_delete_items_by_ids(
    State(state): State<AppState>,
    Json(req): Json<ItemBatchDeleteIdsRequest>,
) -> Json<ApiResponse<ItemBatchDeleteResponse>> {
    match state.item_service.delete_by_ids(&req.ids).await {
        Ok(deleted) => Json(ApiResponse::ok(ItemBatchDeleteResponse { deleted })),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
