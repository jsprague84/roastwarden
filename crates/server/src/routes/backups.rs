//! Backups and restore (`docs/DEPLOY.md`, "Backups"): the copies, the last attempt and the
//! next nightly run; "Back up now"; a checked restore that waits for the next start.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::ops::backups::{self, BackupFile, BackupRun, Created, Inspection, NotRestorable};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(overview, back_up_now))
        .routes(routes!(cancel_restore))
        .routes(routes!(check))
        .routes(routes!(restore))
}

#[derive(Serialize, ToSchema)]
pub struct BackupOverview {
    /// Where the copies are written
    dir: String,
    /// How many nightly copies are kept; 0 turns the nightly copy off
    keep: i64,
    /// The hour (UTC) of the nightly copy
    hour_utc: u32,
    /// The copies, newest first
    files: Vec<BackupFile>,
    /// The newest attempt, nightly or by hand
    last: Option<BackupRun>,
    /// When the next nightly copy is due
    next_run: Option<DateTime<Utc>>,
    /// A copy is being written now
    running: bool,
    /// A restore waits for the next start
    restore_staged: bool,
}

#[derive(Serialize, ToSchema)]
pub struct RestoreStaged {
    /// The copy that goes in at the next start
    staged: String,
    inspection: Inspection,
}

fn failed(what: &str, e: anyhow::Error) -> Response {
    if let Some(r) = e.downcast_ref::<NotRestorable>() {
        return (StatusCode::BAD_REQUEST, r.0.clone()).into_response();
    }
    tracing::error!(error = %format!("{e:#}"), "{what}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{what}: {e:#}")).into_response()
}

/// The copies, the last attempt and the next nightly run.
#[utoipa::path(get, path = "/api/backups", tag = "backups",
    responses((status = 200, body = BackupOverview)))]
async fn overview(State(state): State<AppState>) -> Response {
    let b = &state.backups;
    let files = match backups::list(&b.dir) {
        Ok(f) => f,
        Err(e) => return failed("Could not list the backups", e),
    };
    let keep = backups::keep(&state.db).await;
    let hour = backups::hour(&state.db).await;
    let last = backups::latest_run(&state.db, None).await.ok().flatten();
    let last_nightly = backups::latest_run(&state.db, Some("nightly"))
        .await
        .ok()
        .flatten();
    let next = (keep > 0).then(|| {
        backups::next_run(
            Utc::now(),
            hour,
            last_nightly.as_ref().map(|r| r.started_at),
        )
    });
    Json(BackupOverview {
        dir: b.dir.display().to_string(),
        keep,
        hour_utc: hour,
        files,
        last,
        next_run: next,
        running: b.is_running(),
        restore_staged: b.staged(),
    })
    .into_response()
}

/// Write a copy now; older copies beyond `backup_keep` go once it is in place.
#[utoipa::path(post, path = "/api/backups", tag = "backups",
    responses((status = 200, body = Created)))]
async fn back_up_now(State(state): State<AppState>) -> Response {
    match backups::run(&state.db, &state.backups, "manual").await {
        Ok(created) => Json(created).into_response(),
        Err(e) => failed("Backup failed", e),
    }
}

// The error is the response itself, returned at once; boxing it would buy nothing.
#[allow(clippy::result_large_err)]
fn found(state: &AppState, name: &str) -> Result<backups::BackupFile, Response> {
    match backups::find(&state.backups.dir, name) {
        Ok(Some(f)) => Ok(f),
        Ok(None) => Err((StatusCode::NOT_FOUND, "No such backup").into_response()),
        Err(e) => Err(failed("Could not list the backups", e)),
    }
}

/// Check a copy without touching anything: safe at any time.
#[utoipa::path(post, path = "/api/backups/{name}/check", tag = "backups",
    params(("name" = String, Path, description = "A file name from `GET /api/backups`")),
    responses((status = 200, body = Inspection),
        (status = 400, description = "Not a copy that can be restored", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn check(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let file = match found(&state, &name) {
        Ok(f) => f,
        Err(r) => return r,
    };
    match backups::inspect(&file.path).await {
        Ok(i) => Json(i).into_response(),
        Err(e) => failed("Could not check the backup", e),
    }
}

/// Check a copy and stage it; the next start of the server puts it in place. Owner key only.
#[utoipa::path(post, path = "/api/backups/{name}/restore", tag = "backups",
    params(("name" = String, Path, description = "A file name from `GET /api/backups`")),
    responses((status = 200, body = RestoreStaged),
        (status = 400, description = "Not a copy that can be restored", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn restore(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let file = match found(&state, &name) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let inspection = match backups::inspect(&file.path).await {
        Ok(i) => i,
        Err(e) => return failed("Could not check the backup", e),
    };
    match backups::stage_restore(&state.backups.db_path, &file.path) {
        Ok(_) => {
            tracing::warn!(backup = %name, "restore staged: it replaces the database at the next start");
            Json(RestoreStaged {
                staged: name,
                inspection,
            })
            .into_response()
        }
        Err(e) => failed("Could not stage the restore", e),
    }
}

/// Cancel a staged restore.
#[utoipa::path(delete, path = "/api/backups/restore", tag = "backups",
    responses((status = 204, description = "Cancelled"),
        (status = 404, description = "No restore is staged", body = ErrorBody)))]
async fn cancel_restore(State(state): State<AppState>) -> Response {
    match backups::unstage_restore(&state.backups.db_path) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "No restore is staged").into_response(),
        Err(e) => failed("Could not cancel the restore", e),
    }
}
