//! Settings API (`docs/notifications.md`, "Secrets"): the flat list leaves secrets out; one
//! key in detail says whether a secret is set; writes are checked, and a batch write leaves a
//! secret sent empty as it is.

use std::collections::{BTreeMap, HashMap};

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
use crate::auth::settings::{self, check_key, check_value, is_known, is_secret};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, update_many))
        .routes(routes!(one, update_one))
}

fn failed(what: &str, e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "{what}");
    (StatusCode::INTERNAL_SERVER_ERROR, what.to_string()).into_response()
}

/// Every setting except the secrets, as `{key: value}`.
#[utoipa::path(get, path = "/api/settings", tag = "settings", operation_id = "list_settings",
    responses((status = 200, body = BTreeMap<String, String>)))]
async fn list(State(state): State<AppState>) -> Response {
    match settings::all(&state.db).await {
        Ok(rows) => {
            let map: BTreeMap<String, String> =
                rows.into_iter().filter(|(k, _)| !is_secret(k)).collect();
            Json(map).into_response()
        }
        Err(e) => failed("Failed to get settings", e),
    }
}

#[derive(Serialize, ToSchema)]
struct SettingView {
    key: String,
    /// Empty for a secret.
    value: String,
    secret: bool,
    is_set: bool,
    updated_at: Option<String>,
}

/// One setting; a secret's value is never sent back, only whether it is set. A key the server
/// reads that has no value answers `is_set: false` (unset); an unknown key with none, 404.
#[utoipa::path(get, path = "/api/settings/{key}", tag = "settings", operation_id = "get_setting",
    params(("key" = String, Path)),
    responses((status = 200, body = SettingView), (status = 404, body = ErrorBody)))]
async fn one(State(state): State<AppState>, Path(key): Path<String>) -> Response {
    let row = settings::row(&state.db, &key).await;
    let secret = is_secret(&key);
    match row {
        Ok(Some((value, updated_at))) => Json(SettingView {
            is_set: !value.is_empty(),
            value: if secret { String::new() } else { value },
            key,
            secret,
            updated_at,
        })
        .into_response(),
        // A known key that was never set (a secret too) is a normal answer, not a missing
        // resource: unset. An unknown one with nothing stored is a 404, so a typo shows.
        Ok(None) if is_known(&key) => Json(SettingView {
            key,
            value: String::new(),
            secret,
            is_set: false,
            updated_at: None,
        })
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            format!("No setting {key}: the server does not read it and nothing is stored under it"),
        )
            .into_response(),
        Err(e) => failed("Failed to get the setting", e),
    }
}

#[derive(Deserialize, ToSchema)]
struct SetSettingRequest {
    value: String,
}

/// Write one value as given: an empty value clears it, secrets included.
#[utoipa::path(put, path = "/api/settings/{key}", tag = "settings", operation_id = "set_setting",
    params(("key" = String, Path)), request_body = SetSettingRequest,
    responses((status = 204, description = "Saved"),
        (status = 400, description = "Out of range or not a known form", body = ErrorBody)))]
async fn update_one(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<SetSettingRequest>,
) -> Response {
    if let Err(msg) = check_value(&key, &req.value) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => return failed("Failed to set setting", e),
    };
    if let Err(e) = settings::write(&mut tx, &key, &req.value).await {
        return failed("Failed to set setting", e);
    }
    if let Err(e) = tx.commit().await {
        return failed("Failed to set setting", e);
    }
    applied(&state, [key.as_str()]).await;
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize, ToSchema)]
struct UpdateMany {
    values: HashMap<String, String>,
}

#[derive(Serialize, ToSchema)]
struct Updated {
    /// The keys written, sorted (secrets sent empty are left out)
    updated: Vec<String>,
}

/// Write several values at once, all or none. A secret sent empty is left as it is, so a form
/// can send every field without knowing the stored token.
#[utoipa::path(put, path = "/api/settings", tag = "settings", operation_id = "set_settings",
    request_body = UpdateMany,
    responses((status = 200, body = Updated),
        (status = 400, description = "A key is unknown or a value out of range", body = ErrorBody)))]
async fn update_many(State(state): State<AppState>, Json(req): Json<UpdateMany>) -> Response {
    let keys = match to_write(&req.values) {
        Ok(k) => k,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => return failed("Failed to save the settings", e),
    };
    for key in &keys {
        if let Err(e) = settings::write(&mut tx, key, &req.values[*key]).await {
            return failed("Failed to save the settings", e);
        }
    }
    if let Err(e) = tx.commit().await {
        return failed("Failed to save the settings", e);
    }
    applied(&state, keys.iter().map(|k| k.as_str())).await;
    Json(Updated {
        updated: keys.into_iter().cloned().collect(),
    })
    .into_response()
}

/// The keys a batch writes, sorted: all checked first, secrets sent empty left out.
fn to_write(values: &HashMap<String, String>) -> Result<Vec<&String>, String> {
    for (key, value) in values {
        check_key(key).and_then(|()| check_value(key, value))?;
    }
    let mut keys: Vec<&String> = values
        .iter()
        .filter(|(k, v)| !(is_secret(k) && v.is_empty()))
        .map(|(k, _)| k)
        .collect();
    keys.sort();
    Ok(keys)
}

/// Tell the services that cache a setting.
async fn applied<'a>(state: &AppState, keys: impl IntoIterator<Item = &'a str>) {
    let (mut ror, mut alarms) = (false, false);
    for k in keys {
        ror |= k.starts_with("ror_");
        alarms |= k == "roast_alarms" || k.starts_with("auto_");
    }
    if ror {
        state.telemetry_service.load_ror_settings().await;
    }
    if alarms {
        state.alarms.reload().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_batch_keeps_secrets_sent_empty_and_refuses_as_a_whole() {
        let v = values(&[
            ("notify_gotify_url", "https://g.example.org"),
            ("notify_gotify_token", ""),
            ("notify_gotify_enabled", "true"),
            ("notify_ntfy_token", "new"),
        ]);
        let keys = to_write(&v).unwrap();
        assert_eq!(
            keys,
            [
                "notify_gotify_enabled",
                "notify_gotify_url",
                "notify_ntfy_token"
            ]
        );
        let bad = values(&[("notify_gotify_url", "https://ok"), ("backup_hour", "25")]);
        assert!(to_write(&bad).unwrap_err().contains("hour"));
    }
}
