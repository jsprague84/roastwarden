//! Label printers (docs/labels.md, "Printers"): more than one, each
//! with what it said it can do when last asked, the tray and colour to ask for, how to send
//! (PDF, or raster for a printer without PDF), the certificate the owner trusted for `ipps`,
//! and its own calibration per sheet. The `label_printer_url` setting is read only while no
//! printer is set up here (the migration moved it in).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::Offset;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// How a printer is sent labels
pub const SENDS: [&str; 4] = ["auto", "pdf", "pwg", "urf"];
/// The colour a printer is asked for
pub const COLORS: [&str; 2] = ["monochrome", "color"];

/// A label printer
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Printer {
    /// Letters, digits, `-` and `_`
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// `ipp://host:631/ipp/print`, or `ipps://…` with a trusted certificate
    pub url: String,
    /// The one labels go to unless another is named
    #[serde(default)]
    pub is_default: bool,
    /// The tray to ask for (`media-source`, e.g. `tray-1`); none: the tray that holds paper of
    /// the sheet's size
    pub tray: Option<String>,
    /// `monochrome` or `color`; none: monochrome unless a label has colour on it and the
    /// printer prints colour
    pub color: Option<String>,
    /// `auto` (PDF, else PWG Raster, else URF), `pdf`, `pwg` or `urf`
    #[serde(default = "auto")]
    pub send: String,
    /// For `ipps`: the SHA-256 of the certificate the owner trusted (hex)
    pub fingerprint: Option<String>,
    /// What the printer answered when last asked (Get-Printer-Attributes)
    #[sqlx(json(nullable))]
    #[schema(value_type = Option<Object>)]
    pub info: Option<Value>,
    pub info_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn auto() -> String {
    "auto".into()
}

impl Printer {
    /// What is wrong with it, if anything
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a printer has a name".into());
        }
        super::print::check_url(&self.url)?;
        if self.url.trim().is_empty() {
            return Err("a printer has an address: ipp://host/ipp/print".into());
        }
        if !SENDS.contains(&self.send.as_str()) {
            return Err(format!("send is one of {}", SENDS.join(", ")));
        }
        if let Some(c) = &self.color
            && !COLORS.contains(&c.as_str())
        {
            return Err(format!("color is {} (or none)", COLORS.join(" or ")));
        }
        if let Some(f) = &self.fingerprint {
            let hex = f.replace(':', "");
            if !(hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())) {
                return Err("fingerprint is a SHA-256 in hex (64 digits)".into());
            }
        }
        Ok(())
    }
}

/// A printer's offset for one sheet: where its labels land, measured
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow, PartialEq)]
pub struct Calibration {
    #[serde(default)]
    pub sheet: String,
    /// Right, mm
    pub x_mm: f64,
    /// Down, mm
    pub y_mm: f64,
    /// The tray it was measured from
    pub tray: Option<String>,
    #[serde(default)]
    pub updated_at: String,
}

const COLUMNS: &str = "id, name, url, is_default, tray, color, send, fingerprint, info, info_at, created_at, updated_at";

pub async fn list(db: &SqlitePool) -> Result<Vec<Printer>> {
    Ok(sqlx::query_as::<_, Printer>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM label_printers ORDER BY is_default DESC, name"
    )))
    .fetch_all(db)
    .await?)
}

pub async fn get(db: &SqlitePool, id: &str) -> Result<Option<Printer>> {
    Ok(sqlx::query_as::<_, Printer>(AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM label_printers WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// The default printer, else the only one
pub async fn default(db: &SqlitePool) -> Result<Option<Printer>> {
    let all = list(db).await?;
    Ok(all
        .iter()
        .find(|p| p.is_default)
        .cloned()
        .or_else(|| (all.len() == 1).then(|| all[0].clone())))
}

/// Add or replace a printer (its info and calibrations stay). True when it is new.
pub async fn put(db: &SqlitePool, p: &Printer) -> Result<bool> {
    // It reads whether it exists and the count, then writes: the write lock first (db::begin_write).
    let mut tx = crate::db::begin_write(db).await?;
    let at = now();
    let existed = sqlx::query_as::<_, (String,)>("SELECT id FROM label_printers WHERE id = ?")
        .bind(&p.id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
    let first = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM label_printers")
        .fetch_one(&mut *tx)
        .await?
        .0
        == 0;
    let is_default = p.is_default || first;
    if is_default {
        sqlx::query("UPDATE label_printers SET is_default = 0")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO label_printers (id, name, url, is_default, tray, color, send, fingerprint,
         created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, url = excluded.url,
         is_default = excluded.is_default OR (label_printers.is_default AND NOT ?),
         tray = excluded.tray, color = excluded.color, send = excluded.send,
         fingerprint = excluded.fingerprint, updated_at = excluded.updated_at",
    )
    .bind(&p.id)
    .bind(p.name.trim())
    .bind(p.url.trim())
    .bind(is_default)
    .bind(p.tray.as_deref().filter(|t| !t.trim().is_empty()))
    .bind(p.color.as_deref().filter(|c| !c.trim().is_empty()))
    .bind(&p.send)
    .bind(
        p.fingerprint
            .as_deref()
            .map(|f| f.replace(':', "").to_ascii_lowercase()),
    )
    .bind(&at)
    .bind(&at)
    .bind(is_default)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(!existed)
}

/// The labels go here unless another is named
pub async fn make_default(db: &SqlitePool, id: &str) -> Result<bool> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE label_printers SET is_default = 0")
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query("UPDATE label_printers SET is_default = 1 WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(n > 0)
}

pub async fn delete(db: &SqlitePool, id: &str) -> Result<bool> {
    let n = sqlx::query("DELETE FROM label_printers WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    // Another becomes the default when the default went
    sqlx::query(
        "UPDATE label_printers SET is_default = 1 WHERE id = (SELECT id FROM label_printers ORDER BY name LIMIT 1)
         AND NOT EXISTS (SELECT 1 FROM label_printers WHERE is_default = 1)",
    )
    .execute(db)
    .await?;
    Ok(n > 0)
}

/// What the printer answered when asked, kept with the time
pub async fn set_info(db: &SqlitePool, id: &str, info: &Value) -> Result<()> {
    sqlx::query("UPDATE label_printers SET info = ?, info_at = ? WHERE id = ?")
        .bind(info.to_string())
        .bind(now())
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn calibrations(db: &SqlitePool, printer: &str) -> Result<Vec<Calibration>> {
    Ok(sqlx::query_as::<_, Calibration>(
        "SELECT sheet, x_mm, y_mm, tray, updated_at FROM label_calibrations WHERE printer_id = ? ORDER BY sheet",
    )
    .bind(printer)
    .fetch_all(db)
    .await?)
}

pub async fn set_calibration(db: &SqlitePool, printer: &str, c: &Calibration) -> Result<()> {
    sqlx::query(
        "INSERT INTO label_calibrations (printer_id, sheet, x_mm, y_mm, tray, updated_at)
         VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (printer_id, sheet) DO UPDATE SET
         x_mm = excluded.x_mm, y_mm = excluded.y_mm, tray = excluded.tray,
         updated_at = excluded.updated_at",
    )
    .bind(printer)
    .bind(&c.sheet)
    .bind(c.x_mm)
    .bind(c.y_mm)
    .bind(c.tray.as_deref())
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_calibration(db: &SqlitePool, printer: &str, sheet: &str) -> Result<bool> {
    Ok(
        sqlx::query("DELETE FROM label_calibrations WHERE printer_id = ? AND sheet = ?")
            .bind(printer)
            .bind(sheet)
            .execute(db)
            .await?
            .rows_affected()
            > 0,
    )
}

/// Where labels land on `printer` (else the default printer) for `sheet`: its calibration,
/// else the sheet's offset, else none
pub async fn offset_for(
    db: &SqlitePool,
    printer: Option<&str>,
    sheet: &str,
    sheet_offset: Offset,
) -> Offset {
    let id = match printer {
        Some(p) => Some(p.to_string()),
        None => default(db).await.ok().flatten().map(|p| p.id),
    };
    let Some(id) = id else {
        return sheet_offset;
    };
    match sqlx::query_as::<_, (f64, f64)>(
        "SELECT x_mm, y_mm FROM label_calibrations WHERE printer_id = ? AND sheet = ?",
    )
    .bind(&id)
    .bind(sheet)
    .fetch_optional(db)
    .await
    {
        Ok(Some((x, y))) => Offset { x_mm: x, y_mm: y },
        _ => sheet_offset,
    }
}

/// Two corners measured on a calibration page: where labels land (their middle), or why it
/// cannot be one offset (the corners disagree by more than 0.5 mm: the printer scales the
/// page or the feed skews)
pub fn from_corners(first: (f64, f64), last: (f64, f64)) -> Result<(f64, f64), String> {
    let (dx, dy) = ((first.0 - last.0).abs(), (first.1 - last.1).abs());
    if dx > 0.5 || dy > 0.5 {
        return Err(format!(
            "the first and last labels are {dx:.1} mm apart across and {dy:.1} mm down: the \
             printer is scaling the page (print at Actual size, 100 %) or the sheet feeds \
             askew; one offset cannot fix that"
        ));
    }
    let r = |v: f64| (v * 10.0).round() / 10.0;
    Ok((
        r(f64::midpoint(first.0, last.0)),
        r(f64::midpoint(first.1, last.1)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_corners_make_one_offset_or_say_the_page_scales() {
        assert_eq!(from_corners((0.6, -0.2), (0.8, 0.0)), Ok((0.7, -0.1)));
        let e = from_corners((0.5, 0.0), (1.6, 0.1)).unwrap_err();
        assert!(e.contains("scaling"), "{e}");
    }

    #[tokio::test]
    async fn a_print_uses_its_printers_calibration_else_the_sheets() {
        let path = std::env::temp_dir().join(format!("rr-printers-{}.db", uuid::Uuid::new_v4()));
        let db = crate::db::open(path.to_str().unwrap()).await.unwrap();
        let sheet = Offset {
            x_mm: 0.3,
            y_mm: -0.2,
        };
        // No printer: the sheet's own offset
        assert_eq!(offset_for(&db, None, "5163", sheet).await, sheet);
        let mut p = Printer {
            id: "laser".into(),
            name: "Laser".into(),
            url: "ipp://10.0.0.9/ipp/print".into(),
            is_default: false,
            tray: None,
            color: None,
            send: "auto".into(),
            fingerprint: None,
            info: None,
            info_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(put(&db, &p).await.unwrap(), "new");
        p.id = "inkjet".into();
        put(&db, &p).await.unwrap();
        assert_eq!(
            default(&db).await.unwrap().unwrap().id,
            "laser",
            "the first is the default"
        );
        let cal = |x, y| Calibration {
            sheet: "5163".into(),
            x_mm: x,
            y_mm: y,
            tray: None,
            updated_at: String::new(),
        };
        set_calibration(&db, "laser", &cal(1.1, 0.5)).await.unwrap();
        let laser = Offset {
            x_mm: 1.1,
            y_mm: 0.5,
        };
        assert_eq!(
            offset_for(&db, None, "5163", sheet).await,
            laser,
            "the default printer's"
        );
        assert_eq!(offset_for(&db, Some("laser"), "5163", sheet).await, laser);
        assert_eq!(
            offset_for(&db, Some("inkjet"), "5163", sheet).await,
            sheet,
            "none of its own"
        );
        assert_eq!(
            offset_for(&db, Some("laser"), "6874", sheet).await,
            sheet,
            "another sheet"
        );
        assert!(make_default(&db, "inkjet").await.unwrap());
        assert_eq!(offset_for(&db, None, "5163", sheet).await, sheet);
        // The default goes: another becomes it
        assert!(delete(&db, "inkjet").await.unwrap());
        assert_eq!(default(&db).await.unwrap().unwrap().id, "laser");
        assert_eq!(
            calibrations(&db, "inkjet").await.unwrap(),
            [] as [crate::labels::printers::Calibration; 0]
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_printer_is_checked() {
        let mut p = Printer {
            id: "hp".into(),
            name: "HP".into(),
            url: "ipp://192.168.1.143:631/ipp/print".into(),
            is_default: false,
            tray: None,
            color: None,
            send: "auto".into(),
            fingerprint: None,
            info: None,
            info_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert_eq!(p.check(), Ok(()));
        p.send = "fax".into();
        assert!(p.check().is_err());
        p.send = "urf".into();
        p.color = Some("sepia".into());
        assert!(p.check().is_err());
        p.color = Some("color".into());
        p.fingerprint = Some("ab:cd".into());
        assert!(p.check().is_err());
        p.fingerprint = Some("a".repeat(64));
        assert_eq!(p.check(), Ok(()));
        p.url = "lpd://x/q".into();
        assert!(p.check().is_err());
    }
}
