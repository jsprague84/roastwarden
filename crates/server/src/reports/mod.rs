//! Reports made with Typst (`docs/reports.md`): PDF documents the server
//! makes from its own data with the label engine's sandboxed worker (`labels/engine.rs`,
//! [`engine::Kind::Report`]).
//!
//! Each kind gathers its data here, does the arithmetic and formats every number (Typst only
//! lays out), and hands a design (`assets/reports/<kind>.typ`) one JSON value, the same one
//! `format=json` answers: `{schema, meta, …}`. Charts are drawn by the design from prepared
//! series (`chart.rs`); the generation time comes in `meta`, as the worker has no clock.

pub mod chart;
#[cfg(test)]
pub mod check;
pub mod coffee;
pub mod compare;
pub mod cupping;
pub mod expect;
pub mod inventory;
pub mod period;
pub mod plan;
pub mod profile;
pub mod purchases;
pub mod roast;
pub mod shopping;
pub mod trace;

use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::labels::engine::{self, Diag, Reply, Request};
use crate::labels::Renderer;

/// A kind of report and its built-in design.
#[derive(Debug, Clone, Copy)]
pub struct ReportKind {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The schema of its data: `roastwarden.report.<kind>/<version>`
    pub schema: &'static str,
    pub design: &'static str,
}

pub const KINDS: &[ReportKind] = &[
    ReportKind {
        id: "roast",
        title: "Roast report",
        description: "One roast: the coffee, its numbers against what its level, profile and earlier roasts lead to expect, the curve with phases and events, heater and fan, then notes and cupping. One page unless it has more to say.",
        schema: "roastwarden.report.roast/1",
        design: include_str!("../../assets/reports/roast.typ"),
    },
    ReportKind {
        id: "compare",
        title: "Compare two roasts",
        description: "Two roasts as an A/B experiment: BT and RoR of both on one chart with heater and fan, the phases on one time scale, every number side by side with B − A, and how closely B's BT followed A's.",
        schema: "roastwarden.report.compare/1",
        design: include_str!("../../assets/reports/compare.typ"),
    },
    ReportKind {
        id: "cupping",
        title: "Cupping sheet",
        description: "A blank form to score on after the classic SCA protocol (two samples a page with scales to mark, or six with boxes), blind codes with the key on its own page; or a session's scores as a table with a dot strip per attribute.",
        schema: "roastwarden.report.cupping/1",
        design: include_str!("../../assets/reports/cupping.typ"),
    },
    ReportKind {
        id: "inventory",
        title: "Green inventory",
        description: "The green coffee in stock: kg, planned roasts, cost per kg and value, age since bought, the recent pace and when each coffee runs out; or a stock-take sheet to weigh against.",
        schema: "roastwarden.report.inventory/1",
        design: include_str!("../../assets/reports/inventory.typ"),
    },
    ReportKind {
        id: "shopping",
        title: "Shopping list",
        description: "The shopping list as one short order per supplier, prices as seen and the shipping that supplier usually adds; then the coffees running low.",
        schema: "roastwarden.report.shopping/1",
        design: include_str!("../../assets/reports/shopping.typ"),
    },
    ReportKind {
        id: "coffee",
        title: "Coffee report",
        description: "One green coffee: what is known of it, its stock and cost, its roasts as a table and overlaid on one chart with their RoR, its cuppings, and its ledger with a running balance.",
        schema: "roastwarden.report.coffee/2",
        design: include_str!("../../assets/reports/coffee.typ"),
    },
    ReportKind {
        id: "profile",
        title: "Profile report",
        description: "A profile's targets, its roasts in a ranking table with an average row, their BT overlaid on the profile's, and how first crack, drop, DTR and loss spread.",
        schema: "roastwarden.report.profile/1",
        design: include_str!("../../assets/reports/profile.typ"),
    },
    ReportKind {
        id: "period",
        title: "Period report",
        description: "A month, a range or a year in review: the roasts with a sum row, green roasted and bought, spend and landed cost per kg, each coffee's part and the stock at the end; a year by month.",
        schema: "roastwarden.report.period/1",
        design: include_str!("../../assets/reports/period.typ"),
    },
    ReportKind {
        id: "purchases",
        title: "Purchases",
        description: "Green coffee bought over a range: each order with its coffees, shipping, fees and landed cost per kg; each supplier's share; landed cost over time.",
        schema: "roastwarden.report.purchases/1",
        design: include_str!("../../assets/reports/purchases.typ"),
    },
    ReportKind {
        id: "plan",
        title: "Roast-day plan",
        description: "The planned roasts in order: the green to weigh and the output to expect, the profile's targets, boxes to write charge, first crack, drop and the roasted weight in; the green each coffee needs against stock.",
        schema: "roastwarden.report.plan/1",
        design: include_str!("../../assets/reports/plan.typ"),
    },
    ReportKind {
        id: "trace",
        title: "Traceability",
        description: "From the green coffee to every bag, or a bag's roast back to its green lot: the coffee, its supplier, lot and purchases; each roast with its batch, roaster, profile, weights and loss; and where it went: packed bags (whole or ground), label runs, gifts, use, discards and what is left.",
        schema: "roastwarden.report.trace/1",
        design: include_str!("../../assets/reports/trace.typ"),
    },
];

pub fn kind(id: &str) -> Option<&'static ReportKind> {
    KINDS.iter().find(|k| k.id == id)
}

/// What to make of a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// The document (the default)
    #[default]
    Pdf,
    /// Every page in one SVG, one under the other: a preview
    Svg,
    /// One page as PNG (`page`, `dpi`): a cheap look for an agent
    Png,
    /// The data the design gets, as JSON
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    Letter,
    A4,
}

impl Paper {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "letter" | "us-letter" | "us_letter" => Some(Paper::Letter),
            "a4" => Some(Paper::A4),
            _ => None,
        }
    }
    /// Typst's name for it
    pub fn typst(self) -> &'static str {
        match self {
            Paper::Letter => "us-letter",
            Paper::A4 => "a4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    C,
    F,
}

impl Units {
    pub fn parse(s: &str) -> Option<Self> {
        match s
            .trim()
            .trim_start_matches('°')
            .to_ascii_uppercase()
            .as_str()
        {
            "C" | "CELSIUS" => Some(Units::C),
            "F" | "FAHRENHEIT" => Some(Units::F),
            _ => None,
        }
    }
    /// A temperature (°C stored) in these units
    pub fn temp(self, c: f64) -> f64 {
        match self {
            Units::C => c,
            Units::F => c * 9.0 / 5.0 + 32.0,
        }
    }
    /// A difference or a rate (°C stored) in these units
    pub fn delta(self, c: f64) -> f64 {
        match self {
            Units::C => c,
            Units::F => c * 9.0 / 5.0,
        }
    }
    pub fn temp_label(self) -> &'static str {
        match self {
            Units::C => "°C",
            Units::F => "°F",
        }
    }
    pub fn rate_label(self) -> &'static str {
        match self {
            Units::C => "°C/min",
            Units::F => "°F/min",
        }
    }
}

/// How one report is made: paper, units, the reader's time zone and the time it is made.
#[derive(Debug, Clone)]
pub struct Options {
    pub paper: Paper,
    pub units: Units,
    /// The reader's zone, for dates and times: the request's `tz`, else the `time_zone`
    /// setting, else UTC
    pub zone: crate::ops::zone::Zone,
    pub now: DateTime<Utc>,
    /// The home currency (Settings → Roasting)
    pub currency: String,
}

impl Options {
    /// The settings (`report_paper`, `report_units`, `currency`, `time_zone`), then what the
    /// request asks.
    pub async fn from_settings(
        db: &SqlitePool,
        paper: Option<&str>,
        units: Option<&str>,
        tz: Option<i32>,
    ) -> Result<Self, ReportError> {
        let setting = |k: &'static str| async move { crate::auth::settings::get(db, k).await };
        let paper = match paper {
            Some(p) => Paper::parse(p)
                .ok_or_else(|| ReportError::Bad(format!("paper '{p}': letter or a4")))?,
            None => setting("report_paper")
                .await
                .and_then(|p| Paper::parse(&p))
                .unwrap_or(Paper::Letter),
        };
        let units = match units {
            Some(u) => {
                Units::parse(u).ok_or_else(|| ReportError::Bad(format!("units '{u}': C or F")))?
            }
            None => setting("report_units")
                .await
                .and_then(|u| Units::parse(&u))
                .unwrap_or(Units::C),
        };
        let currency = setting("currency")
            .await
            .map(|c| c.trim().to_ascii_uppercase())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "USD".into());
        Ok(Self {
            paper,
            units,
            zone: crate::ops::zone::resolve(db, tz).await,
            now: Utc::now(),
            currency,
        })
    }

    pub fn local(&self, t: DateTime<Utc>) -> DateTime<FixedOffset> {
        self.zone.local(t)
    }

    pub fn temp(&self, c: Option<f64>, digits: usize) -> String {
        c.filter(|v| v.is_finite()).map_or(DASH.into(), |c| {
            format!("{:.*}", digits, self.units.temp(c))
        })
    }

    pub fn temp_u(&self, c: Option<f64>, digits: usize) -> String {
        match c.filter(|v| v.is_finite()) {
            Some(_) => format!("{} {}", self.temp(c, digits), self.units.temp_label()),
            None => DASH.into(),
        }
    }

    pub fn delta(&self, c: Option<f64>, digits: usize) -> String {
        c.filter(|v| v.is_finite()).map_or(DASH.into(), |c| {
            format!("{:.*}", digits, self.units.delta(c))
        })
    }
}

/// A missing value: an en dash, so nothing shifts
pub const DASH: &str = "–";

/// `$25.09`, `€3.10`, `£2.00`, else `CHF 4.20`
pub fn money(currency: &str, v: f64) -> String {
    let sign = if v < 0.0 { "−" } else { "" };
    let a = v.abs();
    match currency {
        "USD" | "CAD" | "AUD" | "NZD" => format!("{sign}${a:.2}"),
        "EUR" => format!("{sign}€{a:.2}"),
        "GBP" => format!("{sign}£{a:.2}"),
        "JPY" => format!("{sign}¥{a:.0}"),
        c => format!("{sign}{c} {a:.2}"),
    }
}

/// Seconds as minutes and seconds: `9:51`, `−0:15`
pub fn clock(s: f64) -> String {
    if !s.is_finite() {
        return DASH.into();
    }
    let sign = if s < -0.5 { "−" } else { "" };
    let a = s.abs().round() as i64;
    format!("{sign}{}:{:02}", a / 60, a % 60)
}

pub fn clock_opt(s: Option<f64>) -> String {
    s.map_or(DASH.into(), clock)
}

/// One decimal, or a dash
pub fn n1(v: Option<f64>) -> String {
    v.filter(|v| v.is_finite())
        .map_or(DASH.into(), |v| format!("{v:.1}"))
}

/// Grams as typed: `100`, `85.75` (in a sentence or a figure; a table takes [`g1`])
pub fn grams(g: f64) -> String {
    let s = format!("{g:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Grams in a table column: always one decimal, so the column lines up (`100.0`, `85.8`)
pub fn g1(g: f64) -> String {
    format!("{g:.1}")
}

/// A weight to read: grams below a kilogram (`54 g`), else kilograms (`1.30 kg`)
pub fn weight(g: f64) -> String {
    if g.abs() < 999.5 {
        format!("{g:.0} g")
    } else {
        format!("{:.2} kg", g / 1000.0)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// `Sunday 25 October 2026`
pub fn long_date(t: &DateTime<FixedOffset>) -> String {
    format!(
        "{} {} {} {}",
        DAYS[t.weekday().num_days_from_monday() as usize],
        t.day(),
        MONTHS[t.month0() as usize],
        t.year()
    )
}

/// `25 Oct 2026`
pub fn short_date(t: &DateTime<FixedOffset>) -> String {
    format!(
        "{} {} {}",
        t.day(),
        &MONTHS[t.month0() as usize][..3],
        t.year()
    )
}

/// `25 Oct`
pub fn day_month(t: &DateTime<FixedOffset>) -> String {
    format!("{} {}", t.day(), &MONTHS[t.month0() as usize][..3])
}

/// A day ahead: `7 Nov` within about ten months of `now`, else `May 2028`
pub fn ahead(t: &DateTime<FixedOffset>, now: &DateTime<FixedOffset>) -> String {
    if (*t - *now).num_days() < 300 {
        day_month(t)
    } else {
        format!("{} {}", &MONTHS[t.month0() as usize][..3], t.year())
    }
}

/// A day gone by: `24 Oct` within about ten months of `now`, else `31 Dec 2024`
pub fn past(t: &DateTime<FixedOffset>, now: &DateTime<FixedOffset>) -> String {
    if (*now - *t).num_days() < 300 {
        day_month(t)
    } else {
        short_date(t)
    }
}

/// `13:26`
pub fn hm(t: &DateTime<FixedOffset>) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// What every report's `meta` holds: what it is, its subject, when it was made and how.
pub fn meta(kind: &ReportKind, subject: &str, o: &Options) -> Value {
    let made = o.local(o.now);
    json!({
        "kind": kind.id,
        "title": kind.title,
        "subject": subject,
        "made": format!("{}, {}", short_date(&made), hm(&made)),
        "made_iso": [made.year(), made.month(), made.day(), made.hour(), made.minute(), made.second()],
        "version": env!("CARGO_PKG_VERSION"),
        "paper": o.paper.typst(),
        "temp": o.units.temp_label(),
        "rate": o.units.rate_label(),
        "currency": o.currency,
        "lang": "en",
    })
}

#[derive(Debug)]
pub enum ReportError {
    /// No such roast, coffee or profile
    NotFound(String),
    /// A bad parameter
    Bad(String),
    /// The design did not compile or lay out
    Design(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ReportError {
    fn from(e: anyhow::Error) -> Self {
        ReportError::Internal(e)
    }
}

impl From<sqlx::Error> for ReportError {
    fn from(e: sqlx::Error) -> Self {
        ReportError::Internal(e.into())
    }
}

/// A made report.
pub struct Made {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    /// Pages laid out (none for JSON)
    pub pages: Option<usize>,
}

/// The report's data in its design: the PDF, all pages as one SVG, or one page as PNG.
pub async fn render(
    r: &Renderer,
    kind: &ReportKind,
    data: Value,
    format: Format,
    page: usize,
    dpi: u32,
) -> Result<Made, ReportError> {
    let (output, content_type) = match format {
        Format::Json => {
            return Ok(Made {
                bytes: serde_json::to_vec(&data).map_err(|e| ReportError::Internal(e.into()))?,
                content_type: "application/json",
                pages: None,
            })
        }
        Format::Pdf => (engine::Output::Pdf, "application/pdf"),
        Format::Svg => (engine::Output::SvgPages, "image/svg+xml"),
        Format::Png => (
            engine::Output::PagePng {
                page: page.max(1),
                dpi,
            },
            "image/png",
        ),
    };
    let req = Request {
        kind: engine::Kind::Report,
        design: Some(kind.design.to_string()),
        more: vec![],
        data,
        files: crate::ops::brand::report_files(),
        fonts: vec![],
        output,
    };
    match r.render(req).await {
        Reply::Done { bytes, pages, .. } => Ok(Made {
            bytes: bytes.into_vec(),
            content_type,
            pages: Some(pages),
        }),
        Reply::Failed { errors, .. } => Err(ReportError::Design(lines(&errors))),
        Reply::Bad(m) => Err(ReportError::Bad(m)),
    }
}

fn lines(errors: &[Diag]) -> String {
    errors
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(clock(605.4), "10:05");
        assert_eq!(clock(-15.0), "−0:15");
        assert_eq!(grams(85.75), "85.75");
        assert_eq!(grams(100.0), "100");
        assert_eq!(g1(100.0), "100.0");
        assert_eq!(g1(88.96), "89.0");
        assert_eq!(weight(53.6), "54 g");
        assert_eq!(weight(0.0), "0 g");
        assert_eq!(weight(1295.25), "1.30 kg");
        assert_eq!(money("USD", 25.091), "$25.09");
        assert_eq!(money("CHF", 4.2), "CHF 4.20");
        assert_eq!(Units::F.temp(100.0), 212.0);
        assert_eq!(Units::F.delta(10.0), 18.0);
        assert_eq!(Paper::parse("Letter"), Some(Paper::Letter));
        assert_eq!(Units::parse("°f"), Some(Units::F));
        let t = DateTime::parse_from_rfc3339("2026-10-25T13:26:00-05:00").unwrap();
        assert_eq!(long_date(&t), "Sunday 25 October 2026");
        assert_eq!(short_date(&t), "25 Oct 2026");
        assert_eq!(hm(&t), "13:26");
        let later = DateTime::parse_from_rfc3339("2028-05-27T10:00:00-05:00").unwrap();
        assert_eq!(ahead(&later, &t), "May 2028");
        let soon = DateTime::parse_from_rfc3339("2026-11-07T10:00:00-05:00").unwrap();
        assert_eq!(ahead(&soon, &t), "7 Nov");
    }
}
