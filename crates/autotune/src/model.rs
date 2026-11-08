//! Process models: first or second order with dead time, their frequency response, and a
//! simulator that runs them on a recorded (or made up) input.
//!
//! Units are the caller's: `k` is output units per input unit (°C per % heater on a roaster),
//! times are seconds. Nothing here knows what the process is.

use serde::{Deserialize, Serialize};

/// A complex number, just enough for frequency responses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    pub const ONE: C64 = C64 { re: 1.0, im: 0.0 };

    pub const fn new(re: f64, im: f64) -> C64 {
        C64 { re, im }
    }

    /// `e^{iθ}`
    pub fn cis(theta: f64) -> C64 {
        C64::new(theta.cos(), theta.sin())
    }

    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    /// Phase in radians, in (-π, π].
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    pub fn recip(self) -> C64 {
        let d = self.re * self.re + self.im * self.im;
        C64::new(self.re / d, -self.im / d)
    }

    pub fn scale(self, s: f64) -> C64 {
        C64::new(self.re * s, self.im * s)
    }
}

impl std::ops::Add for C64 {
    type Output = C64;
    fn add(self, o: C64) -> C64 {
        C64::new(self.re + o.re, self.im + o.im)
    }
}

impl std::ops::Sub for C64 {
    type Output = C64;
    fn sub(self, o: C64) -> C64 {
        C64::new(self.re - o.re, self.im - o.im)
    }
}

impl std::ops::Mul for C64 {
    type Output = C64;
    fn mul(self, o: C64) -> C64 {
        C64::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl std::ops::Div for C64 {
    type Output = C64;
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: C64) -> C64 {
        self * o.recip()
    }
}

/// `k·e^{-l·s} / ((1 + t1·s)(1 + t2·s))`. `t2 = 0` is first order with dead time (FOTD).
/// With `integrating` it is `k·e^{-l·s} / (s·(1 + t2·s))` (ITD when `t2 = 0`): `k` is then the
/// rate `kv` (output per second per input) and `t1` is unused.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// Static gain, output per input (°C per %); for an integrating model the rate, output per
    /// second per input.
    pub k: f64,
    /// The larger time constant, s (unused when integrating).
    pub t1: f64,
    /// The smaller time constant, s; 0 for a first-order model.
    #[serde(default)]
    pub t2: f64,
    /// Dead time, s.
    pub l: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub integrating: bool,
}

impl Model {
    pub fn fotd(k: f64, t: f64, l: f64) -> Model {
        Model {
            k,
            t1: t,
            t2: 0.0,
            l,
            integrating: false,
        }
    }

    pub fn sotd(k: f64, t1: f64, t2: f64, l: f64) -> Model {
        Model {
            k,
            t1,
            t2,
            l,
            integrating: false,
        }
    }

    /// Integrator plus dead time: `kv·e^{-l·s}/s`.
    pub fn itd(kv: f64, l: f64) -> Model {
        Model {
            k: kv,
            t1: 0.0,
            t2: 0.0,
            l,
            integrating: true,
        }
    }

    pub fn is_valid(&self) -> bool {
        [self.k, self.t2, self.l].iter().all(|v| v.is_finite())
            && self.k != 0.0
            && (self.integrating || (self.t1.is_finite() && self.t1 > 0.0))
            && self.t2 >= 0.0
            && self.l >= 0.0
    }

    /// The rate the output moves per unit of input, per second, early in a step: `k/t1`, or
    /// `k` for an integrating model. What a short test pins down best.
    pub fn kv(&self) -> f64 {
        if self.integrating {
            self.k
        } else {
            self.k / self.t1
        }
    }

    /// Frequency response at `w` rad/s.
    pub fn freq(&self, w: f64) -> C64 {
        let first = if self.integrating {
            C64::new(0.0, w)
        } else {
            C64::new(1.0, w * self.t1)
        };
        let den = first * C64::new(1.0, w * self.t2);
        C64::cis(-w * self.l).scale(self.k) / den
    }

    /// Normalised dead time `τ = L / (L + T)`: 0 is all lag, 1 all delay. For a second-order
    /// model the smaller lag counts as delay (the half rule's spirit); 0 for an integrator.
    pub fn tau(&self) -> f64 {
        if self.integrating {
            return 0.0;
        }
        let (t, l) = self.fotd_equivalent();
        l / (l + t)
    }

    /// The model a PI rule designs on: second order becomes first (or an integrator with dead
    /// time) by Skogestad's half rule.
    pub fn for_pi(&self) -> Model {
        let l = self.l + self.t2 / 2.0;
        if self.integrating {
            Model::itd(self.k, l)
        } else {
            Model::fotd(self.k, self.t1 + self.t2 / 2.0, l)
        }
    }

    /// The first-order-plus-dead-time model a second-order one acts like (Skogestad's half
    /// rule: half the smaller lag goes to the larger, half to the dead time).
    pub fn fotd_equivalent(&self) -> (f64, f64) {
        (self.t1 + self.t2 / 2.0, self.l + self.t2 / 2.0)
    }

    /// The ultimate point: the frequency where the phase reaches −180°, as `(Ku, Tu)` (the
    /// proportional gain that would make the loop oscillate, input per output, and the
    /// period, s). `None` when the phase never gets there (no dead time and first order).
    pub fn ultimate(&self) -> Option<(f64, f64)> {
        let phase = |w: f64| {
            let lags = if self.integrating {
                std::f64::consts::FRAC_PI_2
            } else {
                (w * self.t1).atan()
            };
            -(lags + (w * self.t2).atan() + w * self.l)
        };
        let pi = std::f64::consts::PI;
        let (mut lo, mut hi) = (1e-9, 1.0);
        while phase(hi) > -pi {
            hi *= 2.0;
            if hi > 1e6 {
                return None;
            }
        }
        for _ in 0..200 {
            let mid = (lo * hi).sqrt();
            if phase(mid) > -pi {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let w = (lo * hi).sqrt();
        Some((1.0 / self.freq(w).abs(), 2.0 * pi / w))
    }

    /// Unit step response at `t` seconds (output change for a one-unit input step at 0).
    pub fn step(&self, t: f64) -> f64 {
        let s = t - self.l;
        if s <= 0.0 {
            return 0.0;
        }
        if self.integrating {
            let b = self.t2;
            return self.k * (s - b * (1.0 - (-s / b.max(1e-12)).exp()));
        }
        let (a, b) = (self.t1, self.t2);
        let shape = if b <= 0.0 {
            1.0 - (-s / a).exp()
        } else if (a - b).abs() < 1e-9 * a {
            1.0 - (1.0 + s / a) * (-s / a).exp()
        } else {
            1.0 - (a * (-s / a).exp() - b * (-s / b).exp()) / (a - b)
        };
        self.k * shape
    }
}

/// A recorded input, piecewise constant: `u[i]` holds from `t[i]` until `t[i + 1]` (zero-order
/// hold, as a heater % is applied until the next change). Before `t[0]` it is `u[0]`.
pub struct Input<'a> {
    pub t: &'a [f64],
    pub u: &'a [f64],
}

impl Input<'_> {
    /// The input in force at time `t`.
    pub fn at(&self, t: f64) -> f64 {
        match self.t.partition_point(|&x| x <= t) {
            0 => self.u[0],
            i => self.u[i - 1],
        }
    }
}

/// Simulate `m` driven by `input` and give its output at each of `times` (ascending), from
/// the state `x0 = [stage 1, output]` at `times[0]`. Deviation variables: the caller adds any
/// offset. The input is read `m.l` earlier, so a record that starts before `times[0]` feeds
/// the dead time. `step_s` bounds the integration step.
pub fn simulate(m: &Model, input: &Input, times: &[f64], x0: [f64; 2], step_s: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(times.len());
    if times.is_empty() {
        return out;
    }
    let (mut x1, mut y) = (x0[0], x0[1]);
    out.push(y);
    let e = |h: f64, t: f64| if t > 0.0 { (-h / t).exp() } else { 0.0 };
    for w in times.windows(2) {
        let (ta, tb) = (w[0], w[1]);
        let span = (tb - ta).max(0.0);
        let steps = (span / step_s).ceil().max(1.0) as usize;
        let h = span / steps as f64;
        let (e1, e2) = (e(h, m.t1), e(h, m.t2));
        for j in 0..steps {
            // The delayed input over this sub-step, taken at its middle.
            let u = input.at(ta + (j as f64 + 0.5) * h - m.l);
            if m.integrating {
                let x1_new = x1 + h * m.k * u;
                if m.t2 > 0.0 {
                    y = y * e2 + (1.0 - e2) * 0.5 * (x1 + x1_new);
                } else {
                    y = x1_new;
                }
                x1 = x1_new;
            } else if m.t2 > 0.0 {
                let x1_new = x1 * e1 + (1.0 - e1) * m.k * u;
                // The second stage sees the first's average over the step.
                y = y * e2 + (1.0 - e2) * 0.5 * (x1 + x1_new);
                x1 = x1_new;
            } else {
                y = y * e1 + (1.0 - e1) * m.k * u;
                x1 = y;
            }
        }
        out.push(y);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_matches_the_step_response() {
        for m in [
            Model::fotd(2.0, 60.0, 5.0),
            Model::sotd(1.5, 60.0, 8.0, 2.0),
            Model::itd(0.03, 4.0),
        ] {
            let t: Vec<f64> = (0..400).map(|i| i as f64).collect();
            let u: Vec<f64> = t
                .iter()
                .map(|&x| if x < 10.0 { 0.0 } else { 1.0 })
                .collect();
            let y = simulate(&m, &Input { t: &t, u: &u }, &t, [0.0, 0.0], 0.05);
            for (i, &ti) in t.iter().enumerate() {
                let want = m.step(ti - 10.0);
                let scale = if m.integrating { m.k * 400.0 } else { m.k };
                assert!(
                    (y[i] - want).abs() < 2e-3 * scale,
                    "{m:?} t={ti}: {} vs {want}",
                    y[i]
                );
            }
        }
    }

    #[test]
    fn the_ultimate_point_of_known_models() {
        // FOTD k=1, T=1, L=1: w + atan(w) = π at w ≈ 2.0288; Ku = sqrt(1 + w²).
        let (ku, tu) = Model::fotd(1.0, 1.0, 1.0).ultimate().unwrap();
        assert!((tu - std::f64::consts::TAU / 2.0288).abs() < 1e-3, "{tu}");
        assert!(
            (ku - (1.0f64 + 2.0288 * 2.0288).sqrt()).abs() < 1e-3,
            "{ku}"
        );
        // ITD kv, L: the phase is -π/2 - wL, so w = π/(2L) and Ku = w/kv.
        let (ku, tu) = Model::itd(0.02, 5.0).ultimate().unwrap();
        assert!((tu - 20.0).abs() < 1e-6 && (ku - std::f64::consts::PI / 10.0 / 0.02).abs() < 1e-6);
        assert!(Model::fotd(1.0, 10.0, 0.0).ultimate().is_none());
    }

    #[test]
    fn frequency_response_has_the_dead_time_phase() {
        let m = Model::fotd(2.0, 50.0, 10.0);
        let w = 0.05;
        let p = m.freq(w);
        assert!((p.abs() - 2.0 / (1.0 + (w * 50.0f64).powi(2)).sqrt()).abs() < 1e-12);
        let phase = -(w * 50.0f64).atan() - w * 10.0;
        assert!((p.arg() - phase).abs() < 1e-12);
    }
}
