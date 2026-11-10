//! A roast's events (CHARGE … DROP, docs/roast-sequence.md) and its cupping scores.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::roasts::types::{
    CreateCuppingRequest, CreateRoastEventRequest, CuppingWithAttributes, RoastEvent,
    RoastEventType, UpdateRoastEventRequest,
};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_get_roast_events, api_create_roast_event))
        .routes(routes!(api_update_roast_event, api_delete_roast_event))
        .routes(routes!(
            api_get_cupping,
            api_create_cupping,
            api_delete_cupping
        ))
}

/// A roast's events (CHARGE, turning point, DRY, FC, …, DROP), in time order.
#[utoipa::path(get, path = "/api/sessions/{session_id}/events", tag = "events", params(("session_id" = String, Path)),
    responses((status = 200, body = Vec<RoastEvent>)))]
async fn api_get_roast_events(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Response {
    match crate::roasts::events::get_roast_events(&state.db, &session_id).await {
        Ok(events) => Json(events).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to get roast events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get roast events",
            )
                .into_response()
        }
    }
}

/// A finished roast's numbers follow its landmarks (first crack moved off a lone early crack
/// changes its DTR, development RoR and AUC). Does nothing during a roast.
async fn recompute(state: &AppState, session_id: &str) {
    if let Err(e) = crate::roasts::sessions::recompute_statistics(&state.db, session_id).await {
        tracing::warn!(?e, session_id, "could not recompute the roast's statistics");
    }
}

/// Mark an event. CHARGE and DROP on a live roast act on the roaster
/// (docs/roast-sequence.md). A roast has one CHARGE and one DROP: a second is refused
/// (409 `ALREADY_MARKED`); move the one there with PUT instead.
#[utoipa::path(post, path = "/api/sessions/{session_id}/events", tag = "events", params(("session_id" = String, Path)),
    request_body = CreateRoastEventRequest, responses((status = 201, body = RoastEvent),
        (status = 409, description = "CHARGE or DROP is already marked on this roast", body = ErrorBody)))]
async fn api_create_roast_event(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<CreateRoastEventRequest>,
) -> Response {
    // A page that missed an auto DROP (a hidden tab) still shows the DROP button: a
    // second DROP would end the roast again at the cooled BT.
    if matches!(
        req.event_type,
        RoastEventType::Charge | RoastEventType::Drop
    ) && let Ok(events) = crate::roasts::events::get_roast_events(&state.db, &session_id).await
        && events.iter().any(|e| e.event_type == req.event_type)
    {
        let name = req.event_type.to_string().to_uppercase();
        let mut res = (
            StatusCode::CONFLICT,
            format!("{name} is already marked on this roast"),
        )
            .into_response();
        res.extensions_mut()
            .insert(crate::http::errors::ErrorCode("ALREADY_MARKED"));
        return res;
    }
    match crate::roasts::events::create_roast_event(&state.db, &session_id, req).await {
        Ok(event) => {
            state.alarms.invalidate_events(&session_id).await;
            recompute(&state, &session_id).await;
            (StatusCode::CREATED, Json(event)).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "Failed to create roast event");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create roast event",
            )
                .into_response()
        }
    }
}

/// Move or annotate an event.
#[utoipa::path(put, path = "/api/sessions/{session_id}/events/{event_id}", tag = "events",
    params(("session_id" = String, Path), ("event_id" = String, Path)), request_body = UpdateRoastEventRequest,
    responses((status = 200, body = RoastEvent),
        (status = 400, description = "Nothing to change", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn api_update_roast_event(
    State(state): State<AppState>,
    Path((session_id, event_id)): Path<(String, String)>,
    Json(req): Json<UpdateRoastEventRequest>,
) -> Response {
    if req.elapsed_seconds.is_none() && req.temperature.is_none() && req.notes.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "Nothing to change: send elapsed_seconds, temperature or notes",
        )
            .into_response();
    }
    match crate::roasts::events::update_roast_event(&state.db, &event_id, req).await {
        Ok(Some(event)) => {
            state.alarms.invalidate_events(&session_id).await;
            recompute(&state, &session_id).await;
            Json(event).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Roast event not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to update roast event");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update roast event",
            )
                .into_response()
        }
    }
}

/// Remove an event.
#[utoipa::path(delete, path = "/api/sessions/{session_id}/events/{event_id}", tag = "events",
    params(("session_id" = String, Path), ("event_id" = String, Path)),
    responses((status = 204, description = "Removed")))]
async fn api_delete_roast_event(
    State(state): State<AppState>,
    Path((session_id, event_id)): Path<(String, String)>,
) -> Response {
    match crate::roasts::events::delete_roast_event(&state.db, &event_id).await {
        Ok(()) => {
            state.alarms.invalidate_events(&session_id).await;
            recompute(&state, &session_id).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!(?e, "Failed to delete roast event");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete roast event",
            )
                .into_response()
        }
    }
}

/// The roast's cupping scores, or `null`.
#[utoipa::path(get, path = "/api/sessions/{session_id}/cupping", tag = "events",
    params(("session_id" = String, Path)), responses((status = 200, body = Option<CuppingWithAttributes>)))]
async fn api_get_cupping(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Response {
    match crate::roasts::cupping::get_cupping(&state.db, &session_id).await {
        Ok(cupping) => Json(cupping).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to get cupping");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get cupping").into_response()
        }
    }
}

/// Score the roast on the classic SCA form, replacing an earlier score (`docs/cupping.md`):
/// some or all of the ten attributes, and the cups with a taint or a fault. The answer has the
/// total, defects, final score and band once all ten are scored.
#[utoipa::path(post, path = "/api/sessions/{session_id}/cupping", tag = "events",
    params(("session_id" = String, Path)), request_body = CreateCuppingRequest,
    responses((status = 201, body = CuppingWithAttributes),
        (status = 400, description = "A score the form does not allow: the reason", body = ErrorBody),
        (status = 404, description = "No such roast", body = ErrorBody)))]
async fn api_create_cupping(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(req): Json<CreateCuppingRequest>,
) -> Response {
    match crate::roasts::cupping::create_cupping(&state.db, &session_id, req).await {
        Ok(Some(cupping)) => (StatusCode::CREATED, Json(cupping)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "No such roast").into_response(),
        Err(e) => {
            if let Some(bad) = e.downcast_ref::<crate::profiles::validate::ProfileInputError>() {
                return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
            }
            tracing::error!(?e, "Failed to create cupping");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create cupping",
            )
                .into_response()
        }
    }
}

/// Remove the roast's cupping scores.
#[utoipa::path(delete, path = "/api/sessions/{session_id}/cupping", tag = "events",
    params(("session_id" = String, Path)), responses((status = 204, description = "Removed")))]
async fn api_delete_cupping(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Response {
    match crate::roasts::cupping::delete_cupping(&state.db, &session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to delete cupping");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete cupping",
            )
                .into_response()
        }
    }
}
