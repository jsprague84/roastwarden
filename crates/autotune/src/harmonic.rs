//! What the relay's oscillation says directly, without a model: the process's frequency
//! response at the oscillation's frequency (and twice it), from least-squares harmonics over
//! whole periods, and Berner's normalised dead time from the asymmetric half-periods.
//!
//! The harmonics replace the describing function and the peaks: each signal is regressed on
//! `[1, t, cos ωt, sin ωt, cos 2ωt, sin 2ωt]`, which takes out an offset and a linear drift,
//! and `G(iω) = Y(ω)/U(ω)`. The input is piecewise constant, so it is regressed at the middle
//! of each hold.

use serde::{Deserialize, Serialize};

use crate::fit::{least_squares, Record};
use crate::model::C64;

/// A point of the frequency response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FreqPoint {
    /// rad/s
    pub w: f64,
    /// |G|, output per input.
    pub gain: f64,
    /// arg G, degrees (−90 to −180 for a lag-dominant process near its oscillation).
    pub phase_deg: f64,
}

impl FreqPoint {
    pub fn from_complex(w: f64, g: C64) -> FreqPoint {
        FreqPoint {
            w,
            gain: g.abs(),
            phase_deg: g.arg().to_degrees(),
        }
    }

    pub fn complex(&self) -> C64 {
        C64::cis(self.phase_deg.to_radians()).scale(self.gain)
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Harmonics {
    pub from: f64,
    pub to: f64,
    /// Mean period over the window, s.
    pub period: f64,
    pub cycles: usize,
    /// At the oscillation's frequency.
    pub first: FreqPoint,
    /// At twice it (an asymmetric relay excites it; a symmetric one barely).
    pub second: Option<FreqPoint>,
    /// The output's first-harmonic amplitude (half the swing a sine would have).
    pub y_amplitude: f64,
    /// The output's drift over the window, per second.
    pub y_drift: f64,
}

/// Harmonics over `[from, from + cycles·period]`. `second` asks for the 2ω point too (only
/// meaningful when the input has a second harmonic: an asymmetric relay).
pub fn harmonics(
    rec: Record,
    from: f64,
    period: f64,
    cycles: usize,
    second: bool,
) -> Option<Harmonics> {
    if period <= 0.0 || cycles == 0 {
        return None;
    }
    let to = from + period * cycles as f64;
    let w = std::f64::consts::TAU / period;
    let n = rec.t.len();
    let mut ty = Vec::new();
    let mut ys = Vec::new();
    let mut tu = Vec::new();
    let mut us = Vec::new();
    for i in 0..n {
        let t = rec.t[i];
        if t >= from && t <= to && rec.y[i].is_finite() {
            ty.push(t);
            ys.push(rec.y[i]);
        }
        // The input holds from t[i] to t[i+1]: regress it at the middle of the hold.
        if i + 1 < n {
            let mid = f64::midpoint(t, rec.t[i + 1]);
            if mid >= from && mid <= to {
                tu.push(mid);
                us.push(rec.u[i]);
            }
        }
    }
    if ys.len() < 8 * cycles || us.len() < 8 * cycles {
        return None;
    }
    let fit = |ts: &[f64], v: &[f64]| -> Option<Vec<f64>> {
        let mut cols = vec![
            vec![1.0; ts.len()],
            ts.iter().map(|t| t - from).collect::<Vec<_>>(),
            ts.iter().map(|t| (w * (t - from)).cos()).collect(),
            ts.iter().map(|t| (w * (t - from)).sin()).collect(),
        ];
        if second {
            cols.push(ts.iter().map(|t| (2.0 * w * (t - from)).cos()).collect());
            cols.push(ts.iter().map(|t| (2.0 * w * (t - from)).sin()).collect());
        }
        least_squares(&cols, v)
    };
    let cy = fit(&ty, &ys)?;
    let cu = fit(&tu, &us)?;
    // a·cos + b·sin = Re[(a − ib)·e^{iωt}]
    let phasor = |c: &[f64], k: usize| C64::new(c[k], -c[k + 1]);
    let g1 = phasor(&cy, 2) / phasor(&cu, 2);
    let second_point = if second {
        let u2 = phasor(&cu, 4);
        (u2.abs() > 1e-6 * phasor(&cu, 2).abs().max(1e-12))
            .then(|| FreqPoint::from_complex(2.0 * w, phasor(&cy, 4) / u2))
    } else {
        None
    };
    Some(Harmonics {
        from,
        to,
        period,
        cycles,
        first: FreqPoint::from_complex(w, g1),
        second: second_point,
        y_amplitude: phasor(&cy, 2).abs(),
        y_drift: cy[1],
    })
}

/// Berner's normalised dead time `τ = L/(L + T)` from an asymmetric relay: `rho` the ratio of
/// the longer half-period to the shorter, `gamma` the ratio of the larger swing to the
/// smaller (as applied). A curve fit over Åström & Hägglund's 134-process batch, within 0.08
/// for γ = 2 (Berner, Hägglund & Åström, ACC 2016, eq. 12). Clamped to [0, 1].
pub fn berner_tau(rho: f64, gamma: f64) -> Option<f64> {
    if !(gamma > 1.0 && rho >= 1.0) {
        return None;
    }
    Some(((gamma - rho) / ((gamma - 1.0) * (0.35 * rho + 0.65))).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{simulate, Input, Model};

    #[test]
    fn harmonics_of_a_simulated_square_wave_response() {
        // FOTD driven by an asymmetric square wave: the harmonic estimate matches the model's
        // frequency response at ω and 2ω.
        let m = Model::fotd(2.0, 40.0, 4.0);
        let period = 30.0;
        let t: Vec<f64> = (0..600).map(|i| i as f64).collect();
        let u: Vec<f64> = t
            .iter()
            .map(|&x| if (x % period) < 20.0 { 80.0 } else { 50.0 })
            .collect();
        let y: Vec<f64> = simulate(&m, &Input { t: &t, u: &u }, &t, [130.0, 130.0], 0.02)
            .into_iter()
            .map(|v| v + 25.0)
            .collect();
        let h = harmonics(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            300.0,
            period,
            9,
            true,
        )
        .unwrap();
        let w = std::f64::consts::TAU / period;
        let want = m.freq(w);
        assert!(
            (h.first.gain / want.abs() - 1.0).abs() < 0.01,
            "{h:?} {}",
            want.abs()
        );
        assert!(
            (h.first.phase_deg - want.arg().to_degrees()).abs() < 1.0,
            "{h:?}"
        );
        let want2 = m.freq(2.0 * w);
        let s = h.second.unwrap();
        assert!(
            (s.gain / want2.abs() - 1.0).abs() < 0.03,
            "{s:?} {}",
            want2.abs()
        );
    }

    #[test]
    fn berner_tau_ends() {
        assert_eq!(berner_tau(1.0, 2.0), Some(1.0));
        assert_eq!(berner_tau(2.0, 2.0), Some(0.0));
        let mid = berner_tau(1.5, 2.0).unwrap();
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(berner_tau(1.5, 1.0), None);
    }
}
