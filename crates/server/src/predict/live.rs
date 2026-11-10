//! The live prediction: BT for the next two minutes under the plan, first crack and DROP
//! (docs/prediction.md, "Live").

use serde::Serialize;
use std::fmt::Write as _;
use utoipa::ToSchema;

use super::model::{
    FitQuality, Mode, State, ThermalModel, Tick, drive, projection, sim_closed_loop,
};
use super::ror_watch::{self, RorWatch, WatchInput};
use crate::replay::engine::PidGains;

/// Seconds ahead drawn as the band.
pub const HORIZON_S: usize = 120;
/// Seconds ahead looked at for first crack and DROP.
const LONG_S: usize = 900;
/// The band's points are this far apart, s.
const POINT_EVERY_S: usize = 5;

/// What the roaster will do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// Manual: the heater and fan stay where they are
    Held,
    /// The roaster's PID holds its setpoint
    Setpoint,
    /// The profile drive's setpoints (or heater curve) from here on
    Drive,
}

/// The plan, one value a second from now (index 0 is now); past the end the last holds.
#[derive(Debug, Clone)]
pub struct Plan {
    pub kind: PlanKind,
    /// Setpoints the PID follows; None runs open loop on `heater`
    pub setpoints: Option<Vec<f64>>,
    /// The profile's BT without lookahead (what "behind the plan" is measured against)
    pub targets: Option<Vec<f64>>,
    /// Heater %, open loop (manual, or a feedforward drive)
    pub heater: Vec<f64>,
    pub fan: Vec<f64>,
    /// The PID's gains (heater % units)
    pub gains: Option<PidGains>,
    /// Says where the plan comes from: "Light espresso's setpoints, 8 s ahead"
    pub label: String,
}

fn at(v: &[f64], k: usize) -> f64 {
    v.get(k).or(v.last()).copied().unwrap_or(0.0)
}

/// First crack in this roaster's history: the BT it came at.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FcHistory {
    pub bt_mean: f64,
    pub bt_sd: f64,
    pub roasts: usize,
    /// "3 roasts of Kenya AA on Light espresso"
    pub basis: String,
}

/// Where the roast drops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropTarget {
    /// At this time since CHARGE, s
    Time(f64),
    /// At this BT, °C
    Bt(f64),
    /// This long after first crack, s
    Development(f64),
}

pub struct LiveInput<'a> {
    pub model: &'a ThermalModel,
    pub quality: &'a FitQuality,
    /// Samples since CHARGE, ascending
    pub ticks: &'a [Tick],
    pub plan: Plan,
    pub fc_history: Option<FcHistory>,
    /// First crack marked, s since CHARGE
    pub fc_marked: Option<f64>,
    /// DRY end marked: the RoR watch is armed from here (else from 160 °C)
    pub dry_marked: bool,
    pub drop_target: Option<(DropTarget, String)>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PredictedPoint {
    /// Seconds since CHARGE
    pub t: f64,
    pub bt: f64,
    /// The 90 % band
    pub low: f64,
    pub high: f64,
    /// Heater % the plan takes
    pub heater: f64,
}

/// A predicted landmark: when, the range, BT then and why.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Landmark {
    /// Seconds since CHARGE
    pub t: f64,
    pub low: f64,
    pub high: f64,
    pub bt: Option<f64>,
    pub reason: String,
}

/// The plan runs out of heater.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HeaterLimit {
    /// When the heater reaches 100 %, s since CHARGE
    pub t: f64,
    /// BT under the profile at the end of the band, °C (positive: behind)
    pub behind_c: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Prediction {
    /// Now, s since CHARGE
    pub t: f64,
    pub bt: f64,
    pub plan: PlanKind,
    /// Which of the fit's scores the band comes from
    pub mode: Mode,
    /// Why the line goes where it goes, in one line
    pub reason: String,
    /// Every 5 s to two minutes ahead
    pub points: Vec<PredictedPoint>,
    pub bt_60: f64,
    pub bt_120: f64,
    /// The band two minutes ahead, ± °C
    pub band_120: f64,
    /// Today's projection (RoR now, bent by a falling RoR), for comparison
    pub projection_60: Option<f64>,
    pub projection_120: Option<f64>,
    pub heater_limit: Option<HeaterLimit>,
    pub first_crack: Option<Landmark>,
    pub drop: Option<Landmark>,
    /// The RoR now and ahead, and a warning before a crash, a flick or a stall
    pub ror: RorWatch,
}

/// The band at `dt` seconds ahead, ± °C: the fit's 90 % error, growing with the square
/// root of the time past two minutes; without a score about 5 °C at two minutes.
fn band(q: &FitQuality, mode: Mode, dt: f64) -> f64 {
    let h = HORIZON_S as f64;
    let at = q.band(mode, dt.min(h)).unwrap_or(dt.min(h) / 24.0);
    if dt > h { at * (dt / h).sqrt() } else { at }
}

/// The lag state now, from the samples since CHARGE.
fn state_now(m: &ThermalModel, ticks: &[Tick]) -> Option<State> {
    let last = ticks.iter().rev().find(|k| k.bt.is_some())?;
    let mut x: Option<f64> = None;
    let mut prev: Option<&Tick> = None;
    for k in ticks {
        let u = drive(k.heater.unwrap_or(0.0), k.fan.unwrap_or(0.0));
        x = Some(match (x, prev) {
            (Some(x), Some(p)) => {
                let dt = (k.t - p.t).clamp(0.0, 5.0);
                let up = drive(p.heater.unwrap_or(0.0), p.fan.unwrap_or(0.0));
                x + (up - x) * dt / (m.tau_s + dt)
            }
            _ => u,
        });
        prev = Some(k);
    }
    Some(State {
        bt: last.bt?,
        x: x?,
    })
}

/// When `bt` first reaches `c` (interpolated), as seconds ahead.
fn crossing(bt: &[f64], bt0: f64, c: f64) -> Option<f64> {
    let mut prev = bt0;
    for (k, &b) in bt.iter().enumerate() {
        if b >= c {
            let w = if b > prev {
                (c - prev) / (b - prev)
            } else {
                1.0
            };
            return Some(k as f64 + w.clamp(0.0, 1.0));
        }
        prev = b;
    }
    None
}

pub fn mmss(t: f64) -> String {
    let s = t.max(0.0).round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn predict(inp: &LiveInput) -> Option<Prediction> {
    let m = inp.model;
    let s0 = state_now(m, inp.ticks)?;
    let t_now = inp.ticks.iter().rev().find(|k| k.bt.is_some())?.t;
    let heater_now = inp.ticks.last()?.heater.unwrap_or(0.0);
    let p = &inp.plan;

    // BT (and heater) each second, far enough for first crack and DROP.
    let (bt, heater): (Vec<f64>, Vec<f64>) = match (&p.setpoints, p.gains) {
        (Some(sp), Some(g)) => sim_closed_loop(
            m,
            s0,
            heater_now,
            &|k| at(&p.fan, k),
            g,
            &|k| at(sp, k),
            LONG_S,
        )
        .into_iter()
        .unzip(),
        _ => {
            let mut s = s0;
            (0..LONG_S)
                .map(|k| {
                    let h = at(&p.heater, k);
                    m.step(&mut s, drive(h, at(&p.fan, k)), 1.0);
                    (s.bt, h)
                })
                .unzip()
        }
    };
    let mode = if p.setpoints.is_some() && p.gains.is_some() {
        Mode::Plan
    } else {
        Mode::Held
    };
    let q = inp.quality;
    let points: Vec<PredictedPoint> = (POINT_EVERY_S..=HORIZON_S)
        .step_by(POINT_EVERY_S)
        .map(|k| {
            let b = band(q, mode, k as f64);
            PredictedPoint {
                t: t_now + k as f64,
                bt: r1(bt[k - 1]),
                low: r1(bt[k - 1] - b),
                high: r1(bt[k - 1] + b),
                heater: r1(heater[k - 1]),
            }
        })
        .collect();

    let heater_limit = (p.setpoints.is_some())
        .then(|| heater[..HORIZON_S].iter().position(|&h| h >= 99.5))
        .flatten()
        .map(|k| HeaterLimit {
            t: r1(t_now + k as f64 + 1.0),
            behind_c: p
                .targets
                .as_ref()
                .map(|tg| r1(at(tg, HORIZON_S) - bt[HORIZON_S - 1])),
        });

    // First crack: where BT reaches this bean's and profile's FC BT.
    let rate_at = |k: usize| {
        let a = bt[k.saturating_sub(1).min(LONG_S - 1)];
        let b = bt[(k + 1).min(LONG_S - 1)];
        ((b - a) / 2.0).max(0.01)
    };
    let first_crack = match (&inp.fc_history, inp.fc_marked) {
        // BT already past it with first crack not marked: no time to give.
        (Some(h), None) if s0.bt < h.bt_mean => {
            let sd = h.bt_sd.max(0.5);
            crossing(&bt, s0.bt, h.bt_mean).map(|dt| {
                let k = dt.round() as usize;
                let tb = band(q, mode, dt) / rate_at(k);
                let lo = crossing(&bt, s0.bt, h.bt_mean - sd).unwrap_or(0.0) - tb;
                let hi = crossing(&bt, s0.bt, h.bt_mean + sd).unwrap_or(LONG_S as f64) + tb;
                Landmark {
                    t: r1(t_now + dt),
                    low: r1(t_now + lo.max(0.0)),
                    high: r1(t_now + hi),
                    bt: Some(r1(h.bt_mean)),
                    reason: format!(
                        "BT reaches {:.1} °C (± {:.1}), first crack on {}",
                        h.bt_mean, sd, h.basis
                    ),
                }
            })
        }
        _ => None,
    };

    let bt_at = |t: f64| {
        let k = (t - t_now).round();
        (k >= 1.0 && k <= LONG_S as f64).then(|| r1(bt[k as usize - 1]))
    };
    let drop = inp.drop_target.as_ref().and_then(|(target, what)| {
        let (t, lo, hi, reason) = match *target {
            DropTarget::Time(t) => (t, t, t, what.clone()),
            DropTarget::Bt(c) => {
                let dt = crossing(&bt, s0.bt, c)?;
                let tb = band(q, mode, dt) / rate_at(dt.round() as usize);
                (t_now + dt, t_now + dt - tb, t_now + dt + tb, what.clone())
            }
            DropTarget::Development(s) => match (inp.fc_marked, &first_crack) {
                (Some(fc), _) => (fc + s, fc + s, fc + s, what.clone()),
                (None, Some(fc)) => (
                    fc.t + s,
                    fc.low + s,
                    fc.high + s,
                    format!("{what}, after the predicted first crack"),
                ),
                _ => return None,
            },
        };
        (t > t_now).then(|| Landmark {
            t: r1(t),
            low: r1(lo.max(t_now)),
            high: r1(hi),
            bt: bt_at(t),
            reason,
        })
    });

    let ror = ror_watch::watch(&WatchInput {
        model: m,
        ticks: inp.ticks,
        t_now,
        bt_now: s0.bt,
        bt: &bt,
        heater: &heater,
        fan: &p.fan,
        targets: p.targets.as_deref(),
        dry_marked: inp.dry_marked,
        fc_marked: inp.fc_marked,
        fc_bt: inp.fc_history.as_ref().map(|h| h.bt_mean),
        drop_t: drop.as_ref().map(|d| d.t),
        drop_bt: match inp.drop_target {
            Some((DropTarget::Bt(c), _)) => Some(c),
            _ => None,
        },
    });

    let b120 = band(q, mode, HORIZON_S as f64);
    let basis = if q.roasts == 0 {
        "the virtual roaster's model (no roasts yet)".to_string()
    } else if q.modes.is_empty() {
        format!("a model from {} roast (not scored yet)", q.roasts)
    } else {
        format!("a model from {} roasts", q.roasts)
    };
    let mut reason = format!("{}: {basis}, ± {:.1} °C at 2 min (90 %)", p.label, b120);
    if let Some(l) = &heater_limit {
        let _ = write!(reason, "; the heater reaches 100 % at {}", mmss(l.t));
        if let Some(b) = l.behind_c.filter(|b| *b > 0.5) {
            let _ = write!(reason, ", BT {b:.1} °C behind the profile at +2 min");
        }
    }
    let lo = inp.ticks.len().saturating_sub(40);
    let recent = &inp.ticks[lo..];
    let tt: Vec<f64> = recent.iter().map(|k| k.t).collect();
    let rr: Vec<Option<f64>> = recent.iter().map(|k| k.ror).collect();
    Some(Prediction {
        t: r1(t_now),
        bt: r1(s0.bt),
        plan: p.kind,
        mode,
        reason,
        bt_60: r1(bt[59]),
        bt_120: r1(bt[HORIZON_S - 1]),
        band_120: r1(b120),
        projection_60: projection(&tt, s0.bt, &rr, 60.0).map(r1),
        projection_120: projection(&tt, s0.bt, &rr, 120.0).map(r1),
        points,
        heater_limit,
        first_crack,
        drop,
        ror,
    })
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(n: usize, heater: f64, fan: f64, bt0: f64, rate: f64) -> Vec<Tick> {
        (0..n)
            .map(|i| Tick {
                t: i as f64,
                bt: Some(bt0 + rate * i as f64),
                heater: Some(heater),
                fan: Some(fan),
                ror: Some(rate * 60.0),
                ..Tick::default()
            })
            .collect()
    }

    fn quality() -> FitQuality {
        FitQuality {
            roasts: 0,
            samples: 0,
            modes: vec![],
        }
    }

    #[test]
    fn held_heat_settles_toward_the_models_equilibrium() {
        let m = ThermalModel::prior();
        let tk = ticks(300, 88.0, 100.0, 100.0, 0.2);
        let q = quality();
        let p = predict(&LiveInput {
            model: &m,
            quality: &q,
            ticks: &tk,
            plan: Plan {
                kind: PlanKind::Held,
                setpoints: None,
                targets: None,
                heater: vec![88.0],
                fan: vec![100.0],
                gains: None,
                label: "Heater 88 % and fan 100 % held".into(),
            },
            fc_history: None,
            fc_marked: None,
            dry_marked: false,
            drop_target: None,
        })
        .unwrap();
        assert_eq!(p.points.len(), 24);
        let settle = m.settles_at(88.0, 100.0).unwrap();
        assert!(
            p.bt_120 > p.bt && p.bt_120 < settle,
            "{} {settle}",
            p.bt_120
        );
        assert!(p.points[23].high > p.points[0].high - p.points[0].bt + p.points[23].bt - 0.01);
        assert!(p.reason.contains("virtual roaster"));
    }

    #[test]
    fn a_plan_beyond_the_heater_reports_the_limit_and_first_crack_comes_from_history() {
        let m = ThermalModel::prior();
        let tk = ticks(400, 90.0, 100.0, 120.0, 0.1);
        let q = quality();
        // A plan climbing 30 °C a minute: no heater keeps up.
        let sp: Vec<f64> = (0..900).map(|k| 190.0 + k as f64 * 0.5).collect();
        let p = predict(&LiveInput {
            model: &m,
            quality: &q,
            ticks: &tk,
            plan: Plan {
                kind: PlanKind::Drive,
                setpoints: Some(sp.clone()),
                targets: Some(sp),
                heater: vec![],
                fan: vec![100.0],
                gains: Some(PidGains {
                    kp: 3.0,
                    ki: 0.05,
                    kd: 0.0,
                }),
                label: "Test's setpoints".into(),
            },
            fc_history: Some(FcHistory {
                bt_mean: 175.0,
                bt_sd: 1.0,
                roasts: 3,
                basis: "3 roasts".into(),
            }),
            fc_marked: None,
            dry_marked: false,
            drop_target: Some((
                DropTarget::Development(90.0),
                "1:30 after first crack".into(),
            )),
        })
        .unwrap();
        let lim = p.heater_limit.as_ref().unwrap();
        assert!(lim.t < p.t + 30.0, "{lim:?}");
        assert!(lim.behind_c.unwrap() > 10.0);
        assert!(p.reason.contains("heater reaches 100 %"));
        // The prior at full heat and fan settles near 184: first crack at 175 comes.
        let fc = p.first_crack.as_ref().unwrap();
        assert!(fc.low <= fc.t && fc.t <= fc.high, "{fc:?}");
        let d = p.drop.as_ref().unwrap();
        assert!((d.t - fc.t - 90.0).abs() < 0.2);
    }
}
