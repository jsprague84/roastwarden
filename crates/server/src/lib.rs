//! roastwarden-server as a library: the server (`run`, used by the `roastwarden-server` binary)
//! and what the `roast` CLI and integration tests share with it: backups (the CLI's offline
//! `backup` and `restore`), the OpenAPI document, and, with the `test-support` feature, the
//! whole server in a test.

use std::net::SocketAddr;
use std::path::PathBuf;

use roastwarden_mqtt::{MqttConfig, MqttService};
use tracing::info;

mod analysis;
#[cfg(test)]
mod api_shapes;
mod app;
mod auth;
mod coffee;
#[cfg(test)]
mod connect_examples;
mod control;
mod db;
mod http;
mod labels;
mod notify;
pub mod openapi;
mod ops;
mod predict;
mod profiles;
mod replay;
mod reports;
mod roasters;
mod roasts;
mod routes;
mod sim;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
mod transports;

// The paths the `roast` CLI and the server binary use.
pub use auth::modules;
pub use ops::{backups, legacy, logbuf};

/// The token presets, for the `roast` CLI's agent-surface check (`bin/roast/surface.rs`).
pub mod presets {
    pub use crate::auth::tokens::{is_owner_only, Preset};
}

/// The label render worker (`roastwarden-server label-render`, labels/sandbox.rs): reads one
/// render from stdin, answers on stdout. Returns the exit code.
pub fn label_worker() -> i32 {
    labels::sandbox::worker_main()
}

/// The argument that starts the server binary as the label render worker
pub const LABEL_WORKER_ARG: &str = labels::sandbox::WORKER_ARG;

use auth::AuthConfig;

/// Retained server presence topic (see hardware guide, "Controller heartbeat").
pub(crate) const SERVER_STATUS_TOPIC: &str = "roastwarden/server/status";

/// The presence topic before the rename to Roastwarden (0.25.0). 0.25.x announced it too, for
/// firmware older than 3.7.0; from 0.26.0 the server only clears its retained message at start.
const OLD_SERVER_STATUS_TOPIC: &str = "rustroast/server/status";

/// The server as `roastwarden-server` runs it: MQTT, the database (with a staged restore put
/// in place first), every service, the HTTP app and the Modbus server, configured from the
/// environment, until SIGINT or SIGTERM.
/// ring as the process's one TLS crypto provider: reqwest's `rustls-no-provider` and the IPP
/// pinning build on it, and the MQTT client installs the same, so one rustls is in the build
/// and no aws-lc. Idempotent: a second call does nothing.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// The SHA-256 of `data` as 64 hex digits (the token store, logins, receipts, label assets).
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(data)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub async fn run(shutdown: impl std::future::Future<Output = ()> + Send + 'static) {
    routes::about::mark_started();
    install_crypto_provider();

    let db_path = std::env::var("ROASTWARDEN_DB_PATH")
        .unwrap_or_else(|_| "./data/roastwarden.db".to_string());
    // An install from before the rename keeps its database and backups (legacy.rs)
    match ops::legacy::adopt_files(
        std::path::Path::new(&db_path),
        &ops::backups::dir_for(&db_path),
    ) {
        Ok(moved) => {
            for m in moved {
                tracing::warn!(moved = %m, "renamed for Roastwarden");
            }
        }
        Err(e) => tracing::error!(error = %e, "could not rename the rustroast database or backups"),
    }
    // A restore staged from Settings → Backups goes in before the database opens.
    match ops::backups::apply_staged_restore(std::path::Path::new(&db_path)) {
        Ok(Some(previous)) => {
            tracing::warn!(previous = %previous.display(), "database restored from a backup; the old one was moved aside");
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "could not apply the staged restore; starting with the current database");
        }
    }
    let addr: SocketAddr = std::env::var("ROASTWARDEN_HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .expect("Invalid ROASTWARDEN_HTTP_ADDR");
    let auth = match AuthConfig::at_start(addr, &db_path) {
        Ok((auth, source)) => {
            match source {
                auth::KeySource::Env => info!("owner key from ROASTWARDEN_API_KEY"),
                auth::KeySource::File(path) => {
                    info!(file = %path.display(), "owner key from the file made at first start");
                }
                auth::KeySource::Made(path, key) => tracing::warn!(
                    file = %path.display(),
                    "No ROASTWARDEN_API_KEY: made an owner key and saved it in the file named here. \
                     The dashboard asks for it once; this is the only time it is logged: {key}"
                ),
                auth::KeySource::OpenLoopback => {
                    info!(%addr, "no owner key: the server listens on loopback only");
                }
                auth::KeySource::OpenByChoice => info!("no owner key: ROASTWARDEN_AUTH=off"),
            }
            if auth.enabled() {
                if auth.reads_open() {
                    info!("ROASTWARDEN_READS=open: anyone who reaches the server may read");
                } else {
                    info!("reads need a credential: sign the dashboard in with the owner key");
                }
            }
            if auth.demo() {
                tracing::warn!(
                    "ROASTWARDEN_DEMO: a public demo. Anyone may roast on the virtual roaster and \
                     record; nothing reaches another host (docs/DEPLOY.md, \"A public demo\")"
                );
            }
            auth
        }
        Err(e) => {
            tracing::error!("{e}");
            eprintln!("roastwarden-server: {e}");
            std::process::exit(2);
        }
    };
    let db = match db::open(&db_path).await {
        Ok(db) => db,
        Err(e) => {
            let dir = std::path::Path::new(&db_path)
                .parent()
                .map_or_else(|| ".".into(), |p| p.display().to_string());
            let msg = format!(
                "cannot open the database {db_path}: {e}. The folder {dir} must exist and be \
                 writable by the user the server runs as (uid 999 in the image: \
                 `sudo chown 999:999 data` beside the compose file; docs/INSTALL.md)"
            );
            tracing::error!("{msg}");
            eprintln!("roastwarden-server: {msg}");
            std::process::exit(2);
        }
    };
    // MQTT setup, after the database: the built-in broker signs roasters in with the passwords
    // kept there (mqtt_passwords.rs)
    let mut mqtt_cfg = MqttConfig::from_env();
    // The built-in broker (ROASTWARDEN_BROKER_ADDR; broker.rs): started before the server's
    // client, which signs in to it on this machine as `roastwarden`.
    let broker = match ops::broker::BrokerConfig::from_env() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "the built-in MQTT broker is set up wrong");
            std::process::exit(2);
        }
    };
    // MQTT_DEVICES: the built-in broker's (checked above), else read for the firmware download
    let env_devices = match &broker {
        Some(b) => b.devices.clone(),
        None => ops::broker::parse_devices(&std::env::var("MQTT_DEVICES").unwrap_or_default())
            .unwrap_or_default(),
    };
    let mqtt_passwords = match roasters::mqtt_passwords::MqttPasswords::load(
        db.clone(),
        env_devices,
        broker.is_some(),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "cannot read the roasters' MQTT passwords");
            std::process::exit(2);
        }
    };
    if let Some(b) = &broker {
        match ops::broker::start(b, mqtt_passwords.clone()).await {
            Ok(handle) => mqtt_passwords.attach(handle),
            Err(e) => {
                tracing::error!(error = %format!("{e:#}"), "the built-in MQTT broker did not start");
                std::process::exit(2);
            }
        }
        let local = b.local_addr();
        mqtt_cfg.host = local.ip().to_string();
        mqtt_cfg.port = local.port();
        mqtt_cfg.tls = false;
        mqtt_cfg.username = Some(ops::broker::SERVER_USER.to_string());
        mqtt_cfg.password = Some(b.server_password.clone());
        routes::about::set_broker(b.info());
    }
    // Retained presence on a topic outside the roaster/ namespace, with a last will.
    // Firmware can watch it to fail safe when the control plane (not just the broker) is gone.
    mqtt_cfg.status = Some(roastwarden_mqtt::StatusAnnouncement {
        topic: SERVER_STATUS_TOPIC.to_string(),
        online: serde_json::json!({
            "status": "online",
            "version": env!("CARGO_PKG_VERSION"),
            "client_id": mqtt_cfg.client_id,
        })
        .to_string()
        .into_bytes(),
        offline: serde_json::json!({ "status": "offline" })
            .to_string()
            .into_bytes(),
    });
    // `ROASTWARDEN_MQTT=loopback`: no broker, messages stay in the server (the virtual roaster
    // works; a real roaster cannot reach it). For CI's browser checks and the local stack.
    let mqtt = if std::env::var("ROASTWARDEN_MQTT").is_ok_and(|v| v == "loopback") {
        tracing::warn!(
            "MQTT loopback: no broker, only the virtual roaster can talk to this server"
        );
        MqttService::loopback()
    } else {
        tracing::info!(host = %mqtt_cfg.host, port = mqtt_cfg.port, tls = mqtt_cfg.tls, "Configuring MQTT client");
        let mqtt = MqttService::connect(mqtt_cfg)
            .await
            .expect("Failed to initialize MQTT");
        // 0.25.x left an "offline" there (its last will); an empty retained message removes it.
        // Queued until the connection is up.
        if let Err(e) = mqtt
            .publish(
                OLD_SERVER_STATUS_TOPIC,
                rumqttc::QoS::AtLeastOnce,
                true,
                Vec::<u8>::new(),
            )
            .await
        {
            tracing::debug!(error = %e, "could not clear the old presence topic");
        }
        mqtt
    };

    let mdns = std::env::var("ROASTWARDEN_MDNS").map_or(true, |v| v != "false" && v != "0");
    let state = app::build::start(
        mqtt,
        app::build::Options {
            db,
            db_path,
            auth,
            mdns,
            label_renderer: labels::Renderer::from_env(),
            modbus: transports::modbus::Screen::from_env(),
            mqtt_passwords: Some(mqtt_passwords),
        },
    )
    .await;

    // Static frontend (SPA fallback)
    let server_crate_dir = env!("CARGO_MANIFEST_DIR");
    let default_app_dir = PathBuf::from(server_crate_dir).join("../../apps/dashboard/build");
    let app_dir = std::env::var("ROASTWARDEN_APP_DIR")
        .unwrap_or_else(|_| default_app_dir.to_string_lossy().to_string());
    let app = app::build::router(state.clone(), &app_dir);

    info!(%addr, "Starting HTTP server");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind the HTTP address");
    // The Modbus slave, Artisan's screen (off unless ROASTWARDEN_MODBUS_ADDR is set)
    let _modbus_handle = transports::modbus::start(state.clone()).await;
    // The peer's address, for the failed-credential count (auth::client)
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
    .expect("serve HTTP");
}
