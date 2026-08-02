use std::collections::HashMap;

use axum::extract::{Json, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::domain::product::Product;

use super::dto::{
    ApiResponse, PageResponse, PriceTrendPoint, PriceTrendQuery, PriceTrendSeries,
    ProductBatchCreateRequest, ProductBatchCreateResponse,
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
        .list_paginated(page, page_size, q.sort_by, q.sort_dir, q.search, q.tag_id)
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
        recycle_price: req.recycle_price,
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
    let product = match state.product_service.get_product(id).await {
        Ok(p) => p,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    let product_name = product.name.as_str().to_string();
    match state.item_service.latest_for_product(id).await {
        Ok(items) => Json(ApiResponse::ok(
            items
                .into_iter()
                .map(|it| {
                    let mut resp: super::dto::ItemResponse = it.into();
                    resp.product_name = Some(product_name.clone());
                    resp
                })
                .collect(),
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

/// GET /api/products/export：导出全部商品为 Excel 文件
pub async fn export_products(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let tag_names = tag_name_map(&state).await;

    let products = match state.product_service.list_all().await {
        Ok(p) => p,
        Err(e) => {
            return axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(axum::body::Body::from(e.to_string()))
                .unwrap()
        }
    };

    let buf = match build_excel(&products, &tag_names) {
        Ok(b) => b,
        Err(e) => {
            return axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(axum::body::Body::from(format!("生成 Excel 失败: {e}")))
                .unwrap()
        }
    };

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"products.xlsx\"",
        )
        .body(axum::body::Body::from(buf))
        .unwrap()
}

fn build_excel(
    products: &[Product],
    tag_names: &HashMap<i64, String>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use rust_xlsxwriter::*;

    let mut workbook = Workbook::new();
    let mut sheet = workbook.add_worksheet();

    let header_fmt = Format::new().set_bold().set_background_color(Color::RGB(0xD9E2F3));
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss");
    let headers = [
        "ID", "商品名", "标签", "备注", "中位数价格", "均价", "爬取数量",
        "最后爬取时间", "回收价格", "创建时间", "更新时间",
    ];
    for (col, h) in headers.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *h, &header_fmt)?;
    }

    for (row, p) in products.iter().enumerate() {
        let r = row as u32 + 1;
        sheet.write_number(r, 0, p.id as f64)?;
        sheet.write_string(r, 1, p.name.as_str())?;
        let tags: Vec<&str> = p.tag_ids.iter()
            .filter_map(|id| tag_names.get(id).map(|s| s.as_str()))
            .collect();
        sheet.write_string(r, 2, &tags.join("、"))?;
        sheet.write_string(r, 3, p.remark.as_deref().unwrap_or(""))?;
        write_opt_number(&mut sheet, r, 4, p.median_price)?;
        write_opt_number(&mut sheet, r, 5, p.avg_price)?;
        if let Some(v) = p.crawled_count {
            sheet.write_number(r, 6, v as f64)?;
        }
        write_opt_datetime(&mut sheet, r, 7, p.last_crawled_at, &date_fmt)?;
        write_opt_number(&mut sheet, r, 8, p.recycle_price)?;
        write_opt_datetime(&mut sheet, r, 9, Some(p.created_at), &date_fmt)?;
        write_opt_datetime(&mut sheet, r, 10, Some(p.updated_at), &date_fmt)?;
    }

    for col in 0..headers.len() as u16 {
        sheet.set_column_width(col, 16)?;
    }

    workbook.save_to_buffer().map_err(|e| e.into())
}

fn unix_to_excel_date(ts: u64) -> f64 {
    ts as f64 / 86400.0 + 25569.0
}

fn write_opt_number(
    sheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: Option<f64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(v) = value {
        sheet.write_number(row, col, v)?;
    }
    Ok(())
}

fn write_opt_datetime(
    sheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: Option<u64>,
    fmt: &rust_xlsxwriter::Format,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(v) = value {
        sheet.write_number_with_format(row, col, unix_to_excel_date(v), fmt)?;
    }
    Ok(())
}

/// GET /api/products/price-trend?product_ids=1,2,3：多商品价格趋势数据
pub async fn price_trend(
    State(state): State<AppState>,
    Query(q): Query<PriceTrendQuery>,
) -> Json<ApiResponse<Vec<PriceTrendSeries>>> {
    match state.trend_service.compute(&q.product_ids).await {
        Ok(series) => {
            let data: Vec<PriceTrendSeries> = series
                .into_iter()
                .map(|s| PriceTrendSeries {
                    product_id: s.product_id,
                    product_name: s.product_name,
                    points: s.points.into_iter().map(PriceTrendPoint::from).collect(),
                })
                .collect();
            Json(ApiResponse::ok(data))
        }
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
