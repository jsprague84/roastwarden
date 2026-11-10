//! The profile report (`assets/reports/profile.typ`, schema `roastwarden.report.profile/1`):
//! a profile's targets and its roasts ranked in a table with an average row (after Artisan's
//! Ranking report), their BT overlaid on the profile's, and a dot strip each for first
//! crack, drop, DTR and loss.

use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::chart::{self, Point};
use super::coffee::{RoastLine, line, roast_row};
use super::{DASH, Options, ReportError, clock, clock_opt, grams, meta, n1};
use crate::profiles::types::ProfileWithPoints;
use crate::roasts::types::RoastSession;

/// Roasts in the table; overlaid on the chart
pub const MAX_ROASTS: usize = 40;
pub const MAX_OVERLAID: usize = 20;

pub struct Input {
    pub profile: ProfileWithPoints,
    /// Finished roasts of the profile, newest first
    pub roasts: Vec<RoastLine>,
    /// Only this roaster's, when asked
    pub roaster: Option<String>,
}

pub async fn gather(
    db: &SqlitePool,
    id: &str,
    roaster: Option<&str>,
) -> Result<Input, ReportError> {
    let Some(profile) = crate::profiles::store::get_profile_with_points(db, id).await? else {
        return Err(ReportError::NotFound(format!("no profile {id}")));
    };
    let list = sqlx::query_as::<_, RoastSession>(
        "SELECT * FROM roast_sessions WHERE profile_id = ? AND status = 'completed'
         AND (? IS NULL OR device_id = ?)
         ORDER BY COALESCE(start_time, created_at) DESC LIMIT ?",
    )
    .bind(id)
    .bind(roaster)
    .bind(roaster)
    .bind(MAX_ROASTS as i64)
    .fetch_all(db)
    .await?;
    let mut roasts = Vec::with_capacity(list.len());
    for s in list {
        roasts.push(line(db, s, 220).await?);
    }
    Ok(Input {
        profile,
        roasts,
        roaster: roaster.map(str::to_string),
    })
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

/// A strip's axis: the values' range widened a little, with about four ticks
fn strip_axis(values: &[f64], pad: f64, label: impl Fn(f64) -> String) -> chart::Axis {
    let lo = values.iter().copied().fold(f64::MAX, f64::min) - pad;
    let hi = values.iter().copied().fold(f64::MIN, f64::max) + pad;
    chart::axis(lo, hi, 4, label)
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("profile").expect("the profile kind");
    let p = &i.profile.profile;
    let u = o.units;
    let level = super::expect::level_label(p.roast_level.as_deref());
    let purpose = p.roast_purpose.clone();
    let facts: Vec<Value> = [
        ("Level", level.map(str::to_string)),
        (
            "Purpose",
            purpose.map(|s| {
                let mut c = s.chars();
                c.next().map_or(String::new(), |f| {
                    f.to_uppercase().collect::<String>() + c.as_str()
                })
            }),
        ),
        (
            "First crack",
            p.target_first_crack.map(|t| clock(f64::from(t))),
        ),
        (
            "Drop",
            match (p.target_total_time, p.target_end_temp) {
                (Some(t), Some(c)) => Some(format!(
                    "{} at {}",
                    clock(f64::from(t)),
                    o.temp_u(Some(f64::from(c)), 0)
                )),
                (Some(t), None) => Some(clock(f64::from(t))),
                (None, Some(c)) => Some(o.temp_u(Some(f64::from(c)), 0)),
                _ => None,
            },
        ),
        (
            "Charge",
            p.charge_temp.map(|c| o.temp_u(Some(f64::from(c)), 0)),
        ),
        (
            "Batch",
            p.batch_size_g.map(|g| format!("{} g", grams(f64::from(g)))),
        ),
        ("Roaster", p.roaster.clone()),
        ("Beans", p.beans.clone()),
    ]
    .into_iter()
    .filter_map(|(l, v)| {
        v.filter(|v| !v.trim().is_empty())
            .map(|v| json!({ "label": l, "value": v }))
    })
    .collect();

    let nums: Vec<_> = i
        .roasts
        .iter()
        .map(super::coffee::RoastLine::numbers)
        .collect();
    let pick =
        |f: &dyn Fn(&crate::profiles::history::ProfileHistoryRoast) -> Option<f32>| -> Vec<f64> {
            nums.iter().filter_map(|n| f(n).map(f64::from)).collect()
        };
    let fc = pick(&|n| n.fc_s);
    let fc_bt = pick(&|n| n.fc_bt);
    let drop_t = pick(&|n| n.drop_s);
    let drop_bt = pick(&|n| n.drop_bt);
    let dev = pick(&|n| n.development_s);
    let loss = pick(&|n| n.weight_loss_pct);
    let rmse = pick(&|n| n.rmse);
    let dtr: Vec<f64> = nums
        .iter()
        .filter_map(|n| {
            let (d, t) = (n.development_s?, n.total_s?);
            (t > 0.0).then(|| f64::from(d / t * 100.0))
        })
        .collect();
    let green: Vec<f64> = i
        .roasts
        .iter()
        .filter_map(|l| l.session.green_weight.map(f64::from))
        .collect();
    let cups: Vec<f64> = i
        .roasts
        .iter()
        .filter_map(|l| l.cupping.as_ref()?.cupping.final_score.map(f64::from))
        .collect();
    let average = json!({
        "green": mean(&green).map_or(DASH.into(), grams_round),
        "loss": n1(mean(&loss)),
        "fc": clock_opt(mean(&fc)),
        "fc_bt": o.temp(mean(&fc_bt), 1),
        "drop": clock_opt(mean(&drop_t)),
        "drop_bt": o.temp(mean(&drop_bt), 1),
        "dev": clock_opt(mean(&dev)),
        "dtr": n1(mean(&dtr)),
        "rmse": o.delta(mean(&rmse), 1),
        "cup": mean(&cups).map_or(DASH.into(), |c| format!("{c:.2}")),
    });

    // The overlay: the profile's BT, then each roast's (the newest named)
    let mut series: Vec<Value> = Vec::new();
    let mut ends: Vec<f64> = Vec::new();
    let mut hi = u.temp(100.0);
    let prof: Vec<Point> = i
        .profile
        .points
        .iter()
        .map(|q| [f64::from(q.time_seconds), u.temp(f64::from(q.target_temp))])
        .collect();
    for (k, l) in i.roasts.iter().take(MAX_OVERLAID).enumerate().rev() {
        let pts: Vec<Point> = l.bt.iter().map(|q| [q[0], u.temp(q[1])]).collect();
        ends.extend(pts.last().map(|q| q[0]));
        hi = pts.iter().map(|q| q[1]).fold(hi, f64::max);
        series.push(json!({
            "key": "bt", "label": if k == 0 { Some(l.mark()) } else { None },
            "newest": k == 0, "points": chart::rounded(&pts, 1),
        }));
    }
    if !prof.is_empty() {
        // As far as the roasts went; with none, the whole curve
        let end = if ends.is_empty() {
            f64::INFINITY
        } else {
            ends.iter().copied().fold(0.0, f64::max).max(60.0)
        };
        let prof: Vec<Point> = prof.into_iter().filter(|q| q[0] <= end + 30.0).collect();
        hi = prof.iter().map(|q| q[1]).fold(hi, f64::max);
        ends.extend(prof.last().map(|q| q[0]));
        series.push(
            json!({ "key": "profile", "label": "Profile", "points": chart::rounded(&prof, 1) }),
        );
    }
    let x_max = (ends.iter().copied().fold(60.0, f64::max) / 60.0).ceil() * 60.0;
    let x = chart::Axis {
        min: 0.0,
        max: x_max,
        ticks: (0..=(x_max / 60.0) as usize)
            .map(|k| chart::Tick {
                v: k as f64 * 60.0,
                label: k.to_string(),
            })
            .collect(),
    };
    let y = chart::axis(0.0, hi + u.delta(5.0), 5, chart::plain);

    // One strip per measure, each roast its batch number, the newest filled
    let dots =
        |f: &dyn Fn(&crate::profiles::history::ProfileHistoryRoast) -> Option<f64>| -> Vec<Value> {
            i.roasts
            .iter()
            .zip(&nums)
            .enumerate()
            .filter_map(|(k, (l, n))| {
                let v = f(n)?;
                Some(json!({
                    "mark": l.session.batch_number.map_or_else(|| (k + 1).to_string(), |b| b.to_string()),
                    "v": v, "newest": k == 0,
                }))
            })
            .collect()
        };
    let mut strips = Vec::new();
    if !fc.is_empty() {
        strips.push(json!({ "label": "First crack", "unit": "from charge", "axis": strip_axis(&fc, 15.0, clock), "dots": dots(&|n| n.fc_s.map(f64::from)) }));
    }
    if !drop_bt.is_empty() {
        let v: Vec<f64> = drop_bt.iter().map(|c| u.temp(*c)).collect();
        strips.push(json!({ "label": "Drop", "unit": u.temp_label(), "axis": strip_axis(&v, u.delta(1.0), chart::plain), "dots": dots(&|n| n.drop_bt.map(|c| u.temp(f64::from(c)))) }));
    }
    if !dtr.is_empty() {
        strips.push(json!({ "label": "DTR", "unit": "%", "axis": strip_axis(&dtr, 1.0, chart::plain), "dots": dots(&|n| {
            let (d, t) = (n.development_s?, n.total_s?);
            (t > 0.0).then(|| f64::from(d / t * 100.0))
        }) }));
    }
    if !loss.is_empty() {
        strips.push(json!({ "label": "Weight loss", "unit": "%", "axis": strip_axis(&loss, 0.5, chart::plain), "dots": dots(&|n| n.weight_loss_pct.map(f64::from)) }));
    }

    let rows: Vec<Value> = i.roasts.iter().map(|l| roast_row(l, o)).collect();

    // What a roast of it should do: its own ranges, else its level's (no history: these are
    // the profile's, not this roaster's)
    let ex = super::expect::expected(
        p.expectations.as_ref().map(|j| &j.0),
        p.roast_level.as_deref(),
        None,
        &[],
        "",
    );
    let who = |r: &super::expect::Range| match r.source {
        super::expect::Source::Profile => "the profile's own",
        _ => "its level's",
    };
    let pct = |v: f64| {
        let s = format!("{v:.1}");
        s.strip_suffix(".0").map_or(s.clone(), str::to_string)
    };
    let expected: Vec<Value> = [
        (
            "Weight loss",
            ex.weight_loss
                .as_ref()
                .map(|r| (format!("{}–{} %", pct(r.lo), pct(r.hi)), who(r))),
        ),
        (
            "First crack",
            ex.fc_s
                .as_ref()
                .map(|r| (format!("{}–{}", clock(r.lo), clock(r.hi)), who(r))),
        ),
        (
            "Development",
            ex.development_s
                .as_ref()
                .map(|r| (format!("{}–{}", clock(r.lo), clock(r.hi)), who(r))),
        ),
        (
            "Roast time",
            ex.total_s
                .as_ref()
                .map(|r| (format!("{}–{}", clock(r.lo), clock(r.hi)), who(r))),
        ),
        (
            "Drop",
            ex.drop_bt.as_ref().map(|r| {
                (
                    format!("{}–{}", o.temp(Some(r.lo), 0), o.temp_u(Some(r.hi), 0)),
                    who(r),
                )
            }),
        ),
        (
            "Colour",
            ex.color_ground
                .as_ref()
                .map(|r| (format!("Agtron {}–{} ground", pct(r.lo), pct(r.hi)), who(r))),
        ),
    ]
    .into_iter()
    .filter_map(|(label, v)| {
        v.map(|(value, from)| json!({ "label": label, "value": value, "from": from }))
    })
    .collect();
    // The target curve minute by minute, to write a roast against by hand
    let end = p
        .target_total_time
        .map(f64::from)
        .or_else(|| i.profile.points.last().map(|q| f64::from(q.time_seconds)))
        .unwrap_or(0.0);
    #[expect(
        clippy::maybe_infinite_iter,
        reason = "take_while ends it: t only grows"
    )]
    let curve: Vec<Value> = (0..)
        .map(|k| f64::from(k) * 60.0)
        .take_while(|t| *t <= end + 0.5)
        .filter_map(|t| {
            let bt = super::plan::curve_at(&i.profile, t)?;
            Some(json!({ "t": clock(t), "bt": o.temp(Some(bt), 0) }))
        })
        .collect();

    let summary = vec![
        json!({ "label": "Roasts", "value": i.roasts.len().to_string(),
                "sub": i.roaster.as_ref().map_or("on every roaster".to_string(), |r| format!("on {r}")) }),
        json!({ "label": "First crack", "value": clock_opt(mean(&fc)), "sub": "on average" }),
        json!({ "label": "Weight loss", "value": mean(&loss).map_or(DASH.into(), |l| format!("{l:.1} %")), "sub": "on average" }),
        json!({ "label": "Tracking", "value": mean(&rmse).map_or(DASH.into(), |r| format!("± {}", o.units_delta_u(r))), "sub": "RMS from the profile" }),
    ];
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &p.name, o),
        "profile": { "name": p.name, "description": p.description },
        "facts": facts,
        "summary": summary,
        "roasts": rows,
        "average": average,
        "overlay": { "x": x, "y": y, "x-name": "min", "y-name": u.temp_label(), "series": series },
        "overlaid": i.roasts.len().min(MAX_OVERLAID),
        "strips": strips,
        "expected": expected,
        "curve": curve,
        "notes": if i.roasts.is_empty() {
            format!("Times from charge (mm:ss), temperatures {}. BT minute by minute is the profile's target curve; the ranges are the profile's own, else its roast level's.", u.temp_label())
        } else {
            format!("Times from charge (mm:ss), temperatures {}. The average row is the mean of the roasts that have the value. The chart overlays each roast's BT from charge, the newest in colour, on the profile's.", u.temp_label())
        },
    })
}

fn grams_round(g: f64) -> String {
    format!("{g:.0}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::coffee::tests::lines;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};

    #[tokio::test]
    async fn a_profile_ranks_its_roasts_and_averages_them() {
        let input = Input {
            profile: fixture().profile.unwrap(),
            roasts: lines(),
            roaster: Some("esp32_roaster_01".into()),
        };
        let o = options(Units::C, Paper::Letter);
        let d = build(&input, &o);
        assert_eq!(d["roasts"].as_array().unwrap().len(), 3);
        // First crack 7:29, 7:21, 7:43: the mean 7:31
        assert_eq!(d["average"]["fc"], "7:31");
        assert_eq!(d["summary"][0]["sub"], "on esp32_roaster_01");
        let series = d["overlay"]["series"].as_array().unwrap();
        assert_eq!(series.len(), 4, "three roasts and the profile");
        assert_eq!(series.last().unwrap()["key"], "profile");
        assert_eq!(series[2]["newest"], true);
        assert_eq!(d["strips"].as_array().unwrap().len(), 4);
        assert_eq!(d["strips"][0]["dots"][0]["mark"], "5");
        assert!(
            d["facts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["label"] == "Drop" && f["value"] == "10:05 at 210 °C")
        );

        let r = Renderer::in_process();
        let pdf = render(&r, kind("profile").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert!(pdf.pages.unwrap() <= 2, "{:?} pages", pdf.pages);
        crate::reports::check::assert_clean(
            "profile",
            d["meta"]["subject"].as_str().unwrap_or("profile"),
            &d,
        );
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/profile.pdf"), &pdf.bytes).unwrap();
            for p in 1..=pdf.pages.unwrap() {
                let png = render(&r, kind("profile").unwrap(), d.clone(), Format::Png, p, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/profile-{p}.png"), &png.bytes).unwrap();
            }
        }
    }

    /// A profile never roasted prints its target curve, minute by minute, and what a roast of
    /// it should do, not an empty page
    #[tokio::test]
    async fn a_profile_with_no_roasts_prints_its_target() {
        let input = Input {
            profile: fixture().profile.unwrap(),
            roasts: vec![],
            roaster: None,
        };
        let d = build(&input, &options(Units::C, Paper::Letter));
        let curve = d["curve"].as_array().unwrap();
        // 0:00 to the 10:05 drop: eleven whole minutes
        assert_eq!(curve.len(), 11, "{curve:?}");
        assert_eq!(curve[0]["t"], "0:00");
        let expected = d["expected"].as_array().unwrap();
        assert!(
            expected
                .iter()
                .any(|e| e["label"] == "Weight loss" && e["value"] == "14–16 %"),
            "{expected:?}"
        );
        let series = d["overlay"]["series"].as_array().unwrap();
        assert_eq!(series.len(), 1);
        // The whole curve, to its drop at 10:05
        assert!(
            series[0]["points"].as_array().unwrap().last().unwrap()[0]
                .as_f64()
                .unwrap()
                >= 600.0
        );
        assert_eq!(d["overlay"]["x"]["max"], 660.0);
        let r = Renderer::in_process();
        let pdf = render(&r, kind("profile").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert_eq!(pdf.pages, Some(1));
        crate::reports::check::assert_clean("profile", "a profile with no roasts", &d);
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            let png = render(&r, kind("profile").unwrap(), d, Format::Png, 1, 110)
                .await
                .unwrap();
            std::fs::write(format!("{dir}/profile-empty-1.png"), &png.bytes).unwrap();
        }
    }

    /// Thirteen roasts close together, as on the owner's espresso profile: the spread strips'
    /// rings stack upward without touching (R1), on A4 in °F
    #[tokio::test]
    async fn a_crowded_spread_lays_out() {
        use crate::roasts::types::RoastEventType;
        // (first crack shift s, drop shift s, drop BT shift °C, roasted g)
        let shifts = [
            (0.0, 0.0, 0.0, 85.7),
            (-8.0, 1.0, 0.1, 85.75),
            (14.0, 0.0, 0.3, 85.33),
            (2.0, -3.0, -0.2, 86.0),
            (-6.0, 4.0, 0.2, 85.5),
            (9.0, 2.0, 0.1, 85.9),
            (1.0, 1.0, 0.0, 85.6),
            (-2.0, -2.0, -0.4, 85.8),
            (20.0, 5.0, 0.6, 85.2),
            (-14.0, -6.0, -0.9, 86.1),
            (3.0, 0.0, 0.1, 85.7),
            (5.0, 3.0, 0.4, 85.4),
            (-1.0, 1.0, 0.0, 85.65),
        ];
        let roasts: Vec<RoastLine> = shifts
            .iter()
            .enumerate()
            .map(|(k, (fc, drop, bt, out))| {
                let mut l = lines().remove(0);
                l.session.id = format!("roast-{}", 13 - k);
                l.session.batch_number = Some(13 - k as i32);
                l.session.roasted_weight = Some(*out);
                l.session.weight_loss_pct = Some(100.0 - out);
                for e in &mut l.events {
                    match e.event_type {
                        RoastEventType::FirstCrackStart => e.elapsed_seconds += fc,
                        RoastEventType::Drop => {
                            e.elapsed_seconds += drop;
                            e.temperature = e.temperature.map(|t| t + bt);
                        }
                        _ => {}
                    }
                }
                l
            })
            .collect();
        let input = Input {
            profile: fixture().profile.unwrap(),
            roasts,
            roaster: None,
        };
        let d = build(&input, &options(Units::F, Paper::A4));
        assert_eq!(d["strips"][0]["dots"].as_array().unwrap().len(), 13);
        let r = Renderer::in_process();
        let pdf = render(&r, kind("profile").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert!(pdf.pages.unwrap() <= 2, "{:?} pages", pdf.pages);
        crate::reports::check::assert_clean(
            "profile",
            d["meta"]["subject"].as_str().unwrap_or("profile"),
            &d,
        );
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            for p in 1..=pdf.pages.unwrap() {
                let png = render(&r, kind("profile").unwrap(), d.clone(), Format::Png, p, 110)
                    .await
                    .unwrap();
                std::fs::write(format!("{dir}/profile-crowded-a4f-{p}.png"), &png.bytes).unwrap();
            }
        }
    }
}
