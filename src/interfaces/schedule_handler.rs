use axum::extract::{Json, Path, State};

use crate::application::schedule_service::UpdateSchedule;
use crate::domain::crawl_schedule::{NewCrawlSchedule, ScheduleName};

use super::dto::{ApiResponse, ScheduleCreateRequest, ScheduleResponse, ScheduleUpdateRequest};
use super::AppState;

pub async fn list_schedules(State(state): State<AppState>) -> Json<ApiResponse<Vec<ScheduleResponse>>> {
    match state.schedule_service.list().await {
        Ok(items) => Json(ApiResponse::ok(items.into_iter().map(Into::into).collect())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

pub async fn get_schedule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<ScheduleResponse>> {
    match state.schedule_service.get(id).await {
        Ok(item) => Json(ApiResponse::ok(item.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

pub async fn create_schedule(
    State(state): State<AppState>,
    Json(req): Json<ScheduleCreateRequest>,
) -> Json<ApiResponse<ScheduleResponse>> {
    let name = match ScheduleName::new(req.name) {
        Ok(name) => name,
        Err(e) => return Json(ApiResponse::err(e.to_string())),
    };
    match state
        .schedule_service
        .create(NewCrawlSchedule {
            name,
            tag_ids: req.tag_ids,
            every_days: req.every_days,
            queue_interval_secs: req.queue_interval_secs.unwrap_or(3),
            first_run_at: req.first_run_at.unwrap_or_else(crate::domain::crawl_task::now_unix),
        })
        .await
    {
        Ok(item) => Json(ApiResponse::ok(item.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

pub async fn update_schedule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ScheduleUpdateRequest>,
) -> Json<ApiResponse<ScheduleResponse>> {
    match state
        .schedule_service
        .update(
            id,
            UpdateSchedule {
                name: req.name,
                tag_ids: req.tag_ids,
                every_days: req.every_days,
                queue_interval_secs: req.queue_interval_secs,
                enabled: req.enabled,
                next_run_at: req.next_run_at,
            },
        )
        .await
    {
        Ok(item) => Json(ApiResponse::ok(item.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

pub async fn delete_schedule(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<()>> {
    match state.schedule_service.delete(id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}

pub async fn run_now(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Json<ApiResponse<ScheduleResponse>> {
    match state.schedule_service.run_now(id).await {
        Ok(item) => Json(ApiResponse::ok(item.into())),
        Err(e) => Json(ApiResponse::err(e.to_string())),
    }
}
