//! Fan assist settings and state (`docs/fan-assist.md`).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::control::fan_assist::{DeviceAssistState, FanAssistConfig};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_assist, put_assist))
}

#[derive(Serialize, ToSchema)]
struct FanAssistView {
    device_id: String,
    config: FanAssistConfig,
    state: DeviceAssistState,
}

async fn view(state: &AppState, device_id: &str) -> Response {
    Json(FanAssistView {
        device_id: device_id.to_string(),
        config: state.fan_assist.config(device_id).await,
        state: state.fan_assist.state(device_id).await,
    })
    .into_response()
}

/// Fan assist's settings and what it is doing (docs/fan-assist.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/fan_assist", tag = "roasters",
    params(("device_id" = String, Path)), responses((status = 200, body = FanAssistView)))]
async fn get_assist(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    view(&state, &device_id).await
}

/// Save fan assist's settings for this roaster.
#[utoipa::path(put, path = "/api/roaster/{device_id}/fan_assist", tag = "roasters",
    params(("device_id" = String, Path)), request_body = FanAssistConfig,
    responses((status = 200, body = FanAssistView), (status = 400, body = ErrorBody)))]
async fn put_assist(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(c): Json<FanAssistConfig>,
) -> Response {
    if let Err(e) = state.fan_assist.set_config(&device_id, c).await {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    view(&state, &device_id).await
}
