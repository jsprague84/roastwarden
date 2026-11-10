//! Approvals (docs/control.md, "Approvals"; `crate::auth::approvals`): the owner's list and
//! decisions, and the one request a token waits on.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::approvals::{Approval, Decision};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_approvals))
        .routes(routes!(get_approval))
        .routes(routes!(approve))
        .routes(routes!(deny))
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "approval request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "the approval store failed",
    )
        .into_response()
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ListQuery {
    /// At most this many (default 100)
    limit: Option<i64>,
}

/// Every approval request, the pending ones first, then newest first. Owner key only.
#[utoipa::path(get, path = "/api/approvals", tag = "approvals", params(ListQuery),
    responses((status = 200, body = Vec<Approval>)))]
async fn list_approvals(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Response {
    match state.approvals.list(q.limit.unwrap_or(100)).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct WaitQuery {
    /// Seconds to wait for a decision (at most 25): the answer comes as soon as the owner
    /// decides, or with the request still pending
    wait: Option<u64>,
}

/// One approval request by its id (the token that asked waits on it with `?wait=25`).
#[utoipa::path(get, path = "/api/approvals/{id}", tag = "approvals",
    params(("id" = String, Path), WaitQuery),
    responses((status = 200, body = Approval), (status = 404, body = ErrorBody)))]
async fn get_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<WaitQuery>,
) -> Response {
    let found = match q.wait {
        Some(secs) if secs > 0 => state.approvals.wait(&id, secs).await,
        _ => state.approvals.get(&id).await,
    };
    match found {
        Ok(Some(a)) => Json(a).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such approval request").into_response(),
        Err(e) => failed(e),
    }
}

async fn decide(state: AppState, id: String, approve: bool) -> Response {
    match state.approvals.decide(&id, approve).await {
        Ok(Decision::Decided(a)) => Json(*a).into_response(),
        Ok(Decision::NotFound) => {
            (StatusCode::NOT_FOUND, "no such approval request").into_response()
        }
        Ok(Decision::Closed(status)) => (
            StatusCode::CONFLICT,
            format!("This request is {status} and can no longer be decided"),
        )
            .into_response(),
        Err(e) => failed(e),
    }
}

/// Approve a pending request: the held call is made now, as the token that asked; the
/// answer says what it answered (`result_status`, `result`). Owner key only.
#[utoipa::path(post, path = "/api/approvals/{id}/approve", tag = "approvals",
    params(("id" = String, Path)),
    responses((status = 200, body = Approval), (status = 404, body = ErrorBody),
        (status = 409, description = "Already decided, or expired", body = ErrorBody)))]
async fn approve(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    decide(state, id, true).await
}

/// Deny a pending request: nothing is done. Owner key only.
#[utoipa::path(post, path = "/api/approvals/{id}/deny", tag = "approvals",
    params(("id" = String, Path)),
    responses((status = 200, body = Approval), (status = 404, body = ErrorBody),
        (status = 409, description = "Already decided, or expired", body = ErrorBody)))]
async fn deny(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    decide(state, id, false).await
}
