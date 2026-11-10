//! Notification channels (`docs/notifications.md`): what is set up (secrets only as "set"),
//! how each channel's last send went, the system problems that have not cleared, and a test
//! through one channel with the form's unsaved values. Saving goes through `PUT
//! /api/settings`, which keeps a secret sent empty.

use std::collections::HashMap;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::settings;
use crate::http::errors::ErrorBody;
use crate::notify::{Channel, ChannelStatus, Problem};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(overview))
        .routes(routes!(test))
}

/// What is set up, how each channel's last send went, and the open problems.
#[derive(Serialize, ToSchema)]
pub struct NotificationOverview {
    channels: Channels,
    events: Events,
    problems: Vec<Problem>,
}

#[derive(Serialize, ToSchema)]
struct Channels {
    push: PushChannel,
    gotify: GotifyChannel,
    ntfy: NtfyChannel,
    matrix: MatrixChannel,
    webhook: WebhookChannel,
}

#[derive(Serialize, ToSchema)]
struct PushChannel {
    /// Always on: a device signs up from Settings → Notifications
    enabled: bool,
    devices: usize,
    status: ChannelStatus,
}

#[derive(Serialize, ToSchema)]
struct GotifyChannel {
    enabled: bool,
    url: String,
    token_set: bool,
    status: ChannelStatus,
}

#[derive(Serialize, ToSchema)]
struct NtfyChannel {
    enabled: bool,
    url: String,
    topic: String,
    token_set: bool,
    status: ChannelStatus,
}

#[derive(Serialize, ToSchema)]
struct MatrixChannel {
    enabled: bool,
    homeserver: String,
    room: String,
    token_set: bool,
    status: ChannelStatus,
}

#[derive(Serialize, ToSchema)]
struct WebhookChannel {
    enabled: bool,
    /// The URL is a secret: only whether it is set
    url_set: bool,
    status: ChannelStatus,
}

/// Which system events notify
#[derive(Serialize, ToSchema)]
struct Events {
    backup: bool,
    offline: bool,
    stock: bool,
    /// How long a roaster is silent outside a roast before it counts as offline
    offline_minutes: i64,
}

#[derive(Serialize, ToSchema)]
struct Message {
    message: String,
}

/// What is set up, how each channel's last send went, and the open problems.
#[utoipa::path(get, path = "/api/notifications", tag = "notifications",
    operation_id = "notification_overview",
    responses((status = 200, body = NotificationOverview)))]
async fn overview(State(state): State<AppState>) -> Response {
    let stored = match settings::with_prefix(&state.db, "notify_").await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "could not read the notification settings");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not read the notification settings",
            )
                .into_response();
        }
    };
    let get = |k: &str| stored.get(k).cloned().unwrap_or_default();
    let on = |k: &str| stored.get(k).is_some_and(|v| v == "true");
    let set = |k: &str| stored.get(k).is_some_and(|v| !v.trim().is_empty());
    let flag = |k: &str| stored.get(k).is_none_or(|v| v == "true");
    let status = state.notifier.statuses().await;
    let st = |c: Channel| status.get(&c).cloned().unwrap_or_default();
    let devices = state.push.list().await.map_or(0, |d| d.len());
    let problems = state.notifier.problems().await.unwrap_or_default();
    Json(NotificationOverview {
        channels: Channels {
            push: PushChannel {
                enabled: true,
                devices,
                status: st(Channel::Push),
            },
            gotify: GotifyChannel {
                enabled: on("notify_gotify_enabled"),
                url: get("notify_gotify_url"),
                token_set: set("notify_gotify_token"),
                status: st(Channel::Gotify),
            },
            ntfy: NtfyChannel {
                enabled: on("notify_ntfy_enabled"),
                url: get("notify_ntfy_url"),
                topic: get("notify_ntfy_topic"),
                token_set: set("notify_ntfy_token"),
                status: st(Channel::Ntfy),
            },
            matrix: MatrixChannel {
                enabled: on("notify_matrix_enabled"),
                homeserver: get("notify_matrix_homeserver"),
                room: get("notify_matrix_room"),
                token_set: set("notify_matrix_token"),
                status: st(Channel::Matrix),
            },
            webhook: WebhookChannel {
                enabled: on("notify_webhook_enabled"),
                url_set: set("notify_webhook_url"),
                status: st(Channel::Webhook),
            },
        },
        events: Events {
            backup: flag("notify_event_backup"),
            offline: flag("notify_event_offline"),
            stock: flag("notify_event_stock"),
            offline_minutes: get("notify_offline_minutes").parse::<i64>().unwrap_or(3),
        },
        problems,
    })
    .into_response()
}

#[derive(Deserialize, ToSchema)]
struct TestRequest {
    /// `push`, `gotify`, `ntfy`, `matrix` or `webhook`
    channel: String,
    /// The form's values (`notify_*` keys); an empty secret means the stored one.
    #[serde(default)]
    values: HashMap<String, String>,
}

/// Send a test through one channel with the form's values, saved or not.
#[utoipa::path(post, path = "/api/notifications/test", tag = "notifications",
    operation_id = "test_notification", request_body = TestRequest,
    responses((status = 200, body = Message),
        (status = 400, description = "Not set up, or the channel refused the test", body = ErrorBody)))]
async fn test(State(state): State<AppState>, Json(req): Json<TestRequest>) -> Response {
    let Some(channel) = Channel::parse(&req.channel) else {
        return (StatusCode::BAD_REQUEST, "No such channel").into_response();
    };
    for (k, v) in &req.values {
        if let Err(msg) = settings::check_value(k, v) {
            return (StatusCode::BAD_REQUEST, msg).into_response();
        }
    }
    match state.notifier.test(channel, req.values).await {
        Ok(detail) => Json(Message {
            message: format!("Test sent: {detail}"),
        })
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response(),
    }
}
