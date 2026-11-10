//! What goes on a label: the facts of a roast (its bean, lot, weight, dates and notes) read
//! from the database, the rest windows, and the data a design reads from them.

use chrono::{DateTime, Datelike, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::notes::notes_for;
use super::share;

/// Rest before a roast is at its best, days (docs/labels.md, "Best from"): espresso and
/// filter, light (up to medium-light) and darker.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq)]
pub struct RestDays {
    pub espresso_light: i64,
    pub espresso: i64,
    pub filter_light: i64,
    pub filter: i64,
}

pub const REST_KEYS: [(&str, i64); 4] = [
    ("rest_days_espresso_light", 10),
    ("rest_days_espresso", 7),
    ("rest_days_filter_light", 7),
    ("rest_days_filter", 4),
];

pub async fn rest_days(db: &SqlitePool) -> RestDays {
    let mut v = [0i64; 4];
    for (i, (key, default)) in REST_KEYS.iter().enumerate() {
        v[i] = crate::auth::settings::get(db, key)
            .await
            .and_then(|s| s.trim().parse().ok())
            .filter(|d| (0..=90).contains(d))
            .unwrap_or(*default);
    }
    RestDays {
        espresso_light: v[0],
        espresso: v[1],
        filter_light: v[2],
        filter: v[3],
    }
}

impl RestDays {
    /// Espresso for espresso; filter, omni and other read as filter. A light roast is any
    /// level up to medium-light; no level reads as light (the rest is the longer).
    pub fn for_roast(&self, purpose: Option<&str>, level: Option<&str>) -> i64 {
        let light = level.is_none_or(|l| {
            matches!(
                l,
                "extremely_light" | "very_light" | "light" | "medium_light"
            )
        });
        match (purpose == Some("espresso"), light) {
            (true, true) => self.espresso_light,
            (true, false) => self.espresso,
            (false, true) => self.filter_light,
            (false, false) => self.filter,
        }
    }
}

/// The fields a label can show; any can be left off (`hide`).
pub const FIELDS: [&str; 12] = [
    "origin",
    "variety",
    "process",
    "level",
    "roasted",
    "batch",
    "best_from",
    "form",
    "weight",
    "notes",
    "qr",
    "roaster",
];

/// What the owner asks for beyond the roast itself.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    /// Net weight on the label, g (default the roast's roasted weight)
    pub weight_g: Option<f64>,
    /// A line for a gift ("for Sam")
    pub message: Option<String>,
    /// Tasting notes instead of the bean's
    pub notes: Option<String>,
    pub hide: Vec<String>,
    /// The owner's time zone: the request's offset, else the `time_zone` setting, else UTC
    /// (dates are the owner's local dates)
    pub zone: crate::ops::zone::Zone,
    /// Whole bean, or ground with a grind (docs/labels.md, "Whole bean or ground")
    pub form: crate::coffee::roasted::Form,
    /// How long it keeps its best (Settings → Roasting)
    pub best: crate::coffee::roasted::BestDays,
    /// The day it is packed (a ground bag's best-until counts from it); default the roast date
    pub packed_on: Option<chrono::NaiveDate>,
}

/// The roast, its bean and the settings, read for a label.
pub struct Facts {
    /// The roast's bean: its own label (design, parameters, brand) is the default
    pub bean_id: Option<String>,
    pub name: String,
    pub origin: Option<String>,
    pub region: Option<String>,
    pub producer: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    pub level: Option<String>,
    pub purpose: Option<String>,
    /// When the roast dropped (else started)
    pub roasted_at: Option<DateTime<Utc>>,
    pub batch: Option<i64>,
    pub roasted_g: Option<f64>,
    /// What the label says it tastes like (`notes_for`)
    pub notes: Option<String>,
    pub roaster: Option<String>,
    pub url: Option<String>,
    /// The bean's short name, printed as `name` (the full one is `full_name`)
    pub short_name: Option<String>,
    /// How to brew it (`share::brew_for`), as `l.brew`
    pub brew: Option<share::Brew>,
    /// In one bag with other roasts of the coffee (docs/roasted.md, "Roasts in one bag"): the
    /// label is the bag's
    pub lot: Option<LotFacts>,
}

/// A combined lot as its label prints it.
#[derive(Debug, Clone)]
pub struct LotFacts {
    /// "16+17"
    pub batches: String,
    /// Its oldest roast's and its newest's
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    /// The roasts' roasted weight together
    pub roasted_g: f64,
    /// The bag's own name, when the owner gave it one
    pub name: Option<String>,
    /// Its coffees (bean ids) and their whole-percent shares, the largest first
    pub coffees: Vec<(String, u32)>,
    /// A blend's coffees as its label prints them (`facts` fills them); empty for one coffee
    pub parts: Vec<BlendPart>,
}

/// A coffee in a blend, as its label prints it.
#[derive(Debug, Clone, Serialize)]
pub struct BlendPart {
    /// Its short name, else its name
    pub name: String,
    pub full_name: String,
    /// The country, as the bean has it
    pub origin: Option<String>,
    pub region: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    /// Whole percent of the bag
    pub share: u32,
}

/// A blend's origin line: each origin (else the coffee's name) with its share, the largest
/// first, one origin's coffees together ("55 % Ethiopia · 45 % Rwanda").
pub fn blend_origin(parts: &[BlendPart]) -> String {
    let mut by: Vec<(String, u32)> = Vec::new();
    for p in parts {
        let place = p
            .origin
            .clone()
            .filter(|o| !o.trim().is_empty())
            .unwrap_or_else(|| p.name.clone());
        match by.iter_mut().find(|b| b.0 == place) {
            Some(b) => b.1 += p.share,
            None => by.push((place, p.share)),
        }
    }
    by.sort_by_key(|b| std::cmp::Reverse(b.1));
    by.iter()
        .map(|(place, share)| format!("{share} % {place}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Distinct values in order, joined: a blend's varieties or processes ("natural / washed"; a
/// label line already uses " · " between its fields).
fn distinct(values: impl Iterator<Item = Option<String>>) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    for v in values.flatten() {
        let v = v.trim().to_string();
        if !v.is_empty() && !seen.iter().any(|s| s.eq_ignore_ascii_case(&v)) {
            seen.push(v);
        }
    }
    (!seen.is_empty()).then(|| seen.join(" / "))
}

fn level_word(l: &str) -> String {
    let s = l.replace('_', "-");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn date_text(d: chrono::NaiveDate) -> String {
    format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
}

/// Roasted over days: "6–7 Nov 2026", "30 Oct – 2 Nov 2026", or one day's date
pub(super) fn range_text(a: chrono::NaiveDate, b: chrono::NaiveDate) -> String {
    let (a, b) = (a.min(b), a.max(b));
    if a == b {
        date_text(a)
    } else if (a.year(), a.month()) == (b.year(), b.month()) {
        format!("{}–{}", a.day(), date_text(b))
    } else if a.year() == b.year() {
        format!(
            "{} {} – {}",
            a.day(),
            MONTHS[a.month0() as usize],
            date_text(b)
        )
    } else {
        format!("{} – {}", date_text(a), date_text(b))
    }
}

/// One label's data as the design reads it: text ready to print, `none` for what is hidden
/// or unknown.
pub fn label_data(f: &Facts, ask: &Ask, rest: &RestDays) -> Value {
    // A bag of several roasts: roasted over their days, ready when the newest is, at its best
    // until the oldest's best-until
    let (first, day) = match &f.lot {
        Some(l) => (
            Some(ask.zone.date(l.first_at)),
            Some(ask.zone.date(l.last_at)),
        ),
        None => {
            let d = f.roasted_at.map(|t| ask.zone.date(t));
            (d, d)
        }
    };
    let name = f
        .short_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| f.name.clone());
    let rest_days = rest.for_roast(f.purpose.as_deref(), f.level.as_deref());
    let best_until = first.zip(day).map(|(first, d)| {
        let lot = ask.best.until(first, f.level.as_deref(), rest_days);
        ask.best
            .bag_until(lot, &ask.form, ask.packed_on.unwrap_or(d).max(d))
    });
    let grams = ask.weight_g.or(f.roasted_g).filter(|g| *g > 0.0);
    // A blend: its coffees' origins with their shares, varieties and processes
    let parts = f.lot.as_ref().map_or(&[][..], |l| l.parts.as_slice());
    let blend = parts.len() > 1;
    let origin_line = if blend {
        blend_origin(parts)
    } else {
        [
            [f.region.as_deref(), f.origin.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", "),
            f.producer.clone().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
    };
    let (variety, process) = if blend {
        (
            distinct(parts.iter().map(|p| p.variety.clone())),
            distinct(parts.iter().map(|p| p.process.clone())),
        )
    } else {
        (f.variety.clone(), f.process.clone())
    };
    let mut l = json!({
        "name": name,
        "full_name": f.name,
        // A blend: "55 % Ethiopia · 45 % Rwanda"
        "origin": (!origin_line.is_empty()).then_some(origin_line),
        "variety": variety,
        "process": process,
        // A blend's coffees, the largest share first, for a design that lists them
        "blend": blend,
        "components": if blend { json!(parts) } else { json!([]) },
        "level": f.level.as_deref().map(level_word),
        // "Espresso", "Filter": a word to print, like the level
        "purpose": f.purpose.as_deref().map(level_word),
        "roasted": first.zip(day).map(|(a, b)| range_text(a, b)),
        // A number; "16+17" for roasts in one bag
        "batch": match &f.lot {
            Some(l) if !l.batches.is_empty() => json!(l.batches),
            _ => json!(f.batch),
        },
        "best_from": day.map(|d| date_text(d + Duration::days(rest_days))),
        // The dates as data, for a design that prints them its own way ("29 Oct",
        // "Thursday"): ISO dates, and the rest in days
        "roasted_iso": first.map(|d| d.format("%Y-%m-%d").to_string()),
        // Roasts in one bag: the newest one's day (else the same as roasted_iso)
        "roasted_last_iso": day.map(|d| d.format("%Y-%m-%d").to_string()),
        "best_from_iso": day.map(|d| (d + Duration::days(rest_days)).format("%Y-%m-%d").to_string()),
        "rest_days": day.map(|_| rest_days),
        // Whole bean or ground: "Whole bean", "Ground · espresso"; the bag's best-until (ground
        // coffee's is the earlier of the lot's and the pack date plus its days)
        "form": ask.form.text(),
        "ground": ask.form.ground,
        "grind": ask.form.grind,
        "best_until": best_until.map(date_text),
        "best_until_iso": best_until.map(|d| d.format("%Y-%m-%d").to_string()),
        "weight": grams.map(weight_text),
        "weight_g": grams.map(|g| (g * 10.0).round() / 10.0),
        "net": grams.map(net_weight),
        "notes": ask.notes.clone().filter(|n| !n.trim().is_empty()).or_else(|| f.notes.clone()),
        "message": ask.message.clone().filter(|m| !m.trim().is_empty()),
        "roaster": f.roaster,
        "brew": f.brew,
        "qr": f.url.is_some(),
        "qr_to": f.url.is_some().then_some("app"),
        "url": f.url,
    });
    hide_fields(&mut l, &ask.hide);
    l
}

/// Leave the hidden fields out (`none` in a design; the QR code `false`). Hiding the weight
/// hides every form of it, a date its ISO form too.
pub(super) fn hide_fields(l: &mut Value, hide: &[String]) {
    for key in hide {
        let keys: &[&str] = match key.as_str() {
            "weight" => &["weight", "weight_g", "net"],
            "roasted" => &["roasted", "roasted_iso", "roasted_last_iso"],
            "best_from" => &[
                "best_from",
                "best_from_iso",
                "rest_days",
                "best_until",
                "best_until_iso",
            ],
            // A blend's coffees are its origins
            "origin" => &["origin", "components"],
            k => &[k],
        };
        for k in keys {
            if let Some(v) = l.get_mut(*k) {
                *v = match *k {
                    "qr" => json!(false),
                    "components" => json!([]),
                    _ => Value::Null,
                };
            }
        }
    }
}

/// The weight as every label prints it, "250 g · 8.8 oz": the same rounding as `net`
/// (down, never more than is in the bag), whole grams
pub fn weight_text(g: f64) -> String {
    let oz = ((g / 28.349_523_125) * 10.0 + 1e-9).floor() / 10.0;
    format!("{} g · {oz:.1} oz", (g + 1e-9).floor())
}

/// The net quantity as a US label states it (21 CFR 101.105), never more than is in the
/// bag: ounces with the grams under a pound ("Net wt 8.8 oz (250 g)"), ounces with pounds
/// and ounces from 1 to 4 lb ("Net wt 24 oz (1 lb 8 oz) 680 g"), pounds from 4 lb.
pub fn net_weight(g: f64) -> String {
    let oz = g / 28.349_523_125;
    // Down, not to the nearest: a label may not claim more than the bag holds
    let down = |v: f64, places: i32| {
        let f = 10f64.powi(places);
        let x = (v * f + 1e-9).floor() / f;
        let t = format!("{x:.*}", places as usize);
        if t.contains('.') {
            t.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            t
        }
    };
    let grams = if g >= 1000.0 {
        format!("{} kg", down(g / 1000.0, 2))
    } else {
        format!("{} g", down(g, 0))
    };
    if oz < 16.0 {
        format!("Net wt {} oz ({grams})", down(oz, 1))
    } else if oz < 64.0 {
        let lb = (oz / 16.0).floor();
        let rest = oz - lb * 16.0;
        let lb_text = if lb == 1.0 {
            "1 lb".to_string()
        } else {
            format!("{lb} lb")
        };
        let mixed = if rest < 0.05 {
            lb_text
        } else {
            format!("{lb_text} {} oz", down(rest, 1))
        };
        format!("Net wt {} oz ({mixed}) {grams}", down(oz, 1))
    } else {
        format!("Net wt {} lb ({grams})", down(oz / 16.0, 2))
    }
}

pub async fn facts(db: &SqlitePool, id: &str, link_base: &str) -> anyhow::Result<Option<Facts>> {
    let Some(s) = crate::roasts::sessions::get_session(db, id).await? else {
        return Ok(None);
    };
    let events = crate::roasts::events::get_roast_events(db, id)
        .await
        .unwrap_or_default();
    let drop = events
        .iter()
        .find(|e| e.event_type == crate::roasts::types::RoastEventType::Drop)
        .map(|e| e.elapsed_seconds);
    let roasted_at = match (s.start_time, drop) {
        (Some(t), Some(d)) => Some(t + Duration::milliseconds((f64::from(d) * 1000.0) as i64)),
        (Some(t), None) => Some(s.end_time.unwrap_or(t)),
        _ => s.end_time,
    };
    #[derive(sqlx::FromRow)]
    struct BeanBits {
        name: String,
        origin: Option<String>,
        region: Option<String>,
        producer: Option<String>,
        variety: Option<String>,
        process: Option<String>,
        cupping_notes: Option<String>,
        label_notes: Option<String>,
        short_name: Option<String>,
    }
    let bean = match &s.bean_id {
        Some(b) => sqlx::query_as::<_, BeanBits>(
            "SELECT name, origin, region, producer, variety, process, cupping_notes, label_notes, \
             short_name FROM beans WHERE id = ?",
        )
        .bind(b)
        .fetch_optional(db)
        .await
        .ok()
        .flatten(),
        None => None,
    };
    let cupping = crate::roasts::cupping::get_cupping(db, id)
        .await
        .ok()
        .flatten()
        .and_then(|c| c.cupping.notes.clone())
        .filter(|n| !n.trim().is_empty());
    let roaster = crate::auth::settings::get(db, "label_roaster_name")
        .await
        .filter(|v| !v.trim().is_empty());
    let (_, sizes) = crate::coffee::inventory::cost_settings(db).await;
    let brew = share::brew_for(
        s.roast_purpose.as_deref(),
        s.target_roast_level.as_deref(),
        sizes.espresso_g,
        sizes.filter_g,
    );
    let mut lot = crate::coffee::roasted::label_lot(db, id).await?;
    // A blend: each coffee's origin, variety and process; the bag's name, else its coffees'
    let mut blend_name = None;
    if let Some(l) = lot.as_mut().filter(|l| l.coffees.len() > 1) {
        for (bean_id, share) in &l.coffees {
            let Some(b) = sqlx::query_as::<_, BeanBits>(
                "SELECT name, origin, region, producer, variety, process, cupping_notes, \
                 label_notes, short_name FROM beans WHERE id = ?",
            )
            .bind(bean_id)
            .fetch_optional(db)
            .await?
            else {
                continue;
            };
            l.parts.push(BlendPart {
                name: b
                    .short_name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| b.name.clone()),
                full_name: b.name,
                origin: b.origin,
                region: b.region,
                variety: b.variety,
                process: b.process,
                share: *share,
            });
        }
        blend_name = Some(l.name.clone().unwrap_or_else(|| {
            l.parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        }));
    }
    if let Some(name) = blend_name {
        // One bean's facts would mislabel a blend: its own come from `parts`, and the
        // tasting notes are the owner's to write (`Ask::notes`)
        return Ok(Some(Facts {
            bean_id: s.bean_id.clone(),
            name,
            origin: None,
            region: None,
            producer: None,
            variety: None,
            process: None,
            level: s.target_roast_level.clone(),
            purpose: s.roast_purpose.clone(),
            roasted_at,
            batch: s.batch_number.map(i64::from),
            roasted_g: lot.as_ref().map(|l| l.roasted_g),
            notes: None,
            roaster,
            url: Some(format!("{link_base}/history/{id}")),
            short_name: None,
            brew,
            lot,
        }));
    }
    Ok(Some(Facts {
        bean_id: s.bean_id.clone(),
        name: bean
            .as_ref()
            .map_or_else(|| s.name.clone(), |b| b.name.clone()),
        origin: bean
            .as_ref()
            .and_then(|b| b.origin.clone())
            .or(s.bean_origin.clone()),
        region: bean.as_ref().and_then(|b| b.region.clone()),
        producer: bean.as_ref().and_then(|b| b.producer.clone()),
        variety: bean
            .as_ref()
            .and_then(|b| b.variety.clone())
            .or(s.bean_variety.clone()),
        process: bean.as_ref().and_then(|b| b.process.clone()),
        level: s.target_roast_level.clone(),
        purpose: s.roast_purpose.clone(),
        roasted_at,
        batch: s.batch_number.map(i64::from),
        roasted_g: lot
            .as_ref()
            .map(|l| l.roasted_g)
            .or(s.roasted_weight.map(f64::from)),
        notes: notes_for(
            bean.as_ref().and_then(|b| b.label_notes.as_deref()),
            cupping.as_deref(),
            bean.as_ref().and_then(|b| b.cupping_notes.as_deref()),
        ),
        roaster,
        url: Some(format!("{link_base}/history/{id}")),
        short_name: bean.as_ref().and_then(|b| b.short_name.clone()),
        brew,
        lot,
    }))
}
