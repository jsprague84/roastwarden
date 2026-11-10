//! The compare report (`assets/reports/compare.typ`, schema `roastwarden.report.compare/1`):
//! two roasts as an A/B experiment, from `roast compare`'s numbers (`analysis.rs`). BT and
//! RoR of both on one chart, heater and fan under it, the phases stacked on one time scale,
//! every number side by side with B − A, and how closely B's BT followed A's.

use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::chart::{self, Point};
use super::roast::{self, Input as RoastInput};
use super::{DASH, Options, ReportError, clock, hm, meta, n1, short_date};
use crate::analysis::{self, Comparison, Row};
use crate::roasts::types::{RoastEventType, SessionTelemetry};

/// The rate of rise starts this long after charge, as on the roast report
const ROR_FROM: f64 = 45.0;

/// Two roasts: A, the one B is measured against, and B.
pub struct Input {
    pub a: RoastInput,
    pub b: RoastInput,
    /// How close BT counts as together, °C
    pub within: f64,
}

/// The roasts to compare: two ids (A then B); one (B, against the finished roast before it on
/// its roaster); none (the newest two finished roasts, on `roaster` when given).
pub async fn gather(
    db: &SqlitePool,
    ids: &[String],
    roaster: Option<&str>,
    within: f64,
) -> Result<Input, ReportError> {
    if !(within.is_finite() && within > 0.0 && within <= 20.0) {
        return Err(ReportError::Bad(
            "within is a tolerance in °C, above 0 and at most 20".into(),
        ));
    }
    let (a, b) = match ids {
        [a, b] if a == b => {
            return Err(ReportError::Bad(
                "the two roasts are the same roast: name two".into(),
            ));
        }
        [a, b] => (a.clone(), b.clone()),
        [b] => {
            let row: Option<(
                String,
                Option<chrono::DateTime<chrono::Utc>>,
                chrono::DateTime<chrono::Utc>,
            )> = sqlx::query_as(
                "SELECT device_id, start_time, created_at FROM roast_sessions WHERE id = ?",
            )
            .bind(b)
            .fetch_optional(db)
            .await?;
            let Some((device, start, created)) = row else {
                return Err(ReportError::NotFound(format!("no roast {b}")));
            };
            let before: Option<String> = sqlx::query_scalar(
                "SELECT id FROM roast_sessions WHERE status = 'completed' AND device_id = ?
                 AND id != ? AND COALESCE(start_time, created_at) < ?
                 ORDER BY COALESCE(start_time, created_at) DESC LIMIT 1",
            )
            .bind(&device)
            .bind(b)
            .bind(start.unwrap_or(created))
            .fetch_optional(db)
            .await?;
            let Some(a) = before else {
                return Err(ReportError::NotFound(format!(
                    "no finished roast on {device} before this one to compare it with: name two"
                )));
            };
            (a, b.clone())
        }
        [] => {
            let newest: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM roast_sessions WHERE status = 'completed'
                 AND (? IS NULL OR device_id = ?)
                 ORDER BY COALESCE(start_time, created_at) DESC LIMIT 2",
            )
            .bind(roaster)
            .bind(roaster)
            .fetch_all(db)
            .await?;
            match newest.as_slice() {
                [b, a] => (a.clone(), b.clone()),
                _ => {
                    return Err(ReportError::NotFound(
                        "fewer than two finished roasts to compare".into(),
                    ));
                }
            }
        }
        _ => {
            return Err(ReportError::Bad(format!(
                "a compare report is of two roasts, A and B (got {}); `roast compare` lines up more",
                ids.len()
            )));
        }
    };
    Ok(Input {
        a: roast::gather(db, &a).await?,
        b: roast::gather(db, &b).await?,
        within,
    })
}

/// A roast's curves from charge, °C, as the roast report draws them
struct Curves {
    end: f64,
    bt: Vec<Point>,
    ror: Vec<Point>,
    heater: Vec<Point>,
    fan: Vec<Point>,
    marks: Vec<(&'static str, f64)>,
}

fn curves(i: &RoastInput) -> Curves {
    let ev = |t: RoastEventType| {
        i.events
            .iter()
            .find(|e| e.event_type == t)
            .map(|e| f64::from(e.elapsed_seconds))
    };
    let charge = ev(RoastEventType::Charge).unwrap_or(0.0);
    let mut samples: Vec<&SessionTelemetry> = i.telemetry.iter().collect();
    samples.sort_by(|a, b| a.elapsed_seconds.total_cmp(&b.elapsed_seconds));
    let last = samples
        .last()
        .map(|p| f64::from(p.elapsed_seconds) - charge);
    let drop = ev(RoastEventType::Drop).map(|d| d - charge);
    let end = drop.or(last).unwrap_or(0.0).max(1.0);
    let series = |pick: &dyn Fn(&SessionTelemetry) -> Option<f64>| -> Vec<Point> {
        samples
            .iter()
            .filter_map(|p| {
                let t = f64::from(p.elapsed_seconds) - charge;
                let v = pick(p)?;
                (v.is_finite() && (0.0..=end + 1.0).contains(&t)).then_some([t, v])
            })
            .collect()
    };
    let bt = chart::every(&series(&|p| p.bean_temp.map(f64::from)), 1.0);
    let ror = chart::ror(&bt, 15.0)
        .into_iter()
        .filter(|p| p[0] >= ROR_FROM && p[0] <= end - 10.0)
        .collect();
    let marks = [
        (RoastEventType::DryingEnd, "DRY"),
        (RoastEventType::FirstCrackStart, "FC"),
        (RoastEventType::Drop, "DROP"),
    ]
    .into_iter()
    .filter_map(|(t, short)| ev(t).map(|at| (short, at - charge)))
    .collect();
    Curves {
        end,
        bt,
        ror,
        heater: chart::every(&chart::trailing_mean(&series(&|p| p.heater), 15.0), 3.0),
        fan: chart::every(&series(&|p| p.fan), 3.0),
        marks,
    }
}

/// `+0:12`, `−1.4`, `same`: B − A in the row's unit
fn signed(row: &Row, d: Option<f64>, o: &Options) -> String {
    let Some(d) = d.filter(|d| d.is_finite()) else {
        return DASH.into();
    };
    let digits = i32::from(row.unit != "s");
    let shown = match row.unit {
        "s" => d.round(),
        "°C" | "°C/min" | "°C/min/min" => o.units.delta(d),
        _ => d,
    };
    let rounded = (shown * 10f64.powi(digits)).round() / 10f64.powi(digits);
    if rounded == 0.0 {
        return "same".into();
    }
    let sign = if rounded > 0.0 { "+" } else { "−" };
    match row.unit {
        "s" => format!("{sign}{}", clock(rounded.abs())),
        _ => format!("{sign}{:.1}", rounded.abs()),
    }
}

/// Rows whose °C is a difference (BT − profile, RMS, bias), not a temperature
const DELTA_ROWS: [&str; 4] = [
    "bt_vs_profile_30",
    "early_peak",
    "profile_rmse",
    "profile_bias",
];

/// A value in the row's unit, as the page shows it
fn cell(row: &Row, v: Option<f64>, o: &Options) -> String {
    match row.unit {
        "s" => v.map_or(DASH.into(), clock),
        "°C" if DELTA_ROWS.contains(&row.key) => o.delta(v, 1),
        "°C" => o.temp(v, 1),
        "°C/min" | "°C/min/min" => o.delta(v, 1),
        _ => n1(v),
    }
}

/// The unit as the page names it, in the report's units
fn unit_label(row: &Row, o: &Options) -> &'static str {
    // Seconds, like any unit not named here, have no label
    match row.unit {
        "°C" => o.units.temp_label(),
        "°C/min" => o.units.rate_label(),
        "°C/min/min" => match o.units {
            super::Units::C => "°C/min²",
            super::Units::F => "°F/min²",
        },
        "%" => "%",
        _ => "",
    }
}

/// `#12`, else the roast's name
fn mark_of(i: &RoastInput) -> String {
    i.session
        .batch_number
        .map_or_else(|| i.session.name.clone(), |n| format!("#{n}"))
}

/// The green coffee's name, when the roast names one
fn coffee_of(i: &RoastInput) -> Option<String> {
    i.bean.as_ref().map(|b| b.name.clone())
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("compare").expect("the compare kind");
    let u = o.units;
    let list = [analysis::analyse(&i.a), analysis::analyse(&i.b)];
    let cmp: Comparison = analysis::compare(&list, 0, i.within);
    let (ca, cb) = (curves(&i.a), curves(&i.b));
    let (ra, rb) = (&cmp.roasts[0], &cmp.roasts[1]);

    // The chart: both roasts' BT and RoR, the profile (once when they share it)
    let end = ca.end.max(cb.end);
    let to_u = |pts: &[Point], f: &dyn Fn(f64) -> f64| -> Vec<Point> {
        pts.iter().map(|p| [p[0], f(p[1])]).collect()
    };
    let bt = |c: &Curves| chart::rounded(&chart::lttb(&to_u(&c.bt, &|v| u.temp(v)), 700), 1);
    let ror_of = |c: &Curves| chart::every(&to_u(&c.ror, &|v| u.delta(v)), 2.0);
    let (ror_a, ror_b) = (ror_of(&ca), ror_of(&cb));
    let x_max = ((end + 15.0) / 60.0).ceil() * 60.0;
    let step = if x_max > 20.0 * 60.0 { 120.0 } else { 60.0 };
    let x = chart::Axis {
        min: 0.0,
        max: x_max,
        ticks: (0..=((x_max / step) as usize))
            .map(|k| chart::Tick {
                v: k as f64 * step,
                label: format!("{}", (k as f64 * step / 60.0) as i64),
            })
            .collect(),
    };
    let profile = |p: &crate::profiles::types::ProfileWithPoints| -> Vec<Point> {
        p.points
            .iter()
            .filter(|q| f64::from(q.time_seconds) <= end + 30.0)
            .map(|q| [f64::from(q.time_seconds), u.temp(f64::from(q.target_temp))])
            .collect()
    };
    let mut profiles: Vec<(String, Vec<Point>)> = Vec::new();
    match (&i.a.profile, &i.b.profile) {
        (Some(pa), Some(pb)) if pa.profile.id == pb.profile.id => {
            profiles.push(("Profile".into(), profile(pa)));
        }
        (pa, pb) => {
            if let Some(p) = pa {
                profiles.push(("Profile A".into(), profile(p)));
            }
            if let Some(p) = pb {
                profiles.push(("Profile B".into(), profile(p)));
            }
        }
    }
    let hi = ca
        .bt
        .iter()
        .chain(&cb.bt)
        .map(|p| u.temp(p[1]))
        .chain(profiles.iter().flat_map(|p| p.1.iter().map(|q| q[1])))
        .fold(u.temp(100.0), f64::max);
    let y = chart::axis(0.0, hi + u.delta(5.0), 5, chart::plain);
    let ror_hi = chart::quantile(ror_a.iter().chain(&ror_b).map(|p| p[1]), 0.95)
        .unwrap_or(0.0)
        .max(u.delta(20.0));
    let y2 = chart::axis(0.0, ror_hi * 1.05, 6, chart::plain);
    let clip = |r: Vec<Point>| -> Vec<Point> {
        chart::rounded(
            &r.into_iter()
                .skip_while(|p| p[1] > y2.max)
                .collect::<Vec<_>>(),
            1,
        )
    };
    let mut series: Vec<Value> = profiles
        .iter()
        .map(|(label, pts)| json!({ "key": "profile", "label": label, "axis": "y", "points": chart::rounded(pts, 1) }))
        .collect();
    series.push(json!({ "key": "a", "label": "A", "axis": "y", "points": bt(&ca) }));
    series.push(json!({ "key": "b", "label": "B", "axis": "y", "points": bt(&cb) }));
    series.push(json!({ "key": "a_ror", "label": "RoR A", "axis": "y2", "points": clip(ror_a) }));
    series.push(json!({ "key": "b_ror", "label": "RoR B", "axis": "y2", "points": clip(ror_b) }));
    let events: Vec<Value> = [("A", "a", &ca), ("B", "b", &cb)]
        .into_iter()
        .flat_map(|(m, key, c)| {
            c.marks
                .iter()
                .filter(|(short, t)| *short != "DRY" && *t > 0.5 && *t <= x_max)
                .map(move |(short, t)| {
                    json!({ "key": format!("{short}-{key}"), "short": format!("{short} {m}"), "t": (t * 10.0).round() / 10.0, "of": key })
                })
        })
        .collect();
    let strip = |pick: &dyn Fn(&Curves) -> &Vec<Point>| -> Vec<Value> {
        [("a", "A", &ca), ("b", "B", &cb)]
            .into_iter()
            .filter(|(_, _, c)| !pick(c).is_empty())
            .map(|(key, label, c)| json!({ "key": key, "label": label, "tag": label, "points": chart::rounded(pick(c), 1) }))
            .collect()
    };
    let strips: Vec<Value> = [
        ("Heater %", strip(&|c| &c.heater)),
        ("Fan %", strip(&|c| &c.fan)),
    ]
    .into_iter()
    .filter(|(_, s)| !s.is_empty())
    .map(|(title, s)| json!({ "title": title, "series": s }))
    .collect();

    // The phases of each, on one time scale
    let phases: Vec<Value> = [("A", ra), ("B", rb)]
        .into_iter()
        .map(|(m, r)| {
            let parts: Vec<Value> = r
                .phases
                .iter()
                .map(|p| {
                    json!({
                        "key": p.key,
                        "name": match p.key { "drying" => "Drying", "maillard" => "Maillard", _ => "Development" },
                        "time": clock(p.duration_s),
                        "pct": format!("{:.1}", p.pct),
                        "frac": p.duration_s / end.max(1.0),
                    })
                })
                .collect();
            let shown: f64 = r.phases.iter().map(|p| p.duration_s).sum();
            json!({ "mark": m, "total": clock(r.total_s), "parts": parts, "rest": (1.0 - shown / end.max(1.0)).max(0.0) })
        })
        .collect();

    // Every number side by side: the start (to 2:00), then to drop
    let split = cmp
        .rows
        .iter()
        .position(|r| r.key == "drying_end_s")
        .unwrap_or(cmp.rows.len());
    let table = |rows: &[Row]| -> Vec<Value> {
        rows.iter()
            .map(|r| {
                json!({
                    "key": r.key,
                    "label": r.label,
                    "unit": unit_label(r, o),
                    "a": cell(r, r.values[0], o),
                    "b": cell(r, r.values[1], o),
                    "diff": signed(r, r.deltas[1], o),
                })
            })
            .collect()
    };
    let row = |key: &str| cmp.rows.iter().find(|r| r.key == key);
    let pair = |key: &str| -> (String, String) {
        row(key).map_or((DASH.into(), DASH.into()), |r| {
            (
                format!("{} → {}", cell(r, r.values[0], o), cell(r, r.values[1], o)),
                signed(r, r.deltas[1], o),
            )
        })
    };
    let headline = |label: &str, key: &str, also: Option<&str>| -> Value {
        let (value, diff) = pair(key);
        let sub = match also.and_then(row) {
            Some(r) => format!(
                "B {diff} · {} {} → {}",
                match r.key {
                    "first_crack_bt" | "drop_bt" => "at",
                    _ => "DTR",
                },
                cell(r, r.values[0], o),
                cell(r, r.values[1], o)
            ),
            None => format!("B {diff}"),
        };
        json!({ "label": label, "value": value, "sub": sub, "small": true })
    };
    let summary = vec![
        headline("First crack", "first_crack_s", Some("first_crack_bt")),
        headline("Drop", "drop_s", Some("drop_bt")),
        headline("Development", "development_s", Some("dtr_pct")),
        headline("Weight loss %", "loss_pct", None),
    ];

    let ov = cmp.overlay.first();
    let tolerance = format!("{} {}", o.delta(Some(i.within), 1), u.temp_label());
    let overlay = ov.map_or(String::new(), |v| {
        let settle = match v.within_from_s {
            Some(t) if t <= 0.5 => format!("B's BT stayed within {tolerance} of A's all the way"),
            Some(t) => format!(
                "B's BT stayed within {tolerance} of A's from {} on",
                clock(t)
            ),
            None => format!("B's BT did not settle within {tolerance} of A's"),
        };
        let rms = v.rms_bt_delta.map_or(String::new(), |r| {
            format!(" (RMS {} {}", o.delta(Some(r), 1), u.temp_label())
        });
        let max = match (&v.max_bt_delta, rms.is_empty()) {
            (Some(m), false) => format!(
                ", the largest {}{} {} at {})",
                if m.value >= 0.0 { "+" } else { "−" },
                o.delta(Some(m.value.abs()), 1),
                u.temp_label(),
                clock(m.t)
            ),
            (_, false) => ")".into(),
            _ => String::new(),
        };
        format!("{settle}{rms}{max}.")
    });

    let who = |m: &str, r: &RoastInput, a: &analysis::RoastAnalysis| -> Value {
        let s = &r.session;
        let started = o.local(s.start_time.unwrap_or(s.created_at));
        json!({
            "mark": m,
            "id": s.id,
            "batch": s.batch_number,
            "label": mark_of(r),
            "name": s.name,
            "coffee": coffee_of(r),
            "date": format!("{}, {}", short_date(&started), hm(&started)),
            "roaster": s.device_id,
            "profile": r.profile.as_ref().map(|p| p.profile.name.clone()),
            "green": s.green_weight.map(|g| format!("{} g", super::grams(f64::from(g)))),
            "charge_bt": o.temp_u(a.charge_bt, 1),
            "notes": s.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()),
        })
    };
    let coffees = match (coffee_of(&i.a), coffee_of(&i.b)) {
        (Some(a), Some(b)) if a == b => format!(" · {a}"),
        (Some(a), Some(b)) => format!(" · {a} / {b}"),
        (Some(c), None) | (None, Some(c)) => format!(" · {c}"),
        (None, None) => String::new(),
    };
    let subject = format!("{} vs {}{coffees}", mark_of(&i.a), mark_of(&i.b));
    let alt = format!(
        "Bean temperature and rate of rise of {} (A) and {} (B) from charge.{}",
        mark_of(&i.a),
        mark_of(&i.b),
        if overlay.is_empty() {
            String::new()
        } else {
            format!(" {overlay}")
        }
    );
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &subject, o),
        "roasts": [who("A", &i.a, ra), who("B", &i.b, rb)],
        "summary": summary,
        "chart": {
            "x": x, "y": y, "y2": y2,
            "x-name": "min", "y-name": u.temp_label(), "y2-name": u.rate_label(),
            "bands": [],
            "events": events,
            "series": series,
            "alt": alt,
        },
        "strips": strips,
        "overlay": overlay,
        "phases": phases,
        "start": table(&cmp.rows[..split]),
        "rest": table(&cmp.rows[split..]),
        // The comparison as `roast compare` answers it, in °C and s, for whoever reads the data
        "comparison": {
            "reference": cmp.reference,
            "within": cmp.within,
            "rows": cmp.rows,
            "overlay": cmp.overlay,
        },
        "notes": format!("B − A is B's value less A's. Times from charge (mm:ss), temperatures {}, RoR {}: on the chart a ±15 s slope from 0:45, in the tables the server's own.", u.temp_label(), u.rate_label()),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};

    /// Batch #5 as A, and B a copy charged 4 °C hotter that cracks 10 s later and drops 6 s
    /// later at 0.6 °C more, with 0.4 g more out
    pub(crate) fn pair() -> Input {
        let a = fixture();
        let mut b = fixture();
        b.session.id = "roast-6".into();
        b.session.batch_number = Some(6);
        b.session.name = "Palmichal B: CHARGE heater 45 %".into();
        b.session.notes = Some("Same as #5 with one change: CHARGE heater 45 %.".into());
        b.session.roasted_weight = Some(86.1);
        b.session.weight_loss_pct = Some(13.9);
        for e in &mut b.events {
            match e.event_type {
                RoastEventType::FirstCrackStart => e.elapsed_seconds += 10.0,
                RoastEventType::Drop => {
                    e.elapsed_seconds += 6.0;
                    e.temperature = e.temperature.map(|t| t + 0.6);
                }
                _ => {}
            }
        }
        for p in &mut b.telemetry {
            let t = f64::from(p.elapsed_seconds);
            // Hotter early, the same from 2:00 on
            let lift = (4.0 * (1.0 - t / 120.0)).max(0.0) as f32;
            p.bean_temp = p.bean_temp.map(|v| v + lift);
            p.heater = p.heater.map(|h| (h - 8.0).max(0.0));
        }
        Input { a, b, within: 0.5 }
    }

    #[test]
    fn the_numbers_of_two_roasts_side_by_side() {
        let d = build(&pair(), &options(Units::C, Paper::Letter));
        assert_eq!(d["schema"], "roastwarden.report.compare/1");
        assert_eq!(d["meta"]["subject"], "#5 vs #6 · Palmichal Vecindario");
        assert_eq!(d["roasts"][0]["mark"], "A");
        assert_eq!(d["roasts"][1]["label"], "#6");
        let fc = &d["summary"][0];
        assert_eq!(fc["value"], "7:29 → 7:39");
        assert!(
            fc["sub"].as_str().unwrap().starts_with("B +0:10"),
            "{}",
            fc["sub"]
        );
        let loss = &d["summary"][3];
        assert_eq!(loss["value"], "14.3 → 13.9");
        assert_eq!(loss["sub"], "B −0.4");
        let rest = d["rest"].as_array().unwrap();
        let drop = rest.iter().find(|r| r["key"] == "drop_s").unwrap();
        assert_eq!(drop["diff"], "+0:06");
        let start = d["start"].as_array().unwrap();
        let charge = start.iter().find(|r| r["key"] == "charge_bt").unwrap();
        assert_eq!(charge["unit"], "°C");
        // Four lines and the profile, once: both roasts follow it
        let series = d["chart"]["series"].as_array().unwrap();
        assert_eq!(series.len(), 5);
        assert_eq!(series[0]["label"], "Profile");
        let shorts: Vec<&str> = d["chart"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["short"].as_str().unwrap())
            .collect();
        assert_eq!(shorts, ["FC A", "DROP A", "FC B", "DROP B"]);
        assert_eq!(d["phases"].as_array().unwrap().len(), 2);
        assert!(
            d["overlay"]
                .as_str()
                .unwrap()
                .starts_with("B's BT stayed within 0.5 °C of A's from"),
            "{}",
            d["overlay"]
        );
        assert_eq!(d["strips"].as_array().unwrap().len(), 2);
        // °F: differences in °F, not converted as temperatures
        let f = build(&pair(), &options(Units::F, Paper::A4));
        let rest = f["rest"].as_array().unwrap();
        let drop_bt = rest.iter().find(|r| r["key"] == "drop_bt").unwrap();
        assert_eq!(drop_bt["diff"], "+1.1");
        assert_eq!(drop_bt["unit"], "°F");
    }

    #[tokio::test]
    async fn the_compare_report_lays_out_on_one_page() {
        let r = Renderer::in_process();
        let k = kind("compare").unwrap();
        for (name, units, paper) in [
            ("compare", Units::C, Paper::Letter),
            ("compare-a4-f", Units::F, Paper::A4),
        ] {
            let data = build(&pair(), &options(units, paper));
            let pdf = render(&r, k, data.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(format!("{dir}/{name}.pdf"), &pdf.bytes).unwrap();
                for p in 1..=pdf.pages.unwrap_or(1) {
                    let png = render(&r, k, data.clone(), Format::Png, p, 110)
                        .await
                        .unwrap();
                    std::fs::write(format!("{dir}/{name}-{p}.png"), &png.bytes).unwrap();
                }
            }
            crate::reports::check::assert_clean("compare", name, &data);
            assert_eq!(pdf.pages, Some(1), "{name} fits one page");
        }
    }
}
