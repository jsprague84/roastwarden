//! Authentication: the owner key and API tokens.
//!
//! The owner key is configured with `ROASTWARDEN_API_KEY`. When it is set, every request that
//! can change state needs a credential:
//!
//! - any non-GET request under `/api/` (control, autotune, sessions, profiles, devices,
//!   settings, test emitters)
//! - the device WebSocket under `/ws/device/`, because anything that can connect there can
//!   inject telemetry into a live roast
//!
//! Reads stay open so a wall display or a second browser can follow a roast without a key.
//! The dashboard telemetry WebSocket is read-only and open.
//!
//! A credential is sent as `Authorization: Bearer <credential>` or `X-API-Key`. Only the
//! device WebSocket also takes it as an `api_key=` or `token=` query parameter (a roaster
//! cannot always set headers); anywhere else a query-string key is ignored, so it never lands
//! in a proxy's access log.
//!
//! The credential is the owner key, which may do everything, or an API token (`rr_…`,
//! `tokens.rs`), which may do what its preset names and nothing under a physical roaster
//! unless the owner allowed it. A token that is unknown, revoked or expired is refused even
//! for a read, so a script finds out at once. A few requests are the owner key's alone
//! (`tokens::OWNER_ONLY`), reads included.
//!
//! The key is at least [`KEY_MIN`] characters. With no `ROASTWARDEN_API_KEY` the server makes
//! one at its first start and keeps it in `owner.key` beside the database ([`AuthConfig::at_start`]),
//! unless it listens on loopback only or `ROASTWARDEN_AUTH=off` says to stay open: then
//! authentication is off and a warning is logged, as anyone who can reach the server can drive
//! the heater and tokens restrict nothing.
//!
//! A client that sends [`FAIL_LIMIT`] refused credentials within [`FAIL_WINDOW`] gets 429
//! (`TOO_MANY_FAILURES`) for any credential until the window ends ([`client`] names it).

pub(crate) mod approvals;
pub(crate) mod audit;
/// The dashboard's optional parts (docs/lean-home.md), for the `roast modules` command too.
pub mod modules;
pub(crate) mod settings;
pub(crate) mod tokens;

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;

use tokens::{TokenAuth, TokenCheck};

#[derive(Clone, Default)]
pub struct AuthConfig {
    key: Option<Arc<str>>,
    /// Refused credentials per client, for the 429
    failures: Failures,
    /// Where tokens and the per-roaster switch live; without it every token is refused.
    db: Option<SqlitePool>,
    /// Holds a token's gated calls for the owner (`approvals.rs`)
    approvals: Option<crate::auth::approvals::Approvals>,
    /// `ROASTWARDEN_READS=open`: reads need no credential (a trusted LAN, or a proxy that signs
    /// people in). Otherwise every read but the app shell, health and the share page does.
    reads_open: bool,
    /// `ROASTWARDEN_DEMO=1`: a request with no credential is a demo visitor ([`Actor::Demo`]),
    /// who may do what [`tokens::demo_allows`] names, writes counted per client.
    demo: bool,
    /// A demo visitor's writes per client, for the 429
    demo_writes: Failures,
    /// `ROASTWARDEN_DEMO_STATS_URL` and `_WEBSITE`: the demo counts its visits in Umami
    /// (docs/DEPLOY.md, "A public demo"). Only ever with `demo`.
    demo_stats: Option<DemoStats>,
    /// A demo's visitors and their own simulated roasters (demo.rs)
    visitors: crate::ops::demo::Visitors,
}

/// Where a public demo counts its visits: an Umami server's origin and the website's id there.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
pub struct DemoStats {
    /// The Umami server's origin (`https://stats.example.com`): its `/script.js`
    pub url: String,
    /// The demo's website id in Umami
    pub website: String,
}

/// A demo visitor's writes per client and minute: enough for a person clicking through, not
/// for a script filling the database between resets.
pub const DEMO_WRITE_LIMIT: u32 = 120;

impl AuthConfig {
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_key(key: &str) -> Self {
        Self {
            key: Some(Arc::from(key)),
            ..Self::default()
        }
    }

    pub fn with_db(mut self, db: SqlitePool) -> Self {
        self.db = Some(db);
        self
    }

    pub fn with_approvals(mut self, approvals: crate::auth::approvals::Approvals) -> Self {
        self.approvals = Some(approvals);
        self
    }

    /// Reads need no credential (`ROASTWARDEN_READS=open`).
    pub fn with_reads_open(mut self, open: bool) -> Self {
        self.reads_open = open;
        self
    }

    /// A public demo (`ROASTWARDEN_DEMO=1`).
    pub fn with_demo(mut self, demo: bool) -> Self {
        self.demo = demo;
        self
    }

    pub fn demo(&self) -> bool {
        self.demo
    }

    /// A demo's visit counting (never without `demo`).
    pub fn with_demo_stats(mut self, stats: Option<DemoStats>) -> Self {
        self.demo_stats = stats.filter(|_| self.demo);
        self
    }

    pub fn demo_stats(&self) -> Option<&DemoStats> {
        self.demo_stats.as_ref()
    }

    /// A demo's visitors' roasters (empty on any other server).
    pub fn visitors(&self) -> &crate::ops::demo::Visitors {
        &self.visitors
    }

    pub fn enabled(&self) -> bool {
        self.key.is_some()
    }

    /// Whether a read needs no credential: authentication is off, or `ROASTWARDEN_READS=open`.
    pub fn reads_open(&self) -> bool {
        !self.enabled() || self.reads_open
    }

    /// The owner key, for a login's proof (`logins.rs`).
    pub(crate) fn owner_key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// Seconds until `client` may present a credential again, when it sent too many wrong ones.
    pub(crate) fn blocked(&self, client: &str) -> Option<u64> {
        self.failures.wait(client, std::time::Instant::now())
    }

    /// Count a refused credential from `client`.
    pub(crate) fn failed(&self, client: &str) {
        self.failures.fail(client, std::time::Instant::now());
    }

    /// Constant-time comparison against the configured key.
    pub fn key_ok(&self, token: &str) -> bool {
        match &self.key {
            None => true,
            Some(key) => {
                let a = key.as_bytes();
                let b = token.as_bytes();
                if a.len() != b.len() {
                    return false;
                }
                a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
            }
        }
    }
}

/// The shortest owner key the server starts with.
pub const KEY_MIN: usize = 16;

/// Where the owner key made at first start is kept: `owner.key` beside the database.
pub fn key_file(db_path: &str) -> std::path::PathBuf {
    std::path::Path::new(db_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("owner.key")
}

/// Where the owner key came from at startup, for its log line.
#[derive(Debug, PartialEq, Eq)]
pub enum KeySource {
    /// `ROASTWARDEN_API_KEY`
    Env,
    /// `owner.key`, made at an earlier start
    File(std::path::PathBuf),
    /// `owner.key`, made now: the key is logged this once
    Made(std::path::PathBuf, String),
    /// No key: the server listens on loopback only
    OpenLoopback,
    /// No key: `ROASTWARDEN_AUTH=off`
    OpenByChoice,
}

impl AuthConfig {
    /// The owner key for a server listening on `addr` with its database at `db_path`
    /// `ROASTWARDEN_API_KEY`, else `owner.key` beside the database, made now if there
    /// is none; open only on a loopback address or with `ROASTWARDEN_AUTH=off`. A key shorter
    /// than [`KEY_MIN`] is an error: the server does not start.
    pub fn at_start(
        addr: std::net::SocketAddr,
        db_path: &str,
    ) -> Result<(Self, KeySource), String> {
        let env_key = std::env::var("ROASTWARDEN_API_KEY").ok();
        let off = std::env::var("ROASTWARDEN_AUTH").is_ok_and(|v| v.trim() == "off");
        let reads_open = reads_open_from(std::env::var("ROASTWARDEN_READS").ok().as_deref())?;
        let demo = demo_from(std::env::var("ROASTWARDEN_DEMO").ok().as_deref())?;
        let (auth, source) = Self::decide(
            env_key.as_deref(),
            off,
            addr.ip().is_loopback(),
            &key_file(db_path),
        )?;
        if demo && !auth.enabled() {
            return Err(
                "ROASTWARDEN_DEMO needs an owner key: with authentication off every \
                        visitor would be the owner (set ROASTWARDEN_API_KEY, or let the server \
                        make owner.key)"
                    .into(),
            );
        }
        let stats = demo_stats_from(
            std::env::var("ROASTWARDEN_DEMO_STATS_URL").ok().as_deref(),
            std::env::var("ROASTWARDEN_DEMO_STATS_WEBSITE")
                .ok()
                .as_deref(),
            demo,
        )?;
        Ok((
            auth.with_reads_open(reads_open)
                .with_demo(demo)
                .with_demo_stats(stats),
            source,
        ))
    }

    fn decide(
        env_key: Option<&str>,
        off: bool,
        loopback: bool,
        file: &std::path::Path,
    ) -> Result<(Self, KeySource), String> {
        let with = |key: &str| Self {
            key: Some(Arc::from(key)),
            ..Self::default()
        };
        let long_enough = |key: &str, from: &str| {
            if key.chars().count() < KEY_MIN {
                Err(format!(
                    "{from} is {} characters; an owner key needs at least {KEY_MIN} (make one with `openssl rand -hex 24`)",
                    key.chars().count()
                ))
            } else {
                Ok(())
            }
        };
        if let Some(key) = env_key.map(str::trim).filter(|k| !k.is_empty()) {
            long_enough(key, "ROASTWARDEN_API_KEY")?;
            return Ok((with(key), KeySource::Env));
        }
        if off {
            return Ok((Self::default(), KeySource::OpenByChoice));
        }
        if file.exists() {
            let key = std::fs::read_to_string(file)
                .map_err(|e| format!("cannot read the owner key in {}: {e}", file.display()))?;
            let key = key.trim();
            long_enough(key, &format!("the owner key in {}", file.display()))?;
            return Ok((with(key), KeySource::File(file.to_path_buf())));
        }
        if loopback {
            return Ok((Self::default(), KeySource::OpenLoopback));
        }
        let key = new_owner_key();
        write_private(file, &key).map_err(|e| {
            format!(
                "cannot save a new owner key in {}: {e}. The folder must be writable by the \
                 user the server runs as (uid 999 in the image: `sudo chown 999:999 data`; \
                 docs/INSTALL.md), or set ROASTWARDEN_API_KEY",
                file.display()
            )
        })?;
        Ok((with(&key), KeySource::Made(file.to_path_buf(), key)))
    }
}

/// `ROASTWARDEN_DEMO`: `1`/`true` makes the server a public demo; unset, empty, `0` or `false`
/// does not.
fn demo_from(value: Option<&str>) -> Result<bool, String> {
    match value.map_or("", str::trim) {
        "" | "0" | "false" => Ok(false),
        "1" | "true" => Ok(true),
        other => Err(format!(
            "ROASTWARDEN_DEMO is \"{other}\"; it is 1 (a public demo) or 0 (the default)"
        )),
    }
}

/// `ROASTWARDEN_DEMO_STATS_URL` and `ROASTWARDEN_DEMO_STATS_WEBSITE`: both or neither, an
/// `https://` origin and a UUID, and only on a demo: no other install loads a tracker.
fn demo_stats_from(
    url: Option<&str>,
    website: Option<&str>,
    demo: bool,
) -> Result<Option<DemoStats>, String> {
    let url = url.map(str::trim).filter(|v| !v.is_empty());
    let website = website.map(str::trim).filter(|v| !v.is_empty());
    let (url, website) = match (url, website) {
        (None, None) => return Ok(None),
        (Some(u), Some(w)) => (u, w),
        _ => {
            return Err(
                "ROASTWARDEN_DEMO_STATS_URL and ROASTWARDEN_DEMO_STATS_WEBSITE go \
                        together: set both, or neither"
                    .into(),
            )
        }
    };
    if !demo {
        return Err("ROASTWARDEN_DEMO_STATS_URL is only for a public demo \
                    (ROASTWARDEN_DEMO=1): an install never counts its visits"
            .into());
    }
    let origin = url.trim_end_matches('/');
    let host = origin.strip_prefix("https://").unwrap_or("");
    let host_ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if !host_ok {
        return Err(format!(
            "ROASTWARDEN_DEMO_STATS_URL is \"{url}\"; it is an Umami server's https:// origin, \
             with no path (https://stats.example.com)"
        ));
    }
    if uuid::Uuid::parse_str(website).is_err() {
        return Err(format!(
            "ROASTWARDEN_DEMO_STATS_WEBSITE is \"{website}\"; it is the website's id in Umami \
             (a UUID)"
        ));
    }
    Ok(Some(DemoStats {
        url: origin.to_string(),
        website: website.to_ascii_lowercase(),
    }))
}

/// `ROASTWARDEN_READS`: `open` lets anyone who reaches the server read; unset or `login` makes
/// reads need a credential.
fn reads_open_from(value: Option<&str>) -> Result<bool, String> {
    match value.map(str::trim) {
        None | Some("" | "login") => Ok(false),
        Some("open") => Ok(true),
        Some(other) => Err(format!(
            "ROASTWARDEN_READS is \"{other}\"; it is \"open\" (anyone may read) or \"login\" (the default)"
        )),
    }
}

/// 24 random bytes as 48 hex digits.
fn new_owner_key() -> String {
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).expect("the OS random source failed");
    buf.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Write `text` to a new file only its owner may read.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut f = options.open(path)?;
    f.write_all(format!("{text}\n").as_bytes())?;
    f.sync_all()
}

/// Refused credentials a client may send within [`FAIL_WINDOW`] before it gets 429.
pub const FAIL_LIMIT: u32 = 10;
pub const FAIL_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

/// Refused credentials per client: when the client's window started and how many.
#[derive(Clone, Default)]
struct Failures(
    Arc<std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, u32)>>>,
);

impl Failures {
    /// Seconds until `client` may try again, when it is over the limit.
    fn wait(&self, client: &str, now: std::time::Instant) -> Option<u64> {
        self.wait_at(client, now, FAIL_LIMIT)
    }

    /// The same with another limit per window (a demo visitor's writes).
    fn wait_at(&self, client: &str, now: std::time::Instant, limit: u32) -> Option<u64> {
        let map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (start, n) = map.get(client)?;
        let gone = now.saturating_duration_since(*start);
        (*n >= limit && gone < FAIL_WINDOW)
            .then(|| FAIL_WINDOW.saturating_sub(gone).as_secs().max(1))
    }

    fn fail(&self, client: &str, now: std::time::Instant) {
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if map.len() >= 4096 {
            map.retain(|_, (start, _)| now.saturating_duration_since(*start) < FAIL_WINDOW);
        }
        let entry = map.entry(client.to_string()).or_insert((now, 0));
        if now.saturating_duration_since(entry.0) >= FAIL_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
    }
}

/// Who sent a request, for the failure count: the peer's address, or, when the peer is a
/// local address (a reverse proxy in front: Caddy, Traefik), the address that proxy added
/// last to `X-Forwarded-For` (else `X-Real-IP`). A client cannot forge the entry its own
/// proxy appends. One on the LAN talking to the server directly could send its own header;
/// a setting for trusted proxies is not built.
pub fn client(headers: &HeaderMap, peer: Option<std::net::IpAddr>) -> String {
    let local = |ip: std::net::IpAddr| match ip {
        std::net::IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
        std::net::IpAddr::V6(v) => {
            v.is_loopback()
                || v.is_unique_local()
                || v.is_unicast_link_local()
                || v.to_ipv4_mapped()
                    .is_some_and(|v| v.is_loopback() || v.is_private())
        }
    };
    if peer.is_none_or(local) {
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let forwarded = header("x-forwarded-for")
            .and_then(|v| v.rsplit(',').next().map(|s| s.trim().to_string()))
            .or_else(|| header("x-real-ip").map(|v| v.trim().to_string()))
            .filter(|v| !v.is_empty());
        if let Some(f) = forwarded {
            return f.chars().take(64).collect();
        }
    }
    peer.map_or_else(|| "unknown".to_string(), |p| p.to_string())
}

/// The credential a request presents, if any: the bearer header, then `X-API-Key`, then (on
/// the device WebSocket only) the query string.
pub fn presented<'a>(
    headers: &'a HeaderMap,
    path: &str,
    query: Option<&'a str>,
) -> Option<&'a str> {
    bearer_token(headers)
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
        })
        .filter(|t| !t.is_empty())
        .or_else(|| {
            if path.starts_with("/ws/device/") {
                query.and_then(query_token)
            } else {
                None
            }
        })
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        Some(token.trim())
    } else {
        None
    }
}

fn query_token(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == "api_key" || k == "token" {
            Some(v)
        } else {
            None
        }
    })
}

/// Whether a request needs a credential when authentication is enabled, whatever
/// `ROASTWARDEN_READS` says: writes, the device WebSocket and the audit log. Signing in and out
/// need none (the key is in the body; signing out ends the cookie it carries).
pub fn is_protected(method: &Method, path: &str) -> bool {
    if path.starts_with("/ws/device/") || path == "/api/audit" || is_receipt(path) {
        return true;
    }
    if path == "/api/auth/login" || path == "/api/auth/logout" {
        return false;
    }
    if path.starts_with("/api/") {
        return !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    }
    false
}

/// A receipt's file (`/api/orders/{id}/receipts/{id}`): the owner's address, phone and card
/// digits. A credential always, even with reads open (docs/green-coffee.md, "Orders").
fn is_receipt(path: &str) -> bool {
    let p: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    matches!(p.as_slice(), ["", "api", "orders", _, "receipts", _])
}

/// Whether a read needs a credential when reads are not open (`ROASTWARDEN_READS`): the API and
/// the WebSockets and metrics. The app shell, health, the API's description and the share
/// page stay open, and so does the auth status, which tells the dashboard to ask for the key.
pub fn read_needs_login(path: &str) -> bool {
    (path.starts_with("/api/")
        && !matches!(
            path,
            "/api/auth/status" | "/api/auth/login" | "/api/auth/logout"
        ))
        || path.starts_with("/ws/")
        || path == "/metrics"
}

/// A token named in an audit row although the request was refused (revoked, expired).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRef {
    pub id: i64,
    pub name: String,
}

/// Who a request came from, as far as the server can tell. The auth middleware puts it on
/// the request and on the response, where the audit log (`audit.rs`) reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// Authentication is off (`ROASTWARDEN_API_KEY` unset): anyone.
    Open,
    /// The owner's key.
    OwnerKey,
    /// A browser the owner signed in with the key (`logins.rs`); acts as the key.
    OwnerLogin,
    /// An API token.
    Token(TokenAuth),
    /// No valid credential (fine for a read; a protected request is refused). Names the
    /// token when a revoked or expired one was presented.
    Anonymous(Option<TokenRef>),
    /// A visitor to a public demo (`ROASTWARDEN_DEMO=1`) who brought no credential: may do
    /// what [`tokens::demo_allows`] names, on the virtual roaster only.
    Demo,
}

impl Actor {
    pub fn label(&self) -> &'static str {
        match self {
            Actor::Open => "open",
            Actor::OwnerKey => "owner key",
            Actor::OwnerLogin => "owner login",
            Actor::Token(_) => "token",
            Actor::Anonymous(_) => "none",
            Actor::Demo => "demo",
        }
    }

    /// The token this request carried, if the server knows it.
    /// The owner, by key or a signed-in browser, or anyone when authentication is off.
    pub fn is_owner(&self) -> bool {
        matches!(self, Actor::Open | Actor::OwnerKey | Actor::OwnerLogin)
    }

    pub fn token(&self) -> Option<(i64, &str)> {
        match self {
            Actor::Token(t) => Some((t.id, t.name.as_str())),
            Actor::Anonymous(Some(t)) => Some((t.id, t.name.as_str())),
            _ => None,
        }
    }
}

pub(crate) fn refuse(
    status: StatusCode,
    code: &'static str,
    msg: String,
    actor: Actor,
) -> Response {
    let mut res = if status == StatusCode::UNAUTHORIZED {
        (
            status,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"roastwarden\"")],
            msg,
        )
            .into_response()
    } else {
        (status, msg).into_response()
    };
    res.extensions_mut().insert(actor);
    res.extensions_mut()
        .insert(crate::http::errors::ErrorCode(code));
    res
}

/// A token's or a demo visitor's change for a roaster tokens may not control (the real-hardware
/// rule, [`tokens::hardware_refusal`]): the refusal to send, if it is one.
async fn hardware_check(
    auth: &AuthConfig,
    method: &str,
    path: &str,
    actor: &Actor,
) -> Option<Response> {
    let db = auth.db.as_ref()?;
    match tokens::hardware_refusal(db, method, path).await {
        Ok(None) => None,
        Ok(Some(device)) => Some(refuse(
            StatusCode::FORBIDDEN,
            "REAL_HARDWARE",
            format!(
                "Tokens may not change anything for {device}: only the virtual \
                 roaster, or a roaster the owner allowed under Roasters → \
                 {device} → Control (\"Tokens may control this roaster\")."
            ),
            actor.clone(),
        )),
        Err(()) => Some(refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "Could not check whether tokens may control this roaster.".into(),
            actor.clone(),
        )),
    }
}

/// The roaster a demo visitor's change is for, when it is another visitor's own (demo.rs):
/// a roaster's path, a roast's, or the simulator's `roaster=`.
async fn others_roaster(
    auth: &AuthConfig,
    path: &str,
    query: Option<&str>,
    own: Option<&str>,
) -> Option<String> {
    let device = if path.starts_with("/api/virtual-roaster") {
        query
            .unwrap_or("")
            .split('&')
            .find_map(|kv| kv.strip_prefix("roaster="))
            .map(str::to_string)
    } else {
        crate::auth::audit::device_for(auth.db.as_ref()?, path).await
    }?;
    (crate::ops::demo::is_visitor_roaster(&device) && own != Some(device.as_str()))
        .then_some(device)
}

/// Tower middleware: identify the credential, then apply [`is_protected`], the owner-only
/// requests, the token's preset and the real-hardware rule, and hold a token's gated call for
/// the owner's approval (`approvals.rs`). A call the owner approved comes back through here
/// as the token ([`crate::auth::approvals::ApprovedCall`]) and is checked again, but not held.
pub async fn require_api_key(
    State(auth): State<AuthConfig>,
    mut req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let approved = req
        .extensions()
        .get::<crate::auth::approvals::ApprovedCall>()
        .cloned();
    // A wrong owner key counts only when it is refused: an open read goes on as anonymous.
    let mut wrong_key = None;
    // A browser's login cookie (`logins.rs`), when it brought no other credential: its id and
    // what is due on it, or that the cookie is stale and should go.
    let mut login: Option<(i64, crate::http::logins::Due)> = None;
    let mut stale_cookie = false;
    // Who a demo visitor is, for the count of their writes
    let mut demo_client: Option<String> = None;
    let actor = if let Some(call) = &approved {
        Actor::Token(call.0.clone())
    } else if !auth.enabled() {
        Actor::Open
    } else {
        let credential = presented(req.headers(), &path, req.uri().query());
        let peer = req
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip());
        let client = client(req.headers(), peer);
        if auth.demo {
            demo_client = Some(client.clone());
        }
        let now = std::time::Instant::now();
        // A request from a demo visitor keeps their simulated roaster (demo.rs)
        if auth.demo {
            if let Some(own) = crate::ops::demo::own_roaster(req.headers()) {
                auth.visitors.touch(&own, now);
            }
        }
        if credential.is_some() {
            if let Some(secs) = auth.failures.wait(&client, now) {
                let mut res = refuse(
                    StatusCode::TOO_MANY_REQUESTS,
                    "TOO_MANY_FAILURES",
                    format!(
                        "{FAIL_LIMIT} wrong keys or tokens came from here within a minute: wait {secs} s."
                    ),
                    Actor::Anonymous(None),
                );
                res.headers_mut()
                    .insert(header::RETRY_AFTER, header::HeaderValue::from(secs));
                return res;
            }
        }
        let refused = |status, code, msg, actor| {
            auth.failures.fail(&client, now);
            refuse(status, code, msg, actor)
        };
        let cookie = credential
            .is_none()
            .then(|| crate::http::logins::presented(req.headers()))
            .flatten();
        match (credential, cookie, &auth.db) {
            (None, Some(secret), Some(db)) => {
                match crate::http::logins::check(db, secret, auth.owner_key(), chrono::Utc::now())
                    .await
                {
                    crate::http::logins::LoginCheck::Owner { id, due } => {
                        login = Some((id, due));
                        Actor::OwnerLogin
                    }
                    crate::http::logins::LoginCheck::Token { id, token, due } => {
                        login = Some((id, due));
                        Actor::Token(token)
                    }
                    crate::http::logins::LoginCheck::Gone => {
                        stale_cookie = true;
                        if auth.demo {
                            Actor::Demo
                        } else {
                            Actor::Anonymous(None)
                        }
                    }
                    crate::http::logins::LoginCheck::Error => {
                        return refuse(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "UNAVAILABLE",
                            "Could not check the login.".into(),
                            Actor::Anonymous(None),
                        )
                    }
                }
            }
            (None, _, _) if auth.demo => Actor::Demo,
            (None, _, _) => Actor::Anonymous(None),
            (Some(c), _, _) if tokens::is_token(c) => {
                let Some(db) = &auth.db else {
                    return refuse(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "UNAVAILABLE",
                        "tokens are not available".into(),
                        Actor::Anonymous(None),
                    );
                };
                match tokens::check(db, c, chrono::Utc::now()).await {
                    TokenCheck::Valid { token, touch } => {
                        if touch {
                            let (db, id) = (db.clone(), token.id);
                            let ip = crate::auth::audit::client_ip(req.headers());
                            tokio::spawn(async move { tokens::touch(&db, id, ip).await });
                        }
                        Actor::Token(token)
                    }
                    TokenCheck::Malformed | TokenCheck::Unknown => {
                        return refused(
                            StatusCode::UNAUTHORIZED,
                            "TOKEN_INVALID",
                            "Unknown API token.".into(),
                            Actor::Anonymous(None),
                        )
                    }
                    TokenCheck::Revoked { id, name } => {
                        return refused(
                            StatusCode::UNAUTHORIZED,
                            "TOKEN_REVOKED",
                            format!("The API token \"{name}\" was revoked."),
                            Actor::Anonymous(Some(TokenRef { id, name })),
                        )
                    }
                    TokenCheck::Expired { id, name } => {
                        return refused(
                            StatusCode::UNAUTHORIZED,
                            "TOKEN_EXPIRED",
                            format!("The API token \"{name}\" has expired."),
                            Actor::Anonymous(Some(TokenRef { id, name })),
                        )
                    }
                    TokenCheck::Error => {
                        return refuse(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "UNAVAILABLE",
                            "Could not check the API token.".into(),
                            Actor::Anonymous(None),
                        )
                    }
                }
            }
            (Some(c), _, _) if auth.key_ok(c) => Actor::OwnerKey,
            (Some(_), _, _) => {
                wrong_key = Some((client.clone(), now));
                Actor::Anonymous(None)
            }
        }
    };

    // A write carried by the cookie must come from the dashboard's own page (CSRF).
    if login.is_some()
        && !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && !crate::http::logins::same_origin(req.headers())
    {
        return refuse(
            StatusCode::FORBIDDEN,
            "CROSS_SITE",
            "A signed-in browser's change must come from the dashboard's own page.".into(),
            actor,
        );
    }

    match &actor {
        Actor::Open | Actor::OwnerKey | Actor::OwnerLogin => {}
        Actor::Anonymous(_) => {
            let needs = is_protected(req.method(), &path)
                || tokens::is_owner_only(&method, &path)
                || (!auth.reads_open() && read_needs_login(&path));
            if needs {
                if let Some((client, now)) = &wrong_key {
                    auth.failures.fail(client, *now);
                }
                let mut res = refuse(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "Sign in: the owner key or an API token is required.".into(),
                    actor,
                );
                if stale_cookie {
                    res.headers_mut().append(
                        header::SET_COOKIE,
                        crate::http::logins::cleared(crate::http::logins::https(req.headers())),
                    );
                }
                return res;
            }
        }
        Actor::Demo => {
            let needs = is_protected(req.method(), &path)
                || tokens::is_owner_only(&method, &path)
                || (!auth.reads_open() && read_needs_login(&path));
            if needs {
                if !tokens::demo_allows(&method, &path) {
                    return refuse(
                        StatusCode::FORBIDDEN,
                        "DEMO",
                        format!(
                            "The demo does not {method} {path}: it is a shared server on a \
                             simulated roaster. Install Roastwarden to do this on your own."
                        ),
                        actor.clone(),
                    );
                }
                if let Some(res) = hardware_check(&auth, &method, &path, &actor).await {
                    return res;
                }
                let write = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
                if write {
                    let own = crate::ops::demo::own_roaster(req.headers());
                    if let Some(other) =
                        others_roaster(&auth, &path, req.uri().query(), own.as_deref()).await
                    {
                        return refuse(
                            StatusCode::FORBIDDEN,
                            "DEMO",
                            format!(
                                "{other} is another visitor's simulated roaster: use your own, \
                                 or the shared virtual_roaster."
                            ),
                            actor.clone(),
                        );
                    }
                }
                if let (true, Some(client)) = (write, &demo_client) {
                    let now = std::time::Instant::now();
                    if let Some(secs) = auth.demo_writes.wait_at(client, now, DEMO_WRITE_LIMIT) {
                        let mut res = refuse(
                            StatusCode::TOO_MANY_REQUESTS,
                            "DEMO_BUSY",
                            format!(
                                "{DEMO_WRITE_LIMIT} changes a minute is the demo's limit: wait {secs} s."
                            ),
                            actor.clone(),
                        );
                        res.headers_mut()
                            .insert(header::RETRY_AFTER, header::HeaderValue::from(secs));
                        return res;
                    }
                    auth.demo_writes.fail(client, now);
                }
            }
        }
        Actor::Token(t) => {
            if tokens::is_owner_only(&method, &path) {
                return refuse(
                    StatusCode::FORBIDDEN,
                    "OWNER_ONLY",
                    format!("Only the owner key may {method} {path}; tokens may not."),
                    actor.clone(),
                );
            }
            if !t.preset.allows(&method, &path) {
                return refuse(
                    StatusCode::FORBIDDEN,
                    "TOKEN_SCOPE",
                    format!(
                        "The {} token \"{}\" may not {method} {path}.",
                        t.preset.label(),
                        t.name
                    ),
                    actor.clone(),
                );
            }
            if let Some(res) = hardware_check(&auth, &method, &path, &actor).await {
                return res;
            }
            let gate = crate::auth::approvals::gated(&method, &path)
                .filter(|(action, _)| t.require_approval || action.always_held());
            if let (None, Some(approvals), Some(gate)) = (&approved, &auth.approvals, gate) {
                let (parts, body) = req.into_parts();
                let (text, bytes) = crate::auth::approvals::read_body(body).await;
                if let Some(mut held) = approvals
                    .hold(t, gate, &method, &path, text, &parts.headers)
                    .await
                {
                    held.extensions_mut().insert(actor.clone());
                    return held;
                }
                req = Request::from_parts(parts, axum::body::Body::from(bytes));
            }
        }
    }

    // A login that was used: record it, and renew it (with its cookie) once a day.
    let renewal = match (login, &auth.db) {
        (Some((id, due)), Some(db)) if due.renew || due.touch => {
            let (db, ip) = (db.clone(), crate::auth::audit::client_ip(req.headers()));
            tokio::spawn(async move { crate::http::logins::used(&db, id, ip, due.renew).await });
            due.renew
                .then(|| crate::http::logins::presented(req.headers()).map(str::to_string))
                .flatten()
        }
        _ => None,
    };
    let secure = crate::http::logins::https(req.headers());
    if let Some((id, _)) = login {
        req.extensions_mut()
            .insert(crate::http::logins::LoginId(id));
    }
    req.extensions_mut().insert(actor.clone());
    let mut res = next.run(req).await;
    if let Some(secret) = renewal {
        res.headers_mut().append(
            header::SET_COOKIE,
            crate::http::logins::cookie(&secret, secure),
        );
    } else if stale_cookie && !res.headers().contains_key(header::SET_COOKIE) {
        res.headers_mut()
            .append(header::SET_COOKIE, crate::http::logins::cleared(secure));
    }
    // Signing in names the credential it signed in with; otherwise the request's own
    if res.extensions().get::<Actor>().is_none() {
        res.extensions_mut().insert(actor);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn protected_paths() {
        assert!(is_protected(
            &Method::POST,
            "/api/roaster/x/control/setpoint"
        ));
        assert!(is_protected(&Method::DELETE, "/api/sessions/1"));
        assert!(is_protected(&Method::PUT, "/api/settings/k"));
        assert!(is_protected(&Method::GET, "/ws/device/x/telemetry"));
        assert!(!is_protected(&Method::GET, "/api/sessions"));
        assert!(!is_protected(&Method::GET, "/ws/telemetry"));
        assert!(!is_protected(&Method::GET, "/"));
        assert!(!is_protected(&Method::GET, "/healthz"));
        assert!(is_protected(&Method::GET, "/api/audit"));
        // A receipt is private even with reads open; the order itself is a read
        assert!(is_protected(&Method::GET, "/api/orders/o1/receipts/r1"));
        assert!(!is_protected(&Method::GET, "/api/orders/o1"));
        assert!(!is_protected(&Method::POST, "/api/auth/login"));
        assert!(read_needs_login("/api/sessions") && read_needs_login("/ws/v2/telemetry"));
        assert!(!read_needs_login("/api/auth/status") && !read_needs_login("/R/abc"));
        assert!(!read_needs_login("/healthz") && !read_needs_login("/_app/x.js"));
        assert_eq!(reads_open_from(None), Ok(false));
        assert_eq!(reads_open_from(Some("open")), Ok(true));
        assert!(reads_open_from(Some("yes")).is_err());
    }

    #[test]
    fn a_demo_alone_counts_its_visits() {
        let w = "506406F7-0660-4c49-a8db-e57eb385bc07";
        assert_eq!(demo_stats_from(None, None, true), Ok(None));
        assert_eq!(demo_stats_from(Some(" "), Some(""), false), Ok(None));
        assert_eq!(
            demo_stats_from(Some("https://stats.example.com/"), Some(w), true),
            Ok(Some(DemoStats {
                url: "https://stats.example.com".into(),
                website: w.to_ascii_lowercase(),
            }))
        );
        // Never on an install that is not a demo
        assert!(demo_stats_from(Some("https://stats.example.com"), Some(w), false).is_err());
        // Both or neither; an https origin; a UUID
        assert!(demo_stats_from(Some("https://stats.example.com"), None, true).is_err());
        assert!(demo_stats_from(Some("http://stats.example.com"), Some(w), true).is_err());
        assert!(demo_stats_from(Some("https://stats.example.com/x"), Some(w), true).is_err());
        assert!(demo_stats_from(Some("https://a.com; script-src *"), Some(w), true).is_err());
        assert!(demo_stats_from(Some("https://stats.example.com"), Some("abc"), true).is_err());
        let auth = AuthConfig::default().with_demo_stats(Some(DemoStats {
            url: "https://s".into(),
            website: w.into(),
        }));
        assert_eq!(auth.demo_stats(), None, "kept only on a demo");
    }

    #[test]
    fn credentials_come_from_headers_and_only_the_device_socket_takes_a_query() {
        let mut h = HeaderMap::new();
        assert_eq!(presented(&h, "/api/sessions", None), None);

        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer s3cret"),
        );
        assert_eq!(presented(&h, "/api/sessions", None), Some("s3cret"));

        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static(" s3cret "));
        assert_eq!(presented(&h, "/api/sessions", None), Some("s3cret"));

        let h = HeaderMap::new();
        assert_eq!(presented(&h, "/api/sessions", Some("api_key=s3cret")), None);
        assert_eq!(
            presented(&h, "/ws/device/x/telemetry", Some("foo=1&api_key=s3cret")),
            Some("s3cret")
        );
        assert_eq!(
            presented(&h, "/ws/device/x/telemetry", Some("token=s3cret")),
            Some("s3cret")
        );
    }

    #[test]
    fn the_key_is_compared_exactly() {
        let auth = AuthConfig::with_key("s3cret");
        assert!(auth.enabled());
        assert!(auth.key_ok("s3cret"));
        assert!(!auth.key_ok("s3cre"));
        assert!(!auth.key_ok("s3cret0"));
        assert!(!AuthConfig::default().enabled());
    }

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("roastwarden-key-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// No key on a network address makes one and keeps it; loopback or
    /// `ROASTWARDEN_AUTH=off` stay open; a short key stops the start.
    #[test]
    fn the_owner_key_at_start() {
        let dir = temp_dir();
        let file = dir.join("owner.key");
        let long = "0123456789abcdef";

        let (a, src) = AuthConfig::decide(Some(long), true, false, &file).unwrap();
        assert!(a.key_ok(long) && a.enabled());
        assert_eq!(
            src,
            KeySource::Env,
            "a key set wins over ROASTWARDEN_AUTH=off"
        );
        let err = AuthConfig::decide(Some(" short "), false, false, &file)
            .err()
            .unwrap();
        assert!(err.contains("5 characters") && err.contains("16"), "{err}");

        let (a, src) = AuthConfig::decide(None, false, true, &file).unwrap();
        assert!(!a.enabled());
        assert_eq!(src, KeySource::OpenLoopback);
        let (a, src) = AuthConfig::decide(Some(""), true, false, &file).unwrap();
        assert!(!a.enabled());
        assert_eq!(src, KeySource::OpenByChoice);
        assert!(!file.exists(), "nothing is made while open");

        let (a, src) = AuthConfig::decide(None, false, false, &file).unwrap();
        let KeySource::Made(path, key) = src else {
            panic!("expected a new key, got {src:?}")
        };
        assert_eq!(path, file);
        assert_eq!(key.len(), 48);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(a.key_ok(&key));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        // The next start reads it, on loopback too, and never logs it again.
        for loopback in [false, true] {
            let (a, src) = AuthConfig::decide(None, false, loopback, &file).unwrap();
            assert_eq!(src, KeySource::File(file.clone()));
            assert!(a.key_ok(&key));
        }
        std::fs::write(&file, "too short\n").unwrap();
        assert!(AuthConfig::decide(None, false, false, &file).is_err());

        // A directory it cannot write: the start stops and says what to do.
        let err = AuthConfig::decide(None, false, false, &dir.join("no/such/owner.key"))
            .err()
            .unwrap();
        assert!(err.contains("ROASTWARDEN_API_KEY"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failures_block_a_client_for_the_rest_of_its_window() {
        let f = Failures::default();
        let t0 = std::time::Instant::now();
        for _ in 0..FAIL_LIMIT - 1 {
            f.fail("a", t0);
        }
        assert_eq!(f.wait("a", t0), None);
        f.fail("a", t0);
        assert_eq!(f.wait("a", t0), Some(60));
        assert_eq!(f.wait("b", t0), None, "another client is not blocked");
        let later = t0 + std::time::Duration::from_secs(45);
        assert_eq!(f.wait("a", later), Some(15));
        let after = t0 + FAIL_WINDOW;
        assert_eq!(f.wait("a", after), None);
        f.fail("a", after);
        assert_eq!(f.wait("a", after), None, "a new window starts from one");
    }

    #[test]
    fn the_client_is_the_peer_or_what_its_proxy_added() {
        let ip = |s: &str| Some(s.parse::<std::net::IpAddr>().unwrap());
        let mut h = HeaderMap::new();
        assert_eq!(client(&h, ip("203.0.113.9")), "203.0.113.9");
        assert_eq!(client(&h, None), "unknown");
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 198.51.100.7"),
        );
        // Behind a local proxy: the entry it appended, not the one the client sent.
        assert_eq!(client(&h, ip("172.18.0.3")), "198.51.100.7");
        assert_eq!(client(&h, ip("127.0.0.1")), "198.51.100.7");
        assert_eq!(client(&h, ip("::ffff:10.0.0.2")), "198.51.100.7");
        // A public peer's own header is ignored.
        assert_eq!(client(&h, ip("203.0.113.9")), "203.0.113.9");
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", HeaderValue::from_static("198.51.100.8"));
        assert_eq!(client(&h, ip("192.168.1.2")), "198.51.100.8");
    }

    /// Wrong credentials that were refused count; after `FAIL_LIMIT` the client gets 429 for any
    /// credential, the right key too, while a request with none still reads.
    #[tokio::test]
    async fn too_many_wrong_keys_get_429() {
        use axum::routing::{get, post};
        use tower::ServiceExt;
        let key = "the-right-owner-key";
        let app = axum::Router::new()
            .route(
                "/api/sessions",
                get(|| async { "[]" }).post(|| async { "made" }),
            )
            .route("/api/x", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                AuthConfig::with_key(key)
                    .with_reads_open(true)
                    .with_db(crate::db::open_memory().await),
                require_api_key,
            ));
        let send = |method: &str, path: &str, cred: Option<&str>, from: &str| {
            let mut b = Request::builder()
                .method(method)
                .uri(path)
                .header("x-forwarded-for", from);
            if let Some(c) = cred {
                b = b.header("x-api-key", c);
            }
            let app = app.clone();
            let req = b.body(axum::body::Body::empty()).unwrap();
            async move { app.oneshot(req).await.unwrap() }
        };
        // A wrong key on an open read is served and not counted.
        for _ in 0..FAIL_LIMIT + 2 {
            let r = send("GET", "/api/sessions", Some("wrong"), "198.51.100.1").await;
            assert_eq!(r.status(), StatusCode::OK);
        }
        for _ in 0..FAIL_LIMIT {
            let r = send("POST", "/api/x", Some("wrong"), "198.51.100.1").await;
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        }
        let r = send("POST", "/api/x", Some(key), "198.51.100.1").await;
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(r.headers().get(header::RETRY_AFTER).is_some());
        let r = send("GET", "/api/sessions", None, "198.51.100.1").await;
        assert_eq!(
            r.status(),
            StatusCode::OK,
            "a read with no credential still works"
        );
        let r = send("POST", "/api/x", Some(key), "198.51.100.2").await;
        assert_eq!(r.status(), StatusCode::OK, "another client is not blocked");
        // Unknown tokens count too.
        for _ in 0..FAIL_LIMIT {
            let r = send("GET", "/api/sessions", Some("rr_unknown"), "198.51.100.3").await;
            assert_ne!(r.status(), StatusCode::OK);
        }
        let r = send("GET", "/api/sessions", Some(key), "198.51.100.3").await;
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
