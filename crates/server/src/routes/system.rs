//! The server itself: health, readiness, version, metrics, the caller's credential, signing a
//! browser in and out and its logins (`logins.rs`), mDNS discovery, the MQTT reset and the
//! test helpers that publish a message.

use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use prometheus::{Encoder, TextEncoder};
use rumqttc::QoS;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::auth::tokens::{self, Preset};
use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;
use crate::roasters::discovery::DiscoveredService;
use crate::{app::state::AppState, auth, http::logins};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(healthz))
        .routes(routes!(readyz))
        .routes(routes!(version))
        .routes(routes!(metrics_handler))
        .routes(routes!(api_auth_status))
        .routes(routes!(api_demo_roaster))
        .routes(routes!(api_login))
        .routes(routes!(api_logout))
        .routes(routes!(api_logins, api_logins_end_all))
        .routes(routes!(api_login_end))
        .routes(routes!(api_mdns_devices))
        .routes(routes!(api_mqtt_reset))
        .routes(routes!(api_test_emit_telemetry))
        .routes(routes!(api_test_emit_status))
}

#[derive(Serialize, ToSchema)]
pub struct Version {
    name: &'static str,
    version: &'static str,
    /// The git commit the image was built from, or `unknown`
    revision: String,
}

#[derive(Serialize, ToSchema)]
pub struct AuthStatus {
    /// The server has an owner key: writes need a credential
    auth_enabled: bool,
    /// Reads need no credential: authentication is off, or `ROASTWARDEN_READS=open`
    reads_open: bool,
    /// A public demo (`ROASTWARDEN_DEMO=1`): a visitor with no credential is `demo`, may roast
    /// on the simulated roaster and record, and reaches no other host
    demo: bool,
    /// A demo that counts its visits (`ROASTWARDEN_DEMO_STATS_URL`): the dashboard loads the
    /// Umami tracker from here. Never set on an install that is not a demo.
    demo_stats: Option<auth::DemoStats>,
    /// `open`, `owner key`, `owner login`, `token`, `demo` or `none`
    credential: &'static str,
    /// This browser's login, when the credential came from its cookie (sign out ends it)
    login_id: Option<i64>,
    /// The token this request came with
    token: Option<TokenInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct TokenInfo {
    id: i64,
    name: String,
    preset: Preset,
}

#[derive(Serialize, ToSchema)]
pub struct MdnsServices {
    /// Browsing is on (`ROASTWARDEN_MDNS`)
    enabled: bool,
    services: Vec<DiscoveredService>,
}

/// The process is up (`ok`).
#[utoipa::path(get, path = "/healthz", tag = "system",
    responses((status = 200, content_type = "text/plain", body = String)))]
async fn healthz() -> &'static str {
    "ok"
}

/// MQTT is connected and the database answers.
#[utoipa::path(get, path = "/readyz", tag = "system",
    responses((status = 200, description = "Ready"), (status = 503, description = "Not ready")))]
async fn readyz(State(state): State<AppState>) -> axum::http::StatusCode {
    // MQTT must be ready and DB reachable
    let mqtt_ok = state.mqtt.is_ready();
    let db_ok = crate::db::is_up(&state.db).await;
    if mqtt_ok && db_ok {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    }
}

/// The server's name, version and git revision.
#[utoipa::path(get, path = "/version", tag = "system", responses((status = 200, body = Version)))]
async fn version() -> Json<Version> {
    Json(Version {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        revision: crate::routes::about::revision(),
    })
}

/// Prometheus metrics.
#[utoipa::path(get, path = "/metrics", tag = "system",
    responses((status = 200, content_type = "text/plain", body = String)))]
async fn metrics_handler() -> Response {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder
        .encode(&metric_families, &mut buf)
        .expect("the text format encodes any gathered metrics");
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, encoder.format_type())
        .body(axum::body::Body::from(buf))
        .expect("a fixed status and header")
}

/// Whether the owner key is on, and what this request's credential is (so the dashboard can
/// say whether it holds the owner key or a token).
#[utoipa::path(get, path = "/api/auth/status", tag = "system",
    responses((status = 200, body = AuthStatus)))]
async fn api_auth_status(
    State(state): State<AppState>,
    actor: Option<axum::Extension<auth::Actor>>,
    login: Option<axum::Extension<logins::LoginId>>,
) -> Response {
    let actor = actor.map_or(auth::Actor::Anonymous(None), |a| a.0);
    Json(status_of(&state.auth, &actor, login.map(|l| l.0.0))).into_response()
}

fn status_of(auth: &auth::AuthConfig, actor: &auth::Actor, login_id: Option<i64>) -> AuthStatus {
    let token = match actor {
        auth::Actor::Token(t) => Some(TokenInfo {
            id: t.id,
            name: t.name.clone(),
            preset: t.preset,
        }),
        _ => None,
    };
    AuthStatus {
        auth_enabled: auth.enabled(),
        reads_open: auth.reads_open(),
        demo: auth.demo(),
        demo_stats: auth.demo_stats().cloned(),
        credential: actor.label(),
        login_id,
        token,
    }
}

/// A demo visitor's own simulated roaster (docs/DEPLOY.md, "A simulated roaster per visitor").
#[derive(Serialize, ToSchema)]
pub struct DemoRoaster {
    /// The roaster to select: the visitor's own (`virtual_` and 8 hex digits), or
    /// `virtual_roaster` when the demo had none to spare
    device_id: String,
    /// The visitor shares `virtual_roaster` with everyone (the demo is full)
    shared: bool,
    /// Minutes without a request after which the roaster goes
    idle_minutes: u64,
}

/// A demo visitor's own simulated roaster: made on the first call (with the `rw_visitor`
/// cookie), the same one after; the shared `virtual_roaster` when the demo is full. Only on a
/// public demo, and only for a visitor: the owner key and tokens get none.
#[utoipa::path(post, path = "/api/demo/roaster", tag = "system",
    responses((status = 200, body = DemoRoaster),
        (status = 404, description = "Not a demo, or not a visitor (NOT_DEMO)", body = ErrorBody)))]
async fn api_demo_roaster(
    State(state): State<AppState>,
    actor: Option<Extension<auth::Actor>>,
    peer: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
    headers: HeaderMap,
) -> Response {
    use crate::ops::demo;
    let visitor = matches!(actor.map(|a| a.0), Some(auth::Actor::Demo));
    if !state.auth.demo() || !visitor {
        let mut r = (
            StatusCode::NOT_FOUND,
            "Only a visitor to a public demo gets a simulated roaster of their own.",
        )
            .into_response();
        r.extensions_mut()
            .insert(crate::http::errors::ErrorCode("NOT_DEMO"));
        return r;
    }
    let (cookie, fresh) = match demo::presented(&headers) {
        Some(c) => (c.to_string(), false),
        None => (demo::new_cookie(), true),
    };
    let roaster = demo::roaster_for(&cookie);
    let address = auth::client(&headers, peer.map(|p| p.0.0.ip()));
    let visitors = state.auth.visitors();
    let now = std::time::Instant::now();
    let claim = visitors.claim(&roaster, &address, now);
    let running = state.virtual_roaster.state_of(&roaster).await.is_some();
    let answer = match claim {
        demo::Claim::Full => DemoRoaster {
            device_id: crate::sim::virtual_roaster::DEVICE_ID.into(),
            shared: true,
            idle_minutes: demo::IDLE.as_secs() / 60,
        },
        demo::Claim::Own { new } => {
            if (new || !running)
                && let Err(e) = demo::provision(&state, &roaster).await
            {
                tracing::error!(error = %e, %roaster, "demo: could not make a visitor's roaster");
                visitors.release(&roaster);
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Could not make a simulated roaster: try again.",
                )
                    .into_response();
            }
            DemoRoaster {
                device_id: roaster,
                shared: false,
                idle_minutes: demo::IDLE.as_secs() / 60,
            }
        }
    };
    let mut res = Json(answer).into_response();
    if fresh {
        res.headers_mut().append(
            header::SET_COOKIE,
            demo::set_cookie(&cookie, logins::https(&headers)),
        );
    }
    res
}

#[derive(Deserialize, ToSchema)]
struct LoginBody {
    /// The owner key, or an API token (`rr_…`): the browser then acts as that token
    credential: String,
}

fn login_refused(code: &'static str, msg: &str) -> Response {
    auth::refuse(
        StatusCode::UNAUTHORIZED,
        code,
        msg.into(),
        auth::Actor::Anonymous(None),
    )
}

/// Sign this browser in: the owner key or an API token for an `HttpOnly` cookie, `rr_login`,
/// good for 30 days from its last use (SECURITY.md, "Logins"). Wrong credentials count toward
/// the 429 like any other. With authentication off it changes nothing.
#[utoipa::path(post, path = "/api/auth/login", tag = "system", request_body = LoginBody,
    responses((status = 200, body = AuthStatus, description = "Signed in; `Set-Cookie: rr_login`"),
        (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)))]
async fn api_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<std::net::SocketAddr>>>,
    Json(body): Json<LoginBody>,
) -> Response {
    let auth = &state.auth;
    if !auth.enabled() {
        return Json(status_of(auth, &auth::Actor::Open, None)).into_response();
    }
    let client = auth::client(&headers, peer.map(|p| p.0.0.ip()));
    if let Some(secs) = auth.blocked(&client) {
        let mut res = auth::refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "TOO_MANY_FAILURES",
            format!(
                "{} wrong keys or tokens came from here within a minute: wait {secs} s.",
                auth::FAIL_LIMIT
            ),
            auth::Actor::Anonymous(None),
        );
        res.headers_mut()
            .insert(header::RETRY_AFTER, header::HeaderValue::from(secs));
        return res;
    }
    let given = body.credential.trim();
    let (who, actor) = if tokens::is_token(given) {
        match tokens::check(&state.db, given, chrono::Utc::now()).await {
            tokens::TokenCheck::Valid { token, .. } => {
                (logins::Who::Token(token.id), auth::Actor::Token(token))
            }
            tokens::TokenCheck::Error => {
                return auth::refuse(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "UNAVAILABLE",
                    "Could not check the API token.".into(),
                    auth::Actor::Anonymous(None),
                );
            }
            _ => {
                auth.failed(&client);
                return login_refused(
                    "TOKEN_INVALID",
                    "That API token is unknown, revoked or expired.",
                );
            }
        }
    } else if !given.is_empty() && auth.key_ok(given) {
        let key = auth.owner_key().unwrap_or_default();
        (logins::Who::Owner(key), auth::Actor::OwnerLogin)
    } else {
        auth.failed(&client);
        return login_refused("UNAUTHORIZED", "That is not the owner key.");
    };
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let ip = crate::auth::audit::client_ip(&headers);
    match logins::create(&state.db, who, agent, ip).await {
        Ok((id, secret)) => {
            let mut res = Json(status_of(auth, &actor, Some(id))).into_response();
            res.headers_mut().append(
                header::SET_COOKIE,
                logins::cookie(&secret, logins::https(&headers)),
            );
            res.extensions_mut().insert(actor);
            res
        }
        Err(e) => {
            tracing::error!(error = %e, "could not start a login");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not sign in").into_response()
        }
    }
}

/// Sign this browser out: its login ends and its cookie is removed. Fine to call signed out.
#[utoipa::path(post, path = "/api/auth/logout", tag = "system",
    responses((status = 204, description = "Signed out; `Set-Cookie` removes `rr_login`")))]
async fn api_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(secret) = logins::presented(&headers)
        && let Err(e) = logins::end(&state.db, secret).await
    {
        tracing::error!(error = %e, "could not end a login");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not sign out").into_response();
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut()
        .append(header::SET_COOKIE, logins::cleared(logins::https(&headers)));
    res
}

/// The signed-in browsers (Settings → Logins). The owner's alone.
#[utoipa::path(get, path = "/api/auth/logins", tag = "system", operation_id = "list_logins",
    responses((status = 200, body = Vec<logins::Login>)))]
async fn api_logins(State(state): State<AppState>) -> Response {
    match logins::list(&state.db).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => login_store_failed(e),
    }
}

#[derive(Serialize, ToSchema)]
struct Ended {
    /// How many logins ended
    ended: u64,
}

/// Sign every browser out, this one too. The owner's alone.
#[utoipa::path(delete, path = "/api/auth/logins", tag = "system", operation_id = "end_all_logins",
    responses((status = 200, body = Ended)))]
async fn api_logins_end_all(State(state): State<AppState>) -> Response {
    match logins::end_all(&state.db).await {
        Ok(ended) => Json(Ended { ended }).into_response(),
        Err(e) => login_store_failed(e),
    }
}

/// Sign one browser out by its login's id. The owner's alone.
#[utoipa::path(delete, path = "/api/auth/logins/{id}", tag = "system", operation_id = "end_login",
    params(("id" = i64, Path, description = "The login's id")),
    responses((status = 200, body = Ended), (status = 404, body = ErrorBody)))]
async fn api_login_end(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match logins::end_id(&state.db, id).await {
        Ok(true) => Json(Ended { ended: 1 }).into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such login").into_response(),
        Err(e) => login_store_failed(e),
    }
}

fn login_store_failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "login request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "the login store failed").into_response()
}

/// Restore the MQTT subscriptions, reconnecting when that fails.
#[utoipa::path(post, path = "/api/admin/mqtt/reset", tag = "system",
    responses((status = 200, content_type = "text/plain", body = String),
        (status = 500, body = ErrorBody)))]
async fn api_mqtt_reset(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("MQTT reset requested via API");

    // Instead of forcing a disconnect which can fail, try to reestablish subscriptions
    // This is more robust and addresses the actual issue users experience
    match state.mqtt.resubscribe_tracked().await {
        Ok(()) => {
            tracing::info!("MQTT subscriptions reestablished successfully");
            (
                StatusCode::OK,
                "MQTT reset completed - subscriptions restored",
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!(
                ?e,
                "Failed to reestablish MQTT subscriptions, attempting disconnect/reconnect"
            );

            // If resubscribe fails, then try the disconnect approach as fallback
            match state.mqtt.disconnect().await {
                Ok(()) => {
                    tracing::info!(
                        "MQTT disconnect successful - automatic reconnection will begin"
                    );
                    // Give the MQTT client a moment to reconnect, then restore subscriptions
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                        if let Err(e) = state_clone.mqtt.resubscribe_tracked().await {
                            tracing::warn!(?e, "Failed to resubscribe after MQTT reset");
                        } else {
                            tracing::info!("MQTT subscriptions restored after reconnection");
                        }
                    });
                    (
                        StatusCode::OK,
                        "MQTT reset initiated - reconnection will happen automatically",
                    )
                        .into_response()
                }
                Err(disconnect_err) => {
                    tracing::error!(
                        ?e,
                        ?disconnect_err,
                        "Both resubscribe and disconnect failed"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to reset MQTT connection",
                    )
                        .into_response()
                }
            }
        }
    }
}

async fn publish_ok(state: &AppState, topic: &str, payload: impl Into<Vec<u8>>) -> Response {
    match state
        .mqtt
        .publish(topic, QoS::AtMostOnce, false, payload)
        .await
    {
        Ok(()) => {
            state.metrics.mqtt_tx_total.inc();
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::warn!(?e, topic, "MQTT publish failed");
            (StatusCode::BAD_GATEWAY, "MQTT publish failed").into_response()
        }
    }
}

/// Test helper: publish a telemetry message for a device (the body, or a sample one).
#[utoipa::path(post, path = "/api/test/emit-telemetry/{device_id}", tag = "system",
    params(("device_id" = String, Path)),
    request_body(content = Option<Object>, description = "Optional: the message to publish"),
    responses((status = 204, description = "Published"), (status = 502, body = ErrorBody)))]
async fn api_test_emit_telemetry(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    OptJson(maybe_body): OptJson<serde_json::Value>,
) -> Response {
    let topic = roastwarden_core::telemetry_topic(&device_id);
    let payload = if let Some(v) = maybe_body {
        v.to_string()
    } else {
        serde_json::json!({
            "timestamp": 0,
            "beanTemp": 100.0,
            "envTemp": 90.0,
            "rateOfRise": 10.0,
            "heater": 50.0,
            "fan": 70.0,
            "setpoint": 200.0,
            "controlMode": 1,
            "heaterEnable": 1,
            "uptime": 1,
            "Kp": 15.0,
            "Ki": 1.0,
            "Kd": 25.0,
            "freeHeap": 0,
            "rssi": -40,
            "systemStatus": 0
        })
        .to_string()
    };
    publish_ok(&state, &topic, payload).await
}

/// Test helper: publish a status message for a device (the body, or a sample one).
#[utoipa::path(post, path = "/api/test/emit-status/{device_id}", tag = "system",
    params(("device_id" = String, Path)),
    request_body(content = Option<Object>, description = "Optional: the message to publish"),
    responses((status = 204, description = "Published"), (status = 502, body = ErrorBody)))]
async fn api_test_emit_status(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    OptJson(maybe_body): OptJson<serde_json::Value>,
) -> Response {
    let topic = roastwarden_core::status_topic(&device_id);
    let payload = if let Some(v) = maybe_body {
        v.to_string()
    } else {
        serde_json::json!({
            "status": "online",
            "id": format!("{}-TEST", device_id),
            "ip": "127.0.0.1",
            "rssi": -40,
            "freeHeap": 123_456,
            "version": "test"
        })
        .to_string()
    };
    publish_ok(&state, &topic, payload).await
}

/// Roasters and Modbus devices seen on the LAN via mDNS/DNS-SD.
#[utoipa::path(get, path = "/api/devices/mdns", tag = "system",
    responses((status = 200, body = MdnsServices)))]
async fn api_mdns_devices(State(state): State<AppState>) -> Response {
    Json(MdnsServices {
        enabled: state.discovery.enabled(),
        services: state.discovery.list().await,
    })
    .into_response()
}
