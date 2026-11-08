//! PI designs on an identified model, each with what it costs and how robust it is.
//!
//! - **SIMC** (Skogestad 2003), the default: one knob `τc` (as a multiple of the effective
//!   dead time θ) from tight (1·θ, Ms ≈ 1.6–1.7) to smooth (3·θ, Ms ≈ 1.25). On a first-order
//!   model `Kc = T/(k(τc + θ))`, `Ti = min(T, 4(τc + θ))`; on an integrator
//!   `Kc = 1/(kv(τc + θ))`, `Ti = 4(τc + θ)`. The `4(τc + θ)` cap is what keeps a lag-dominant
//!   process's load recovery from being slow.
//! - **AMIGO PI** (Åström & Hägglund 2006, eqs. 7.5–7.6): a fixed robust design (Ms ≈ 1.4),
//!   the conservative cross-check.
//! - **Ziegler–Nichols PI and Tyreus–Luyben PI** from the model's ultimate point: for
//!   comparison with the first autotune. Not meant for lag-dominant processes (Z–N PI comes
//!   out near Ms 2.6 on this roaster).
//!
//! θ includes half the controller's sample time (a sampled controller adds about `dt/2` of
//! delay), and so does every figure.

use serde::{Deserialize, Serialize};

use crate::model::Model;
use crate::robust::{evaluate, ramp_lag, robust_box, Pid, RobustBox, Robustness};

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Simc,
    Amigo,
    ZnPi,
    TyreusLuybenPi,
}

impl Rule {
    pub const ALL: [Rule; 4] = [Rule::Simc, Rule::Amigo, Rule::ZnPi, Rule::TyreusLuybenPi];

    pub fn id(&self) -> &'static str {
        match self {
            Rule::Simc => "simc_pi",
            Rule::Amigo => "amigo_pi",
            Rule::ZnPi => "zn_pi",
            Rule::TyreusLuybenPi => "tyreus_luyben_pi",
        }
    }

    pub fn from_id(id: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.id() == id)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Rule::Simc => "SIMC PI",
            Rule::Amigo => "AMIGO PI",
            Rule::ZnPi => "Ziegler–Nichols PI",
            Rule::TyreusLuybenPi => "Tyreus–Luyben PI",
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesignOptions {
    /// The controller's sample time, s.
    pub dt: f64,
    /// The measurement's noise (standard deviation) the controller will see.
    pub noise: Option<f64>,
    /// The measurement's smallest step.
    pub quantum: Option<f64>,
    /// A setpoint ramp to report the lag for, output units per second (10 °C/min = 0.1667).
    pub ramp_rate: f64,
}

impl Default for DesignOptions {
    fn default() -> Self {
        DesignOptions {
            dt: 1.0,
            noise: None,
            quantum: None,
            ramp_rate: 10.0 / 60.0,
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Ms within 1.8.
    Ok,
    /// Ms 1.8–2.0: works, little margin.
    Warn,
    /// Ms above 2.0 (or unstable): do not apply.
    Refuse,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub rule: Rule,
    pub label: String,
    /// SIMC's `τc` as a multiple of θ.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tau_c: Option<f64>,
    /// Parallel gains in process units: input per output (% per °C), `ki` per second.
    pub pid: Pid,
    /// Integral time `kp/ki`, s.
    pub ti: f64,
    pub robustness: Robustness,
    pub robust_box: Option<RobustBox>,
    /// The input's standard deviation from the measurement noise alone (`kp·σ`), input units.
    pub jitter: Option<f64>,
    /// The input step one measurement step causes (`kp·q`).
    pub step_kick: Option<f64>,
    /// How far the output trails `ramp_rate` with no setpoint lead, output units; `None` on an
    /// integrating model (it does not trail).
    pub ramp_lag: Option<f64>,
    pub verdict: Verdict,
    pub notes: Vec<String>,
}

/// The effective dead time a design uses: the model's (with a second lag's half) plus half a
/// sample.
pub fn theta(model: &Model, dt: f64) -> f64 {
    model.for_pi().l + dt.max(0.0) / 2.0
}

/// Gains for `rule` on `model` (any structure: a second-order model is designed on its
/// first-order equivalent). `tau_c` is SIMC's knob (multiples of θ); others ignore it.
pub fn gains(rule: Rule, model: &Model, tau_c: f64, dt: f64) -> Option<Pid> {
    let m = model.for_pi();
    let th = theta(model, dt).max(1e-6);
    let (kp, ti) = match rule {
        Rule::Simc => {
            let tc = tau_c.max(0.1) * th;
            if m.integrating {
                (1.0 / (m.k * (tc + th)), 4.0 * (tc + th))
            } else {
                (m.t1 / (m.k * (tc + th)), m.t1.min(4.0 * (tc + th)))
            }
        }
        Rule::Amigo => {
            if m.integrating {
                (0.35 / (m.k * th), 13.4 * th)
            } else {
                let (t, l) = (m.t1, th);
                (
                    0.15 / m.k + (0.35 - l * t / (l + t).powi(2)) * t / (m.k * l),
                    0.35 * l + 13.0 * l * t * t / (t * t + 12.0 * l * t + 7.0 * l * l),
                )
            }
        }
        Rule::ZnPi | Rule::TyreusLuybenPi => {
            let sampled = Model { l: th, ..m };
            let (ku, tu) = sampled.ultimate()?;
            if rule == Rule::ZnPi {
                (0.45 * ku, tu / 1.2)
            } else {
                (ku / 3.2, 2.2 * tu)
            }
        }
    };
    (kp.is_finite() && kp > 0.0 && ti.is_finite() && ti > 0.0).then(|| Pid::pi(kp, kp / ti))
}

/// A design with its figures. `None` when the model gives no gains for the rule.
pub fn design(rule: Rule, model: &Model, tau_c: f64, opt: &DesignOptions) -> Option<Design> {
    let pid = gains(rule, model, tau_c, opt.dt)?;
    let robustness = evaluate(model, &pid, None, opt.dt)?;
    let robust_box = robust_box(model, &pid, None, opt.dt);
    let mut notes = Vec::new();
    let unstable = !robustness.gain_margin.is_finite() && robustness.ms > 10.0;
    let verdict = if unstable || robustness.ms > 2.0 || robustness.gain_margin < 1.0 {
        notes.push(format!(
            "Ms {:.2}: too little margin to use (above 2.0)",
            robustness.ms
        ));
        Verdict::Refuse
    } else if robustness.ms > 1.8 {
        notes.push(format!(
            "Ms {:.2}: little margin (above 1.8)",
            robustness.ms
        ));
        Verdict::Warn
    } else {
        Verdict::Ok
    };
    if let Some(b) = robust_box {
        // Twice the gain or the dead time takes even SIMC's default (τc 1.5θ) to Ms ≈ 2.5;
        // above 3 the design has little room for the roaster to change (more beans, a fan).
        if b.delay_x2 > 3.0 || b.gain_x2 > 3.0 {
            notes.push(format!(
                "fragile if the process changes: Ms {:.1} with twice the gain, {:.1} with twice the dead time",
                b.gain_x2, b.delay_x2
            ));
        }
    }
    if matches!(rule, Rule::ZnPi | Rule::TyreusLuybenPi) && model.for_pi().tau() < 0.1 {
        notes.push(
            "an ultimate-point rule on a lag-dominant process: for comparison only".to_string(),
        );
    }
    Some(Design {
        rule,
        label: match rule {
            Rule::Simc => format!("SIMC PI, τc {tau_c:.1}·θ"),
            _ => rule.label().to_string(),
        },
        tau_c: (rule == Rule::Simc).then_some(tau_c),
        ti: pid.kp / pid.ki,
        jitter: opt.noise.map(|s| pid.kp * s),
        step_kick: opt.quantum.map(|q| pid.kp * q),
        ramp_lag: (!model.integrating).then(|| ramp_lag(&model.for_pi(), &pid, opt.ramp_rate)),
        pid,
        robustness,
        robust_box,
        verdict,
        notes,
    })
}

/// The SIMC knob's usual stops, tight to smooth.
pub const TAU_C_STOPS: [f64; 4] = [1.0, 1.5, 2.0, 3.0];
/// The default: Ms about 1.5, and still under 2.5 if the gain or the dead time doubles.
pub const TAU_C_DEFAULT: f64 = 1.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simc_on_an_integrator_matches_skogestad() {
        // τc = θ: Kc = 1/(2 kv θ), Ti = 8θ, Ms 1.70 (no sampling).
        let m = Model::itd(0.01, 5.0);
        let g = gains(Rule::Simc, &m, 1.0, 0.0).unwrap();
        assert!(
            (g.kp - 10.0).abs() < 1e-9 && (g.kp / g.ki - 40.0).abs() < 1e-9,
            "{g:?}"
        );
        let d = design(
            Rule::Simc,
            &m,
            1.0,
            &DesignOptions {
                dt: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!((d.robustness.ms - 1.70).abs() < 0.01, "{d:?}");
        assert_eq!(d.verdict, Verdict::Ok);
        assert_eq!(d.ramp_lag, None);
    }

    #[test]
    fn the_knob_runs_from_tight_to_smooth() {
        // The research's illustrative roaster: k 1.14 °C/count → 2.9 °C/%, T 163 s, L 7.5 s.
        let m = Model::fotd(2.9, 163.0, 7.5);
        let ms: Vec<f64> = TAU_C_STOPS
            .iter()
            .map(|&tc| {
                design(Rule::Simc, &m, tc, &DesignOptions::default())
                    .unwrap()
                    .robustness
                    .ms
            })
            .collect();
        assert!(ms.windows(2).all(|w| w[1] < w[0]), "{ms:?}");
        assert!(ms[0] > 1.55 && ms[0] < 1.8, "{ms:?}");
        assert!(ms[3] < 1.35, "{ms:?}");
        // Lag-dominant: the integral time is capped at 4(τc + θ), far below T.
        let d = design(Rule::Simc, &m, 1.5, &DesignOptions::default()).unwrap();
        assert!((d.ti - 4.0 * 2.5 * 8.0).abs() < 1e-6, "{d:?}");
        // The ramp lag at 10 °C/min is small (about a degree).
        assert!(
            d.ramp_lag.unwrap() > 0.3 && d.ramp_lag.unwrap() < 3.0,
            "{d:?}"
        );
    }

    #[test]
    fn ziegler_nichols_is_fragile_on_a_lag_dominant_process() {
        let m = Model::fotd(2.9, 163.0, 7.5);
        let zn = design(Rule::ZnPi, &m, 1.5, &DesignOptions::default()).unwrap();
        let amigo = design(Rule::Amigo, &m, 1.5, &DesignOptions::default()).unwrap();
        assert!(zn.robustness.ms > 2.0, "{zn:?}");
        assert_eq!(zn.verdict, Verdict::Refuse);
        assert!(zn.notes.iter().any(|n| n.contains("comparison")));
        assert!(
            amigo.robustness.ms > 1.25 && amigo.robustness.ms < 1.55,
            "{amigo:?}"
        );
    }

    #[test]
    fn noise_figures_scale_with_kp() {
        let m = Model::fotd(2.9, 163.0, 7.5);
        let opt = DesignOptions {
            noise: Some(0.13),
            quantum: Some(0.0625),
            ..Default::default()
        };
        let d = design(Rule::Simc, &m, 1.5, &opt).unwrap();
        assert!((d.jitter.unwrap() - d.pid.kp * 0.13).abs() < 1e-12);
        assert!((d.step_kick.unwrap() - d.pid.kp * 0.0625).abs() < 1e-12);
    }
}
