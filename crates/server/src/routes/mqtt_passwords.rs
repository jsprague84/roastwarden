//! Roasters' MQTT passwords (`mqtt_passwords.rs`; docs/add-roaster.md, "MQTT passwords"): who
//! may sign in to the built-in broker, a new roaster's password added, changed or removed at
//! once, without a restart.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;
use crate::roasters::mqtt_passwords::{
    ChangeMqttPassword, MqttPasswordGiven, MqttPasswordList, NewMqttPassword, PasswordError,
};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, add))
        .routes(routes!(change, remove))
}

fn failed(e: PasswordError) -> Response {
    let status = match &e {
        PasswordError::Invalid(_) => StatusCode::BAD_REQUEST,
        PasswordError::NotFound(_) => StatusCode::NOT_FOUND,
        PasswordError::NoBuiltInBroker | PasswordError::Exists(_) | PasswordError::FromEnv(_) => {
            StatusCode::CONFLICT
        }
        PasswordError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string()).into_response()
}

/// A password answered once: never cached
fn given(status: StatusCode, roaster_id: &str, password: String) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(MqttPasswordGiven {
            roaster_id: roaster_id.to_string(),
            password,
        }),
    )
        .into_response()
}

/// A roaster mid-roast, mid-drive, mid-autotune, heating or cooling keeps its password: a
/// change would sign it out of the broker.
async fn busy(state: &AppState, id: &str) -> Option<Response> {
    let busy = crate::control::busy::check(state).await;
    busy.roasters.iter().find(|r| r.device_id == id).map(|_| {
        (
            StatusCode::CONFLICT,
            format!(
                "{id} is busy (roasting, driving, tuning, heating or cooling): a new password \
                 would sign it out. Change it when it is idle."
            ),
        )
            .into_response()
    })
}

/// The roasters that may sign in to the built-in broker: `MQTT_DEVICES`' and those added here,
/// whether each is connected now and set up under Roasters. Never a password.
#[utoipa::path(get, path = "/api/mqtt/passwords", tag = "devices",
    responses((status = 200, body = MqttPasswordList)))]
async fn list(State(state): State<AppState>) -> Response {
    let added: Vec<String> = match crate::roasters::registry::list_devices(&state.db, None).await {
        // Set up: not only found streaming ("Found, not added yet")
        Ok(d) => d
            .into_iter()
            .filter(|d| d.status != crate::roasters::types::DeviceStatus::Pending)
            .map(|d| d.device_id)
            .collect(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    Json(state.mqtt_passwords.list(&added).await).into_response()
}

/// A new roaster's password (left out: a random one); it may sign in at once. Answered once:
/// the firmware download fills it in later, the list never shows it.
#[utoipa::path(post, path = "/api/mqtt/passwords", tag = "devices",
    request_body = NewMqttPassword,
    responses((status = 201, body = MqttPasswordGiven),
        (status = 400, description = "An id or password that cannot be used", body = ErrorBody),
        (status = 409, description = "It has one already, it is in MQTT_DEVICES, or the server uses a broker of its own", body = ErrorBody)))]
async fn add(State(state): State<AppState>, Json(req): Json<NewMqttPassword>) -> Response {
    let id = req.roaster_id.trim();
    match state.mqtt_passwords.add(id, req.password).await {
        Ok(pw) => given(StatusCode::CREATED, id, pw),
        Err(e) => failed(e),
    }
}

/// A roaster's new password (left out: a random one). Connected with the old one, it is signed
/// out and must come back with the new one; refused while it is busy.
#[utoipa::path(put, path = "/api/mqtt/passwords/{roaster_id}", tag = "devices",
    params(("roaster_id" = String, Path)),
    request_body(content = Option<ChangeMqttPassword>, description = "Optional: a password of your own"),
    responses((status = 200, body = MqttPasswordGiven),
        (status = 400, body = ErrorBody),
        (status = 404, description = "It has no password here", body = ErrorBody),
        (status = 409, description = "It is busy, its password is in MQTT_DEVICES, or the server uses a broker of its own", body = ErrorBody)))]
async fn change(
    State(state): State<AppState>,
    Path(roaster_id): Path<String>,
    OptJson(req): OptJson<ChangeMqttPassword>,
) -> Response {
    if let Some(r) = busy(&state, &roaster_id).await {
        return r;
    }
    let req = req.unwrap_or_default();
    match state.mqtt_passwords.change(&roaster_id, req.password).await {
        Ok(pw) => given(StatusCode::OK, &roaster_id, pw),
        Err(e) => failed(e),
    }
}

/// A roaster's password removed: it is signed out and can no longer sign in. Refused while it
/// is busy. Its roasts and setup stay.
#[utoipa::path(delete, path = "/api/mqtt/passwords/{roaster_id}", tag = "devices",
    params(("roaster_id" = String, Path)),
    responses((status = 204, description = "Removed"),
        (status = 404, description = "It has no password here", body = ErrorBody),
        (status = 409, description = "It is busy, its password is in MQTT_DEVICES, or the server uses a broker of its own", body = ErrorBody)))]
async fn remove(State(state): State<AppState>, Path(roaster_id): Path<String>) -> Response {
    if let Some(r) = busy(&state, &roaster_id).await {
        return r;
    }
    match state.mqtt_passwords.remove(&roaster_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => failed(e),
    }
}
