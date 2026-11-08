//! Traceability (docs/reports.md, "Traceability"): a roast back to its
//! green lot and where its coffee went, or a green coffee to every bag. Reads: any token.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::reports::trace::{self, Trace};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(trace_roast))
        .routes(routes!(trace_bean))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct TzQuery {
    /// The owner's offset from UTC, minutes (default: the `time_zone` setting, else UTC)
    tz: Option<i32>,
}

fn reply(r: anyhow::Result<Option<Trace>>, what: &str) -> Response {
    match r {
        Ok(Some(t)) => Json(t).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, format!("no {what}")).into_response(),
        Err(e) => {
            tracing::error!(?e, "trace of a {what}");
            (StatusCode::INTERNAL_SERVER_ERROR, "the trace failed").into_response()
        }
    }
}

/// A roast back to its green coffee (bean, supplier, lot, origin, process, purchases) and
/// where its coffee went: packed bags (form and grind), label runs, gifts, use, discards,
/// what is left. No prices.
#[utoipa::path(get, path = "/api/trace/roast/{id}", tag = "beans",
    params(("id" = String, Path, description = "The roast"), TzQuery),
    responses((status = 200, body = Trace), (status = 404, body = ErrorBody)))]
async fn trace_roast(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TzQuery>,
) -> Response {
    let zone = crate::ops::zone::resolve(&state.db, q.tz).await;
    reply(
        trace::of_roast(&state.db, &id, zone, chrono::Utc::now()).await,
        "roast",
    )
}

/// A green coffee to every finished roast of it and where each went.
#[utoipa::path(get, path = "/api/trace/bean/{id}", tag = "beans",
    params(("id" = String, Path, description = "The bean"), TzQuery),
    responses((status = 200, body = Trace), (status = 404, body = ErrorBody)))]
async fn trace_bean(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<TzQuery>,
) -> Response {
    let zone = crate::ops::zone::resolve(&state.db, q.tz).await;
    reply(
        trace::of_bean(&state.db, &id, zone, chrono::Utc::now()).await,
        "bean",
    )
}
