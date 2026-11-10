//! The roast report's data (`assets/reports/roast.typ`, schema `roastwarden.report.roast/1`):
//! the coffee and the roast, eight figures judged against what was expected
//! (`expect.rs`), the chart's series, phases, events, notes and cupping.

use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::chart::{self, Point};
use super::expect::{self, Expected, Range, Source};
use super::{DASH, Options, ReportError, clock, clock_opt, grams, hm, long_date, meta, n1};
use crate::coffee::costs::RoastCost;
use crate::coffee::inventory::InventoryService;
use crate::profiles::history::ProfileHistoryRoast;
use crate::profiles::types::ProfileWithPoints;
use crate::roasts::types::{
    CuppingWithAttributes, RoastEvent, RoastEventType, RoastSession, SessionTelemetry,
};

/// The roast's green coffee, as far as the report names it
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Coffee {
    pub name: String,
    pub origin: Option<String>,
    pub region: Option<String>,
    pub producer: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
}

/// Everything the report reads about one roast.
pub struct Input {
    pub session: RoastSession,
    pub telemetry: Vec<SessionTelemetry>,
    pub events: Vec<RoastEvent>,
    pub profile: Option<ProfileWithPoints>,
    pub cupping: Option<CuppingWithAttributes>,
    pub bean: Option<Coffee>,
    pub cost: Option<RoastCost>,
    pub roaster: Option<String>,
    /// The profile's completed roasts on this roaster
    pub history: Vec<ProfileHistoryRoast>,
    /// The roast's declared channels (docs/channels.md, its own list or, from before, the
    /// roaster's): drawn, and read by `roast review`
    pub channels: Vec<crate::roasters::channels::Channel>,
}

pub async fn gather(db: &SqlitePool, id: &str) -> Result<Input, ReportError> {
    let Some(swt) = crate::roasts::sessions::get_session_with_telemetry(db, id).await? else {
        return Err(ReportError::NotFound(format!("no roast {id}")));
    };
    let events = crate::roasts::events::get_roast_events(db, id).await?;
    let inv = InventoryService::new(db.clone());
    let bean = match &swt.session.bean_id {
        Some(b) => inv.bean(b).await?.map(|b| Coffee {
            name: b.name,
            origin: b.origin,
            region: b.region,
            producer: b.producer,
            variety: b.variety,
            process: b.process,
        }),
        None => None,
    };
    let cost = inv.roast_cost(id).await?;
    let roaster: Option<String> =
        sqlx::query_scalar("SELECT name FROM devices WHERE device_id = ?")
            .bind(&swt.session.device_id)
            .fetch_optional(db)
            .await?;
    let history = match &swt.session.profile_id {
        Some(p) => {
            crate::profiles::history::profile_history(db, p, Some(&swt.session.device_id)).await?
        }
        None => vec![],
    };
    Ok(Input {
        channels: swt.channels,
        session: swt.session,
        telemetry: swt.telemetry,
        events,
        profile: swt.profile,
        cupping: swt.cupping,
        bean,
        cost,
        roaster,
        history,
    })
}

/// The landmarks in the order a roast meets them: (type, key, short, name)
/// The rate of rise starts this long after charge, on the chart and in its scale
const ROR_FROM: f64 = 45.0;

const LANDMARKS: [(RoastEventType, &str, &str, &str); 9] = [
    (RoastEventType::Charge, "charge", "CHARGE", "Charge"),
    (
        RoastEventType::TurningPoint,
        "turning_point",
        "TP",
        "Turning point",
    ),
    (RoastEventType::DryingEnd, "drying_end", "DRY", "Dry end"),
    (
        RoastEventType::FirstCrackStart,
        "first_crack_start",
        "FC",
        "First crack",
    ),
    (
        RoastEventType::FirstCrackEnd,
        "first_crack_end",
        "FCe",
        "First crack end",
    ),
    (
        RoastEventType::SecondCrackStart,
        "second_crack_start",
        "SC",
        "Second crack",
    ),
    (
        RoastEventType::SecondCrackEnd,
        "second_crack_end",
        "SCe",
        "Second crack end",
    ),
    (RoastEventType::Drop, "drop", "DROP", "Drop"),
    (RoastEventType::CoolEnd, "cool_end", "COOL", "Cool end"),
];

fn purpose_label(p: &str) -> String {
    match p {
        "espresso" => "Espresso".into(),
        "filter" => "Filter".into(),
        "omni" => "Omni".into(),
        "other" => "Other".into(),
        p => p.into(),
    }
}

fn flag_label(f: &str) -> String {
    match f {
        "underdeveloped" => "Underdeveloped".into(),
        "baked" => "Baked / flat".into(),
        "scorching" => "Scorching".into(),
        "tipping" => "Tipping".into(),
        "uneven" => "Uneven".into(),
        "stall" => "RoR stall/crash".into(),
        "flick" => "RoR flick".into(),
        f => f.into(),
    }
}

/// The value nearest to `t` among `points`
fn nearest(points: &[Point], t: f64, within: f64) -> Option<f64> {
    points
        .iter()
        .min_by(|a, b| (a[0] - t).abs().total_cmp(&(b[0] - t).abs()))
        .filter(|p| (p[0] - t).abs() <= within)
        .map(|p| p[1])
}

/// BT at `t`, straight between samples
fn at(points: &[Point], t: f64) -> Option<f64> {
    let i = points.iter().position(|p| p[0] >= t)?;
    if i == 0 {
        return Some(points[0][1]);
    }
    let (a, b) = (points[i - 1], points[i]);
    let f = if b[0] > a[0] {
        (t - a[0]) / (b[0] - a[0])
    } else {
        0.0
    };
    Some(a[1] + (b[1] - a[1]) * f)
}

/// The range as the figure shows it: who set it and the span
fn range_text(r: &Range, ex: &Expected, span: String) -> String {
    let who = match r.source {
        Source::Profile => "profile".to_string(),
        Source::Level => ex.level_label.map_or("level".into(), str::to_lowercase),
        Source::History => "usual".into(),
    };
    format!("{who} {span}")
}

/// One figure: label, value, the line under it, and the range it was judged against
fn figure(
    label: &str,
    value: String,
    sub: Option<String>,
    judged: Option<(&Range, Option<f64>, String)>,
    ex: &Expected,
) -> Value {
    let (range, state) = match judged {
        Some((r, v, span)) => (
            Some(range_text(r, ex, span)),
            r.judge(v).map(super::expect::Verdict::as_str),
        ),
        None => (None, None),
    };
    json!({ "label": label, "value": value, "sub": sub, "range": range, "state": state, "note": null })
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("roast").expect("the roast kind");
    let s = &i.session;
    let u = o.units;
    let ev = |t: RoastEventType| i.events.iter().find(|e| e.event_type == t);
    let charge = ev(RoastEventType::Charge).map_or(0.0, |e| f64::from(e.elapsed_seconds));
    let mark = |t: RoastEventType| {
        ev(t).map(|e| {
            (
                f64::from(e.elapsed_seconds) - charge,
                e.temperature.map(f64::from),
            )
        })
    };
    let mut samples: Vec<&SessionTelemetry> = i.telemetry.iter().collect();
    samples.sort_by(|a, b| a.elapsed_seconds.total_cmp(&b.elapsed_seconds));
    let last = samples
        .last()
        .map(|p| f64::from(p.elapsed_seconds) - charge);
    let drop = mark(RoastEventType::Drop);
    let fc = mark(RoastEventType::FirstCrackStart);
    let dry = mark(RoastEventType::DryingEnd);
    let tp = mark(RoastEventType::TurningPoint);
    let end = drop.map(|d| d.0).or(last).unwrap_or(0.0).max(1.0);

    // The series, °C, from 15 s before charge to drop
    let series = |pick: &dyn Fn(&SessionTelemetry) -> Option<f64>| -> Vec<Point> {
        samples
            .iter()
            .filter_map(|p| {
                let t = f64::from(p.elapsed_seconds) - charge;
                let v = pick(p)?;
                (v.is_finite() && (-15.0..=end + 1.0).contains(&t)).then_some([t, v])
            })
            .collect()
    };
    let bt_c = series(&|p| p.bean_temp.map(f64::from));
    let et_c = series(&|p| p.env_temp.map(f64::from));
    let heater = series(&|p| p.heater);
    let fan = series(&|p| p.fan);
    // The rate of rise from ROR_FROM after charge (the probe has caught up with the beans) to
    // 10 s before drop; drawn and scaled from the same start
    let ror_c: Vec<Point> = chart::ror(&chart::every(&bt_c, 1.0), 15.0)
        .into_iter()
        .filter(|p| p[0] >= ROR_FROM && p[0] <= end - 10.0)
        .collect();
    let to_units = |pts: &[Point], f: &dyn Fn(f64) -> f64| -> Vec<Point> {
        pts.iter().map(|p| [p[0], f(p[1])]).collect()
    };
    let bt = chart::lttb(&chart::every(&to_units(&bt_c, &|c| u.temp(c)), 1.0), 900);
    let et = chart::lttb(&chart::every(&to_units(&et_c, &|c| u.temp(c)), 1.0), 900);
    let ror = chart::every(&to_units(&ror_c, &|c| u.delta(c)), 2.0);
    let profile_pts: Vec<Point> = i
        .profile
        .as_ref()
        .map(|p| {
            p.points
                .iter()
                .filter(|q| f64::from(q.time_seconds) <= end + 30.0)
                .map(|q| [f64::from(q.time_seconds), u.temp(f64::from(q.target_temp))])
                .collect()
        })
        .unwrap_or_default();
    let heater_s = chart::every(&chart::trailing_mean(&heater, 15.0), 3.0);
    let fan_s = chart::every(&fan, 3.0);
    // The roaster's declared channels (docs/channels.md): a temperature on the chart in the
    // report's unit; a % channel in the strip; another unit in the strip on its own 0..max
    let mut channel_lines: Vec<Value> = Vec::new();
    let mut channel_strip: Vec<Value> = Vec::new();
    for (n, c) in i.channels.iter().enumerate() {
        let pts = series(&|p| crate::roasters::channels::stored(p.extras.as_ref(), &c.key));
        if pts.is_empty() {
            continue;
        }
        let key = format!("ch{n}");
        if c.is_temp() {
            let p = chart::lttb(&chart::every(&to_units(&pts, &|v| u.temp(v)), 1.0), 900);
            channel_lines.push(json!({ "key": key, "label": c.label, "axis": "y", "points": p }));
        } else if c.is_pct() {
            channel_strip
                .push(json!({ "key": key, "label": c.label, "points": chart::every(&pts, 3.0) }));
        } else {
            let top = c
                .max
                .unwrap_or_else(|| pts.iter().map(|p| p[1]).fold(0.0, f64::max))
                .max(1e-9);
            let scaled = to_units(&pts, &|v| (v / top * 100.0).clamp(0.0, 100.0));
            channel_strip.push(json!({ "key": key, "label": c.label,
                "tag": format!("{} (0–{} {})", c.label, trim1(top), c.unit),
                "points": chart::every(&scaled, 3.0) }));
        }
    }

    // Axes: minutes along the bottom, temperature left, rate of rise right
    let x_max = ((end + 15.0) / 60.0).ceil() * 60.0;
    let minute_step = if x_max > 20.0 * 60.0 { 120.0 } else { 60.0 };
    let x = chart::Axis {
        min: 0.0,
        max: x_max,
        ticks: (0..=((x_max / minute_step) as usize))
            .map(|k| {
                let v = k as f64 * minute_step;
                chart::Tick {
                    v,
                    label: format!("{}", (v / 60.0) as i64),
                }
            })
            .collect(),
    };
    let channel_hi = channel_lines
        .iter()
        .flat_map(|s| s["points"].as_array().cloned().unwrap_or_default())
        .filter_map(|p| p[1].as_f64())
        .fold(f64::MIN, f64::max);
    let hi = bt
        .iter()
        .chain(&et)
        .chain(&profile_pts)
        .map(|p| p[1])
        .fold(u.temp(100.0), f64::max)
        .max(channel_hi);
    let y = chart::axis(0.0, hi + u.delta(5.0), 5, chart::plain);
    // The rate-of-rise axis on a robust maximum (the 95th percentile, at least 20 °C/min), so
    // the first minute's steep start or a spike does not squeeze the rest into the bottom;
    // the line starts where it first comes inside the axis, so it never runs off the top there
    let ror_hi = chart::quantile(ror.iter().map(|p| p[1]), 0.95)
        .unwrap_or(0.0)
        .max(u.delta(20.0));
    let y2 = chart::axis(0.0, ror_hi * 1.05, 6, chart::plain);
    let ror: Vec<Point> = ror.into_iter().skip_while(|p| p[1] > y2.max).collect();

    let bands: Vec<Value> = [
        ("drying", "Drying", Some(0.0), dry.map(|d| d.0)),
        ("maillard", "Maillard", dry.map(|d| d.0), fc.map(|f| f.0)),
        (
            "development",
            "Development",
            fc.map(|f| f.0),
            drop.map(|d| d.0),
        ),
    ]
    .into_iter()
    .filter_map(|(key, label, a, b)| {
        let (a, b) = (a?, b?);
        (b > a).then(|| json!({ "key": key, "label": label, "from": a, "to": b }))
    })
    .collect();

    // Events: when, BT, ET, RoR and heater there
    let events: Vec<Value> = LANDMARKS
        .iter()
        .filter_map(|(t, key, short, name)| {
            let (at_s, temp) = mark(t.clone())?;
            let bt_there = temp.or_else(|| nearest(&bt_c, at_s, 5.0));
            Some(json!({
                "key": key,
                "short": short,
                "name": name,
                "t": (at_s * 10.0).round() / 10.0,
                "time": clock(at_s),
                "bt": o.temp(bt_there, 1),
                "et": o.temp(nearest(&et_c, at_s, 5.0), 1),
                "ror": if at_s >= 30.0 { o.delta(nearest(&ror_c, at_s, 15.0), 1) } else { DASH.into() },
                "heater": nearest(&heater, at_s, 5.0).map_or(DASH.into(), |h| format!("{h:.0}")),
                "on_chart": at_s > 0.5 && at_s <= x_max,
            }))
        })
        .collect();

    // Phases: drying (its ΔBT from the turning point, as Artisan's), Maillard, development
    let phases: Vec<Value> = [
        ("drying", "Drying", Some(0.0), dry.map(|d| d.0)),
        ("maillard", "Maillard", dry.map(|d| d.0), fc.map(|f| f.0)),
        (
            "development",
            "Development",
            fc.map(|f| f.0),
            drop.map(|d| d.0),
        ),
    ]
    .into_iter()
    .filter_map(|(key, name, a, b)| {
        let (a, b) = (a?, b?);
        if b <= a {
            return None;
        }
        let from = match tp {
            Some((t, _)) if key == "drying" && t > a && t < b => t,
            _ => a,
        };
        let dt = at(&bt_c, b).zip(at(&bt_c, from)).map(|(y1, y0)| y1 - y0);
        let ror = dt.filter(|_| b > from).map(|d| d / (b - from) * 60.0);
        Some(json!({
            "key": key,
            "name": name,
            "time": clock(b - a),
            "pct": format!("{:.1}", (b - a) / end * 100.0),
            "frac": (b - a) / end,
            "dt": o.delta(dt, 1),
            "ror": o.delta(ror, 1),
        }))
    })
    .collect();

    // What was expected
    let level = i
        .profile
        .as_ref()
        .and_then(|p| p.profile.roast_level.clone())
        .or_else(|| s.target_roast_level.clone());
    let ex = expect::expected(
        i.profile
            .as_ref()
            .and_then(|p| p.profile.expectations.as_ref())
            .map(|j| &j.0),
        level.as_deref(),
        s.moisture_green.map(f64::from),
        &i.history,
        &s.id,
    );
    let loss = s.weight_loss_pct.map(f64::from).or_else(|| {
        let (g, r) = (f64::from(s.green_weight?), f64::from(s.roasted_weight?));
        (g > 0.0).then(|| (g - r) / g * 100.0)
    });
    let development = fc.zip(drop).map(|(f, d)| d.0 - f.0);
    let dtr = development.map(|d| d / end * 100.0);
    let pct_span = |r: &Range| format!("{}–{} %", trim1(r.lo), trim1(r.hi));
    let time_span = |r: &Range| format!("{}–{}", clock(r.lo), clock(r.hi));
    let temp_span = |r: &Range| format!("{}–{}", o.temp(Some(r.lo), 0), o.temp_u(Some(r.hi), 0));

    let mut figures = Vec::new();
    figures.push(match (s.green_weight, s.roasted_weight) {
        (Some(g), Some(r)) => figure(
            "Weight",
            format!("{} → {} g", grams(f64::from(g)), grams(f64::from(r))),
            Some(format!("{} % loss", n1(loss))),
            ex.weight_loss.as_ref().map(|r| (r, loss, pct_span(r))),
            &ex,
        ),
        (Some(g), None) => figure(
            "Weight",
            format!("{} g", grams(f64::from(g))),
            Some("not weighed after".into()),
            None,
            &ex,
        ),
        _ => figure("Weight", DASH.into(), Some("not weighed".into()), None, &ex),
    });
    figures.push(figure(
        "Roast time",
        clock(end),
        Some(
            if drop.is_some() {
                "charge to drop"
            } else {
                "charge to the last reading"
            }
            .into(),
        ),
        ex.total_s.as_ref().map(|r| (r, Some(end), time_span(r))),
        &ex,
    ));
    figures.push(figure(
        "First crack",
        clock_opt(fc.map(|f| f.0)),
        Some(fc.map_or("not marked".into(), |f| o.temp_u(f.1, 1))),
        fc.and(ex.fc_s.as_ref())
            .map(|r| (r, fc.map(|f| f.0), time_span(r))),
        &ex,
    ));
    figures.push(figure(
        "Development",
        clock_opt(development),
        Some(dtr.map_or("first crack to drop".into(), |d| format!("DTR {d:.1} %"))),
        development
            .and(ex.development_s.as_ref())
            .map(|r| (r, development, time_span(r))),
        &ex,
    ));
    figures.push(figure(
        "Drop",
        o.temp_u(drop.and_then(|d| d.1), 1),
        Some(drop.map_or("not marked".into(), |d| format!("at {}", clock(d.0)))),
        drop.and(ex.drop_bt.as_ref())
            .map(|r| (r, drop.and_then(|d| d.1), temp_span(r))),
        &ex,
    ));
    let agtron = s
        .color_ground
        .map(f64::from)
        .filter(|_| s.color_scale.as_deref().is_none_or(|c| c == "agtron"));
    figures.push(match (s.color_whole, s.color_ground) {
        (None, None) => figure(
            "Colour",
            DASH.into(),
            Some("not measured".into()),
            None,
            &ex,
        ),
        (whole, ground) => {
            let scale = match s.color_scale.as_deref() {
                None | Some("agtron") => "Agtron",
                Some("tonino") => "Tonino",
                Some("lebrew") => "RoastSee",
                Some(_) => "Colour",
            };
            figure(
                "Colour",
                format!(
                    "{scale} {}",
                    ground
                        .or(whole)
                        .map_or(DASH.into(), |v| trim1(f64::from(v)))
                ),
                Some(match (whole, ground) {
                    (Some(w), Some(_)) => format!("ground · whole {}", trim1(f64::from(w))),
                    (None, Some(_)) => "ground".into(),
                    _ => "whole bean".into(),
                }),
                agtron
                    .and(ex.color_ground.as_ref())
                    .map(|r| (r, agtron, format!("Agtron {}–{}", trim1(r.lo), trim1(r.hi)))),
                &ex,
            )
        }
    });
    figures.push(match (s.profile_rmse, i.profile.as_ref()) {
        (Some(rmse), Some(_)) => figure(
            "Profile tracking",
            format!("± {}", o.units_delta_u(f64::from(rmse))),
            Some(format!(
                "RMS, max {}",
                o.delta(s.profile_max_dev.map(f64::from), 1)
            )),
            None,
            &ex,
        ),
        (_, Some(_)) => figure(
            "Profile tracking",
            DASH.into(),
            Some("not measured".into()),
            None,
            &ex,
        ),
        _ => figure(
            "Profile",
            DASH.into(),
            Some("roasted by hand".into()),
            None,
            &ex,
        ),
    });
    figures.push(match &i.cost {
        Some(c) => {
            let m = |v: f64| super::money(&c.currency, v);
            let dose = match s.roast_purpose.as_deref() {
                Some("filter") => c.per_filter_dose.map(|d| (d, c.filter_dose_g)),
                _ => c.per_espresso_dose.map(|d| (d, c.espresso_dose_g)),
            };
            // Per kg of roasted coffee (the green that went in over what came out): the label
            // says so, as a price per kg reads as the green's otherwise
            let mut f = figure(
                if c.roasted_per_kg.is_some() {
                    "Cost per kg roasted"
                } else {
                    "Coffee cost"
                },
                c.roasted_per_kg.map_or_else(
                    || format!("{} green", m(c.green_cost)),
                    |k| format!("{}/kg", m(k)),
                ),
                Some(match (c.roasted_per_kg, dose) {
                    (Some(_), Some((d, g))) => format!("{} per {} g dose", m(d), grams(g)),
                    _ => format!("{}/kg green", m(c.green_per_kg)),
                }),
                None,
                &ex,
            );
            if c.roasted_per_kg.is_some() {
                f["note"] = json!(format!("{} of green coffee", m(c.green_cost)));
            }
            f
        }
        None => figure(
            "Coffee cost",
            DASH.into(),
            Some("no priced green".into()),
            None,
            &ex,
        ),
    });

    // The coffee and the roast
    let b = i.bean.as_ref();
    let started = o.local(s.start_time.unwrap_or(s.created_at));
    let coffee = b.map_or_else(|| s.name.clone(), |b| b.name.clone());
    let pick =
        |a: Option<&String>, c: Option<&String>| a.or(c).filter(|v| !v.trim().is_empty()).cloned();
    let roaster = match &i.roaster {
        Some(n) if n != &s.device_id => format!("{n} ({})", s.device_id),
        _ => s.device_id.clone(),
    };
    let flags: Vec<String> = s
        .roast_flags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(flag_label)
        .collect();
    let cupping = i.cupping.as_ref().map(|c| {
        json!({
            "final": c.cupping.final_score.map_or(DASH.into(), |f| format!("{f:.2}")),
            "band": c.band.clone().unwrap_or_else(|| "not all ten scored".into()),
            "total": c.total_score.map_or(DASH.into(), |t| format!("{t:.2}")),
            "defects": c.defects.to_string(),
            "taint_cups": c.cupping.taint_cups,
            "fault_cups": c.cupping.fault_cups,
            "attributes": c.attributes.iter().map(|a| {
                let label = crate::roasts::cupping::ATTRIBUTES.iter().find(|x| x.0 == a.attribute_name).map_or(a.attribute_name.as_str(), |x| x.1);
                json!({ "key": a.attribute_name, "label": label, "short": short_attr(label), "score": format!("{:.2}", a.score) })
            }).collect::<Vec<_>>(),
            "notes": c.cupping.notes,
        })
    });
    let subject = match s.batch_number {
        Some(n) => format!("#{n} · {coffee}"),
        None => coffee.clone(),
    };
    let alt = format!(
        "Bean temperature from charge{} to {}{}{}.",
        drop.map_or(String::new(), |d| format!(" to drop at {}", clock(d.0))),
        o.temp_u(
            drop.and_then(|d| d.1).or_else(|| bt_c.last().map(|p| p[1])),
            0
        ),
        fc.map_or(String::new(), |f| format!(
            ", first crack at {}",
            clock(f.0)
        )),
        development.map_or(String::new(), |d| format!(", development {}", clock(d))),
    );
    let rounded = |pts: &[Point]| chart::rounded(pts, 1);
    let mut strip: Vec<Value> = [("heater", "Heater", &heater_s), ("fan", "Fan", &fan_s)]
        .into_iter()
        .filter(|(_, _, p)| !p.is_empty())
        .map(|(key, label, p)| json!({ "key": key, "label": label, "points": rounded(p) }))
        .collect();
    let round_points = |mut v: Value| {
        let pts: Vec<Point> = serde_json::from_value(v["points"].take()).unwrap_or_default();
        v["points"] = json!(rounded(&pts));
        v
    };
    strip.extend(channel_strip.into_iter().map(round_points));
    let mut chart_series = vec![
        json!({ "key": "profile", "label": "Profile", "axis": "y", "points": rounded(&profile_pts) }),
        json!({ "key": "et", "label": "ET", "axis": "y", "points": rounded(&et) }),
    ];
    chart_series.extend(channel_lines.into_iter().map(round_points));
    chart_series.push(json!({ "key": "bt", "label": "BT", "axis": "y", "points": rounded(&bt) }));
    chart_series
        .push(json!({ "key": "ror", "label": "RoR", "axis": "y2", "points": rounded(&ror) }));

    json!({
        "schema": kind.schema,
        "meta": meta(kind, &subject, o),
        "roast": {
            "id": s.id,
            "name": s.name,
            "batch": s.batch_number,
            "coffee": coffee,
            "origin": pick(b.and_then(|b| b.origin.as_ref()), s.bean_origin.as_ref()),
            "region": b.and_then(|b| b.region.clone()),
            "producer": b.and_then(|b| b.producer.clone()),
            "variety": pick(b.and_then(|b| b.variety.as_ref()), s.bean_variety.as_ref()),
            "process": b.and_then(|b| b.process.clone()),
            "date": long_date(&started),
            "time": hm(&started),
            "roaster": roaster,
            "profile": i.profile.as_ref().map(|p| p.profile.name.clone()),
            "purpose": s.roast_purpose.as_deref().map(purpose_label),
            "level": expect::level_label(s.target_roast_level.as_deref()).or(ex.level_label),
            "rating": s.rating,
            "notes": s.notes.as_deref().filter(|n| !n.trim().is_empty()),
            "flags": flags,
        },
        // The numbers, in the report's units, for whoever reads the data rather than the page
        "numbers": {
            "green_g": s.green_weight,
            "roasted_g": s.roasted_weight,
            "loss_pct": loss,
            "total_s": end,
            "fc_s": fc.map(|f| f.0),
            "fc_temp": fc.and_then(|f| f.1).map(|c| u.temp(c)),
            "drop_temp": drop.and_then(|d| d.1).map(|c| u.temp(c)),
            "development_s": development,
            "dtr_pct": dtr,
            "max_temp": s.max_temp.map(|c| u.temp(f64::from(c))),
            "profile_rmse": s.profile_rmse.map(|c| u.delta(f64::from(c))),
            "history_roasts": ex.history,
        },
        "figures": figures,
        "chart": {
            "x": x, "y": y, "y2": y2,
            "x-name": "min", "y-name": u.temp_label(), "y2-name": u.rate_label(),
            "bands": bands,
            "events": events.iter().filter(|e| e["on_chart"] == true).cloned().collect::<Vec<_>>(),
            "series": chart_series,
            "strip": strip,
            "alt": alt,
        },
        "phases": phases,
        "events": events,
        "cupping": cupping,
    })
}

/// `14`, `14.5`
fn trim1(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_string)
}

/// The cupping table's column: `Aroma` for Fragrance/Aroma, `Clean cup` as is
fn short_attr(label: &str) -> String {
    label.rsplit('/').next().unwrap_or(label).to_string()
}

impl Options {
    /// A temperature difference with its unit: `1.4 °C`
    pub fn units_delta_u(&self, c: f64) -> String {
        format!("{:.1} {}", self.units.delta(c), self.units.temp_label())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::profiles::types::{ProfileEvent, ProfilePoint, RoastProfile};
    use crate::reports::{Format, Paper, Units, kind, render};
    use crate::roasts::types::{CuppingAttribute, CuppingScore};

    /// A recorded roast on the real fluid bed, as `roast show --telemetry` gave it
    pub(crate) fn fixture() -> Input {
        let v: Value =
            serde_json::from_str(include_str!("../../testdata/reports/roast-batch5.json")).unwrap();
        let from = |x: &Value| x.clone();
        let profile = ProfileWithPoints {
            profile: serde_json::from_value::<RoastProfile>(from(&v["profile"])).unwrap(),
            points: serde_json::from_value::<Vec<ProfilePoint>>(from(&v["profile"]["points"]))
                .unwrap(),
            events: serde_json::from_value::<Vec<ProfileEvent>>(from(&v["profile"]["events"]))
                .unwrap(),
            curves: None,
        };
        Input {
            session: serde_json::from_value(from(&v)).unwrap(),
            telemetry: serde_json::from_value(from(&v["telemetry"])).unwrap(),
            events: serde_json::from_value(from(&v["events"])).unwrap(),
            profile: Some(profile),
            cupping: None,
            bean: Some(serde_json::from_value(from(&v["bean"])).unwrap()),
            cost: crate::coffee::costs::roast_cost(
                2.15,
                100.0,
                Some(85.7),
                "USD",
                crate::coffee::costs::Sizes::default(),
            ),
            roaster: Some("Fluid bed".into()),
            history: vec![],
            channels: vec![],
        }
    }

    pub(crate) fn options(units: Units, paper: Paper) -> Options {
        Options {
            paper,
            units,
            zone: crate::ops::zone::Zone::minutes(-300),
            now: "2026-10-26T13:40:00Z".parse().unwrap(),
            currency: "USD".into(),
        }
    }

    fn cupping() -> CuppingWithAttributes {
        let now = chrono::Utc::now();
        let attrs: Vec<CuppingAttribute> = crate::roasts::cupping::ATTRIBUTES
            .iter()
            .map(|(k, _, kind, _)| CuppingAttribute {
                id: "a".into(),
                cupping_id: "c".into(),
                attribute_name: k.to_string(),
                score: if *kind == crate::roasts::cupping::Kind::Cups {
                    10.0
                } else {
                    7.75
                },
                created_at: now,
            })
            .collect();
        CuppingWithAttributes {
            cupping: CuppingScore {
                id: "c".into(),
                session_id: "s".into(),
                scoring_framework: "sca".into(),
                final_score: Some(82.25),
                notes: Some("Chocolate sauce, toasted coconut; a little thin.".into()),
                taint_cups: 1,
                fault_cups: 0,
                created_at: now,
                updated_at: now,
            },
            total_score: Some(84.25),
            defects: 2,
            band: Some("very good".into()),
            attributes: attrs,
        }
    }

    fn figure<'a>(d: &'a Value, label: &str) -> &'a Value {
        d["figures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["label"] == label)
            .unwrap_or_else(|| panic!("no figure {label}"))
    }

    /// An exhaust (°C) and a gas pressure (kPa, top 4) on the roaster, in every sample.
    fn with_channels(i: &mut Input) {
        use crate::roasters::channels::Channel;
        i.channels = vec![
            Channel {
                key: "t3".into(),
                label: "Exhaust".into(),
                unit: "C".into(),
                max: None,
            },
            Channel {
                key: "gas".into(),
                label: "Gas".into(),
                unit: "kPa".into(),
                max: Some(4.0),
            },
        ];
        for p in &mut i.telemetry {
            let t3 = p.bean_temp.map_or(100.0, |b| f64::from(b) - 15.0);
            p.extras = Some(json!({ "t3": t3, "gas": 2.0 }));
        }
    }

    #[test]
    fn declared_channels_are_drawn() {
        let mut i = fixture();
        with_channels(&mut i);
        let d = build(&i, &options(Units::F, Paper::Letter));
        let series = d["chart"]["series"].as_array().unwrap();
        let ex = series.iter().find(|s| s["label"] == "Exhaust").unwrap();
        assert_eq!(
            (ex["key"].as_str(), ex["axis"].as_str()),
            (Some("ch0"), Some("y"))
        );
        // In the report's unit: °F
        let c = build(&i, &options(Units::C, Paper::Letter));
        let ex_c = c["chart"]["series"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == "Exhaust")
            .unwrap()["points"][0][1]
            .as_f64()
            .unwrap();
        let first = ex["points"][0][1].as_f64().unwrap();
        assert!(
            (first - (ex_c * 1.8 + 32.0)).abs() < 0.2,
            "{first} vs {ex_c} °C"
        );
        let gas = d["chart"]["strip"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["label"] == "Gas")
            .unwrap();
        assert_eq!(gas["tag"], "Gas (0–4 kPa)");
        assert_eq!(gas["points"][0][1], 50.0);
        // None declared: the chart is as before
        let plain = build(&fixture(), &options(Units::C, Paper::Letter));
        assert_eq!(plain["chart"]["series"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn the_data_holds_what_the_page_shows() {
        let d = build(&fixture(), &options(Units::C, Paper::Letter));
        assert_eq!(d["schema"], "roastwarden.report.roast/1");
        assert_eq!(d["meta"]["subject"], "#5 · Palmichal Vecindario");
        assert_eq!(d["meta"]["made"], "26 Oct 2026, 08:40");
        assert_eq!(d["meta"]["made_iso"], json!([2026, 10, 26, 8, 40, 0]));
        assert_eq!(d["roast"]["date"], "Sunday 25 October 2026");
        assert_eq!(d["roast"]["time"], "13:26");
        assert_eq!(d["roast"]["roaster"], "Fluid bed (esp32_roaster_01)");
        assert_eq!(d["roast"]["level"], "Light");
        let w = figure(&d, "Weight");
        assert_eq!(w["value"], "100 → 85.7 g");
        assert_eq!(w["sub"], "14.3 % loss");
        assert_eq!(
            (w["range"].as_str(), w["state"].as_str()),
            (Some("light 14–16 %"), Some("within"))
        );
        // Charge at 7.2 s: drop at 612.6 is 10:05, first crack at 456.3 is 7:29
        assert_eq!(figure(&d, "Roast time")["value"], "10:05");
        assert_eq!(figure(&d, "First crack")["value"], "7:29");
        let dev = figure(&d, "Development");
        assert_eq!(
            (dev["value"].as_str(), dev["state"].as_str()),
            (Some("2:36"), Some("above"))
        );
        assert_eq!(dev["range"], "light 1:00–2:00");
        assert_eq!(figure(&d, "Drop")["value"], "210.3 °C");
        assert_eq!(figure(&d, "Cost per kg roasted")["value"], "$25.09/kg");
        // On the chart: dry end, first crack, drop (charge is its left edge, cool end past it)
        let on_chart: Vec<&str> = d["chart"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["short"].as_str().unwrap())
            .collect();
        assert_eq!(on_chart, ["DRY", "FC", "DROP"]);
        assert_eq!(d["events"].as_array().unwrap().len(), 5);
        assert_eq!(d["phases"].as_array().unwrap().len(), 3);
        assert_eq!(d["chart"]["x"]["max"], 660.0);
        assert!(d["chart"]["series"][2]["points"].as_array().unwrap().len() > 500);
        // °F
        let f = build(&fixture(), &options(Units::F, Paper::A4));
        assert_eq!(figure(&f, "Drop")["value"], "410.5 °F");
        assert_eq!(f["meta"]["paper"], "a4");
        assert_eq!(f["chart"]["y2-name"], "°F/min");
    }

    #[tokio::test]
    async fn the_report_lays_out_on_one_page() {
        let r = Renderer::in_process();
        let k = kind("roast").unwrap();
        let out = std::env::var("RR_REPORTS_OUT").ok();
        let mut cupped = fixture();
        cupped.cupping = Some(cupping());
        // Events close together (first crack end and second crack just before drop at
        // 612.6 s): their names on the chart push apart (R9)
        for (t, at) in [
            (RoastEventType::FirstCrackEnd, 590.0),
            (RoastEventType::SecondCrackStart, 600.0),
        ] {
            let mut e = cupped.events[0].clone();
            e.id = format!("{t:?}");
            e.event_type = t;
            e.elapsed_seconds = at;
            e.temperature = None;
            cupped.events.push(e);
        }
        // Declared channels (docs/channels.md): an exhaust on the chart, gas in the strip
        with_channels(&mut cupped);
        for (name, input, units, paper) in [
            ("roast", fixture(), Units::C, Paper::Letter),
            ("roast-cupped-a4-f", cupped, Units::F, Paper::A4),
        ] {
            let data = build(&input, &options(units, paper));
            let pdf = render(&r, k, data.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            assert!(pdf.bytes.starts_with(b"%PDF"), "{name}");
            assert_eq!(pdf.pages, Some(1), "{name} fits one page");
            crate::reports::check::assert_clean("roast", name, &data);
            if let Some(dir) = &out {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(format!("{dir}/{name}.pdf"), &pdf.bytes).unwrap();
                let png = render(&r, k, data, Format::Png, 1, 110).await.unwrap();
                std::fs::write(format!("{dir}/{name}-1.png"), &png.bytes).unwrap();
            }
        }
    }
}
