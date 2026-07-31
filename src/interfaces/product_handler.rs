use std::collections::HashMap;

use axum::extract::{Json, Path, Query, State};

use crate::domain::product::Product;

use super::dto::{
    ApiResponse, PageResponse, ProductBatchCreateRequest, ProductBatchCreateResponse,
    ProductCreateRequest, ProductListQuery, ProductResponse, ProductUpdateRequest,
    BatchSkippedItem,
};
use super::item_handler::normalize_page;
use super::AppState;

/// GET /api/products?page=1&page_size=20&sort_by=avg_price&sort_dir=desc：商品列表（分页 + 服务端排序）
pub async fn list_products(
    State(state): State<AppState>,
    Query(q): Query<ProductListQuery>,
) -> Json<ApiResponse<PageResponse<ProductResponse>>> {
    let (page, page_size) = normalize_page(q.page, q.page_size);
    match state
        .product_service
        .list_paginated(page, page_size, q.sort_by, q.sort_dir)
        .await
    {
        Ok(p) => {
            let tag_names = tag_name_map(&state).await;
            let items = p
                .items
                .into_iter()
                .map(|prod| {
                    let names = resolve_names(&prod, &tag_names);
                    ProductResponse::from_product(prod, names)
                })
                .collect();
            Json(ApiResponse::ok(PageResponse::new(items, p.total, page, page_size)))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// POST /api/products：创建商品
pub async fn create_product(
    State(state): State<AppState>,
    Json(req): Json<ProductCreateRequest>,
) -> Json<ApiResponse<ProductResponse>> {
    match state
        .product_service
        .create_product(req.name, req.tag_ids.unwrap_or_default(), req.remark)
        .await
    {
        Ok(product) => {
            let tag_names = tag_name_map(&state).await;
            let names = resolve_names(&product, &tag_names);
            Json(ApiResponse::ok(ProductResponse::from_product(product, names)))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/products/{id}：商品详情
pub async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<ProductResponse>> {
    match state.product_service.get_product(id).await {
        Ok(product) => {
            let tag_names = tag_name_map(&state).await;
            let names = resolve_names(&product, &tag_names);
            Json(ApiResponse::ok(ProductResponse::from_product(product, names)))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// PUT /api/products/{id}：更新商品（部分字段）
pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ProductUpdateRequest>,
) -> Json<ApiResponse<ProductResponse>> {
    let patch = crate::application::product_service::ProductPatch {
        name: req.name,
        tag_ids: req.tag_ids,
        remark: req.remark,
    };
    match state.product_service.update_product(id, patch).await {
        Ok(product) => {
            let tag_names = tag_name_map(&state).await;
            let names = resolve_names(&product, &tag_names);
            Json(ApiResponse::ok(ProductResponse::from_product(product, names)))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// GET /api/products/{id}/latest-items：该商品最后一轮抓取的明细（「查看明细」弹窗用）
pub async fn latest_product_items(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<Vec<super::dto::ItemResponse>>> {
    // 商品不存在 → 404 语义；存在但还没抓过 → 空数组
    if let Err(e) = state.product_service.get_product(id).await {
        return Json(ApiResponse::err(e.to_string()));
    }
    match state.item_service.latest_for_product(id).await {
        Ok(items) => Json(ApiResponse::ok(
            items.into_iter().map(super::dto::ItemResponse::from).collect(),
        )),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// DELETE /api/products/{id}：删除商品
pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.product_service.delete_product(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

/// 按 tag_ids 顺序解析标签名（忽略已不存在的 id）
fn resolve_names(product: &Product, map: &HashMap<i64, String>) -> Vec<String> {
    product
        .tag_ids
        .iter()
        .filter_map(|id| map.get(id).cloned())
        .collect()
}

/// id → 标签名映射；标签加载失败时退化为空映射
async fn tag_name_map(state: &AppState) -> HashMap<i64, String> {
    state
        .tag_service
        .list_tags()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|t| (t.id, t.name.as_str().to_string()))
        .collect()
}

/// POST /api/products/batch：批量导入商品
pub async fn batch_create_products(
    State(state): State<AppState>,
    Json(req): Json<ProductBatchCreateRequest>,
) -> Json<ApiResponse<ProductBatchCreateResponse>> {
    match state
        .product_service
        .batch_create(req.names, req.tag_ids.unwrap_or_default())
        .await
    {
        Ok(result) => {
            let tag_names = tag_name_map(&state).await;
            let created: Vec<ProductResponse> = result
                .created
                .into_iter()
                .map(|p| {
                    let names = resolve_names(&p, &tag_names);
                    ProductResponse::from_product(p, names)
                })
                .collect();
            let skipped: Vec<BatchSkippedItem> = result
                .skipped
                .into_iter()
                .map(|(name, reason)| BatchSkippedItem { name, reason })
                .collect();
            Json(ApiResponse::ok(ProductBatchCreateResponse { created, skipped }))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
