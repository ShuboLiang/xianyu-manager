use std::collections::HashSet;

use axum::extract::{Json, Query, State};

use super::dto::{ApiResponse, ItemResponse, PageQuery, PageResponse};
use super::AppState;

/// 分页参数钳制：page ≥ 1，page_size ∈ [1, 100]，默认 20
pub(crate) fn normalize_page(page: Option<u64>, page_size: Option<u64>) -> (u64, u64) {
    (
        page.unwrap_or(1).max(1),
        page_size.unwrap_or(20).clamp(1, 100),
    )
}

/// GET /api/items?page=1&page_size=20：已抓取原始数据，按抓取时间倒序分页
pub async fn list_items(
    State(state): State<AppState>,
    Query(q): Query<PageQuery>,
) -> Json<ApiResponse<PageResponse<ItemResponse>>> {
    let (page, page_size) = normalize_page(q.page, q.page_size);
    match state.item_service.list_paginated(page, page_size).await {
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
