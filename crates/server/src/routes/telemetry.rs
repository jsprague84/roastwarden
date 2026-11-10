//! What the roasters report: the latest telemetry, the roasters heard on MQTT and the
//! transports that reach each. The sample history and the live stream are in `ws2.rs`
//! (docs/websocket-v2.md).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::http::errors::ErrorBody;
use crate::{app::state::AppState, app::state::DeviceInfo};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_get_latest_telemetry))
        .routes(routes!(api_get_transports))
        .routes(routes!(api_get_devices))
}

#[derive(Serialize, ToSchema)]
struct Transports {
    device_id: String,
    /// `mqtt`, `websocket`, `modbus`, `serial`
    live: Vec<&'static str>,
}

#[derive(Serialize, ToSchema)]
struct LatestTelemetryResponse {
    device_id: String,
    /// When it arrived, Unix seconds
    timestamp: u64,
    /// The message as the roaster sent it
    telemetry: serde_json::Value,
}

/// The roaster's latest telemetry message, as it sent it (the firmware's names).
#[utoipa::path(get, path = "/api/roaster/{device_id}/telemetry/latest", tag = "telemetry",
    params(("device_id" = String, Path)),
    responses((status = 200, body = LatestTelemetryResponse),
        (status = 404, description = "No telemetry from this roaster yet", body = ErrorBody)))]
async fn api_get_latest_telemetry(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let map = state.telemetry_cache.read().await;
    if let Some((val, ts)) = map.get(&device_id) {
        Json(LatestTelemetryResponse {
            device_id,
            timestamp: *ts,
            telemetry: val.clone(),
        })
        .into_response()
    } else {
        (StatusCode::NOT_FOUND, "No telemetry").into_response()
    }
}

/// The command transports that reach the roaster now, best first.
#[utoipa::path(get, path = "/api/roaster/{device_id}/transports", tag = "telemetry",
    params(("device_id" = String, Path)), responses((status = 200, body = Transports)))]
async fn api_get_transports(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> Response {
    let live = state.transports.live_transports(&device_id).await;
    Json(Transports { device_id, live }).into_response()
}

#[derive(Serialize, ToSchema)]
struct DevicesResponse {
    devices: Vec<DeviceInfo>,
}

/// Roasters heard on MQTT, with their last status.
#[utoipa::path(get, path = "/api/devices/registry", tag = "telemetry",
    responses((status = 200, body = DevicesResponse)))]
async fn api_get_devices(State(state): State<AppState>) -> Response {
    let reg = state.device_registry.read().await;
    let list: Vec<_> = reg.values().cloned().collect();
    Json(DevicesResponse { devices: list }).into_response()
}
