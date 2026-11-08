//! The server-side BT loop: the gains and the PID that runs them, the same algorithm as the
//! firmware's so a run behaves alike on either side.

use serde::{Deserialize, Serialize};

/// Server PID gains, in heater % (P per °C, I per °C·s, D per °C/s).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PidGains {
    pub kp: f64,
    pub ki: f64,
    #[serde(default)]
    pub kd: f64,
}

impl Default for PidGains {
    fn default() -> Self {
        Self {
            kp: 3.0,
            ki: 0.03,
            kd: 0.0,
        }
    }
}

/// The server-side BT loop: P on error, D on measurement, conditional
/// integration and feedforward, all in heater % (0..100). The same algorithm
/// as the firmware's `control_math.h`, so a run behaves alike on either side.
#[derive(Debug, Clone, Default)]
pub struct ServerPid {
    pub gains: PidGains,
    pub integ: f64,
    last_input: Option<f64>,
    /// Error and output of the last step with a fresh input.
    last_err: Option<f64>,
    last_out: Option<f64>,
    started: bool,
}

impl ServerPid {
    pub fn new(gains: PidGains) -> Self {
        Self {
            gains,
            ..Self::default()
        }
    }

    /// Bumpless start: the first output equals `current` (the heater now).
    pub fn reset(&mut self, sp: f64, input: f64, current: f64, ff: f64) {
        self.last_input = Some(input);
        self.integ = (current - ff - self.gains.kp * (sp - input)).clamp(-100.0, 100.0);
        self.started = true;
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// New gains mid-run (gain schedule) without a step in the output: the
    /// integral takes up the change in the P term at the last error.
    pub fn set_gains(&mut self, g: PidGains) {
        if let Some(e) = self.last_err {
            self.integ = (self.integ + (self.gains.kp - g.kp) * e).clamp(-100.0, 100.0);
        }
        self.gains = g;
    }

    /// One step `dt` seconds after the previous one. Without a fresh input
    /// the output holds where it was (dropping P and D on a stale reading
    /// would step the heater by kp·e).
    pub fn compute(&mut self, sp: f64, input: Option<f64>, dt: f64, ff: f64) -> f64 {
        let Some(input) = input else {
            return self.last_out.unwrap_or((ff + self.integ).clamp(0.0, 100.0));
        };
        let e = sp - input;
        let d_in = self.last_input.map_or(0.0, |l| input - l);
        self.last_input = Some(input);
        let p = self.gains.kp * e;
        let d = if dt > 0.0 {
            -self.gains.kd * d_in / dt
        } else {
            0.0
        };
        let candidate = self.integ + self.gains.ki * e * dt.max(0.0);
        let raw = ff + p + candidate + d;
        if !((raw > 100.0 && e > 0.0) || (raw < 0.0 && e < 0.0)) {
            self.integ = candidate.clamp(-100.0 - ff, 100.0 - ff);
        }
        let out = (ff + p + self.integ + d).clamp(0.0, 100.0);
        self.last_err = Some(e);
        self.last_out = Some(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(kp: f64, ki: f64, kd: f64) -> ServerPid {
        ServerPid::new(PidGains { kp, ki, kd })
    }

    #[test]
    fn the_terms_add_up_and_the_derivative_acts_on_the_measurement() {
        let mut p = pid(2.0, 0.5, 1.0);
        // e = 2 with no earlier input: P 4, I 0.5 · 2 · 1 s = 1, D 0, plus the feedforward 5.
        assert_eq!(p.compute(10.0, Some(8.0), 1.0, 5.0), 10.0);
        assert_eq!(p.integ, 1.0);
        // e = 1 over 2 s: P 2, I 1 + 0.5 · 1 · 2 = 2, D −kd · (9 − 8) / 2 = −0.5.
        assert_eq!(p.compute(10.0, Some(9.0), 2.0, 5.0), 8.5);
        assert_eq!(p.integ, 2.0);
        // No time passed: no derivative.
        let mut q = pid(0.0, 0.0, 1.0);
        q.compute(0.0, Some(0.0), 1.0, 0.0);
        assert_eq!(q.compute(0.0, Some(5.0), 0.0, 50.0), 50.0);
    }

    #[test]
    fn a_stale_reading_holds_the_output_or_starts_from_the_feedforward_and_integral() {
        let mut p = pid(2.0, 0.0, 0.0);
        // A bumpless reset puts the integral at current − ff − kp · e = 50 − 10 − 2 · 5 = 30;
        // with no output yet, a stale reading gives ff + integral.
        p.reset(100.0, 95.0, 50.0, 10.0);
        assert_eq!(p.compute(100.0, None, 1.0, 10.0), 40.0);
        // Once there is an output, a stale reading repeats it.
        assert_eq!(p.compute(100.0, Some(95.0), 1.0, 10.0), 50.0);
        assert_eq!(p.compute(100.0, None, 1.0, 10.0), 50.0);
    }

    #[test]
    fn the_integral_stops_at_saturation_and_only_there() {
        // Pinned high with a positive error: the integral holds.
        let mut p = pid(10.0, 1.0, 0.0);
        assert_eq!(p.compute(20.0, Some(0.0), 1.0, 0.0), 100.0);
        assert_eq!(p.integ, 0.0);
        // Pinned low with a negative error: it holds too.
        let mut p = pid(10.0, 1.0, 0.0);
        assert_eq!(p.compute(0.0, Some(20.0), 1.0, 0.0), 0.0);
        assert_eq!(p.integ, 0.0);
        // Exactly at a limit it still integrates: 100 is not above 100, 0 is not below 0.
        let mut p = pid(4.0, 1.0, 0.0);
        p.reset(10.0, 0.0, 90.0, 0.0); // integral 90 − 4 · 10 = 50
        assert_eq!(p.compute(10.0, Some(0.0), 1.0, 0.0), 100.0);
        assert_eq!(p.integ, 60.0);
        let mut p = pid(4.0, 1.0, 0.0);
        p.reset(0.0, 10.0, 10.0, 0.0); // integral 10 + 4 · 10 = 50
        assert_eq!(p.compute(0.0, Some(10.0), 1.0, 0.0), 0.0);
        assert_eq!(p.integ, 40.0);
    }

    #[test]
    fn the_integral_is_bounded_by_what_the_feedforward_leaves() {
        // The first step is pinned (the integral holds at 0, the output is the feedforward); the
        // second is not, a large D pulling the sum down, and the integral stops at 100 − ff.
        let mut p = pid(0.0, 1.0, 100.0);
        assert_eq!(p.compute(100.0, Some(0.0), 1.0, 30.0), 30.0);
        assert_eq!(p.compute(100.0, Some(1.0), 1.0, 30.0), 0.0);
        assert_eq!(p.integ, 70.0);
        // And at −100 − ff on the way down.
        let mut p = pid(0.0, 1.0, 180.0);
        assert_eq!(p.compute(0.0, Some(200.0), 1.0, 30.0), 30.0);
        assert_eq!(p.compute(0.0, Some(199.0), 1.0, 30.0), 80.0);
        assert_eq!(p.integ, -130.0);
    }
}
