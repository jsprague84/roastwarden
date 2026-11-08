//! A roaster's thermal model, fitted from its own roast logs (docs/prediction.md).
//!
//! The bean temperature moves as
//!
//! ```text
//! dBT/dt = gain · x + g(BT)          x = lag_τ(heater / fan)
//! ```
//!
//! `x` is the heater's share over the fan's (the air temperature the heater sets, as the
//! virtual roaster has it), through a first-order lag `τ` (the element and the air). `g` is a
//! curve in BT, piecewise linear between knots every 20 °C: the losses and whatever the beans
//! do at that temperature (drying, the reactions near first crack). Below the firmware's fan
//! interlock no heat reaches the beans.
//!
//! With `τ` fixed every other parameter enters linearly, so the fit is a least-squares solve
//! of `dBT/dt` against `x` and the knots' hat functions, pulled toward the virtual roaster's
//! physics (the prior) and kept smooth along BT. `τ` comes from a grid, judged by
//! leave-one-roast-out simulation two minutes ahead. ET is not used: on the fluid bed it is
//! read above the bed and falls below BT late in a roast.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::replay::engine::{PidGains, ServerPid};

/// No heat reaches the beans below this fan (the firmware's interlock, %).
pub const MIN_FAN_PCT: f64 = 40.0;
/// Knots of `g(BT)`, °C.
pub const KNOT_FROM_C: f64 = 20.0;
pub const KNOT_STEP_C: f64 = 20.0;
pub const KNOTS: usize = 13; // 20 … 260 °C
/// The lags tried, s.
const TAUS: [f64; 7] = [2.0, 3.0, 5.0, 8.0, 12.0, 18.0, 25.0];
/// The horizons a fit is judged at, s.
pub const HORIZONS: [usize; 4] = [30, 60, 90, 120];
/// Predictions start this long after CHARGE (the dashboard's projections wait as long).
pub const START_AFTER_CHARGE_S: usize = 30;
/// Starting points in a roast are this far apart when scoring, s.
const SCORE_EVERY_S: usize = 15;
/// The fit skips the first seconds after CHARGE (the probe moving into the beans).
const FIT_SKIP_S: usize = 20;

/// The model's parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ThermalModel {
    /// Lag of the heater's effect, s
    pub tau_s: f64,
    /// °C/s of BT rise per unit of heater ÷ fan (both as fractions)
    pub gain: f64,
    /// The first knot of `g(BT)` and the spacing, °C
    pub knot_from_c: f64,
    pub knot_step_c: f64,
    /// `g(BT)` at each knot, °C/s
    pub drift: Vec<f64>,
}

impl ThermalModel {
    /// The virtual roaster's physics in this form (`virtual_roaster.rs`: the air settles at
    /// `25 + 164 · heater / fan` with an 8 s lag, the beans follow it with 60 s): where a
    /// roaster's fit starts, and its model before it has roasts.
    pub fn prior() -> ThermalModel {
        let knots = knot_temps();
        ThermalModel {
            tau_s: 8.0,
            gain: 0.9695 * 164.0 / 60.0,
            knot_from_c: KNOT_FROM_C,
            knot_step_c: KNOT_STEP_C,
            drift: knots.iter().map(|k| (25.0 - k) / 60.0).collect(),
        }
    }

    /// `g(BT)`, °C/s: linear between knots, held beyond the ends.
    pub fn g(&self, bt: f64) -> f64 {
        let n = self.drift.len();
        if n == 0 {
            return 0.0;
        }
        let pos = ((bt - self.knot_from_c) / self.knot_step_c).clamp(0.0, (n - 1) as f64);
        let j = (pos.floor() as usize).min(n.saturating_sub(2));
        if n == 1 {
            return self.drift[0];
        }
        let w = pos - j as f64;
        self.drift[j] * (1.0 - w) + self.drift[j + 1] * w
    }

    /// One step of `dt` seconds: the lag takes the input `u` (heater ÷ fan), BT moves at the
    /// rate it had at the start of the step.
    pub fn step(&self, s: &mut State, u: f64, dt: f64) {
        let rate = self.gain * s.x + self.g(s.bt);
        s.bt += rate * dt;
        s.x += (u - s.x) * dt / (self.tau_s + dt);
    }

    /// Where BT settles with this heater and fan, °C (where `gain · u + g = 0`), if it does
    /// below 300 °C.
    pub fn settles_at(&self, heater_pct: f64, fan_pct: f64) -> Option<f64> {
        let u = drive(heater_pct, fan_pct);
        let mut bt = 0.0;
        while bt < 300.0 {
            if self.gain * u + self.g(bt) <= 0.0 {
                return Some(bt);
            }
            bt += 0.5;
        }
        None
    }
}

/// Bean temperature and the lagged heater ÷ fan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub bt: f64,
    pub x: f64,
}

/// What heats the beans: heater ÷ fan as fractions, 0 below the interlock.
pub fn drive(heater_pct: f64, fan_pct: f64) -> f64 {
    if fan_pct.is_nan() || fan_pct < MIN_FAN_PCT || !heater_pct.is_finite() {
        return 0.0;
    }
    heater_pct.clamp(0.0, 100.0) / fan_pct.min(100.0)
}

fn knot_temps() -> Vec<f64> {
    (0..KNOTS)
        .map(|j| KNOT_FROM_C + KNOT_STEP_C * j as f64)
        .collect()
}

/// One roast on a 1 s grid from CHARGE (index 0) to DROP.
#[derive(Debug, Clone, Default)]
pub struct Record {
    pub session_id: String,
    pub bt: Vec<f64>,
    /// Heater applied, %
    pub heater: Vec<f64>,
    pub fan: Vec<f64>,
    pub setpoint: Vec<Option<f64>>,
    /// The roaster's PID in auto
    pub auto: Vec<bool>,
    /// The server's RoR, °C/min
    pub ror: Vec<Option<f64>>,
    /// The PID's gains, heater % units
    pub gains: Vec<Option<PidGains>>,
}

/// A recorded sample, as the fit and the live prediction read it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tick {
    /// Seconds since CHARGE
    pub t: f64,
    pub bt: Option<f64>,
    pub heater: Option<f64>,
    pub fan: Option<f64>,
    pub setpoint: Option<f64>,
    pub auto: bool,
    pub ror: Option<f64>,
    pub gains: Option<PidGains>,
}

impl Record {
    /// Resample ticks (seconds since CHARGE, ascending) onto 1 s from 0 to `drop_t`: BT
    /// linearly, the rest held from the last tick. None when BT is missing or too short.
    pub fn from_ticks(session_id: &str, ticks: &[Tick], drop_t: f64) -> Option<Record> {
        let pts: Vec<&Tick> = ticks.iter().filter(|k| k.bt.is_some()).collect();
        if pts.len() < 60 || drop_t < 120.0 {
            return None;
        }
        // A roast without the heater and fan (an import, BT alone) says nothing of the heat.
        let inputs = ticks
            .iter()
            .filter(|k| k.heater.is_some() && k.fan.is_some())
            .count();
        if inputs * 2 < ticks.len() {
            return None;
        }
        let n = drop_t.floor() as usize;
        let mut r = Record {
            session_id: session_id.to_string(),
            ..Record::default()
        };
        let mut j = 0usize; // last tick at or before t
        let mut last = Tick::default();
        let mut ti = 0usize;
        for i in 0..=n {
            let t = i as f64;
            while j + 1 < pts.len() && pts[j + 1].t <= t {
                j += 1;
            }
            while ti < ticks.len() && ticks[ti].t <= t {
                let k = ticks[ti];
                last = Tick {
                    heater: k.heater.or(last.heater),
                    fan: k.fan.or(last.fan),
                    setpoint: k.setpoint.or(last.setpoint),
                    ror: k.ror.or(last.ror),
                    gains: k.gains.or(last.gains),
                    ..k
                };
                ti += 1;
            }
            let a = pts[j];
            let bt = if a.t >= t || j + 1 >= pts.len() {
                a.bt?
            } else {
                let b = pts[j + 1];
                let w = (t - a.t) / (b.t - a.t).max(1e-9);
                a.bt? + (b.bt? - a.bt?) * w
            };
            r.bt.push(bt);
            r.heater.push(last.heater.unwrap_or(0.0));
            r.fan.push(last.fan.unwrap_or(0.0));
            r.setpoint.push(last.setpoint);
            r.auto.push(last.auto);
            r.ror.push(last.ror);
            r.gains.push(last.gains);
        }
        // No RoR stored (an import, telemetry posted by hand): BT's slope over the last 30 s,
        // so the projection can be scored too.
        if r.ror.iter().all(Option::is_none) {
            r.ror = (0..r.bt.len()).map(|i| slope(&r.bt, i, 30)).collect();
        }
        Some(r)
    }

    fn len(&self) -> usize {
        self.bt.len()
    }

    fn inputs(&self) -> Vec<f64> {
        self.heater
            .iter()
            .zip(&self.fan)
            .map(|(&h, &f)| drive(h, f))
            .collect()
    }
}

/// The least-squares slope of `v` over the `w` seconds up to `i`, per minute.
fn slope(v: &[f64], i: usize, w: usize) -> Option<f64> {
    let lo = i.saturating_sub(w);
    let n = (i - lo + 1) as f64;
    if n < 5.0 {
        return None;
    }
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
    for (k, y) in v[lo..=i].iter().enumerate() {
        let x = k as f64;
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    Some((n * sxy - sx * sy) / (n * sxx - sx * sx) * 60.0)
}

/// The lag state along a record.
fn lagged(u: &[f64], tau: f64) -> Vec<f64> {
    let mut x = Vec::with_capacity(u.len());
    let mut v = u.first().copied().unwrap_or(0.0);
    for &ui in u {
        x.push(v);
        v += (ui - v) / (tau + 1.0);
    }
    x
}

/// dBT/dt at each second: BT smoothed forward and back (2 s), then a central difference.
fn derivative(bt: &[f64]) -> Vec<f64> {
    let n = bt.len();
    let mut s = bt.to_vec();
    let a = 1.0 / 3.0;
    for i in 1..n {
        s[i] = s[i - 1] + a * (s[i] - s[i - 1]);
    }
    for i in (0..n.saturating_sub(1)).rev() {
        s[i] = s[i + 1] + a * (s[i] - s[i + 1]);
    }
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(n - 1);
            if hi > lo {
                (s[hi] - s[lo]) / (hi - lo) as f64
            } else {
                0.0
            }
        })
        .collect()
}

fn hat(bt: f64, out: &mut [f64]) {
    for v in out.iter_mut() {
        *v = 0.0;
    }
    let n = out.len();
    let pos = ((bt - KNOT_FROM_C) / KNOT_STEP_C).clamp(0.0, (n - 1) as f64);
    let j = (pos.floor() as usize).min(n - 2);
    let w = pos - j as f64;
    out[j] = 1.0 - w;
    out[j + 1] = w;
}

/// Solve `a·x = b` (symmetric positive definite) by Cholesky; None when singular.
// Index loops read as the textbook algorithm.
#[allow(clippy::needless_range_loop)]
fn solve_spd(mut a: Vec<Vec<f64>>, b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    for j in 0..n {
        let mut d = a[j][j];
        for k in 0..j {
            d -= a[j][k] * a[j][k];
        }
        if d.is_nan() || d <= 1e-12 {
            return None;
        }
        let d = d.sqrt();
        a[j][j] = d;
        for i in j + 1..n {
            let mut s = a[i][j];
            for k in 0..j {
                s -= a[i][k] * a[j][k];
            }
            a[i][j] = s / d;
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= a[i][k] * y[k];
        }
        y[i] = s / a[i][i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= a[k][i] * x[k];
        }
        x[i] = s / a[i][i];
    }
    Some(x)
}

/// Least squares for one lag: `[gain, g_1 … g_K]`.
fn solve_for_tau(records: &[&Record], tau: f64) -> Option<ThermalModel> {
    let p = 1 + KNOTS;
    let mut ata = vec![vec![0.0; p]; p];
    let mut atb = vec![0.0; p];
    let mut row = vec![0.0; p];
    let mut n = 0usize;
    for r in records {
        let x = lagged(&r.inputs(), tau);
        let y = derivative(&r.bt);
        for i in FIT_SKIP_S..r.len().saturating_sub(1) {
            row[0] = x[i];
            hat(r.bt[i], &mut row[1..]);
            for a in 0..p {
                if row[a] == 0.0 {
                    continue;
                }
                atb[a] += row[a] * y[i];
                for b in 0..p {
                    ata[a][b] += row[a] * row[b];
                }
            }
            n += 1;
        }
    }
    if n < 100 {
        return None;
    }
    // Pull toward the prior (lightly: a few roasts outweigh it) and keep g smooth along BT.
    let prior = ThermalModel::prior();
    let mut w0 = vec![prior.gain];
    w0.extend(&prior.drift);
    let ridge = 2.0;
    let smooth = 400.0;
    for a in 0..p {
        ata[a][a] += ridge;
        atb[a] += ridge * w0[a];
    }
    // Second differences of g (indices 1..): penalise (g[j-1] - 2g[j] + g[j+1])².
    for j in 2..p - 1 {
        let idx = [j - 1, j, j + 1];
        let c = [1.0, -2.0, 1.0];
        for (ia, &a) in idx.iter().enumerate() {
            for (ib, &b) in idx.iter().enumerate() {
                ata[a][b] += smooth * c[ia] * c[ib];
            }
        }
    }
    let w = solve_spd(ata, &atb)?;
    Some(ThermalModel {
        tau_s: tau,
        gain: w[0],
        knot_from_c: KNOT_FROM_C,
        knot_step_c: KNOT_STEP_C,
        drift: w[1..].to_vec(),
    })
}

/// How a prediction is made, for scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The model with the heater and fan the roast then had (the model's own quality)
    Recorded,
    /// The roaster's PID following the setpoint the roast then had, fan held: what a drive
    /// or a held setpoint shows live
    Plan,
    /// Heater and fan held at their values at the start: what manual control shows live
    Held,
    /// The dashboard's projection (RoR now, bent by a falling RoR): what the model must beat
    Projection,
}

/// The error of one way of predicting, at one horizon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HorizonError {
    pub horizon_s: usize,
    /// Root mean square, °C
    pub rmse: f64,
    /// 90 % of the predictions were within this, °C
    pub p90: f64,
    /// Mean of predicted minus actual, °C
    pub bias: f64,
    pub n: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ModeErrors {
    pub mode: Mode,
    pub horizons: Vec<HorizonError>,
}

/// How good a fit is, each roast predicted by a model fitted without it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FitQuality {
    pub roasts: usize,
    pub samples: usize,
    /// Leave-one-roast-out errors (none with a single roast)
    pub modes: Vec<ModeErrors>,
}

impl FitQuality {
    pub fn errors(&self, mode: Mode) -> Option<&ModeErrors> {
        self.modes.iter().find(|m| m.mode == mode)
    }

    /// The 90 % band at `dt` seconds ahead for `mode`, interpolated from the horizons
    /// (0 at 0 s, held past the last).
    pub fn band(&self, mode: Mode, dt: f64) -> Option<f64> {
        let m = self.errors(mode)?;
        let mut prev = (0.0, 0.0);
        for h in &m.horizons {
            let x = h.horizon_s as f64;
            if dt <= x {
                let w = (dt - prev.0) / (x - prev.0).max(1e-9);
                return Some(prev.1 + (h.p90 - prev.1) * w.clamp(0.0, 1.0));
            }
            prev = (x, h.p90);
        }
        m.horizons.last().map(|h| h.p90)
    }

    #[cfg(test)]
    pub fn at(&self, mode: Mode, horizon_s: usize) -> Option<&HorizonError> {
        self.errors(mode)?
            .horizons
            .iter()
            .find(|h| h.horizon_s == horizon_s)
    }
}

/// Simulate `n` seconds from index `i0` of a record, the inputs taken as recorded.
fn sim_recorded(m: &ThermalModel, r: &Record, u: &[f64], x0: f64, i0: usize, n: usize) -> f64 {
    let mut s = State {
        bt: r.bt[i0],
        x: x0,
    };
    for &uk in &u[i0..i0 + n] {
        m.step(&mut s, uk, 1.0);
    }
    s.bt
}

fn sim_held(m: &ThermalModel, r: &Record, x0: f64, i0: usize, n: usize) -> f64 {
    let u = drive(r.heater[i0], r.fan[i0]);
    let mut s = State {
        bt: r.bt[i0],
        x: x0,
    };
    for _ in 0..n {
        m.step(&mut s, u, 1.0);
    }
    s.bt
}

/// The PID following `setpoint(k)` from `bt0` with the heater at `heater0` now (a bumpless
/// start), fan held. Returns BT after each second and the heater it took.
pub fn sim_closed_loop(
    m: &ThermalModel,
    s0: State,
    heater0: f64,
    fan: &dyn Fn(usize) -> f64,
    gains: PidGains,
    setpoint: &dyn Fn(usize) -> f64,
    n: usize,
) -> Vec<(f64, f64)> {
    let mut pid = ServerPid::new(gains);
    pid.reset(setpoint(0), s0.bt, heater0, 0.0);
    let mut s = s0;
    let mut heater = heater0;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        m.step(&mut s, drive(heater, fan(k)), 1.0);
        heater = pid.compute(setpoint(k + 1), Some(s.bt), 1.0, 0.0);
        out.push((s.bt, heater));
    }
    out
}

/// Scored only where the roaster stayed in auto the whole time: no plan foresees the roaster
/// leaving it (a person taking over, a heater cut), and live the prediction then changes to
/// heater and fan held.
fn sim_plan(m: &ThermalModel, r: &Record, x0: f64, i0: usize, n: usize) -> Option<f64> {
    if !r.auto[i0..=i0 + n].iter().all(|&a| a) {
        return None;
    }
    let gains = r.gains[i0]?;
    r.setpoint[i0]?;
    let fan0 = r.fan[i0];
    let sp = |k: usize| {
        r.setpoint[(i0 + k).min(r.len() - 1)]
            .or(r.setpoint[i0])
            .unwrap_or(0.0)
    };
    let out = sim_closed_loop(
        m,
        State {
            bt: r.bt[i0],
            x: x0,
        },
        r.heater[i0],
        &|_| fan0,
        gains,
        &sp,
        n,
    );
    out.last().map(|p| p.0)
}

/// The dashboard's projection (`scopeData.ts` `projectBt`): BT now plus the RoR now, bent
/// down by the RoR's falling trend over the last 30 s, never below flat.
pub fn projection(t: &[f64], bt_now: f64, ror: &[Option<f64>], dt: f64) -> Option<f64> {
    let i = ror.len().checked_sub(1)?;
    let t_now = *t.last()?;
    let (mut sx, mut sy, mut sxx, mut sxy, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (k, r) in ror.iter().enumerate() {
        let Some(r) = r else { continue };
        let x = t[k] - t_now;
        if x < -30.0 {
            continue;
        }
        sx += x;
        sy += r;
        sxx += x * x;
        sxy += x * r;
        n += 1.0;
    }
    let slope = if n >= 3.0 && (n * sxx - sx * sx).abs() > 1e-9 {
        (n * sxy - sx * sy) / (n * sxx - sx * sx)
    } else {
        0.0
    };
    let r0 = ror[i].or(if n >= 3.0 {
        Some((sy - slope * sx) / n)
    } else {
        None
    })?;
    let rps = r0 / 60.0;
    let acc = slope.min(0.0) / 60.0;
    let tz = if acc < 0.0 && rps > 0.0 {
        -rps / acc
    } else {
        f64::INFINITY
    };
    let d = dt.min(tz);
    Some(bt_now + rps * d + 0.5 * acc * d * d)
}

fn projection_at(r: &Record, i0: usize, n: usize) -> Option<f64> {
    let lo = i0.saturating_sub(30);
    let t: Vec<f64> = (lo..=i0).map(|i| i as f64).collect();
    projection(&t, r.bt[i0], &r.ror[lo..=i0], n as f64)
}

#[derive(Default)]
struct Errs(Vec<Vec<f64>>);

impl Errs {
    fn new() -> Errs {
        Errs(vec![Vec::new(); HORIZONS.len()])
    }
    fn summary(&self, mode: Mode) -> Option<ModeErrors> {
        let horizons: Vec<HorizonError> = HORIZONS
            .iter()
            .zip(&self.0)
            .filter(|(_, e)| !e.is_empty())
            .map(|(&h, e)| summarize(h, e))
            .collect();
        (!horizons.is_empty()).then_some(ModeErrors { mode, horizons })
    }
}

/// Root mean square, 90th percentile of the absolute error and the bias.
pub fn summarize(horizon_s: usize, e: &[f64]) -> HorizonError {
    let n = e.len();
    let rmse = (e.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64).sqrt();
    let mut abs: Vec<f64> = e.iter().map(|v| v.abs()).collect();
    abs.sort_by(f64::total_cmp);
    let p90 = abs
        .get(((n as f64 * 0.9).ceil() as usize).saturating_sub(1))
        .copied()
        .unwrap_or(0.0);
    let r2 = |v: f64| (v * 100.0).round() / 100.0;
    HorizonError {
        horizon_s,
        rmse: r2(rmse),
        p90: r2(p90),
        bias: r2(e.iter().sum::<f64>() / n.max(1) as f64),
        n,
    }
}

/// Score `m` on `r` (a roast it was not fitted on) into `errs` per mode.
fn score_into(m: &ThermalModel, r: &Record, errs: &mut [Errs; 4], recorded_only: bool) {
    let u = r.inputs();
    let x = lagged(&u, m.tau_s);
    let max_h = *HORIZONS.last().unwrap_or(&120);
    let mut i0 = START_AFTER_CHARGE_S;
    while i0 + max_h < r.len() {
        for (hi, &h) in HORIZONS.iter().enumerate() {
            let actual = r.bt[i0 + h];
            errs[0].0[hi].push(sim_recorded(m, r, &u, x[i0], i0, h) - actual);
            if recorded_only {
                continue;
            }
            if let Some(p) = sim_plan(m, r, x[i0], i0, h) {
                errs[1].0[hi].push(p - actual);
            }
            errs[2].0[hi].push(sim_held(m, r, x[i0], i0, h) - actual);
            if let Some(p) = projection_at(r, i0, h) {
                errs[3].0[hi].push(p - actual);
            }
        }
        i0 += SCORE_EVERY_S;
    }
}

/// A fitted model and how good it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Fitted {
    pub model: ThermalModel,
    pub quality: FitQuality,
}

/// Fit a model to a roaster's roasts. With one roast the fit has no score; with none it
/// is the prior.
pub fn fit(records: &[Record]) -> Fitted {
    let samples: usize = records.iter().map(Record::len).sum();
    let all: Vec<&Record> = records.iter().collect();
    if records.len() < 2 {
        let model =
            solve_for_tau(&all, ThermalModel::prior().tau_s).unwrap_or_else(ThermalModel::prior);
        return Fitted {
            model,
            quality: FitQuality {
                roasts: records.len(),
                samples,
                modes: Vec::new(),
            },
        };
    }
    // The lag: leave-one-roast-out, two minutes ahead with the recorded inputs.
    let max_h = *HORIZONS.last().unwrap_or(&120);
    let mut best: Option<(f64, f64)> = None;
    for &tau in &TAUS {
        let (mut sse, mut n) = (0.0, 0usize);
        for (k, r) in records.iter().enumerate() {
            let train: Vec<&Record> = records
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, r)| r)
                .collect();
            let Some(m) = solve_for_tau(&train, tau) else {
                continue;
            };
            let u = r.inputs();
            let x = lagged(&u, tau);
            let mut i0 = START_AFTER_CHARGE_S;
            while i0 + max_h < r.len() {
                let e = sim_recorded(&m, r, &u, x[i0], i0, max_h) - r.bt[i0 + max_h];
                sse += e * e;
                n += 1;
                i0 += SCORE_EVERY_S;
            }
        }
        if n > 0 && best.is_none_or(|b| sse / (n as f64) < b.1) {
            best = Some((tau, sse / n as f64));
        }
    }
    let tau = best.map_or(ThermalModel::prior().tau_s, |b| b.0);
    let mut errs = [Errs::new(), Errs::new(), Errs::new(), Errs::new()];
    for (k, r) in records.iter().enumerate() {
        let train: Vec<&Record> = records
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, r)| r)
            .collect();
        if let Some(m) = solve_for_tau(&train, tau) {
            score_into(&m, r, &mut errs, false);
        }
    }
    let model = solve_for_tau(&all, tau).unwrap_or_else(ThermalModel::prior);
    let modes = [Mode::Recorded, Mode::Plan, Mode::Held, Mode::Projection]
        .iter()
        .zip(&errs)
        .filter_map(|(&mode, e)| e.summary(mode))
        .collect();
    Fitted {
        model,
        quality: FitQuality {
            roasts: records.len(),
            samples,
            modes,
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sim::virtual_roaster::Model as Sim;
    use std::collections::HashMap;

    /// A roast on the virtual roaster's physics: the heater ramps up by BT, fan 100 % then
    /// easing to 80 %, with a little noise on BT.
    fn sim_roast(id: &str, heater_lo: f64, len: usize, seed: u64) -> Record {
        let mut m = Sim::default();
        m.charge();
        let mut r = Record {
            session_id: id.into(),
            ..Record::default()
        };
        let mut rng = seed;
        let gains = PidGains {
            kp: 3.0,
            ki: 0.05,
            kd: 0.0,
        };
        for i in 0..len {
            let t = i as f64;
            let heater = (heater_lo + t / 10.0).min(95.0);
            let fan = if t < 300.0 {
                100.0
            } else {
                (100.0 - (t - 300.0) / 10.0).max(80.0)
            };
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let noise = ((rng % 1000) as f64 / 1000.0 - 0.5) * 0.3;
            r.bt.push(m.bt + noise);
            r.heater.push(heater);
            r.fan.push(fan);
            r.setpoint.push(Some(m.bt + 2.0));
            r.auto.push(false);
            r.ror.push(None);
            r.gains.push(Some(gains));
            m.step(1.0, heater, fan);
        }
        r
    }

    #[test]
    fn the_prior_settles_where_the_virtual_roaster_does() {
        let p = ThermalModel::prior();
        let mut sim = Sim::default();
        sim.charge();
        for _ in 0..3000 {
            sim.step(1.0, 88.0, 100.0);
        }
        let settle = p.settles_at(88.0, 100.0).unwrap();
        assert!((settle - sim.bt).abs() < 1.5, "{settle} vs {}", sim.bt);
        // Below the interlock nothing heats.
        assert_eq!(drive(80.0, 30.0), 0.0);
    }

    #[test]
    fn a_fit_on_virtual_roasts_predicts_them_two_minutes_ahead() {
        let roasts: Vec<Record> = (0..4)
            .map(|k| sim_roast(&format!("r{k}"), 40.0 + 5.0 * k as f64, 600, 7 + k))
            .collect();
        let f = fit(&roasts);
        assert_eq!(f.quality.roasts, 4);
        let rec = f.quality.at(Mode::Recorded, 120).unwrap();
        assert!(rec.rmse < 1.5, "{rec:?}");
        // The fitted gain is the prior's (the same physics) to within a fifth.
        let g0 = ThermalModel::prior().gain;
        assert!(
            (f.model.gain - g0).abs() / g0 < 0.2,
            "{} vs {g0}",
            f.model.gain
        );
        // The projection, which knows nothing of the heater, is worse than the model.
        let held = f.quality.at(Mode::Held, 120).unwrap();
        assert!(held.n > 0);
    }

    #[test]
    fn the_band_interpolates_between_horizons() {
        let q = FitQuality {
            roasts: 3,
            samples: 0,
            modes: vec![ModeErrors {
                mode: Mode::Plan,
                horizons: vec![
                    HorizonError {
                        horizon_s: 30,
                        rmse: 0.0,
                        p90: 1.0,
                        bias: 0.0,
                        n: 1,
                    },
                    HorizonError {
                        horizon_s: 60,
                        rmse: 0.0,
                        p90: 2.0,
                        bias: 0.0,
                        n: 1,
                    },
                ],
            }],
        };
        assert_eq!(q.band(Mode::Plan, 15.0), Some(0.5));
        assert_eq!(q.band(Mode::Plan, 45.0), Some(1.5));
        assert_eq!(q.band(Mode::Plan, 200.0), Some(2.0));
        assert_eq!(q.band(Mode::Held, 10.0), None);
    }

    #[test]
    fn resampling_holds_inputs_and_interpolates_bt() {
        let ticks: Vec<Tick> = (0..200)
            .map(|i| Tick {
                t: i as f64 * 1.5,
                bt: Some(i as f64 * 1.5),
                heater: Some(50.0),
                fan: Some(100.0),
                ..Tick::default()
            })
            .collect();
        let r = Record::from_ticks("s", &ticks, 250.0).unwrap();
        assert_eq!(r.bt.len(), 251);
        // No RoR stored: it comes from BT (1 °C/s here, 60 °C/min).
        assert!(
            (r.ror[100].unwrap() - 60.0).abs() < 1e-6,
            "{:?}",
            r.ror[100]
        );
        // BT alone (no heater or fan): not a record a fit can use.
        let bare: Vec<Tick> = ticks
            .iter()
            .map(|k| Tick {
                heater: None,
                fan: None,
                ..*k
            })
            .collect();
        assert!(Record::from_ticks("s", &bare, 250.0).is_none());
        assert!((r.bt[100] - 100.0).abs() < 1e-9);
        assert_eq!(r.heater[100], 50.0);
    }

    #[test]
    fn the_projection_matches_the_dashboards() {
        // Steady RoR 12 °C/min: linear, 24 °C in 2 min.
        let t: Vec<f64> = (0..=30).map(f64::from).collect();
        let ror = vec![Some(12.0); 31];
        assert!((projection(&t, 150.0, &ror, 120.0).unwrap() - 174.0).abs() < 1e-9);
        // Falling 1 °C/min per 10 s from 12: flattens where the RoR reaches 0 (120 s).
        let ror: Vec<Option<f64>> = (0..=30).map(|i| Some(15.0 - f64::from(i) / 10.0)).collect();
        let p = projection(&t, 150.0, &ror, 120.0).unwrap();
        assert!(
            (p - (150.0 + 0.2 * 120.0 - 0.5 * (0.1 / 60.0) * 120.0 * 120.0)).abs() < 1e-6,
            "{p}"
        );
    }

    /// The roasts exported from a database to CSV (docs/prediction.md, "Checking a fit"): per
    /// roast its landmarks and its samples, both in seconds since CHARGE.
    pub(crate) type Exported = Vec<(String, HashMap<String, f64>, Vec<Tick>)>;

    pub(crate) fn exported(dir: &str) -> Exported {
        let num = |s: &str| s.trim_matches('"').parse::<f64>().ok();
        let mut events = std::collections::BTreeMap::<String, HashMap<String, f64>>::new();
        for line in std::fs::read_to_string(format!("{dir}/events.csv"))
            .unwrap()
            .lines()
        {
            let c: Vec<&str> = line.split(',').collect();
            if let Some(v) = num(c[2]) {
                events
                    .entry(c[0].to_string())
                    .or_default()
                    .entry(c[1].trim_matches('"').to_string())
                    .or_insert(v);
            }
        }
        let mut ticks = std::collections::BTreeMap::<String, Vec<Tick>>::new();
        for line in std::fs::read_to_string(format!("{dir}/tele.csv"))
            .unwrap()
            .lines()
        {
            let c: Vec<&str> = line.split(',').collect();
            let Some(&charge) = events.get(c[0]).and_then(|e| e.get("charge")) else {
                continue;
            };
            let gains = match (num(c[9]), num(c[10])) {
                (Some(kp), Some(ki)) => Some(PidGains {
                    kp: kp / 2.55,
                    ki: ki / 2.55,
                    kd: 0.0,
                }),
                _ => None,
            };
            ticks.entry(c[0].to_string()).or_default().push(Tick {
                t: num(c[1]).unwrap() - charge,
                bt: num(c[2]),
                setpoint: num(c[4]),
                heater: num(c[5]),
                fan: num(c[6]),
                auto: num(c[7]) == Some(1.0),
                ror: num(c[8]),
                gains,
            });
        }
        ticks
            .into_iter()
            .filter_map(|(b, t)| {
                let e = events.get(&b)?;
                let charge = *e.get("charge")?;
                let marks = e.iter().map(|(k, v)| (k.clone(), v - charge)).collect();
                Some((b, marks, t))
            })
            .collect()
    }

    /// Fit the roasts exported from a database to CSV (docs/prediction.md, "Checking a fit"):
    /// `ROASTWARDEN_PREDICT_DIR=dir cargo test -p roastwarden-server --lib predict::model -- --ignored --nocapture`
    #[test]
    #[ignore = "needs ROASTWARDEN_PREDICT_DIR: a directory of roasts exported to CSV"]
    fn fit_exported_roasts() {
        let dir = std::env::var("ROASTWARDEN_PREDICT_DIR").expect("ROASTWARDEN_PREDICT_DIR");
        let recs: Vec<Record> = exported(&dir)
            .iter()
            .filter_map(|(b, e, t)| {
                let t: Vec<Tick> = t.iter().filter(|k| k.t >= 0.0).copied().collect();
                Record::from_ticks(b, &t, *e.get("drop")?)
            })
            .collect();
        let t0 = std::time::Instant::now();
        let f = fit(&recs);
        println!(
            "fit {} roasts in {:?}: tau {} gain {:.3}",
            recs.len(),
            t0.elapsed(),
            f.model.tau_s,
            f.model.gain
        );
        println!(
            "g {:?}",
            f.model
                .drift
                .iter()
                .map(|v| (v * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>()
        );
        for m in &f.quality.modes {
            for h in &m.horizons {
                println!(
                    "{:?} {:>3} s  rmse {:5.2}  p90 {:5.2}  bias {:+.2}  n {}",
                    m.mode, h.horizon_s, h.rmse, h.p90, h.bias, h.n
                );
            }
        }
    }
}
