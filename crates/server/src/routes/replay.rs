//! Profile drive (`replay`, docs/replay.md): start, change, pause, stop and inspect the
//! server-side drive of a live roast, the legacy follow API, and resuming after the roaster
//! cut its heater.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;
use crate::replay::engine::{Channel, EventView};
use crate::replay::{self, EventOp, ReplayError, ReplayState, StartReplay, UpdateReplay};
use crate::{app::state::AppState, control::sequence};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            api_get_replay,
            api_start_replay,
            api_update_replay,
            api_stop_replay
        ))
        .routes(routes!(api_pause_replay))
        .routes(routes!(api_resume_replay))
        .routes(routes!(api_get_replay_events))
        .routes(routes!(api_replay_event_op))
        .routes(routes!(api_get_replay_log))
        .routes(routes!(api_list_replay))
        .routes(routes!(api_resume_drive))
        // Legacy follow API: a setpoint-only drive without fan
        .routes(routes!(api_get_follow, api_start_follow, api_stop_follow))
        .routes(routes!(api_list_follow))
}

/// Legacy: the same as `GET /api/sessions/{id}/replay`.
#[utoipa::path(get, path = "/api/sessions/{id}/follow", tag = "profile drive",
    params(("id" = String, Path)), responses((status = 200, body = ReplayState)))]
async fn api_get_follow(state: State<AppState>, id: Path<String>) -> Response {
    api_get_replay(state, id).await
}

/// Legacy: the same as `DELETE /api/sessions/{id}/replay`.
#[utoipa::path(delete, path = "/api/sessions/{id}/follow", tag = "profile drive",
    params(("id" = String, Path)),
    responses((status = 200, description = "The stopped run", body = ReplayState),
        (status = 204, description = "Nothing was running")))]
async fn api_stop_follow(state: State<AppState>, id: Path<String>) -> Response {
    api_stop_replay(state, id).await
}

/// Legacy: the same as `GET /api/replay`.
#[utoipa::path(get, path = "/api/follow", tag = "profile drive",
    responses((status = 200, body = Vec<ReplayState>)))]
async fn api_list_follow(state: State<AppState>) -> Response {
    api_list_replay(state).await
}

fn replay_error_response(e: ReplayError) -> Response {
    let status = match &e {
        ReplayError::SessionNotFound => StatusCode::NOT_FOUND,
        ReplayError::SessionNotActive(_)
        | ReplayError::NoProfile
        | ReplayError::EmptyProfile
        | ReplayError::NoHeaterData
        | ReplayError::NotRunning => StatusCode::CONFLICT,
        ReplayError::Invalid(_) => StatusCode::BAD_REQUEST,
        ReplayError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        ReplayError::Transport(_) => StatusCode::BAD_GATEWAY,
        ReplayError::ControlOff(inner) => {
            let code = match inner {
                crate::transports::transport::TransportError::NotDeclared { .. } => {
                    "OUTPUT_NOT_DECLARED"
                }
                _ => "CONTROL_OFF",
            };
            let mut r = (StatusCode::CONFLICT, e.to_string()).into_response();
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode(code));
            return r;
        }
    };
    (status, e.to_string()).into_response()
}

fn replay_result(r: Result<replay::ReplayState, ReplayError>) -> Response {
    match r {
        Ok(st) => Json(st).into_response(),
        Err(e) => replay_error_response(e),
    }
}

/// Start Profile drive for a live roast (docs/replay.md): the server sends the profile's
/// setpoint, fan and heater to the roaster as the roast runs.
#[utoipa::path(post, path = "/api/sessions/{id}/replay", tag = "profile drive", params(("id" = String, Path)),
    request_body(content = Option<StartReplay>, description = "Optional; defaults from the profile"),
    responses((status = 200, body = ReplayState),
        (status = 400, description = "A setting out of range", body = ErrorBody), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody),
        (status = 502, description = "The first command did not reach the roaster", body = ErrorBody)))]
async fn api_start_replay(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(body): OptJson<StartReplay>,
) -> Response {
    let req = body.unwrap_or_default();
    replay_result(state.replay.start(&id, req).await)
}

#[derive(Deserialize, Default, ToSchema)]
struct StartFollowRequest {
    #[serde(default)]
    lookahead_seconds: Option<f64>,
}

/// Legacy follow: Profile drive of the setpoint only, without fan.
#[utoipa::path(post, path = "/api/sessions/{id}/follow", tag = "profile drive", params(("id" = String, Path)),
    request_body(content = Option<StartFollowRequest>, description = "Optional"),
    responses((status = 200, body = ReplayState), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_start_follow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(body): OptJson<StartFollowRequest>,
) -> Response {
    let req = body.unwrap_or_default();
    let start = StartReplay {
        fan: Some(false),
        lookahead_seconds: req.lookahead_seconds,
        ..StartReplay::default()
    };
    replay_result(state.replay.start(&id, start).await)
}

/// Change a running drive (look-ahead, hysteresis, channels, alignment).
#[utoipa::path(patch, path = "/api/sessions/{id}/replay", tag = "profile drive", params(("id" = String, Path)),
    request_body = UpdateReplay,
    responses((status = 200, body = ReplayState),
        (status = 400, description = "A setting out of range", body = ErrorBody), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_update_replay(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateReplay>,
) -> Response {
    replay_result(state.replay.update(&id, req).await)
}

/// Stop the drive; the roaster keeps its last setpoint.
#[utoipa::path(delete, path = "/api/sessions/{id}/replay", tag = "profile drive", params(("id" = String, Path)),
    responses((status = 200, description = "The stopped run", body = ReplayState),
        (status = 204, description = "Nothing was running")))]
async fn api_stop_replay(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.replay.stop(&id, "stopped by user").await {
        Some(st) => Json(st).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

/// The drive's state: the running one, else the last that stopped (with why), else
/// `{"session_id", "active": false}`.
#[utoipa::path(get, path = "/api/sessions/{id}/replay", tag = "profile drive", params(("id" = String, Path)),
    responses((status = 200, body = ReplayState)))]
async fn api_get_replay(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.replay.state(&id).await {
        Some(st) => Json(st).into_response(),
        // The last run once it stopped (why, and whether the roaster cut it), so a reload
        // still shows it; otherwise nothing has run.
        None => match state.replay.last_stopped(&id).await {
            Some(st) => Json(st).into_response(),
            None => Json(serde_json::json!({ "session_id": id, "active": false })).into_response(),
        },
    }
}

/// Every running drive.
#[utoipa::path(get, path = "/api/replay", tag = "profile drive",
    responses((status = 200, body = Vec<ReplayState>)))]
async fn api_list_replay(State(state): State<AppState>) -> Response {
    Json(state.replay.list().await).into_response()
}

#[derive(Deserialize, Default, ToSchema)]
struct ChannelRequest {
    #[serde(default)]
    channel: Option<Channel>,
}

/// Pause one channel (`setpoint`, `fan`, `heater`) or, without one, the whole drive.
#[utoipa::path(post, path = "/api/sessions/{id}/replay/pause", tag = "profile drive", params(("id" = String, Path)),
    request_body(content = Option<ChannelRequest>, description = "Optional"),
    responses((status = 200, body = ReplayState), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_pause_replay(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(body): OptJson<ChannelRequest>,
) -> Response {
    let req = body.unwrap_or_default();
    replay_result(
        state
            .replay
            .set_paused(&id, req.channel, true, "paused by the operator", None)
            .await,
    )
}

/// Resume a paused channel or the whole drive.
#[utoipa::path(post, path = "/api/sessions/{id}/replay/resume", tag = "profile drive", params(("id" = String, Path)),
    request_body(content = Option<ChannelRequest>, description = "Optional"),
    responses((status = 200, body = ReplayState), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_resume_replay(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(body): OptJson<ChannelRequest>,
) -> Response {
    let req = body.unwrap_or_default();
    replay_result(
        state
            .replay
            .set_paused(&id, req.channel, false, "", None)
            .await,
    )
}

/// The profile's events in this run, with their state and when they fire.
#[utoipa::path(get, path = "/api/sessions/{id}/replay/events", tag = "profile drive", params(("id" = String, Path)),
    responses((status = 200, body = Vec<EventView>),
        (status = 409, description = "No drive is running", body = ErrorBody)))]
async fn api_get_replay_events(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.replay.events(&id).await {
        Some(v) => Json(v).into_response(),
        None => replay_error_response(ReplayError::NotRunning),
    }
}

#[derive(Deserialize, ToSchema)]
struct EventOpRequest {
    op: EventOp,
    /// For `delay`: how long
    #[serde(default)]
    seconds: Option<f64>,
}

/// Skip, delay or fire one of the run's events now.
#[utoipa::path(post, path = "/api/sessions/{id}/replay/events/{event_id}", tag = "profile drive",
    params(("id" = String, Path), ("event_id" = String, Path)), request_body = EventOpRequest,
    responses((status = 200, body = ReplayState),
        (status = 400, description = "No such event, or a bad delay", body = ErrorBody), (status = 404, description = "No such roast", body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_replay_event_op(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(String, String)>,
    Json(req): Json<EventOpRequest>,
) -> Response {
    replay_result(
        state
            .replay
            .event_op(&id, &event_id, req.op, req.seconds)
            .await,
    )
}

/// The drive's log: every step it took and why, this run and the earlier ones.
#[utoipa::path(get, path = "/api/sessions/{id}/replay/log", tag = "profile drive", params(("id" = String, Path)),
    responses((status = 200, body = Object), (status = 404, body = ErrorBody)))]
async fn api_get_replay_log(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.replay.log(&id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => replay_error_response(e),
    }
}

/// Resume Profile drive after the roaster cut its heater (docs/roast-sequence.md): heater
/// power on from the start output, then the drive with its previous settings.
#[utoipa::path(post, path = "/api/sessions/{id}/drive/resume", tag = "profile drive", params(("id" = String, Path)),
    responses((status = 200, body = ReplayState),
        (status = 204, description = "Resumed; no drive state to show"),
        (status = 404, body = ErrorBody),
        (status = 409, description = "Not possible now: the reason", body = ErrorBody)))]
async fn api_resume_drive(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.sequence.resume(&id).await {
        Ok(()) => match state.replay.state(&id).await {
            Some(st) => Json(st).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        },
        Err(sequence::ResumeError::NotFound) => {
            (StatusCode::NOT_FOUND, "Session not found").into_response()
        }
        Err(sequence::ResumeError::Invalid(e)) => (StatusCode::CONFLICT, e).into_response(),
    }
}
