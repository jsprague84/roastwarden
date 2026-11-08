//! Prediction from the roaster's own model (`docs/prediction.md`): the
//! roaster's fitted model and its score, the live prediction during a roast, and how the
//! prediction did on one roast.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::predict::live::Prediction;
use crate::predict::{ModelView, RoastScore};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_model))
        .routes(routes!(fit_model))
        .routes(routes!(get_prediction))
        .routes(routes!(roast_prediction))
}

fn failed(e: anyhow::Error, what: &'static str) -> Response {
    tracing::error!(error = %format!("{e:#}"), "{what}");
    (StatusCode::INTERNAL_SERVER_ERROR, what).into_response()
}

/// The roaster's thermal model: the fit in use (or the virtual roaster's model before it has
/// roasts) with its leave-one-roast-out score per way of predicting, where BT settles at a few
/// heater and fan values, and how the live prediction did on the recent roasts against
/// today's projection.
#[utoipa::path(get, path = "/api/roaster/{device_id}/model", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = ModelView)))]
async fn get_model(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    match state.predict.view(&device_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => failed(e, "Could not read the model"),
    }
}

/// Fit the roaster's model now from its completed roasts (the server also refits by itself
/// when a roast completes). With no roasts the model stays the prior.
#[utoipa::path(post, path = "/api/roaster/{device_id}/model/fit", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = ModelView)))]
async fn fit_model(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    if let Err(e) = state.predict.fit_now(&device_id).await {
        return failed(e, "Could not fit the model");
    }
    match state.predict.view(&device_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => failed(e, "Could not read the model"),
    }
}

/// The live prediction while a roast runs, from CHARGE + 30 s to DROP: BT for the next two
/// minutes under the plan (heater and fan held, the PID's setpoint, or the drive's
/// setpoints) with a 90 % band and its reason, first crack from this bean's and profile's
/// history, and DROP from the profile. Updated every 5 s; times from CHARGE in s, °C.
#[utoipa::path(get, path = "/api/roaster/{device_id}/prediction", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = Prediction),
        (status = 404, description = "No roast between CHARGE + 30 s and DROP", body = ErrorBody)))]
async fn get_prediction(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    match state.predict.live(&device_id).await {
        Some(p) => Json(p).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "no prediction: no roast on this roaster between CHARGE + 30 s and DROP",
        )
            .into_response(),
    }
}

/// How the live prediction did on one roast: its BT 60 and 120 s ahead against what came,
/// beside today's projection at the same moments, and first crack and DROP as predicted
/// about two minutes before.
#[utoipa::path(get, path = "/api/sessions/{id}/prediction", tag = "sessions",
    params(("id" = String, Path, description = "The roast's id")),
    responses((status = 200, body = RoastScore),
        (status = 404, description = "No such roast", body = ErrorBody)))]
async fn roast_prediction(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::roasts::sessions::get_session(&state.db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "no such roast").into_response(),
        Err(e) => return failed(e, "Could not read the roast"),
    }
    match state.predict.score(&id).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => failed(e, "Could not read the roast's predictions"),
    }
}
