//! Print runs (docs/labels.md, "Print runs"): labels made for one
//! sheet, of one roast or several (a gift run), kept with what each label carried, its design
//! version and source, the printer's job and its states with times, and the PDF, so a reprint
//! matches. The next free label of each part-used sheet moves on when a run's job completes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use utoipa::ToSchema;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// One item of a run: a roast's labels in one design, as made
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct RunItem {
    /// The roast (its id), when the labels are a roast's
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roast: Option<String>,
    /// The label's name (the coffee), with its batch: "Guji 1 (#7)"
    pub name: String,
    /// `id@version` for a saved design, else its id
    pub design: String,
    /// How many labels
    pub count: usize,
    /// The design's parameters as given
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    #[schema(value_type = Object)]
    pub params: serde_json::Map<String, Value>,
    /// The brand kit's id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// What each label carried (the fields a design reads): a reprint uses it as it was
    #[schema(value_type = Object)]
    pub data: Value,
    /// The brand as the design read it (`l.brand`)
    #[serde(default)]
    #[schema(value_type = Object)]
    pub brand_value: Value,
    /// The design's source as it was, so a reprint lays it out the same
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The QR code's target
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A state the printer reported for a run's job, and when
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct RunState {
    pub at: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheets: Option<i32>,
}

/// A run as listed: what, where, when and how it went (no label data, no PDF)
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct RunSummary {
    pub id: String,
    pub created_at: String,
    /// owner, or the token's name
    pub created_by: String,
    pub sheet: String,
    pub sheet_name: String,
    /// The first label used on the first page
    pub start: i64,
    pub labels: i64,
    pub pages: i64,
    /// "Guji 1 (#7) ×2, Palmichal (#5)"
    pub summary: String,
    /// made (a PDF, not sent), sent, pending, processing, stopped (waiting: paper, a door),
    /// completed, aborted, canceled, failed (could not be sent), printed (marked by hand)
    pub state: String,
    /// Where it was sent (its address)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer: Option<String>,
    /// The printer it was laid out for (its calibration) and sent to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reprint_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A run with its items (each label's data and design) and its job's states
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Run {
    #[serde(flatten)]
    pub run: RunSummary,
    pub items: Vec<RunItem>,
    pub states: Vec<RunState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tray: Option<String>,
    /// True when a make found the same labels in a run not yet printed and answered that
    /// run, so a second Download is the same run (`new: true` makes another)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reused: bool,
    /// The bags it took off its roasts (an item's `pack`): packed moves that name the run
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packed: Vec<crate::coffee::roasted::RoastedMove>,
}

#[derive(sqlx::FromRow)]
struct Row {
    #[sqlx(flatten)]
    run: RunSummary,
    items: String,
    states: String,
    tray: Option<String>,
}

const COLUMNS: &str = "id, created_at, created_by, sheet, sheet_name, start, labels, pages, summary, state, printer, printer_id, job_id, reprint_of, note";

/// What a new run holds
pub struct NewRun<'a> {
    pub id: &'a str,
    pub by: &'a str,
    pub sheet: &'a str,
    pub sheet_name: &'a str,
    pub start: usize,
    pub labels: usize,
    pub pages: usize,
    pub items: &'a [RunItem],
    pub pdf: &'a [u8],
    pub reprint_of: Option<&'a str>,
    pub note: Option<&'a str>,
    /// The printer it is laid out for (its calibration)
    pub printer_id: Option<&'a str>,
}

/// "Guji 1 (#7) ×2, Palmichal (#5)"
pub fn summary(items: &[RunItem]) -> String {
    items
        .iter()
        .map(|i| {
            if i.count > 1 {
                format!("{} ×{}", i.name, i.count)
            } else {
                i.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn insert(db: &SqlitePool, r: NewRun<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO label_runs (id, created_at, created_by, sheet, sheet_name, start, labels, pages,
         items, summary, state, states, pdf, reprint_of, note, printer_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'made', '[]', ?, ?, ?, ?)",
    )
    .bind(r.id)
    .bind(now())
    .bind(r.by)
    .bind(r.sheet)
    .bind(r.sheet_name)
    .bind(r.start as i64)
    .bind(r.labels as i64)
    .bind(r.pages as i64)
    .bind(serde_json::to_string(r.items)?)
    .bind(summary(r.items))
    .bind(r.pdf)
    .bind(r.reprint_of)
    .bind(r.note)
    .bind(r.printer_id)
    .execute(db)
    .await?;
    Ok(())
}

/// The newest runs first
pub async fn list(db: &SqlitePool, limit: i64) -> Result<Vec<RunSummary>> {
    Ok(sqlx::query_as::<_, RunSummary>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM label_runs ORDER BY created_at DESC, rowid DESC LIMIT ?"
    )))
    .bind(limit)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: &SqlitePool, id: &str) -> Result<Option<Run>> {
    let row = sqlx::query_as::<_, Row>(AssertSqlSafe(format!(
        "SELECT {COLUMNS}, items, states, tray FROM label_runs WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?;
    let packed = sqlx::query_as::<_, crate::coffee::roasted::RoastedMove>(AssertSqlSafe(format!(
        "SELECT {} FROM roasted_moves WHERE label_run = ? ORDER BY at, created_at",
        crate::coffee::roasted::MOVE_COLUMNS
    )))
    .bind(id)
    .fetch_all(db)
    .await?;
    Ok(match row {
        None => None,
        Some(r) => Some(Run {
            run: r.run,
            items: serde_json::from_str(&r.items)?,
            states: serde_json::from_str(&r.states)?,
            tray: r.tray,
            reused: false,
            packed,
        }),
    })
}

/// How long a run made and not printed stays the one a repeat make answers
pub const REUSE_HOURS: i64 = 24;

/// The newest run made in the last [`REUSE_HOURS`] and not yet sent or printed, that is not
/// a reprint and has the same labels: the sheet, first label, printer, note and every item
/// as it was laid out (design, settings, brand, data, QR code). A repeat Download answers it
/// instead of keeping a second copy, so the sheet's next free label moves on once.
pub async fn unprinted_same(
    db: &SqlitePool,
    sheet: &str,
    start: usize,
    printer_id: Option<&str>,
    note: Option<&str>,
    items: &[RunItem],
) -> Result<Option<String>> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(REUSE_HOURS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT id, items FROM label_runs
         WHERE state = 'made' AND reprint_of IS NULL AND sheet = ? AND start = ?
           AND printer_id IS ? AND note IS ? AND created_at >= ?
         ORDER BY created_at DESC, rowid DESC LIMIT 5",
    )
    .bind(sheet)
    .bind(start as i64)
    .bind(printer_id)
    .bind(note)
    .bind(since)
    .fetch_all(db)
    .await?;
    for (id, text) in rows {
        let made: Vec<RunItem> = serde_json::from_str(&text)?;
        if made == items {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub async fn pdf(db: &SqlitePool, id: &str) -> Result<Option<Vec<u8>>> {
    Ok(
        sqlx::query_as::<_, (Vec<u8>,)>("SELECT pdf FROM label_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?
            .map(|r| r.0),
    )
}

/// The run was sent to `printer`: its job and tray
pub async fn sent(
    db: &SqlitePool,
    id: &str,
    printer: &str,
    printer_id: Option<&str>,
    job_id: Option<i32>,
    tray: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE label_runs SET printer = ?, printer_id = COALESCE(?, printer_id), job_id = ?, tray = ? WHERE id = ?",
    )
        .bind(printer)
        .bind(printer_id)
        .bind(job_id)
        .bind(tray)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// A new state from the printer (or "failed", "printed"): kept with the time when it differs
/// from the last one. True when it changed.
pub async fn state(
    db: &SqlitePool,
    id: &str,
    state: &str,
    reasons: &[String],
    sheets: Option<i32>,
) -> Result<bool> {
    // It reads the states, then writes them: the write lock first (db::begin_write).
    let mut tx = crate::db::begin_write(db).await?;
    let Some((current, states)) =
        sqlx::query_as::<_, (String, String)>("SELECT state, states FROM label_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok(false);
    };
    let mut list: Vec<RunState> = serde_json::from_str(&states)?;
    let reasons: Vec<String> = reasons.iter().filter(|r| *r != "none").cloned().collect();
    let same = list
        .last()
        .is_some_and(|l| l.state == state && l.reasons == reasons && l.sheets == sheets);
    if current == state && same {
        return Ok(false);
    }
    list.push(RunState {
        at: now(),
        state: state.to_string(),
        reasons,
        sheets,
    });
    sqlx::query("UPDATE label_runs SET state = ?, states = ? WHERE id = ?")
        .bind(state)
        .bind(serde_json::to_string(&list)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// The next free label on each part-used sheet: (sheet, next, the run that left it so)
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct SheetUse {
    pub sheet: String,
    /// The next free label, from 1 (1: a new sheet)
    pub next_label: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub updated_at: String,
}

pub async fn sheet_use(db: &SqlitePool) -> Result<Vec<SheetUse>> {
    Ok(sqlx::query_as::<_, SheetUse>(
        "SELECT sheet, next_label, run_id, updated_at FROM label_sheet_use ORDER BY sheet",
    )
    .fetch_all(db)
    .await?)
}

pub async fn next_label(db: &SqlitePool, sheet: &str) -> Result<Option<i64>> {
    Ok(
        sqlx::query_as::<_, (i64,)>("SELECT next_label FROM label_sheet_use WHERE sheet = ?")
            .bind(sheet)
            .fetch_optional(db)
            .await?
            .map(|r| r.0),
    )
}

pub async fn set_next(db: &SqlitePool, sheet: &str, next: i64, run: Option<&str>) -> Result<()> {
    sqlx::query(
        "INSERT INTO label_sheet_use (sheet, next_label, run_id, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (sheet) DO UPDATE SET next_label = excluded.next_label,
         run_id = excluded.run_id, updated_at = excluded.updated_at",
    )
    .bind(sheet)
    .bind(next)
    .bind(run)
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

/// After `labels` from `start` on a sheet of `per`: the next free label (1 when the last
/// sheet was used up)
pub fn after(start: usize, labels: usize, per: usize) -> usize {
    (start - 1 + labels) % per + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_free_label_follows_a_run_and_wraps_to_a_new_sheet() {
        assert_eq!(after(1, 3, 10), 4);
        assert_eq!(after(4, 7, 10), 1, "the sheet is used up: a new one");
        assert_eq!(after(9, 5, 10), 4, "onto a second sheet");
        assert_eq!(after(1, 10, 10), 1);
    }
}
