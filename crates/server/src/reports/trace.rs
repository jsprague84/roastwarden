//! Traceability (`docs/reports.md`, "Traceability"): from a green coffee to
//! every bag, or from a roast (a bag's batch number) back to its green lot. `GET
//! /api/trace/roast/{id}` and `/api/trace/bean/{id}` answer [`Trace`]; the report
//! (`assets/reports/trace.typ`, schema `roastwarden.report.trace/1`) lays the same out: the
//! green coffee (bean, supplier, lot, origin, process, its purchases), each roast (batch,
//! date, roaster, profile, green → roasted, loss) and where it went (packed bags with form
//! and grind, label runs, gifts with the recipient, used, discarded, what is left). No
//! prices: a trace is for whoever holds the bag.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::{DASH, Options, ReportError, grams, meta, short_date};
use crate::coffee::inventory::InventoryService;
use crate::coffee::roasted::{LotState, RoastedMove, RoastedService};
use crate::labels::runs::RunItem;
use crate::ops::zone::Zone;

/// The most roasts one trace lists
pub const MAX_ROASTS: usize = 200;

/// A green coffee to its bags, or a roast back to its green lot.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Trace {
    /// What was asked: roast | bean
    pub subject: String,
    /// The green coffee (none: a roast with no bean)
    pub coffee: Option<TraceCoffee>,
    /// Its finished roasts, oldest first (a roast's trace: that one)
    pub roasts: Vec<TraceRoast>,
}

/// The green coffee
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TraceCoffee {
    pub id: String,
    pub name: String,
    pub supplier: Option<String>,
    pub lot: Option<String>,
    pub origin: Option<String>,
    pub region: Option<String>,
    pub producer: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    /// The supplier's offer
    pub url: Option<String>,
    /// Purchases, oldest first (no prices)
    pub purchases: Vec<TracePurchase>,
}

/// A purchase of the green coffee
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TracePurchase {
    pub at: DateTime<Utc>,
    pub grams: f32,
    pub supplier: Option<String>,
    /// "2026" or "2025/26"
    pub crop_year: Option<String>,
    /// When the lot landed at the supplier
    pub arrival: Option<String>,
    /// The moves of one order share it
    pub order_id: Option<String>,
    pub note: Option<String>,
}

/// A roast and where its coffee went
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TraceRoast {
    pub id: String,
    pub name: String,
    pub batch_number: Option<i32>,
    pub device_id: String,
    /// The roaster's name
    pub roaster: Option<String>,
    /// The profile it followed
    pub profile: Option<String>,
    /// The DROP (or the roast's end)
    pub roasted_at: DateTime<Utc>,
    /// The owner's date
    pub roasted_on: NaiveDate,
    pub green_g: Option<f32>,
    pub roasted_g: Option<f32>,
    /// (green − roasted) / green, %
    pub loss_pct: Option<f32>,
    /// What is left (none before it is weighed)
    pub left_g: Option<f32>,
    pub state: Option<LotState>,
    /// The lot's best-until
    pub best_until: Option<NaiveDate>,
    pub used_g: f32,
    pub given_g: f32,
    pub packed_g: f32,
    pub discarded_g: f32,
    /// Every move: used, given (with the recipient), packed (bags, size, form, grind, the
    /// label run, the bags' best-until), discarded, counted
    pub moves: Vec<RoastedMove>,
    /// Label runs with this roast's labels, oldest first
    pub label_runs: Vec<TraceRun>,
    /// In one bag with other roasts: the combined lot (its moves are split across them)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combined: Option<crate::coffee::roasted::CombinedRef>,
}

/// A label print run with a roast's labels
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TraceRun {
    pub id: String,
    pub created_at: String,
    /// This roast's labels in it
    pub labels: usize,
    /// made, sent, completed, printed, …
    pub state: String,
    pub note: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RoastRow {
    id: String,
    name: String,
    batch_number: Option<i32>,
    device_id: String,
    roaster: Option<String>,
    profile: Option<String>,
    roasted_at: DateTime<Utc>,
    green_weight: Option<f32>,
    roasted_weight: Option<f32>,
    bean_id: Option<String>,
}

const ROAST_SELECT: &str = "SELECT s.id, s.name, s.batch_number, s.device_id, d.name AS roaster, \
     p.name AS profile, COALESCE(s.end_time, s.start_time, s.created_at) AS roasted_at, \
     s.green_weight, s.roasted_weight, s.bean_id FROM roast_sessions s \
     LEFT JOIN devices d ON d.device_id = s.device_id \
     LEFT JOIN roast_profiles p ON p.id = s.profile_id";

async fn roast_line(
    db: &SqlitePool,
    r: RoastRow,
    zone: Zone,
    now: DateTime<Utc>,
) -> anyhow::Result<TraceRoast> {
    let lot = RoastedService::new(db.clone())
        .lot(&r.id, zone, now)
        .await?;
    let moves = lot
        .as_ref()
        .and_then(|l| l.moves.clone())
        .unwrap_or_default();
    let taken = |kind: &str| -> f32 {
        let g: f32 = moves
            .iter()
            .filter(|m| m.kind == kind)
            .map(|m| -m.delta_g)
            .sum();
        (g * 10.0).round() / 10.0 + 0.0
    };
    let runs: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, created_at, state, note, items FROM label_runs WHERE items LIKE ? \
         ORDER BY created_at",
    )
    .bind(format!("%{}%", r.id))
    .fetch_all(db)
    .await?;
    let label_runs = runs
        .into_iter()
        .filter_map(|(id, created_at, state, note, items)| {
            let items: Vec<RunItem> = serde_json::from_str(&items).ok()?;
            let labels: usize = items
                .iter()
                .filter(|i| i.roast.as_deref() == Some(r.id.as_str()))
                .map(|i| i.count)
                .sum();
            (labels > 0).then_some(TraceRun {
                id,
                created_at,
                labels,
                state,
                note,
            })
        })
        .collect();
    let loss_pct = match (r.green_weight, r.roasted_weight) {
        (Some(g), Some(o)) if g > 0.0 && o > 0.0 => Some(((g - o) / g * 1000.0).round() / 10.0),
        _ => None,
    };
    Ok(TraceRoast {
        roasted_on: lot
            .as_ref()
            .map_or_else(|| zone.date(r.roasted_at), |l| l.roasted_on),
        left_g: lot.as_ref().and_then(|l| l.left_g),
        state: lot.as_ref().map(|l| l.state),
        best_until: lot.as_ref().map(|l| l.best_until),
        used_g: taken("used"),
        given_g: taken("given"),
        packed_g: taken("packed"),
        discarded_g: taken("discarded"),
        moves,
        label_runs,
        combined: lot.as_ref().and_then(|l| l.combined.clone()),
        id: r.id,
        name: r.name,
        batch_number: r.batch_number,
        device_id: r.device_id,
        roaster: r.roaster,
        profile: r.profile,
        roasted_at: r.roasted_at,
        green_g: r.green_weight,
        roasted_g: r.roasted_weight,
        loss_pct,
    })
}

async fn coffee(db: &SqlitePool, bean_id: &str) -> anyhow::Result<Option<TraceCoffee>> {
    let Some(d) = InventoryService::new(db.clone())
        .bean_detail(bean_id)
        .await?
    else {
        return Ok(None);
    };
    let mut purchases: Vec<TracePurchase> = d
        .moves
        .into_iter()
        .filter(|m| m.kind == "purchase")
        .map(|m| TracePurchase {
            at: m.at,
            grams: m.delta_g,
            supplier: m.supplier,
            crop_year: m.crop_year,
            arrival: m.arrival,
            order_id: m.order_id,
            note: m.note,
        })
        .collect();
    purchases.sort_by_key(|p| p.at);
    let b = d.bean;
    Ok(Some(TraceCoffee {
        id: b.id,
        name: b.name,
        supplier: b.supplier,
        lot: b.lot,
        origin: b.origin,
        region: b.region,
        producer: b.producer,
        variety: b.variety,
        process: b.process,
        url: b.url,
        purchases,
    }))
}

/// A roast back to its green coffee, and where its coffee went. None: no such roast.
pub async fn of_roast(
    db: &SqlitePool,
    id: &str,
    zone: Zone,
    now: DateTime<Utc>,
) -> anyhow::Result<Option<Trace>> {
    let row: Option<RoastRow> =
        sqlx::query_as(AssertSqlSafe(format!("{ROAST_SELECT} WHERE s.id = ?")))
            .bind(id)
            .fetch_optional(db)
            .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let coffee = match &row.bean_id {
        Some(b) => coffee(db, b).await?,
        None => None,
    };
    Ok(Some(Trace {
        subject: "roast".into(),
        coffee,
        roasts: vec![roast_line(db, row, zone, now).await?],
    }))
}

/// A green coffee to every finished roast of it and where each went. None: no such bean.
pub async fn of_bean(
    db: &SqlitePool,
    id: &str,
    zone: Zone,
    now: DateTime<Utc>,
) -> anyhow::Result<Option<Trace>> {
    let Some(coffee) = coffee(db, id).await? else {
        return Ok(None);
    };
    let rows: Vec<RoastRow> = sqlx::query_as(AssertSqlSafe(format!(
        "{ROAST_SELECT} WHERE s.bean_id = ? AND s.status = 'completed' ORDER BY roasted_at LIMIT ?"
    )))
    .bind(id)
    .bind(MAX_ROASTS as i64)
    .fetch_all(db)
    .await?;
    let mut roasts = Vec::with_capacity(rows.len());
    for r in rows {
        roasts.push(roast_line(db, r, zone, now).await?);
    }
    Ok(Some(Trace {
        subject: "bean".into(),
        coffee: Some(coffee),
        roasts,
    }))
}

// ---- the report ----

fn day(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

fn mark(r: &TraceRoast) -> String {
    r.batch_number
        .map_or_else(|| r.name.clone(), |n| format!("#{n}"))
}

fn g_text(g: f32) -> String {
    super::weight(f64::from(g))
}

/// Where a roast's coffee went, one row a move or label run, by date
fn rows(r: &TraceRoast, o: &Options) -> Vec<Value> {
    let mut out: Vec<(DateTime<Utc>, Value)> = Vec::new();
    for m in &r.moves {
        let amount = g_text(m.delta_g.abs());
        let (what, amount, detail) = match m.kind.as_str() {
            "packed" => {
                let form =
                    crate::coffee::roasted::Form::stored(m.form.as_deref(), m.grind.as_deref());
                let bags = match (m.bags, m.bag_g) {
                    (Some(n), Some(g)) => format!("{n} × {} g", grams(f64::from(g))),
                    _ => amount.clone(),
                };
                let mut detail = vec![form.text().to_lowercase()];
                if let Some(b) = m.best_until {
                    detail.push(format!("best until {}", day(b)));
                }
                if m.label_run.is_some() {
                    detail.push("from a label run".into());
                }
                ("Packed", bags, detail.join(" · "))
            }
            "given" => (
                "Given",
                amount,
                m.recipient
                    .as_ref()
                    .map_or_else(String::new, |t| format!("to {t}")),
            ),
            "used" => ("Used", amount, String::new()),
            "discarded" => ("Discarded", amount, String::new()),
            "adjust" => (
                "Counted",
                format!("{}{}", if m.delta_g >= 0.0 { "+" } else { "−" }, amount),
                "what was left, weighed".into(),
            ),
            _ => ("Moved", amount, String::new()),
        };
        // This roast's share of a move on the bag it is in
        let detail = match (&r.combined, &m.combine_move) {
            (Some(c), Some(_)) if detail.is_empty() => {
                format!("its share, from the bag #{}", c.batches)
            }
            (Some(c), Some(_)) => format!("{detail} · its share, from the bag #{}", c.batches),
            _ => detail,
        };
        let detail = match &m.note {
            Some(n) if detail.is_empty() => n.clone(),
            Some(n) => format!("{detail} · {n}"),
            None => detail,
        };
        out.push((
            m.at,
            json!({ "date": short_date(&o.local(m.at)), "what": what, "amount": amount, "detail": detail }),
        ));
    }
    for run in &r.label_runs {
        let at = DateTime::parse_from_rfc3339(&run.created_at)
            .map_or(r.roasted_at, |t| t.with_timezone(&Utc));
        let state = match run.state.as_str() {
            "made" => "made, not printed",
            s => s,
        };
        let detail = match &run.note {
            Some(n) => format!("{state} · {n}"),
            None => state.to_string(),
        };
        out.push((
            at,
            json!({
                "date": short_date(&o.local(at)),
                "what": "Labels",
                "amount": format!("{} label{}", run.labels, if run.labels == 1 { "" } else { "s" }),
                "detail": detail,
            }),
        ));
    }
    out.sort_by_key(|(at, _)| *at);
    out.into_iter().map(|(_, v)| v).collect()
}

pub fn build(t: &Trace, o: &Options) -> Value {
    let kind = super::kind("trace").expect("the trace kind");
    let subject = match (t.subject.as_str(), t.roasts.first(), &t.coffee) {
        ("roast", Some(r), _) => format!("{} {}", mark(r), r.name),
        (_, _, Some(c)) => c.name.clone(),
        _ => "Trace".into(),
    };
    let coffee = t.coffee.as_ref().map(|c| {
        let place = [c.region.as_deref(), c.origin.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
        let facts: Vec<Value> = [
            ("Supplier", c.supplier.clone()),
            ("Lot", c.lot.clone()),
            ("Origin", (!place.is_empty()).then_some(place)),
            ("Producer", c.producer.clone()),
            ("Variety", c.variety.clone()),
            ("Process", c.process.clone()),
        ]
        .into_iter()
        .map(|(label, v)| json!({ "label": label, "value": v.unwrap_or_else(|| DASH.into()) }))
        .collect();
        let purchases: Vec<Value> = c
            .purchases
            .iter()
            .map(|p| {
                json!({
                    "date": short_date(&o.local(p.at)),
                    "amount": g_text(p.grams),
                    "supplier": p.supplier.clone().unwrap_or_else(|| DASH.into()),
                    "crop": p.crop_year.clone().unwrap_or_else(|| DASH.into()),
                    "arrival": p.arrival.clone().unwrap_or_else(|| DASH.into()),
                    "note": p.note.clone(),
                })
            })
            .collect();
        json!({ "name": c.name, "facts": facts, "purchases": purchases, "link": c.url })
    });
    let roasts: Vec<Value> = t
        .roasts
        .iter()
        .map(|r| {
            let weights = match (r.green_g, r.roasted_g) {
                (Some(g), Some(o)) => format!("{} → {}", g_text(g), g_text(o)),
                (Some(g), None) => format!("{} → not weighed", g_text(g)),
                (None, Some(o)) => format!("{DASH} → {}", g_text(o)),
                _ => DASH.into(),
            };
            let left = match (r.left_g, r.state) {
                (None, _) => "not weighed".to_string(),
                (Some(_), Some(LotState::Finished)) => "none left".to_string(),
                (Some(l), _) => format!("{} left", g_text(l)),
            };
            json!({
                "mark": mark(r),
                "name": r.name,
                "date": day(r.roasted_on),
                "roaster": r.roaster.clone().unwrap_or_else(|| r.device_id.clone()),
                "profile": r.profile.clone().unwrap_or_else(|| DASH.into()),
                "weights": weights,
                "loss": r.loss_pct.map_or(DASH.into(), |l| format!("{l:.1} %")),
                "left": left,
                "best_until": r.best_until.map(day),
                "rows": rows(r, o),
            })
        })
        .collect();
    let sum = |f: fn(&TraceRoast) -> f32| -> f32 { t.roasts.iter().map(f).sum() };
    let bags: i64 = t
        .roasts
        .iter()
        .flat_map(|r| &r.moves)
        .filter(|m| m.kind == "packed")
        .filter_map(|m| m.bags)
        .sum();
    let green: f32 = t.roasts.iter().filter_map(|r| r.green_g).sum();
    let left: f32 = t.roasts.iter().filter_map(|r| r.left_g).sum();
    let n = t.roasts.len();
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &subject, o),
        "title": subject,
        "subject": t.subject,
        "coffee": coffee,
        "summary": [
            { "label": "Roasts", "value": n.to_string(),
              "sub": format!("{} green", g_text(green)) },
            { "label": "Packed", "value": format!("{bags} bag{}", if bags == 1 { "" } else { "s" }),
              "sub": g_text(sum(|r| r.packed_g)) },
            { "label": "Given and used", "value": g_text(sum(|r| r.given_g) + sum(|r| r.used_g)),
              "sub": format!("{} given", g_text(sum(|r| r.given_g))) },
            { "label": "Left", "value": g_text(left),
              "sub": if sum(|r| r.discarded_g) > 0.0 { format!("{} discarded", g_text(sum(|r| r.discarded_g))) } else { "none discarded".into() } },
        ],
        "roasts": roasts,
        "notes": "A bag's batch number (#) is its roast: roast trace #N, or this report, leads from the bag back to the green lot. Packed bags, gifts, use and discards are what was recorded (Beans → Roasted coffee, roast roasted); label runs are the labels made for the roast.",
    })
}

/// A trace for the report: a roast's or a bean's
pub async fn gather(
    db: &SqlitePool,
    roast: Option<&str>,
    bean: Option<&str>,
    o: &Options,
) -> Result<Trace, ReportError> {
    let found = match (roast, bean) {
        (Some(r), None) => of_roast(db, r, o.zone, o.now).await?,
        (None, Some(b)) => of_bean(db, b, o.zone, o.now).await?,
        _ => {
            return Err(ReportError::Bad(
                "a trace is of one roast or one bean (roast=… or bean=…)".into(),
            ));
        }
    };
    found.ok_or_else(|| {
        ReportError::NotFound(match (roast, bean) {
            (Some(r), _) => format!("no roast {r:?}"),
            (_, b) => format!("no bean {:?}", b.unwrap_or_default()),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::inventory::{BeanRequest, PurchaseFields, StockRequest};
    use crate::coffee::roasted::RoastedMoveRequest;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::options;
    use crate::reports::{Format, Paper, Units, kind, render};

    #[tokio::test]
    async fn a_bean_leads_to_its_roasts_and_their_bags() {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        let bean = inv
            .create_bean(BeanRequest {
                name: Some("Ethiopia Guji Hambela".into()),
                origin: Some(Some("Ethiopia".into())),
                process: Some(Some("natural".into())),
                lot: Some(Some("HMB-2026-14".into())),
                supplier: Some(Some("Sweet Maria's".into())),
                ..Default::default()
            })
            .await
            .unwrap();
        inv.add_stock(
            &bean.id,
            StockRequest {
                delta_g: Some(2000.0),
                purchase: PurchaseFields {
                    price: Some(30.0),
                    supplier: Some("Sweet Maria's".into()),
                    crop_year: Some("2026".into()),
                    at: Some("2026-09-01".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for (id, batch, end) in [
            ("r1", 11, "2026-10-05T15:00:00Z"),
            ("r2", 12, "2026-10-12T15:00:00Z"),
        ] {
            sqlx::query(
                "INSERT INTO roast_sessions (id, name, device_id, status, created_at, updated_at, \
                 end_time, roast_purpose, target_roast_level, green_weight, roasted_weight, \
                 bean_id, batch_number) VALUES (?, ?, 'esp32_roaster_01', 'completed', ?, ?, ?, \
                 'espresso', 'light', 500.0, 430.0, ?, ?)",
            )
            .bind(id)
            .bind(format!("Guji {batch}"))
            .bind(end)
            .bind(end)
            .bind(end)
            .bind(&bean.id)
            .bind(batch)
            .execute(&db)
            .await
            .unwrap();
        }
        let o = options(Units::C, Paper::Letter);
        let svc = RoastedService::new(db.clone());
        let at = |d: &str| Some(d.to_string());
        for req in [
            RoastedMoveRequest {
                kind: "packed".into(),
                bags: Some(2.0),
                bag_g: Some(100.0),
                grind: Some("espresso".into()),
                note: Some("farmers market".into()),
                at: at("2026-10-20"),
                ..Default::default()
            },
            RoastedMoveRequest {
                kind: "given".into(),
                grams: Some(100.0),
                recipient: Some("Sam".into()),
                at: at("2026-10-21"),
                ..Default::default()
            },
            RoastedMoveRequest {
                kind: "used".into(),
                grams: Some(36.0),
                at: at("2026-10-22"),
                ..Default::default()
            },
        ] {
            svc.add_move("r2", req, o.zone, o.now).await.unwrap();
        }
        // A label run with three of #12's labels
        let item = RunItem {
            roast: Some("r2".into()),
            name: "Guji 12 (#12)".into(),
            design: "plain".into(),
            count: 3,
            params: serde_json::Map::new(),
            brand: None,
            data: json!({}),
            brand_value: json!({}),
            source: String::new(),
            url: None,
        };
        crate::labels::runs::insert(
            &db,
            crate::labels::runs::NewRun {
                id: "run1",
                by: "owner",
                sheet: "5163",
                sheet_name: "Avery 5163",
                start: 1,
                labels: 3,
                pages: 1,
                items: &[item],
                pdf: b"%PDF",
                reprint_of: None,
                note: Some("market"),
                printer_id: None,
            },
        )
        .await
        .unwrap();

        // The bean: its purchase, both roasts, and #12's bags, gift, use and labels
        let t = of_bean(&db, &bean.id, o.zone, o.now)
            .await
            .unwrap()
            .unwrap();
        let c = t.coffee.as_ref().unwrap();
        assert_eq!(c.lot.as_deref(), Some("HMB-2026-14"));
        assert_eq!(c.supplier.as_deref(), Some("Sweet Maria's"));
        assert_eq!(c.purchases.len(), 1);
        assert_eq!(c.purchases[0].grams, 2000.0);
        let ids: Vec<&str> = t.roasts.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["r1", "r2"]);
        let r = &t.roasts[1];
        assert_eq!(r.loss_pct, Some(14.0));
        assert_eq!(r.packed_g, 200.0);
        assert_eq!(r.given_g, 100.0);
        assert_eq!(r.used_g, 36.0);
        assert_eq!(r.left_g, Some(94.0));
        let packed = r.moves.iter().find(|m| m.kind == "packed").unwrap();
        assert_eq!(packed.grind.as_deref(), Some("espresso"));
        assert_eq!(r.label_runs.len(), 1);
        assert_eq!(r.label_runs[0].labels, 3);
        assert!(t.roasts[0].label_runs.is_empty());
        // The JSON says no price anywhere
        let text = serde_json::to_string(&t).unwrap();
        assert!(!text.contains("price") && !text.contains("cost"), "{text}");

        // A roast leads back to the same green lot
        let back = of_roast(&db, "r2", o.zone, o.now).await.unwrap().unwrap();
        assert_eq!(back.roasts.len(), 1);
        assert_eq!(back.coffee.unwrap().lot.as_deref(), Some("HMB-2026-14"));
        assert!(
            of_roast(&db, "nope", o.zone, o.now)
                .await
                .unwrap()
                .is_none()
        );

        // The report: both subjects lay out, pass the report check
        for (name, t) in [("bean", t), ("roast", back_trace(&db, &o).await)] {
            let d = build(&t, &o);
            assert_eq!(d["schema"], "roastwarden.report.trace/1");
            let r = Renderer::in_process();
            let pdf = render(&r, kind("trace").unwrap(), d.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            assert_eq!(pdf.pages, Some(1), "{name}");
            crate::reports::check::assert_clean("trace", name, &d);
            if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                let png = render(&r, kind("trace").unwrap(), d, Format::Png, 1, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/trace-{name}-1.png"), &png.bytes).unwrap();
            }
        }
    }

    async fn back_trace(db: &SqlitePool, o: &Options) -> Trace {
        of_roast(db, "r2", o.zone, o.now).await.unwrap().unwrap()
    }
}
