//! Roast analysis (`docs/analysis.md`): the numbers a roast is read by, in one
//! place for the API, the CLI (`roast review`, `roast compare`) and agents. Times are seconds
//! from CHARGE, temperatures °C, RoR °C/min (the server's own RoR, as the scope shows it).
//!
//! What it reads is what the roast report reads (`reports::roast::gather`); what it computes is
//! what sessions 11–12 worked out by hand: the marks and phases, RoR (its early peak, by
//! minute, the late dip), tracking against the profile (early overshoot, bias, RMS), how the
//! heater and fan were used (opening at CHARGE, headroom late), and the guidance's ranges.
//! [`compare`] lines several roasts up against a reference.

pub(crate) mod alarms;
pub(crate) mod bean_guide;
pub(crate) mod ring;
pub(crate) mod telemetry;
pub(crate) mod ws2;

use serde::Serialize;
use utoipa::ToSchema;

use crate::reports::chart;
use crate::reports::expect::{self, Range, Source};
use crate::reports::roast::Input;
use crate::roasts::types::{RoastEventType, SessionTelemetry};

/// A value at a roast time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
pub struct At {
    /// Seconds from CHARGE
    pub t: f64,
    pub value: f64,
}

/// A landmark of the roast.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Mark {
    /// `charge`, `turning_point`, `drying_end`, `first_crack_start`, `first_crack_end`,
    /// `second_crack_start`, `drop`
    pub key: &'static str,
    /// Seconds from CHARGE
    pub t: f64,
    /// BT there: the mark's own reading, else the nearest sample's
    pub bt: Option<f64>,
    pub et: Option<f64>,
    pub ror: Option<f64>,
    pub heater: Option<f64>,
    pub fan: Option<f64>,
}

/// Drying, Maillard, development.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Phase {
    /// `drying`, `maillard`, `development`
    pub key: &'static str,
    pub from_s: f64,
    pub to_s: f64,
    pub duration_s: f64,
    /// Of the whole roast
    pub pct: f64,
    /// BT gained (drying: from the turning point, as Artisan counts it)
    pub delta_bt: Option<f64>,
    /// Its mean RoR, °C/min
    pub mean_ror: Option<f64>,
}

/// RoR through the roast.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct RorStats {
    /// The early peak, from the turning point (else 0:15) to 5:00
    pub peak: Option<At>,
    /// At each whole minute
    pub by_minute: Vec<At>,
    /// The lowest in the last 2:30 before drop, from first crack on
    pub late_min: Option<At>,
    /// At drop
    pub at_drop: Option<f64>,
    /// How RoR fell from first crack to drop, °C/min per minute (least squares; negative
    /// when it falls)
    pub fc_drop_slope: Option<f64>,
}

/// BT against the profile the roast followed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Tracking {
    pub profile_id: String,
    pub profile: String,
    /// BT − profile at 0:30, 1:00, 1:30 and 2:00
    pub early: Vec<At>,
    /// The largest BT − profile in the first two minutes (the overshoot after CHARGE)
    pub early_peak: Option<At>,
    /// From the turning point to drop (as stored at Complete): mean BT − profile
    pub bias: Option<f64>,
    pub rmse: Option<f64>,
    pub max_dev: Option<f64>,
    /// First crack against the profile's (positive: later)
    pub fc_vs_profile_s: Option<f64>,
    /// Drop against the profile's total time (positive: later)
    pub total_vs_profile_s: Option<f64>,
}

/// BT against the setpoint the roaster was given.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SetpointTracking {
    /// Mean BT − setpoint from the turning point to drop
    pub bias: f64,
    pub sd: f64,
}

/// How the heater and fan were used, CHARGE to drop.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Control {
    /// Mean heater % in the first 30 s
    pub heater_opening: Option<f64>,
    pub heater_mean: Option<f64>,
    pub heater_max: Option<f64>,
    /// Seconds with the heater at 90 % or more
    pub heater_high_s: f64,
    /// Seconds with the heater at 99.5 % or more
    pub heater_saturated_s: f64,
    /// Mean |change| of heater %, per second (how busy the controller was)
    pub heater_step_per_s: Option<f64>,
    /// Means over the last minute before drop
    pub late_heater: Option<f64>,
    pub late_fan: Option<f64>,
    pub fan_min: Option<f64>,
    pub fan_max: Option<f64>,
    pub setpoint: Option<SetpointTracking>,
}

/// One number judged against what was expected (`docs/roast-guidance.md`).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Judged {
    /// `weight_loss_pct`, `total_s`, `fc_s`, `development_s`, `drop_bt`, `color_ground`
    pub key: &'static str,
    pub label: &'static str,
    pub value: Option<f64>,
    pub lo: f64,
    pub hi: f64,
    /// Where the range comes from: `profile`, `level` or `history` (this roaster's own roasts
    /// of the profile)
    pub source: &'static str,
    /// `within`, `below`, `above`; null when the value is missing
    pub verdict: Option<&'static str>,
}

/// One roast, read.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RoastAnalysis {
    pub session_id: String,
    pub name: String,
    pub device_id: String,
    pub batch_number: Option<i32>,
    pub status: String,
    pub roasted_at: String,
    pub coffee: Option<String>,
    pub green_g: Option<f64>,
    pub roasted_g: Option<f64>,
    pub loss_pct: Option<f64>,
    /// BT at CHARGE
    pub charge_bt: Option<f64>,
    /// CHARGE to drop, else to the last sample
    pub total_s: f64,
    pub dropped: bool,
    pub development_s: Option<f64>,
    pub dtr_pct: Option<f64>,
    pub max_bt: Option<f64>,
    /// BT area above the AUC base, CHARGE to drop, °C·min (as stored at Complete)
    pub auc: Option<f64>,
    pub marks: Vec<Mark>,
    pub phases: Vec<Phase>,
    pub ror: RorStats,
    pub profile: Option<Tracking>,
    pub control: Control,
    pub expected: Vec<Judged>,
    /// Plain lines on what stands out: a number outside its range, little heater left late
    pub notes: Vec<String>,
    /// The roaster's declared channels at CHARGE, first crack and DROP (docs/channels.md)
    pub channels: Vec<ChannelMarks>,
}

/// A declared channel's value at the roast's marks.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ChannelMarks {
    pub key: String,
    pub label: String,
    /// `C` (°C), `%` or the channel's own unit
    pub unit: String,
    pub charge: Option<f64>,
    pub first_crack: Option<f64>,
    pub drop: Option<f64>,
}

/// One sample, from CHARGE.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct S {
    pub t: f64,
    pub bt: Option<f64>,
    pub et: Option<f64>,
    pub ror: Option<f64>,
    pub heater: Option<f64>,
    pub fan: Option<f64>,
    pub sp: Option<f64>,
}

/// A roast read, with the samples [`compare`] lines up.
pub struct Analysed {
    pub analysis: RoastAnalysis,
    pub(crate) samples: Vec<S>,
}

const LANDMARKS: [(RoastEventType, &str); 7] = [
    (RoastEventType::Charge, "charge"),
    (RoastEventType::TurningPoint, "turning_point"),
    (RoastEventType::DryingEnd, "drying_end"),
    (RoastEventType::FirstCrackStart, "first_crack_start"),
    (RoastEventType::FirstCrackEnd, "first_crack_end"),
    (RoastEventType::SecondCrackStart, "second_crack_start"),
    (RoastEventType::Drop, "drop"),
];

fn finite(v: Option<f64>) -> Option<f64> {
    v.filter(|v| v.is_finite())
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The samples from CHARGE, sorted, RoR from the server's own where the roast has it (else
/// computed from BT as the report draws it).
fn samples(telemetry: &[SessionTelemetry], charge: f64) -> Vec<S> {
    let mut out: Vec<S> = telemetry
        .iter()
        .map(|p| S {
            t: f64::from(p.elapsed_seconds) - charge,
            bt: finite(p.bean_temp.map(f64::from)),
            et: finite(p.env_temp.map(f64::from)),
            ror: finite(p.rate_of_rise.map(f64::from)),
            heater: finite(p.heater),
            fan: finite(p.fan),
            sp: finite(p.setpoint.map(f64::from)),
        })
        .collect();
    out.sort_by(|a, b| a.t.total_cmp(&b.t));
    let with_ror = out.iter().filter(|s| s.ror.is_some()).count();
    if with_ror * 2 < out.len() {
        let bt: Vec<chart::Point> = out.iter().filter_map(|s| Some([s.t, s.bt?])).collect();
        let ror = chart::ror(&chart::every(&bt, 1.0), 15.0);
        for s in &mut out {
            s.ror = nearest_point(&ror, s.t, 2.0);
        }
    }
    out
}

fn nearest_point(points: &[chart::Point], t: f64, within: f64) -> Option<f64> {
    points
        .iter()
        .filter(|p| (p[0] - t).abs() <= within)
        .min_by(|a, b| (a[0] - t).abs().total_cmp(&(b[0] - t).abs()))
        .map(|p| p[1])
}

/// The nearest sample's value within `within` s of `t`.
fn near(s: &[S], t: f64, within: f64, pick: impl Fn(&S) -> Option<f64>) -> Option<f64> {
    s.iter()
        .filter(|x| (x.t - t).abs() <= within)
        .filter_map(|x| Some(((x.t - t).abs(), pick(x)?)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v)| v)
}

/// BT at `t`, linear between samples; none outside them.
fn bt_at(s: &[S], t: f64) -> Option<f64> {
    let mut prev: Option<(f64, f64)> = None;
    for x in s {
        let Some(bt) = x.bt else { continue };
        if x.t >= t {
            return match prev {
                Some((pt, pb)) if x.t > pt => Some(pb + (bt - pb) * (t - pt) / (x.t - pt)),
                Some(_) => Some(bt),
                None => ((x.t - t).abs() <= 2.0).then_some(bt),
            };
        }
        prev = Some((x.t, bt));
    }
    prev.filter(|(pt, _)| (t - pt).abs() <= 2.0).map(|(_, b)| b)
}

/// Mean of a channel over `from..=to`.
fn mean_in(s: &[S], from: f64, to: f64, pick: impl Fn(&S) -> Option<f64>) -> Option<f64> {
    let v: Vec<f64> = s
        .iter()
        .filter(|x| x.t >= from && x.t <= to)
        .filter_map(pick)
        .collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

/// Least-squares slope of (t s, v) points, per minute.
fn slope_per_min(pts: &[(f64, f64)]) -> Option<f64> {
    if pts.len() < 5 {
        return None;
    }
    let n = pts.len() as f64;
    let (mt, mv) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let sxx: f64 = pts.iter().map(|p| (p.0 - mt).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mt) * (p.1 - mv)).sum();
    (sxx > 0.0).then(|| sxy / sxx * 60.0)
}

fn clock(s: f64) -> String {
    let s = s.round() as i64;
    let sign = if s < 0 { "-" } else { "" };
    format!("{sign}{}:{:02}", s.abs() / 60, s.abs() % 60)
}

/// Read one roast.
pub fn analyse(i: &Input) -> Analysed {
    let s = &i.session;
    let ev = |t: &RoastEventType| i.events.iter().find(|e| &e.event_type == t);
    let charge = ev(&RoastEventType::Charge).map_or(0.0, |e| f64::from(e.elapsed_seconds));
    let samples = samples(&i.telemetry, charge);
    let mark_t = |t: RoastEventType| ev(&t).map(|e| f64::from(e.elapsed_seconds) - charge);
    let drop_t = mark_t(RoastEventType::Drop);
    let last = samples.last().map(|x| x.t);
    let total = drop_t.or(last).unwrap_or(0.0).max(0.0);
    let (tp, dry, fc) = (
        mark_t(RoastEventType::TurningPoint),
        mark_t(RoastEventType::DryingEnd),
        mark_t(RoastEventType::FirstCrackStart),
    );
    let ror_at = |t: f64| near(&samples, t, 3.0, |x| x.ror);

    let marks: Vec<Mark> = LANDMARKS
        .iter()
        .filter_map(|(ty, key)| {
            let e = ev(ty)?;
            let t = f64::from(e.elapsed_seconds) - charge;
            Some(Mark {
                key,
                t: r1(t),
                bt: finite(e.temperature.map(f64::from))
                    .or_else(|| near(&samples, t, 5.0, |x| x.bt))
                    .map(r1),
                et: near(&samples, t, 5.0, |x| x.et).map(r1),
                ror: (t >= 15.0).then(|| ror_at(t)).flatten().map(r1),
                heater: near(&samples, t, 5.0, |x| x.heater).map(r1),
                fan: near(&samples, t, 5.0, |x| x.fan).map(r1),
            })
        })
        .collect();
    let charge_bt = marks
        .iter()
        .find(|m| m.key == "charge")
        .and_then(|m| m.bt)
        .or_else(|| near(&samples, 0.0, 5.0, |x| x.bt).map(r1));

    let phases: Vec<Phase> = [
        ("drying", Some(0.0), dry),
        ("maillard", dry, fc),
        ("development", fc, drop_t),
    ]
    .into_iter()
    .filter_map(|(key, a, b)| {
        let (a, b) = (a?, b?);
        if b <= a || total <= 0.0 {
            return None;
        }
        let from = match tp {
            Some(t) if key == "drying" && t > a && t < b => t,
            _ => a,
        };
        let delta = bt_at(&samples, b)
            .zip(bt_at(&samples, from))
            .map(|(y1, y0)| y1 - y0);
        Some(Phase {
            key,
            from_s: r1(a),
            to_s: r1(b),
            duration_s: r1(b - a),
            pct: r1((b - a) / total * 100.0),
            delta_bt: delta.map(r1),
            mean_ror: delta
                .filter(|_| b > from)
                .map(|d| r1(d / (b - from) * 60.0)),
        })
    })
    .collect();

    // RoR
    let in_roast = |x: &&S| x.t > 0.0 && x.t <= total;
    let peak_from = tp.unwrap_or(15.0).max(15.0);
    let peak_to = 300f64.min(fc.unwrap_or(total)).min(total);
    let peak = samples
        .iter()
        .filter(|x| x.t >= peak_from && x.t <= peak_to)
        .filter_map(|x| {
            Some(At {
                t: x.t,
                value: x.ror?,
            })
        })
        .max_by(|a, b| a.value.total_cmp(&b.value));
    #[expect(
        clippy::maybe_infinite_iter,
        reason = "take_while ends it: t only grows"
    )]
    let by_minute: Vec<At> = (1..)
        .map(|m| f64::from(m) * 60.0)
        .take_while(|t| *t <= total)
        .filter_map(|t| ror_at(t).map(|v| At { t, value: r1(v) }))
        .collect();
    let late_from = fc.unwrap_or(0.0).max(total - 150.0);
    let late_min = drop_t.and_then(|d| {
        samples
            .iter()
            .filter(|x| x.t >= late_from && x.t <= d - 3.0)
            .filter_map(|x| {
                Some(At {
                    t: x.t,
                    value: x.ror?,
                })
            })
            .min_by(|a, b| a.value.total_cmp(&b.value))
    });
    let fc_drop_slope = fc.zip(drop_t).and_then(|(f, d)| {
        let pts: Vec<(f64, f64)> = samples
            .iter()
            .filter(|x| x.t >= f && x.t <= d)
            .filter_map(|x| Some((x.t, x.ror?)))
            .collect();
        slope_per_min(&pts)
    });
    let ror = RorStats {
        peak: peak.map(|p| At {
            t: r1(p.t),
            value: r1(p.value),
        }),
        by_minute,
        late_min: late_min.map(|p| At {
            t: r1(p.t),
            value: r1(p.value),
        }),
        at_drop: drop_t.and_then(|d| ror_at(d - 2.0)).map(r1),
        fc_drop_slope: fc_drop_slope.map(r2),
    };

    // The profile
    let profile = i.profile.as_ref().map(|p| {
        let curve = crate::replay::engine::Plan::from_profile(p).bt;
        let end = curve.last().map_or(0.0, |c| c.0);
        let dev = |t: f64| -> Option<f64> {
            if t > end {
                return None;
            }
            Some(bt_at(&samples, t)? - crate::replay::engine::interpolate(&curve, t)?)
        };
        let early = [30.0, 60.0, 90.0, 120.0]
            .into_iter()
            .filter(|t| *t <= total)
            .filter_map(|t| dev(t).map(|v| At { t, value: r1(v) }))
            .collect();
        let early_peak = samples
            .iter()
            .filter(|x| x.t > 0.0 && x.t <= 120f64.min(total))
            .filter_map(|x| {
                Some(At {
                    t: x.t,
                    value: dev(x.t)?,
                })
            })
            .max_by(|a, b| a.value.total_cmp(&b.value))
            .map(|a| At {
                t: r1(a.t),
                value: r1(a.value),
            });
        // As Complete stores them; computed the same way for a roast still open
        let from = tp.unwrap_or(60.0);
        let pairs: Vec<(f64, f64)> = samples.iter().filter_map(|x| Some((x.t, x.bt?))).collect();
        let live = crate::roasts::stats::tracking_stats(
            &pairs,
            &curve,
            from,
            drop_t.unwrap_or(f64::INFINITY),
        );
        let stored = s.profile_rmse.map(f64::from);
        let (rmse, max_dev, bias) = match (stored, live) {
            (Some(r), _) => (
                Some(r),
                s.profile_max_dev.map(f64::from),
                s.profile_bias.map(f64::from),
            ),
            (None, Some((r, m, b))) => (Some(r), Some(m), Some(b)),
            _ => (None, None, None),
        };
        Tracking {
            profile_id: p.profile.id.clone(),
            profile: p.profile.name.clone(),
            early,
            early_peak,
            bias: bias.map(r2),
            rmse: rmse.map(r2),
            max_dev: max_dev.map(r2),
            fc_vs_profile_s: fc
                .zip(p.profile.target_first_crack)
                .map(|(f, pf)| r1(f - f64::from(pf))),
            total_vs_profile_s: drop_t
                .zip(p.profile.target_total_time)
                .map(|(d, pt)| r1(d - f64::from(pt))),
        }
    });

    // The heater and fan
    let roast: Vec<&S> = samples.iter().filter(in_roast).collect();
    let mut high = 0.0;
    let mut saturated = 0.0;
    let (mut steps, mut step_n) = (0.0, 0usize);
    for w in roast.windows(2) {
        let dt = w[1].t - w[0].t;
        if !(dt > 0.0 && dt <= 5.0) {
            continue;
        }
        if let (Some(a), Some(b)) = (w[0].heater, w[1].heater) {
            if a >= 90.0 {
                high += dt;
            }
            if a >= 99.5 {
                saturated += dt;
            }
            steps += (b - a).abs() / dt;
            step_n += 1;
        }
    }
    let heaters: Vec<f64> = roast.iter().filter_map(|x| x.heater).collect();
    let fans: Vec<f64> = roast.iter().filter_map(|x| x.fan).collect();
    let after_tp = tp.unwrap_or(60.0);
    let sp_dev: Vec<f64> = roast
        .iter()
        .filter(|x| x.t >= after_tp)
        .filter_map(|x| Some(x.bt? - x.sp?))
        .collect();
    let setpoint = (sp_dev.len() >= 10).then(|| {
        let n = sp_dev.len() as f64;
        let mean = sp_dev.iter().sum::<f64>() / n;
        let sd = (sp_dev.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        SetpointTracking {
            bias: r2(mean),
            sd: r2(sd),
        }
    });
    let control = Control {
        heater_opening: mean_in(&samples, 0.0, 30f64.min(total), |x| x.heater).map(r1),
        heater_mean: (!heaters.is_empty())
            .then(|| r1(heaters.iter().sum::<f64>() / heaters.len() as f64)),
        heater_max: heaters.iter().copied().reduce(f64::max).map(r1),
        heater_high_s: r1(high),
        heater_saturated_s: r1(saturated),
        heater_step_per_s: (step_n > 0).then(|| r2(steps / step_n as f64)),
        late_heater: drop_t
            .and_then(|d| mean_in(&samples, d - 60.0, d, |x| x.heater))
            .map(r1),
        late_fan: drop_t
            .and_then(|d| mean_in(&samples, d - 60.0, d, |x| x.fan))
            .map(r1),
        fan_min: fans.iter().copied().reduce(f64::min).map(r1),
        fan_max: fans.iter().copied().reduce(f64::max).map(r1),
        setpoint,
    };

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
    let development = fc.zip(drop_t).map(|(f, d)| d - f);
    let dtr = development
        .filter(|_| total > 0.0)
        .map(|d| d / total * 100.0);
    let drop_bt = marks.iter().find(|m| m.key == "drop").and_then(|m| m.bt);
    let agtron = s
        .color_ground
        .map(f64::from)
        .filter(|_| s.color_scale.as_deref().is_none_or(|c| c == "agtron"));
    let judged = |key, label, r: &Option<Range>, v: Option<f64>| {
        r.as_ref().map(|r| Judged {
            key,
            label,
            value: v.map(r1),
            lo: r1(r.lo),
            hi: r1(r.hi),
            source: match r.source {
                Source::Profile => "profile",
                Source::Level => "level",
                Source::History => "history",
            },
            verdict: r.judge(v).map(super::reports::expect::Verdict::as_str),
        })
    };
    let expected: Vec<Judged> = [
        judged("weight_loss_pct", "Weight loss", &ex.weight_loss, loss),
        judged("total_s", "Roast time", &ex.total_s, drop_t),
        judged("fc_s", "First crack", &ex.fc_s, fc),
        judged(
            "development_s",
            "Development",
            &ex.development_s,
            development,
        ),
        judged("drop_bt", "Drop BT", &ex.drop_bt, drop_bt),
        judged(
            "color_ground",
            "Ground colour (Agtron)",
            &ex.color_ground,
            agtron,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut notes: Vec<String> = expected
        .iter()
        .filter(|j| matches!(j.verdict, Some("below" | "above")))
        .map(|j| {
            let num = |v: f64| match j.key {
                "total_s" | "fc_s" | "development_s" => clock(v),
                _ => format!("{v:.1}"),
            };
            let unit = match j.key {
                "weight_loss_pct" => " %",
                "drop_bt" => " °C",
                _ => "",
            };
            format!(
                "{} {}{unit} is {} the {} range {}–{}{unit}",
                j.label,
                j.value.map_or("—".into(), num),
                j.verdict.unwrap_or(""),
                match j.source {
                    "history" => "usual",
                    s => s,
                },
                num(j.lo),
                num(j.hi),
            )
        })
        .collect();
    if let (Some(h), Some(low)) = (control.late_heater, ror.late_min)
        && h >= 90.0
    {
        notes.push(format!(
                "Little heater left late: {h:.0} % over the last minute, RoR down to {:.1} °C/min at {}",
                low.value,
                clock(low.t)
            ));
    }
    if let Some(p) = profile.as_ref().and_then(|p| p.early_peak)
        && p.value >= 5.0
    {
        notes.push(format!(
            "BT ran {:.1} °C above the profile at {} after CHARGE",
            p.value,
            clock(p.t)
        ));
    }

    // Declared channels at CHARGE, first crack and DROP: the nearest sample within 5 s
    let channels = i
        .channels
        .iter()
        .map(|c| {
            let pts: Vec<(f64, f64)> = i
                .telemetry
                .iter()
                .filter_map(|p| {
                    let v = crate::roasters::channels::stored(p.extras.as_ref(), &c.key)?;
                    Some((f64::from(p.elapsed_seconds) - charge, v))
                })
                .collect();
            let at = |t: Option<f64>| -> Option<f64> {
                let t = t?;
                pts.iter()
                    .filter(|p| (p.0 - t).abs() <= 5.0)
                    .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
                    .map(|p| r1(p.1))
            };
            ChannelMarks {
                key: c.key.clone(),
                label: c.label.clone(),
                unit: c.unit.clone(),
                charge: at(Some(0.0)),
                first_crack: at(fc),
                drop: at(drop_t),
            }
        })
        .collect();

    let analysis = RoastAnalysis {
        session_id: s.id.clone(),
        name: s.name.clone(),
        device_id: s.device_id.clone(),
        batch_number: s.batch_number,
        status: format!("{:?}", s.status).to_lowercase(),
        roasted_at: s.start_time.unwrap_or(s.created_at).to_rfc3339(),
        coffee: i.bean.as_ref().map(|b| b.name.clone()),
        green_g: s.green_weight.map(f64::from),
        roasted_g: s.roasted_weight.map(f64::from),
        loss_pct: loss.map(r1),
        charge_bt,
        total_s: r1(total),
        dropped: drop_t.is_some(),
        development_s: development.map(r1),
        dtr_pct: dtr.map(r1),
        max_bt: roast.iter().filter_map(|x| x.bt).reduce(f64::max).map(r1),
        auc: s.auc_value.map(|a| r1(f64::from(a))),
        marks,
        phases,
        ror,
        profile,
        control,
        expected,
        notes,
        channels,
    };
    Analysed { analysis, samples }
}

// ------------------------------------------------------------------ compare --

/// One number across the roasts, with each one's difference from the reference.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Row {
    pub key: &'static str,
    pub label: &'static str,
    /// `°C`, `s`, `%`, `°C/min`, `°C/min/min`
    pub unit: &'static str,
    /// In the order of `roasts`
    pub values: Vec<Option<f64>>,
    /// Value − the reference's
    pub deltas: Vec<Option<f64>>,
}

/// The roasts at one roast time.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct CurvePoint {
    /// Seconds from CHARGE
    pub t: f64,
    pub bt: Vec<Option<f64>>,
    pub ror: Vec<Option<f64>>,
    pub heater: Vec<Option<f64>>,
    pub fan: Vec<Option<f64>>,
}

/// How closely one roast's BT followed the reference's.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Overlay {
    pub session_id: String,
    /// From this roast time on BT stays within `within` °C of the reference's (to the end of
    /// the shorter roast); null when it does not end within it
    pub within_from_s: Option<f64>,
    /// Over the time both roasts ran
    pub rms_bt_delta: Option<f64>,
    pub max_bt_delta: Option<At>,
}

/// Several roasts against one.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Comparison {
    /// The reference's session id (the first roast unless chosen)
    pub reference: String,
    /// The tolerance of `overlay`, °C
    pub within: f64,
    pub roasts: Vec<RoastAnalysis>,
    pub rows: Vec<Row>,
    /// Every 30 s from CHARGE to the longest roast's drop
    pub curve: Vec<CurvePoint>,
    /// Each roast but the reference
    pub overlay: Vec<Overlay>,
}

type Pick = fn(&RoastAnalysis) -> Option<f64>;

fn mark_of(a: &RoastAnalysis, key: &str) -> Option<f64> {
    a.marks.iter().find(|m| m.key == key).map(|m| m.t)
}

fn mark_bt(a: &RoastAnalysis, key: &str) -> Option<f64> {
    a.marks.iter().find(|m| m.key == key).and_then(|m| m.bt)
}

fn ror_minute(a: &RoastAnalysis, t: f64) -> Option<f64> {
    a.ror.by_minute.iter().find(|p| p.t == t).map(|p| p.value)
}

fn early_dev(a: &RoastAnalysis, t: f64) -> Option<f64> {
    a.profile
        .as_ref()?
        .early
        .iter()
        .find(|p| p.t == t)
        .map(|p| p.value)
}

const ROWS: &[(&str, &str, &str, Pick)] = &[
    ("charge_bt", "BT at CHARGE", "°C", |a| a.charge_bt),
    ("heater_opening", "Heater, first 30 s", "%", |a| {
        a.control.heater_opening
    }),
    ("bt_vs_profile_30", "BT − profile at 0:30", "°C", |a| {
        early_dev(a, 30.0)
    }),
    (
        "early_peak",
        "Largest BT − profile, first 2:00",
        "°C",
        |a| a.profile.as_ref()?.early_peak.map(|p| p.value),
    ),
    ("turning_point_s", "Turning point", "s", |a| {
        mark_of(a, "turning_point")
    }),
    ("turning_point_bt", "Turning point BT", "°C", |a| {
        mark_bt(a, "turning_point")
    }),
    ("ror_peak", "RoR peak", "°C/min", |a| {
        a.ror.peak.map(|p| p.value)
    }),
    ("ror_peak_s", "RoR peak at", "s", |a| {
        a.ror.peak.map(|p| p.t)
    }),
    ("ror_60", "RoR at 1:00", "°C/min", |a| ror_minute(a, 60.0)),
    ("ror_120", "RoR at 2:00", "°C/min", |a| {
        ror_minute(a, 120.0)
    }),
    ("drying_end_s", "Dry end", "s", |a| mark_of(a, "drying_end")),
    ("first_crack_s", "First crack", "s", |a| {
        mark_of(a, "first_crack_start")
    }),
    ("first_crack_bt", "First crack BT", "°C", |a| {
        mark_bt(a, "first_crack_start")
    }),
    ("drop_s", "Drop", "s", |a| mark_of(a, "drop")),
    ("drop_bt", "Drop BT", "°C", |a| mark_bt(a, "drop")),
    ("development_s", "Development", "s", |a| a.development_s),
    ("dtr_pct", "DTR", "%", |a| a.dtr_pct),
    ("ror_late_min", "Lowest RoR late", "°C/min", |a| {
        a.ror.late_min.map(|p| p.value)
    }),
    ("ror_slope", "RoR slope FC → drop", "°C/min/min", |a| {
        a.ror.fc_drop_slope
    }),
    ("late_heater", "Heater, last minute", "%", |a| {
        a.control.late_heater
    }),
    ("late_fan", "Fan, last minute", "%", |a| a.control.late_fan),
    ("heater_high_s", "Heater at 90 % or more", "s", |a| {
        Some(a.control.heater_high_s)
    }),
    ("profile_rmse", "Profile RMS", "°C", |a| {
        a.profile.as_ref()?.rmse
    }),
    ("profile_bias", "Profile bias", "°C", |a| {
        a.profile.as_ref()?.bias
    }),
    ("loss_pct", "Weight loss", "%", |a| a.loss_pct),
];

/// Line `list` up against `list[reference]`: every number, the curves every 30 s, and where
/// each roast's BT settles within `within` °C of the reference's.
pub fn compare(list: &[Analysed], reference: usize, within: f64) -> Comparison {
    let refa = &list[reference];
    let rows: Vec<Row> = ROWS
        .iter()
        .map(|(key, label, unit, pick)| {
            let base = pick(&refa.analysis);
            let values: Vec<Option<f64>> = list.iter().map(|a| pick(&a.analysis)).collect();
            Row {
                key,
                label,
                unit,
                deltas: values
                    .iter()
                    .map(|v| v.zip(base).map(|(v, b)| r2(v - b)))
                    .collect(),
                values,
            }
        })
        .filter(|r| r.values.iter().any(Option::is_some))
        .collect();
    let longest = list.iter().map(|a| a.analysis.total_s).fold(0.0, f64::max);
    #[expect(
        clippy::maybe_infinite_iter,
        reason = "take_while ends it: t only grows"
    )]
    let curve: Vec<CurvePoint> = (0..)
        .map(|k| f64::from(k) * 30.0)
        .take_while(|t| *t <= longest)
        .map(|t| {
            let each = |pick: &dyn Fn(&Analysed) -> Option<f64>| -> Vec<Option<f64>> {
                list.iter()
                    .map(|a| (t <= a.analysis.total_s).then(|| pick(a)).flatten().map(r1))
                    .collect()
            };
            CurvePoint {
                t,
                bt: each(&|a| bt_at(&a.samples, t)),
                ror: each(&|a| {
                    if t >= 15.0 {
                        near(&a.samples, t, 3.0, |x| x.ror)
                    } else {
                        None
                    }
                }),
                heater: each(&|a| near(&a.samples, t, 3.0, |x| x.heater)),
                fan: each(&|a| near(&a.samples, t, 3.0, |x| x.fan)),
            }
        })
        .collect();
    let overlay = list
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != reference)
        .map(|(_, a)| {
            let end = a.analysis.total_s.min(refa.analysis.total_s);
            #[expect(
                clippy::maybe_infinite_iter,
                reason = "take_while ends it: t only grows"
            )]
            let deltas: Vec<(f64, f64)> = (0..)
                .map(|k| f64::from(k) * 5.0)
                .take_while(|t| *t <= end)
                .filter_map(|t| Some((t, bt_at(&a.samples, t)? - bt_at(&refa.samples, t)?)))
                .collect();
            let outside = deltas
                .iter()
                .rev()
                .find(|d| d.1.abs() > within)
                .map(|d| d.0);
            let within_from_s = match outside {
                None => deltas.first().map(|d| d.0),
                Some(t) if t + 5.0 <= end => Some(t + 5.0),
                Some(_) => None,
            };
            let rms = (!deltas.is_empty()).then(|| {
                (deltas.iter().map(|d| d.1 * d.1).sum::<f64>() / deltas.len() as f64).sqrt()
            });
            let max = deltas
                .iter()
                .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
                .map(|d| At {
                    t: d.0,
                    value: r1(d.1),
                });
            Overlay {
                session_id: a.analysis.session_id.clone(),
                within_from_s,
                rms_bt_delta: rms.map(r2),
                max_bt_delta: max,
            }
        })
        .collect();
    Comparison {
        reference: refa.analysis.session_id.clone(),
        within,
        roasts: list.iter().map(|a| a.analysis.clone()).collect(),
        rows,
        curve,
        overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasts::types::*;
    use chrono::Utc;

    fn session(id: &str) -> RoastSession {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "device_id": "virtual_roaster", "status": "completed",
            "created_at": Utc::now(), "updated_at": Utc::now(),
            "green_weight": 100.0, "roasted_weight": 86.0,
        }))
        .unwrap()
    }

    fn event(id: &str, t: RoastEventType, at: f32, temp: Option<f32>) -> RoastEvent {
        serde_json::from_value(serde_json::json!({
            "id": format!("{id}-{t:?}"), "session_id": id, "event_type": t,
            "elapsed_seconds": at, "temperature": temp, "created_at": Utc::now(),
        }))
        .unwrap()
    }

    /// A roast that charges at session time 10 s: BT rises fast to 90 by 0:45, then climbs on a
    /// falling RoR to drop at 9:00; the heater sits at 60 % then rises to 95 % late.
    fn roast(id: &str, offset: f64, heater_late: f64) -> Input {
        let charge = 10.0f32;
        let bt = |t: f64| -> f64 {
            if t < 45.0 {
                25.0 + (90.0 - 25.0) * (t / 45.0) + offset
            } else {
                90.0 + 105.0 * (1.0 - (-(t - 45.0) / 260.0).exp()) + offset
            }
        };
        let ror = |t: f64| (bt(t + 1.0) - bt(t - 1.0)) / 2.0 * 60.0;
        let telemetry = (0..=560)
            .map(|k| {
                let t = f64::from(k) - 10.0;
                SessionTelemetry {
                    id: format!("{id}-{k}"),
                    session_id: id.into(),
                    timestamp: Utc::now(),
                    elapsed_seconds: k as f32,
                    bean_temp: Some(bt(t.max(0.0)) as f32),
                    env_temp: Some(220.0),
                    rate_of_rise: (t > 0.0).then(|| ror(t) as f32),
                    heater: Some(if t > 420.0 { heater_late } else { 60.0 }),
                    fan: Some(if t > 420.0 { 79.0 } else { 100.0 }),
                    setpoint: None,
                    extras: Some(serde_json::json!({ "t3": 150.0 + t.max(0.0) / 10.0 })),
                }
            })
            .collect();
        let events = vec![
            event(
                id,
                RoastEventType::Charge,
                charge,
                Some(25.0 + offset as f32),
            ),
            event(id, RoastEventType::TurningPoint, charge + 45.0, None),
            event(id, RoastEventType::DryingEnd, charge + 240.0, None),
            event(id, RoastEventType::FirstCrackStart, charge + 450.0, None),
            event(id, RoastEventType::Drop, charge + 540.0, None),
        ];
        Input {
            session: session(id),
            telemetry,
            events,
            profile: None,
            cupping: None,
            bean: None,
            cost: None,
            roaster: None,
            history: vec![],
            channels: vec![crate::roasters::channels::Channel {
                key: "t3".into(),
                label: "Exhaust".into(),
                unit: "C".into(),
                max: None,
            }],
        }
    }

    #[test]
    fn a_roast_is_read_from_charge() {
        let a = analyse(&roast("a", 0.0, 95.0)).analysis;
        assert_eq!(a.total_s, 540.0);
        assert!(a.dropped);
        // The declared channel at CHARGE, first crack and DROP (docs/channels.md)
        let ch = &a.channels[0];
        assert_eq!((ch.key.as_str(), ch.label.as_str()), ("t3", "Exhaust"));
        assert_eq!(
            (ch.charge, ch.first_crack, ch.drop),
            (Some(150.0), Some(195.0), Some(204.0))
        );
        assert_eq!(a.charge_bt, Some(25.0));
        assert_eq!(a.development_s, Some(90.0));
        assert_eq!(a.dtr_pct, Some(16.7));
        assert_eq!(a.loss_pct, Some(14.0));
        let keys: Vec<&str> = a.phases.iter().map(|p| p.key).collect();
        assert_eq!(keys, ["drying", "maillard", "development"]);
        assert_eq!(a.phases[0].from_s, 0.0);
        assert_eq!(a.phases[2].duration_s, 90.0);
        // The RoR peak is right after the turning point; the late minimum just before drop.
        let peak = a.ror.peak.unwrap();
        assert!(peak.t >= 45.0 && peak.t < 60.0, "{peak:?}");
        let late = a.ror.late_min.unwrap();
        assert!(late.t > 500.0, "{late:?}");
        assert!(a.ror.fc_drop_slope.unwrap() < 0.0);
        assert_eq!(a.ror.by_minute.len(), 9);
        // The heater: 60 % at CHARGE, 95 % over the last minute, high for the last 2 min.
        assert_eq!(a.control.heater_opening, Some(60.0));
        assert_eq!(a.control.late_heater, Some(95.0));
        assert!(
            (a.control.heater_high_s - 120.0).abs() <= 2.0,
            "{}",
            a.control.heater_high_s
        );
        assert_eq!(a.control.heater_saturated_s, 0.0);
        assert!(
            a.notes
                .iter()
                .any(|n| n.starts_with("Little heater left late")),
            "{:?}",
            a.notes
        );
    }

    #[test]
    fn the_eval_roasts_read_as_task_31_expects() {
        // The agent eval's seeded roasts: BT 25 + 0.31 °C/s every 5 s, CHARGE 0,
        // DRY 4:00, FC 7:40 or 7:50, DROP 10:00.
        let seeded = |id: &str, fc: f32| {
            let telemetry = (0..125)
                .map(|k| SessionTelemetry {
                    id: format!("{id}-{k}"),
                    session_id: id.into(),
                    timestamp: Utc::now(),
                    elapsed_seconds: (k * 5) as f32,
                    bean_temp: Some(25.0 + 0.31 * (k * 5) as f32),
                    env_temp: None,
                    rate_of_rise: None,
                    heater: None,
                    fan: None,
                    setpoint: None,
                    extras: None,
                })
                .collect();
            let events = vec![
                event(id, RoastEventType::Charge, 0.0, None),
                event(id, RoastEventType::DryingEnd, 240.0, None),
                event(id, RoastEventType::FirstCrackStart, fc, None),
                event(id, RoastEventType::Drop, 600.0, None),
            ];
            analyse(&Input {
                telemetry,
                events,
                ..roast(id, 0.0, 60.0)
            })
        };
        let c = compare(&[seeded("t2", 460.0), seeded("t3", 470.0)], 0, 0.5);
        let (a, b) = (&c.roasts[0], &c.roasts[1]);
        assert_eq!((a.development_s, a.dtr_pct), (Some(140.0), Some(23.3)));
        assert_eq!((b.development_s, b.dtr_pct), (Some(130.0), Some(21.7)));
        let fc = c.rows.iter().find(|r| r.key == "first_crack_s").unwrap();
        assert_eq!(fc.deltas, vec![Some(0.0), Some(10.0)]);
        // RoR computed from BT where the roast has none: 0.31 °C/s is 18.6 °C/min.
        assert!(
            (a.ror.by_minute[4].value - 18.6).abs() < 0.2,
            "{:?}",
            a.ror.by_minute
        );
    }

    #[test]
    fn no_headroom_note_when_the_heater_had_room() {
        let a = analyse(&roast("a", 0.0, 70.0)).analysis;
        assert!(
            a.notes.iter().all(|n| !n.contains("heater")),
            "{:?}",
            a.notes
        );
    }

    #[test]
    fn roasts_compare_against_the_reference() {
        let list = [
            analyse(&roast("a", 0.0, 95.0)),
            analyse(&roast("b", 0.3, 95.0)),
            analyse(&roast("c", 2.0, 95.0)),
        ];
        let c = compare(&list, 0, 0.5);
        assert_eq!(c.reference, "a");
        let charge = c.rows.iter().find(|r| r.key == "charge_bt").unwrap();
        assert_eq!(charge.deltas, vec![Some(0.0), Some(0.3), Some(2.0)]);
        assert_eq!(c.curve.first().unwrap().t, 0.0);
        assert_eq!(c.curve.last().unwrap().t, 540.0);
        assert_eq!(c.curve[2].bt.len(), 3);
        // b is within 0.5 °C throughout; c never is.
        assert_eq!(c.overlay[0].within_from_s, Some(0.0));
        assert_eq!(c.overlay[1].within_from_s, None);
        assert_eq!(c.overlay[1].max_bt_delta.unwrap().value, 2.0);
    }

    #[test]
    fn overlay_finds_where_two_roasts_come_together() {
        // b runs 3 °C hot until 2:00, then the same as a.
        let a = analyse(&roast("a", 0.0, 95.0));
        let mut bi = roast("b", 0.0, 95.0);
        for p in &mut bi.telemetry {
            if f64::from(p.elapsed_seconds) - 10.0 < 120.0 {
                p.bean_temp = p.bean_temp.map(|v| v + 3.0);
            }
        }
        let b = analyse(&bi);
        let c = compare(&[a, b], 0, 0.5);
        assert_eq!(c.overlay[0].within_from_s, Some(120.0));
    }
}
