//! Database backups and restore (docs/DEPLOY.md "Backups").
//!
//! A backup is `<dir>/roastwarden-<UTC time>.db`: a `VACUUM INTO` copy, consistent while the
//! server keeps writing, switched out of WAL so it is one file. It is written as a hidden
//! `.tmp` file and renamed into place, so a crash never leaves a half copy under a name that
//! counts; folder 0700, file 0600. The nightly run keeps the newest `backup_keep` copies
//! (default 14; 0 turns nightly copies off) at `backup_hour` UTC (default 3), and waits while
//! a roast is running.
//!
//! Restoring is two steps so that no database is swapped under a running server: [`inspect`]
//! checks a copy (integrity, a Roastwarden schema no newer than this binary) and
//! [`stage_restore`] puts it next to the database as `<db>.restore`; the next start calls
//! [`apply_staged_restore`] before opening the pool, which moves the current database aside
//! into `before-restore-<time>/` and the copy into its place.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Days, NaiveDateTime, TimeZone, Utc};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tracing::{error, info, warn};

use crate::notify::{EventKind, Notifier};

/// Copies are `roastwarden-<STAMP>.db`; only names of this shape are listed or pruned.
const PREFIX: &str = "roastwarden-";
const SUFFIX: &str = ".db";
const STAMP: &str = "%Y%m%d-%H%M%SZ";

pub const DEFAULT_KEEP: i64 = 14;
pub const DEFAULT_HOUR: u32 = 3;
pub const MAX_KEEP: i64 = 365;
const FIRST_CHECK: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(300);

/// A copy in the backup folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BackupFile {
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub bytes: u64,
    /// When the copy was made (from its name; the file's mtime for a renamed one).
    pub created_at: DateTime<Utc>,
}

/// A copy just written, and how many older ones retention removed.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Created {
    pub file: BackupFile,
    pub removed: usize,
}

/// What [`inspect`] found in a copy that can be restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct Inspection {
    /// The newest migration the copy has applied.
    pub schema: String,
    pub migrations: usize,
    pub roasts: i64,
    pub profiles: i64,
    pub beans: i64,
}

/// The backup folder: `ROASTWARDEN_BACKUP_DIR`, else `backups/` beside the database.
pub fn dir_for(db_path: &str) -> PathBuf {
    if let Some(d) = std::env::var("ROASTWARDEN_BACKUP_DIR")
        .ok()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
    {
        return PathBuf::from(d);
    }
    Path::new(db_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("backups")
}

/// Where a restore waits for the next start: `<db>.restore`.
pub fn staged_path(db: &Path) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(".restore");
    PathBuf::from(name)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("could not set the permissions of {}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// One connection to a file that is not the server's database.
async fn open_file(path: &Path, read_only: bool) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(read_only);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("could not open {}", path.display()))
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(name)
            .fetch_one(pool)
            .await?;
    Ok(n > 0)
}

/// Write a copy of the database behind `pool` into `dir`, then keep only the newest `keep`
/// copies there (0 keeps all). Older copies are removed only after this one is in place.
pub async fn create(pool: &SqlitePool, dir: &Path, keep: usize) -> Result<Created> {
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    set_mode(dir, 0o700)?;
    remove_leftovers(dir);

    let now = Utc::now();
    let name = format!("{PREFIX}{}{SUFFIX}", now.format(STAMP));
    let target = dir.join(&name);
    if target.exists() {
        bail!("{name} already exists: a backup was made this second");
    }
    let tmp = dir.join(format!(".{name}.tmp"));
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| anyhow!("the backup folder's path is not valid UTF-8"))?
        .to_string();

    let result = async {
        sqlx::query("VACUUM INTO ?")
            .bind(&tmp_str)
            .execute(pool)
            .await
            .context("VACUUM INTO failed")?;
        // One self-contained file, not WAL.
        let copy = open_file(&tmp, false).await?;
        let r = sqlx::query("PRAGMA journal_mode = DELETE")
            .execute(&copy)
            .await
            .context("could not switch the copy out of WAL");
        copy.close().await;
        r?;
        set_mode(&tmp, 0o600)?;
        std::fs::File::open(&tmp)
            .and_then(|f| f.sync_all())
            .with_context(|| format!("could not flush {}", tmp.display()))?;
        std::fs::rename(&tmp, &target)
            .with_context(|| format!("could not move the copy to {}", target.display()))?;
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    let bytes = std::fs::metadata(&target).map_or(0, |m| m.len());
    let removed = prune(dir, keep)?;
    info!(file = %name, bytes, removed, "database backup written");
    Ok(Created {
        file: BackupFile {
            name,
            path: target,
            bytes,
            created_at: now,
        },
        removed,
    })
}

/// Hidden `.tmp` files a crash left behind.
fn remove_leftovers(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(&format!(".{PREFIX}")) && name.ends_with(".tmp")
            && let Err(e) = std::fs::remove_file(entry.path()) {
                warn!(file = name, error = %e, "could not remove a half-written backup");
            }
    }
}

/// The copies in `dir`, newest first; an absent folder has none.
pub fn list(dir: &Path) -> Result<Vec<BackupFile>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", dir.display())),
    };
    let mut files: Vec<BackupFile> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let stamp = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.to_string();
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            let created_at = NaiveDateTime::parse_from_str(&stamp, STAMP)
                .map(|t| Utc.from_utc_datetime(&t))
                .ok()
                .or_else(|| meta.modified().ok().map(DateTime::<Utc>::from))?;
            Some(BackupFile {
                path: entry.path(),
                name,
                bytes: meta.len(),
                created_at,
            })
        })
        .collect();
    files.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.name.cmp(&a.name)));
    Ok(files)
}

/// A copy in `dir` by name; only names `list` would show, so no path tricks.
pub fn find(dir: &Path, name: &str) -> Result<Option<BackupFile>> {
    Ok(list(dir)?.into_iter().find(|f| f.name == name))
}

/// Delete all but the newest `keep` copies (0 keeps all). Returns how many went.
pub fn prune(dir: &Path, keep: usize) -> Result<usize> {
    if keep == 0 {
        return Ok(0);
    }
    let mut removed = 0;
    for old in list(dir)?.into_iter().skip(keep) {
        std::fs::remove_file(&old.path)
            .with_context(|| format!("could not remove {}", old.name))?;
        removed += 1;
    }
    Ok(removed)
}

/// Why a copy cannot be restored (a 400, not a 500).
#[derive(Debug)]
pub struct NotRestorable(pub String);

impl std::fmt::Display for NotRestorable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotRestorable {}

/// Check that `path` is a Roastwarden database this binary can run: SQLite's integrity check,
/// the core tables, and no migration this binary does not know.
pub async fn inspect(path: &Path) -> Result<Inspection> {
    let bad = |m: String| anyhow::Error::new(NotRestorable(m));
    if !path.is_file() {
        return Err(bad(format!("{} is not a file", path.display())));
    }
    let pool = open_file(path, true)
        .await
        .map_err(|e| bad(format!("not a database: {e:#}")))?;
    let result = async {
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .map_err(|e| bad(format!("not a database: {e}")))?;
        if integrity != "ok" {
            return Err(bad(format!("the copy is damaged: {integrity}")));
        }
        for table in [
            "schema_migrations",
            "roast_sessions",
            "roast_profiles",
            "settings",
        ] {
            if !table_exists(&pool, table).await? {
                return Err(bad(format!(
                    "not a Roastwarden database (it has no {table} table)"
                )));
            }
        }
        let applied: Vec<String> = sqlx::query_scalar("SELECT name FROM schema_migrations")
            .fetch_all(&pool)
            .await?;
        let known: Vec<&str> = crate::db::MIGRATIONS.iter().map(|m| m.name).collect();
        if let Some(unknown) = applied.iter().find(|n| !known.contains(&n.as_str())) {
            return Err(bad(format!(
                "the copy has migration {unknown}, which this version does not know: restore it \
                 with the release that made it"
            )));
        }
        let schema = known
            .iter()
            .rev()
            .find(|k| applied.iter().any(|a| a == *k))
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        let count = |sql: &'static str| {
            let pool = pool.clone();
            async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await }
        };
        let beans = if table_exists(&pool, "beans").await? {
            count("SELECT COUNT(*) FROM beans").await?
        } else {
            0
        };
        Ok(Inspection {
            schema,
            migrations: applied.len(),
            roasts: count("SELECT COUNT(*) FROM roast_sessions").await?,
            profiles: count("SELECT COUNT(*) FROM roast_profiles").await?,
            beans,
        })
    }
    .await;
    pool.close().await;
    result
}

/// Copy `backup` next to the database as `<db>.restore`, for the next start to put in place.
/// Check it with [`inspect`] first.
pub fn stage_restore(db: &Path, backup: &Path) -> Result<PathBuf> {
    let staged = staged_path(db);
    let mut tmp = staged.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::copy(backup, &tmp)
        .with_context(|| format!("could not copy {} next to the database", backup.display()))?;
    set_mode(&tmp, 0o600)?;
    std::fs::File::open(&tmp)
        .and_then(|f| f.sync_all())
        .with_context(|| format!("could not flush {}", tmp.display()))?;
    std::fs::rename(&tmp, &staged)
        .with_context(|| format!("could not move the copy to {}", staged.display()))?;
    Ok(staged)
}

/// Drop a staged restore. True when there was one.
pub fn unstage_restore(db: &Path) -> Result<bool> {
    let staged = staged_path(db);
    if !staged.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&staged)
        .with_context(|| format!("could not remove {}", staged.display()))?;
    Ok(true)
}

/// At start-up, before the database is opened: put a staged restore in place. The current
/// database (with its `-wal` and `-shm`, which belong to it) moves into
/// `before-restore-<time>/` beside it. Returns that folder; `None` when nothing is staged.
pub fn apply_staged_restore(db: &Path) -> Result<Option<PathBuf>> {
    let staged = staged_path(db);
    if !staged.is_file() {
        return Ok(None);
    }
    let parent = db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let previous = parent.join(format!("before-restore-{}", Utc::now().format(STAMP)));
    std::fs::create_dir_all(&previous)
        .with_context(|| format!("could not create {}", previous.display()))?;
    set_mode(&previous, 0o700)?;
    let file_name = db
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", db.display()))?
        .to_owned();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = file_name.clone();
        name.push(suffix);
        let from = parent.join(&name);
        match std::fs::rename(&from, previous.join(&name)) {
            Ok(()) => {}
            // Not there, or SQLite removed its -wal or -shm as the last connection closed
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| format!("could not move {} aside", from.display()))
            }
        }
    }
    std::fs::rename(&staged, db).with_context(|| {
        format!(
            "could not put the restored copy in place as {}",
            db.display()
        )
    })?;
    info!(previous = %previous.display(), "restored the database from a backup");
    Ok(Some(previous))
}

// ---- the nightly schedule ----

/// `backup_keep`: copies to keep, 0..=365 (0 turns nightly copies off).
pub async fn keep(pool: &SqlitePool) -> i64 {
    setting(pool, "backup_keep")
        .await
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|k| (0..=MAX_KEEP).contains(k))
        .unwrap_or(DEFAULT_KEEP)
}

/// `backup_hour`: the UTC hour of the nightly copy.
pub async fn hour(pool: &SqlitePool) -> u32 {
    setting(pool, "backup_hour")
        .await
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|h| *h < 24)
        .unwrap_or(DEFAULT_HOUR)
}

async fn setting(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// The most recent `hour`:00 UTC at or before `now`.
pub fn latest_slot(now: DateTime<Utc>, hour: u32) -> DateTime<Utc> {
    let today = now
        .date_naive()
        .and_hms_opt(hour, 0, 0)
        .map_or(now, |t| t.and_utc());
    if today <= now {
        today
    } else {
        today.checked_sub_days(Days::new(1)).unwrap_or(today)
    }
}

/// One nightly attempt per slot; a server that was down at that hour catches up.
pub fn due(now: DateTime<Utc>, hour: u32, last_attempt: Option<DateTime<Utc>>) -> bool {
    last_attempt.is_none_or(|t| t < latest_slot(now, hour))
}

pub fn next_run(
    now: DateTime<Utc>,
    hour: u32,
    last_attempt: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    let slot = latest_slot(now, hour);
    if due(now, hour, last_attempt) {
        slot
    } else {
        slot.checked_add_days(Days::new(1)).unwrap_or(slot)
    }
}

/// One attempt, as `backup_runs` records it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct BackupRun {
    pub trigger: String,
    pub started_at: DateTime<Utc>,
    pub file_name: Option<String>,
    pub size_bytes: Option<i64>,
    pub removed: i64,
    pub error: Option<String>,
}

pub async fn latest_run(pool: &SqlitePool, trigger: Option<&str>) -> Result<Option<BackupRun>> {
    Ok(sqlx::query_as::<_, BackupRun>(
        "SELECT trigger, started_at, file_name, size_bytes, removed, error FROM backup_runs
         WHERE (? IS NULL OR trigger = ?) ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(trigger)
    .bind(trigger)
    .fetch_optional(pool)
    .await?)
}

/// The server's backups: the folder, the database file, and one run at a time.
pub struct Backups {
    pub dir: PathBuf,
    pub db_path: PathBuf,
    running: tokio::sync::Mutex<()>,
}

impl Backups {
    pub fn new(db_path: &str) -> Arc<Self> {
        Arc::new(Self {
            dir: dir_for(db_path),
            db_path: PathBuf::from(db_path),
            running: tokio::sync::Mutex::new(()),
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.try_lock().is_err()
    }

    pub fn staged(&self) -> bool {
        staged_path(&self.db_path).is_file()
    }
}

/// Write a copy now (`trigger` = nightly | manual) and record the attempt.
pub async fn run(pool: &SqlitePool, backups: &Backups, trigger: &str) -> Result<Created> {
    let _one_at_a_time = backups.running.lock().await;
    let keep = keep(pool).await as usize;
    let started_at = Utc::now();
    let result = create(pool, &backups.dir, keep).await;
    let error = result.as_ref().err().map(|e| format!("{e:#}"));
    let recorded = sqlx::query(
        "INSERT INTO backup_runs (trigger, started_at, file_name, size_bytes, removed, error)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(trigger)
    .bind(started_at)
    .bind(result.as_ref().ok().map(|c| c.file.name.clone()))
    .bind(result.as_ref().ok().map(|c| c.file.bytes as i64))
    .bind(result.as_ref().map_or(0, |c| c.removed as i64))
    .bind(error)
    .execute(pool)
    .await;
    if let Err(e) = recorded {
        warn!(error = %e, "could not record the backup attempt");
    }
    result
}

/// `roast backup`: the same copy as [`run`], made next to the database without the server
/// (`docker exec roastwarden roast backup`). The database must exist; it is not migrated. The
/// attempt is recorded with `trigger`, so Settings → Backups shows it.
pub async fn back_up_file(db_path: &str, trigger: &str) -> Result<Created> {
    let path = Path::new(db_path);
    if !path.is_file() {
        bail!("no database at {db_path} (set ROASTWARDEN_DB_PATH or pass --db)");
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .with_context(|| format!("could not open {db_path}"))?;
    if !table_exists(&pool, "backup_runs").await? {
        pool.close().await;
        bail!("{db_path} is not a Roastwarden database of this version (no backup_runs table)");
    }
    let result = run(&pool, &Backups::new(db_path), trigger).await;
    pool.close().await;
    result
}

/// Check every few minutes whether a nightly copy is due.
pub fn start_schedule(pool: SqlitePool, backups: Arc<Backups>, notifier: Notifier) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            tick(&pool, &backups, &notifier, Utc::now()).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

async fn tick(pool: &SqlitePool, backups: &Backups, notifier: &Notifier, now: DateTime<Utc>) {
    if keep(pool).await == 0 {
        return;
    }
    let last = match latest_run(pool, Some("nightly")).await {
        Ok(last) => last.map(|r| r.started_at),
        Err(e) => {
            error!(error = %e, "could not read the backup history; skipping this check");
            return;
        }
    };
    if !due(now, hour(pool).await, last) {
        return;
    }
    // A roast writes a sample a second: let it finish first.
    let roasting: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM roast_sessions WHERE status IN ('active', 'paused')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if roasting > 0 {
        return;
    }
    // Told once when it starts failing and once when it works again (docs/notifications.md).
    match run(pool, backups, "nightly").await {
        Ok(c) => {
            notifier
                .resolved(
                    EventKind::Backup,
                    "backup",
                    "Nightly backups work again",
                    &format!("{} written.", c.file.name),
                )
                .await;
        }
        Err(e) => {
            let why = format!("{e:#}");
            error!(error = %why, "nightly backup failed");
            let why: String = why.chars().take(300).collect();
            notifier
                .problem(EventKind::Backup, "backup", "Nightly backup failed", &why)
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("roastwarden-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    async fn database(dir: &Path) -> (PathBuf, SqlitePool) {
        let path = dir.join("roastwarden.db");
        let pool = crate::db::open(path.to_str().unwrap()).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('backup_keep', '14')")
            .execute(&pool)
            .await
            .unwrap();
        (path, pool)
    }

    #[test]
    fn the_latest_slot_is_today_or_yesterday() {
        assert_eq!(
            latest_slot(at("2026-10-22T14:20:00Z"), 3),
            at("2026-10-22T03:00:00Z")
        );
        assert_eq!(
            latest_slot(at("2026-10-22T02:59:59Z"), 3),
            at("2026-10-21T03:00:00Z")
        );
    }

    #[test]
    fn one_attempt_per_slot_and_a_catch_up_after_downtime() {
        assert!(due(at("2026-10-22T14:00:00Z"), 3, None));
        let tried = Some(at("2026-10-22T03:02:00Z"));
        assert!(!due(at("2026-10-22T03:07:00Z"), 3, tried));
        assert!(!due(at("2026-10-23T02:59:00Z"), 3, tried));
        assert!(due(at("2026-10-23T03:00:00Z"), 3, tried));
        assert!(due(
            at("2026-10-22T09:02:00Z"),
            3,
            Some(at("2026-10-21T03:01:00Z"))
        ));
        assert_eq!(
            next_run(at("2026-10-22T09:00:00Z"), 3, tried),
            at("2026-10-23T03:00:00Z")
        );
    }

    #[tokio::test]
    async fn a_backup_is_a_private_single_file_copy() {
        let tmp = scratch();
        let (_path, pool) = database(&tmp).await;
        let dir = tmp.join("backups");
        let created = create(&pool, &dir, 14).await.unwrap();
        assert!(
            created.file.name.starts_with("roastwarden-") && created.file.name.ends_with(".db")
        );
        assert!(created.file.bytes > 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir), 0o700);
            assert_eq!(mode(&created.file.path), 0o600);
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec![created.file.name.clone()], "no -wal, no .tmp");
        let copy = open_file(&created.file.path, true).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&copy)
            .await
            .unwrap();
        assert_eq!(mode, "delete");
        copy.close().await;
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn lists_newest_first_and_prunes_beyond_keep() {
        let dir = scratch();
        for name in [
            "roastwarden-20261001-030000Z.db",
            "roastwarden-20261003-030000Z.db",
            "roastwarden-20261002-030000Z.db",
            "notes.txt",
            "roastwarden.db",
            ".roastwarden-20261004-030000Z.db.tmp",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let names: Vec<String> = list(&dir).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(
            names,
            [
                "roastwarden-20261003-030000Z.db",
                "roastwarden-20261002-030000Z.db",
                "roastwarden-20261001-030000Z.db"
            ]
        );
        assert!(find(&dir, "../roastwarden.db").unwrap().is_none());
        assert_eq!(prune(&dir, 0).unwrap(), 0);
        assert_eq!(prune(&dir, 2).unwrap(), 1);
        assert!(dir.join("notes.txt").exists() && dir.join("roastwarden.db").exists());
        remove_leftovers(&dir);
        assert!(!dir.join(".roastwarden-20261004-030000Z.db.tmp").exists());
        assert_eq!(
            list(&dir.join("absent")).unwrap(),
            [] as [crate::ops::backups::BackupFile; 0]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn inspect_accepts_a_good_copy_and_refuses_the_rest() {
        let tmp = scratch();
        let (_path, pool) = database(&tmp).await;
        let created = create(&pool, &tmp.join("b"), 0).await.unwrap();
        let found = inspect(&created.file.path).await.unwrap();
        assert_eq!(found.schema, crate::db::MIGRATIONS.last().unwrap().name);
        assert_eq!(found.migrations, crate::db::MIGRATIONS.len());
        assert!(found.profiles >= 3, "the seed profiles");

        let junk = tmp.join("junk.db");
        std::fs::write(&junk, b"this is not sqlite at all, not even close......").unwrap();
        let err = inspect(&junk).await.unwrap_err();
        assert!(err.downcast_ref::<NotRestorable>().is_some(), "{err:#}");
        assert!(inspect(&tmp.join("missing.db")).await.is_err());

        let future = tmp.join("future.db");
        std::fs::copy(&created.file.path, &future).unwrap();
        let rw = open_file(&future, false).await.unwrap();
        sqlx::query("INSERT INTO schema_migrations (name) VALUES ('999_from_the_future')")
            .execute(&rw)
            .await
            .unwrap();
        rw.close().await;
        let err = inspect(&future).await.unwrap_err().to_string();
        assert!(err.contains("does not know"), "{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn a_staged_restore_replaces_the_database_at_the_next_start() {
        let tmp = scratch();
        let (path, pool) = database(&tmp).await;
        let created = create(&pool, &tmp.join("b"), 0).await.unwrap();
        // A change after the backup, which the restore must undo.
        sqlx::query("UPDATE settings SET value = '5' WHERE key = 'backup_keep'")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        assert!(
            apply_staged_restore(&path).unwrap().is_none(),
            "nothing staged"
        );
        let staged = stage_restore(&path, &created.file.path).unwrap();
        assert!(staged.ends_with("roastwarden.db.restore"));
        assert!(unstage_restore(&path).unwrap());
        stage_restore(&path, &created.file.path).unwrap();
        let previous = apply_staged_restore(&path).unwrap().unwrap();
        assert!(!staged.exists());
        assert!(previous.join("roastwarden.db").is_file());

        let pool = crate::db::open(path.to_str().unwrap()).await.unwrap();
        assert_eq!(
            keep(&pool).await,
            14,
            "the restored copy predates the change"
        );
        pool.close().await;
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn runs_are_recorded() {
        let tmp = scratch();
        let (path, pool) = database(&tmp).await;
        let backups = Backups::new(path.to_str().unwrap());
        assert_eq!(backups.dir, tmp.join("backups"));
        let created = run(&pool, &backups, "manual").await.unwrap();
        let last = latest_run(&pool, None).await.unwrap().unwrap();
        assert_eq!(last.trigger, "manual");
        assert_eq!(last.file_name.as_deref(), Some(created.file.name.as_str()));
        assert!(latest_run(&pool, Some("nightly")).await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
