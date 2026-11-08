//! The server-run autotune (docs/autotune-2.md): start, stop and its state.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::control::tune::{StartTune, TuneError, TuneState};
use crate::http::errors::ErrorBody;
use crate::{app::dispatch::PublishOpts, app::state::AppState};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(start_tune))
        .routes(routes!(stop_tune))
        .routes(routes!(get_tune))
}

/// Start the server's autotune on a roaster: heat to the level, a symmetric relay that finds
/// the holding power, an asymmetric one, then the analysis (docs/autotune-2.md). The fan is
/// set first when `fan_pct` is given. Not during a roast or a drive.
#[utoipa::path(post, path = "/api/roaster/{device_id}/tune/start", tag = "autotune",
    params(("device_id" = String, Path), PublishOpts), request_body = StartTune,
    responses((status = 200, body = TuneState),
        (status = 400, body = ErrorBody),
        (status = 409, description = "A roast, a drive or a test is on the roaster, or it is offline", body = ErrorBody),
        (status = 502, description = "The roaster did not take a command", body = ErrorBody)))]
async fn start_tune(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(opts): Query<PublishOpts>,
    Json(req): Json<StartTune>,
) -> Response {
    let floor = state.transports.limits(&device_id).await.fan_floor_pct;
    if let Err(e) = req.config(floor) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    // Nothing reaches the roaster (the fan included) unless a test may start.
    if let Err(e) = state.tune.precheck(&device_id).await {
        return error(e);
    }
    // A roast that just dropped may still be cooling: its fan-off would land in the test and
    // the fan interlock would keep the heater off for the whole approach.
    state
        .sequence
        .end_cooling(&device_id, "the server autotune takes the roaster")
        .await;
    if let Some(f) = req.fan_pct {
        let res = crate::routes::control::set_fan(&state, &device_id, f, &opts).await;
        if !res.status().is_success() {
            return res;
        }
    }
    match state.tune.start(&device_id, &req).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => error(e),
    }
}

fn error(e: TuneError) -> Response {
    match e {
        TuneError::Invalid(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        TuneError::Busy(e) => (StatusCode::CONFLICT, e).into_response(),
        TuneError::Offline => (
            StatusCode::CONFLICT,
            "the roaster is offline (no telemetry in the last 5 s)",
        )
            .into_response(),
        TuneError::Transport(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
        TuneError::ControlOff(e) => {
            let mut r = (StatusCode::CONFLICT, e).into_response();
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode("CONTROL_OFF"));
            r
        }
    }
}

/// Stop the test: the heater goes off and what was recorded is analysed and stored.
#[utoipa::path(post, path = "/api/roaster/{device_id}/tune/stop", tag = "autotune",
    params(("device_id" = String, Path)),
    responses((status = 200, body = TuneState),
        (status = 409, description = "No test is running", body = ErrorBody)))]
async fn stop_tune(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    if !state.tune.stop(&device_id, "stopped by the operator").await {
        return (StatusCode::CONFLICT, "no test is running on this roaster").into_response();
    }
    match state.tune.state(&device_id).await {
        Some(s) => Json(s).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// The test running on the roaster, or the last one since the server started (404: none).
#[utoipa::path(get, path = "/api/roaster/{device_id}/tune", tag = "autotune",
    params(("device_id" = String, Path)),
    responses((status = 200, body = TuneState), (status = 404, body = ErrorBody)))]
async fn get_tune(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    match state.tune.state(&device_id).await {
        Some(s) => Json(s).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "no autotune on this roaster since the server started",
        )
            .into_response(),
    }
}
