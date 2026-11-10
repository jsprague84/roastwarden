//! The share page (docs/labels.md, "The share page"): what a bag's
//! QR code opens. A public page per roast at `/R/<code>`, made by the server (no dashboard,
//! no API, no roast id, cost, stock or street address): the coffee, its origin and process,
//! the roast and when it is at its best, the notes, a gift line, a brew suggestion and the
//! curve as a picture. The code is 25 base32 characters (125 random bits), in capitals in the
//! QR code so it encodes in QR's smaller alphanumeric mode; a code can be revoked, and an
//! unknown or revoked one is the same 404. Scans are counted, without addresses.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use std::fmt::Write as _;
use utoipa::ToSchema;

use crate::roasts::types::{RoastEvent, RoastEventType, SessionTelemetry};

/// RFC 4648 base32: A–Z and 2–7, all in QR's alphanumeric set
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// The code the sample label's QR code shows (a preview with no roast)
pub const SAMPLE_CODE: &str = "SAMPLESHARECODE2345ABCDEF";

/// A new code: 25 characters, 125 bits from the OS's random source
pub fn new_code() -> String {
    // Two v4 UUIDs: 244 random bits from getrandom
    let mut bits: Vec<u8> = Vec::with_capacity(32);
    bits.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bits.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    // Skip each UUID's version and variant bytes: take the wholly random bytes only
    let random: Vec<u8> = bits
        .iter()
        .enumerate()
        .filter(|(i, _)| !matches!(i % 16, 6 | 8))
        .map(|(_, b)| *b)
        .collect();
    let mut out = String::with_capacity(25);
    let mut acc: u32 = 0;
    let mut n = 0;
    for b in random {
        acc = (acc << 8) | u32::from(b);
        n += 8;
        while n >= 5 && out.len() < 25 {
            n -= 5;
            out.push(ALPHABET[((acc >> n) & 31) as usize] as char);
        }
        if out.len() == 25 {
            break;
        }
    }
    out
}

/// A code as typed or scanned: either case, no separators
pub fn normal(code: &str) -> Option<String> {
    let c: String = code.trim().to_ascii_uppercase();
    (c.len() == 25 && c.bytes().all(|b| ALPHABET.contains(&b))).then_some(c)
}

/// The page's address, to read or send: `https://host/R/CODE`
pub fn url(base: &str, code: &str) -> String {
    format!("{}/R/{code}", base.trim_end_matches('/'))
}

/// What the QR code encodes: the address in capitals when that is still the same address
/// (scheme and host are not case-sensitive, `/R/` is routed in both cases), so the code is
/// in QR's alphanumeric mode: fewer modules, a QR code that scans from further away. A base
/// with a path of its own keeps its case.
pub fn qr_text(base: &str, code: &str) -> String {
    let full = url(base, code);
    let after_scheme = base.split_once("://").map_or(base, |(_, rest)| rest);
    if after_scheme.trim_end_matches('/').contains('/') {
        full
    } else {
        full.to_ascii_uppercase()
    }
}

/// A share link
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct ShareLink {
    pub code: String,
    /// The roast it shows
    pub session_id: String,
    /// The coffee: the roast's bean, else the roast's name
    pub name: String,
    pub batch: Option<i64>,
    /// A line for the person it is for, shown on the page
    pub message: Option<String>,
    /// The owner's offset from UTC (minutes east) when it was made: the page's dates (the
    /// request's `tz`, else the `time_zone` setting's offset then, else 0)
    pub tz_minutes: i64,
    pub created_at: String,
    pub created_by: String,
    /// Set once revoked: the page is then a 404 like an unknown code
    pub revoked_at: Option<String>,
    /// How often the page was opened
    pub scans: i64,
    pub last_scan_at: Option<String>,
    /// The page's address
    #[sqlx(skip)]
    #[serde(default)]
    pub url: String,
}

const SELECT: &str = "SELECT l.code, l.session_id, COALESCE(b.name, s.name) AS name,
        s.batch_number AS batch, l.message, l.tz_minutes, l.created_at, l.created_by,
        l.revoked_at, l.scans, l.last_scan_at
    FROM share_links l
    JOIN roast_sessions s ON s.id = l.session_id
    LEFT JOIN beans b ON b.id = s.bean_id";

/// The links, newest first: a roast's or every roast's, the revoked ones too with `all`
pub async fn list(db: &SqlitePool, session: Option<&str>, all: bool) -> Result<Vec<ShareLink>> {
    let sql = format!(
        "{SELECT} WHERE (?1 IS NULL OR l.session_id = ?1) AND (?2 OR l.revoked_at IS NULL)
         ORDER BY l.created_at DESC, l.rowid DESC"
    );
    Ok(sqlx::query_as::<_, ShareLink>(AssertSqlSafe(sql.as_str()))
        .bind(session)
        .bind(all)
        .fetch_all(db)
        .await?)
}

pub async fn get(db: &SqlitePool, code: &str) -> Result<Option<ShareLink>> {
    let sql = format!("{SELECT} WHERE l.code = ?");
    Ok(sqlx::query_as::<_, ShareLink>(AssertSqlSafe(sql.as_str()))
        .bind(code)
        .fetch_optional(db)
        .await?)
}

/// The roast's newest link that is not revoked: the one its labels' QR codes open
pub async fn active(db: &SqlitePool, session: &str) -> Result<Option<ShareLink>> {
    Ok(list(db, Some(session), false).await?.into_iter().next())
}

fn now_text() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// A message as stored: trimmed, empty is none, at most 280 characters
pub fn check_message(message: Option<&str>) -> Result<Option<String>, String> {
    let m = message.map(str::trim).filter(|m| !m.is_empty());
    match m {
        Some(m) if m.chars().count() > 280 => Err("a message is at most 280 characters".into()),
        Some(m) => Ok(Some(m.to_string())),
        None => Ok(None),
    }
}

/// A new link for a roast (the caller has checked the roast exists). Its dates use `tz`
/// (minutes east of UTC), else the offset the `time_zone` setting has now, else UTC.
pub async fn create(
    db: &SqlitePool,
    session: &str,
    message: Option<&str>,
    tz: Option<i32>,
    by: &str,
) -> Result<ShareLink> {
    let tz_minutes = crate::ops::zone::resolve(db, tz)
        .await
        .minutes_at(Utc::now());
    let code = new_code();
    sqlx::query(
        "INSERT INTO share_links (code, session_id, message, tz_minutes, created_at, created_by)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&code)
    .bind(session)
    .bind(message)
    .bind(tz_minutes.clamp(-840, 840))
    .bind(now_text())
    .bind(by)
    .execute(db)
    .await?;
    get(db, &code)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the share link vanished"))
}

/// Change a link's message (none clears it); false when there is no such link
pub async fn set_message(db: &SqlitePool, code: &str, message: Option<&str>) -> Result<bool> {
    Ok(
        sqlx::query("UPDATE share_links SET message = ? WHERE code = ?")
            .bind(message)
            .bind(code)
            .execute(db)
            .await?
            .rows_affected()
            > 0,
    )
}

/// Revoke a link (again is fine); false when there is no such link
pub async fn revoke(db: &SqlitePool, code: &str) -> Result<bool> {
    sqlx::query("UPDATE share_links SET revoked_at = ? WHERE code = ? AND revoked_at IS NULL")
        .bind(now_text())
        .bind(code)
        .execute(db)
        .await?;
    Ok(get(db, code).await?.is_some())
}

/// The link a page is opened by, counted; none when unknown or revoked
pub async fn scanned(db: &SqlitePool, code: &str) -> Result<Option<ShareLink>> {
    let n = sqlx::query(
        "UPDATE share_links SET scans = scans + 1, last_scan_at = ?
         WHERE code = ? AND revoked_at IS NULL",
    )
    .bind(now_text())
    .bind(code)
    .execute(db)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(None);
    }
    get(db, code).await
}

// ---- the page ----

/// The roast as a picture: bean temperature from charge, and where its stages began
#[derive(Debug, Clone, Default)]
pub struct Curve {
    /// (seconds from charge, °C)
    pub bt: Vec<(f32, f32)>,
    /// (what, seconds from charge, °C)
    pub events: Vec<(&'static str, f32, Option<f32>)>,
    /// Seconds from charge to first crack and to drop
    pub first_crack: Option<f32>,
    pub drop: Option<f32>,
    pub drop_temp: Option<f32>,
}

/// Seconds as minutes and seconds, `9:51`
fn clock(s: f32) -> String {
    let s = s.max(0.0).round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn f_of(c: f32) -> f32 {
    c * 9.0 / 5.0 + 32.0
}

/// The curve from the roast's samples and events: charge to drop, a point every two seconds
/// at most
pub fn curve(tele: &[SessionTelemetry], events: &[RoastEvent]) -> Option<Curve> {
    let at = |t: RoastEventType| {
        events
            .iter()
            .find(|e| e.event_type == t)
            .map(|e| (e.elapsed_seconds, e.temperature))
    };
    let charge = at(RoastEventType::Charge).map_or(0.0, |(t, _)| t);
    let drop = at(RoastEventType::Drop);
    let end = drop.map_or(f32::MAX, |(t, _)| t + 0.5);
    let mut bt: Vec<(f32, f32)> = Vec::new();
    for s in tele {
        let (Some(b), t) = (s.bean_temp, s.elapsed_seconds) else {
            continue;
        };
        if t < charge || t > end || !b.is_finite() {
            continue;
        }
        let rel = t - charge;
        if bt.last().is_some_and(|(p, _)| rel - p < 2.0) {
            continue;
        }
        bt.push((rel, b));
    }
    if bt.len() < 10 {
        return None;
    }
    let temp_at = |t: f32| {
        bt.iter()
            .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
            .map(|p| p.1)
    };
    let mut marks = Vec::new();
    for (kind, word) in [
        (RoastEventType::DryingEnd, "Dry end"),
        (RoastEventType::FirstCrackStart, "First crack"),
        (RoastEventType::SecondCrackStart, "Second crack"),
        (RoastEventType::Drop, "Drop"),
    ] {
        if let Some((t, temp)) = at(kind) {
            let rel = t - charge;
            marks.push((word, rel, temp.or_else(|| temp_at(rel))));
        }
    }
    let first_crack = at(RoastEventType::FirstCrackStart).map(|(t, _)| t - charge);
    let drop_rel = drop.map(|(t, _)| t - charge);
    Some(Curve {
        drop_temp: drop.and_then(|(t, temp)| temp.or_else(|| temp_at(t - charge))),
        bt,
        events: marks,
        first_crack,
        drop: drop_rel,
    })
}

/// The curve as an SVG on chart paper: °C on the left, °F on the right, minutes along the
/// bottom, development (first crack to drop) shaded
pub fn curve_svg(c: &Curve) -> String {
    // About the size it is shown at on a phone, so its text stays readable there
    let (w, h) = (420.0_f32, 236.0_f32);
    let (left, right, top, bottom) = (38.0_f32, 40.0_f32, 12.0_f32, 26.0_f32);
    let t_max = c.bt.last().map_or(60.0, |p| p.0).max(60.0);
    let minutes = (t_max / 60.0).ceil().max(1.0);
    let lo = c.bt.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let hi = c.bt.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let y_lo = ((lo - 5.0) / 50.0).floor() * 50.0;
    let y_hi = ((hi + 10.0) / 25.0).ceil() * 25.0;
    let x = |t: f32| left + (t / (minutes * 60.0)) * (w - left - right);
    let y = |v: f32| top + (1.0 - (v - y_lo) / (y_hi - y_lo).max(1.0)) * (h - top - bottom);
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg viewBox="0 0 {w} {h}" role="img" aria-label="The roast's bean temperature over time" xmlns="http://www.w3.org/2000/svg">"#
    );
    // Development: first crack to drop
    if let (Some(fc), Some(d)) = (c.first_crack, c.drop) {
        let _ = write!(
            s,
            r#"<rect class="dev" x="{:.1}" y="{top}" width="{:.1}" height="{:.1}"/>"#,
            x(fc),
            (x(d) - x(fc)).max(0.0),
            h - top - bottom
        );
    }
    // The paper: a line every 30 s, a darker one every minute; every 10 °C, darker every 50
    let mut half = 0.0;
    while half <= minutes * 60.0 + 0.1 {
        let major = (half % 60.0).abs() < 0.1;
        let _ = write!(
            s,
            r#"<line class="{}" x1="{:.1}" x2="{:.1}" y1="{top}" y2="{:.1}"/>"#,
            if major { "g2" } else { "g1" },
            x(half),
            x(half),
            h - bottom
        );
        if major {
            let _ = write!(
                s,
                r#"<text class="ax" x="{:.1}" y="{:.1}" text-anchor="middle">{}</text>"#,
                x(half),
                h - bottom + 15.0,
                (half / 60.0).round()
            );
        }
        half += 30.0;
    }
    let mut v = y_lo;
    while v <= y_hi + 0.1 {
        let major = (v % 50.0).abs() < 0.1;
        let _ = write!(
            s,
            r#"<line class="{}" x1="{left}" x2="{:.1}" y1="{:.1}" y2="{:.1}"/>"#,
            if major { "g2" } else { "g1" },
            w - right,
            y(v),
            y(v)
        );
        if major {
            let _ = write!(
                s,
                r#"<text class="ax" x="{:.1}" y="{:.1}" text-anchor="end">{v:.0}°C</text><text class="ax" x="{:.1}" y="{:.1}">{:.0}°F</text>"#,
                left - 6.0,
                y(v) + 4.0,
                w - right + 6.0,
                y(v) + 4.0,
                f_of(v)
            );
        }
        v += 10.0;
    }
    let _ = write!(
        s,
        r#"<text class="ax" x="{:.1}" y="{:.1}" text-anchor="end">min</text>"#,
        w - right,
        h - 2.0
    );
    // The bean temperature
    let mut d = String::new();
    for (i, (t, b)) in c.bt.iter().enumerate() {
        let _ = write!(
            d,
            "{}{:.1} {:.1}",
            if i == 0 { "M" } else { " L" },
            x(*t),
            y(*b)
        );
    }
    let _ = write!(s, r#"<path class="bt" d="{d}"/>"#);
    // Where each stage began: a dot and its name, alternately above and below the line
    for (i, (word, t, temp)) in c.events.iter().enumerate() {
        let Some(temp) = temp else { continue };
        let (cx, cy) = (x(*t), y(*temp));
        let below = i % 2 == 1 || cy < top + 30.0;
        let ty = if below { cy + 17.0 } else { cy - 9.0 };
        let anchor = if cx > w - right - 50.0 {
            "end"
        } else if cx < left + 40.0 {
            "start"
        } else {
            "middle"
        };
        let _ = write!(
            s,
            r#"<circle class="ev" cx="{cx:.1}" cy="{cy:.1}" r="3.5"/><text class="evt" x="{cx:.1}" y="{ty:.1}" text-anchor="{anchor}">{} {}</text>"#,
            esc(word),
            clock(*t)
        );
    }
    s.push_str("</svg>");
    s
}

/// Text made safe for HTML
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// The brand kit's parts the page shows: no street address, only the place
#[derive(Debug, Clone, Default)]
pub struct PageBrand {
    pub name: String,
    pub tagline: Option<String>,
    pub place: Option<String>,
    pub website: Option<String>,
    /// The logo as a `data:` URL (the page loads nothing from anywhere)
    pub logo: Option<String>,
    pub ink: Option<String>,
    pub accent: Option<String>,
    pub paper: Option<String>,
}

/// A blend's coffee as the page lists it: "55 % Senna Kata (Guji, Ethiopia), natural"
fn coffee_line(p: &super::BlendPart) -> String {
    let place = [p.region.as_deref(), p.origin.as_deref()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mut t = format!("{} % {}", p.share, p.name);
    if !place.is_empty() {
        let _ = write!(t, " ({place})");
    }
    for v in [&p.process, &p.variety].into_iter().flatten() {
        if !v.trim().is_empty() {
            let _ = write!(t, ", {v}");
        }
    }
    t
}

/// Everything the page shows, as text ready to show
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub name: String,
    pub origin: Option<String>,
    /// A blend's coffees, the largest share first: "55 % Senna Kata (Guji, Ethiopia), natural"
    pub coffees: Vec<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    /// "Light roast for espresso"
    pub roast: Option<String>,
    pub roasted: Option<String>,
    pub best_from: Option<String>,
    /// "Resting: at its best in 3 days" or "Ready: roasted 9 days ago"
    pub rest: Option<String>,
    /// "16", or "16+17" for roasts in one bag
    pub batch: Option<String>,
    pub notes: Option<String>,
    pub message: Option<String>,
    pub brew: Option<Brew>,
    pub curve: Option<Curve>,
    pub brand: Option<PageBrand>,
}

/// Where the roast is in its rest on the owner's `today`
pub fn rest_line(roasted: chrono::NaiveDate, rest_days: i64, today: chrono::NaiveDate) -> String {
    let age = (today - roasted).num_days();
    let ready = roasted + Duration::days(rest_days);
    let to_go = (ready - today).num_days();
    let days = |n: i64| {
        if n == 1 {
            "1 day".to_string()
        } else {
            format!("{n} days")
        }
    };
    if age < 0 {
        "Freshly roasted".into()
    } else if to_go > 0 {
        format!("Resting: at its best in {}", days(to_go))
    } else if age == 0 {
        "Ready: roasted today".into()
    } else {
        format!("Ready: roasted {} ago", days(age))
    }
}

/// How to brew a roast: the one recipe the share page and the labels (`l.brew`) both print
/// (docs/labels.md, "What goes on a label"). The owner's doses (Settings → Roasting), the
/// ratio, water and time from what it is for and how light it is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Brew {
    /// "Espresso" or "Filter"
    pub method: String,
    /// Coffee in, g
    pub dose_g: f64,
    /// Out: the shot (espresso) or the water (filter), g
    pub out_g: f64,
    /// "Yield" or "Water": what `out_g` is
    pub out_is: String,
    /// "1:2.5"
    pub ratio: String,
    /// Water, °C
    pub temp_c: u32,
    /// "27–30 s", "3:00"
    pub time: String,
    pub grind: String,
    /// The recipe in a sentence, with a tip
    pub line: String,
}

/// The recipe for what the roast is for (espresso or filter; other purposes have none), its
/// level, and the owner's doses
pub fn brew_for(
    purpose: Option<&str>,
    level: Option<&str>,
    espresso_g: f64,
    filter_g: f64,
) -> Option<Brew> {
    let level = level.unwrap_or_default().to_ascii_lowercase();
    // "light", "light-medium": light; "medium-dark", "dark": dark
    let light = level.starts_with("light");
    let dark = level.contains("dark");
    let ratio_text = |r: f64| {
        let t = format!("{r:.1}");
        format!("1:{}", t.trim_end_matches(".0"))
    };
    match purpose.map(str::to_ascii_lowercase).as_deref() {
        Some("espresso") => {
            // Lighter roasts extract less: a longer ratio and hotter water
            let (ratio, temp) = if light {
                (2.5, 94)
            } else if dark {
                (2.0, 91)
            } else {
                (2.2, 93)
            };
            let out = (espresso_g * ratio).round();
            Some(Brew {
                method: "Espresso".into(),
                dose_g: espresso_g,
                out_g: out,
                out_is: "Yield".into(),
                ratio: ratio_text(ratio),
                temp_c: temp,
                time: "27–30 s".into(),
                grind: "fine".into(),
                line: format!(
                    "{espresso_g:.0} g in, about {out:.0} g out ({}) in 27 to 30 seconds, water at {temp} °C. {}",
                    ratio_text(ratio),
                    if light {
                        "A light roast likes the longer shot: go longer before you grind finer."
                    } else {
                        "Taste, then grind finer if it is sour or coarser if it is bitter."
                    }
                ),
            })
        }
        Some("filter") => {
            let temp = if light {
                96
            } else if dark {
                90
            } else {
                94
            };
            let out = (filter_g * 16.0).round();
            Some(Brew {
                method: "Filter".into(),
                dose_g: filter_g,
                out_g: out,
                out_is: "Water".into(),
                ratio: "1:16".into(),
                temp_c: temp,
                time: "3:00".into(),
                grind: "medium-fine".into(),
                line: format!(
                    "{filter_g:.0} g of coffee to {out:.0} g of water (1:16) at {temp} °C, about 3 minutes. Taste, then grind finer for more sweetness or coarser if it turns bitter."
                ),
            })
        }
        _ => None,
    }
}

/// The page for a link: none when its roast is gone
pub async fn page(db: &SqlitePool, link: &ShareLink, now: DateTime<Utc>) -> Result<Option<Page>> {
    let Some(f) = super::facts(db, &link.session_id, "").await? else {
        return Ok(None);
    };
    let ask = super::Ask {
        message: link.message.clone(),
        zone: crate::ops::zone::Zone::minutes(link.tz_minutes as i32),
        ..Default::default()
    };
    let rest = super::rest_days(db).await;
    let l = super::label_data(&f, &ask, &rest);
    let text = |k: &str| l[k].as_str().map(str::to_string);
    let rest_days = rest.for_roast(f.purpose.as_deref(), f.level.as_deref());
    // Roasts in one bag rest as the newest one does
    let rest_text = f
        .lot
        .as_ref()
        .map(|l| l.last_at)
        .or(f.roasted_at)
        .map(|t| rest_line(ask.zone.date(t), rest_days, ask.zone.date(now)));
    let roast = match (text("level"), f.purpose.as_deref()) {
        (Some(level), Some(p)) => Some(format!("{level} roast for {p}")),
        (Some(level), None) => Some(format!("{level} roast")),
        (None, Some(p)) => Some(format!("Roasted for {p}")),
        (None, None) => None,
    };
    let (_, sizes) = crate::coffee::inventory::cost_settings(db).await;
    let tele = crate::roasts::telemetry_store::get_session_telemetry(db, &link.session_id)
        .await
        .unwrap_or_default();
    let events = crate::roasts::events::get_roast_events(db, &link.session_id)
        .await
        .unwrap_or_default();
    Ok(Some(Page {
        name: f.name.clone(),
        origin: text("origin"),
        coffees: f
            .lot
            .as_ref()
            .filter(|l| l.parts.len() > 1)
            .map(|l| l.parts.iter().map(coffee_line).collect())
            .unwrap_or_default(),
        variety: text("variety"),
        process: text("process"),
        roast,
        roasted: text("roasted"),
        best_from: text("best_from"),
        rest: rest_text,
        batch: f
            .lot
            .as_ref()
            .map(|l| l.batches.clone())
            .filter(|b| !b.is_empty())
            .or(f.batch.map(|b| b.to_string())),
        notes: text("notes"),
        message: text("message"),
        brew: brew_for(
            f.purpose.as_deref(),
            f.level.as_deref(),
            sizes.espresso_g,
            sizes.filter_g,
        ),
        curve: curve(&tele, &events),
        brand: brand_for(db, f.bean_id.as_deref()).await.or_else(|| {
            f.roaster.clone().map(|name| PageBrand {
                name,
                ..Default::default()
            })
        }),
    }))
}

/// The roast's brand kit: its bean's, else the `label_brand` setting's
pub(crate) async fn brand_for(db: &SqlitePool, bean: Option<&str>) -> Option<PageBrand> {
    let mut id = None;
    if let Some(b) = bean {
        id = super::store::bean_label(db, b)
            .await
            .ok()
            .flatten()
            .and_then(|l| l.brand);
    }
    if id.is_none() {
        id = crate::auth::settings::get(db, "label_brand").await;
    }
    let b = super::store::brand(db, id.as_deref()?.trim())
        .await
        .ok()??;
    let logo = match &b.logo {
        Some(name) => match super::store::asset_data(db, name).await.ok().flatten() {
            Some((kind, data)) if kind.starts_with("image/") && data.len() <= 512 * 1024 => {
                use base64::Engine as _;
                Some(format!(
                    "data:{kind};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(data)
                ))
            }
            _ => None,
        },
        None => None,
    };
    Some(PageBrand {
        name: b.name,
        tagline: b.tagline,
        place: b.place,
        website: b.website,
        logo,
        ink: b.ink,
        accent: b.accent,
        paper: b.paper,
    })
}

/// A colour that may go into the page's CSS: `#rrggbb` only
fn colour(c: Option<&str>) -> Option<&str> {
    c.filter(|c| {
        c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
    })
}

/// The page's look: chart paper, in the brand's colours when it has them; dark in a dark
/// browser
pub(crate) fn style(brand: Option<&PageBrand>) -> String {
    let ink = colour(brand.and_then(|b| b.ink.as_deref())).unwrap_or("#1f2933");
    let accent = colour(brand.and_then(|b| b.accent.as_deref())).unwrap_or("#b4532a");
    let paper = colour(brand.and_then(|b| b.paper.as_deref())).unwrap_or("#faf7f0");
    format!(
        r#":root{{--ink:{ink};--accent:{accent};--paper:{paper};--muted:color-mix(in srgb,var(--ink) 62%,var(--paper));--rule:color-mix(in srgb,var(--ink) 14%,var(--paper));--grid1:color-mix(in srgb,#2f8f6b 12%,var(--paper));--grid2:color-mix(in srgb,#2f8f6b 28%,var(--paper));--card:color-mix(in srgb,var(--accent) 7%,var(--paper));color-scheme:light}}
@media (prefers-color-scheme:dark){{:root{{--ink:#ece6da;--paper:#191714;--accent:color-mix(in srgb,{accent} 70%,#fff);color-scheme:dark}}header.brand img{{background:#f3eee4;border-radius:6px;padding:3px}}}}
*{{box-sizing:border-box}}
html{{-webkit-text-size-adjust:100%}}
body{{margin:0;background:var(--paper);color:var(--ink);font:17px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}}
main{{max-width:36rem;margin:0 auto;padding:20px 16px 40px}}
header.brand{{display:flex;align-items:center;gap:12px;padding-bottom:14px;border-bottom:1px solid var(--rule)}}
header.brand img{{height:44px;width:auto;max-width:120px}}
header.brand .n{{font-weight:650;letter-spacing:.02em}}
header.brand .t{{color:var(--muted);font-size:.85rem}}
.msg{{margin:22px 0 0;padding:14px 16px;background:var(--card);border-left:3px solid var(--accent);font:italic 1.15rem/1.45 ui-serif,Georgia,"Iowan Old Style",serif}}
h1{{font:600 2.1rem/1.15 ui-serif,Georgia,"Iowan Old Style",serif;margin:22px 0 6px;letter-spacing:-.01em;overflow-wrap:anywhere}}
.origin{{color:var(--muted);margin:0}}
.blend{{color:var(--muted);margin:.25rem 0 0;padding-left:1.1rem}}
.pill{{display:inline-block;margin-top:12px;padding:2px 10px;border:1px solid var(--accent);color:var(--accent);border-radius:3px;font-size:.85rem;font-weight:600;letter-spacing:.03em;text-transform:uppercase}}
.dates{{margin:20px 0 0;display:grid;grid-template-columns:1fr 1fr;gap:12px}}
.dates div{{border-top:2px solid var(--ink);padding-top:6px}}
.k{{display:block;color:var(--muted);font-size:.75rem;letter-spacing:.06em;text-transform:uppercase}}
.v{{font-weight:600}}
.rest{{margin:12px 0 0;font-weight:600;color:var(--accent)}}
h2{{font-size:.8rem;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);margin:30px 0 8px;font-weight:650}}
.notes{{font:1.3rem/1.4 ui-serif,Georgia,"Iowan Old Style",serif;margin:0}}
dl{{display:grid;grid-template-columns:auto 1fr;gap:4px 16px;margin:0}}
dt{{color:var(--muted)}}
dd{{margin:0}}
figure{{margin:0}}
svg{{display:block;width:100%;height:auto;background:var(--paper)}}
svg .g1{{stroke:var(--grid1);stroke-width:.6}}
svg .g2{{stroke:var(--grid2);stroke-width:.9}}
svg .ax{{fill:var(--muted);font-size:10px;font-family:ui-sans-serif,system-ui,sans-serif}}
svg .dev{{fill:var(--accent);opacity:.1}}
svg .bt{{fill:none;stroke:var(--ink);stroke-width:1.8;stroke-linejoin:round;stroke-linecap:round}}
svg .ev{{fill:var(--accent);stroke:var(--paper);stroke-width:1.2}}
svg .evt{{fill:var(--ink);font-size:11px;font-weight:600;font-family:ui-sans-serif,system-ui,sans-serif;paint-order:stroke;stroke:var(--paper);stroke-width:3px;stroke-linejoin:round}}
figcaption{{color:var(--muted);font-size:.9rem;margin-top:6px}}
.brew b{{display:block}}
.brew p{{margin:2px 0 0}}
footer{{margin-top:36px;padding-top:14px;border-top:1px solid var(--rule);color:var(--muted);font-size:.85rem}}
footer .mk{{display:inline;width:14px;height:14px;vertical-align:-3px;margin-right:2px;background:none}}
footer a{{color:inherit}}
.gone{{text-align:center;padding-top:20vh}}"#
    )
}

pub(crate) fn head(title: &str, style: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="robots" content="noindex,nofollow"><meta name="referrer" content="no-referrer"><title>{}</title><meta property="og:title" content="{}"><style>{style}</style></head>"#,
        esc(title),
        esc(title)
    )
}

/// The page as HTML: no script, nothing loaded from elsewhere
pub fn html(p: &Page) -> String {
    let brand = p.brand.as_ref();
    let title = match brand {
        Some(b) => format!("{} · {}", p.name, b.name),
        None => p.name.clone(),
    };
    let mut s = head(&title, &style(brand));
    s.push_str("<body><main>");
    if let Some(b) = brand {
        s.push_str(r#"<header class="brand">"#);
        if let Some(logo) = &b.logo {
            let _ = write!(s, r#"<img src="{}" alt="">"#, esc(logo));
        }
        let _ = write!(s, r#"<div><div class="n">{}</div>"#, esc(&b.name));
        if let Some(t) = b.tagline.as_deref().filter(|t| !t.trim().is_empty()) {
            let _ = write!(s, r#"<div class="t">{}</div>"#, esc(t));
        }
        s.push_str("</div></header>");
    }
    if let Some(m) = &p.message {
        let _ = write!(s, r#"<p class="msg">{}</p>"#, esc(m));
    }
    let _ = write!(s, "<h1>{}</h1>", esc(&p.name));
    if let Some(o) = &p.origin {
        let _ = write!(s, r#"<p class="origin">{}</p>"#, esc(o));
    }
    if !p.coffees.is_empty() {
        s.push_str(r#"<ul class="blend" aria-label="In this blend">"#);
        for c in &p.coffees {
            let _ = write!(s, "<li>{}</li>", esc(c));
        }
        s.push_str("</ul>");
    }
    if let Some(r) = &p.roast {
        let _ = write!(s, r#"<span class="pill">{}</span>"#, esc(r));
    }
    if p.roasted.is_some() || p.best_from.is_some() {
        s.push_str(r#"<div class="dates">"#);
        if let Some(r) = &p.roasted {
            let _ = write!(
                s,
                r#"<div><span class="k">Roasted</span><span class="v">{}</span></div>"#,
                esc(r)
            );
        }
        if let Some(b) = &p.best_from {
            let _ = write!(
                s,
                r#"<div><span class="k">Best from</span><span class="v">{}</span></div>"#,
                esc(b)
            );
        }
        s.push_str("</div>");
    }
    if let Some(r) = &p.rest {
        let _ = write!(s, r#"<p class="rest">{}</p>"#, esc(r));
    }
    if let Some(n) = &p.notes {
        let _ = write!(
            s,
            r#"<h2>Tasting notes</h2><p class="notes">{}</p>"#,
            esc(n)
        );
    }
    let first_up = |v: &Option<String>| {
        v.as_deref().map(|t| {
            let mut c = t.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
    };
    let details: Vec<(&str, String)> = [
        ("Variety", first_up(&p.variety)),
        ("Process", first_up(&p.process)),
        ("Batch", p.batch.as_ref().map(|b| format!("#{b}"))),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    .collect();
    if !details.is_empty() {
        s.push_str("<h2>The coffee</h2><dl>");
        for (k, v) in details {
            let _ = write!(s, "<dt>{k}</dt><dd>{}</dd>", esc(&v));
        }
        s.push_str("</dl>");
    }
    if let Some(c) = &p.curve {
        s.push_str("<h2>The roast</h2><figure>");
        s.push_str(&curve_svg(c));
        let mut parts = Vec::new();
        if let Some(d) = c.drop {
            parts.push(format!("Roast time {}", clock(d)));
        }
        if let (Some(fc), Some(d)) = (c.first_crack, c.drop)
            && d > fc
            && d > 0.0
        {
            parts.push(format!(
                "development {} ({:.0} %)",
                clock(d - fc),
                (d - fc) / d * 100.0
            ));
        }
        if let Some(t) = c.drop_temp {
            parts.push(format!("dropped at {t:.0} °C ({:.0} °F)", f_of(t)));
        }
        if !parts.is_empty() {
            let _ = write!(s, "<figcaption>{}</figcaption>", esc(&parts.join(" · ")));
        }
        s.push_str("</figure>");
    }
    if let Some(Brew {
        method: k, line, ..
    }) = &p.brew
    {
        let _ = write!(
            s,
            r#"<h2>Brewing it</h2><div class="brew"><b>{}</b><p>{}</p></div>"#,
            esc(k),
            esc(line)
        );
    }
    s.push_str("<footer>");
    match brand {
        Some(b) => {
            let _ = write!(s, "Roasted by {}", esc(&b.name));
            if let Some(pl) = b.place.as_deref().filter(|v| !v.trim().is_empty()) {
                let _ = write!(s, " · {}", esc(pl));
            }
            if let Some(w) = b.website.as_deref().filter(|v| !v.trim().is_empty()) {
                let href = if w.starts_with("http://") || w.starts_with("https://") {
                    w.to_string()
                } else {
                    format!("https://{w}")
                };
                let shown = w
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/');
                let _ = write!(
                    s,
                    r#" · <a href="{}" rel="noopener noreferrer">{}</a>"#,
                    esc(&href),
                    esc(shown)
                );
            }
        }
        None => s.push_str("Roasted at home"),
    }
    let _ = write!(
        s,
        " · Made with {} Roastwarden",
        crate::ops::brand::html(14)
    );
    s.push_str("</footer></main></body></html>");
    s
}

/// What an unknown or revoked code shows (the same for both)
pub fn gone_html() -> String {
    let mut s = head("Not found", &style(None));
    s.push_str(
        r#"<body><main class="gone"><h1>Nothing here</h1><p class="origin">This link is not active.</p></main></body></html>"#,
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_25_base32_characters_and_differ() {
        let a = new_code();
        let b = new_code();
        assert_eq!(a.len(), 25);
        assert!(a.bytes().all(|c| ALPHABET.contains(&c)), "{a}");
        assert_ne!(a, b);
        assert_eq!(normal(&a.to_ascii_lowercase()), Some(a.clone()));
        assert_eq!(normal(&format!(" {a} ")), Some(a));
        assert_eq!(normal("short"), None);
        assert_eq!(normal("0OOOOOOOOOOOOOOOOOOOOOOOO"), None, "0 is not base32");
        assert!(normal(SAMPLE_CODE).is_some());
        // Every character turns up (no stuck bits from the UUIDs' version and variant)
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.extend(new_code().bytes());
        }
        assert_eq!(seen.len(), 32);
    }

    #[test]
    fn the_qr_code_is_in_capitals_when_that_is_the_same_address() {
        let code = "ABCDEFGHIJKLMNOPQRSTUVWXY";
        assert_eq!(
            url("https://roastwarden.example.org/", code),
            "https://roastwarden.example.org/R/ABCDEFGHIJKLMNOPQRSTUVWXY"
        );
        assert_eq!(
            qr_text("https://roastwarden.example.org", code),
            "HTTPS://ROASTWARDEN.EXAMPLE.ORG/R/ABCDEFGHIJKLMNOPQRSTUVWXY"
        );
        // A path of its own keeps its case
        assert_eq!(
            qr_text("https://example.com/Roast", code),
            "https://example.com/Roast/R/ABCDEFGHIJKLMNOPQRSTUVWXY"
        );
        // and capitals make a smaller QR code
        let small = qrcode::QrCode::with_error_correction_level(
            qr_text("https://roastwarden.example.org", code),
            qrcode::EcLevel::M,
        )
        .unwrap();
        let mixed = qrcode::QrCode::with_error_correction_level(
            url("https://roastwarden.example.org", code),
            qrcode::EcLevel::M,
        )
        .unwrap();
        assert!(
            small.width() < mixed.width(),
            "{} < {}",
            small.width(),
            mixed.width()
        );
    }

    #[test]
    fn rest_lines() {
        let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(
            rest_line(d("2026-10-20"), 7, d("2026-10-24")),
            "Resting: at its best in 3 days"
        );
        assert_eq!(
            rest_line(d("2026-10-20"), 7, d("2026-10-26")),
            "Resting: at its best in 1 day"
        );
        assert_eq!(
            rest_line(d("2026-10-20"), 7, d("2026-10-27")),
            "Ready: roasted 7 days ago"
        );
        assert_eq!(
            rest_line(d("2026-10-20"), 0, d("2026-10-20")),
            "Ready: roasted today"
        );
    }

    #[test]
    fn the_page_escapes_what_it_shows_and_loads_nothing() {
        let p = Page {
            name: "Kenya <script>alert(1)</script>".into(),
            message: Some("for Sam & \"Jo\"".into()),
            brand: Some(PageBrand {
                name: "Hill Street".into(),
                ink: Some("red;}body{display:none".into()),
                website: Some("hillstreet.example".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let h = html(&p);
        assert!(!h.contains("<script>"), "{h}");
        assert!(
            h.contains(r#"<svg class="mk""#) && h.contains("Made with"),
            "the mark in the footer"
        );
        assert!(h.contains("Kenya &lt;script&gt;"));
        assert!(h.contains("for Sam &amp; &quot;Jo&quot;"));
        assert!(
            !h.contains("display:none"),
            "a colour that is not #rrggbb is left out"
        );
        assert!(h.contains(r#"href="https://hillstreet.example" rel="noopener noreferrer""#));
        assert!(
            !h.contains("src=\"http"),
            "nothing is loaded from elsewhere"
        );
        assert!(h.contains(r#"<meta name="robots" content="noindex,nofollow">"#));
    }

    /// A blend's page lists each coffee with its share, place and process
    #[test]
    fn a_blend_lists_its_coffees() {
        let part = |name: &str, region: Option<&str>, origin: &str, process: &str, share| {
            super::super::BlendPart {
                name: name.into(),
                full_name: name.into(),
                origin: Some(origin.into()),
                region: region.map(str::to_string),
                variety: None,
                process: Some(process.into()),
                share,
            }
        };
        let a = part("Guji", Some("Guji"), "Ethiopia", "natural", 60);
        let b = part("Kivu", None, "Rwanda", "washed", 40);
        assert_eq!(coffee_line(&a), "60 % Guji (Guji, Ethiopia), natural");
        assert_eq!(coffee_line(&b), "40 % Kivu (Rwanda), washed");
        let p = Page {
            name: "House filter".into(),
            origin: Some("60 % Ethiopia · 40 % Rwanda".into()),
            coffees: vec![coffee_line(&a), coffee_line(&b)],
            ..Default::default()
        };
        let h = html(&p);
        assert!(
            h.contains(r#"<ul class="blend" aria-label="In this blend"><li>60 % Guji"#),
            "{h}"
        );
        assert!(!html(&Page::default()).contains(r#"class="blend""#));
    }

    fn sample(t: f32, bt: f32) -> SessionTelemetry {
        SessionTelemetry {
            id: String::new(),
            session_id: String::new(),
            timestamp: Utc::now(),
            elapsed_seconds: t,
            bean_temp: Some(bt),
            env_temp: None,
            rate_of_rise: None,
            heater: None,
            fan: None,
            setpoint: None,
            extras: None,
        }
    }

    fn event(kind: RoastEventType, t: f32) -> RoastEvent {
        RoastEvent {
            id: String::new(),
            session_id: String::new(),
            event_type: kind,
            elapsed_seconds: t,
            temperature: None,
            notes: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn the_curve_runs_from_charge_to_drop() {
        // Preheat before charge at 30 s, a roast to 205 °C dropped at 630 s
        let tele: Vec<_> = (0..700)
            .map(|t| {
                let t = t as f32;
                sample(
                    t,
                    if t < 30.0 {
                        180.0
                    } else {
                        90.0 + (t - 30.0) * 0.19
                    },
                )
            })
            .collect();
        let events = vec![
            event(RoastEventType::Charge, 30.0),
            event(RoastEventType::FirstCrackStart, 500.0),
            event(RoastEventType::Drop, 630.0),
        ];
        let c = curve(&tele, &events).unwrap();
        assert_eq!(c.bt.first().unwrap().0, 0.0, "from charge");
        assert!(c.bt.last().unwrap().0 <= 600.0, "to drop");
        assert!(
            c.bt.len() < 330,
            "a point every 2 s at most: {}",
            c.bt.len()
        );
        assert_eq!(c.first_crack, Some(470.0));
        assert_eq!(c.drop, Some(600.0));
        assert!((c.drop_temp.unwrap() - 204.0).abs() < 1.0);
        let svg = curve_svg(&c);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert!(svg.contains("First crack 7:50"), "{svg}");
        assert!(svg.contains("Drop 10:00"));
        assert!(svg.contains(r#"class="dev""#), "development is shaded");
        // Too few samples: no curve
        assert!(curve(&tele[..5], &events).is_none());
    }
}
