//! Roasted coffee: resting and ready (`docs/roasted.md`).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::coffee::inventory::InventoryInputError;
use crate::coffee::roasted::{
    CombineRequest, CombinedLot, RoastedLot, RoastedMoveRequest, RoastedService, RoastedStock,
};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(roasted_stock))
        .routes(routes!(roasted_lot))
        .routes(routes!(add_roasted_move))
        .routes(routes!(delete_roasted_move))
        .routes(routes!(combine_roasts))
        .routes(routes!(combined_lot, uncombine))
}

fn svc(state: &AppState) -> RoastedService {
    RoastedService::new(state.db.clone())
}

fn reply(r: anyhow::Result<Option<RoastedLot>>, what: &str) -> Response {
    match r {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            if let Some(bad) = e.downcast_ref::<InventoryInputError>() {
                return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
            }
            tracing::error!(?e, "{what}");
            (StatusCode::INTERNAL_SERVER_ERROR, what.to_string()).into_response()
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct StockQuery {
    /// The owner's offset from UTC, minutes: dates are the owner's dates (default: the
    /// `time_zone` setting, else UTC)
    tz: Option<i32>,
    /// Finished roasts too
    #[serde(default)]
    all: bool,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct TzQuery {
    /// The owner's offset from UTC, minutes (default: the `time_zone` setting, else UTC)
    tz: Option<i32>,
}

/// Roasted coffee: each roast's coffee left, resting or ready by its rest window, and for
/// espresso, filter and either what is ready, resting, how fast it goes and when to roast
/// again (docs/roasted.md).
#[utoipa::path(get, path = "/api/roasted", tag = "beans", params(StockQuery),
    responses((status = 200, body = RoastedStock)))]
async fn roasted_stock(State(state): State<AppState>, Query(q): Query<StockQuery>) -> Response {
    match svc(&state)
        .stock(
            crate::ops::zone::resolve(&state.db, q.tz).await,
            q.all,
            chrono::Utc::now(),
        )
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to read roasted stock");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read roasted stock",
            )
                .into_response()
        }
    }
}

/// One roast's roasted coffee and its moves.
#[utoipa::path(get, path = "/api/roasted/{id}", tag = "beans",
    params(("id" = String, Path, description = "The roast"), TzQuery),
    responses((status = 200, body = RoastedLot), (status = 404, body = ErrorBody)))]
async fn roasted_lot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TzQuery>,
) -> Response {
    reply(
        svc(&state)
            .lot(
                &id,
                crate::ops::zone::resolve(&state.db, q.tz).await,
                chrono::Utc::now(),
            )
            .await,
        "Failed to read the roast's coffee",
    )
}

/// Coffee used, given or thrown away (grams, doses, bags or all), or a count of what is left
/// (`adjust`).
#[utoipa::path(post, path = "/api/roasted/{id}/moves", tag = "beans",
    params(("id" = String, Path, description = "The roast"), TzQuery),
    request_body = RoastedMoveRequest,
    responses((status = 200, body = RoastedLot), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn add_roasted_move(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TzQuery>,
    Json(req): Json<RoastedMoveRequest>,
) -> Response {
    reply(
        svc(&state)
            .add_move(
                &id,
                req,
                crate::ops::zone::resolve(&state.db, q.tz).await,
                chrono::Utc::now(),
            )
            .await,
        "Failed to record the move",
    )
}

/// Take a move back.
#[utoipa::path(delete, path = "/api/roasted/{id}/moves/{move_id}", tag = "beans",
    params(("id" = String, Path, description = "The roast"), ("move_id" = String, Path), TzQuery),
    responses((status = 200, body = RoastedLot), (status = 404, body = ErrorBody)))]
async fn delete_roasted_move(
    State(state): State<AppState>,
    Path((id, move_id)): Path<(String, String)>,
    Query(q): Query<TzQuery>,
) -> Response {
    reply(
        svc(&state)
            .delete_move(
                &id,
                &move_id,
                crate::ops::zone::resolve(&state.db, q.tz).await,
                chrono::Utc::now(),
            )
            .await,
        "Failed to take the move back",
    )
}

/// Roasts in one bag: put two or more roasts of one coffee, for the same use, into a combined
/// lot (or the others join the lot one of them is in). Each must be weighed and have coffee
/// left. From then on a move on any of them is split across them by what each has left, and
/// a label for any of them prints the lot (docs/roasted.md, "Roasts in one bag").
#[utoipa::path(post, path = "/api/roasted/combined", tag = "beans",
    params(TzQuery),
    request_body = CombineRequest,
    responses((status = 200, body = CombinedLot), (status = 400, body = ErrorBody)))]
async fn combine_roasts(
    State(state): State<AppState>,
    Query(q): Query<TzQuery>,
    Json(req): Json<CombineRequest>,
) -> Response {
    let r = svc(&state)
        .combine(
            req,
            crate::ops::zone::resolve(&state.db, q.tz).await,
            chrono::Utc::now(),
        )
        .await;
    match r {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            if let Some(bad) = e.downcast_ref::<InventoryInputError>() {
                return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
            }
            tracing::error!(?e, "Failed to combine roasts");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to combine roasts",
            )
                .into_response()
        }
    }
}

/// A combined lot: what is left of its roasts together, when it is ready, its roasts and the
/// moves made on it with how each was split. `id` is the lot's or any of its roasts'.
#[utoipa::path(get, path = "/api/roasted/combined/{id}", tag = "beans",
    params(("id" = String, Path, description = "The combined lot, or one of its roasts"), TzQuery),
    responses((status = 200, body = CombinedLot), (status = 404, body = ErrorBody)))]
async fn combined_lot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TzQuery>,
) -> Response {
    match svc(&state)
        .combined(
            &id,
            crate::ops::zone::resolve(&state.db, q.tz).await,
            chrono::Utc::now(),
        )
        .await
    {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such combined lot").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to read the combined lot");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read the combined lot",
            )
                .into_response()
        }
    }
}

/// The roasts a combined lot held, separate again.
#[derive(Serialize, ToSchema)]
struct Uncombined {
    roasts: Vec<String>,
}

/// Split a combined lot apart: its roasts are separate again, each keeping its share of the
/// moves made on the lot. `id` is the lot's or any of its roasts'.
#[utoipa::path(delete, path = "/api/roasted/combined/{id}", tag = "beans",
    params(("id" = String, Path, description = "The combined lot, or one of its roasts")),
    responses((status = 200, body = Uncombined), (status = 404, body = ErrorBody)))]
async fn uncombine(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match svc(&state).uncombine(&id).await {
        Ok(Some(roasts)) => Json(Uncombined { roasts }).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such combined lot").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to split the combined lot");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to split the combined lot",
            )
                .into_response()
        }
    }
}
