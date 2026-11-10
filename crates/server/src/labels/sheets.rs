//! The label stock: a sheet's page and where each label sits on it (Avery's or the owner's
//! own in the `label_sheets` setting), and the owner's print offsets.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// A stock of label sheets: the page and where each label sits on it (inches). From
/// gLabels' Avery US templates (docs/labels.md, "Stock").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Sheet {
    /// e.g. `5163`
    pub id: String,
    pub name: String,
    pub page_w: f64,
    pub page_h: f64,
    /// One label's width and height (a round label's diameter twice)
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub round: bool,
    pub cols: u32,
    pub rows: u32,
    /// The page's edges to the first label
    pub left: f64,
    pub top: f64,
    /// One label's edge to the next one's
    pub pitch_x: f64,
    pub pitch_y: f64,
}

impl Sheet {
    pub fn per_sheet(&self) -> usize {
        (self.cols * self.rows) as usize
    }

    pub(super) fn check(&self) -> Result<(), String> {
        let positive = [
            self.page_w,
            self.page_h,
            self.w,
            self.h,
            self.pitch_x,
            self.pitch_y,
        ];
        if positive.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || !self.left.is_finite()
            || !self.top.is_finite()
            || self.cols == 0
            || self.rows == 0
            || self.cols * self.rows > 200
        {
            return Err(format!(
                "sheet {}: sizes above 0, at most 200 labels",
                self.id
            ));
        }
        Ok(())
    }
}

pub(super) fn builtin_sheets() -> Vec<Sheet> {
    let s = |id: &str, name: &str, w, h, round, cols, rows, left, top, px, py| Sheet {
        id: id.into(),
        name: name.into(),
        page_w: 8.5,
        page_h: 11.0,
        w,
        h,
        round,
        cols,
        rows,
        left,
        top,
        pitch_x: px,
        pitch_y: py,
    };
    vec![
        s(
            "5163",
            "Avery 5163, 4 × 2 in, 10 a sheet",
            4.0,
            2.0,
            false,
            2,
            5,
            0.1625,
            0.5,
            4.1875,
            2.0,
        ),
        s(
            "6874",
            "Avery 6874, 3¾ × 3 in, 6 a sheet",
            3.75,
            3.0,
            false,
            2,
            3,
            0.375,
            0.625,
            4.0,
            3.375,
        ),
        s(
            "5294",
            "Avery 5294, 2½ in round, 12 a sheet",
            2.5,
            2.5,
            true,
            3,
            4,
            0.21,
            0.425,
            2.75,
            2.515,
        ),
    ]
}

/// The built-in sheets and the owner's (`label_sheets`, a JSON list; one with a built-in's
/// id replaces it).
pub async fn sheets(db: &SqlitePool) -> Vec<Sheet> {
    let mut all = builtin_sheets();
    if let Some(own) = crate::auth::settings::get(db, "label_sheets").await
        && let Ok(list) = serde_json::from_str::<Vec<Sheet>>(&own) {
            for s in list.into_iter().filter(|s| s.check().is_ok()) {
                all.retain(|b| b.id != s.id);
                all.push(s);
            }
        }
    all
}

/// Check the `label_sheets` setting's value (settings.rs).
pub fn check_sheets(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let list: Vec<Sheet> = serde_json::from_str(value)
        .map_err(|e| format!("label_sheets is a JSON list of sheets: {e}"))?;
    list.iter().try_for_each(Sheet::check)
}

/// Where the owner nudges a sheet so the print lands on the labels, mm (right and down).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Offset {
    #[serde(default)]
    pub x_mm: f64,
    #[serde(default)]
    pub y_mm: f64,
}

pub async fn offsets(db: &SqlitePool) -> HashMap<String, Offset> {
    crate::auth::settings::get(db, "label_offsets")
        .await
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

pub fn check_offsets(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    let map: HashMap<String, Offset> = serde_json::from_str(value).map_err(|e| {
        format!("label_offsets is {{\"5163\": {{\"x_mm\": 0.5, \"y_mm\": -1}}}}: {e}")
    })?;
    if map
        .values()
        .any(|o| !(o.x_mm.abs() <= 20.0 && o.y_mm.abs() <= 20.0))
    {
        return Err("a label offset is at most 20 mm either way".into());
    }
    Ok(())
}
