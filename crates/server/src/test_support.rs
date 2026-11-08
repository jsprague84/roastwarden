//! The whole server (`build::start` + `build::router`) for tests: a temporary database, the MQTT
//! loopback instead of a broker, and an owner key. Requests go through every layer the real
//! server has (auth, audit, JSON errors, security headers, request ids).

#![allow(
    clippy::unwrap_used,
    reason = "the test harness: a panic fails the test that called it"
)]

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use roastwarden_mqtt::MqttService;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tower::ServiceExt;

use crate::auth::AuthConfig;
use crate::{app::build, app::state::AppState};

pub const OWNER_KEY: &str = "test-owner-key";

pub struct Harness {
    pub app: Router,
    /// For the crate's own tests (`api_shapes` reads services directly)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) state: AppState,
    dir: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A response: status, content type and the body (JSON when it parses).
/// A demo visitor's changes a minute (`auth.rs`)
pub use crate::auth::DEMO_WRITE_LIMIT;

pub struct Reply {
    pub status: StatusCode,
    pub content_type: Option<String>,
    pub body: ReplyBody,
    /// The `Set-Cookie` headers
    pub set_cookie: Vec<String>,
    /// The `Content-Security-Policy` header
    pub csp: Option<String>,
}

pub enum ReplyBody {
    Empty,
    Json(Value),
    Text,
}

impl Reply {
    /// The JSON body, or Null
    pub fn json(&self) -> Value {
        match &self.body {
            ReplyBody::Json(v) => v.clone(),
            _ => Value::Null,
        }
    }
}

impl Harness {
    pub async fn new() -> Self {
        Self::with(crate::labels::Renderer::in_process(), false).await
    }

    /// With `ROASTWARDEN_READS=open`: reads need no credential (the `roast` CLI's tests)
    pub async fn with_open_reads() -> Self {
        Self::with(crate::labels::Renderer::in_process(), true).await
    }

    /// A public demo (`ROASTWARDEN_DEMO=1`): a call with no credential is a demo visitor
    pub async fn demo() -> Self {
        Self::demo_with_stats(None).await
    }

    /// A demo that counts its visits in Umami at `stats` (origin, website id)
    pub async fn demo_with_stats(stats: Option<(&str, &str)>) -> Self {
        let mut h = Self::new().await;
        let auth = h
            .state
            .auth
            .clone()
            .with_demo(true)
            .with_demo_stats(stats.map(|(url, website)| crate::auth::DemoStats {
                url: url.into(),
                website: website.into(),
            }));
        h.state.auth = auth.clone();
        let dir = h.dir.clone();
        h.app = build::router(h.state.clone(), &dir.join("no-dashboard").to_string_lossy());
        h
    }

    /// As if the server ran the built-in broker (no listener): roasters' MQTT passwords can be
    /// added; `MQTT_DEVICES` names `env_roaster` (password `env-password-1`)
    pub async fn with_built_in_broker() -> Self {
        let mut h = Self::with_open_reads().await;
        h.state.mqtt_passwords = crate::roasters::mqtt_passwords::MqttPasswords::load(
            h.state.db.clone(),
            vec![("env_roaster".into(), "env-password-1".into())],
            true,
        )
        .await
        .unwrap();
        let dir = h.dir.clone();
        h.app = build::router(h.state.clone(), &dir.join("no-dashboard").to_string_lossy());
        h
    }

    /// Label renders in a worker process of `exe` (the server binary), killed after `limit`
    pub async fn with_label_worker(exe: std::path::PathBuf, limit: std::time::Duration) -> Self {
        Self::with(
            crate::labels::Renderer::worker(exe).with_time_limit(limit),
            false,
        )
        .await
    }

    /// With the Modbus slave on a free local port (writes on or off), serving `roaster`
    /// (none: the one heard); its address
    pub async fn with_modbus(writes: bool, roaster: Option<&str>) -> (Self, std::net::SocketAddr) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let screen =
            crate::transports::modbus::Screen::new(addr, roaster.map(str::to_string), writes);
        let h = Self::build(crate::labels::Renderer::in_process(), false, Some(screen)).await;
        tokio::spawn(crate::transports::modbus::serve(
            listener,
            h.state.clone(),
            h.state.modbus.clone().unwrap(),
        ));
        (h, addr)
    }

    async fn with(label_renderer: crate::labels::Renderer, reads_open: bool) -> Self {
        Self::build(label_renderer, reads_open, None).await
    }

    async fn build(
        label_renderer: crate::labels::Renderer,
        reads_open: bool,
        modbus: Option<crate::transports::modbus::Screen>,
    ) -> Self {
        let dir = std::env::temp_dir().join(format!("roastwarden-app-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("roastwarden.db").to_string_lossy().to_string();
        let db = crate::db::open(&db_path).await.expect("test database");
        let state = build::start(
            MqttService::loopback(),
            build::Options {
                db,
                db_path,
                auth: AuthConfig::with_key(OWNER_KEY).with_reads_open(reads_open),
                mdns: false,
                label_renderer,
                modbus,
                mqtt_passwords: None,
            },
        )
        .await;
        let app = build::router(state.clone(), &dir.join("no-dashboard").to_string_lossy());
        Self { app, state, dir }
    }

    /// A call with the owner key
    pub async fn owner(&self, method: &str, uri: &str, body: Option<Value>) -> Reply {
        call(&self.app, method, uri, Some(OWNER_KEY), body).await
    }

    /// A GET with the owner key; the body as text
    pub async fn text(&self, uri: &str) -> String {
        let request = Request::get(uri)
            .header(header::AUTHORIZATION, format!("Bearer {OWNER_KEY}"))
            .body(Body::empty())
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        assert!(
            response.status().is_success(),
            "GET {uri}: {}",
            response.status()
        );
        let bytes = to_bytes(response.into_body(), 64 << 20).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// Serve the app on a free local port, for tests that need a real HTTP client (the
    /// `roast` CLI). Returns the base URL; the server runs as long as the test's runtime.
    pub async fn serve(&self) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = self.app.clone();
        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });
        format!("http://{addr}")
    }

    /// The database file (backups go to `backups/` beside it)
    pub fn db_path(&self) -> PathBuf {
        self.dir.join("roastwarden.db")
    }

    /// A new API token with this preset (`read_only`, `roast`, `admin`, …): its secret
    pub async fn token(&self, preset: &str) -> String {
        let reply = self
            .owner(
                "POST",
                "/api/tokens",
                Some(serde_json::json!({ "name": format!("test {preset}"), "preset": preset })),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json());
        reply.json()["secret"]
            .as_str()
            .expect("the token's secret")
            .to_string()
    }

    /// A call with the owner key whose body is a file as it is (a label asset)
    pub async fn owner_raw(&self, method: &str, uri: &str, bytes: Vec<u8>) -> Reply {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::AUTHORIZATION, format!("Bearer {OWNER_KEY}"))
            .body(Body::from(bytes))
            .unwrap();
        reply_of(self.app.clone().oneshot(request).await.unwrap()).await
    }

    /// A call with no credential
    pub async fn anon(&self, method: &str, uri: &str, body: Option<Value>) -> Reply {
        call(&self.app, method, uri, None, body).await
    }

    /// A call with no credential but these cookies (`name=value; …`), from `address`
    pub async fn with_cookie(
        &self,
        cookie: &str,
        address: &str,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> Reply {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, cookie)
            .header("x-forwarded-for", address);
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let request = request
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
            .unwrap();
        reply_of(self.app.clone().oneshot(request).await.unwrap()).await
    }
}

/// One request through `app`, the credential as a Bearer.
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    if body.is_some() {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let request = request
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    reply_of(app.clone().oneshot(request).await.unwrap()).await
}

async fn reply_of(response: axum::response::Response) -> Reply {
    let status = response.status();
    let set_cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok().map(str::to_string))
        .collect();
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string());
    let bytes = to_bytes(response.into_body(), 64 << 20).await.unwrap();
    let body = if bytes.is_empty() {
        ReplyBody::Empty
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(v) => ReplyBody::Json(v),
            Err(_) => ReplyBody::Text,
        }
    };
    Reply {
        status,
        content_type,
        body,
        set_cookie,
        csp,
    }
}

/// Poll for something that happens in the background (telemetry, audit rows), up to 30 s.
pub async fn eventually<F, Fut>(mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..300 {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
