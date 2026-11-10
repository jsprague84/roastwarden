//! The coffee report (`assets/reports/coffee.typ`, schema `roastwarden.report.coffee/2`): one
//! green coffee, what is known of it, its stock and cost, its roasts as a table and overlaid
//! on one chart with their RoR, its cuppings, and its ledger with a running balance.

use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::chart::{self, Point};
use super::{
    DASH, Options, ReportError, clock_opt, g1, grams, meta, money, n1, short_date, weight,
};
use crate::coffee::inventory::{BeanDetail, InventoryService};
use crate::profiles::history::ProfileHistoryRoast;
use crate::roasts::types::{CuppingWithAttributes, RoastEvent, RoastSession};

/// Roasts in the table, and overlaid on the chart
pub const MAX_ROASTS: usize = 60;
pub const MAX_OVERLAID: usize = 12;
/// Ledger lines
pub const MAX_MOVES: usize = 400;

/// One finished roast with what the reports draw of it
pub struct RoastLine {
    pub session: RoastSession,
    pub events: Vec<RoastEvent>,
    /// BT from charge, °C, thinned
    pub bt: Vec<Point>,
    /// RoR from 0:45 to 10 s before drop, °C/min (a ±15 s slope, as the roast report's)
    pub ror: Vec<Point>,
    pub cupping: Option<CuppingWithAttributes>,
    pub profile: Option<String>,
    pub coffee: Option<String>,
}

impl RoastLine {
    pub fn numbers(&self) -> ProfileHistoryRoast {
        ProfileHistoryRoast::from_roast(&self.session, &self.events)
    }
    /// `#5`, else the roast's name
    pub fn mark(&self) -> String {
        self.session
            .batch_number
            .map_or_else(|| self.session.name.clone(), |n| format!("#{n}"))
    }
}

/// A finished roast's line: its events, BT from charge (at most `points`), cupping, names
pub async fn line(
    db: &SqlitePool,
    s: RoastSession,
    points: usize,
) -> Result<RoastLine, ReportError> {
    let events = crate::roasts::events::get_roast_events(db, &s.id).await?;
    let tele = crate::roasts::sessions::get_session_with_telemetry(db, &s.id)
        .await?
        .map(|t| t.telemetry)
        .unwrap_or_default();
    let charge = events
        .iter()
        .find(|e| e.event_type == crate::roasts::types::RoastEventType::Charge)
        .map_or(0.0, |e| f64::from(e.elapsed_seconds));
    let end = events
        .iter()
        .find(|e| e.event_type == crate::roasts::types::RoastEventType::Drop)
        .map_or(f64::MAX, |e| f64::from(e.elapsed_seconds) - charge + 1.0);
    let mut bt: Vec<Point> = tele
        .iter()
        .filter_map(|p| {
            let t = f64::from(p.elapsed_seconds) - charge;
            let v = f64::from(p.bean_temp?);
            (v.is_finite() && (0.0..=end).contains(&t)).then_some([t, v])
        })
        .collect();
    bt.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let every = chart::every(&bt, 1.0);
    let last = every.last().map_or(0.0, |p| p[0]);
    let ror = ror_of(&every, end.min(last + 1.0) - 1.0);
    let bt = chart::lttb(&every, points);
    let cupping = crate::roasts::cupping::get_cupping(db, &s.id).await?;
    let profile: Option<String> = match &s.profile_id {
        Some(p) => {
            sqlx::query_scalar("SELECT name FROM roast_profiles WHERE id = ?")
                .bind(p)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    let coffee: Option<String> = match &s.bean_id {
        Some(b) => {
            sqlx::query_scalar("SELECT name FROM beans WHERE id = ?")
                .bind(b)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    Ok(RoastLine {
        session: s,
        events,
        bt,
        ror,
        cupping,
        profile,
        coffee,
    })
}

pub struct Input {
    pub detail: BeanDetail,
    /// Finished roasts, newest first
    pub roasts: Vec<RoastLine>,
}

pub async fn gather(db: &SqlitePool, id: &str) -> Result<Input, ReportError> {
    let Some(mut detail) = InventoryService::new(db.clone()).bean_detail(id).await? else {
        return Err(ReportError::NotFound(format!("no coffee {id}")));
    };
    let done: Vec<RoastSession> = std::mem::take(&mut detail.roasts)
        .into_iter()
        .filter(|s| s.status == crate::roasts::types::SessionStatus::Completed)
        .take(MAX_ROASTS)
        .collect();
    let mut roasts = Vec::with_capacity(done.len());
    for s in done {
        roasts.push(line(db, s, 160).await?);
    }
    Ok(Input { detail, roasts })
}

/// The rate of rise of BT points 1 s apart, from 0:45 to 10 s before `end`
pub fn ror_of(bt: &[Point], end: f64) -> Vec<Point> {
    chart::ror(bt, 15.0)
        .into_iter()
        .filter(|p| p[0] >= 45.0 && p[0] <= end - 10.0)
        .collect()
}

/// These roasts (newest first) on one chart from charge: each BT and RoR thin and grey, the
/// newest's in colour and named, with its first crack and drop marked (`overlay-chart`)
pub fn overlay(lines: &[&RoastLine], o: &Options) -> Value {
    let u = o.units;
    let x_end = lines
        .iter()
        .filter_map(|l| l.bt.last().map(|p| p[0]))
        .fold(60.0, f64::max);
    let x_max = (x_end / 60.0).ceil() * 60.0;
    let step = if x_max > 1200.0 { 120.0 } else { 60.0 };
    let x = chart::Axis {
        min: 0.0,
        max: x_max,
        ticks: (0..=(x_max / step) as usize)
            .map(|k| chart::Tick {
                v: k as f64 * step,
                label: format!("{}", (k as f64 * step / 60.0) as i64),
            })
            .collect(),
    };
    let hi = lines
        .iter()
        .flat_map(|l| l.bt.iter().map(|p| u.temp(p[1])))
        .fold(u.temp(100.0), f64::max);
    let y = chart::axis(0.0, hi + u.delta(5.0), 5, chart::plain);
    let rors: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| {
            chart::every(&l.ror, 3.0)
                .iter()
                .map(|p| [p[0], u.delta(p[1])])
                .collect()
        })
        .collect();
    let ror_hi = chart::quantile(rors.iter().flatten().map(|p| p[1]), 0.95)
        .unwrap_or(0.0)
        .max(u.delta(20.0));
    let y2 = chart::axis(0.0, ror_hi * 1.05, 6, chart::plain);
    let mut series = Vec::new();
    for (k, (l, ror)) in lines.iter().zip(&rors).enumerate().rev() {
        let newest = k == 0;
        let bt: Vec<Point> = l.bt.iter().map(|p| [p[0], u.temp(p[1])]).collect();
        let ror: Vec<Point> = ror.iter().copied().skip_while(|p| p[1] > y2.max).collect();
        series.push(json!({ "key": "bt", "label": newest.then(|| l.mark()), "newest": newest, "points": chart::rounded(&bt, 1) }));
        series.push(json!({ "key": "ror", "axis": "y2", "label": newest.then(|| format!("RoR {}", l.mark())), "newest": newest, "points": chart::rounded(&ror, 1) }));
    }
    let events: Vec<Value> = lines.first().map_or(vec![], |l| {
        let n = l.numbers();
        [("FC", n.fc_s), ("DROP", n.drop_s)]
            .into_iter()
            .filter_map(|(short, t)| t.map(|t| json!({ "short": short, "t": t })))
            .collect()
    });
    json!({
        "x": x, "y": y, "y2": y2,
        "x-name": "min", "y-name": u.temp_label(), "y2-name": u.rate_label(),
        "series": series, "events": events,
    })
}

/// The table's row for a roast
pub fn roast_row(l: &RoastLine, o: &Options) -> Value {
    let n = l.numbers();
    let started = o.local(l.session.start_time.unwrap_or(l.session.created_at));
    let dtr = n
        .development_s
        .zip(n.total_s)
        .filter(|(_, t)| *t > 0.0)
        .map(|(d, t)| f64::from(d / t * 100.0));
    json!({
        "mark": l.mark(),
        "date": short_date(&started),
        "name": l.session.name,
        "coffee": l.coffee.clone().unwrap_or_else(|| DASH.into()),
        "profile": l.profile.clone().unwrap_or_else(|| DASH.into()),
        "roaster": l.session.device_id,
        "green": l.session.green_weight.map_or(DASH.into(), |g| g1(f64::from(g))),
        "loss": n1(n.weight_loss_pct.map(f64::from)),
        "fc": clock_opt(n.fc_s.map(f64::from)),
        "fc_bt": o.temp(n.fc_bt.map(f64::from), 1),
        "drop": clock_opt(n.drop_s.map(f64::from)),
        "drop_bt": o.temp(n.drop_bt.map(f64::from), 1),
        "dev": clock_opt(n.development_s.map(f64::from)),
        "dtr": n1(dtr),
        "rmse": o.delta(n.rmse.map(f64::from), 1),
        "cup": l.cupping.as_ref().and_then(|c| c.cupping.final_score).map_or(DASH.into(), |f| format!("{f:.2}")),
    })
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("coffee").expect("the coffee kind");
    let b = &i.detail.bean;
    let mut moves: Vec<_> = i.detail.moves.iter().collect();
    moves.reverse();
    let last_buy = moves.iter().rev().find(|m| m.kind == "purchase");
    let fact = |label: &str, v: Option<String>| {
        v.filter(|v| !v.trim().is_empty())
            .map(|v| json!({ "label": label, "value": v }))
    };
    let facts: Vec<Value> = [
        fact("Origin", b.origin.clone()),
        fact("Region", b.region.clone()),
        fact("Producer", b.producer.clone()),
        fact("Variety", b.variety.clone()),
        fact("Process", b.process.clone()),
        fact("Drying", b.drying.clone()),
        fact("Altitude", b.altitude.clone()),
        fact("Grade", b.grade.clone()),
        fact("Screen", b.screen_size.clone()),
        fact("Certified", b.certifications.clone()),
        fact("Crop", last_buy.and_then(|m| m.crop_year.clone())),
        fact("Arrived", last_buy.and_then(|m| m.arrival.clone())),
        fact("Supplier", b.supplier.clone()),
        fact("Lot", b.lot.clone()),
        fact("Their score", b.score.map(|s| format!("{s:.1}"))),
        fact("Moisture", b.moisture_pct.map(|m| format!("{m:.1} %"))),
        fact("Density", b.density_g_l.map(|d| format!("{d:.0} g/L"))),
        fact("Roast", b.roast_recommendation.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let bought: f64 = moves
        .iter()
        .filter(|m| m.kind == "purchase")
        .map(|m| f64::from(m.delta_g))
        .sum();
    let purchases = moves.iter().filter(|m| m.kind == "purchase").count();
    let spent: f64 = moves
        .iter()
        .filter(|m| m.kind == "purchase")
        .filter_map(|m| m.cost)
        .sum();
    let roasted: f64 = i
        .roasts
        .iter()
        .filter_map(|l| l.session.green_weight.map(f64::from))
        .sum();
    let summary = vec![
        json!({ "label": "In stock", "value": weight(f64::from(b.stock_g).max(0.0)),
                "sub": if b.low { format!("below its low mark of {} g", grams(f64::from(b.low_stock_g))) } else { format!("low mark {} g", grams(f64::from(b.low_stock_g))) },
                "state": if b.low { "below" } else { "within" } }),
        json!({ "label": "Cost", "value": b.cost_per_kg.map_or(DASH.into(), |c| format!("{}/kg", money(&o.currency, c))),
                "sub": match (b.stock_value, b.cost_per_kg) {
                    (Some(v), _) => format!("stock worth {}", money(&o.currency, v)),
                    (None, Some(_)) if b.stock_g <= 0.0 => "none in stock".into(),
                    _ => "no priced purchase".into(),
                } }),
        json!({ "label": "Bought", "value": weight(bought),
                "sub": format!("{purchases} purchase{}{}", if purchases == 1 { "" } else { "s" },
                               if spent > 0.0 { format!(", {}", money(&o.currency, spent)) } else { String::new() }) }),
        json!({ "label": "Roasted", "value": format!("{} roast{}", i.roasts.len(), if i.roasts.len() == 1 { "" } else { "s" }),
                "sub": format!("{} of green", weight(roasted)) }),
    ];
    let rows: Vec<Value> = i.roasts.iter().map(|l| roast_row(l, o)).collect();
    let overlaid: Vec<&RoastLine> = i.roasts.iter().take(MAX_OVERLAID).collect();
    let cuppings: Vec<Value> = i
        .roasts
        .iter()
        .filter_map(|l| {
            let c = l.cupping.as_ref()?;
            Some(json!({
                "mark": l.mark(),
                "final": c.cupping.final_score.map_or(DASH.into(), |f| format!("{f:.2}")),
                "band": c.band.clone().unwrap_or_else(|| "not all ten scored".into()),
                "notes": c.cupping.notes,
            }))
        })
        .collect();
    let kinds = |k: &str| match k {
        "purchase" => "Bought",
        "adjust" => "Correction",
        "roast" => "Roasted",
        _ => "Change",
    };
    let mut balance = 0.0;
    let ledger: Vec<Value> = moves
        .iter()
        .map(|m| {
            balance += f64::from(m.delta_g);
            let what = match (m.kind.as_str(), &m.session_id) {
                ("roast", Some(sid)) => i
                    .roasts
                    .iter()
                    .find(|l| &l.session.id == sid)
                    .map_or("Roasted".into(), |l| format!("Roasted {}", l.mark())),
                (k, _) => kinds(k).to_string(),
            };
            json!({
                "date": short_date(&o.local(m.at)),
                "what": what,
                "note": m.note.clone().or_else(|| m.supplier.clone()),
                "change": format!("{}{}", if m.delta_g > 0.0 { "+" } else { "−" }, g1(f64::from(m.delta_g.abs()))),
                "balance": g1(balance.max(0.0)),
                "cost": m.cost.map_or(DASH.into(), |c| money(&o.currency, c)),
                "per_kg": m.cost_per_kg.map_or(DASH.into(), |c| money(&o.currency, c)),
            })
        })
        .collect();
    let skip = ledger.len().saturating_sub(MAX_MOVES);
    let place: Vec<String> = [b.origin.clone(), b.region.clone(), b.process.clone()]
        .into_iter()
        .flatten()
        .collect();
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &b.name, o),
        "coffee": {
            "name": b.name,
            "line": place.join(" · "),
            "notes": b.cupping_notes,
            "own_notes": b.notes,
            "url": b.url,
        },
        "facts": facts,
        "summary": summary,
        "roasts": rows,
        "overlay": overlay(&overlaid, o),
        "overlaid": overlaid.len(),
        "more_roasts": i.roasts.len().saturating_sub(MAX_OVERLAID),
        "cuppings": cuppings,
        "ledger": ledger[skip..].to_vec(),
        "ledger_cut": skip,
        "notes": format!("Times from charge (mm:ss), temperatures {}; loss is green less roasted; DTR is development's share of the roast. Cost per kg is the weighted average of what was left after each move, shipping and fees in.", o.units.temp_label()),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::coffee::inventory::{BeanRequest, PurchaseFields, StockRequest};
    use crate::labels::Renderer;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};
    use crate::roasts::types::RoastEventType;

    /// Batch #5 and two variants of it (#4 a little faster, #3 slower), newest first
    pub(crate) fn lines() -> Vec<RoastLine> {
        [(5, 0.0, 85.7), (4, -8.0, 85.75), (3, 14.0, 85.33)]
            .into_iter()
            .map(|(batch, shift, out)| {
                let f = fixture();
                let mut s = f.session.clone();
                s.id = format!("roast-{batch}");
                s.batch_number = Some(batch);
                s.roasted_weight = Some(out);
                s.weight_loss_pct = Some(100.0 - out);
                let mut events = f.events.clone();
                for e in &mut events {
                    if e.event_type == RoastEventType::FirstCrackStart
                        || e.event_type == RoastEventType::Drop
                    {
                        e.elapsed_seconds += shift;
                    }
                }
                let charge = 7.2;
                // The same curve run a little faster or slower, so the overlay shows them apart
                let stretch = 1.0 + f64::from(shift) / 606.0;
                let bt: Vec<Point> = f
                    .telemetry
                    .iter()
                    .filter_map(|p| {
                        Some([
                            (f64::from(p.elapsed_seconds) - charge) * stretch,
                            f64::from(p.bean_temp?),
                        ])
                    })
                    .filter(|p| p[0] >= 0.0 && p[0] <= 606.0 + f64::from(shift))
                    .collect();
                let every = chart::every(&bt, 1.0);
                RoastLine {
                    session: s,
                    events,
                    ror: ror_of(&every, 606.0 + f64::from(shift)),
                    bt: chart::lttb(&every, 160),
                    cupping: None,
                    profile: Some("Fluid bed — Light espresso".into()),
                    coffee: Some("Palmichal Vecindario".into()),
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn a_coffee_with_its_roasts_and_ledger() {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        let b = inv
            .create_bean(BeanRequest {
                name: Some("Palmichal Vecindario".into()),
                origin: Some(Some("Costa Rica".into())),
                region: Some(Some("Zarcero, West Valley".into())),
                process: Some(Some("Honey".into())),
                supplier: Some(Some("Sweet Maria's".into())),
                ..Default::default()
            })
            .await
            .unwrap();
        inv.add_stock(
            &b.id,
            StockRequest {
                delta_g: Some(1000.0),
                purchase: PurchaseFields {
                    price: Some(21.5),
                    crop_year: Some("2026".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for k in 0..3 {
            sqlx::query("INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind, session_id) VALUES (?, ?, ?, -100, 'roast', ?)")
                .bind(format!("r{k}"))
                .bind(&b.id)
                .bind(chrono::Utc::now() + chrono::Duration::seconds(k + 1))
                .bind(format!("roast-{}", 3 + k))
                .execute(&db)
                .await
                .unwrap();
        }
        let detail = inv.bean_detail(&b.id).await.unwrap().unwrap();
        let input = Input {
            detail,
            roasts: lines(),
        };
        let o = options(Units::C, Paper::Letter);
        let d = build(&input, &o);
        assert_eq!(d["meta"]["subject"], "Palmichal Vecindario");
        assert_eq!(
            d["coffee"]["line"],
            "Costa Rica · Zarcero, West Valley · Honey"
        );
        assert_eq!(d["summary"][0]["value"], "700 g");
        assert_eq!(d["summary"][1]["sub"], "stock worth $15.05");
        assert_eq!(d["summary"][2]["value"], "1.00 kg");
        assert_eq!(d["summary"][2]["sub"], "1 purchase, $21.50");
        assert_eq!(d["roasts"][0]["mark"], "#5");
        assert_eq!(d["roasts"][1]["fc"], "7:21", "#4's first crack 8 s earlier");
        // Each roast's BT and RoR, the newest last (drawn over the rest) and named
        let series = d["overlay"]["series"].as_array().unwrap();
        assert_eq!(series.len(), 6);
        assert_eq!(series[4]["label"], "#5");
        assert_eq!(series[5]["label"], "RoR #5");
        assert_eq!(series[5]["axis"], "y2");
        assert!(series[0]["label"].is_null());
        assert_eq!(d["overlay"]["events"][0]["short"], "FC");
        assert_eq!(d["roasts"][0]["name"], d["roasts"][1]["name"]);
        let ledger = d["ledger"].as_array().unwrap();
        assert_eq!(ledger.len(), 4);
        assert_eq!(
            (ledger[0]["what"].as_str(), ledger[0]["balance"].as_str()),
            (Some("Bought"), Some("1000.0"))
        );
        assert_eq!(ledger[1]["what"], "Roasted #3");
        assert_eq!(ledger[3]["balance"], "700.0");
        assert_eq!(d["roasts"][0]["green"], "100.0", "one decimal in a table");
        assert!(
            d["facts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["label"] == "Crop" && f["value"] == "2026")
        );

        let r = Renderer::in_process();
        let pdf = render(&r, kind("coffee").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert!(pdf.pages.unwrap() <= 2, "{:?} pages", pdf.pages);
        crate::reports::check::assert_clean("coffee", "coffee", &d);
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/coffee.pdf"), &pdf.bytes).unwrap();
            for p in 1..=pdf.pages.unwrap() {
                let png = render(&r, kind("coffee").unwrap(), d.clone(), Format::Png, p, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/coffee-{p}.png"), &png.bytes).unwrap();
            }
        }

        // All of it roasted: the price is known, there is just none left (R3)
        sqlx::query("INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind) VALUES ('r9', ?, ?, -700, 'adjust')")
            .bind(&b.id)
            .bind(chrono::Utc::now() + chrono::Duration::seconds(10))
            .execute(&db)
            .await
            .unwrap();
        let detail = inv.bean_detail(&b.id).await.unwrap().unwrap();
        // Twelve roasts on one chart on A4: no half-empty page (R2)
        let roasts: Vec<RoastLine> = (0..4)
            .flat_map(|k| {
                lines().into_iter().map(move |mut l| {
                    let n = l.session.batch_number.unwrap_or(0) + 3 * (3 - k);
                    l.session.id = format!("roast-{n}");
                    l.session.batch_number = Some(n);
                    l
                })
            })
            .collect();
        let d = build(&Input { detail, roasts }, &options(Units::F, Paper::A4));
        assert_eq!(d["overlaid"], 12);
        assert_eq!(d["overlay"]["series"].as_array().unwrap().len(), 24);
        assert_eq!(d["summary"][0]["value"], "0 g");
        assert_eq!(d["summary"][1]["value"], "$21.50/kg");
        assert_eq!(d["summary"][1]["sub"], "none in stock");
        let pdf = render(&r, kind("coffee").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            for p in 1..=pdf.pages.unwrap() {
                let png = render(&r, kind("coffee").unwrap(), d.clone(), Format::Png, p, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/coffee-a4f-{p}.png"), &png.bytes).unwrap();
            }
        }
        assert!(pdf.pages.unwrap() <= 2, "{:?} pages", pdf.pages);
        crate::reports::check::assert_clean("coffee", "coffee at 0 g, twelve roasts, A4 °F", &d);
    }
}
