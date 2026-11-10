//! Approvals: the few calls an API token may make only after the owner says yes
//! (docs/control.md, "Approvals").
//!
//! Starting a roast on a physical roaster, switching a roaster's control on, and deleting a
//! roast, bean, profile, roaster, label design, label asset or brand are
//! the gated calls ([`GATED`]). For a token with `require_approval`, the auth layer hands such
//! a call to [`Approvals::hold`] instead of the handler, after the token's preset and the
//! real-hardware switch allowed it (an approval never bypasses either). It is stored as a
//! pending request for an hour, a link goes out through the notification channels (Web Push,
//! Gotify, ntfy, Matrix, the webhook; the caller has the link in the 202 either way), and the
//! caller may wait on `GET /api/approvals/{id}?wait=25`. Only the owner key approves or denies
//! (`tokens::OWNER_ONLY`), so an agent can never approve itself. On approval
//! [`Approvals::decide`] makes the original call through the whole router as the token
//! ([`ApprovedCall`]), so the preset, the switch, the handler's own checks and the audit row
//! naming the token apply unchanged.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::Result;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use tower::ServiceExt;
use tracing::{info, warn};

use crate::auth::tokens::{self, TokenAuth, path_matches};
use crate::notify::{Note, Notifier, Priority};

/// How long a request waits for the owner
pub const TTL_MINUTES: i64 = 60;
/// Longest one `GET /api/approvals/{id}?wait=` waits (under a proxy's 30 s timeout)
pub const MAX_WAIT_SECS: u64 = 25;
/// The largest body a held call may carry
const MAX_BODY: usize = 64 << 10;

/// A call that can need approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    RoastStart,
    RoastDelete,
    BeanDelete,
    OrderDelete,
    ProfileDelete,
    RoasterDelete,
    LabelDesignDelete,
    LabelAssetDelete,
    LabelBrandDelete,
    LabelPrint,
    LabelRunPrint,
    LabelPrinterDelete,
    /// A setup `PUT` that leaves control `manual` or `full` (docs/roaster-setup.md)
    RoasterControl,
}

/// The gated calls. Everything else a token's preset allows runs at once; live control of a
/// roast in progress stays behind the per-roaster switch alone.
const GATED: &[(&str, &str, Action)] = &[
    ("POST", "/api/sessions/*/start", Action::RoastStart),
    ("DELETE", "/api/sessions/*", Action::RoastDelete),
    ("DELETE", "/api/beans/*", Action::BeanDelete),
    ("DELETE", "/api/orders/*", Action::OrderDelete),
    ("DELETE", "/api/profiles/*", Action::ProfileDelete),
    ("DELETE", "/api/devices/*", Action::RoasterDelete),
    ("DELETE", "/api/label-designs/*", Action::LabelDesignDelete),
    ("DELETE", "/api/label-assets/*", Action::LabelAssetDelete),
    ("DELETE", "/api/label-brands/*", Action::LabelBrandDelete),
    ("POST", "/api/sessions/*/labels/print", Action::LabelPrint),
    ("POST", "/api/label-runs/*/print", Action::LabelRunPrint),
    (
        "DELETE",
        "/api/label-printers/*",
        Action::LabelPrinterDelete,
    ),
    // Only when the body leaves control on ([`raises_control`]); at off it runs at once
    ("PUT", "/api/roaster/*/setup", Action::RoasterControl),
];

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::RoastStart => "roast.start",
            Action::RoastDelete => "roast.delete",
            Action::BeanDelete => "bean.delete",
            Action::OrderDelete => "order.delete",
            Action::ProfileDelete => "profile.delete",
            Action::RoasterDelete => "roaster.delete",
            Action::LabelDesignDelete => "label_design.delete",
            Action::LabelAssetDelete => "label_asset.delete",
            Action::LabelBrandDelete => "label_brand.delete",
            Action::LabelPrint => "labels.print",
            Action::LabelRunPrint => "labels.run.print",
            Action::LabelPrinterDelete => "label_printer.delete",
            Action::RoasterControl => "roaster.control",
        }
    }

    /// Held for every token, whether or not it asks first: a print uses a sheet of labels
    /// and someone must load it; switching a roaster's control on is the owner's decision, even
    /// when a token asks for it
    pub fn always_held(self) -> bool {
        matches!(
            self,
            Action::LabelPrint | Action::LabelRunPrint | Action::RoasterControl
        )
    }
}

/// The control level a setup `PUT` leaves on (`manual` or `full`), `None` when it leaves
/// control off. A body that does not parse is not held: the handler answers it.
fn raises_control(body: Option<&str>) -> Option<String> {
    let b: Value = serde_json::from_str(body?).ok()?;
    let level = b["control"].as_str()?;
    matches!(level, "manual" | "full").then(|| level.to_string())
}

/// The gated action and its target id, if this call is one.
pub fn gated(method: &str, path: &str) -> Option<(Action, String)> {
    let (_, _, action) = GATED
        .iter()
        .find(|(m, p, _)| *m == method && path_matches(p, path))?;
    let id = path.split('/').nth(3)?.to_string();
    Some((*action, id))
}

/// "Start roast "Guji 3" on `esp32_roaster_01`", for the notification, the page and the 202.
pub fn summary(action: &str, name: Option<&str>, id: &str, device: Option<&str>) -> String {
    let what = name.map_or_else(|| id.to_string(), |n| format!("\"{n}\""));
    match action {
        "roast.start" => format!("Start roast {what} on {}", device.unwrap_or("its roaster")),
        "roast.delete" => format!("Delete roast {what}"),
        "bean.delete" => format!("Delete bean {what}"),
        "order.delete" => {
            format!("Delete order {what}, its receipts and any purchases it recorded")
        }
        "profile.delete" => format!("Delete profile {what}"),
        "roaster.delete" => format!("Delete roaster {what}"),
        "label_design.delete" => format!("Delete label design {what} and its versions"),
        "label_asset.delete" => format!("Delete label asset {what}"),
        "label_brand.delete" => format!("Delete brand {what}"),
        "label_printer.delete" => format!("Delete label printer {what} and its calibrations"),
        // The name is the level (`raises_control`)
        "roaster.control" => format!(
            "Set {}'s control to {}",
            device.unwrap_or(id),
            name.unwrap_or("on")
        ),
        // The name carries what to load (`print_details`)
        "labels.print" => format!("Print labels of {}", name.unwrap_or(id)),
        "labels.run.print" => format!("Print {}", name.unwrap_or(id)),
        other => format!("{other} {what}"),
    }
}

/// "Guji 1 (#7): 2 labels in sam-gift on a 5163 sheet from label 4 (labels 1–3 used)": what
/// the owner loads before approving a print.
fn print_details(roast: Option<&str>, body: Option<&str>) -> String {
    let b: Value = body
        .and_then(|b| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    let count = b["count"].as_u64().unwrap_or(1);
    let start = b["start"].as_u64().unwrap_or(1);
    let sheet = b["sheet"].as_str().unwrap_or("5163");
    let design = match (b["source"].is_string(), b["design"].as_str()) {
        (true, _) => " in a one-off design".to_string(),
        (false, Some(d)) => format!(" in {d}"),
        (false, None) => String::new(),
    };
    let used = match start {
        1 => "a full sheet".to_string(),
        2 => "label 1 used".to_string(),
        n => format!("labels 1–{} used", n - 1),
    };
    // What the QR code opens, when the print says (a share page is public)
    let qr = match b["qr"].as_str() {
        Some("share") => ", the QR code to its public share page",
        Some("none") => ", no QR code",
        _ => "",
    };
    format!(
        "\"{}\": {count} label{} {design} on a {sheet} sheet from label {start} ({used}){qr}",
        roast.unwrap_or("a roast"),
        if count == 1 { "" } else { "s" },
    )
    .replace("  ", " ")
}

/// What a gated call is about.
enum Target {
    /// There is no such thing: the handler answers 404 as it would without approvals
    Missing,
    /// Not gated after all: a roast on the virtual roaster
    Free,
    Found {
        name: Option<String>,
        device_id: Option<String>,
    },
}

async fn target(db: &SqlitePool, action: Action, id: &str) -> Result<Target> {
    let found = |row: Option<(Option<String>, Option<String>)>| match row {
        None => Target::Missing,
        Some((name, device_id)) => Target::Found { name, device_id },
    };
    Ok(match action {
        Action::RoastStart | Action::RoastDelete | Action::LabelPrint => {
            let row: Option<(Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
                "SELECT name, device_id, batch_number FROM roast_sessions WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(db)
            .await?;
            match row {
                Some((_, Some(d), _))
                    if action == Action::RoastStart
                        && crate::sim::virtual_roaster::is_simulated(&d) =>
                {
                    Target::Free
                }
                Some((name, device_id, batch)) => Target::Found {
                    name: match (name, batch) {
                        (Some(n), Some(b)) => Some(format!("{n} (#{b})")),
                        (n, _) => n,
                    },
                    device_id,
                },
                None => Target::Missing,
            }
        }
        Action::BeanDelete => found(
            sqlx::query_as("SELECT name, NULL FROM beans WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::OrderDelete => found(
            sqlx::query_as(
                "SELECT supplier || ' ' || order_number, NULL FROM purchase_orders WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(db)
            .await?,
        ),
        Action::ProfileDelete => found(
            sqlx::query_as("SELECT name, NULL FROM roast_profiles WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::LabelDesignDelete => found(
            sqlx::query_as("SELECT name, NULL FROM label_designs WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::LabelAssetDelete => found(
            sqlx::query_as("SELECT name, NULL FROM label_assets WHERE name = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::LabelBrandDelete => found(
            sqlx::query_as("SELECT name, NULL FROM label_brands WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::RoasterDelete => found(
            sqlx::query_as("SELECT name, device_id FROM devices WHERE id = ? OR device_id = ?")
                .bind(id)
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        // Any roaster id may have a setup; the virtual roaster's is fixed (the handler's 400)
        Action::RoasterControl if crate::sim::virtual_roaster::is_simulated(id) => Target::Free,
        Action::RoasterControl => Target::Found {
            name: None,
            device_id: Some(id.to_string()),
        },
        Action::LabelPrinterDelete => found(
            sqlx::query_as("SELECT name, NULL FROM label_printers WHERE id = ?")
                .bind(id)
                .fetch_optional(db)
                .await?,
        ),
        Action::LabelRunPrint => {
            let row: Option<(i64, String, String, i64)> =
                sqlx::query_as("SELECT labels, summary, sheet, start FROM label_runs WHERE id = ?")
                    .bind(id)
                    .fetch_optional(db)
                    .await?;
            match row {
                None => Target::Missing,
                Some((labels, summary, sheet, start)) => Target::Found {
                    name: Some(run_details(labels, &summary, &sheet, start)),
                    device_id: None,
                },
            }
        }
    })
}

/// "6 labels (Guji 1 (#7) ×2, …) on a 5163 sheet from label 4 (labels 1–3 used)": what the
/// owner loads before approving a run's print.
fn run_details(labels: i64, summary: &str, sheet: &str, start: i64) -> String {
    let used = match start {
        1 => "a full sheet".to_string(),
        2 => "label 1 used".to_string(),
        n => format!("labels 1–{} used", n - 1),
    };
    format!(
        "{labels} label{} ({summary}) on a {sheet} sheet from label {start} ({used})",
        if labels == 1 { "" } else { "s" }
    )
}

/// A row of `approvals`.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Row {
    id: String,
    token_id: Option<i64>,
    token_name: String,
    action: String,
    method: String,
    path: String,
    body: Option<String>,
    target_id: String,
    target_name: Option<String>,
    device_id: Option<String>,
    client_ip: Option<String>,
    url: String,
    status: String,
    created_at: String,
    expires_at: String,
    decided_at: Option<String>,
    result_status: Option<i64>,
    result: Option<String>,
}

/// An approval request, as every endpoint (and the 202) shows it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Approval {
    pub id: String,
    /// pending, approved (the call is being made), done, failed (the approved call answered
    /// an error), denied or expired
    pub status: String,
    /// roast.start, roast.delete, bean.delete, order.delete, profile.delete, roaster.delete,
    /// label_design.delete, label_asset.delete, label_brand.delete, labels.print,
    /// labels.run.print, label_printer.delete or roaster.control (its target name is the level)
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    pub action: String,
    /// "Delete bean "Kenya AA"", in words
    pub summary: String,
    pub method: String,
    pub path: String,
    pub target_id: String,
    pub target_name: Option<String>,
    /// The roaster, for a roast or a roaster
    pub device_id: Option<String>,
    pub token_id: Option<i64>,
    pub token_name: String,
    pub client_ip: Option<String>,
    /// Where the owner approves or denies it
    pub url: String,
    pub created_at: String,
    pub expires_at: String,
    pub decided_at: Option<String>,
    /// What the approved call answered: its HTTP status and body
    pub result_status: Option<i64>,
    pub result: Option<Value>,
}

fn view(r: &Row, now: &str) -> Approval {
    let status = if r.status == "pending" && r.expires_at.as_str() <= now {
        "expired".to_string()
    } else {
        r.status.clone()
    };
    Approval {
        id: r.id.clone(),
        status,
        summary: summary(
            &r.action,
            r.target_name.as_deref(),
            &r.target_id,
            r.device_id.as_deref(),
        ),
        action: r.action.clone(),
        method: r.method.clone(),
        path: r.path.clone(),
        target_id: r.target_id.clone(),
        target_name: r.target_name.clone(),
        device_id: r.device_id.clone(),
        token_id: r.token_id,
        token_name: r.token_name.clone(),
        client_ip: r.client_ip.clone(),
        url: r.url.clone(),
        created_at: r.created_at.clone(),
        expires_at: r.expires_at.clone(),
        decided_at: r.decided_at.clone(),
        result_status: r.result_status,
        result: r
            .result
            .as_deref()
            .map(|t| serde_json::from_str(t).unwrap_or_else(|_| json!(t))),
    }
}

/// The 202 a held call answers.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Held {
    pub approval: Approval,
    /// What happens now, in words
    pub message: String,
    /// `GET` it to wait for the answer (up to 25 s a call)
    pub wait_url: String,
}

fn accepted(a: Approval) -> Response {
    let wait_url = format!("/api/approvals/{}?wait={MAX_WAIT_SECS}", a.id);
    let message = format!(
        "{} needs the owner's approval: they approve or deny it at {} within {TTL_MINUTES} \
         minutes. Wait for the answer at {wait_url}.",
        a.summary, a.url
    );
    (
        StatusCode::ACCEPTED,
        [(header::LOCATION, wait_url.clone())],
        Json(Held {
            approval: a,
            message,
            wait_url,
        }),
    )
        .into_response()
}

/// On a request the approvals service makes after the owner approved: the auth layer takes
/// it as this token and does not hold it again. The network cannot set an extension.
#[derive(Debug, Clone)]
pub struct ApprovedCall(pub TokenAuth);

/// What became of a decision.
pub enum Decision {
    NotFound,
    /// It was no longer pending (its status)
    Closed(String),
    Decided(Box<Approval>),
}

/// The approvals service: held calls, the owner's decisions, and the replay.
#[derive(Clone)]
pub struct Approvals {
    db: SqlitePool,
    notifier: Notifier,
    /// The whole app, to make an approved call through; set once the router is built
    router: Arc<OnceLock<Router>>,
}

fn new_id() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut buf = [0u8; 64];
    let mut id = String::with_capacity(22);
    while id.len() < 22 {
        getrandom::fill(&mut buf).expect("the OS random source failed");
        for b in buf {
            if b < 248 && id.len() < 22 {
                id.push(ALPHABET[(b % 62) as usize] as char);
            }
        }
    }
    id
}

fn now_plus(minutes: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(minutes))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

const COLUMNS: &str = "id, token_id, token_name, action, method, path, body, target_id, \
     target_name, device_id, client_ip, url, status, created_at, expires_at, decided_at, \
     result_status, result";

impl Approvals {
    pub fn new(db: SqlitePool, notifier: Notifier) -> Self {
        Self {
            db,
            notifier,
            router: Arc::new(OnceLock::new()),
        }
    }

    pub fn set_router(&self, router: Router) {
        let _ = self.router.set(router);
    }

    async fn link_base(&self, headers: &HeaderMap) -> String {
        link_base(&self.db, headers).await
    }

    /// Hold a gated call from `token` (`gate` from [`gated`]): answer 202 with a pending
    /// request (the one already waiting for the same call, its body too, if any) and send its
    /// link out. Another body is another request: a second print of the same roast is not
    /// the first one's.
    /// `None` lets the call through: no such target (the handler answers 404), or not gated
    /// after all.
    pub async fn hold(
        &self,
        token: &TokenAuth,
        (action, target_id): (Action, String),
        method: &str,
        path: &str,
        body: Option<String>,
        headers: &HeaderMap,
    ) -> Option<Response> {
        let failed = |e: anyhow::Error| {
            tracing::error!(error = %e, "could not hold a call for approval");
            Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "the approval store failed",
                )
                    .into_response(),
            )
        };
        // A setup that leaves control off is not held (docs/roaster-setup.md)
        let level = match action {
            Action::RoasterControl => match raises_control(body.as_deref()) {
                Some(level) => Some(level),
                None => return None,
            },
            _ => None,
        };
        let now = tokens::now_text();
        let waiting = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM approvals WHERE token_id = ? AND method = ? AND path = ?
             AND body IS ? AND status = 'pending' AND expires_at > ?
             ORDER BY created_at DESC LIMIT 1"
        )))
        .bind(token.id)
        .bind(method)
        .bind(path)
        .bind(body.as_deref())
        .bind(&now)
        .fetch_optional(&self.db)
        .await;
        match waiting {
            Ok(Some(row)) => return Some(accepted(view(&row, &now))),
            Ok(None) => {}
            Err(e) => return failed(e.into()),
        }
        let (name, device_id) = match target(&self.db, action, &target_id).await {
            Ok(Target::Found { name, device_id }) => (name, device_id),
            Ok(Target::Missing | Target::Free) => return None,
            Err(e) => return failed(e),
        };
        let name = match action {
            Action::LabelPrint => Some(print_details(name.as_deref(), body.as_deref())),
            Action::RoasterControl => level,
            _ => name,
        };
        let id = new_id();
        let url = format!(
            "{}/settings/approvals?id={id}",
            self.link_base(headers).await
        );
        let ip = crate::auth::audit::client_ip(headers);
        let inserted = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "INSERT INTO approvals (id, token_id, token_name, action, method, path, body,
             target_id, target_name, device_id, client_ip, url, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING {COLUMNS}"
        )))
        .bind(&id)
        .bind(token.id)
        .bind(&token.name)
        .bind(action.as_str())
        .bind(method)
        .bind(path)
        .bind(body)
        .bind(&target_id)
        .bind(&name)
        .bind(&device_id)
        .bind(ip)
        .bind(&url)
        .bind(&now)
        .bind(now_plus(TTL_MINUTES))
        .fetch_all(&self.db)
        .await;
        let row = match inserted.map(|mut rows| rows.pop()) {
            Ok(Some(row)) => row,
            Ok(None) => return failed(anyhow::anyhow!("the insert returned no row")),
            Err(e) => return failed(e.into()),
        };
        let a = view(&row, &now);
        info!(approval = %a.id, token = %token.name, action = %a.action, target = %a.target_id,
            "call held for approval");
        let notifier = self.notifier.clone();
        let note = Note {
            title: "Roastwarden: approval needed".into(),
            body: format!(
                "The token \"{}\" asks to {}. Approve or deny it within {TTL_MINUTES} minutes: {}",
                a.token_name,
                lowercase_first(&a.summary),
                a.url
            ),
            tag: format!("approval-{}", a.id),
            url: format!("/settings/approvals?id={}", a.id),
            priority: Priority::High,
            payload: json!({ "event": "approval", "approval": &a }),
        };
        tokio::spawn(async move { notifier.send(note).await });
        Some(accepted(a))
    }

    async fn row(&self, id: &str) -> Result<Option<Row>> {
        Ok(sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM approvals WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(&self.db)
        .await?)
    }

    pub async fn get(&self, id: &str) -> Result<Option<Approval>> {
        Ok(self.row(id).await?.map(|r| view(&r, &tokens::now_text())))
    }

    /// The request, once it is decided or after `secs` (at most [`MAX_WAIT_SECS`]).
    pub async fn wait(&self, id: &str, secs: u64) -> Result<Option<Approval>> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs.min(MAX_WAIT_SECS));
        loop {
            let a = self.get(id).await?;
            let open = a
                .as_ref()
                .is_some_and(|a| matches!(a.status.as_str(), "pending" | "approved"));
            if !open || tokio::time::Instant::now() >= deadline {
                return Ok(a);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Newest first, the pending ones on top.
    pub async fn list(&self, limit: i64) -> Result<Vec<Approval>> {
        let now = tokens::now_text();
        let rows = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM approvals
             ORDER BY (status = 'pending' AND expires_at > ?) DESC, created_at DESC LIMIT ?"
        )))
        .bind(&now)
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.db)
        .await?;
        Ok(rows.iter().map(|r| view(r, &now)).collect())
    }

    /// Approve or deny a pending request. On approval the held call is made now, as the
    /// token, and its answer is kept in `result`.
    pub async fn decide(&self, id: &str, approve: bool) -> Result<Decision> {
        let now = tokens::now_text();
        let decided = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "UPDATE approvals SET status = ?, decided_at = ?
             WHERE id = ? AND status = 'pending' AND expires_at > ? RETURNING {COLUMNS}"
        )))
        .bind(if approve { "approved" } else { "denied" })
        .bind(&now)
        .bind(id)
        .bind(&now)
        .fetch_all(&self.db)
        .await?
        .pop();
        let Some(row) = decided else {
            return Ok(match self.row(id).await? {
                None => Decision::NotFound,
                Some(r) => Decision::Closed(view(&r, &now).status),
            });
        };
        info!(approval = %row.id, approve, action = %row.action, "approval decided");
        if !approve {
            return Ok(Decision::Decided(Box::new(view(&row, &now))));
        }
        let (status, body) = self.run(&row).await;
        let finished = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
            "UPDATE approvals SET status = ?, result_status = ?, result = ? WHERE id = ?
             RETURNING {COLUMNS}"
        )))
        .bind(if status.is_success() {
            "done"
        } else {
            "failed"
        })
        .bind(status.as_u16() as i64)
        .bind(body)
        .bind(&row.id)
        .fetch_all(&self.db)
        .await?
        .pop()
        .unwrap_or(row);
        Ok(Decision::Decided(Box::new(view(
            &finished,
            &tokens::now_text(),
        ))))
    }

    /// Make the held call as the token that asked, if the token is still good.
    async fn run(&self, a: &Row) -> (StatusCode, String) {
        let gone = |why: &str| {
            (
                StatusCode::GONE,
                json!({ "success": false, "error": { "code": "TOKEN_GONE", "message": why } })
                    .to_string(),
            )
        };
        let row = match a.token_id {
            Some(id) => tokens::get(&self.db, id).await,
            None => Ok(None),
        };
        let row = match row {
            Ok(Some(row)) => row,
            Ok(None) => return gone("The token was removed before the approval; nothing was done"),
            Err(e) => {
                warn!(error = %e, "could not read the token of an approved call");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "success": false, "error": { "code": "INTERNAL_ERROR", "message": "the token store failed" } }).to_string(),
                );
            }
        };
        let Some(token) = tokens::active(&row, chrono::Utc::now()) else {
            return gone("The token was revoked or expired before the approval; nothing was done");
        };
        let Some(router) = self.router.get().cloned() else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({ "success": false, "error": { "code": "UNAVAILABLE", "message": "the server is still starting" } }).to_string(),
            );
        };
        let Ok(method) = a.method.parse::<Method>() else {
            return gone("The held call is not valid; nothing was done");
        };
        let mut request = Request::builder().method(method).uri(&a.path);
        if a.body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        if let Some(ip) = &a.client_ip {
            request = request.header("x-forwarded-for", ip);
        }
        let Ok(mut request) = request.body(a.body.clone().map_or_else(Body::empty, Body::from))
        else {
            return gone("The held call is not valid; nothing was done");
        };
        request.extensions_mut().insert(ApprovedCall(token));
        let response = match router.oneshot(request).await {
            Ok(r) => r,
            Err(never) => match never {},
        };
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let body = if body.is_empty() {
            "null".to_string()
        } else if serde_json::from_str::<Value>(&body).is_ok() {
            body
        } else {
            json!(body).to_string()
        };
        (status, body)
    }
}

/// The body of a call about to be held, as text (JSON); `None` when empty or too large.
pub async fn read_body(body: Body) -> (Option<String>, axum::body::Bytes) {
    let bytes = axum::body::to_bytes(body, MAX_BODY)
        .await
        .unwrap_or_default();
    let text = (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned());
    (text, bytes)
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Where links point (approvals, label QR codes): the `public_url` setting, else the address
/// the request came to.
pub async fn link_base(db: &SqlitePool, headers: &HeaderMap) -> String {
    if let Some(base) = crate::auth::settings::get(db, "public_url")
        .await
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
    {
        return base;
    }
    let first = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let host = first("x-forwarded-host")
        .or_else(|| first(header::HOST.as_str()))
        .unwrap_or_else(|| "localhost:8080".to_string());
    let proto = first("x-forwarded-proto").unwrap_or_else(|| "http".to_string());
    format!("{proto}://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_print_says_what_its_qr_code_opens() {
        let d = print_details(
            Some("Guji"),
            Some(r#"{"count":1,"design":"gift","qr":"share"}"#),
        );
        assert_eq!(
            d,
            "\"Guji\": 1 label in gift on a 5163 sheet from label 1 (a full sheet), the QR code to its public share page"
        );
        assert!(!print_details(Some("Guji"), Some("{}")).contains("QR"));
    }

    #[test]
    fn only_the_gated_calls_are_gated() {
        assert_eq!(
            gated("POST", "/api/sessions/abc/start"),
            Some((Action::RoastStart, "abc".into()))
        );
        assert_eq!(
            gated("DELETE", "/api/sessions/abc"),
            Some((Action::RoastDelete, "abc".into()))
        );
        assert_eq!(
            gated("DELETE", "/api/beans/b1"),
            Some((Action::BeanDelete, "b1".into()))
        );
        assert_eq!(
            gated("DELETE", "/api/profiles/p1"),
            Some((Action::ProfileDelete, "p1".into()))
        );
        assert_eq!(
            gated("DELETE", "/api/devices/esp32_roaster_01"),
            Some((Action::RoasterDelete, "esp32_roaster_01".into()))
        );
        assert_eq!(gated("POST", "/api/sessions/abc/complete"), None);
        assert_eq!(
            gated("POST", "/api/label-runs/r1/print"),
            Some((Action::LabelRunPrint, "r1".into()))
        );
        assert!(Action::LabelRunPrint.always_held());
        assert_eq!(
            gated("POST", "/api/label-runs"),
            None,
            "making a PDF is not held"
        );
        assert_eq!(
            run_details(6, "Guji 1 (#7) ×2", "5163", 4),
            "6 labels (Guji 1 (#7) ×2) on a 5163 sheet from label 4 (labels 1–3 used)"
        );
        assert_eq!(gated("DELETE", "/api/sessions/abc/events/e1"), None);
        assert_eq!(gated("DELETE", "/api/plan/p1"), None);
        assert_eq!(gated("POST", "/api/roaster/esp32/control/setpoint"), None);
        assert_eq!(gated("PUT", "/api/beans/b1"), None);
        assert_eq!(
            gated("DELETE", "/api/label-designs/sam-gift"),
            Some((Action::LabelDesignDelete, "sam-gift".into()))
        );
        assert_eq!(
            gated("DELETE", "/api/label-assets/logo.svg"),
            Some((Action::LabelAssetDelete, "logo.svg".into()))
        );
        assert_eq!(gated("PUT", "/api/label-designs/sam-gift"), None);
        assert_eq!(
            gated("PUT", "/api/roaster/tc4_popper/setup"),
            Some((Action::RoasterControl, "tc4_popper".into()))
        );
        assert!(Action::RoasterControl.always_held());
        // Held only when the body leaves control on
        assert_eq!(
            raises_control(Some(r#"{"kind":"logger","control":"manual"}"#)),
            Some("manual".into())
        );
        assert_eq!(
            raises_control(Some(r#"{"kind":"drum_gas","control":"full"}"#)),
            Some("full".into())
        );
        assert_eq!(
            raises_control(Some(r#"{"kind":"logger","control":"off"}"#)),
            None
        );
        assert_eq!(raises_control(Some("not json")), None);
        assert_eq!(raises_control(None), None);
    }

    #[test]
    fn summaries_read_as_a_request() {
        assert_eq!(
            summary(
                "roast.start",
                Some("Guji (#3)"),
                "s1",
                Some("esp32_roaster_01")
            ),
            "Start roast \"Guji (#3)\" on esp32_roaster_01"
        );
        assert_eq!(summary("bean.delete", None, "b1", None), "Delete bean b1");
        assert_eq!(
            summary("roaster.control", Some("manual"), "tc4", Some("tc4")),
            "Set tc4's control to manual"
        );
        assert_eq!(lowercase_first("Delete bean"), "delete bean");
    }

    #[test]
    fn ids_are_long_and_random() {
        let a = new_id();
        assert_eq!(a.len(), 22);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, new_id());
    }
}
