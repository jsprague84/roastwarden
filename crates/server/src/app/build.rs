//! The server's services and router, built the same way by `main` and by tests.
//!
//! `start` wires every service into an `AppState` and starts the loops that feed it (MQTT
//! consumer, alarms, roast sequence, virtual roaster, fan assist, prediction, notifications,
//! backups, watch, device pollers). `router` puts the API behind auth, audit, JSON errors, security
//! headers and request ids. Tests pass `MqttService::loopback()` and a temporary database
//! (`test_support`), so they run the real stack without a broker.

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::Response;
use axum::{routing::get, Router};
use roastwarden_mqtt::MqttService;
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;

use super::ws::{ws_debug, ws_device_telemetry, ws_telemetry};
use crate::analysis::alarms::{self, AlarmService};
use crate::analysis::telemetry::TelemetryService;
use crate::analysis::ws2;
use crate::auth::{self, AuthConfig};
use crate::replay::ReplayService;
use crate::roasters::discovery::DiscoveryService;
use crate::transports::transport::TransportRegistry;
use crate::{
    app::state::AppState, app::state::Metrics, auth::approvals, auth::audit, control::fan_assist,
    control::schedule, control::sequence, http::errors, http::request_id, http::security, notify,
    notify::push, notify::watch, ops::backups, roasters::caps, sim::virtual_roaster,
    transports::pollers,
};
use crate::{openapi, routes};

pub(crate) struct Options {
    /// The database, already opened and migrated
    pub db: SqlitePool,
    /// Its file: backups are written next to it
    pub db_path: String,
    /// The owner key; tokens are checked against `db`
    pub auth: AuthConfig,
    /// Browse for roasters on the LAN
    pub mdns: bool,
    /// Where label renders run (labels/sandbox.rs)
    pub label_renderer: crate::labels::Renderer,
    /// The Modbus slave (modbus.rs), started by the caller; none: off
    pub modbus: Option<crate::transports::modbus::Screen>,
    /// The roasters' MQTT passwords, loaded by the caller; none: `MQTT_DEVICES`' alone, no
    /// built-in broker (tests)
    pub mqtt_passwords: Option<crate::roasters::mqtt_passwords::MqttPasswords>,
}

/// Every service, with its background loops running.
pub(crate) async fn start(mqtt: MqttService, opts: Options) -> AppState {
    let Options {
        db,
        db_path,
        auth,
        mdns,
        label_renderer,
        modbus,
        mqtt_passwords,
    } = opts;
    let mqtt_passwords = match mqtt_passwords {
        Some(p) => p,
        None => crate::roasters::mqtt_passwords::MqttPasswords::load(db.clone(), Vec::new(), false)
            .await
            .expect("MQTT passwords load (migration 060)"),
    };
    let telemetry_cache = Arc::new(RwLock::new(HashMap::new()));
    let device_registry = Arc::new(RwLock::new(HashMap::new()));
    let metrics = Metrics::new();
    let backups_svc = backups::Backups::new(&db_path);
    // Each roaster's control level, the gate every command passes (docs/roaster-setup.md),
    // and its declared channels, which telemetry keeps with a roast (docs/channels.md).
    let setups = crate::roasters::setup::Setups::load(db.clone())
        .await
        .expect("roaster setups load (migration 045)");
    let telemetry_service = TelemetryService::new(
        telemetry_cache.clone(),
        db.clone(),
        metrics.telemetry_last_seen.clone(),
        setups.clone(),
    );
    // Devices with their own MQTT topics, field names and command syntax
    // (docs/ingest-mapping.md); filled by the pollers' sync below and after every change.
    let mqtt_routes = crate::transports::ingest::MqttRouter::new();
    let transports = TransportRegistry::new(
        mqtt.clone(),
        metrics.mqtt_tx_total.clone(),
        db.clone(),
        setups,
        telemetry_service.clone(),
        mqtt_routes.clone(),
    );
    telemetry_service.load_ror_settings().await;
    let caps = Arc::new(caps::CapsRegistry::new());
    let gain_scheduler = schedule::GainScheduler::new(db.clone(), transports.clone());
    tokio::spawn(gain_scheduler.clone().run(telemetry_service.clone()));
    let replay = ReplayService::new(
        transports.clone(),
        telemetry_service.clone(),
        caps.clone(),
        gain_scheduler.clone(),
        db.clone(),
    );
    replay.recover_cut_runs().await;
    let fan_assist = fan_assist::FanAssistService::new(
        db.clone(),
        transports.clone(),
        telemetry_service.clone(),
        gain_scheduler.clone(),
        replay.clone(),
    );
    fan_assist.load_all().await;
    tokio::spawn(fan_assist.clone().run());
    let action_deps = alarms::ActionDeps {
        transports: transports.clone(),
        replay: replay.clone(),
        scheduler: gain_scheduler.clone(),
        fan_assist: fan_assist.clone(),
    };
    let predict = crate::predict::PredictService::new(
        db.clone(),
        telemetry_service.clone(),
        replay.clone(),
        gain_scheduler.clone(),
    );
    tokio::spawn(predict.clone().run());
    // The RoR watch's conditions read the live prediction (docs/prediction.md).
    let alarms = AlarmService::new(db.clone(), action_deps.clone(), predict.clone());
    tokio::spawn(alarms.clone().run(telemetry_service.clone()));
    // Fired alarms and system events to phones and the channels (docs/notifications.md)
    let push = push::PushService::new(db.clone());
    let notifier = notify::Notifier::new(db.clone(), push.clone());
    tokio::spawn(notifier.clone().run(alarms.subscribe()));
    backups::start_schedule(db.clone(), backups_svc.clone(), notifier.clone());
    let sequence = sequence::SequenceService::new(
        db.clone(),
        action_deps,
        alarms.clone(),
        telemetry_service.clone(),
    );
    tokio::spawn(sequence.clone().run());
    let tune = crate::control::tune::TuneService::new(
        db.clone(),
        transports.clone(),
        telemetry_service.clone(),
        replay.clone(),
    );
    let virtual_roaster = virtual_roaster::VirtualRoaster::new(db.clone(), mqtt.clone());
    tokio::spawn(virtual_roaster.clone().run());
    watch::start(db.clone(), notifier.clone(), virtual_roaster.clone());
    let discovery = DiscoveryService::new();
    if mdns {
        tokio::spawn(discovery.clone().run());
    } else {
        info!("mDNS discovery disabled (ROASTWARDEN_MDNS=false)");
    }
    let approvals = approvals::Approvals::new(db.clone(), notifier.clone());
    let auth = auth.with_db(db.clone()).with_approvals(approvals.clone());
    if auth.enabled() {
        info!("API key authentication enabled for control and mutating endpoints");
    } else {
        tracing::warn!(
            "ROASTWARDEN_API_KEY is not set: anyone who can reach this server can control the roaster"
        );
    }

    // Background consumer for MQTT events -> caches + metrics + persistence
    tokio::spawn(crate::app::consumer::mqtt_consumer_loop(
        mqtt.clone(),
        telemetry_service.clone(),
        device_registry.clone(),
        metrics.clone(),
        db.clone(),
        caps.clone(),
        mqtt_routes.clone(),
    ));
    // One subscription covers telemetry, status and the debug stream.
    // Overlapping subscriptions (e.g. `roaster/+/telemetry` next to `roaster/#`)
    // make brokers that allow duplicate delivery (mosquitto 2.1 by default)
    // hand us every message twice, which doubled rows and RoR samples.
    // Subscribe only once the consumer above is listening: the broker sends
    // retained status messages (device capabilities) right away, and events
    // published before anyone listens are dropped. The MQTT client re-sends
    // its subscriptions on reconnect, which also re-delivers retained ones.
    if let Err(e) = mqtt.subscribe("roaster/#", rumqttc::QoS::AtMostOnce).await {
        tracing::warn!(?e, "Failed to subscribe to roaster/#");
    }
    // Device tasks for Modbus (TCP, RTU), WebSocket and serial TC4 connections, and the MQTT
    // route table; the device routes keep them in step with the database after boot.
    let pollers = pollers::Pollers::new(
        db.clone(),
        telemetry_service.clone(),
        transports.clone(),
        mqtt_routes.clone(),
        mqtt.clone(),
    );
    tokio::spawn({
        let pollers = pollers.clone();
        async move { pollers.sync_all().await }
    });
    // The simulated drum writes its roaster through the normal paths when first switched on.
    virtual_roaster
        .drum
        .attach(crate::sim::virtual_drum::Provisioning {
            setups: transports.setups().clone(),
            pollers: pollers.clone(),
        });
    tokio::spawn(virtual_roaster.drum.clone().run());

    let home_assistant = crate::notify::home_assistant::HomeAssistant::new(mqtt.clone());
    let state = AppState {
        mqtt,
        telemetry_cache,
        device_registry,
        metrics,
        db,
        telemetry_service,
        transports,
        pollers,
        mqtt_routes,
        auth,
        gain_scheduler,
        fan_assist,
        virtual_roaster,
        tune,
        predict,
        replay,
        alarms,
        discovery,
        sequence,
        backups: backups_svc,
        push,
        notifier,
        approvals,
        importer: crate::coffee::import::Importer::new(),
        labels: crate::labels::folder_for(std::path::Path::new(&db_path)),
        label_renderer,
        modbus: modbus.map(Arc::new),
        home_assistant,
        mqtt_passwords,
    };
    // Home Assistant discovery reads the services above (docs/home-assistant.md)
    tokio::spawn(state.home_assistant.clone().run(state.clone()));
    // A public demo's visitors' roasters: the idle ones go (demo.rs)
    if state.auth.demo() {
        tokio::spawn(crate::ops::demo::run(state.clone()));
    }
    state
}

/// The whole HTTP app: the API, the dashboard in `app_dir` as the SPA fallback, and the
/// layers every request goes through.
pub(crate) fn router(state: AppState, app_dir: &str) -> Router {
    let spa_fallback =
        ServeDir::new(app_dir).fallback(ServeFile::new(format!("{app_dir}/index.html")));
    let mut app = api_router().with_state(state.clone());
    // A public demo tells crawlers what to index (demo.rs); elsewhere the SPA answers
    if state.auth.demo() {
        app = app.route("/robots.txt", axum::routing::get(crate::ops::demo::robots));
    }
    let app = app
        .fallback_service(spa_fallback)
        .layer(axum::middleware::from_fn_with_state(
            state.auth.clone(),
            auth::require_api_key,
        ))
        // Outside auth, so refused requests are logged too
        .layer(axum::middleware::from_fn_with_state(
            state.db.clone(),
            audit::record,
        ))
        // Outermost of the three: refusals by auth come out as JSON too
        .layer(axum::middleware::from_fn(errors::json_errors));
    let stats = state.auth.demo_stats().map(|s| s.url.clone());
    let app = request_id::with_request_ids(security::with_security_headers(app, stats.as_deref()));
    // An approved call is made through the whole app, as the token (approvals.rs)
    state.approvals.set_router(app.clone());
    app
}

/// Every route the server answers, without state, the SPA fallback or auth. Built in a test
/// too: axum rejects a malformed path only when the router is built. The /api/ routes come
/// from `openapi::routes`; only the WebSockets, the docs and redirects are added here.
pub(crate) fn api_router() -> Router<AppState> {
    Router::new()
        // Undo cached 301 from old /app/ mount point
        .route(
            "/app",
            get(|| async { axum::response::Redirect::temporary("/") }),
        )
        .route(
            "/app/{*rest}",
            get(|| async { axum::response::Redirect::temporary("/") }),
        )
        // The OpenAPI document of every route in `openapi::routes` (all of /api/)
        .route("/api-docs/openapi.json", get(openapi::serve))
        .route("/docs", get(serve_swagger_ui_html))
        // WebSockets (docs/websocket-v2.md)
        .route("/ws/telemetry", get(ws_telemetry))
        .route("/ws/v2/telemetry", get(ws2::ws_telemetry_v2))
        .route("/ws/debug", get(ws_debug))
        // Device-to-server WebSocket: devices push telemetry, receive control commands
        .route("/ws/device/{device_id}/telemetry", get(ws_device_telemetry))
        // The share page a bag's QR code opens: public, no API (docs/labels.md, "The share
        // page"); the QR code spells it in capitals
        .route("/R/{code}", get(routes::share::page))
        .route("/r/{code}", get(routes::share::page))
        // A shared profile's page and its files (docs/profiles.md, "Sharing a profile")
        .route("/P/{code}", get(routes::profile_share::page))
        .route("/p/{code}", get(routes::profile_share::page))
        .route("/P/{code}/{file}", get(routes::profile_share::file))
        .route("/p/{code}/{file}", get(routes::profile_share::file))
        // Every /api/ route, routed and documented by the same `routes!` line
        .merge(Router::from(openapi::routes()))
        // Anything else under /api/ is a JSON 404, not the dashboard's page
        .route("/api/{*rest}", axum::routing::any(errors::api_not_found))
}

async fn serve_swagger_ui_html() -> Response {
    let body = include_str!("../static/docs.html");
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(body))
        .expect("a fixed status and header")
}

#[cfg(test)]
mod router_tests {
    /// axum checks path syntax when a route is added (0.8 panics on the old `:id` and
    /// `*rest` forms), so building the whole router catches a stray route anywhere.
    #[test]
    fn every_route_builds() {
        let _ = super::api_router();
    }
}
