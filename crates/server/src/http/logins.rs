//! Signed-in browsers (SECURITY.md, "Logins").
//!
//! A browser signs in once with the owner key or an API token (`POST /api/auth/login`) and
//! gets a random cookie, [`COOKIE`]: `HttpOnly` (no script reads it), `SameSite=Strict`, and
//! `Secure` when the request came over HTTPS. The dashboard never keeps the key itself.
//!
//! The database keeps the SHA-256 of the cookie's secret, never the secret. An owner login also
//! keeps `key_proof`, the SHA-256 of the owner key and the secret together, so a new owner key
//! signs every owner login out and nothing stored reveals the key. A token's login acts as that
//! token and ends with it (revoked, expired or removed).
//!
//! A login lasts [`LIFETIME_DAYS`] from its last renewal; a request at least
//! [`RENEW_AFTER_SECS`] after the last one renews it (and sends the cookie again).

use axum::http::{HeaderMap, HeaderValue, header};
use serde::Serialize;
use sqlx::SqlitePool;
use std::fmt::Write as _;
use utoipa::ToSchema;

use crate::auth::tokens::{self, TokenAuth, TokenCheck};

/// The cookie's name.
pub const COOKIE: &str = "rr_login";
/// How long a login lasts after its last renewal.
pub const LIFETIME_DAYS: i64 = 30;
/// A login is renewed (and its cookie sent again) at most this often.
pub const RENEW_AFTER_SECS: i64 = 24 * 3600;
/// `last_used_at` is written at most this often per login.
const TOUCH_AFTER_SECS: i64 = 60;

fn hex_sha256(text: &str) -> String {
    crate::sha256_hex(text.as_bytes())
}

fn proof(owner_key: &str, secret: &str) -> String {
    hex_sha256(&format!("{owner_key}:{secret}"))
}

fn new_secret() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("the OS random source failed");
    buf.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn text(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc))
}

/// Who a login signs in as.
pub enum Who<'a> {
    /// The owner, with the owner key as it is now
    Owner(&'a str),
    /// An API token, by its id
    Token(i64),
}

/// Start a login: its id, and the secret for the browser's cookie (given once).
pub async fn create(
    db: &SqlitePool,
    who: Who<'_>,
    user_agent: Option<&str>,
    ip: Option<String>,
) -> anyhow::Result<(i64, String)> {
    let secret = new_secret();
    let (key_proof, token_id) = match who {
        Who::Owner(key) => (Some(proof(key, &secret)), None),
        Who::Token(id) => (None, Some(id)),
    };
    let at = now();
    let expires = at + chrono::Duration::days(LIFETIME_DAYS);
    // Expired logins go when a new one starts: there is no other sweep.
    sqlx::query("DELETE FROM logins WHERE expires_at <= ?")
        .bind(text(at))
        .execute(db)
        .await?;
    let id = sqlx::query(
        "INSERT INTO logins (secret_hash, key_proof, token_id, created_at, renewed_at, expires_at,
                             last_used_at, last_used_ip, user_agent)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(hex_sha256(&secret))
    .bind(key_proof)
    .bind(token_id)
    .bind(text(at))
    .bind(text(at))
    .bind(text(expires))
    .bind(text(at))
    .bind(ip)
    .bind(user_agent.map(|u| u.chars().take(200).collect::<String>()))
    .execute(db)
    .await?
    .last_insert_rowid();
    Ok((id, secret))
}

/// On a request carried by a login cookie: which login (`/api/auth/status` says so).
#[derive(Debug, Clone, Copy)]
pub struct LoginId(pub i64);

/// What a presented login cookie is.
#[derive(Debug)]
pub enum LoginCheck {
    /// The owner, signed in
    Owner { id: i64, due: Due },
    /// A token, signed in; good as of now
    Token { id: i64, token: TokenAuth, due: Due },
    /// Unknown, expired, made with another owner key, or its token is gone: signed out
    Gone,
    /// The database could not say
    Error,
}

/// What is due on a login that was just used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Due {
    /// Move the expiry on and send the cookie again
    pub renew: bool,
    /// Record its last use
    pub touch: bool,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    key_proof: Option<String>,
    token_id: Option<i64>,
    renewed_at: String,
    expires_at: String,
    last_used_at: Option<String>,
}

/// Look a login up by its cookie's secret, for a server whose owner key is `owner_key`.
pub async fn check(
    db: &SqlitePool,
    secret: &str,
    owner_key: Option<&str>,
    at: chrono::DateTime<chrono::Utc>,
) -> LoginCheck {
    if secret.len() != 64 || !secret.bytes().all(|b| b.is_ascii_hexdigit()) {
        return LoginCheck::Gone;
    }
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, key_proof, token_id, renewed_at, expires_at, last_used_at
           FROM logins WHERE secret_hash = ?",
    )
    .bind(hex_sha256(secret))
    .fetch_optional(db)
    .await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return LoginCheck::Gone,
        Err(e) => {
            tracing::error!(error = %e, "login lookup failed");
            return LoginCheck::Error;
        }
    };
    let gone = || async {
        let _ = end_id(db, row.id).await;
        LoginCheck::Gone
    };
    if parse(&row.expires_at).is_none_or(|t| t <= at) {
        return gone().await;
    }
    let due = Due {
        renew: parse(&row.renewed_at).is_none_or(|t| (at - t).num_seconds() >= RENEW_AFTER_SECS),
        touch: row
            .last_used_at
            .as_deref()
            .and_then(parse)
            .is_none_or(|t| (at - t).num_seconds() >= TOUCH_AFTER_SECS),
    };
    match (&row.key_proof, row.token_id) {
        (Some(p), None) => match owner_key {
            Some(key) if constant_eq(p, &proof(key, secret)) => {
                LoginCheck::Owner { id: row.id, due }
            }
            _ => gone().await,
        },
        (None, Some(token_id)) => match tokens::check_id(db, token_id, at).await {
            TokenCheck::Valid { token, .. } => LoginCheck::Token {
                id: row.id,
                token,
                due,
            },
            TokenCheck::Error => LoginCheck::Error,
            _ => gone().await,
        },
        _ => gone().await,
    }
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Record a login's use; with `renew`, move its expiry on as well.
pub async fn used(db: &SqlitePool, id: i64, ip: Option<String>, renew: bool) {
    let at = now();
    let result = if renew {
        sqlx::query(
            "UPDATE logins SET renewed_at = ?, expires_at = ?, last_used_at = ?, last_used_ip = ?
              WHERE id = ?",
        )
        .bind(text(at))
        .bind(text(at + chrono::Duration::days(LIFETIME_DAYS)))
        .bind(text(at))
        .bind(ip)
        .bind(id)
        .execute(db)
        .await
    } else {
        sqlx::query("UPDATE logins SET last_used_at = ?, last_used_ip = ? WHERE id = ?")
            .bind(text(at))
            .bind(ip)
            .bind(id)
            .execute(db)
            .await
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, login_id = id, "could not record a login's use");
    }
}

/// Sign out the login whose cookie this is.
pub async fn end(db: &SqlitePool, secret: &str) -> anyhow::Result<bool> {
    let done = sqlx::query("DELETE FROM logins WHERE secret_hash = ?")
        .bind(hex_sha256(secret))
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Sign out one login by its id (Settings → Logins, `roast logins revoke`).
pub async fn end_id(db: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let done = sqlx::query("DELETE FROM logins WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Sign out every browser.
pub async fn end_all(db: &SqlitePool) -> anyhow::Result<u64> {
    Ok(sqlx::query("DELETE FROM logins")
        .execute(db)
        .await?
        .rows_affected())
}

/// A signed-in browser, as Settings → Logins and `roast logins` show it. The secret is
/// never shown again.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Login {
    pub id: i64,
    /// `owner` or `token`
    pub kind: String,
    /// The token it signed in with
    pub token_name: Option<String>,
    pub created_at: String,
    /// Moves on with use
    pub expires_at: String,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    /// The browser, as it named itself when it signed in
    pub user_agent: Option<String>,
}

/// Every login that has not expired, the most recently used first.
pub async fn list(db: &SqlitePool) -> anyhow::Result<Vec<Login>> {
    Ok(sqlx::query_as::<_, Login>(
        "SELECT l.id,
                CASE WHEN l.token_id IS NULL THEN 'owner' ELSE 'token' END AS kind,
                t.name AS token_name, l.created_at, l.expires_at, l.last_used_at,
                l.last_used_ip, l.user_agent
           FROM logins l LEFT JOIN api_tokens t ON t.id = l.token_id
          WHERE l.expires_at > ?
          ORDER BY COALESCE(l.last_used_at, l.created_at) DESC, l.id DESC",
    )
    .bind(text(now()))
    .fetch_all(db)
    .await?)
}

/// The login cookie a request carries, if any.
pub fn presented(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == COOKIE && !v.is_empty()).then_some(v)
        })
}

/// Whether the browser reached the server over HTTPS: its reverse proxy says so.
pub fn https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').next().is_some_and(|p| p.trim() == "https"))
}

/// The `Set-Cookie` that holds a login.
pub fn cookie(secret: &str, secure: bool) -> HeaderValue {
    let max_age = LIFETIME_DAYS * 24 * 3600;
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE}={secret}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict{secure}"
    ))
    .expect("a hex secret is a valid header")
}

/// The `Set-Cookie` that removes it.
pub fn cleared(secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict{secure}"
    ))
    .expect("a valid header")
}

/// Whether a state-changing request carried by the cookie came from the dashboard's own page.
/// `SameSite=Strict` already keeps the cookie off other sites' requests; this checks what the
/// browser says as well: `Sec-Fetch-Site`, else `Origin` against `Host`. A request with
/// neither is not from a browser page.
pub fn same_origin(headers: &HeaderMap) -> bool {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = get("sec-fetch-site") {
        return matches!(site, "same-origin" | "none");
    }
    let Some(origin) = get("origin") else {
        return true;
    };
    let host = get("x-forwarded-host")
        .and_then(|h| h.split(',').next())
        .or_else(|| get("host"))
        .map(str::trim);
    let origin_host = origin.split_once("://").map(|(_, rest)| rest);
    matches!((origin_host, host), (Some(o), Some(h)) if o.eq_ignore_ascii_case(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("roastwarden-logins-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::db::open(&dir.join("t.db").to_string_lossy())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn an_owner_login_lasts_until_the_key_changes() {
        let db = db().await;
        let key = "owner-key-0123456789";
        let (_, secret) = create(&db, Who::Owner(key), Some("Firefox"), None)
            .await
            .unwrap();
        let at = now();
        assert!(matches!(
            check(&db, &secret, Some(key), at).await,
            LoginCheck::Owner { .. }
        ));
        // Nothing stored is the secret or the key
        let stored: (String, String) = sqlx::query_as("SELECT secret_hash, key_proof FROM logins")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!stored.0.contains(&secret) && !stored.1.contains(key));
        // A wrong cookie, then a new owner key: signed out, and the row is gone
        assert!(matches!(
            check(&db, &"0".repeat(64), Some(key), at).await,
            LoginCheck::Gone
        ));
        assert!(matches!(
            check(&db, &secret, Some("another-owner-key-012"), at).await,
            LoginCheck::Gone
        ));
        assert!(list(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_login_expires_and_renews() {
        let db = db().await;
        let key = "owner-key-0123456789";
        let (_, secret) = create(&db, Who::Owner(key), None, None).await.unwrap();
        let at = now();
        let LoginCheck::Owner { id, due } = check(&db, &secret, Some(key), at).await else {
            panic!("signed in");
        };
        assert!(!due.renew, "a new login is not due for renewal");
        let day = chrono::Duration::seconds(RENEW_AFTER_SECS);
        let LoginCheck::Owner { due, .. } = check(&db, &secret, Some(key), at + day).await else {
            panic!("still signed in a day later");
        };
        assert!(due.renew && due.touch);
        used(&db, id, Some("10.0.0.2".into()), true).await;
        let login = &list(&db).await.unwrap()[0];
        assert_eq!(login.last_used_ip.as_deref(), Some("10.0.0.2"));
        assert!(parse(&login.expires_at).unwrap() > at + chrono::Duration::days(LIFETIME_DAYS - 1));
        // Past its expiry it is gone
        let later = now() + chrono::Duration::days(LIFETIME_DAYS + 1);
        assert!(matches!(
            check(&db, &secret, Some(key), later).await,
            LoginCheck::Gone
        ));
    }

    #[tokio::test]
    async fn a_token_login_ends_with_its_token() {
        let db = db().await;
        let (token, _) = tokens::create(&db, "kitchen", tokens::Preset::ReadOnly, None, false)
            .await
            .unwrap();
        let (_, secret) = create(&db, Who::Token(token.id), None, None).await.unwrap();
        let LoginCheck::Token { token: t, .. } = check(&db, &secret, None, now()).await else {
            panic!("signed in as the token");
        };
        assert_eq!(t.name, "kitchen");
        assert_eq!(
            list(&db).await.unwrap()[0].token_name.as_deref(),
            Some("kitchen")
        );
        tokens::revoke(&db, token.id).await.unwrap();
        assert!(matches!(
            check(&db, &secret, None, now()).await,
            LoginCheck::Gone
        ));
    }

    #[test]
    fn the_cookie_is_read_and_written() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; rr_login=abc123; x=1"),
        );
        assert_eq!(presented(&h), Some("abc123"));
        let c = cookie("abc", true);
        let c = c.to_str().unwrap();
        assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict") && c.ends_with("Secure"));
        assert!(!cookie("abc", false).to_str().unwrap().contains("Secure"));
        assert!(cleared(false).to_str().unwrap().contains("Max-Age=0"));
    }

    #[test]
    fn cookie_writes_must_come_from_the_page_itself() {
        let h = |pairs: &[(&'static str, &'static str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(*k, HeaderValue::from_static(v));
            }
            h
        };
        assert!(same_origin(&h(&[("sec-fetch-site", "same-origin")])));
        assert!(!same_origin(&h(&[("sec-fetch-site", "cross-site")])));
        assert!(!same_origin(&h(&[("sec-fetch-site", "same-site")])));
        assert!(same_origin(&h(&[
            ("origin", "https://roast.example"),
            ("host", "roast.example")
        ])));
        assert!(!same_origin(&h(&[
            ("origin", "https://evil.example"),
            ("host", "roast.example")
        ])));
        assert!(same_origin(&h(&[])));
    }

    /// The whole app: a browser signs in, reads and writes with the cookie alone, cannot be
    /// driven from another site, sees the audit log, and signs out.
    #[tokio::test]
    async fn a_browser_signs_in_reads_and_signs_out() {
        use crate::test_support::{Harness, OWNER_KEY};
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let h = Harness::new().await;
        let send = |method: &str, uri: &str, headers: &[(&str, String)], body: Option<&str>| {
            let mut b = Request::builder().method(method).uri(uri);
            for (k, v) in headers {
                b = b.header(*k, v.as_str());
            }
            if body.is_some() {
                b = b.header("content-type", "application/json");
            }
            let req = b
                .body(body.map_or_else(Body::empty, |t| Body::from(t.to_string())))
                .unwrap();
            let app = h.app.clone();
            async move { app.oneshot(req).await.unwrap() }
        };
        let json = |r: axum::response::Response| async move {
            let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default()
        };

        // Reads need a credential; the auth status, health and the app shell do not.
        assert_eq!(
            send("GET", "/api/sessions", &[], None).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send("GET", "/healthz", &[], None).await.status(),
            StatusCode::OK
        );
        let s = json(send("GET", "/api/auth/status", &[], None).await).await;
        assert_eq!(s["reads_open"], false);
        assert_eq!(s["credential"], "none");

        // A wrong key: 401, no cookie
        let r = send(
            "POST",
            "/api/auth/login",
            &[],
            Some(r#"{"credential":"nope"}"#),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        assert!(r.headers().get(header::SET_COOKIE).is_none());

        // The owner key: an HttpOnly cookie
        let r = send(
            "POST",
            "/api/auth/login",
            &[("x-forwarded-proto", "https".into())],
            Some(&format!(r#"{{"credential":"{OWNER_KEY}"}}"#)),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let set = r.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(set.contains("HttpOnly") && set.contains("Secure"), "{set}");
        let cookie = set.split(';').next().unwrap().to_string();
        let s = json(r).await;
        assert_eq!(s["credential"], "owner login");
        let id = s["login_id"].as_i64().expect("this browser's login");
        let c = || ("cookie", cookie.clone());

        // With the cookie alone: reads, the owner's own lists, and a write from the page
        assert_eq!(
            send("GET", "/api/sessions", &[c()], None).await.status(),
            StatusCode::OK
        );
        let s = json(send("GET", "/api/auth/status", &[c()], None).await).await;
        assert_eq!(
            (s["credential"].as_str(), s["login_id"].as_i64()),
            (Some("owner login"), Some(id))
        );
        assert_eq!(
            send("GET", "/api/tokens", &[c()], None).await.status(),
            StatusCode::OK
        );
        let body = r#"{"name":"Kenya","origin":"Kenya"}"#;
        let r = send(
            "POST",
            "/api/beans",
            &[c(), ("sec-fetch-site", "same-origin".into())],
            Some(body),
        )
        .await;
        assert!(r.status().is_success(), "{}", r.status());
        // The same write from another site is refused
        let r = send(
            "POST",
            "/api/beans",
            &[c(), ("sec-fetch-site", "cross-site".into())],
            Some(body),
        )
        .await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        // The audit log: the owner's, never the key in a body
        let audit = json(send("GET", "/api/audit?limit=50", &[c()], None).await).await;
        let text = audit.to_string();
        assert!(
            !text.contains(OWNER_KEY),
            "the key is masked in the audit log"
        );
        assert!(text.contains("owner login"));
        let logins = json(send("GET", "/api/auth/logins", &[c()], None).await).await;
        assert_eq!(logins.as_array().map(Vec::len), Some(1));
        assert_eq!(logins[0]["kind"], "owner");

        // Signing out ends it
        let r = send("POST", "/api/auth/logout", &[c()], None).await;
        assert_eq!(r.status(), StatusCode::NO_CONTENT);
        assert!(
            r.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        let r = send("GET", "/api/sessions", &[c()], None).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        assert!(
            r.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0"),
            "a stale cookie is removed"
        );
    }

    /// Tokens: a read-only one may sign a browser in and read, not the audit log; an admin
    /// token may read the audit log.
    #[tokio::test]
    async fn the_audit_log_is_the_owners_and_admins() {
        use crate::test_support::Harness;
        use axum::http::StatusCode;
        let h = Harness::new().await;
        let read = h.token("read_only").await;
        let admin = h.token("admin").await;
        let call = |t: String| {
            let app = h.app.clone();
            async move {
                crate::test_support::call(&app, "GET", "/api/audit", Some(&t), None)
                    .await
                    .status
            }
        };
        assert_eq!(call(read.clone()).await, StatusCode::FORBIDDEN);
        assert_eq!(call(admin).await, StatusCode::OK);
        assert_eq!(
            h.anon("GET", "/api/audit", None).await.status,
            StatusCode::UNAUTHORIZED
        );
        let r = h
            .anon(
                "POST",
                "/api/auth/login",
                Some(serde_json::json!({ "credential": read })),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.json()["credential"], "token");
        assert_eq!(r.json()["token"]["preset"], "read_only");
    }
}
