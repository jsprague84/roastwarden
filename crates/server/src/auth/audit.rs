//! Audit log: one row per state-changing API request (any non-GET under `/api/`), refused
//! ones included, with who sent it, what it asked for and how it ended.
//!
//! "Who" is the credential the auth middleware saw ([`Actor`]), plus what the edge proxy and
//! the browser say: the proxy's signed-in user (`Remote-User`), the client's User-Agent and
//! address. "What" is the method, the path, the roaster it concerned (from the path, or the
//! session's roaster for `/api/sessions/{id}/…`) and the JSON body with secrets masked.
//! Reads are not logged. Rows older than a year are pruned.

use axum::body::{Body, Bytes};
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use std::fmt::Write as _;

use crate::auth::Actor;

/// Bodies are kept up to this many bytes (JSON, after masking).
const BODY_MAX: usize = 4096;
/// At most this much of a body is read for the log, before auth has looked at the request;
/// a larger body goes on to the handler untouched and the log notes its type only.
const READ_MAX: usize = 1024 * 1024;
/// Rows are kept this long.
const KEEP_DAYS: i64 = 365;

/// Whether a request is audited.
pub fn is_audited(method: &Method, path: &str) -> bool {
    path.starts_with("/api/") && !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub token_id: Option<i64>,
    pub token_name: Option<String>,
    pub remote_user: Option<String>,
    pub client: Option<String>,
    pub ip: Option<String>,
    pub method: String,
    pub path: String,
    pub device_id: Option<String>,
    pub status: i64,
    pub body: Option<String>,
    /// What the request was about, from its path ([`KINDS`]): control, roast, profile,
    /// beans, roaster, settings, access, backup or other.
    pub kind: String,
}

/// What kind of change a request was, by its path: the first rule whose glob matches wins,
/// anything else is `other`. One table for the column the API returns and for the filter.
pub const KINDS: &[(&str, &str)] = &[
    ("/api/roaster/*/control/*", "control"),
    // A write to the Modbus slave (modbus.rs, docs/artisan-screen.md)
    ("/modbus/*", "control"),
    ("/api/roaster/*/autotune/*", "control"),
    ("/api/roaster/*/tune/*", "control"),
    ("/api/virtual-roaster*", "control"),
    // A demo visitor's own simulated roaster (demo.rs)
    ("/api/demo/*", "control"),
    ("/api/roaster/*/token-access", "access"),
    ("/api/roaster/*/setup", "access"),
    ("/api/tokens*", "access"),
    ("/api/auth/*", "access"),
    ("/api/approvals*", "access"),
    ("/api/roaster/*", "roaster"),
    ("/api/devices*", "roaster"),
    ("/api/firmware*", "roaster"),
    ("/api/mqtt/*", "roaster"),
    ("/api/device-profiles*", "roaster"),
    ("/api/artisan/*", "roaster"),
    ("/api/admin/mqtt*", "roaster"),
    // Share links make a roast's page public (docs/labels.md, "The share page")
    ("/api/sessions/*/share", "labels"),
    ("/api/share*", "labels"),
    ("/api/sessions*", "roast"),
    ("/api/profiles*", "profile"),
    ("/api/profile-shares*", "profile"),
    ("/api/beans*", "beans"),
    ("/api/purchases*", "beans"),
    ("/api/wanted*", "beans"),
    ("/api/orders*", "beans"),
    ("/api/plan*", "beans"),
    ("/api/roasted*", "beans"),
    ("/api/labels*", "labels"),
    ("/api/label-*", "labels"),
    ("/api/settings*", "settings"),
    ("/api/notifications*", "settings"),
    ("/api/push*", "settings"),
    ("/api/backups*", "backup"),
];

/// Every kind, in the order the dashboard shows them.
pub const KIND_NAMES: &[&str] = &[
    "control", "roast", "profile", "beans", "labels", "roaster", "settings", "access", "backup",
    "other",
];

/// [`KINDS`] as an SQL expression over `path` (GLOB: case-sensitive, `*` for anything).
fn kind_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| {
        let mut s = String::from("CASE");
        for (glob, kind) in KINDS {
            let _ = write!(s, " WHEN path GLOB '{glob}' THEN '{kind}'");
        }
        s.push_str(" ELSE 'other' END");
        s
    })
}

/// `text` as a `LIKE … ESCAPE '\'` pattern that matches it anywhere.
fn like_pattern(text: &str) -> String {
    let mut p = String::from("%");
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            p.push('\\');
        }
        p.push(c);
    }
    p.push('%');
    p
}

/// Middleware: outside the auth layer, so it sees refusals too.
pub async fn record(State(db): State<SqlitePool>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    if !is_audited(&method, &path) {
        return next.run(req).await;
    }
    let headers = req.headers().clone();
    let (parts, body) = req.into_parts();
    let (body_text, body) = match peek_body(body).await {
        Ok((Some(bytes), body)) => (summarise_body(&path, &headers, &bytes), body),
        Ok((None, body)) => {
            let kind =
                header_str(&headers, header::CONTENT_TYPE.as_str()).unwrap_or("unknown type");
            (
                Some(format!("(more than {} KB, {kind})", READ_MAX >> 10)),
                body,
            )
        }
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "could not read the request body").into_response();
        }
    };
    let res = next.run(Request::from_parts(parts, body)).await;
    let actor = res.extensions().get::<Actor>();
    let token = actor.and_then(Actor::token);
    let (token_id, token_name) = (token.map(|t| t.0), token.map(|t| t.1.to_string()));
    let actor = actor.map_or("none", Actor::label);
    let device_id = match device_for(&db, &path).await {
        Some(d) => Some(d),
        // e.g. creating a session names its roaster in the body
        None => body_text
            .as_deref()
            .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok())
            .and_then(|v| v.get("device_id")?.as_str().map(str::to_string)),
    };
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let inserted = sqlx::query(
        "INSERT INTO audit_log
           (at, actor, token_id, token_name, remote_user, client, ip, method, path, device_id, status, body)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&at)
    .bind(actor)
    .bind(token_id)
    .bind(&token_name)
    .bind(remote_user(&headers))
    .bind(client(&headers))
    .bind(client_ip(&headers))
    .bind(method.as_str())
    .bind(&path)
    .bind(&device_id)
    .bind(res.status().as_u16() as i64)
    .bind(&body_text)
    .execute(&db)
    .await;
    match inserted {
        Ok(r) if r.last_insert_rowid() % 500 == 0 => prune(&db).await,
        Ok(_) => {}
        Err(e) => tracing::error!(error = %e, %path, "could not write the audit log"),
    }
    res
}

/// One row for a change that came in outside HTTP: a write to the Modbus slave (modbus.rs)
/// is `MODBUS` on `/modbus/holding/<register>`, by the actor `modbus`, from the client's
/// address.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_other(
    db: &SqlitePool,
    actor: &str,
    ip: Option<String>,
    method: &str,
    path: &str,
    device_id: Option<&str>,
    status: u16,
    body: Option<String>,
) {
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let inserted = sqlx::query(
        "INSERT INTO audit_log (at, actor, ip, method, path, device_id, status, body)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&at)
    .bind(actor)
    .bind(ip)
    .bind(method)
    .bind(path)
    .bind(device_id)
    .bind(status as i64)
    .bind(body.map(|b| truncate(&b, BODY_MAX)))
    .execute(db)
    .await;
    if let Err(e) = inserted {
        tracing::error!(error = %e, %path, "could not write the audit log");
    }
}

async fn prune(db: &SqlitePool) {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(KEEP_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Err(e) = sqlx::query("DELETE FROM audit_log WHERE at < ?")
        .bind(cutoff)
        .execute(db)
        .await
    {
        tracing::warn!(error = %e, "could not prune the audit log");
    }
}

/// The roaster a request is about: `/api/roaster/{device_id}/…`, or the roaster of the
/// session in `/api/sessions/{id}/…`.
pub(crate) async fn device_for(db: &SqlitePool, path: &str) -> Option<String> {
    let mut seg = path.trim_start_matches('/').split('/');
    match (seg.next(), seg.next(), seg.next()) {
        (Some("api"), Some("roaster"), Some(dev)) if !dev.is_empty() => Some(dev.to_string()),
        (Some("api"), Some("sessions"), Some(id)) if !id.is_empty() => {
            sqlx::query_scalar("SELECT device_id FROM roast_sessions WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten()
        }
        _ => None,
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn remote_user(headers: &HeaderMap) -> Option<String> {
    [
        "remote-user",
        "x-forwarded-user",
        "remote-email",
        "x-forwarded-email",
    ]
    .iter()
    .find_map(|h| header_str(headers, h))
    .map(|v| truncate(v, 120))
}

fn client(headers: &HeaderMap) -> Option<String> {
    header_str(headers, header::USER_AGENT.as_str()).map(|v| truncate(v, 200))
}

pub(crate) fn client_ip(headers: &HeaderMap) -> Option<String> {
    header_str(headers, "x-forwarded-for")
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .or_else(|| header_str(headers, "x-real-ip"))
        .map(|v| truncate(v, 64))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// The body, read up to [`READ_MAX`]: `Some(bytes)` and the same body to pass on when it ends
/// within that, else `None` and a body that streams what was read and then the rest.
async fn peek_body(body: Body) -> Result<(Option<Bytes>, Body), axum::Error> {
    let mut data = body.into_data_stream();
    let (mut head, mut len) = (Vec::<Bytes>::new(), 0);
    while len <= READ_MAX {
        match data.next().await {
            Some(chunk) => {
                let chunk = chunk?;
                len += chunk.len();
                head.push(chunk);
            }
            None => {
                let bytes = Bytes::from(head.concat());
                return Ok((Some(bytes.clone()), Body::from(bytes)));
            }
        }
    }
    let rest = stream::iter(head.into_iter().map(Ok::<_, axum::Error>)).chain(data);
    Ok((None, Body::from_stream(rest)))
}

/// The body as kept in the log: JSON with secret-looking fields masked, or a note of its
/// size and type. A secret setting's value (`PUT /api/settings/{key}`) is masked as well.
fn summarise_body(path: &str, headers: &HeaderMap, bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(mut v) => {
            mask_secrets(&mut v);
            let setting = path.strip_prefix("/api/settings/");
            if setting.is_some_and(crate::auth::settings::is_secret) {
                if let Some(val) = v.get_mut("value").filter(|val| !val.is_null()) {
                    *val = serde_json::Value::String("***".into());
                }
            }
            Some(truncate(&v.to_string(), BODY_MAX))
        }
        Err(_) => {
            let kind = header_str(headers, header::CONTENT_TYPE.as_str()).unwrap_or("unknown type");
            Some(format!("({} bytes, {kind})", bytes.len()))
        }
    }
}

pub(crate) fn is_secret_key(k: &str) -> bool {
    if crate::auth::settings::is_secret(k) {
        return true;
    }
    let k = k.to_ascii_lowercase();
    // "token" only as the name's end (`token`, `webhook_token`, `accessToken`), so a field
    // like `tokens_may_control` stays readable.
    k.ends_with("token")
        || [
            "password",
            "secret",
            "api_key",
            "apikey",
            "authorization",
            "credential",
        ]
        .iter()
        .any(|s| k.contains(s))
}

fn mask_secrets(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if is_secret_key(k) && !val.is_null() {
                    *val = serde_json::Value::String("***".into());
                } else {
                    mask_secrets(val);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(mask_secrets),
        _ => {}
    }
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    /// 1 to 1000, default 100
    pub limit: Option<i64>,
    /// Rows with a smaller id than this (paging back).
    pub before: Option<i64>,
    /// Only commands for this roaster
    pub device_id: Option<String>,
    /// Only these kinds, comma-separated: control, roast, profile, beans, roaster, settings,
    /// access, backup, other
    pub kind: Option<String>,
    /// Text anywhere in the path, body, token name, signed-in user, roaster, address or
    /// client (case-insensitive for ASCII)
    pub q: Option<String>,
    /// Only requests sent with the token of this name
    pub token: Option<String>,
    /// Only refused or failed requests (status 400 and up)
    pub failed: Option<bool>,
}

pub fn routes() -> utoipa_axum::router::OpenApiRouter<crate::app::state::AppState> {
    utoipa_axum::router::OpenApiRouter::new().routes(utoipa_axum::routes!(list))
}

/// Every control command and setting change, with the credential that sent it, newest first
/// (SECURITY.md, "Audit log").
#[utoipa::path(get, path = "/api/audit", tag = "system", operation_id = "list_audit",
    params(AuditQuery), responses((status = 200, body = Vec<AuditEntry>)))]
pub async fn list(
    State(state): State<crate::app::state::AppState>,
    Query(q): Query<AuditQuery>,
) -> Response {
    match query(&state.db, &q).await {
        Ok(rows) => Json(rows).into_response(),
        Err(QueryError::BadKind(k)) => (
            StatusCode::BAD_REQUEST,
            format!(
                "unknown audit kind '{k}' (one of: {})",
                KIND_NAMES.join(", ")
            ),
        )
            .into_response(),
        Err(QueryError::Db(e)) => {
            tracing::error!(error = %e, "could not read the audit log");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the audit log",
            )
                .into_response()
        }
    }
}

enum QueryError {
    BadKind(String),
    Db(sqlx::Error),
}

async fn query(db: &SqlitePool, q: &AuditQuery) -> Result<Vec<AuditEntry>, QueryError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    // Checked against the list so a typo is an error, not an empty page
    let kinds = match q.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        None => None,
        Some(list) => {
            let mut kinds = Vec::new();
            for k in list.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                let k = k.to_ascii_lowercase();
                if !KIND_NAMES.contains(&k.as_str()) {
                    return Err(QueryError::BadKind(k));
                }
                kinds.push(k);
            }
            Some(format!(",{},", kinds.join(",")))
        }
    };
    let text =
        q.q.as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(like_pattern);
    let token = q.token.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let sql = format!(
        "SELECT * FROM (
           SELECT id, at, actor, token_id, token_name, remote_user, client, ip, method, path,
                  device_id, status, body, {kind} AS kind
           FROM audit_log)
         WHERE (?1 IS NULL OR id < ?1)
           AND (?2 IS NULL OR device_id = ?2)
           AND (?3 IS NULL OR instr(?3, ',' || kind || ',') > 0)
           AND (?4 IS NULL OR path LIKE ?4 ESCAPE '\\' OR body LIKE ?4 ESCAPE '\\'
                OR token_name LIKE ?4 ESCAPE '\\' OR remote_user LIKE ?4 ESCAPE '\\'
                OR device_id LIKE ?4 ESCAPE '\\' OR ip LIKE ?4 ESCAPE '\\'
                OR client LIKE ?4 ESCAPE '\\')
           AND (?5 IS NULL OR token_name = ?5)
           AND (?6 = 0 OR status >= 400)
         ORDER BY id DESC LIMIT ?7",
        kind = kind_sql()
    );
    sqlx::query_as::<_, AuditEntry>(AssertSqlSafe(sql.as_str()))
        .bind(q.before)
        .bind(&q.device_id)
        .bind(kinds)
        .bind(text)
        .bind(token)
        .bind(q.failed.unwrap_or(false))
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(QueryError::Db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{require_api_key, AuthConfig};
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    async fn test_db() -> (SqlitePool, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("roastwarden-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let db = crate::db::open(path.to_str().unwrap()).await.unwrap();
        (db, dir)
    }

    fn app(db: SqlitePool, auth: AuthConfig) -> Router {
        Router::new()
            .route(
                "/api/roaster/{d}/control/setpoint",
                post(|| async { "done" }),
            )
            .route("/api/sessions", get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(auth, require_api_key))
            .layer(axum::middleware::from_fn_with_state(db, record))
    }

    async fn rows(db: &SqlitePool) -> Vec<AuditEntry> {
        let mut r = filtered(db, AuditQuery::default()).await;
        r.reverse();
        r
    }

    async fn filtered(db: &SqlitePool, q: AuditQuery) -> Vec<AuditEntry> {
        let q = AuditQuery {
            limit: q.limit.or(Some(1000)),
            ..q
        };
        match query(db, &q).await {
            Ok(r) => r,
            Err(QueryError::BadKind(k)) => panic!("bad kind {k}"),
            Err(QueryError::Db(e)) => panic!("{e}"),
        }
    }

    fn post_req(path: &str, key: Option<&str>, body: &str) -> Request {
        let mut b = Request::post(path)
            .header("content-type", "application/json")
            .header("user-agent", "test-agent")
            .header("remote-user", "owner@example")
            .header("x-forwarded-for", "10.0.0.7, 172.18.0.2");
        if let Some(k) = key {
            b = b.header("authorization", format!("Bearer {k}"));
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn writes_are_logged_with_who_and_what_reads_are_not() {
        let (db, dir) = test_db().await;
        let app = app(db.clone(), AuthConfig::with_key("k").with_reads_open(true));

        let ok = app
            .clone()
            .oneshot(post_req(
                "/api/roaster/virtual_roaster/control/setpoint",
                Some("k"),
                r#"{"value":200,"webhook_token":"abc"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        let refused = app
            .clone()
            .oneshot(post_req(
                "/api/roaster/esp32_roaster_01/control/setpoint",
                None,
                r#"{"value":250}"#,
            ))
            .await
            .unwrap();
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        let read = app
            .oneshot(Request::get("/api/sessions").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);

        let r = rows(&db).await;
        assert_eq!(r.len(), 2, "two writes logged, the read not");
        assert_eq!(r[0].actor, "owner key");
        assert_eq!(r[0].status, 200);
        assert_eq!(r[0].device_id.as_deref(), Some("virtual_roaster"));
        assert_eq!(r[0].remote_user.as_deref(), Some("owner@example"));
        assert_eq!(r[0].client.as_deref(), Some("test-agent"));
        assert_eq!(r[0].ip.as_deref(), Some("10.0.0.7"));
        let body = r[0].body.as_deref().unwrap();
        assert!(
            body.contains(r#""value":200"#) && body.contains(r#""webhook_token":"***""#),
            "{body}"
        );
        assert_eq!(r[1].actor, "none");
        assert_eq!(r[1].status, 401);
        assert_eq!(r[1].device_id.as_deref(), Some("esp32_roaster_01"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn tokens_get_their_preset_and_the_real_hardware_rule() {
        use crate::auth::tokens::{self, Preset};
        let (db, dir) = test_db().await;
        let auth = AuthConfig::with_key("k").with_db(db.clone());
        let app = Router::new()
            .route(
                "/api/roaster/{d}/control/setpoint",
                post(|| async { "done" }),
            )
            .route("/api/roaster/{d}/control/pid", post(|| async { "done" }))
            .route("/api/sessions/{id}/start", post(|| async { "done" }))
            .route("/api/sessions/{id}/events", post(|| async { "done" }))
            .route(
                "/api/sessions/{id}/events/{eid}",
                axum::routing::put(|| async { "done" }),
            )
            .route(
                "/api/sessions/{id}",
                axum::routing::put(|| async { "done" }),
            )
            .route("/api/sessions", get(|| async { "[]" }))
            .route("/api/tokens", get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(auth, require_api_key))
            .layer(axum::middleware::from_fn_with_state(db.clone(), record));
        let (roast, secret) = tokens::create(&db, "agent", Preset::Roast, None, false)
            .await
            .unwrap();
        let (_, read_secret) = tokens::create(&db, "wall", Preset::ReadOnly, None, false)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at)
             VALUES ('s-real', 'r', 'esp32_roaster_01', 'planned', '2026-10-24', '2026-10-24')",
        )
        .execute(&db)
        .await
        .unwrap();
        let status = |path: &str, key: &str| {
            let app = app.clone();
            let req = post_req(path, Some(key), "{}");
            async move { app.oneshot(req).await.unwrap().status() }
        };
        let virt = "/api/roaster/virtual_roaster/control/setpoint";
        let real = "/api/roaster/esp32_roaster_01/control/setpoint";

        assert_eq!(status(virt, &secret).await, StatusCode::OK);
        assert_eq!(status(virt, &read_secret).await, StatusCode::FORBIDDEN);
        assert_eq!(
            status("/api/roaster/virtual_roaster/control/pid", &secret).await,
            StatusCode::FORBIDDEN,
            "pid is admin"
        );
        assert_eq!(status(real, &secret).await, StatusCode::FORBIDDEN);
        assert_eq!(
            status("/api/sessions/s-real/start", &secret).await,
            StatusCode::FORBIDDEN,
            "a session on the real roaster"
        );
        assert_eq!(status(real, "k").await, StatusCode::OK, "the owner key may");

        // A finished roast's record (weights, landmarks) reaches no roaster: tokens may edit it
        sqlx::query(
            "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at)
             VALUES ('s-done', 'r', 'esp32_roaster_01', 'completed', '2026-10-25', '2026-10-25')",
        )
        .execute(&db)
        .await
        .unwrap();
        let put = |path: &str, key: &str| {
            let app = app.clone();
            let req = Request::put(path)
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {key}"))
                .body(Body::from("{}"))
                .unwrap();
            async move { app.oneshot(req).await.unwrap().status() }
        };
        assert_eq!(put("/api/sessions/s-done", &secret).await, StatusCode::OK);
        assert_eq!(
            put("/api/sessions/s-done/events/e1", &secret).await,
            StatusCode::OK,
            "move first crack on a finished roast"
        );
        assert_eq!(
            status("/api/sessions/s-done/events", &secret).await,
            StatusCode::OK
        );
        assert_eq!(
            status("/api/sessions/s-done/start", &secret).await,
            StatusCode::FORBIDDEN,
            "starting is not record-keeping"
        );
        assert_eq!(
            put("/api/sessions/s-real/events/e1", &secret).await,
            StatusCode::FORBIDDEN,
            "a roast not finished stays behind the switch"
        );
        assert_eq!(
            status("/api/sessions/s-real/events", &secret).await,
            StatusCode::FORBIDDEN
        );

        tokens::set_roaster_access(&db, "esp32_roaster_01", true)
            .await
            .unwrap();
        assert_eq!(status(real, &secret).await, StatusCode::OK);
        assert_eq!(
            status("/api/sessions/s-real/start", &secret).await,
            StatusCode::OK
        );
        tokens::set_roaster_access(&db, "esp32_roaster_01", false)
            .await
            .unwrap();
        assert_eq!(status(real, &secret).await, StatusCode::FORBIDDEN);

        // Owner-only, even a read; and not for anonymous readers either
        let get = |key: Option<&str>| {
            let mut b = Request::get("/api/tokens");
            if let Some(k) = key {
                b = b.header("authorization", format!("Bearer {k}"));
            }
            app.clone().oneshot(b.body(Body::empty()).unwrap())
        };
        assert_eq!(
            get(Some(&secret)).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(get(None).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(get(Some("k")).await.unwrap().status(), StatusCode::OK);

        // A revoked token is refused, reads included, and named in the log
        tokens::revoke(&db, roast.id).await.unwrap();
        assert_eq!(status(virt, &secret).await, StatusCode::UNAUTHORIZED);
        let read = app
            .clone()
            .oneshot(
                Request::get("/api/sessions")
                    .header("authorization", format!("Bearer {secret}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::UNAUTHORIZED);
        let unknown = format!("rr_{}", "x".repeat(40));
        assert_eq!(status(virt, &unknown).await, StatusCode::UNAUTHORIZED);

        let r = rows(&db).await;
        assert_eq!(r[0].actor, "token");
        assert_eq!(r[0].token_id, Some(roast.id));
        assert_eq!(r[0].token_name.as_deref(), Some("agent"));
        assert_eq!(r[0].status, 200);
        assert_eq!(r[1].actor, "token");
        assert_eq!(r[1].token_name.as_deref(), Some("wall"));
        assert_eq!(r[1].status, 403);
        let revoked = r.iter().rev().nth(1).unwrap();
        assert_eq!(revoked.actor, "none");
        assert_eq!(revoked.token_name.as_deref(), Some("agent"));
        assert_eq!(revoked.status, 401);
        let last = r.last().unwrap();
        assert_eq!((last.actor.as_str(), last.token_id), ("none", None));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn kinds_search_token_and_failed_filter_the_log() {
        let (db, dir) = test_db().await;
        // method, path, token name, status, body
        type Case<'a> = (&'a str, &'a str, Option<&'a str>, i64, Option<&'a str>);
        let cases: &[Case] = &[
            (
                "POST",
                "/api/roaster/virtual_roaster/control/setpoint",
                None,
                200,
                Some(r#"{"value":200}"#),
            ),
            (
                "PUT",
                "/api/roaster/esp32_roaster_01/token-access",
                None,
                200,
                None,
            ),
            (
                "PUT",
                "/api/roaster/virtual_roaster/fan_assist",
                None,
                200,
                None,
            ),
            ("POST", "/api/sessions/s1/start", Some("hermes"), 403, None),
            (
                "POST",
                "/api/beans/b1/stock",
                Some("hermes"),
                200,
                Some(r#"{"note":"100% Guji_lot \\ x"}"#),
            ),
            ("PUT", "/api/settings/alarm_rules", None, 200, None),
            ("POST", "/api/backups", None, 500, None),
            ("POST", "/api/test/emit-status/x", None, 200, None),
        ];
        for (method, path, token, status, body) in cases {
            sqlx::query(
                "INSERT INTO audit_log (at, actor, token_name, method, path, status, body)
                 VALUES ('2026-10-25T00:00:00Z', ?, ?, ?, ?, ?, ?)",
            )
            .bind(if token.is_some() {
                "token"
            } else {
                "owner key"
            })
            .bind(token)
            .bind(method)
            .bind(path)
            .bind(status)
            .bind(body)
            .execute(&db)
            .await
            .unwrap();
        }
        let kinds: Vec<String> = rows(&db).await.into_iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            ["control", "access", "roaster", "roast", "beans", "settings", "backup", "other"]
        );
        let paths = |r: Vec<AuditEntry>| r.into_iter().map(|e| e.path).collect::<Vec<_>>();
        let q = |f: fn(&mut AuditQuery)| {
            let mut q = AuditQuery::default();
            f(&mut q);
            q
        };

        let r = filtered(&db, q(|q| q.kind = Some("Control, access".into()))).await;
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|e| e.kind == "control" || e.kind == "access"));
        // `_` and `%` are text, not wildcards
        assert_eq!(
            paths(filtered(&db, q(|q| q.q = Some("100% guji_".into()))).await),
            ["/api/beans/b1/stock"]
        );
        assert!(filtered(&db, q(|q| q.q = Some("guji%lot".into())))
            .await
            .is_empty());
        assert_eq!(filtered(&db, q(|q| q.q = Some("\\".into()))).await.len(), 1);
        assert_eq!(
            filtered(&db, q(|q| q.q = Some("SETPOINT".into())))
                .await
                .len(),
            1
        );
        assert_eq!(
            filtered(&db, q(|q| q.token = Some("hermes".into())))
                .await
                .len(),
            2
        );
        assert_eq!(
            paths(filtered(&db, q(|q| q.failed = Some(true))).await),
            ["/api/backups", "/api/sessions/s1/start"]
        );
        assert_eq!(
            paths(
                filtered(
                    &db,
                    q(|q| {
                        q.token = Some("hermes".into());
                        q.failed = Some(true);
                        q.kind = Some("roast".into());
                    })
                )
                .await
            ),
            ["/api/sessions/s1/start"]
        );
        assert!(matches!(
            query(&db, &q(|q| q.kind = Some("control,nope".into()))).await,
            Err(QueryError::BadKind(k)) if k == "nope"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A new write route must land in a kind, or the Audit page's chips miss it.
    #[tokio::test]
    async fn every_write_route_has_a_kind() {
        let (db, dir) = test_db().await;
        let doc = serde_json::to_value(crate::openapi::document()).unwrap();
        let mut unkinded = Vec::new();
        for (path, ops) in doc["paths"].as_object().unwrap() {
            if path.starts_with("/api/test/") {
                continue; // test hooks: `other`
            }
            let writes = ["post", "put", "patch", "delete"];
            if !ops
                .as_object()
                .unwrap()
                .keys()
                .any(|m| writes.contains(&m.as_str()))
            {
                continue;
            }
            let concrete = path.replace(['{', '}'], "");
            let kind: String = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT {} FROM (SELECT ? AS path)",
                kind_sql()
            )))
            .bind(&concrete)
            .fetch_one(&db)
            .await
            .unwrap();
            if kind == "other" {
                unkinded.push(path.clone());
            }
        }
        assert!(
            unkinded.is_empty(),
            "add these to audit::KINDS: {unkinded:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn every_kind_rule_names_a_known_kind() {
        for (_, kind) in KINDS {
            assert!(KIND_NAMES.contains(kind), "{kind}");
        }
    }

    #[test]
    fn bodies_are_masked_and_capped() {
        let h = HeaderMap::new();
        assert_eq!(summarise_body("/api/x", &h, b"  "), None);
        let s = summarise_body(
            "/api/x",
            &h,
            br#"{"a":{"Password":"x","list":[{"api_key":"y"}]},"n":1,"accessToken":"z","tokens_may_control":true}"#,
        )
        .unwrap();
        assert_eq!(
            s,
            r#"{"a":{"Password":"***","list":[{"api_key":"***"}]},"accessToken":"***","n":1,"tokens_may_control":true}"#
        );
        let big = format!(r#"{{"x":"{}"}}"#, "a".repeat(10_000));
        assert!(summarise_body("/api/x", &h, big.as_bytes()).unwrap().len() <= BODY_MAX + 3);
        assert_eq!(
            summarise_body("/api/x", &h, b"\x00\x01binary").as_deref(),
            Some("(8 bytes, unknown type)")
        );
    }

    #[test]
    fn secret_settings_are_masked_one_at_a_time_and_in_a_batch() {
        let h = HeaderMap::new();
        let one = |path: &str| summarise_body(path, &h, br#"{"value":"https://hook/secret"}"#);
        assert_eq!(
            one("/api/settings/notify_webhook_url").unwrap(),
            r#"{"value":"***"}"#
        );
        assert_eq!(
            one("/api/settings/notify_gotify_token").unwrap(),
            r#"{"value":"***"}"#
        );
        // Other settings stay readable.
        assert_eq!(
            one("/api/settings/notify_gotify_url").unwrap(),
            r#"{"value":"https://hook/secret"}"#
        );
        let batch = summarise_body(
            "/api/settings",
            &h,
            br#"{"values":{"notify_webhook_url":"https://hook/secret","ror_window_seconds":"30"}}"#,
        )
        .unwrap();
        assert_eq!(
            batch,
            r#"{"values":{"notify_webhook_url":"***","ror_window_seconds":"30"}}"#
        );
    }

    #[tokio::test]
    async fn a_large_body_is_read_only_in_part_and_passed_on_whole() {
        let small = Body::from(r#"{"a":1}"#);
        let (seen, body) = peek_body(small).await.unwrap();
        assert_eq!(seen.unwrap(), Bytes::from_static(br#"{"a":1}"#));
        assert_eq!(
            axum::body::to_bytes(body, usize::MAX).await.unwrap(),
            r#"{"a":1}"#
        );
        // In chunks, past the limit: nothing kept for the log, every byte still delivered.
        let chunks: Vec<Result<Bytes, std::io::Error>> = (0..40)
            .map(|i| Ok(Bytes::from(vec![b'a' + (i % 26) as u8; 64 * 1024])))
            .collect();
        let big = Body::from_stream(stream::iter(chunks));
        let (seen, body) = peek_body(big).await.unwrap();
        assert!(seen.is_none());
        let all = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        assert_eq!(all.len(), 40 * 64 * 1024);
        assert_eq!(all[64 * 1024], b'b');
        assert_eq!(all[all.len() - 1], b'a' + 13);
    }
}
