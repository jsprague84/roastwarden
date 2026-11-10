//! Web Push subscriptions (docs/push.md): the server's public key, the devices that get
//! alarm notifications, and a test notification.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;
use crate::notify::push::{Delivery, Message, NewSubscription, Subscription};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(key))
        .routes(routes!(list, subscribe))
        .routes(routes!(remove))
        .routes(routes!(unsubscribe))
        .routes(routes!(test))
}

#[derive(Serialize, ToSchema)]
struct PublicKey {
    /// The server's VAPID key (base64url), for `pushManager.subscribe`
    public_key: String,
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "push request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
}

/// The key a browser subscribes with.
#[utoipa::path(get, path = "/api/push/key", tag = "push", operation_id = "push_key",
    responses((status = 200, body = PublicKey)))]
async fn key(State(state): State<AppState>) -> Response {
    match state.push.public_key().await {
        Ok(public_key) => Json(PublicKey { public_key }).into_response(),
        Err(e) => failed(e),
    }
}

/// The devices that get alarm notifications.
#[utoipa::path(get, path = "/api/push/subscriptions", tag = "push",
    operation_id = "list_push_subscriptions",
    responses((status = 200, body = Vec<Subscription>)))]
async fn list(State(state): State<AppState>) -> Response {
    match state.push.list().await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => failed(e),
    }
}

/// Sign a browser up (`PushSubscription.toJSON()` and a name).
#[utoipa::path(post, path = "/api/push/subscriptions", tag = "push",
    operation_id = "push_subscribe", request_body = NewSubscription,
    responses((status = 201, body = Subscription), (status = 400, body = ErrorBody)))]
async fn subscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(s): Json<NewSubscription>,
) -> Response {
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    match state.push.subscribe(s, origin).await {
        Ok(sub) => (StatusCode::CREATED, Json(sub)).into_response(),
        Err(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
    }
}

/// Remove a device.
#[utoipa::path(delete, path = "/api/push/subscriptions/{id}", tag = "push",
    operation_id = "delete_push_subscription", params(("id" = i64, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match state.push.remove(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such subscription").into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
struct Endpoint {
    endpoint: String,
}

/// A browser turning notifications off names its own endpoint (it does not know its id).
#[utoipa::path(post, path = "/api/push/unsubscribe", tag = "push", operation_id = "push_unsubscribe",
    request_body = Endpoint,
    responses((status = 204, description = "Removed, or was not signed up")))]
async fn unsubscribe(State(state): State<AppState>, Json(b): Json<Endpoint>) -> Response {
    match state.push.remove_endpoint(&b.endpoint).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
struct TestBody {
    /// One subscription; all when absent.
    id: Option<i64>,
}

/// Send a test notification to one device, or to all.
#[utoipa::path(post, path = "/api/push/test", tag = "push", operation_id = "push_test",
    request_body(content = Option<TestBody>, description = "Optional"),
    responses((status = 200, body = Vec<Delivery>)))]
async fn test(State(state): State<AppState>, OptJson(body): OptJson<TestBody>) -> Response {
    let only = body.and_then(|b| b.id);
    let msg = Message {
        title: "Roastwarden test".into(),
        body: "Alarm notifications reach this device.".into(),
        tag: "test".into(),
        url: "/settings".into(),
    };
    match state.push.send(&msg, only).await {
        Ok(d) => {
            state.notifier.record_push(&d).await;
            Json(d).into_response()
        }
        Err(e) => failed(e),
    }
}
