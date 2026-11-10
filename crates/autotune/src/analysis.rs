//! From a finished experiment's record to the report: the harmonics, Berner's τ, three model
//! fits and the choice between them, the model to design on, the designs, and how far to
//! trust it all (docs/autotune-2.md, "The analysis").

use serde::{Deserialize, Serialize};

use crate::design::{Design, DesignOptions, Rule, TAU_C_DEFAULT, TAU_C_STOPS, design};
use crate::fit::{Fit, FitOptions, Order, Record, fit, holdout_rmse};
use crate::harmonic::{FreqPoint, Harmonics, berner_tau, harmonics};
use crate::model::Model;
use crate::relay::{Phase, RelaySummary, clean_cycles};

/// Bump when the report's meaning changes (stored runs keep the version they were made with).
pub const VERSION: u32 = 1;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitReport {
    #[serde(flatten)]
    pub fit: Fit,
    /// The fit's RMS error over the measured noise: near 1 leaves only noise.
    pub residual_ratio: Option<f64>,
    /// RMS error predicting the last cycle from a fit on the rest.
    pub holdout_rmse: Option<f64>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Good,
    Acceptable,
    Poor,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub text: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    pub quality: Quality,
    pub warnings: Vec<Warning>,
    /// The level tested and the input that held it.
    pub setpoint: f64,
    pub holding_power: Option<f64>,
    /// The holding power the best fit implies.
    pub fitted_holding_power: Option<f64>,
    /// How fast the input that holds the level moved during the test (the trend of each clean
    /// cycle's mean input), input per minute: the process changing under the test.
    pub bias_drift_per_min: Option<f64>,
    pub noise: Option<f64>,
    pub quantum: Option<f64>,
    pub hysteresis: f64,
    pub symmetric: Option<Harmonics>,
    pub asymmetric: Option<Harmonics>,
    /// Berner's normalised dead time from the asymmetric half-periods, and the swing ratio
    /// applied.
    pub berner_tau: Option<f64>,
    pub gamma: Option<f64>,
    pub fits: Vec<FitReport>,
    /// The structure that explains the record best.
    pub chosen: Option<Order>,
    /// What the designs use: the chosen model, or an integrator with its rate when the lag is
    /// not pinned down.
    pub design_model: Option<Model>,
    pub design_basis: String,
    /// τ of the chosen model.
    pub tau: Option<f64>,
    /// The self-regulating model the ramp lag is figured on, when the chosen fit pins its
    /// gain down (an integrator does not trail a ramp, but the roaster does).
    pub ramp_model: Option<Model>,
    /// The design model's response at the asymmetric oscillation against the harmonics':
    /// gain ratio and phase difference, degrees.
    pub agreement: Option<Agreement>,
    /// The design model's ultimate point (Ku input per output, Tu s).
    pub ku: Option<f64>,
    pub tu: Option<f64>,
    pub options: DesignOptions,
    pub designs: Vec<Design>,
    /// Index into `designs` of the recommended one (SIMC at τc 1.5θ).
    pub recommended: Option<usize>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Agreement {
    pub gain_ratio: f64,
    pub phase_diff_deg: f64,
}

/// Analyse a finished experiment. `rec` is the whole record (input as applied); `dt` the
/// controller's sample time (for the designs' half-sample of delay).
pub fn analyse(rec: Record, summary: &RelaySummary, dt: f64) -> Analysis {
    let mut warnings = Vec::new();
    let mut warn = |code: &str, text: String| {
        warnings.push(Warning {
            code: code.into(),
            text,
        });
    };
    let options = DesignOptions {
        dt,
        noise: summary.noise,
        quantum: summary.quantum,
        ..Default::default()
    };
    let mut a = Analysis {
        version: VERSION,
        quality: Quality::Poor,
        warnings: Vec::new(),
        setpoint: summary.setpoint,
        holding_power: summary.biases.last().copied(),
        fitted_holding_power: None,
        bias_drift_per_min: None,
        noise: summary.noise,
        quantum: summary.quantum,
        hysteresis: summary.hysteresis,
        symmetric: None,
        asymmetric: None,
        berner_tau: None,
        gamma: None,
        fits: Vec::new(),
        chosen: None,
        design_model: None,
        design_basis: String::new(),
        tau: None,
        ramp_model: None,
        agreement: None,
        ku: None,
        tu: None,
        options,
        designs: Vec::new(),
        recommended: None,
    };
    if summary.phase == Phase::Failed {
        warn(
            "failed",
            format!(
                "the test stopped: {}",
                summary.reason.as_deref().unwrap_or("unknown")
            ),
        );
    }
    if summary.retargeted {
        warn(
            "retargeted",
            format!(
                "the process could not reach the level asked; tested at {:.1}",
                summary.setpoint
            ),
        );
    }
    if !summary.converged && summary.phase == Phase::Done {
        warn(
            "not_converged",
            "the symmetric cycles never agreed; the holding power may be off".into(),
        );
    }

    // Drift: in a steady oscillation the relay's average input over a cycle is what holds
    // the level, whatever the bias does; its trend over the clean cycles is the process
    // changing under the test. Within each phase only: the asymmetric relay shifts the mean
    // output off the level (that is how it measures gain), so the mean input steps between
    // phases. The first symmetric cycle still carries the approach.
    let sym = clean_cycles(&summary.halves, Phase::Symmetric);
    let asym = clean_cycles(&summary.halves, Phase::Asymmetric);
    let cycle_points = |c: &[(crate::relay::Half, crate::relay::Half)]| -> Vec<(f64, f64)> {
        c.iter()
            .map(|(lo, hi)| {
                let (tl, th) = (lo.end - lo.start, hi.end - hi.start);
                (
                    f64::midpoint(lo.start, hi.end) / 60.0,
                    (lo.input * tl + hi.input * th) / (tl + th),
                )
            })
            .collect()
    };
    let groups = [
        cycle_points(sym.get(1..).unwrap_or(&[])),
        cycle_points(&asym),
    ];
    let (mut num, mut den, mut used) = (0.0, 0.0, 0);
    for g in groups.iter().filter(|g| g.len() >= 2) {
        let n = g.len() as f64;
        let mt = g.iter().map(|p| p.0).sum::<f64>() / n;
        let mu = g.iter().map(|p| p.1).sum::<f64>() / n;
        num += g.iter().map(|p| (p.0 - mt) * (p.1 - mu)).sum::<f64>();
        den += g.iter().map(|p| (p.0 - mt).powi(2)).sum::<f64>();
        used += g.len();
    }
    if used >= 3 && den > 0.0 {
        a.bias_drift_per_min = Some(num / den);
    }

    // Harmonics: the agreeing symmetric cycles, and the asymmetric ones.
    let period_of = |c: &[(crate::relay::Half, crate::relay::Half)]| {
        let first = c.first()?.0.start;
        let last = c.last()?.1.end;
        Some((first, (last - first) / c.len() as f64, c.len()))
    };
    let sym_tail = &sym[sym.len().saturating_sub(3)..];
    if let Some((from, p, n)) = period_of(sym_tail) {
        a.symmetric = harmonics(rec, from, p, n, false);
    }
    if let Some((from, p, n)) = period_of(&asym) {
        a.asymmetric = harmonics(rec, from, p, n, true);
    }

    // Fits over the relay part of the record.
    let Some(relay_start) = summary.relay_start else {
        warn("no_relay", "the relay never started".into());
        a.warnings = warnings;
        return a;
    };
    let end = rec.t.last().copied().unwrap_or(relay_start);
    let tu_guess = period_guess(&a);
    let last_cycle_start = summary
        .halves
        .iter()
        .rev()
        .find(|h| !h.high)
        .map(|h| h.start);
    for order in [Order::Integrating, Order::First, Order::Second] {
        let opt = fit_options(&a, relay_start, end, order);
        if let Some(f) = fit(rec, opt) {
            a.fits.push(FitReport {
                residual_ratio: summary.noise.filter(|s| *s > 0.0).map(|s| f.rmse / s),
                holdout_rmse: last_cycle_start
                    .filter(|t| *t > relay_start + 2.0 * tu_guess)
                    .and_then(|split| holdout_rmse(rec, opt, split, &f)),
                fit: f,
            });
        }
    }
    if a.fits.is_empty() {
        warn("no_fit", "no model could be fitted to the record".into());
        a.warnings = warnings;
        return a;
    }
    // The lowest AIC among fits that leave little more than noise; else the lowest.
    let near_noise = |r: &FitReport| r.residual_ratio.is_none_or(|x| x <= 2.0);
    let pick = a
        .fits
        .iter()
        .filter(|r| near_noise(r))
        .min_by(|x, y| x.fit.aic.total_cmp(&y.fit.aic))
        .or_else(|| a.fits.iter().min_by(|x, y| x.fit.aic.total_cmp(&y.fit.aic)))
        .cloned();
    let Some(best) = pick else {
        a.warnings = warnings;
        return a;
    };
    a.chosen = Some(best.fit.order);
    a.tau = Some(best.fit.model.tau());
    // The input that really holds the level, from the fit (the relay's bias is only as good as
    // its last adaptation, and Berner's τ is sensitive to it: a 1.6 % error moved τ from 0.04
    // to 0.23 on a lag-dominant bench process).
    let f = &best.fit;
    a.fitted_holding_power = if f.model.integrating {
        Some(-f.drift / f.model.k)
    } else {
        Some((summary.setpoint - f.offset) / f.model.k)
    }
    .filter(|u| u.is_finite());
    if let Some(u0) = a.fitted_holding_power.or(a.holding_power) {
        let (mut t_hi, mut t_lo, mut d_hi, mut d_lo) = (0.0, 0.0, 0.0, 0.0);
        for (lo, hi) in &asym {
            t_lo += lo.end - lo.start;
            t_hi += hi.end - hi.start;
            d_lo += u0 - lo.input;
            d_hi += hi.input - u0;
        }
        if t_hi > 0.0 && t_lo > 0.0 && d_hi > 0.0 && d_lo > 0.0 {
            let rho = (t_hi / t_lo).max(t_lo / t_hi);
            let gamma = (d_hi / d_lo).max(d_lo / d_hi);
            a.gamma = Some(gamma);
            a.berner_tau = berner_tau(rho, gamma);
        }
    }
    if let Some(r) = best.residual_ratio
        && r > 2.0
    {
        warn(
            "poor_fit",
            format!(
                "the best model leaves {r:.1}× the sensor's noise unexplained (the process changed during the test, or it is not linear here)"
            ),
        );
    }
    if let Some(h) = best.holdout_rmse
        && h > 3.0 * best.fit.rmse.max(summary.noise.unwrap_or(0.0))
    {
        warn(
            "holdout",
            format!(
                "the model fitted without the last cycle predicts it poorly (RMS {h:.2} against {:.2})",
                best.fit.rmse
            ),
        );
    }

    // What to design on.
    let m = best.fit.model;
    let lag_dominant = m.tau() < 0.1;
    let design_model = if m.integrating {
        a.design_basis = "integrator plus dead time: the best fit".into();
        m
    } else if !best.fit.lag_identified || lag_dominant {
        let pi = m.for_pi();
        a.design_basis = if best.fit.lag_identified {
            format!(
                "integrator plus dead time: τ {:.2} is lag-dominant (below 0.1), so only the rate kv = k/T and the dead time count",
                m.tau()
            )
        } else {
            "integrator plus dead time: the test pins down the rate kv = k/T but not k and T apart"
                .into()
        };
        Model::itd(pi.kv(), pi.l)
    } else {
        a.design_basis = match best.fit.order {
            Order::Second => {
                "second order, designed on its first-order equivalent (half rule)".into()
            }
            _ => "first order plus dead time".into(),
        };
        m
    };
    a.design_model = Some(design_model);
    a.ramp_model = (!m.integrating && best.fit.lag_identified).then(|| m.for_pi());
    if let Some((ku, tu)) = (Model {
        l: design_model.l + dt / 2.0,
        ..design_model
    })
    .ultimate()
    {
        a.ku = Some(ku);
        a.tu = Some(tu);
    }

    // Does the model reproduce what the oscillation measured directly?
    if let Some(h) = a.asymmetric.or(a.symmetric) {
        let g = m.freq(h.first.w);
        let measured = h.first;
        let ag = Agreement {
            gain_ratio: g.abs() / measured.gain,
            phase_diff_deg: wrap_deg(g.arg().to_degrees() - measured.phase_deg),
        };
        if (ag.gain_ratio - 1.0).abs() > 0.1 || ag.phase_diff_deg.abs() > 10.0 {
            warn(
                "disagree",
                format!(
                    "the model and the oscillation disagree at its frequency: gain ×{:.2}, phase {:+.0}°",
                    ag.gain_ratio, ag.phase_diff_deg
                ),
            );
        }
        a.agreement = Some(ag);
    }
    // A non-drifting bench process reads within about ±0.5 %/min over a 4-minute test (and a
    // little negative: the first cycles still settle), so only a clear trend is flagged.
    if let Some(d) = a.bias_drift_per_min
        && d.abs() > 1.5
    {
        warn(
            "drift",
            format!(
                "the holding power moved {d:+.1} % a minute during the test: the process was changing (beans drying?)"
            ),
        );
    }

    // The designs: SIMC at each stop, AMIGO, and the ultimate-point rules for comparison.
    for tc in TAU_C_STOPS {
        if let Some(d) = design_at(&a, Rule::Simc, tc) {
            if (tc - TAU_C_DEFAULT).abs() < 1e-9 {
                a.recommended = Some(a.designs.len());
            }
            a.designs.push(d);
        }
    }
    for rule in [Rule::Amigo, Rule::ZnPi, Rule::TyreusLuybenPi] {
        if let Some(d) = design_at(&a, rule, TAU_C_DEFAULT) {
            a.designs.push(d);
        }
    }

    // How far to trust it.
    let codes: Vec<&str> = warnings.iter().map(|w| w.code.as_str()).collect();
    let severe = codes
        .iter()
        .any(|c| matches!(*c, "failed" | "no_fit" | "no_relay"))
        || best.residual_ratio.is_some_and(|r| r > 3.0)
        || a.agreement
            .is_some_and(|g| (g.gain_ratio - 1.0).abs() > 0.3 || g.phase_diff_deg.abs() > 25.0);
    let soft = codes.iter().any(|c| {
        matches!(
            *c,
            "not_converged" | "poor_fit" | "holdout" | "disagree" | "drift"
        )
    });
    a.quality = if severe {
        Quality::Poor
    } else if soft {
        Quality::Acceptable
    } else {
        Quality::Good
    };
    a.warnings = warnings;
    a
}

fn period_guess(a: &Analysis) -> f64 {
    a.symmetric
        .map(|h| h.period)
        .or(a.asymmetric.map(|h| h.period))
        .unwrap_or(60.0)
}

fn fit_options(a: &Analysis, from: f64, to: f64, order: Order) -> FitOptions {
    FitOptions::around_period(period_guess(a), from, to, order)
}

/// The chosen fit's output over the relay part of the record, for plotting against the
/// measured one: `(t, y)`.
pub fn prediction(
    rec: Record,
    summary: &RelaySummary,
    a: &Analysis,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let from = summary.relay_start?;
    let to = rec.t.last().copied()?;
    let chosen = a.chosen?;
    let f = a.fits.iter().find(|f| f.fit.order == chosen)?;
    crate::fit::predict(rec, fit_options(a, from, to, chosen), &f.fit)
}

/// The recommended design, or SIMC at `tau_c` on the analysis's model.
pub fn design_at(a: &Analysis, rule: Rule, tau_c: f64) -> Option<Design> {
    let mut d = design(rule, a.design_model.as_ref()?, tau_c, &a.options)?;
    if let Some(m) = a.ramp_model {
        d.ramp_lag = Some(crate::robust::ramp_lag(&m, &d.pid, a.options.ramp_rate));
    }
    Some(d)
}

fn wrap_deg(d: f64) -> f64 {
    let mut x = (d + 180.0) % 360.0;
    if x < 0.0 {
        x += 360.0;
    }
    x - 180.0
}

/// The frequency point a model gives, for comparing with a measured one.
pub fn model_point(m: &Model, w: f64) -> FreqPoint {
    FreqPoint::from_complex(w, m.freq(w))
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::bench::{Bench, BenchConfig};
    use crate::relay::{Relay, RelayConfig};

    pub(crate) struct Run {
        pub(crate) t: Vec<f64>,
        pub(crate) u: Vec<f64>,
        pub(crate) y: Vec<f64>,
        pub(crate) summary: RelaySummary,
    }

    pub(crate) fn run(bench: BenchConfig, cfg: RelayConfig) -> Run {
        let mut b = Bench::new(bench, 0.0);
        let mut y = b.step(0.0);
        let mut t = 0.0;
        let mut r = Relay::start(cfg, t, y);
        let (mut ts, mut us, mut ys) = (Vec::new(), Vec::new(), Vec::new());
        while r.active() && t < 20_000.0 {
            let u = r.step(t, y);
            ts.push(t);
            us.push(u);
            ys.push(y);
            y = b.step(u);
            t += b.cfg.dt;
        }
        Run {
            t: ts,
            u: us,
            y: ys,
            summary: r.summary(),
        }
    }

    pub(crate) fn analyse_run(r: &Run) -> Analysis {
        analyse(
            Record {
                t: &r.t,
                u: &r.u,
                y: &r.y,
            },
            &r.summary,
            1.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use crate::bench::BenchConfig;
    use crate::design::Verdict;
    use crate::relay::RelayConfig;
    use crate::robust::evaluate;

    /// The real test of the whole chain: the recommended PI, designed from the experiment,
    /// is as robust on the TRUE process as the design says.
    fn check_on_truth(truth: Model, setpoint: f64, oversample: u32) -> Analysis {
        let mut bench = BenchConfig::roaster_like(truth);
        bench.oversample = oversample;
        let r = run(
            bench,
            RelayConfig {
                setpoint,
                ..RelayConfig::default()
            },
        );
        let a = analyse_run(&r);
        assert_ne!(a.quality, Quality::Poor, "{truth:?}: {:#?}", a.warnings);
        let rec = &a.designs[a.recommended.unwrap()];
        assert_eq!(rec.verdict, Verdict::Ok, "{rec:?}");
        let on_truth = evaluate(&truth, &rec.pid, None, 1.0).unwrap();
        assert!(
            (on_truth.ms - rec.robustness.ms).abs() < 0.2 && on_truth.ms < 1.75,
            "{truth:?}: designed Ms {:.2}, on the true process {:.2}; {a:#?}",
            rec.robustness.ms,
            on_truth.ms
        );
        a
    }

    #[test]
    fn roaster_like_two_lag_process() {
        // Roughly the virtual roaster: two lags, little dead time, heat-limited at 190 °C.
        let a = check_on_truth(Model::sotd(2.3, 60.0, 8.0, 2.0), 190.0, 4);
        assert!(a.agreement.is_some());
    }

    #[test]
    fn lag_dominant_first_order() {
        let a = check_on_truth(Model::fotd(2.9, 163.0, 7.5), 190.0, 4);
        // τ ≈ 0.04: designed on the integrator.
        assert!(a.design_model.unwrap().integrating, "{a:#?}");
        let tau = a.berner_tau.unwrap();
        assert!(tau < 0.2, "Berner's τ {tau}");
    }

    #[test]
    fn balanced_process() {
        let a = check_on_truth(Model::fotd(2.0, 40.0, 15.0), 160.0, 4);
        assert!(!a.design_model.unwrap().integrating, "{a:#?}");
    }

    #[test]
    fn raw_one_hertz_sensor_still_works() {
        check_on_truth(Model::fotd(2.9, 163.0, 7.5), 190.0, 1);
    }

    #[test]
    fn a_drifting_process_is_flagged() {
        let mut bench = BenchConfig::roaster_like(Model::sotd(2.3, 60.0, 8.0, 2.0));
        bench.oversample = 4;
        // The gain falls 5 % a minute (a bean load drying fast), plus the level creeps.
        bench.gain_drift_per_h = -3.0;
        bench.offset_drift_per_h = 30.0;
        let r = run(
            bench,
            RelayConfig {
                setpoint: 190.0,
                ..RelayConfig::default()
            },
        );
        let a = analyse_run(&r);
        assert!(
            a.warnings.iter().any(|w| w.code == "drift"),
            "{:#?} {:?}",
            a.warnings,
            a.bias_drift_per_min
        );
        assert_ne!(a.quality, Quality::Good);
    }

    /// The real roaster from a cold start: the drift and the fit are flagged,
    /// rightly, but the hold-out is not. With the main lag searched again on the shorter
    /// window it slid to a 6000 s lag traded against the drift term and missed the last
    /// cycle by 1.24 °C (a false "holdout").
    #[test]
    fn a_real_cold_start_run_flags_its_drift_but_not_the_holdout() {
        let d: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/real_72pct_cold_2026-10-27.json"))
                .unwrap();
        let v = |k: &str| -> Vec<f64> {
            d[k].as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect()
        };
        let (t, u, y) = (v("t"), v("u"), v("y"));
        let summary: RelaySummary = serde_json::from_value(d["summary"].clone()).unwrap();
        let a = analyse(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            &summary,
            1.0,
        );
        let codes: Vec<&str> = a.warnings.iter().map(|w| w.code.as_str()).collect();
        assert!(codes.contains(&"drift"), "{codes:?}");
        assert!(!codes.contains(&"holdout"), "{codes:?}");
        let best = a
            .fits
            .iter()
            .find(|f| Some(f.fit.order) == a.chosen)
            .unwrap();
        let h = best.holdout_rmse.unwrap();
        assert!(h < 0.6, "hold-out {h}");
        assert_eq!(a.quality, Quality::Acceptable);
        // The design rests on kv and the dead time, which a warm repeat agreed with within
        // a few percent (kv 0.0279, θ 6.1 s).
        let m = a.design_model.unwrap();
        assert!(
            (m.k - 0.0288).abs() < 0.002 && (m.l - 6.4).abs() < 0.5,
            "{m:?}"
        );
    }
}
