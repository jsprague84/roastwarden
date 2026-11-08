//! Roast analysis (`docs/analysis.md`): one roast read, and several roasts
//! lined up against one. Only reads; the numbers come from `analysis.rs`.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::analysis::{self, Comparison, RoastAnalysis};
use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::reports::{self, ReportError};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(roast_analysis))
        .routes(routes!(compare_roasts))
}

/// The most roasts one comparison lines up.
const MAX_ROASTS: usize = 8;

fn refused(e: ReportError) -> Response {
    match e {
        ReportError::NotFound(m) => (StatusCode::NOT_FOUND, m).into_response(),
        ReportError::Bad(m) | ReportError::Design(m) => {
            (StatusCode::BAD_REQUEST, m).into_response()
        }
        ReportError::Internal(e) => {
            tracing::error!(error = %format!("{e:#}"), "roast analysis failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not read the roast",
            )
                .into_response()
        }
    }
}

/// One roast read: its marks and phases, RoR (early peak, by minute, the late dip), BT
/// against its profile, how the heater and fan were used, and its numbers judged against
/// what was expected. Times from CHARGE in s, °C, RoR °C/min.
#[utoipa::path(get, path = "/api/sessions/{id}/analysis", tag = "sessions",
    operation_id = "roast_analysis",
    params(("id" = String, Path, description = "The roast's id")),
    responses((status = 200, body = RoastAnalysis),
        (status = 404, description = "No such roast", body = ErrorBody)))]
async fn roast_analysis(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match reports::roast::gather(&state.db, &id).await {
        Ok(input) => Json(analysis::analyse(&input).analysis).into_response(),
        Err(e) => refused(e),
    }
}

#[derive(Deserialize, IntoParams)]
pub struct CompareQuery {
    /// The roasts' ids, comma-separated, 2 to 8
    roasts: String,
    /// The one the others are measured against (default: the first)
    reference: Option<String>,
    /// How close BT counts as together, °C (default 0.5)
    within: Option<f64>,
}

/// Several roasts against one: every number with its difference from the reference, BT,
/// RoR, heater and fan every 30 s from CHARGE, and from when each roast's BT stays within
/// `within` °C of the reference's.
#[utoipa::path(get, path = "/api/analysis/compare", tag = "sessions",
    operation_id = "compare_roasts", params(CompareQuery),
    responses((status = 200, body = Comparison),
        (status = 400, description = "Fewer than 2 roasts or more than 8, or a reference not among them", body = ErrorBody),
        (status = 404, description = "No such roast", body = ErrorBody)))]
async fn compare_roasts(State(state): State<AppState>, Query(q): Query<CompareQuery>) -> Response {
    let ids: Vec<&str> = q
        .roasts
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if !(2..=MAX_ROASTS).contains(&ids.len()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("compare 2 to {MAX_ROASTS} roasts (got {})", ids.len()),
        )
            .into_response();
    }
    let reference = match q.reference.as_deref() {
        None => 0,
        Some(r) => match ids.iter().position(|i| *i == r) {
            Some(k) => k,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    "the reference is not one of the roasts",
                )
                    .into_response()
            }
        },
    };
    let within = q.within.unwrap_or(0.5);
    if !(within.is_finite() && within > 0.0 && within <= 20.0) {
        return (
            StatusCode::BAD_REQUEST,
            "within is a tolerance in °C, above 0 and at most 20",
        )
            .into_response();
    }
    let mut list = Vec::with_capacity(ids.len());
    for id in &ids {
        match reports::roast::gather(&state.db, id).await {
            Ok(input) => list.push(analysis::analyse(&input)),
            Err(e) => return refused(e),
        }
    }
    Json(analysis::compare(&list, reference, within)).into_response()
}
