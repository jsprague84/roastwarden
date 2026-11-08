//! API tokens (Settings → Tokens) and the per-roaster "tokens may control" switch
//! (Roasters → the roaster → Control). Only the owner key reaches the writes and the token
//! list: `tokens::OWNER_ONLY`, enforced by the auth middleware.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::tokens::{self, ApiToken, CreateError, Preset, RoasterAccess};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(revoke))
        .routes(routes!(update, remove))
        .routes(routes!(get_access, put_access))
}

#[derive(Serialize, ToSchema)]
struct CreatedToken {
    token: ApiToken,
    /// The token itself (`rr_…`): shown once, never stored
    secret: String,
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "token request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "the token store failed").into_response()
}

/// Every token, newest first; never with its secret. Owner key only.
#[utoipa::path(get, path = "/api/tokens", tag = "tokens", operation_id = "list_tokens",
    responses((status = 200, body = Vec<ApiToken>)))]
async fn list(State(state): State<AppState>) -> Response {
    match tokens::list(&state.db).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
struct CreateBody {
    name: String,
    /// `read_only`, `roast`, `inventory` or `admin` (docs/control.md, "Tokens")
    preset: String,
    /// Days until it expires; absent or null: never.
    expires_in_days: Option<u32>,
    /// Starting a roast on a physical roaster and deletes wait for the owner's approval
    /// (docs/control.md, "Approvals"). Default true.
    require_approval: Option<bool>,
}

/// Make a token. Owner key only; needs `ROASTWARDEN_API_KEY` set.
#[utoipa::path(post, path = "/api/tokens", tag = "tokens", operation_id = "create_token",
    request_body = CreateBody,
    responses((status = 201, body = CreatedToken),
        (status = 400, body = ErrorBody),
        (status = 409, description = "The name is taken, or no owner key is set", body = ErrorBody)))]
async fn create(State(state): State<AppState>, Json(b): Json<CreateBody>) -> Response {
    if !state.auth.enabled() {
        return (
            StatusCode::CONFLICT,
            "Set ROASTWARDEN_API_KEY first: without it the API is open to anyone and a token restricts nothing.",
        )
            .into_response();
    }
    let Some(preset) = Preset::parse(&b.preset) else {
        return (
            StatusCode::BAD_REQUEST,
            "preset must be read_only, roast, inventory or admin",
        )
            .into_response();
    };
    let expires_at = match b.expires_in_days {
        None => None,
        Some(d @ 1..=3650) => Some(chrono::Utc::now() + chrono::Duration::days(d as i64)),
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "expires_in_days must be 1 to 3650, or null for never",
            )
                .into_response()
        }
    };
    let require_approval = b.require_approval.unwrap_or(true);
    match tokens::create(&state.db, &b.name, preset, expires_at, require_approval).await {
        Ok((token, secret)) => {
            (StatusCode::CREATED, Json(CreatedToken { token, secret })).into_response()
        }
        Err(CreateError::Invalid(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
        Err(CreateError::NameTaken) => (
            StatusCode::CONFLICT,
            "a token with that name exists (remove it first, or pick another name)",
        )
            .into_response(),
        Err(CreateError::Db(e)) => failed(e),
    }
}

/// Revoke a token: it stops working at once and stays in the list. Owner key only.
#[utoipa::path(post, path = "/api/tokens/{id}/revoke", tag = "tokens", operation_id = "revoke_token",
    params(("id" = i64, Path)),
    responses((status = 200, body = ApiToken), (status = 404, body = ErrorBody)))]
async fn revoke(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match tokens::revoke(&state.db, id).await {
        Ok(Some(t)) => Json(t).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such token").into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
struct UpdateBody {
    /// Starting a roast on a physical roaster and deletes wait for the owner's approval
    require_approval: bool,
}

/// Change a token: whether its gated calls wait for approval. Owner key only.
#[utoipa::path(put, path = "/api/tokens/{id}", tag = "tokens", operation_id = "update_token",
    params(("id" = i64, Path)), request_body = UpdateBody,
    responses((status = 200, body = ApiToken), (status = 404, body = ErrorBody)))]
async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<UpdateBody>,
) -> Response {
    match tokens::set_require_approval(&state.db, id, b.require_approval).await {
        Ok(Some(t)) => Json(t).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no such token").into_response(),
        Err(e) => failed(e),
    }
}

/// Remove a token. Owner key only.
#[utoipa::path(delete, path = "/api/tokens/{id}", tag = "tokens", operation_id = "delete_token",
    params(("id" = i64, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match tokens::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such token").into_response(),
        Err(e) => failed(e),
    }
}

/// Whether tokens may control this roaster.
#[utoipa::path(get, path = "/api/roaster/{device_id}/token-access", tag = "tokens",
    params(("device_id" = String, Path)),
    responses((status = 200, body = RoasterAccess)))]
async fn get_access(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    match tokens::roaster_access(&state.db, &device_id).await {
        Ok(a) => Json(a).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
struct AccessBody {
    tokens_may_control: bool,
}

/// Let tokens control this roaster, or stop them. Owner key only.
#[utoipa::path(put, path = "/api/roaster/{device_id}/token-access", tag = "tokens",
    params(("device_id" = String, Path)), request_body = AccessBody,
    responses((status = 200, body = RoasterAccess),
        (status = 400, description = "The virtual roaster needs no switch", body = ErrorBody)))]
async fn put_access(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(b): Json<AccessBody>,
) -> Response {
    if crate::sim::virtual_roaster::is_simulated(&device_id) {
        return (
            StatusCode::BAD_REQUEST,
            "tokens may always control the virtual roaster",
        )
            .into_response();
    }
    match tokens::set_roaster_access(&state.db, &device_id, b.tokens_may_control).await {
        Ok(a) => Json(a).into_response(),
        Err(e) => failed(e),
    }
}
