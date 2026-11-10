//! The relay experiment: what the actuator does, sample by sample (docs/autotune-2.md).
//!
//! 1. **Approach**: full power (or `approach_power`) until the output crosses the test level.
//!    If it levels off below the level, test where it got to instead (a heat-limited
//!    process); the last minute of the approach also measures the sensor's noise.
//! 2. **Symmetric relay** with bias adaptation (Åström–Hägglund, as Marlin): switch between
//!    `bias ± d` around the test level, with a switching band from the noise; after each cycle
//!    the bias moves towards the input that gives equal halves (the holding power) and `d`
//!    widens to the room there is. Stops when three cycles agree.
//! 3. **Asymmetric relay**: the bias held, the swing larger on the side with more room
//!    (`asymmetry`× the other). The halves become unequal in a way that measures the dead time
//!    against the lag, and the mean output shifts by the process gain times the mean input
//!    shift: what a symmetric relay cannot tell.
//!
//! The state machine only decides the input. The analysis (`analysis.rs`) works on the whole
//! record afterwards; the cycles here are the switch times it needs.

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayConfig {
    /// The output level to test at (°C).
    pub setpoint: f64,
    /// Input during the approach, %.
    pub approach_power: f64,
    /// First guess of the holding power, %; `None`: the middle of the input range.
    pub bias: Option<f64>,
    /// Largest relay swing, % of input.
    pub max_swing: f64,
    /// Switching band ± around the level; `None`: from the measured noise.
    pub hysteresis: Option<f64>,
    /// Smallest switching band, whatever the noise says.
    pub min_hysteresis: f64,
    /// Band = this × the noise's standard deviation (when not set): Berner's twice the noise's
    /// half-range is about 4–5σ.
    pub hysteresis_per_sigma: f64,
    /// The band is at least this many of the signal's steps, so one toggle cannot switch.
    pub hysteresis_steps: f64,
    /// Amplitude adaptation: the oscillation's peak deviation kept between these multiples
    /// of the band (Berner: 2 and 12).
    pub min_deviation_bands: f64,
    pub max_deviation_bands: f64,
    /// Symmetric cycles: at least, at most.
    pub min_cycles: usize,
    pub max_cycles: usize,
    /// Cycles that must agree to stop the symmetric phase.
    pub agree_cycles: usize,
    /// Period spread allowed among those, a fraction of their mean.
    pub period_tolerance: f64,
    /// Half-period asymmetry allowed among those.
    pub asymmetry_tolerance: f64,
    /// Asymmetric cycles after the symmetric ones, at most; 0 skips the phase.
    pub asym_cycles: usize,
    /// At least this many asymmetric cycles; the phase ends once the last two periods agree
    /// within `asym_period_tolerance`.
    pub asym_min_cycles: usize,
    pub asym_period_tolerance: f64,
    /// Larger swing / smaller swing in the asymmetric phase.
    pub asymmetry: f64,
    /// Input limits.
    pub input_min: f64,
    pub input_max: f64,
    /// Fail when the output goes this far above the level.
    pub max_overshoot: f64,
    /// A half this long nudges the bias; this long at full swing fails.
    pub stall_s: f64,
    pub max_half_s: f64,
    /// The approach: a rise smaller than `plateau_rise` in `plateau_s` (once above
    /// `plateau_min`) is a plateau; test where `plateau_power` % of the approach's rise holds.
    pub plateau_rise: f64,
    pub plateau_s: f64,
    pub plateau_min: f64,
    pub plateau_power: f64,
    pub approach_timeout_s: f64,
    /// The approach hands over to the relay early by the coast it expects: the slope times
    /// the delay before the output first rose `rise_threshold` (the apparent dead time, as
    /// DeltaV reads it from the first step), times this. 0 waits for the crossing.
    pub approach_lead: f64,
    pub rise_threshold: f64,
    /// Whole test, s.
    pub max_total_s: f64,
}

impl Default for RelayConfig {
    fn default() -> Self {
        RelayConfig {
            setpoint: 200.0,
            approach_power: 100.0,
            bias: None,
            max_swing: 30.0,
            hysteresis: None,
            min_hysteresis: 0.2,
            hysteresis_per_sigma: 4.0,
            hysteresis_steps: 2.0,
            min_deviation_bands: 2.0,
            max_deviation_bands: 12.0,
            min_cycles: 3,
            max_cycles: 10,
            agree_cycles: 3,
            period_tolerance: 0.15,
            asymmetry_tolerance: 0.15,
            asym_cycles: 4,
            asym_min_cycles: 2,
            asym_period_tolerance: 0.05,
            asymmetry: 2.0,
            input_min: 0.0,
            input_max: 100.0,
            max_overshoot: 20.0,
            stall_s: 120.0,
            max_half_s: 300.0,
            plateau_rise: 1.0,
            plateau_s: 60.0,
            plateau_min: 120.0,
            plateau_power: 85.0,
            approach_timeout_s: 900.0,
            approach_lead: 1.0,
            rise_threshold: 1.0,
            max_total_s: 2700.0,
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Approach,
    Symmetric,
    Asymmetric,
    Done,
    Failed,
}

/// One half-cycle as switched: from `start` to `end` (s), at input `input`, measured
/// output min and max inside it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Half {
    pub high: bool,
    pub start: f64,
    pub end: f64,
    pub input: f64,
    pub bias: f64,
    pub phase: Phase,
    /// The half ran while the bias was nudged: not a clean half.
    pub spoiled: bool,
    pub y_min: f64,
    pub y_max: f64,
}

/// What the experiment saw, for the analysis and the record.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaySummary {
    pub phase: Phase,
    pub reason: Option<String>,
    /// The level tested (lower than asked when the approach levelled off).
    pub setpoint: f64,
    pub retargeted: bool,
    pub hysteresis: f64,
    /// Sensor noise measured on the approach (standard deviation).
    pub noise: Option<f64>,
    /// The signal's smallest step seen on the approach (0.25 for a raw MAX6675).
    pub quantum: Option<f64>,
    /// The swing scale the amplitude adaptation ended at (1 = all the room, up to `max_swing`).
    pub swing_scale: f64,
    /// When the relay started, s.
    pub relay_start: Option<f64>,
    pub halves: Vec<Half>,
    /// Bias after each symmetric cycle (its drift says the process changed under the test).
    pub biases: Vec<f64>,
    pub converged: bool,
}

#[derive(Debug, Clone)]
pub struct Relay {
    pub cfg: RelayConfig,
    phase: Phase,
    reason: Option<String>,
    sp: f64,
    retargeted: bool,
    hyst: f64,
    noise: Option<f64>,
    quantum: Option<f64>,
    /// Amplitude adaptation's factor on the room (≤ 1).
    scale: f64,
    /// Halves still settling after an amplitude change (not counted).
    settling: usize,
    adapted_last: bool,
    asym_periods: Vec<f64>,
    bias: f64,
    d_hi: f64,
    d_lo: f64,
    out: f64,
    high: bool,
    half_start: f64,
    last_nudge: f64,
    spoiled: bool,
    half_min: f64,
    half_max: f64,
    t_start: f64,
    relay_start: Option<f64>,
    // Approach
    approach: Vec<(f64, f64)>,
    heat_ref: f64,
    heat_ref_t: f64,
    heat_max: f64,
    start_y: f64,
    /// Seconds from the start until the output first rose `rise_threshold`.
    rise_delay: Option<f64>,
    halves: Vec<Half>,
    biases: Vec<f64>,
    converged: bool,
    sym_cycles: usize,
    asym_cycles: usize,
}

impl Relay {
    /// Start at time `t` with the output at `y`; returns the experiment and its first input.
    pub fn start(cfg: RelayConfig, t: f64, y: f64) -> Relay {
        let mid = f64::midpoint(cfg.input_min, cfg.input_max);
        let bias = cfg
            .bias
            .unwrap_or(mid)
            .clamp(cfg.input_min + 1.0, cfg.input_max - 1.0);
        let mut r = Relay {
            phase: Phase::Approach,
            reason: None,
            sp: cfg.setpoint,
            retargeted: false,
            hyst: cfg.hysteresis.unwrap_or(cfg.min_hysteresis),
            noise: None,
            quantum: None,
            scale: 1.0,
            settling: 0,
            adapted_last: false,
            asym_periods: Vec::new(),
            bias,
            d_hi: 0.0,
            d_lo: 0.0,
            out: cfg.approach_power.clamp(cfg.input_min, cfg.input_max),
            high: true,
            half_start: t,
            last_nudge: t,
            spoiled: false,
            half_min: y,
            half_max: y,
            t_start: t,
            relay_start: None,
            approach: Vec::new(),
            heat_ref: y,
            heat_ref_t: t,
            heat_max: y,
            start_y: y,
            rise_delay: None,
            halves: Vec::new(),
            biases: Vec::new(),
            converged: false,
            sym_cycles: 0,
            asym_cycles: 0,
            cfg,
        };
        r.set_symmetric_swing();
        r
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn active(&self) -> bool {
        matches!(
            self.phase,
            Phase::Approach | Phase::Symmetric | Phase::Asymmetric
        )
    }

    /// The input to apply now.
    pub fn input(&self) -> f64 {
        self.out
    }

    pub fn setpoint(&self) -> f64 {
        self.sp
    }

    pub fn bias(&self) -> f64 {
        self.bias
    }

    pub fn cycles(&self) -> usize {
        self.sym_cycles + self.asym_cycles
    }

    /// Stop from outside (the operator, a lost connection): the experiment fails with `why`.
    pub fn abort(&mut self, why: &str) {
        if self.active() {
            self.fail(why);
        }
    }

    pub fn summary(&self) -> RelaySummary {
        RelaySummary {
            phase: self.phase,
            reason: self.reason.clone(),
            setpoint: self.sp,
            retargeted: self.retargeted,
            hysteresis: self.hyst,
            noise: self.noise,
            quantum: self.quantum,
            swing_scale: self.scale,
            relay_start: self.relay_start,
            halves: self.halves.clone(),
            biases: self.biases.clone(),
            converged: self.converged,
        }
    }

    /// Feed one sample; returns the input to apply until the next.
    pub fn step(&mut self, t: f64, y: f64) -> f64 {
        if !self.active() {
            return self.out;
        }
        if !y.is_finite() {
            return self.out;
        }
        if t - self.t_start > self.cfg.max_total_s {
            return self.fail("the test took too long");
        }
        let ceiling = self.sp.max(self.heat_max) + self.cfg.max_overshoot;
        if self.phase != Phase::Approach && y > ceiling {
            return self.fail("overshoot: the output went far above the test level");
        }
        self.half_min = self.half_min.min(y);
        self.half_max = self.half_max.max(y);
        match self.phase {
            Phase::Approach => self.approach_step(t, y),
            Phase::Symmetric | Phase::Asymmetric => self.relay_step(t, y),
            _ => {}
        }
        self.out
    }

    fn approach_step(&mut self, t: f64, y: f64) {
        self.out = self
            .cfg
            .approach_power
            .clamp(self.cfg.input_min, self.cfg.input_max);
        self.approach.push((t, y));
        self.heat_max = self.heat_max.max(y);
        if y >= self.heat_ref + self.cfg.plateau_rise {
            self.heat_ref = y;
            self.heat_ref_t = t;
        }
        if self.rise_delay.is_none() && y >= self.start_y + self.cfg.rise_threshold {
            self.rise_delay = Some(t - self.t_start);
        }
        if y > self.sp + self.hyst_for_crossing() || self.coast_reaches(y) {
            self.begin_relay(t);
        } else if t - self.heat_ref_t >= self.cfg.plateau_s && y >= self.cfg.plateau_min {
            // Levelled off below the level: input ~ rise, so `plateau_power` % of the power
            // holds that share of the rise. Test there, leaving room to swing.
            let p = self.cfg.approach_power.max(1.0);
            let level = self.start_y + (self.heat_max - self.start_y) * self.cfg.plateau_power / p;
            // By that rule `plateau_power` % of the approach's power holds the new level.
            self.bias = self.cfg.plateau_power * p / 100.0;
            self.retarget(t, y, level);
        } else if t - self.t_start >= self.cfg.approach_timeout_s {
            // Still climbing at this power, slowly: holding a little lower takes most of it.
            self.bias = 0.9 * self.cfg.approach_power;
            self.retarget(t, y, self.heat_max - 5.0);
        }
    }

    /// Would the output coast past the level if the power were cut now? The slope over the
    /// last 10 samples times the apparent dead time, from halfway up.
    fn coast_reaches(&self, y: f64) -> bool {
        let (Some(delay), true) = (self.rise_delay, self.cfg.approach_lead > 0.0) else {
            return false;
        };
        if y < self.start_y + 0.5 * (self.sp - self.start_y) || self.approach.len() < 10 {
            return false;
        }
        let tail = &self.approach[self.approach.len() - 10..];
        let slope = slope(tail);
        slope > 0.0
            && y + slope * delay * self.cfg.approach_lead > self.sp + self.hyst_for_crossing()
    }

    /// The band used to end the approach (the noise is not known yet: use the configured or
    /// the smallest band).
    fn hyst_for_crossing(&self) -> f64 {
        self.cfg.hysteresis.unwrap_or(self.cfg.min_hysteresis)
    }

    fn retarget(&mut self, t: f64, y: f64, level: f64) {
        self.sp = level;
        self.retargeted = true;
        self.bias = self
            .bias
            .clamp(self.cfg.input_min + 1.0, self.cfg.input_max - 1.0);
        self.set_symmetric_swing();
        if y > self.sp {
            self.begin_relay(t);
        }
    }

    fn begin_relay(&mut self, t: f64) {
        self.noise = noise_sigma(&self.approach);
        self.quantum = smallest_step(&self.approach);
        if self.cfg.hysteresis.is_none() {
            let from_noise = self
                .noise
                .map_or(0.0, |s| s * self.cfg.hysteresis_per_sigma);
            let from_steps = self.quantum.map_or(0.0, |q| q * self.cfg.hysteresis_steps);
            self.hyst = from_noise.max(from_steps).max(self.cfg.min_hysteresis);
        }
        self.approach = Vec::new();
        self.phase = Phase::Symmetric;
        self.relay_start = Some(t);
        // The output is above the level: start with the low half.
        self.start_half(t, false);
    }

    fn set_symmetric_swing(&mut self) {
        let room = (self.bias - self.cfg.input_min)
            .min(self.cfg.input_max - self.bias)
            .min(self.cfg.max_swing)
            .max(0.0)
            * self.scale;
        self.d_hi = room;
        self.d_lo = room;
    }

    fn set_asymmetric_swing(&mut self) {
        let up = (self.cfg.input_max - self.bias).max(0.0) * self.scale;
        let down = (self.bias - self.cfg.input_min).max(0.0) * self.scale;
        let a = self.cfg.asymmetry.max(1.0);
        let cap = self.cfg.max_swing * self.scale;
        // The larger swing goes where there is more room, up to the cap; the smaller is it / γ.
        let large = cap.min(up.max(down)).min(a * up.min(down));
        let small = large / a;
        if down >= up {
            (self.d_hi, self.d_lo) = (small, large);
        } else {
            (self.d_hi, self.d_lo) = (large, small);
        }
    }

    fn start_half(&mut self, t: f64, high: bool) {
        self.high = high;
        self.half_start = t;
        self.last_nudge = t;
        self.spoiled = false;
        self.out = if high {
            self.bias + self.d_hi
        } else {
            self.bias - self.d_lo
        }
        .clamp(self.cfg.input_min, self.cfg.input_max);
        // Min and max are tracked from the switch on.
        self.half_min = f64::INFINITY;
        self.half_max = f64::NEG_INFINITY;
    }

    fn end_half(&mut self, t: f64) {
        let settling = self.settling > 0;
        self.settling = self.settling.saturating_sub(1);
        self.halves.push(Half {
            high: self.high,
            start: self.half_start,
            end: t,
            input: self.out,
            bias: self.bias,
            phase: self.phase,
            spoiled: self.spoiled || settling,
            y_min: self.half_min,
            y_max: self.half_max,
        });
        self.adapt_amplitude();
    }

    /// Berner's amplitude adaptation: keep the peak deviation from the level between
    /// `min_deviation_bands` and `max_deviation_bands` bands. Never twice in a row; the next
    /// three halves settle and do not count. The swing never grows past the room there is.
    fn adapt_amplitude(&mut self) {
        let Some(h) = self.halves.last().copied() else {
            return;
        };
        // The relay's first two halves still carry the approach (its overshoot is not the
        // oscillation's size).
        let relay_halves = self.halves.len();
        if self.adapted_last || !h.y_max.is_finite() || relay_halves <= 2 {
            self.adapted_last = false;
            return;
        }
        let dev = (h.y_max - self.sp).abs().max((self.sp - h.y_min).abs());
        let (lo, hi) = (
            self.cfg.min_deviation_bands * self.hyst,
            self.cfg.max_deviation_bands * self.hyst,
        );
        let g = if self.phase == Phase::Asymmetric {
            self.cfg.asymmetry.max(1.0)
        } else {
            1.0
        };
        let phi = if dev > hi {
            f64::midpoint(hi, lo * g) / dev
        } else if dev < lo && self.scale < 1.0 {
            f64::midpoint(lo, hi / g) / dev.max(1e-9)
        } else {
            1.0
        };
        let new_scale = (self.scale * phi).clamp(0.05, 1.0);
        if (new_scale - self.scale).abs() > 1e-3 {
            self.scale = new_scale;
            self.settling = 3;
            self.adapted_last = true;
            if self.phase == Phase::Asymmetric {
                self.set_asymmetric_swing();
                self.asym_periods.clear();
            } else {
                self.set_symmetric_swing();
            }
        }
    }

    fn relay_step(&mut self, t: f64, y: f64) {
        let switch = if self.high {
            y > self.sp + self.hyst
        } else {
            y < self.sp - self.hyst
        };
        if !switch {
            self.check_stall(t);
            return;
        }
        self.end_half(t);
        if self.high {
            // A cycle is a low half then a high half: it ends here.
            self.cycle_done();
            if !self.active() {
                return;
            }
        }
        let high = !self.high;
        self.start_half(t, high);
    }

    /// A half that runs long means the bias is off: move it the way the half is stuck (the
    /// half then does not count). Fails once nudging cannot help.
    fn check_stall(&mut self, t: f64) {
        if t - self.half_start < self.cfg.stall_s || t - self.last_nudge < self.cfg.stall_s {
            return;
        }
        let at_end = if self.high {
            self.out >= self.cfg.input_max - 1e-9
        } else {
            self.out <= self.cfg.input_min + 1e-9
        };
        if at_end || self.phase == Phase::Asymmetric {
            if t - self.half_start > self.cfg.max_half_s {
                self.fail(if self.high {
                    "the output cannot climb back to the test level"
                } else {
                    "the output cannot fall below the test level"
                });
            }
            return;
        }
        let dir = if self.high { 1.0 } else { -1.0 };
        let d = if self.high { self.d_hi } else { self.d_lo };
        self.bias =
            (self.bias + dir * d / 2.0).clamp(self.cfg.input_min + 1.0, self.cfg.input_max - 1.0);
        self.set_symmetric_swing();
        self.out = if self.high {
            self.bias + self.d_hi
        } else {
            self.bias - self.d_lo
        };
        self.spoiled = true;
        self.last_nudge = t;
    }

    fn cycle_done(&mut self) {
        let n = self.halves.len();
        if n < 2 {
            return;
        }
        let (lo, hi) = (self.halves[n - 2], self.halves[n - 1]);
        if lo.high || !hi.high || lo.phase != hi.phase {
            return;
        }
        let spoiled = lo.spoiled || hi.spoiled;
        match self.phase {
            Phase::Symmetric => {
                if spoiled {
                    return;
                }
                self.sym_cycles += 1;
                let (th, tl) = (hi.end - hi.start, lo.end - lo.start);
                let period = th + tl;
                // Towards equal halves: a long high half means too little power.
                let d = self.d_hi.min(self.d_lo);
                self.bias = (self.bias + d * (th - tl) / period)
                    .clamp(self.cfg.input_min + 1.0, self.cfg.input_max - 1.0);
                self.biases.push(self.bias);
                self.set_symmetric_swing();
                if self.sym_cycles >= self.cfg.min_cycles && self.settled() {
                    self.converged = true;
                    self.next_after_symmetric();
                } else if self.sym_cycles >= self.cfg.max_cycles {
                    self.next_after_symmetric();
                }
            }
            Phase::Asymmetric => {
                if spoiled {
                    return;
                }
                self.asym_cycles += 1;
                self.asym_periods.push(hi.end - lo.start);
                let n = self.asym_periods.len();
                let agree = n >= 2 && {
                    let (a, b) = (self.asym_periods[n - 2], self.asym_periods[n - 1]);
                    (a - b).abs() / b <= self.cfg.asym_period_tolerance
                };
                if (self.asym_cycles >= self.cfg.asym_min_cycles && agree)
                    || self.asym_cycles >= self.cfg.asym_cycles
                {
                    self.finish();
                }
            }
            _ => {}
        }
    }

    fn next_after_symmetric(&mut self) {
        if self.cfg.asym_cycles == 0 {
            self.finish();
            return;
        }
        self.phase = Phase::Asymmetric;
        // The bias settled on the holding power: hold it (the mean of the agreeing cycles').
        let k = self.cfg.agree_cycles.min(self.biases.len()).max(1);
        self.bias = self.biases[self.biases.len() - k..].iter().sum::<f64>() / k as f64;
        self.set_asymmetric_swing();
    }

    /// The last `agree_cycles` symmetric cycles agree on period and are near-symmetric.
    fn settled(&self) -> bool {
        let cycles = clean_cycles(&self.halves, Phase::Symmetric);
        let k = self.cfg.agree_cycles;
        if cycles.len() < k {
            return false;
        }
        let last = &cycles[cycles.len() - k..];
        let periods: Vec<f64> = last.iter().map(|(l, h)| h.end - l.start).collect();
        let mean = periods.iter().sum::<f64>() / k as f64;
        let spread = periods.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - periods.iter().copied().fold(f64::INFINITY, f64::min);
        let asym_ok = last.iter().all(|(l, h)| {
            let (tl, th) = (l.end - l.start, h.end - h.start);
            (th - tl).abs() / (th + tl) <= self.cfg.asymmetry_tolerance
        });
        mean > 0.0 && spread / mean <= self.cfg.period_tolerance && asym_ok
    }

    fn finish(&mut self) {
        self.phase = Phase::Done;
        self.out = self.cfg.input_min;
    }

    fn fail(&mut self, why: &str) -> f64 {
        self.phase = Phase::Failed;
        self.reason = Some(why.to_string());
        self.out = self.cfg.input_min;
        self.out
    }
}

/// Least-squares slope of `(t, y)` samples.
fn slope(s: &[(f64, f64)]) -> f64 {
    let n = s.len() as f64;
    let (mt, my) = (
        s.iter().map(|p| p.0).sum::<f64>() / n,
        s.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let num: f64 = s.iter().map(|p| (p.0 - mt) * (p.1 - my)).sum();
    let den: f64 = s.iter().map(|p| (p.0 - mt).powi(2)).sum();
    if den > 0.0 { num / den } else { 0.0 }
}

/// The signal's resolution: the coarsest of the usual steps (1, 0.5, 0.25 for a raw MAX6675,
/// …, 0.01 for two decimals) that every reading is a multiple of. Read from the values, not
/// from differences, which during a fast climb are never a single step.
pub fn smallest_step(samples: &[(f64, f64)]) -> Option<f64> {
    const STEPS: [f64; 11] = [
        1.0, 0.5, 0.25, 0.2, 0.125, 0.1, 0.0625, 0.05, 0.025, 0.01, 0.001,
    ];
    if samples.len() < 5 {
        return None;
    }
    STEPS.into_iter().find(|q| {
        samples.iter().all(|(_, y)| {
            let r = y / q;
            (r - r.round()).abs() < 1e-6 * r.abs().max(1.0)
        })
    })
}

/// Clean (low, high) cycle pairs of one phase, in order.
pub fn clean_cycles(halves: &[Half], phase: Phase) -> Vec<(Half, Half)> {
    halves
        .windows(2)
        .filter(|w| !w[0].high && w[1].high && w[0].phase == phase && w[1].phase == phase)
        .filter(|w| !w[0].spoiled && !w[1].spoiled)
        .filter(|w| (w[0].end - w[1].start).abs() < 1e-9)
        .map(|w| (w[0], w[1]))
        .collect()
}

/// The noise in a slowly moving signal: the spread of the second differences of the last 60
/// samples, scaled for white noise (`var(Δ²y) = 6σ²`). The curve's own bend (a fast heat-up
/// has a large one) is taken out first with a 9-sample moving mean of the differences.
pub fn noise_sigma(samples: &[(f64, f64)]) -> Option<f64> {
    let tail = &samples[samples.len().saturating_sub(60)..];
    if tail.len() < 20 {
        return None;
    }
    let d2: Vec<f64> = tail
        .windows(3)
        .map(|w| w[2].1 - 2.0 * w[1].1 + w[0].1)
        .collect();
    // The curve's bend comes out with a 9-sample moving median (a median, so one kink does
    // not leak into its neighbours).
    const W: usize = 4;
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let resid: Vec<f64> = (W..d2.len() - W)
        .map(|i| d2[i] - median(&mut d2[i - W..=i + W].to_vec()))
        .collect();
    // A kink (the heater coming on through a dead time) is one huge difference, not noise:
    // drop what is beyond 4 robust spreads.
    let mad = 1.4826 * median(&mut resid.iter().map(|v| v.abs()).collect());
    let kept: Vec<f64> = resid
        .iter()
        .copied()
        .filter(|v| mad <= 0.0 || v.abs() <= 4.0 * mad)
        .collect();
    let var = kept.iter().map(|x| x * x).sum::<f64>() / kept.len().max(1) as f64;
    Some((var / 6.0).sqrt())
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::bench::{Bench, BenchConfig};

    /// Run the experiment on a bench; returns it and the record (t, u, y).
    pub(crate) fn run(
        cfg: RelayConfig,
        bench: BenchConfig,
        u0: f64,
    ) -> (Relay, Vec<(f64, f64, f64)>) {
        let mut b = Bench::new(bench, u0);
        let mut y = b.step(u0);
        let mut t = 0.0;
        let mut r = Relay::start(cfg, t, y);
        let mut rec = Vec::new();
        while r.active() && t < 20_000.0 {
            let u = r.step(t, y);
            rec.push((t, u, y));
            y = b.step(u);
            t += b.cfg.dt;
        }
        (r, rec)
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::run;
    use super::*;
    use crate::bench::BenchConfig;
    use crate::model::Model;

    fn roaster() -> Model {
        // Roughly the real roaster at 71 % fan: holding ~76 % at 200 °C from 25 °C.
        Model::sotd(2.3, 60.0, 8.0, 2.0)
    }

    #[test]
    fn it_finds_the_holding_power_then_runs_the_asymmetric_phase() {
        let cfg = RelayConfig {
            setpoint: 190.0,
            ..RelayConfig::default()
        };
        let (r, _) = run(cfg, BenchConfig::roaster_like(roaster()), 0.0);
        let s = r.summary();
        assert_eq!(s.phase, Phase::Done, "{s:?}");
        assert!(s.converged, "{s:?}");
        // (190 - 25) / 2.3 = 71.7 % holds 190 °C.
        let bias = *s.biases.last().unwrap();
        assert!((bias - 71.7).abs() < 4.0, "bias {bias}");
        let asym = clean_cycles(&s.halves, Phase::Asymmetric);
        // It stops once two periods agree (at least 2 cycles, at most 4).
        assert!((2..=4).contains(&asym.len()), "{s:?}");
        // Heat-limited: the high swing is the room above the bias, the low swing twice it.
        let (lo, hi) = asym[0];
        let up = hi.input - hi.bias;
        let down = lo.bias - lo.input;
        assert!((down / up - 2.0).abs() < 0.05, "{up} {down}");
        // The band came from the noise: 4σ of a 0.25 °C-step sensor read once a second.
        assert!(s.hysteresis > 0.8 && s.hysteresis < 1.6, "{}", s.hysteresis);
        assert_eq!(s.quantum, Some(0.25));
    }

    #[test]
    fn a_process_that_cannot_reach_the_level_is_tested_lower() {
        let cfg = RelayConfig {
            setpoint: 260.0,
            ..RelayConfig::default()
        };
        let (r, _) = run(cfg, BenchConfig::roaster_like(roaster()), 0.0);
        let s = r.summary();
        assert!(s.retargeted, "{s:?}");
        assert!(s.setpoint < 255.0 && s.setpoint > 150.0, "{s:?}");
        assert_eq!(s.phase, Phase::Done, "{s:?}");
    }

    #[test]
    fn the_signal_resolution_comes_from_the_values() {
        let climb = |q: f64| -> Vec<(f64, f64)> {
            (0..30)
                .map(|i| (i as f64, ((25.0 + 1.37 * i as f64) / q).round() * q))
                .collect()
        };
        assert_eq!(smallest_step(&climb(0.25)), Some(0.25));
        assert_eq!(smallest_step(&climb(1.0)), Some(1.0));
        // Firmware 3.4.0: four 0.25 °C reads averaged, sent with two decimals.
        let averaged: Vec<(f64, f64)> = (0..30)
            .map(|i| {
                (
                    i as f64,
                    ((25.0 + 1.37 * i as f64 + 0.0625) * 100.0).round() / 100.0,
                )
            })
            .collect();
        assert_eq!(smallest_step(&averaged), Some(0.01));
    }

    #[test]
    fn runaway_output_fails_the_test() {
        let mut r = Relay::start(
            RelayConfig {
                setpoint: 100.0,
                ..RelayConfig::default()
            },
            0.0,
            25.0,
        );
        r.step(1.0, 101.0);
        assert_eq!(r.phase(), Phase::Symmetric);
        r.step(2.0, 125.0);
        assert_eq!(r.phase(), Phase::Failed);
        assert_eq!(r.input(), 0.0);
    }

    #[test]
    fn noise_from_second_differences() {
        // White noise of σ 0.3 on a fast, bending heat-up curve (with the kink where the heater
        // comes on through the dead time): neither is noise. One 60-sample window spreads
        // about ±25 %; the mean over many is unbiased.
        let est = |seed: u64| {
            let mut b = crate::bench::Bench::new(
                crate::bench::BenchConfig {
                    noise: 0.3,
                    seed,
                    ..crate::bench::BenchConfig::new(Model::fotd(2.0, 30.0, 15.0), 25.0)
                },
                0.0,
            );
            let s: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, b.step(100.0))).collect();
            noise_sigma(&s).unwrap()
        };
        let all: Vec<f64> = (1..=100).map(est).collect();
        let mean = all.iter().sum::<f64>() / all.len() as f64;
        assert!((mean - 0.3).abs() < 0.02, "{mean}");
        assert!(all.iter().all(|s| (0.15..0.5).contains(s)), "{all:?}");
    }
}
