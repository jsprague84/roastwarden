//! How a controller would do on a model: stability margins, the maximum sensitivity, how much
//! of the sensor's noise reaches the actuator, and the errors for a load step and a ramp.
//!
//! The controller is `C(s) = kp + ki/s + kd·s/(1 + s·kd/(kp·n))` (parallel gains in the
//! process's units: input per output, e.g. % per °C; `n` filters the derivative, `None` for
//! none). A controller sampled every `dt` seconds adds about `dt/2` of delay, which the loop
//! here includes.

use serde::{Deserialize, Serialize};

use crate::model::{C64, Model};

/// Parallel PID gains in process units (input per output; `ki` per second, `kd` in seconds).
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pid {
    pub kp: f64,
    pub ki: f64,
    #[serde(default)]
    pub kd: f64,
}

impl Pid {
    pub fn pi(kp: f64, ki: f64) -> Pid {
        Pid { kp, ki, kd: 0.0 }
    }

    /// Controller frequency response at `w` rad/s; `n` filters the derivative.
    pub fn freq(&self, w: f64, n: Option<f64>) -> C64 {
        let s = C64::new(0.0, w);
        let mut c = C64::new(self.kp, 0.0) + C64::new(self.ki, 0.0) / s;
        if self.kd > 0.0 {
            let d = s.scale(self.kd);
            c = c + match n {
                Some(n) if self.kp > 0.0 && n > 0.0 => {
                    d / (C64::ONE + s.scale(self.kd / (self.kp * n)))
                }
                _ => d,
            };
        }
        c
    }

    /// The controller's gain at high frequency: how much a fast change in the measurement
    /// moves the output. With no derivative filter a sampled D gives `kd/dt` on top.
    pub fn high_freq_gain(&self, n: Option<f64>, dt: f64) -> f64 {
        if self.kd <= 0.0 {
            return self.kp;
        }
        match n {
            Some(n) if n > 0.0 => self.kp * (1.0 + n),
            _ => self.kp + self.kd / dt.max(1e-9),
        }
    }
}

/// The figures that say whether a tuning is safe and what it costs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Robustness {
    /// Maximum sensitivity `max |1/(1 + PC)|`: 1.2–1.4 robust, 1.4–1.7 balanced, above 2
    /// fragile (Åström & Hägglund).
    pub ms: f64,
    /// Maximum complementary sensitivity `max |PC/(1 + PC)|`.
    pub mt: f64,
    /// Gain margin (how much the loop gain could grow before it oscillates); infinite when
    /// the phase never reaches -180°.
    #[serde(with = "crate::nonfinite")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<f64>))]
    pub gain_margin: f64,
    /// Phase margin, degrees.
    #[serde(with = "crate::nonfinite")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<f64>))]
    pub phase_margin_deg: f64,
    /// How much more dead time the loop takes before it oscillates, s (phase margin over the
    /// crossover frequency).
    #[serde(with = "crate::nonfinite")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<f64>))]
    pub delay_margin_s: f64,
    /// Gain crossover frequency, rad/s (the loop's speed).
    #[serde(with = "crate::nonfinite")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<f64>))]
    pub crossover_rad_s: f64,
    /// Output change per unit change of the measurement at high frequency (`kp`, plus the D
    /// term's): a sensor step of q moves the output by about this × q.
    pub noise_gain: f64,
    /// Integrated error after a unit load step at the process input (`1/ki`, output·s per
    /// input unit): smaller rejects disturbances faster.
    pub load_ie: f64,
}

/// Evaluate `pid` on `model`, sampled every `dt` seconds (0 for continuous).
pub fn evaluate(model: &Model, pid: &Pid, n: Option<f64>, dt: f64) -> Option<Robustness> {
    // (The research's check: SIMC τc = θ on an integrator gives GM 2.96, PM 46.9°, Ms 1.70.)
    if !(model.is_valid() && pid.kp >= 0.0 && pid.ki > 0.0 && pid.kd >= 0.0) {
        return None;
    }
    let delay = model.l + dt.max(0.0) / 2.0;
    let plant = Model { l: delay, ..*model };
    let loop_at = |w: f64| plant.freq(w) * pid.freq(w, n);

    // A log grid from far below the process's corner to the sampling limit.
    let slowest = if model.integrating {
        model.l.max(model.t2).max(1.0) * 50.0
    } else {
        model.t1.max(model.l).max(1.0)
    };
    let w_lo = 1e-3 / slowest;
    let w_hi = if dt > 0.0 {
        std::f64::consts::PI / dt
    } else {
        1e3 / (model.l + model.t2).max(1e-3)
    };
    const N: usize = 4000;
    let ws: Vec<f64> = (0..N)
        .map(|i| w_lo * (w_hi / w_lo).powf(i as f64 / (N - 1) as f64))
        .collect();
    let ls: Vec<C64> = ws.iter().map(|&w| loop_at(w)).collect();

    let sens = |l: C64| (C64::ONE + l).recip().abs();
    let comp = |l: C64| (l / (C64::ONE + l)).abs();
    let mut ms: f64 = 0.0;
    let mut mt: f64 = 0.0;
    let mut i_ms = 0;
    for (i, &l) in ls.iter().enumerate() {
        let s = sens(l);
        if s > ms {
            ms = s;
            i_ms = i;
        }
        mt = mt.max(comp(l));
    }
    // Refine the peak between its grid neighbours (golden section on log w).
    let (a, b) = (
        ws[i_ms.saturating_sub(1)].ln(),
        ws[(i_ms + 1).min(N - 1)].ln(),
    );
    ms = ms.max(golden_max(a, b, |lw| sens(loop_at(lw.exp()))));

    // Unwrapped phase along the grid.
    let mut phase = Vec::with_capacity(N);
    let mut prev = ls[0].arg();
    let mut offset = 0.0;
    for l in &ls {
        let mut p = l.arg() + offset;
        while p - prev > std::f64::consts::PI {
            offset -= 2.0 * std::f64::consts::PI;
            p -= 2.0 * std::f64::consts::PI;
        }
        while p - prev < -std::f64::consts::PI {
            offset += 2.0 * std::f64::consts::PI;
            p += 2.0 * std::f64::consts::PI;
        }
        phase.push(p);
        prev = p;
    }
    let pi = std::f64::consts::PI;
    let mut gain_margin = f64::INFINITY;
    let mut phase_margin_deg = f64::INFINITY;
    let mut crossover = f64::NAN;
    for i in 1..N {
        if phase[i - 1] > -pi && phase[i] <= -pi && gain_margin.is_infinite() {
            let f = (-pi - phase[i - 1]) / (phase[i] - phase[i - 1]);
            let mag = lerp(ls[i - 1].abs(), ls[i].abs(), f);
            gain_margin = 1.0 / mag;
        }
        let (m0, m1) = (ls[i - 1].abs(), ls[i].abs());
        if m0 >= 1.0 && m1 < 1.0 && crossover.is_nan() {
            let f = (1.0 - m0) / (m1 - m0);
            crossover = (lerp(ws[i - 1].ln(), ws[i].ln(), f)).exp();
            phase_margin_deg = (pi + lerp(phase[i - 1], phase[i], f)).to_degrees();
        }
    }
    let delay_margin_s = if crossover.is_finite() && crossover > 0.0 {
        phase_margin_deg.to_radians() / crossover
    } else {
        f64::INFINITY
    };
    Some(Robustness {
        ms,
        mt,
        gain_margin,
        phase_margin_deg,
        delay_margin_s,
        crossover_rad_s: crossover,
        noise_gain: pid.high_freq_gain(n, dt),
        load_ie: 1.0 / pid.ki,
    })
}

/// Ms if the process is not what the model says: the corners Åström & Hägglund and Emerson
/// use (gain doubled or halved, dead time doubled).
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RobustBox {
    pub gain_x2: f64,
    pub gain_half: f64,
    pub delay_x2: f64,
}

pub fn robust_box(model: &Model, pid: &Pid, n: Option<f64>, dt: f64) -> Option<RobustBox> {
    let ms = |m: Model| evaluate(&m, pid, n, dt).map(|r| r.ms);
    Some(RobustBox {
        gain_x2: ms(Model {
            k: model.k * 2.0,
            ..*model
        })?,
        gain_half: ms(Model {
            k: model.k / 2.0,
            ..*model
        })?,
        delay_x2: ms(Model {
            l: model.l * 2.0,
            ..*model
        })?,
    })
}

/// How far the process lags a setpoint ramp of `rate` (output units per second) once the
/// loop has settled into it, without any setpoint lead: `rate / (k·ki)` for a PI with P on the
/// error on a self-regulating process; none on an integrating one (two integrators).
pub fn ramp_lag(model: &Model, pid: &Pid, rate: f64) -> f64 {
    if model.integrating {
        0.0
    } else {
        rate / (model.k * pid.ki)
    }
}

fn lerp(a: f64, b: f64, f: f64) -> f64 {
    a + (b - a) * f
}

fn golden_max(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> f64 {
    let g = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - g * (b - a);
    let mut d = a + g * (b - a);
    for _ in 0..60 {
        if f(c) > f(d) {
            b = d;
        } else {
            a = c;
        }
        c = b - g * (b - a);
        d = a + g * (b - a);
    }
    f(f64::midpoint(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margins_of_a_known_loop() {
        // Pure integrator-like check: P control of e^{-s}/(s+1)... use FOTD k=1, T=1, L=1
        // with kp=1, tiny ki: the phase reaches -180° where w + atan(w) = π (w ≈ 2.029),
        // |P| there = 1/sqrt(1 + w²) ≈ 0.442, so the gain margin is ≈ 2.26.
        let m = Model::fotd(1.0, 1.0, 1.0);
        let r = evaluate(&m, &Pid::pi(1.0, 1e-6), None, 0.0).unwrap();
        assert!((r.gain_margin - 2.262).abs() < 0.01, "{r:?}");
        // |L| = 1 only at w = 0 for kp = 1 and k = 1: a large phase margin, no proper crossover.
        assert!(r.ms > 1.0 && r.ms < 2.0, "{r:?}");
    }

    #[test]
    fn skogestads_published_margins() {
        // SIMC, τc = θ, on kv·e^{-θs}/s: Kc = 1/(2 kv θ), Ti = 8θ → GM 2.96, PM 46.9°, Ms 1.70
        // (Skogestad 2003, Table 2).
        let (kv, th) = (0.01, 5.0);
        let m = Model::itd(kv, th);
        let kc = 1.0 / (2.0 * kv * th);
        let r = evaluate(&m, &Pid::pi(kc, kc / (8.0 * th)), None, 0.0).unwrap();
        assert!((r.gain_margin - 2.96).abs() < 0.02, "{r:?}");
        assert!((r.phase_margin_deg - 46.9).abs() < 0.3, "{r:?}");
        assert!((r.ms - 1.70).abs() < 0.01, "{r:?}");
        assert!((r.delay_margin_s - 46.9f64.to_radians() / r.crossover_rad_s).abs() < 0.1);
    }

    #[test]
    fn a_hotter_tuning_is_less_robust() {
        let m = Model::fotd(1.2, 60.0, 6.0);
        let calm = evaluate(&m, &Pid::pi(2.0, 2.0 / 60.0), None, 1.0).unwrap();
        let hot = evaluate(&m, &Pid::pi(6.0, 6.0 / 30.0), None, 1.0).unwrap();
        assert!(hot.ms > calm.ms, "{calm:?} {hot:?}");
        assert!(hot.gain_margin < calm.gain_margin);
        assert!(hot.crossover_rad_s > calm.crossover_rad_s);
        assert!(hot.noise_gain > calm.noise_gain);
    }

    #[test]
    fn an_unfiltered_d_term_multiplies_the_noise() {
        let pid = Pid {
            kp: 4.0,
            ki: 0.05,
            kd: 30.0,
        };
        assert_eq!(pid.high_freq_gain(None, 1.0), 34.0);
        assert_eq!(pid.high_freq_gain(Some(8.0), 1.0), 36.0);
        assert_eq!(Pid::pi(4.0, 0.05).high_freq_gain(None, 1.0), 4.0);
    }
}

#[cfg(test)]
mod serde_tests {
    use super::*;

    #[test]
    fn an_infinite_margin_survives_json() {
        let m = Model::fotd(1.0, 10.0, 0.0);
        let r = evaluate(&m, &Pid::pi(1.0, 0.1), None, 0.0).unwrap();
        assert!(r.gain_margin.is_infinite());
        let back: Robustness = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert!(back.gain_margin.is_infinite());
        assert_eq!(back.ms, r.ms);
    }
}
