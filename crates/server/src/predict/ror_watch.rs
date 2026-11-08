//! The RoR watch (docs/prediction.md "RoR warnings"): a warning before the
//! RoR crashes, flicks up after first crack, or BT stalls. It reads two things: the RoR now
//! and its trend (BT's slope over the last 30 s, the watch's own filter, so the RoR smoothing
//! setting does not move it), and the RoR the roaster's own model expects under the plan for
//! the next two minutes. Each warning says which it is, when, and why.
//!
//! The thresholds are Roastwarden's own, the same as the RoR notes a roast gets afterwards
//! (`apps/dashboard/src/lib/guidance/ror.ts`, docs/roast-guidance.md): no source gives numbers.
//! The watch starts late in the roast (DRY end marked, or BT at 160 °C), where the early
//! swings of the RoR are over.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use utoipa::ToSchema;

use super::live::mmss;
use super::model::{ThermalModel, Tick};

/// A fall of the RoR this large, °C/min, and at least this share of it, …
pub const CRASH_MIN: f64 = 8.0;
pub const CRASH_SHARE: f64 = 0.5;
/// … within this long, s.
pub const CRASH_WINDOW_S: usize = 60;
/// A rise after first crack this far above its lowest since, °C/min, …
pub const RISE_MIN: f64 = 2.0;
/// … held this long, s.
pub const RISE_HOLD_S: usize = 20;
/// RoR at or below this, °C/min, …
pub const STALL_MAX: f64 = 1.0;
/// … for this long, s.
pub const STALL_HOLD_S: usize = 30;
/// The RoR the watch reads: BT's slope over this many seconds back.
pub const SLOPE_S: usize = 30;
/// How far ahead the model is read, s (the band's horizon).
pub const AHEAD_S: usize = 120;
/// Late in the roast without a DRY mark: from this BT, °C.
pub const LATE_BT: f64 = 160.0;
/// Below this RoR BT is falling fast: the beans leaving at DROP, or a cut. Not a crash to
/// warn of: it is already plain to see.
const DUMP_ROR: f64 = -5.0;
/// A stall is one only where the plan wants BT to rise this fast, °C/min …
const PLAN_RISING: f64 = 2.0;
/// … or BT is this far below the plan, °C.
const PLAN_BEHIND: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RorWarningKind {
    /// The RoR falls 8 °C/min or more, half of it at least, within a minute
    Crash,
    /// After first crack the RoR rises 2 °C/min above its lowest since, for 20 s
    Flick,
    /// The RoR at or below 1 °C/min for 30 s before DROP
    Stall,
}

impl RorWarningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RorWarningKind::Crash => "crash",
            RorWarningKind::Flick => "flick",
            RorWarningKind::Stall => "stall",
        }
    }

    pub fn parse(s: &str) -> Option<RorWarningKind> {
        match s {
            "crash" => Some(RorWarningKind::Crash),
            "flick" => Some(RorWarningKind::Flick),
            "stall" => Some(RorWarningKind::Stall),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RorWarning {
    pub kind: RorWarningKind,
    /// When it is expected to start (or started, when seen), s since CHARGE
    pub t: f64,
    /// Seen in the RoR now; false: expected by the model
    pub seen: bool,
    /// Since when the predictions have said so, s since CHARGE
    pub since: f64,
    /// What the RoR does and why, in one line
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RorWatch {
    /// Now, s since CHARGE
    pub t: f64,
    /// RoR now, °C/min: BT's slope over the last 30 s
    pub now: Option<f64>,
    /// How fast the RoR changes, °C/min per minute (its slope over the last 60 s)
    pub trend: Option<f64>,
    /// The model's RoR in 1 and 2 min under the plan, °C/min
    pub at_60: Option<f64>,
    pub at_120: Option<f64>,
    /// Late in the roast (DRY end marked, or BT at 160 °C): the watch warns only then
    pub armed: bool,
    pub warnings: Vec<RorWarning>,
}

impl RorWatch {
    pub fn warning(&self, kind: RorWarningKind) -> Option<&RorWarning> {
        self.warnings.iter().find(|w| w.kind == kind)
    }
}

/// What the watch reads.
pub struct WatchInput<'a> {
    pub model: &'a ThermalModel,
    /// Samples since CHARGE, ascending
    pub ticks: &'a [Tick],
    pub t_now: f64,
    pub bt_now: f64,
    /// BT each second under the plan, index 0 one second ahead, and the heater it takes
    pub bt: &'a [f64],
    pub heater: &'a [f64],
    /// The plan's fan, index 0 now; past the end the last holds
    pub fan: &'a [f64],
    /// The plan's BT target, index 0 now; past the end the last holds. None: heater and fan
    /// held, no target
    pub targets: Option<&'a [f64]>,
    pub dry_marked: bool,
    /// First crack marked, s since CHARGE
    pub fc_marked: Option<f64>,
    /// The BT first crack comes at on this roaster, °C
    pub fc_bt: Option<f64>,
    /// DROP: when it is predicted, and its BT when the profile drops at a BT
    pub drop_t: Option<f64>,
    pub drop_bt: Option<f64>,
}

fn at(v: &[f64], k: usize) -> f64 {
    v.get(k).or(v.last()).copied().unwrap_or(0.0)
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Least-squares slope of `(t, v)`, per second; None under 5 points or 15 s.
fn slope(pts: &[(f64, f64)]) -> Option<f64> {
    if pts.len() < 5 || pts.last()?.0 - pts[0].0 < 15.0 {
        return None;
    }
    let n = pts.len() as f64;
    let mt = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let mv = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let d: f64 = pts.iter().map(|p| (p.0 - mt).powi(2)).sum();
    (d > 1e-9).then(|| pts.iter().map(|p| (p.0 - mt) * (p.1 - mv)).sum::<f64>() / d)
}

/// One observed point: time, RoR (°C/min) and BT then.
#[derive(Debug, Clone, Copy)]
struct Obs {
    t: f64,
    ror: f64,
    bt: f64,
}

/// The observed RoR at each sample from `from` (s since CHARGE): BT's slope over the 30 s
/// before it.
fn observed(ticks: &[Tick], from: f64) -> Vec<Obs> {
    let pts: Vec<(f64, f64)> = ticks.iter().filter_map(|k| Some((k.t, k.bt?))).collect();
    let mut out = Vec::new();
    let mut lo = 0;
    for i in 0..pts.len() {
        let (t, bt) = pts[i];
        while pts[lo].0 < t - SLOPE_S as f64 {
            lo += 1;
        }
        if t < from {
            continue;
        }
        if let Some(s) = slope(&pts[lo..=i]) {
            out.push(Obs {
                t,
                ror: s * 60.0,
                bt,
            });
        }
    }
    out
}

/// A fall from `from` to `to` (°C/min) is a crash: 8 °C/min and half the RoR. Just after DRY
/// end the RoR still falls fast on its own (24 to 14 °C/min in a minute on the fluid bed); a
/// crash takes most of it.
fn crashes(from: f64, to: f64) -> bool {
    from - to >= CRASH_MIN && from - to >= CRASH_SHARE * from
}

/// Whether first crack has come at `t` with BT `bt`: marked, else BT at first crack's BT.
fn after_fc(fc_marked: Option<f64>, fc_bt: Option<f64>, t: f64, bt: f64) -> bool {
    match (fc_marked, fc_bt) {
        (Some(f), _) => t >= f,
        (None, Some(c)) => bt >= c,
        _ => false,
    }
}

/// Before DROP: a few seconds before its time, and below its BT.
fn before_drop(inp: &WatchInput, t: f64, bt: f64) -> bool {
    inp.drop_t.is_none_or(|d| t < d - 5.0) && inp.drop_bt.is_none_or(|c| bt < c - 0.5)
}

/// The warnings seen in the RoR now, from the samples alone (also how a stored roast is read
/// afterwards, [`events`]).
fn seen(inp: &WatchInput, obs: &[Obs], now: Option<f64>) -> Vec<RorWarning> {
    let mut out = Vec::new();
    let (Some(now), Some(last)) = (now, obs.last()) else {
        return out;
    };
    if !before_drop(inp, inp.t_now, inp.bt_now) || now < DUMP_ROR {
        return out;
    }
    let recent = |s: usize| obs.iter().filter(move |o| o.t >= inp.t_now - s as f64);
    let warn = |kind, t: f64, reason: String| RorWarning {
        kind,
        t: r1(t),
        seen: true,
        since: inp.t_now,
        reason,
    };

    // Crash: the highest RoR in the last minute, 8 °C/min or more above now.
    if let Some(peak) = recent(CRASH_WINDOW_S).max_by(|a, b| a.ror.total_cmp(&b.ror)) {
        if crashes(peak.ror, now) {
            out.push(warn(
                RorWarningKind::Crash,
                peak.t,
                format!(
                    "RoR fell from {:.1} to {now:.1} °C/min in {:.0} s",
                    peak.ror,
                    inp.t_now - peak.t
                ),
            ));
        }
    }

    // Flick: after first crack, RoR held 2 °C/min above its lowest since, for 20 s.
    let since_fc: Vec<&Obs> = obs
        .iter()
        .filter(|o| after_fc(inp.fc_marked, inp.fc_bt, o.t, o.bt))
        .collect();
    let hold_from = inp.t_now - RISE_HOLD_S as f64;
    if since_fc
        .first()
        .is_some_and(|o| o.t <= hold_from - 10.0 && last.t >= inp.t_now - 3.0)
    {
        let low = since_fc
            .iter()
            .filter(|o| o.t < hold_from)
            .map(|o| o.ror)
            .fold(f64::INFINITY, f64::min);
        let run: Vec<&&Obs> = since_fc.iter().filter(|o| o.t >= hold_from).collect();
        if low.is_finite() && run.len() >= 5 && run.iter().all(|o| o.ror - low >= RISE_MIN) {
            out.push(warn(
                RorWarningKind::Flick,
                hold_from,
                format!(
                    "RoR rose from {low:.1} to {now:.1} °C/min after first crack, for {RISE_HOLD_S} s"
                ),
            ));
        }
    }

    // Stall: RoR at or below 1 °C/min for the last 30 s.
    let run: Vec<&Obs> = recent(STALL_HOLD_S).collect();
    if run.len() >= 5
        && run[0].t <= inp.t_now - STALL_HOLD_S as f64 + 3.0
        && run.iter().all(|o| o.ror <= STALL_MAX && o.ror >= DUMP_ROR)
    {
        out.push(warn(
            RorWarningKind::Stall,
            run[0].t,
            format!(
                "RoR {now:.1} °C/min for {:.0} s at BT {:.1} °C",
                inp.t_now - run[0].t,
                inp.bt_now
            ),
        ));
    }
    out
}

/// The watch now: the RoR and its trend, the model's RoR ahead, and the warnings.
pub fn watch(inp: &WatchInput) -> RorWatch {
    let w = SLOPE_S;
    let n = inp.bt.len();
    // BT on the plan's 1 s grid, index 0 now.
    let b = |j: usize| {
        if j == 0 || n == 0 {
            inp.bt_now
        } else {
            inp.bt[(j - 1).min(n - 1)]
        }
    };
    // The model's RoR at k s ahead: BT's slope over the 30 s before, all of it the model's.
    let kmax = AHEAD_S + STALL_HOLD_S.max(RISE_HOLD_S);
    let model_ror: Vec<Option<f64>> = (0..=kmax)
        .map(|k| {
            (k >= w && n > 0).then(|| {
                let pts: Vec<(f64, f64)> = (k - w..=k).map(|j| (j as f64, b(j))).collect();
                slope(&pts).unwrap_or(0.0) * 60.0
            })
        })
        .collect();
    let pr = |k: usize| model_ror.get(k).copied().flatten().unwrap_or(f64::NAN);

    let from = (inp.t_now - 180.0).min(inp.fc_marked.unwrap_or(f64::INFINITY));
    let obs = observed(inp.ticks, from);
    let now = obs.last().filter(|o| o.t >= inp.t_now - 3.0).map(|o| o.ror);
    let trend = {
        let pts: Vec<(f64, f64)> = obs
            .iter()
            .filter(|o| o.t >= inp.t_now - 60.0)
            .map(|o| (o.t, o.ror))
            .collect();
        slope(&pts).map(|s| s * 60.0)
    };
    let armed = inp.dry_marked || inp.bt_now >= LATE_BT;
    let mut warnings = if armed {
        seen(inp, &obs, now)
    } else {
        Vec::new()
    };
    if armed && n > kmax {
        let have = |ws: &Vec<RorWarning>, k| ws.iter().any(|x: &RorWarning| x.kind == k);
        let mut expected = Vec::new();
        if !have(&warnings, RorWarningKind::Crash) {
            expected.extend(expect_crash(inp, &pr, &b, now, trend));
        }
        if !have(&warnings, RorWarningKind::Flick) {
            expected.extend(expect_flick(inp, &pr, &b, now));
        }
        if !have(&warnings, RorWarningKind::Stall) {
            expected.extend(expect_stall(inp, &pr, &b));
        }
        warnings.extend(expected);
    }
    warnings.sort_by(|a, b| a.t.total_cmp(&b.t));
    RorWatch {
        t: r1(inp.t_now),
        now: now.map(r1),
        trend: trend.map(r1),
        at_60: model_ror.get(60).copied().flatten().map(r1),
        at_120: model_ror.get(120).copied().flatten().map(r1),
        armed,
        warnings,
    }
}

/// Why the model's line goes where it goes between now and `k` s ahead.
fn causes(inp: &WatchInput, b: &dyn Fn(usize) -> f64, k: usize) -> Vec<String> {
    let mut c = Vec::new();
    let h0 = inp.ticks.last().and_then(|t| t.heater).unwrap_or(0.0);
    let hk = at(inp.heater, k.saturating_sub(1));
    let full = inp.heater[..k.min(inp.heater.len())]
        .iter()
        .position(|&h| h >= 99.5);
    match full {
        Some(_) if h0 >= 99.5 => c.push("the heater is at 100 %".to_string()),
        Some(j) => c.push(format!(
            "the heater reaches 100 % at {}",
            mmss(inp.t_now + j as f64 + 1.0)
        )),
        None if hk <= h0 - 15.0 => c.push(format!("the heater falls from {h0:.0} to {hk:.0} %")),
        None if hk >= h0 + 15.0 => c.push(format!("the heater rises from {h0:.0} to {hk:.0} %")),
        None => {}
    }
    if let Some(tg) = inp.targets {
        let rate = |a: usize, b: usize| (at(tg, b) - at(tg, a)) * 60.0 / (b - a) as f64;
        let (r0, rk) = (
            rate(0, SLOPE_S),
            rate(k.saturating_sub(SLOPE_S), k.max(SLOPE_S)),
        );
        if rk - r0 >= 1.0 {
            c.push(format!(
                "the profile's RoR rises from {r0:.1} to {rk:.1} °C/min"
            ));
        } else if r0 - rk >= 3.0 {
            c.push(format!(
                "the profile's RoR falls from {r0:.1} to {rk:.1} °C/min"
            ));
        }
        let behind = at(tg, 0) - inp.bt_now;
        if behind >= 0.8 {
            c.push(format!(
                "BT is {behind:.1} °C behind the profile and the PID catches up"
            ));
        }
    }
    let (f0, fk) = (at(inp.fan, 0), at(inp.fan, k));
    if (fk - f0).abs() >= 5.0 {
        c.push(format!("the fan goes from {f0:.0} to {fk:.0} %"));
    }
    if let Some(fc) = inp.fc_bt.filter(|_| inp.fc_marked.is_none()) {
        if inp.bt_now < fc && b(k) >= fc {
            c.push(format!("BT passes first crack's {fc:.1} °C"));
        }
    }
    c
}

fn because(c: &[String]) -> String {
    if c.is_empty() {
        String::new()
    } else {
        format!(": {}", c.join(", "))
    }
}

fn expect_crash(
    inp: &WatchInput,
    pr: &dyn Fn(usize) -> f64,
    b: &dyn Fn(usize) -> f64,
    now: Option<f64>,
    trend: Option<f64>,
) -> Option<RorWarning> {
    for k in SLOPE_S..=AHEAD_S {
        let t = inp.t_now + k as f64;
        if !before_drop(inp, t, b(k)) {
            break;
        }
        let (jp, peak) = (k.saturating_sub(CRASH_WINDOW_S).max(SLOPE_S)..=k)
            .map(|j| (j, pr(j)))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        if crashes(peak, pr(k)) {
            let mut what = String::from("the model expects");
            if let Some(n) = now {
                what = format!("RoR {n:.1} °C/min now");
                if let Some(tr) = trend {
                    let _ = write!(what, " ({tr:+.1} a minute)");
                }
                what.push_str("; the model expects");
            }
            return Some(RorWarning {
                kind: RorWarningKind::Crash,
                t: r1(inp.t_now + jp as f64),
                seen: false,
                since: inp.t_now,
                reason: format!(
                    "{what} it to fall from {peak:.1} at {} to {:.1} °C/min by {}{}",
                    mmss(inp.t_now + jp as f64),
                    pr(k),
                    mmss(t),
                    because(&causes(inp, b, k))
                ),
            });
        }
    }
    None
}

fn expect_flick(
    inp: &WatchInput,
    pr: &dyn Fn(usize) -> f64,
    b: &dyn Fn(usize) -> f64,
    now: Option<f64>,
) -> Option<RorWarning> {
    let mut low = f64::INFINITY;
    for k in SLOPE_S..=AHEAD_S {
        let t = inp.t_now + k as f64;
        if !before_drop(inp, t, b(k)) {
            break;
        }
        if !after_fc(inp.fc_marked, inp.fc_bt, t, b(k)) {
            continue;
        }
        let r = pr(k);
        if low.is_finite()
            && r - low >= RISE_MIN
            && (k..=k + RISE_HOLD_S).all(|j| pr(j) - low >= RISE_MIN)
        {
            let what = now.map_or(String::new(), |n| format!("RoR {n:.1} °C/min now; "));
            return Some(RorWarning {
                kind: RorWarningKind::Flick,
                t: r1(t),
                seen: false,
                since: inp.t_now,
                reason: format!(
                    "{what}after first crack the model expects it to rise from {low:.1} to \
                     {r:.1} °C/min by {}{}",
                    mmss(t),
                    because(&causes(inp, b, k))
                ),
            });
        }
        low = low.min(r);
    }
    None
}

fn expect_stall(
    inp: &WatchInput,
    pr: &dyn Fn(usize) -> f64,
    b: &dyn Fn(usize) -> f64,
) -> Option<RorWarning> {
    for k in SLOPE_S..=AHEAD_S {
        let t = inp.t_now + k as f64;
        let bk = b(k);
        if !before_drop(inp, t, bk) {
            break;
        }
        if !(k..=k + STALL_HOLD_S).all(|j| pr(j) <= STALL_MAX) {
            continue;
        }
        // Where the plan holds BT (a setpoint reached, the profile's end) BT is meant to
        // stay: a stall is BT staying below what the plan wants.
        let wants_rise = inp.targets.is_none_or(|tg| {
            let rise = (at(tg, k + 15) - at(tg, k.saturating_sub(15))) * 2.0;
            rise >= PLAN_RISING || at(tg, k) - bk >= PLAN_BEHIND
        });
        if !wants_rise {
            continue;
        }
        let mut c = causes(inp, b, k);
        let fan = at(inp.fan, k);
        if let Some(s) = inp.model.settles_at(100.0, fan) {
            c.push(format!(
                "at heater 100 % and fan {fan:.0} % BT settles near {s:.0} °C"
            ));
        }
        let drop = match (inp.drop_bt, inp.drop_t) {
            (Some(d), _) => format!(", short of DROP at {d:.0} °C"),
            (None, Some(d)) => format!(", before DROP at {}", mmss(d)),
            _ => String::new(),
        };
        return Some(RorWarning {
            kind: RorWarningKind::Stall,
            t: r1(t),
            seen: false,
            since: inp.t_now,
            reason: format!(
                "the model expects RoR under {STALL_MAX:.0} °C/min from {} at BT {bk:.1} °C{drop}{}",
                mmss(t),
                because(&c)
            ),
        });
    }
    None
}

/// What a stored roast's RoR did, read as the watch reads it live (its seen warnings every 5 s
/// from CHARGE + 30 s to 10 s before DROP): the first of each kind, with when and why.
pub fn events(
    ticks: &[Tick],
    dry: Option<f64>,
    fc: Option<f64>,
    drop: f64,
    drop_bt: Option<f64>,
) -> Vec<RorWarning> {
    let model = ThermalModel::prior();
    let mut out: Vec<RorWarning> = Vec::new();
    let mut t = 30.0;
    let mut end = 0;
    while t <= drop - 10.0 {
        while end < ticks.len() && ticks[end].t <= t {
            end += 1;
        }
        let part = &ticks[..end];
        let Some(bt_now) = part.iter().rev().find_map(|k| k.bt) else {
            t += 5.0;
            continue;
        };
        let inp = WatchInput {
            model: &model,
            ticks: part,
            t_now: t,
            bt_now,
            bt: &[],
            heater: &[],
            fan: &[],
            targets: None,
            dry_marked: dry.is_some_and(|d| t >= d),
            fc_marked: fc.filter(|f| t >= *f),
            fc_bt: None,
            drop_t: Some(drop),
            drop_bt,
        };
        let w = watch(&inp);
        for x in w.warnings {
            if out.iter().all(|o| o.kind != x.kind) {
                out.push(x);
            }
        }
        t += 5.0;
    }
    out.sort_by(|a, b| a.t.total_cmp(&b.t));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples every second from 0 to `n` s with BT from `bt(t)`.
    fn ticks(n: usize, bt: impl Fn(f64) -> f64) -> Vec<Tick> {
        (0..=n)
            .map(|i| Tick {
                t: i as f64,
                bt: Some(bt(i as f64)),
                heater: Some(80.0),
                fan: Some(80.0),
                ..Tick::default()
            })
            .collect()
    }

    /// A late-roast BT: RoR 10 °C/min at 300 s, falling 1.5 °C/min each minute.
    fn smooth(t: f64) -> f64 {
        let m = (t - 300.0) / 60.0;
        170.0 + 10.0 * m - 0.75 * m * m
    }

    fn input<'a>(m: &'a ThermalModel, tk: &'a [Tick], bt: &'a [f64]) -> WatchInput<'a> {
        let last = tk.last().unwrap();
        WatchInput {
            model: m,
            ticks: tk,
            t_now: last.t,
            bt_now: last.bt.unwrap(),
            bt,
            heater: &[80.0],
            fan: &[80.0],
            targets: None,
            dry_marked: true,
            fc_marked: None,
            fc_bt: None,
            drop_t: None,
            drop_bt: None,
        }
    }

    /// The plan's BT from now on: `f(k)` k s ahead.
    fn ahead(t0: f64, f: impl Fn(f64) -> f64) -> Vec<f64> {
        (1..=900).map(|k| f(t0 + k as f64)).collect()
    }

    #[test]
    fn a_smooth_late_roast_warns_of_nothing() {
        let m = ThermalModel::prior();
        let tk = ticks(420, smooth);
        let bt = ahead(420.0, smooth);
        let w = watch(&input(&m, &tk, &bt));
        assert!(w.armed);
        assert!(w.warnings.is_empty(), "{:?}", w.warnings);
        // RoR now: 10 − 1.5·2 = 7 °C/min (the slope over 30 s lags 15 s: 7.4)
        let now = w.now.unwrap();
        assert!((now - 7.4).abs() < 0.3, "{now}");
        assert!((w.trend.unwrap() + 1.5).abs() < 0.3, "{:?}", w.trend);
        assert!((w.at_60.unwrap() - 5.9).abs() < 0.3, "{:?}", w.at_60);
    }

    #[test]
    fn early_in_the_roast_the_watch_is_not_armed() {
        let m = ThermalModel::prior();
        // BT 120 °C, RoR falling fast (the early swings): no warning before DRY end.
        let f = |t: f64| 100.0 + 40.0 * (1.0 - (-t / 120.0).exp());
        let tk = ticks(150, f);
        let bt = ahead(150.0, |_| 135.0);
        let mut inp = input(&m, &tk, &bt);
        inp.dry_marked = false;
        let w = watch(&inp);
        assert!(!w.armed);
        assert!(w.warnings.is_empty());
    }

    #[test]
    fn a_crash_the_model_expects_is_warned_before_it_comes() {
        let m = ThermalModel::prior();
        // RoR 12 °C/min; the plan: 40 s more of it, then BT nearly flat.
        let f = |t: f64| 100.0 + 0.2 * t;
        let tk = ticks(420, f);
        let b0 = f(420.0);
        let bt = ahead(420.0, |t| {
            let k = t - 420.0;
            if k <= 40.0 {
                b0 + 0.2 * k
            } else {
                b0 + 0.2 * 40.0 + 0.5 / 60.0 * (k - 40.0)
            }
        });
        let w = watch(&input(&m, &tk, &bt));
        let c = w.warning(RorWarningKind::Crash).expect("crash");
        assert!(!c.seen);
        assert!(c.t > 420.0 && c.t <= 470.0, "{c:?}");
        assert!(
            c.reason.contains("the model expects it to fall"),
            "{}",
            c.reason
        );
    }

    #[test]
    fn a_crash_in_the_samples_is_seen() {
        let m = ThermalModel::prior();
        // RoR 12 °C/min, then from 380 s BT almost flat.
        let f = |t: f64| {
            if t <= 380.0 {
                150.0 + 0.2 * t
            } else {
                150.0 + 0.2 * 380.0 + 0.02 * (t - 380.0)
            }
        };
        let tk = ticks(420, f);
        let bt = ahead(420.0, |t| f(420.0) + 0.02 * (t - 420.0));
        let w = watch(&input(&m, &tk, &bt));
        let c = w.warning(RorWarningKind::Crash).expect("crash");
        assert!(c.seen, "{c:?}");
        assert!(c.reason.starts_with("RoR fell from 12.0"), "{}", c.reason);
    }

    #[test]
    fn a_crash_while_the_beans_drop_is_not_one() {
        let m = ThermalModel::prior();
        // BT falling 30 °C/min: the beans leaving at DROP.
        let f = |t: f64| {
            if t <= 400.0 {
                150.0 + 0.15 * t
            } else {
                150.0 + 0.15 * 400.0 - 0.5 * (t - 400.0)
            }
        };
        let tk = ticks(430, f);
        let bt = ahead(430.0, |t| f(430.0) - 0.5 * (t - 430.0));
        let w = watch(&input(&m, &tk, &bt));
        assert!(
            w.warning(RorWarningKind::Crash).is_none(),
            "{:?}",
            w.warnings
        );
    }

    #[test]
    fn a_flick_after_first_crack_is_expected_and_seen() {
        let m = ThermalModel::prior();
        // First crack at 450 s: RoR 6 °C/min, falling to 4 at 500 s, then the plan pushes
        // it back up to 8.
        let ror = |t: f64| {
            if t <= 500.0 {
                6.0 - 2.0 * (t - 450.0).max(0.0) / 50.0
            } else {
                (4.0 + 4.0 * (t - 500.0) / 40.0).min(8.0)
            }
        };
        let bt_of = |t: f64| {
            // integrate ror/60 from 300
            let mut b = 170.0;
            let mut s = 300.0;
            while s < t {
                b += ror(s) / 60.0;
                s += 1.0;
            }
            b
        };
        let tk = ticks(500, bt_of);
        let bt = ahead(500.0, bt_of);
        let mut inp = input(&m, &tk, &bt);
        inp.fc_marked = Some(450.0);
        let w = watch(&inp);
        let f = w.warning(RorWarningKind::Flick).expect("flick expected");
        assert!(!f.seen && f.t > 500.0 && f.t < 600.0, "{f:?}");
        assert!(f.reason.contains("rise from"), "{}", f.reason);

        // 40 s later it is in the samples.
        let tk = ticks(560, bt_of);
        let bt = ahead(560.0, bt_of);
        let mut inp = input(&m, &tk, &bt);
        inp.fc_marked = Some(450.0);
        let w = watch(&inp);
        let f = w.warning(RorWarningKind::Flick).expect("flick seen");
        assert!(f.seen, "{f:?}");
    }

    #[test]
    fn a_stall_short_of_drop_is_warned_with_the_heaters_limit() {
        let m = ThermalModel::prior();
        let tk = ticks(420, smooth);
        // The plan: BT (187 °C now) levels off at 195 °C within the next 90 s, the heater at
        // 100 %, while the profile still climbs to its DROP at 205 °C.
        let b0 = smooth(420.0);
        let bt = ahead(420.0, |t| {
            let k = t - 420.0;
            b0 + (195.0 - b0) * (1.0 - (-k / 25.0).exp())
        });
        let tg: Vec<f64> = (0..900).map(|k| b0 + 6.0 / 60.0 * k as f64).collect();
        let heater: Vec<f64> = (0..900).map(|k| (85.0 + k as f64).min(100.0)).collect();
        let mut inp = input(&m, &tk, &bt);
        inp.targets = Some(&tg);
        inp.heater = &heater;
        inp.drop_bt = Some(205.0);
        let w = watch(&inp);
        let s = w.warning(RorWarningKind::Stall).expect("stall");
        assert!(!s.seen && s.t < 420.0 + 120.0, "{s:?}");
        assert!(s.reason.contains("short of DROP at 205 °C"), "{}", s.reason);
        assert!(
            s.reason.contains("the heater reaches 100 % at"),
            "{}",
            s.reason
        );
        assert!(s.reason.contains("BT settles near"), "{}", s.reason);

        // A setpoint reached and held is not a stall.
        let held = vec![195.0; 900];
        inp.targets = Some(&held);
        inp.drop_bt = None;
        let w = watch(&inp);
        assert!(
            w.warning(RorWarningKind::Stall).is_none(),
            "{:?}",
            w.warnings
        );
    }

    #[test]
    fn a_stored_roast_is_read_for_what_its_ror_did() {
        // Smooth to 480 s, then BT flat (a stall) until DROP at 600 s.
        let f = |t: f64| {
            if t <= 480.0 {
                smooth(t)
            } else {
                smooth(480.0) + 0.005 * (t - 480.0)
            }
        };
        let tk = ticks(600, f);
        let ev = events(&tk, Some(270.0), Some(470.0), 600.0, None);
        let kinds: Vec<RorWarningKind> = ev.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&RorWarningKind::Stall), "{ev:?}");
        // The flat line after 480 is a fall of ~6 °C/min: not a crash.
        assert!(!kinds.contains(&RorWarningKind::Crash), "{ev:?}");
        let smooth_roast = events(&ticks(600, smooth), Some(270.0), Some(470.0), 600.0, None);
        assert!(smooth_roast.is_empty(), "{smooth_roast:?}");
    }

    /// Replay the roasts exported from a database (docs/prediction.md, "Checking a fit") through
    /// the live prediction every 5 s, as a drive would have seen them, and print every RoR
    /// warning: on roasts that went as planned there should be none.
    /// `ROASTWARDEN_PREDICT_DIR=dir cargo test -p roastwarden-server --lib ror_watch -- --ignored --nocapture`
    #[test]
    #[ignore = "needs ROASTWARDEN_PREDICT_DIR: a directory of roasts exported to CSV"]
    fn watch_exported_roasts() {
        use super::super::live::{predict, DropTarget, FcHistory, LiveInput, Plan, PlanKind};
        use super::super::model::{fit, tests::exported, FitQuality, Record};
        let dir = std::env::var("ROASTWARDEN_PREDICT_DIR").expect("ROASTWARDEN_PREDICT_DIR");
        let roasts = exported(&dir);
        let recs: Vec<Record> = roasts
            .iter()
            .filter_map(|(b, e, t)| {
                let t: Vec<Tick> = t.iter().filter(|k| k.t >= 0.0).copied().collect();
                Record::from_ticks(b, &t, *e.get("drop")?)
            })
            .collect();
        let f = fit(&recs);
        let q: FitQuality = f.quality.clone();
        let fc_bt = |tk: &[Tick], fc: f64| tk.iter().find(|k| k.t >= fc).and_then(|k| k.bt);
        let fcs: Vec<(String, f64)> = roasts
            .iter()
            .filter_map(|(b, e, t)| Some((b.clone(), fc_bt(t, *e.get("first_crack_start")?)?)))
            .collect();
        for (b, e, all) in &roasts {
            let Some(&drop) = e.get("drop") else { continue };
            // Older firmware did not report its mode or gains: no plan to replay.
            if all.iter().all(|k| k.gains.is_none()) {
                println!("#{b}: skipped (no control mode or gains recorded)");
                continue;
            }
            let others: Vec<f64> = fcs.iter().filter(|(x, _)| x != b).map(|f| f.1).collect();
            let mean = others.iter().sum::<f64>() / others.len().max(1) as f64;
            let fc = e.get("first_crack_start").copied();
            let dry = e.get("drying_end").copied();
            let mut given: Vec<RorWarning> = Vec::new();
            let mut n_warn = 0;
            let mut t = 30.0;
            while t < drop {
                let tk: Vec<Tick> = all
                    .iter()
                    .filter(|k| k.t >= 0.0 && k.t <= t)
                    .copied()
                    .collect();
                let Some(last) = tk.last().copied() else {
                    t += 5.0;
                    continue;
                };
                let future: Vec<f64> = (0..900)
                    .map(|k| {
                        all.iter()
                            .find(|x| x.t >= t + k as f64)
                            .and_then(|x| x.setpoint)
                            .or(last.setpoint)
                            .unwrap_or(0.0)
                    })
                    .collect();
                let plan = match (last.auto, last.setpoint, last.gains) {
                    (true, Some(_), Some(g)) => Plan {
                        kind: PlanKind::Drive,
                        setpoints: Some(future.clone()),
                        targets: Some(future),
                        heater: vec![last.heater.unwrap_or(0.0)],
                        fan: vec![last.fan.unwrap_or(0.0)],
                        gains: Some(g),
                        label: "recorded setpoints".into(),
                    },
                    _ => Plan {
                        kind: PlanKind::Held,
                        setpoints: None,
                        targets: None,
                        heater: vec![last.heater.unwrap_or(0.0)],
                        fan: vec![last.fan.unwrap_or(0.0)],
                        gains: None,
                        label: "held".into(),
                    },
                };
                let p = predict(&LiveInput {
                    model: &f.model,
                    quality: &q,
                    ticks: &tk,
                    plan,
                    fc_history: Some(FcHistory {
                        bt_mean: mean,
                        bt_sd: 1.0,
                        roasts: others.len(),
                        basis: "the others".into(),
                    }),
                    fc_marked: fc.filter(|f| t >= *f),
                    dry_marked: dry.is_some_and(|d| t >= d),
                    drop_target: Some((DropTarget::Time(drop), "recorded".into())),
                });
                if let Some(p) = p {
                    if !p.ror.warnings.is_empty() {
                        n_warn += 1;
                    }
                    for w in p.ror.warnings {
                        if given.iter().all(|g| g.kind != w.kind) {
                            println!(
                                "#{b} at {} {:?} {} at {}: {}",
                                mmss(t),
                                w.kind,
                                if w.seen { "seen" } else { "expected" },
                                mmss(w.t),
                                w.reason
                            );
                            given.push(w);
                        }
                    }
                }
                t += 5.0;
            }
            let ev = events(all, dry, fc, drop, None);
            println!(
                "#{b}: {n_warn} predictions with a warning; afterwards: {:?}",
                ev.iter().map(|e| (e.kind, mmss(e.t))).collect::<Vec<_>>()
            );
        }
    }
}
