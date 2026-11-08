//! Green inventory (`assets/reports/inventory.typ`, schema `roastwarden.report.inventory/1`):
//! the coffee in stock, what it cost and is worth, how old it is, how fast it goes and when
//! each runs out at that pace; or a stock-take sheet to weigh against.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use super::{ahead, long_date, meta, money, past, short_date, Options, ReportError, DASH};
use crate::coffee::inventory::{Bean, InventoryService, PlannedRoast, StockMove};

/// Days of roasting the pace is taken from
const USE_DAYS: i64 = 90;
/// At most this many coffees in the table
pub const MAX_ROWS: usize = 500;

pub struct Input {
    pub beans: Vec<Bean>,
    /// Every stock move, oldest first
    pub moves: Vec<StockMove>,
    /// Roasts planned and not yet roasted
    pub plans: Vec<PlannedRoast>,
}

pub async fn gather(db: &SqlitePool) -> Result<Input, ReportError> {
    let inv = InventoryService::new(db.clone());
    let beans = inv.list_beans(false).await?;
    let moves = sqlx::query_as::<_, StockMove>("SELECT * FROM bean_stock_moves ORDER BY at, rowid")
        .fetch_all(db)
        .await?;
    let plans = inv.list_plans(None, false).await?;
    Ok(Input {
        beans,
        moves,
        plans,
    })
}

/// "2 days", "7 months", "1 year"
fn age(days: i64) -> String {
    match days {
        d if d < 1 => "today".into(),
        1 => "1 day".into(),
        d if d < 60 => format!("{d} days"),
        d if d < 730 => format!("{} months", d / 30),
        d => format!("{} years", d / 365),
    }
}

/// kg with two decimals: `0.45`
fn kg(g: f64) -> String {
    format!("{:.2}", g / 1000.0)
}

pub struct Row {
    pub bean: Bean,
    pub bought: Option<DateTime<Utc>>,
    pub crop: Option<String>,
    pub lot: Option<String>,
    pub planned_g: f64,
    /// Green roasted per day, over the last 90 days or since bought
    pub per_day_g: Option<f64>,
    /// When it runs out at that pace, after the planned roasts
    pub runs_out: Option<DateTime<Utc>>,
}

/// The coffees in stock with what the report says of each, soonest to run out first
pub fn rows(i: &Input, now: DateTime<Utc>) -> Vec<Row> {
    let mut by_bean: HashMap<&str, Vec<&StockMove>> = HashMap::new();
    for m in &i.moves {
        by_bean.entry(m.bean_id.as_str()).or_default().push(m);
    }
    let mut planned: HashMap<&str, f64> = HashMap::new();
    for p in &i.plans {
        if let (Some(b), Some(g)) = (p.bean_id.as_deref(), p.green_weight_g) {
            *planned.entry(b).or_default() += f64::from(g);
        }
    }
    let mut out: Vec<Row> = i
        .beans
        .iter()
        .filter(|b| b.stock_g > 0.5)
        .map(|b| {
            let moves = by_bean.get(b.id.as_str()).cloned().unwrap_or_default();
            let last_buy = moves.iter().rev().find(|m| m.kind == "purchase");
            let first = moves.first().map_or(b.created_at, |m| m.at);
            let since = now - Duration::days(USE_DAYS);
            let window_start = if first > since { first } else { since };
            let days = ((now - window_start).num_hours() as f64 / 24.0).max(7.0);
            let roasted: f64 = moves
                .iter()
                .filter(|m| m.kind == "roast" && m.at >= window_start)
                .map(|m| -f64::from(m.delta_g))
                .sum();
            let per_day = (roasted > 0.0).then(|| roasted / days);
            let planned_g = planned.get(b.id.as_str()).copied().unwrap_or(0.0);
            let left = f64::from(b.stock_g) - planned_g;
            let runs_out =
                per_day.map(|d| now + Duration::seconds((left.max(0.0) / d * 86_400.0) as i64));
            Row {
                bean: b.clone(),
                bought: last_buy.map(|m| m.at),
                crop: last_buy.and_then(|m| m.crop_year.clone()),
                lot: b.lot.clone(),
                planned_g,
                per_day_g: per_day,
                runs_out,
            }
        })
        .collect();
    out.sort_by(|a, b| match (a.runs_out, b.runs_out) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.bean.name.to_lowercase().cmp(&b.bean.name.to_lowercase()),
    });
    out.truncate(MAX_ROWS);
    out
}

const REPORT_NOTE: &str = "Use: green roasted in the last 90 days (or since the coffee came in, at least a week), per week; it runs out at that pace, after the planned roasts. Age from the latest purchase: green keeps its best for about six months, up to a year when cool and dry (\"ageing\" from 6 months, \"old\" from 12). Cost per kg is the weighted average of what is left, shipping and fees in; the value is the stock at that cost.";
const COUNT_NOTE: &str = "Enter each count on Beans → the coffee → Counted (g) → Set stock, or with the agent command roast bean stock set: a correction changes the stock, not the cost per kg.";

pub fn build(i: &Input, count: bool, o: &Options) -> Value {
    let kind = super::kind("inventory").expect("the inventory kind");
    let rows = rows(i, o.now);
    let today = o.local(o.now);
    let sub = |r: &Row| {
        [r.bean.supplier.clone(), r.lot.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    };
    if count {
        let list: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "coffee": r.bean.name,
                    "sub": sub(r),
                    "grams": format!("{:.0}", r.bean.stock_g),
                })
            })
            .collect();
        let mut m = meta(kind, &short_date(&today), o);
        m["title"] = json!("Stock-take");
        return json!({
            "schema": kind.schema,
            "meta": m,
            "mode": "count",
            "as_of": long_date(&today),
            "rows": list,
            "notes": COUNT_NOTE,
        });
    }
    let stock: f64 = rows.iter().map(|r| f64::from(r.bean.stock_g)).sum();
    let planned: f64 = rows.iter().map(|r| r.planned_g).sum();
    let value: f64 = rows.iter().filter_map(|r| r.bean.stock_value).sum();
    let unvalued = rows.iter().filter(|r| r.bean.stock_value.is_none()).count();
    let low: Vec<&Row> = rows.iter().filter(|r| r.bean.low).collect();
    let first = rows.iter().find(|r| r.runs_out.is_some());
    let list: Vec<Value> = rows
        .iter()
        .map(|r| {
            // Calendar days in the reader's time zone: bought yesterday is a day old
            let today = o.local(o.now).date_naive();
            let days = r.bought.map(|b| (today - o.local(b).date_naive()).num_days());
            json!({
                "coffee": r.bean.name,
                "sub": sub(r),
                "bought": r.bought.map_or(DASH.into(), |b| past(&o.local(b), &o.local(o.now))),
                "age": days.map_or(String::new(), age),
                "age_mark": days.and_then(|d| if d >= 365 { Some("old") } else if d >= 182 { Some("ageing") } else { None }),
                "crop": r.crop.clone().unwrap_or_else(|| DASH.into()),
                "stock": kg(f64::from(r.bean.stock_g)),
                "planned": if r.planned_g > 0.0 { kg(r.planned_g) } else { DASH.into() },
                "cost_kg": r.bean.cost_per_kg.map_or(DASH.into(), |c| money(&o.currency, c)),
                "value": r.bean.stock_value.map_or(DASH.into(), |v| money(&o.currency, v)),
                "use": r.per_day_g.map_or(DASH.into(), |d| format!("{:.0} g/wk", d * 7.0)),
                "runs_out": r.runs_out.map_or(DASH.into(), |t| ahead(&o.local(t), &o.local(o.now))),
                "low": r.bean.low,
            })
        })
        .collect();
    let summary = vec![
        json!({ "label": "In stock", "value": format!("{} kg", kg(stock)),
                "sub": format!("{} coffee{}", rows.len(), if rows.len() == 1 { "" } else { "s" }) }),
        json!({ "label": "Worth", "value": money(&o.currency, value),
                "sub": if unvalued > 0 { format!("{unvalued} without a price") } else { "at what it cost".into() } }),
        match first {
            Some(r) => {
                let t = r.runs_out.expect("found by it");
                json!({ "label": "Runs out first", "value": r.bean.name, "small": true,
                        "sub": format!("about {}, in {} days", ahead(&o.local(t), &o.local(o.now)), (t - o.now).num_days().max(0)) })
            }
            None => {
                json!({ "label": "Runs out first", "value": DASH, "sub": "nothing roasted lately" })
            }
        },
        json!({ "label": "Below the low mark", "small": !low.is_empty(),
                "value": if low.is_empty() { "none".to_string() } else { low.iter().map(|r| r.bean.name.as_str()).collect::<Vec<_>>().join(", ") },
                "sub": if low.is_empty() { "every coffee above it".to_string() } else { format!("{} coffee{}", low.len(), if low.len() == 1 { "" } else { "s" }) } }),
    ];
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &short_date(&today), o),
        "mode": "report",
        "as_of": long_date(&today),
        "summary": summary,
        "rows": list,
        "totals": { "stock": kg(stock), "planned": if planned > 0.0 { kg(planned) } else { DASH.into() }, "value": money(&o.currency, value) },
        "notes": REPORT_NOTE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::inventory::{BeanRequest, PurchaseFields, StockRequest};
    use crate::labels::Renderer;
    use crate::reports::roast::tests::options;
    use crate::reports::{kind, render, Format, Paper, Units};

    /// Three coffees: Kenya roasted 700 g over the last 70 days (10 g a day), 2 kg bought on
    /// 1 March at $40 with 300 g planned; Brazil never roasted; Old lot at 0 (left out).
    async fn world() -> (SqlitePool, Input) {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        let now: DateTime<Utc> = "2026-10-26T13:40:00Z".parse().unwrap();
        let kenya = inv
            .create_bean(BeanRequest {
                name: Some("Kenya Nyeri AA".into()),
                supplier: Some(Some("Sweet Maria's".into())),
                lot: Some(Some("SM-1".into())),
                low_stock_g: Some(500.0),
                ..Default::default()
            })
            .await
            .unwrap();
        inv.add_stock(
            &kenya.id,
            StockRequest {
                delta_g: Some(2000.0),
                purchase: PurchaseFields {
                    price: Some(40.0),
                    crop_year: Some("2025/26".into()),
                    at: Some("2026-03-01".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for k in 0..7 {
            let at = now - Duration::days(10 * k + 5);
            sqlx::query("INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind) VALUES (?, ?, ?, -100, 'roast')")
                .bind(format!("r{k}"))
                .bind(&kenya.id)
                .bind(at)
                .execute(&db)
                .await
                .unwrap();
        }
        inv.create_bean(BeanRequest {
            name: Some("Brazil Cerrado".into()),
            stock_g: Some(1000.0),
            ..Default::default()
        })
        .await
        .unwrap();
        inv.create_bean(BeanRequest {
            name: Some("Old lot".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        inv.create_plan(crate::coffee::inventory::PlanRequest {
            bean_id: Some(Some(kenya.id.clone())),
            green_weight_g: Some(Some(300.0)),
            ..Default::default()
        })
        .await
        .unwrap();
        let input = gather(&db).await.unwrap();
        (db, input)
    }

    #[tokio::test]
    async fn rows_run_out_at_the_recent_pace_after_the_plan() {
        let (_db, i) = world().await;
        let o = options(Units::C, Paper::Letter);
        let d = build(&i, false, &o);
        let rows = d["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "the empty lot is left out");
        // Kenya first: it runs out; 1300 g − 300 planned at 700 g / 90 days
        assert_eq!(rows[0]["coffee"], "Kenya Nyeri AA");
        assert_eq!(rows[0]["sub"], "Sweet Maria's · SM-1");
        assert_eq!(rows[0]["stock"], "1.30");
        assert_eq!(rows[0]["planned"], "0.30");
        assert_eq!(rows[0]["use"], "54 g/wk");
        assert_eq!(rows[0]["crop"], "2025/26");
        assert_eq!(rows[0]["age_mark"], "ageing", "bought 1 March");
        assert_eq!(rows[0]["cost_kg"], "$20.00");
        assert_eq!(rows[0]["value"], "$26.00");
        // 1000 g at 7.78 g a day: 128 days, early March
        assert_eq!(rows[0]["runs_out"], "3 Mar");
        assert_eq!(rows[1]["runs_out"], "–");
        assert_eq!(d["totals"]["stock"], "2.30");
        assert_eq!(d["summary"][2]["value"], "Kenya Nyeri AA");
        assert_eq!(d["summary"][1]["sub"], "1 without a price");
        let c = build(&i, true, &o);
        assert_eq!(c["meta"]["title"], "Stock-take");
        assert_eq!(c["rows"][0]["grams"], "1300");
    }

    #[tokio::test]
    async fn the_report_and_the_count_sheet_lay_out() {
        let (_db, i) = world().await;
        let r = Renderer::in_process();
        let k = kind("inventory").unwrap();
        let o = options(Units::C, Paper::Letter);
        let out = std::env::var("RR_REPORTS_OUT").ok();
        for (name, count) in [("inventory", false), ("stock-take", true)] {
            let data = build(&i, count, &o);
            let pdf = render(&r, k, data.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            assert_eq!(pdf.pages, Some(1), "{name}");
            crate::reports::check::assert_clean("inventory", name, &data);
            if let Some(dir) = &out {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(format!("{dir}/{name}.pdf"), &pdf.bytes).unwrap();
                let png = render(&r, k, data, Format::Png, 1, 110).await.unwrap();
                std::fs::write(format!("{dir}/{name}-1.png"), &png.bytes).unwrap();
            }
        }
    }
}
