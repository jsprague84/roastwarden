//! Roasts (`roast_sessions`): plan, start, pause, resume, complete, cancel; their details,
//! samples, and Artisan and CSV files. Events and cupping are in `roast_events.rs`, profile
//! drive in `replay.rs`.

use axum::{
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::http::errors::ErrorBody;
use crate::roasts::types::{
    CreateSessionRequest, RoastSession, SessionWithTelemetry, UpdateSessionRequest,
};
use crate::{app::state::AppState, profiles::artisan};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_list_sessions, api_create_session))
        .routes(routes!(
            api_get_session,
            api_update_session,
            api_delete_session
        ))
        .routes(routes!(api_start_session))
        .routes(routes!(api_pause_session))
        .routes(routes!(api_resume_session))
        .routes(routes!(api_complete_session))
        .routes(routes!(api_cancel_session))
        .routes(routes!(api_get_session_telemetry, api_add_telemetry_point))
        .routes(routes!(api_export_csv))
        .routes(routes!(api_export_artisan))
        .routes(routes!(api_import_artisan_session))
}

/// Plan a roast. With `bean_id` it draws its green weight from that bean's stock; with
/// `planned_roast_id` it fills what the request leaves out from the plan.
#[utoipa::path(post, path = "/api/sessions", tag = "sessions", request_body = CreateSessionRequest,
    responses((status = 200, body = RoastSession)))]
async fn api_create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Response {
    match crate::roasts::sessions::create_session(&state.db, req).await {
        Ok(session) => Json(session).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to create session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create session",
            )
                .into_response()
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct SessionListQuery {
    /// Only this roaster's
    device_id: Option<String>,
    /// At most this many
    limit: Option<i32>,
}

/// Roasts, newest first.
#[utoipa::path(get, path = "/api/sessions", tag = "sessions", params(SessionListQuery),
    responses((status = 200, body = Vec<RoastSession>)))]
async fn api_list_sessions(
    State(state): State<AppState>,
    Query(q): Query<SessionListQuery>,
) -> Response {
    match crate::roasts::sessions::list_sessions(&state.db, q.device_id.as_deref(), q.limit).await {
        Ok(sessions) => Json(sessions).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to list sessions");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list sessions").into_response()
        }
    }
}

/// A roast with its telemetry and its profile.
#[utoipa::path(get, path = "/api/sessions/{id}", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = SessionWithTelemetry), (status = 404, body = ErrorBody)))]
async fn api_get_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::sessions::get_session_with_telemetry(&state.db, &id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to get session");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get session").into_response()
        }
    }
}

/// Edit a roast's details (weights, colour, notes, rating, …): absent keeps, `null` clears.
#[utoipa::path(put, path = "/api/sessions/{id}", tag = "sessions", params(("id" = String, Path)),
    request_body = UpdateSessionRequest,
    responses((status = 200, body = RoastSession), (status = 404, body = ErrorBody)))]
async fn api_update_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> Response {
    match crate::roasts::sessions::update_session(&state.db, &id, req).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to update session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update session",
            )
                .into_response()
        }
    }
}

/// Delete a roast (a running drive stops).
#[utoipa::path(delete, path = "/api/sessions/{id}", tag = "sessions", params(("id" = String, Path)),
    responses((status = 204, description = "Deleted"), (status = 404, body = ErrorBody)))]
async fn api_delete_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    state.replay.stop(&id, "session deleted").await;
    match crate::roasts::sessions::delete_session(&state.db, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to delete session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete session",
            )
                .into_response()
        }
    }
}

/// Cancel a roast (docs/roast-sequence.md): the drive stops, heater power goes off, a hot
/// chamber cools, and the roast is deleted. A bean's stock gets its green weight back.
#[utoipa::path(post, path = "/api/sessions/{id}/cancel", tag = "sessions", params(("id" = String, Path)),
    responses((status = 204, description = "Cancelled"), (status = 404, body = ErrorBody)))]
async fn api_cancel_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.sequence.cancel(&id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to cancel session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to cancel session",
            )
                .into_response()
        }
    }
}

/// Start a planned roast (CHARGE by the clock, docs/roast-sequence.md).
#[utoipa::path(post, path = "/api/sessions/{id}/start", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = RoastSession),
        (status = 404, description = "No such roast, or not planned", body = ErrorBody)))]
async fn api_start_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::sessions::start_session(&state.db, &id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            "Session not found or not in planning state",
        )
            .into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to start session");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to start session").into_response()
        }
    }
}

/// Pause a roast.
#[utoipa::path(post, path = "/api/sessions/{id}/pause", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = RoastSession),
        (status = 404, description = "No such roast, or not active", body = ErrorBody)))]
async fn api_pause_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::sessions::pause_session(&state.db, &id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found or not active").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to pause session");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to pause session").into_response()
        }
    }
}

/// Resume a paused roast.
#[utoipa::path(post, path = "/api/sessions/{id}/resume", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = RoastSession),
        (status = 404, description = "No such roast, or not paused", body = ErrorBody)))]
async fn api_resume_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::sessions::resume_session(&state.db, &id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found or not paused").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to resume session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resume session",
            )
                .into_response()
        }
    }
}

/// Finish a roast: the drive stops and the statistics are computed.
#[utoipa::path(post, path = "/api/sessions/{id}/complete", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = RoastSession),
        (status = 404, description = "No such roast, or not active or paused", body = ErrorBody)))]
async fn api_complete_session(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    state.replay.stop(&id, "session completed").await;
    match crate::roasts::sessions::complete_session(&state.db, &id).await {
        Ok(Some(session)) => Json(session).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            "Session not found or not active/paused",
        )
            .into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to complete session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to complete session",
            )
                .into_response()
        }
    }
}

/// The same as `GET /api/sessions/{id}`.
#[utoipa::path(get, path = "/api/sessions/{id}/telemetry", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, body = SessionWithTelemetry), (status = 404, body = ErrorBody)))]
async fn api_get_session_telemetry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::roasts::sessions::get_session_with_telemetry(&state.db, &id).await {
        Ok(Some(session_with_telemetry)) => Json(session_with_telemetry).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to get session with telemetry");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get session with telemetry",
            )
                .into_response()
        }
    }
}

#[derive(Deserialize, ToSchema)]
struct TelemetryPointRequest {
    elapsed_seconds: f32,
    bean_temp: Option<f32>,
    env_temp: Option<f32>,
    rate_of_rise: Option<f32>,
    /// %
    heater: Option<f64>,
    /// %
    fan: Option<f64>,
    setpoint: Option<f32>,
}

/// Add one sample to a roast by hand (the server records the roaster's own).
#[utoipa::path(post, path = "/api/sessions/{id}/telemetry", tag = "sessions", params(("id" = String, Path)),
    request_body = TelemetryPointRequest,
    responses((status = 201, description = "Added")))]
async fn api_add_telemetry_point(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<TelemetryPointRequest>,
) -> Response {
    match crate::roasts::telemetry_store::add_telemetry_point(
        &state.db,
        &id,
        req.elapsed_seconds,
        req.bean_temp,
        req.env_temp,
        req.rate_of_rise,
        req.heater,
        req.fan,
        req.setpoint,
    )
    .await
    {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to add telemetry point");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to add telemetry point",
            )
                .into_response()
        }
    }
}

#[derive(Deserialize, ToSchema)]
struct ImportSessionRequest {
    /// The `.alog` file's text
    alog_content: String,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// Import a recorded Artisan roast (`.alog`) as a completed roast.
#[utoipa::path(post, path = "/api/sessions/import/artisan", tag = "sessions",
    request_body = ImportSessionRequest,
    responses((status = 201, body = RoastSession),
        (status = 400, description = "Not a roast Artisan wrote", body = ErrorBody)))]
async fn api_import_artisan_session(
    State(state): State<AppState>,
    Json(req): Json<ImportSessionRequest>,
) -> Response {
    match crate::roasts::import::import_artisan_session(
        &state.db,
        &req.alog_content,
        req.device_id,
        req.name,
    )
    .await
    {
        Ok(session) => (StatusCode::CREATED, Json(session)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "Artisan session import failed");
            (StatusCode::BAD_REQUEST, format!("Import failed: {e}")).into_response()
        }
    }
}

/// The roast as CSV, one row a sample.
#[utoipa::path(get, path = "/api/sessions/{id}/export/csv", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, content_type = "text/csv", body = String), (status = 404, body = ErrorBody)))]
async fn api_export_csv(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::export::export_csv(&state.db, &id).await {
        Ok(Some((csv, filename))) => {
            let headers = [
                (CONTENT_TYPE, "text/csv; charset=utf-8"),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ];
            (headers, csv).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to export CSV");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to export CSV").into_response()
        }
    }
}

/// The roast as an Artisan `.alog` (docs/profiles.md, "Compatibility notes").
#[utoipa::path(get, path = "/api/sessions/{id}/export/artisan", tag = "sessions", params(("id" = String, Path)),
    responses((status = 200, content_type = "text/plain", body = String), (status = 404, body = ErrorBody)))]
async fn api_export_artisan(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::export::export_alog(&state.db, &id).await {
        Ok(Some((alog, filename))) => {
            let body = artisan::render_alog(&alog);
            let headers = [
                (CONTENT_TYPE, "text/plain; charset=utf-8"),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ];
            (headers, body).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to export Artisan JSON");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export Artisan JSON",
            )
                .into_response()
        }
    }
}
