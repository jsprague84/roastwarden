//! Fit a model to a recorded experiment: input `u` (the actuator, %), output `y` (the
//! measurement), at times `t`.
//!
//! The structures: an integrator plus dead time (`y' = kv·u(t − L)`), first order plus dead
//! time (`T·y' = −y + k·u(t − L)`), and the same through a second lag. The output is the
//! model's plus an offset, a linear drift (the process changing under the test, or for an
//! integrator the input that holds it level) and the response to a starting state. For fixed
//! lags and dead time all of those enter linearly, so they come from a least-squares solve and
//! only the lags and the dead time are searched (variable projection): a coarse grid, then
//! Nelder–Mead.
//!
//! Around one operating point a lag-dominant process shows mostly its rate `k/T`: the fit
//! profiles the cost over `T` and says whether `T` (and so `k`) is pinned down.

use serde::{Deserialize, Serialize};

use crate::model::{Input, Model, simulate};

/// A recorded experiment. All three slices have the same length; `t` ascends.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub t: &'a [f64],
    pub u: &'a [f64],
    pub y: &'a [f64],
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Integrator plus dead time.
    Integrating,
    /// First order plus dead time.
    First,
    /// Second order plus dead time.
    Second,
}

#[derive(Debug, Clone, Copy)]
pub struct FitOptions {
    /// Fit the output from this time to `to` (s). The input before `from` feeds the dead time.
    pub from: f64,
    pub to: f64,
    pub order: Order,
    /// Range searched for the main lag `t1`, s.
    pub t1_range: (f64, f64),
    /// Largest dead time searched, s.
    pub l_max: f64,
    /// Integration step, s.
    pub step_s: f64,
    /// Fit a linear drift of the output too.
    pub drift: bool,
}

impl FitOptions {
    /// Ranges sized from the oscillation period `tu` of a relay test.
    pub fn around_period(tu: f64, from: f64, to: f64, order: Order) -> FitOptions {
        FitOptions {
            from,
            to,
            order,
            t1_range: (tu / 20.0, tu * 100.0),
            l_max: tu / 2.0,
            step_s: (tu / 200.0).clamp(0.02, 0.5),
            drift: true,
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    pub model: Model,
    pub order: Order,
    /// Output offset (for a self-regulating model, at steady state `y = k·u + offset`).
    pub offset: f64,
    /// The fitted linear drift, output per second (0 when not fitted).
    pub drift: f64,
    /// Root-mean-square error of the fitted output.
    pub rmse: f64,
    /// `100·(1 − |y − ŷ| / |y − mean(y)|)`: 100 is perfect, 0 no better than the mean.
    pub fit_pct: f64,
    pub samples: usize,
    /// Akaike's information criterion, `N·ln(RSS/N) + 2p`: lower is better, comparable between
    /// fits of the same window.
    pub aic: f64,
    /// `k / t1` (or `k` for an integrator): output per input per second.
    pub kv: f64,
    /// The range of `t1` (s) the data cannot tell apart from the best (a 95 % profile
    /// interval, widened for correlated residuals); the upper end can be infinite. Equal to
    /// `t1` for an integrator.
    pub t1_low: f64,
    #[serde(with = "crate::nonfinite")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<f64>))]
    pub t1_high: f64,
    /// Whether `t1` (and so `k`) is known to within a factor of three either way. When not,
    /// design on an integrator with rate `kv`.
    pub lag_identified: bool,
}

/// A point of the nonlinear search, unpacked.
#[derive(Debug, Clone, Copy)]
struct Shape {
    t1: f64,
    t2: f64,
    l: f64,
}

struct Problem<'a> {
    rec: Record<'a>,
    opt: FitOptions,
    times: Vec<f64>,
    ys: Vec<f64>,
}

impl<'a> Problem<'a> {
    fn new(rec: Record<'a>, opt: FitOptions) -> Option<Problem<'a>> {
        let n = rec.t.len();
        if rec.u.len() != n || rec.y.len() != n {
            return None;
        }
        let idx: Vec<usize> = (0..n)
            .filter(|&i| rec.t[i] >= opt.from && rec.t[i] <= opt.to && rec.y[i].is_finite())
            .collect();
        if idx.len() < 20 {
            return None;
        }
        Some(Problem {
            rec,
            opt,
            times: idx.iter().map(|&i| rec.t[i]).collect(),
            ys: idx.iter().map(|&i| rec.y[i]).collect(),
        })
    }

    fn model(&self, s: Shape, k: f64) -> Model {
        match self.opt.order {
            Order::Integrating => Model::itd(k, s.l),
            Order::First => Model::fotd(k, s.t1, s.l),
            Order::Second => Model::sotd(k, s.t1, s.t2, s.l),
        }
    }

    /// The columns of the linear part: the unit model's response to the input, a constant,
    /// the starting state's response(s), and a drift.
    fn basis(&self, s: Shape) -> Vec<Vec<f64>> {
        let input = Input {
            t: self.rec.t,
            u: self.rec.u,
        };
        let step = self.opt.step_s;
        let t0 = self.times[0];
        let mut cols = vec![simulate(
            &self.model(s, 1.0),
            &input,
            &self.times,
            [0.0, 0.0],
            step,
        )];
        cols.push(vec![1.0; self.times.len()]);
        let none = Input {
            t: &self.times[..1],
            u: &[0.0],
        };
        match self.opt.order {
            Order::Integrating => {}
            Order::First => cols.push(
                self.times
                    .iter()
                    .map(|t| (-(t - t0) / s.t1).exp())
                    .collect(),
            ),
            Order::Second => {
                let free = self.model(s, 1.0);
                cols.push(simulate(&free, &none, &self.times, [0.0, 1.0], step));
                cols.push(simulate(&free, &none, &self.times, [1.0, 0.0], step));
            }
        }
        if self.opt.drift {
            cols.push(self.times.iter().map(|t| t - t0).collect());
        }
        cols
    }

    /// Least-squares coefficients over rows `rows` and the residual sum of squares there.
    fn solve(&self, cols: &[Vec<f64>], rows: std::ops::Range<usize>) -> Option<(Vec<f64>, f64)> {
        let sub: Vec<Vec<f64>> = cols.iter().map(|c| c[rows.clone()].to_vec()).collect();
        let ys = &self.ys[rows.clone()];
        let beta = least_squares(&sub, ys)?;
        let rss = ys
            .iter()
            .enumerate()
            .map(|(i, y)| (y - sub.iter().zip(&beta).map(|(c, w)| c[i] * w).sum::<f64>()).powi(2))
            .sum();
        Some((beta, rss))
    }

    fn unpack(&self, p: &[f64]) -> Shape {
        match self.opt.order {
            Order::Integrating => Shape {
                t1: 0.0,
                t2: 0.0,
                l: p[0],
            },
            Order::First => Shape {
                t1: p[0].exp(),
                t2: 0.0,
                l: p[1],
            },
            Order::Second => Shape {
                t1: p[0].exp(),
                t2: p[0].exp() * p[1].exp(),
                l: p[2],
            },
        }
    }

    fn feasible(&self, s: Shape) -> bool {
        let l_ok = (0.0..=self.opt.l_max * 1.5).contains(&s.l);
        let lag_ok = match self.opt.order {
            Order::Integrating => true,
            _ => {
                s.t1 >= self.opt.t1_range.0 * 0.5
                    && s.t1 <= self.opt.t1_range.1 * 2.0
                    && s.t2 <= s.t1
            }
        };
        l_ok && lag_ok
    }

    /// The cost over rows `rows` at search point `p`.
    fn cost(&self, p: &[f64], rows: std::ops::Range<usize>) -> f64 {
        let s = self.unpack(p);
        if !self.feasible(s) {
            return f64::INFINITY;
        }
        self.solve(&self.basis(s), rows)
            .map_or(f64::INFINITY, |(_, rss)| rss)
    }

    fn steps(&self, scale: f64) -> Vec<f64> {
        let l = self.opt.l_max / 8.0 * scale;
        match self.opt.order {
            Order::Integrating => vec![l],
            Order::First => vec![0.3 * scale, l],
            Order::Second => vec![0.3 * scale, 0.5 * scale, l],
        }
    }

    /// The best search point over rows `rows`: a grid, then Nelder–Mead twice.
    fn search(&self, rows: std::ops::Range<usize>) -> Option<Vec<f64>> {
        let (lo, hi) = self.opt.t1_range;
        let ls: Vec<f64> = (0..21).map(|i| self.opt.l_max * i as f64 / 20.0).collect();
        let t1s: Vec<f64> = (0..24)
            .map(|i| lo * (hi / lo).powf(i as f64 / 23.0))
            .collect();
        let mut grid: Vec<Vec<f64>> = Vec::new();
        match self.opt.order {
            Order::Integrating => grid.extend(ls.iter().map(|&l| vec![l])),
            Order::First => {
                for &t in &t1s {
                    grid.extend(ls.iter().map(|&l| vec![t.ln(), l]));
                }
            }
            Order::Second => {
                for &t in &t1s {
                    for r in [0.05f64, 0.12, 0.25, 0.5, 0.9] {
                        grid.extend(ls.iter().step_by(2).map(|&l| vec![t.ln(), r.ln(), l]));
                    }
                }
            }
        }
        let f = |p: &[f64]| self.cost(p, rows.clone());
        let p0 = grid
            .into_iter()
            .map(|p| (f(&p), p))
            .filter(|(c, _)| c.is_finite())
            .min_by(|a, b| a.0.total_cmp(&b.0))?
            .1;
        let p = nelder_mead(&f, &p0, &self.steps(1.0), 400);
        Some(nelder_mead(&f, &p, &self.steps(0.25), 400))
    }

    /// Best cost with `t1` held: the profile used for `t1`'s interval.
    fn profile(&self, t1: f64, start: &[f64]) -> f64 {
        let ln_t1 = t1.ln();
        let f = |q: &[f64]| {
            let mut p = vec![ln_t1];
            p.extend_from_slice(q);
            self.cost(&p, 0..self.ys.len())
        };
        let best = nelder_mead(&f, &start[1..], &self.steps(1.0)[1..], 200);
        f(&best)
    }
}

/// Fit `opt.order` to the record's window. `None` with too few samples or a degenerate record.
pub fn fit(rec: Record, opt: FitOptions) -> Option<Fit> {
    let pb = Problem::new(rec, opt)?;
    let n = pb.ys.len();
    let p = pb.search(0..n)?;
    let shape = pb.unpack(&p);
    let cols = pb.basis(shape);
    let (beta, rss) = pb.solve(&cols, 0..n)?;
    let model = pb.model(shape, beta[0]);
    let offset = beta[1];
    let drift = if opt.drift { *beta.last()? } else { 0.0 };
    let mean = pb.ys.iter().sum::<f64>() / n as f64;
    let tss: f64 = pb.ys.iter().map(|y| (y - mean).powi(2)).sum();
    let residuals: Vec<f64> = (0..n)
        .map(|i| pb.ys[i] - cols.iter().zip(&beta).map(|(c, w)| c[i] * w).sum::<f64>())
        .collect();
    let params = (cols.len() + p.len()) as f64;

    // t1's profile interval. Residuals of a short test are correlated (quantisation, model
    // error), so count samples per correlation time, not each one.
    let (mut t1_low, mut t1_high, mut lag_identified) = (model.t1, model.t1, true);
    if opt.order != Order::Integrating {
        let (lo, hi) = opt.t1_range;
        let n_eff = n as f64 / correlation_samples(&residuals).max(1.0);
        let dof = (n_eff - params).max(1.0);
        let limit = rss * (1.0 + 3.84 / dof);
        let within = |t: f64| pb.profile(t, &p) <= limit;
        let mut t = model.t1;
        while t > lo * 0.5 {
            t /= 1.25;
            if !within(t) {
                break;
            }
            t1_low = t;
        }
        let mut t = model.t1;
        loop {
            t *= 1.25;
            if t > hi * 2.0 {
                t1_high = f64::INFINITY;
                break;
            }
            if !within(t) {
                break;
            }
            t1_high = t;
        }
        lag_identified = t1_high <= 3.0 * model.t1 && t1_low >= model.t1 / 3.0;
    }

    Some(Fit {
        model,
        order: opt.order,
        offset,
        drift,
        rmse: (rss / n as f64).sqrt(),
        fit_pct: if tss > 0.0 {
            100.0 * (1.0 - (rss / tss).sqrt())
        } else {
            0.0
        },
        samples: n,
        aic: n as f64 * (rss / n as f64).max(1e-300).ln() + 2.0 * params,
        kv: model.kv(),
        t1_low,
        t1_high,
        lag_identified,
    })
    .filter(|f| f.model.is_valid())
}

/// Fit on the window up to `split` and say how well that model predicts the rest: the RMS
/// error over `(split, opt.to]`. The part after `split` should hold at least one cycle.
///
/// The main lag stays at the full fit's `t1`; the dead time, the second lag and the linear
/// terms are fitted again. On a lag-dominant process a shorter window cannot tell a long lag
/// from the drift term (on the real roaster, a 6000 s lag against 1.6 °C/s of
/// drift fitted the window and missed the next cycle by 1.2 °C), and the design does not use
/// that lag.
pub fn holdout_rmse(rec: Record, opt: FitOptions, split: f64, full: &Fit) -> Option<f64> {
    let pb = Problem::new(rec, opt)?;
    let train = pb.times.partition_point(|&t| t <= split);
    if train < 20 || train >= pb.ys.len() {
        return None;
    }
    let p = match opt.order {
        Order::Integrating => pb.search(0..train)?,
        Order::First | Order::Second => {
            let m = full.model;
            let ln_t1 = m.t1.ln();
            let start = match opt.order {
                Order::Second => vec![(m.t2 / m.t1).ln(), m.l],
                _ => vec![m.l],
            };
            let held = |q: &[f64]| [&[ln_t1], q].concat();
            let g = |q: &[f64]| pb.cost(&held(q), 0..train);
            let q = nelder_mead(&g, &start, &pb.steps(1.0)[1..], 400);
            held(&nelder_mead(&g, &q, &pb.steps(0.25)[1..], 400))
        }
    };
    let cols = pb.basis(pb.unpack(&p));
    let (beta, _) = pb.solve(&cols, 0..train)?;
    let n = pb.ys.len();
    let sq: f64 = (train..n)
        .map(|i| (pb.ys[i] - cols.iter().zip(&beta).map(|(c, w)| c[i] * w).sum::<f64>()).powi(2))
        .sum();
    Some((sq / (n - train) as f64).sqrt())
}

/// The model's output over the record's window with the fit's offset and drift (for plotting
/// the record against the model). The starting state is fitted again, linearly.
pub fn predict(rec: Record, opt: FitOptions, f: &Fit) -> Option<(Vec<f64>, Vec<f64>)> {
    let pb = Problem::new(
        rec,
        FitOptions {
            order: f.order,
            ..opt
        },
    )?;
    let shape = Shape {
        t1: f.model.t1,
        t2: f.model.t2,
        l: f.model.l,
    };
    let cols = pb.basis(shape);
    let n = pb.ys.len();
    let (beta, _) = pb.solve(&cols, 0..n)?;
    let y = (0..n)
        .map(|i| cols.iter().zip(&beta).map(|(c, w)| c[i] * w).sum())
        .collect();
    Some((pb.times.clone(), y))
}

/// Samples per correlation time of a residual series: the first lag where its
/// autocorrelation falls below 1/e.
fn correlation_samples(r: &[f64]) -> f64 {
    let n = r.len();
    let mean = r.iter().sum::<f64>() / n as f64;
    let var: f64 = r.iter().map(|x| (x - mean).powi(2)).sum();
    if var <= 0.0 {
        return 1.0;
    }
    for lag in 1..n / 2 {
        let c: f64 = (0..n - lag)
            .map(|i| (r[i] - mean) * (r[i + lag] - mean))
            .sum();
        if c / var < (-1.0f64).exp() {
            return lag as f64;
        }
    }
    (n / 2) as f64
}

/// Least squares by the normal equations on scaled columns (few, well-separated columns).
pub(crate) fn least_squares(cols: &[Vec<f64>], y: &[f64]) -> Option<Vec<f64>> {
    let p = cols.len();
    let scale: Vec<f64> = cols
        .iter()
        .map(|c| c.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-300))
        .collect();
    let mut a = vec![vec![0.0; p + 1]; p];
    for i in 0..p {
        for j in 0..p {
            a[i][j] = cols[i]
                .iter()
                .zip(&cols[j])
                .map(|(x, z)| x * z)
                .sum::<f64>()
                / (scale[i] * scale[j]);
        }
        a[i][i] += 1e-12;
        a[i][p] = cols[i].iter().zip(y).map(|(x, z)| x * z).sum::<f64>() / scale[i];
    }
    // Gaussian elimination with partial pivoting.
    for c in 0..p {
        let piv = (c..p).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        a.swap(c, piv);
        if a[c][c].abs() < 1e-14 {
            return None;
        }
        let pivot = a[c].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != c {
                let f = row[c] / pivot[c];
                for (x, y) in row[c..].iter_mut().zip(&pivot[c..]) {
                    *x -= f * y;
                }
            }
        }
    }
    Some((0..p).map(|i| a[i][p] / a[i][i] / scale[i]).collect())
}

/// Nelder–Mead minimisation from `x0` with initial step sizes `steps`.
pub(crate) fn nelder_mead(
    f: &dyn Fn(&[f64]) -> f64,
    x0: &[f64],
    steps: &[f64],
    iters: usize,
) -> Vec<f64> {
    let n = x0.len();
    let mut pts: Vec<Vec<f64>> = vec![x0.to_vec()];
    for i in 0..n {
        let mut x = x0.to_vec();
        x[i] += steps[i];
        pts.push(x);
    }
    let mut vals: Vec<f64> = pts.iter().map(|x| f(x)).collect();
    for _ in 0..iters {
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|&a, &b| vals[a].total_cmp(&vals[b]));
        pts = order.iter().map(|&i| pts[i].clone()).collect();
        vals = order.iter().map(|&i| vals[i]).collect();
        if (vals[n] - vals[0]).abs() <= 1e-12 * vals[0].abs().max(1e-300) {
            break;
        }
        let centroid: Vec<f64> = (0..n)
            .map(|j| pts[..n].iter().map(|p| p[j]).sum::<f64>() / n as f64)
            .collect();
        let towards = |t: f64| -> Vec<f64> {
            (0..n)
                .map(|j| centroid[j] + t * (pts[n][j] - centroid[j]))
                .collect()
        };
        let xr = towards(-1.0);
        let fr = f(&xr);
        if fr < vals[0] {
            let xe = towards(-2.0);
            let fe = f(&xe);
            if fe < fr {
                pts[n] = xe;
                vals[n] = fe;
            } else {
                pts[n] = xr;
                vals[n] = fr;
            }
        } else if fr < vals[n - 1] {
            pts[n] = xr;
            vals[n] = fr;
        } else {
            let xc = if fr < vals[n] {
                towards(-0.5)
            } else {
                towards(0.5)
            };
            let fc = f(&xc);
            if fc < vals[n].min(fr) {
                pts[n] = xc;
                vals[n] = fc;
            } else {
                for i in 1..=n {
                    pts[i] = (0..n)
                        .map(|j| pts[0][j] + 0.5 * (pts[i][j] - pts[0][j]))
                        .collect();
                    vals[i] = f(&pts[i]);
                }
            }
        }
    }
    let best = (0..=n)
        .min_by(|&a, &b| vals[a].total_cmp(&vals[b]))
        .unwrap_or(0);
    pts[best].clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::simulate;

    /// A made-up record: `model` driven by `u` from steady state, plus `offset`, quantised to
    /// `q`.
    fn record(model: &Model, offset: f64, u: &[f64], q: f64) -> (Vec<f64>, Vec<f64>) {
        let t: Vec<f64> = (0..u.len()).map(|i| i as f64).collect();
        let x = if model.integrating {
            0.0
        } else {
            model.k * u[0]
        };
        let y = simulate(model, &Input { t: &t, u }, &t, [x, x], 0.05)
            .into_iter()
            .map(|v| v + offset)
            .map(|v| if q > 0.0 { (v / q).round() * q } else { v })
            .collect();
        (t, y)
    }

    fn steps_input() -> Vec<f64> {
        (0..900)
            .map(|i| {
                if i < 100 {
                    50.0
                } else if i < 500 {
                    70.0
                } else {
                    40.0
                }
            })
            .collect()
    }

    fn opts(order: Order) -> FitOptions {
        FitOptions {
            drift: false,
            ..FitOptions::around_period(40.0, 0.0, 899.0, order)
        }
    }

    #[test]
    fn a_step_test_identifies_a_first_order_process() {
        let m = Model::fotd(1.6, 70.0, 6.0);
        let u = steps_input();
        let (t, y) = record(&m, 30.0, &u, 0.0);
        let f = fit(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            opts(Order::First),
        )
        .unwrap();
        assert!((f.model.k - 1.6).abs() < 0.01, "{f:?}");
        assert!((f.model.t1 - 70.0).abs() < 1.0, "{f:?}");
        assert!((f.model.l - 6.0).abs() < 0.3, "{f:?}");
        assert!(f.lag_identified, "{f:?}");
        assert!(f.fit_pct > 99.0, "{f:?}");
        // With a drift term the answer is the same (the record has none).
        let g = fit(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            FitOptions::around_period(40.0, 0.0, 899.0, Order::First),
        )
        .unwrap();
        assert!(
            (g.model.k - 1.6).abs() < 0.02 && g.drift.abs() < 1e-4,
            "{g:?}"
        );
    }

    #[test]
    fn quantised_readings_still_fit() {
        let m = Model::fotd(1.6, 70.0, 6.0);
        let u = steps_input();
        let (t, y) = record(&m, 30.0, &u, 0.25);
        let f = fit(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            opts(Order::First),
        )
        .unwrap();
        assert!((f.model.k - 1.6).abs() < 0.03, "{f:?}");
        assert!((f.model.t1 - 70.0).abs() < 3.0, "{f:?}");
        assert!((f.model.l - 6.0).abs() < 1.0, "{f:?}");
    }

    #[test]
    fn a_second_order_process_fits_second_order_best() {
        let m = Model::sotd(1.2, 60.0, 8.0, 1.0);
        let u = (0..900)
            .map(|i| {
                if i < 100 {
                    50.0
                } else if i < 500 {
                    75.0
                } else {
                    45.0
                }
            })
            .collect::<Vec<_>>();
        let (t, y) = record(&m, 20.0, &u, 0.0);
        let rec = Record {
            t: &t,
            u: &u,
            y: &y,
        };
        let f2 = fit(rec, opts(Order::Second)).unwrap();
        assert!((f2.model.k - 1.2).abs() < 0.02, "{f2:?}");
        assert!((f2.model.t1 - 60.0).abs() < 2.0, "{f2:?}");
        assert!((f2.model.t2 - 8.0).abs() < 1.5, "{f2:?}");
        let f1 = fit(rec, opts(Order::First)).unwrap();
        assert!(f2.rmse < f1.rmse / 3.0, "{f1:?} {f2:?}");
        assert!(f2.aic < f1.aic);
        // First order stands in with dead time for the second lag.
        assert!(f1.model.l > 4.0 && f1.model.l < 12.0, "{f1:?}");
    }

    #[test]
    fn an_integrator_is_fitted_with_its_level_input_as_drift() {
        let m = Model::itd(0.02, 5.0);
        // Around 60 %, which holds it level: the output moves by kv·(u − 60).
        let u: Vec<f64> = (0..600)
            .map(|i| if (i / 40) % 2 == 0 { 70.0 } else { 50.0 })
            .collect();
        let t: Vec<f64> = (0..600).map(|i| i as f64).collect();
        let dev: Vec<f64> = u.iter().map(|v| v - 60.0).collect();
        let y: Vec<f64> = simulate(&m, &Input { t: &t, u: &dev }, &t, [0.0, 0.0], 0.05)
            .into_iter()
            .map(|v| v + 150.0)
            .collect();
        let f = fit(
            Record {
                t: &t,
                u: &u,
                y: &y,
            },
            FitOptions::around_period(80.0, 0.0, 599.0, Order::Integrating),
        )
        .unwrap();
        assert!((f.model.k - 0.02).abs() < 2e-4, "{f:?}");
        assert!((f.model.l - 5.0).abs() < 0.2, "{f:?}");
        // The level input: drift = −kv·60.
        assert!((f.drift + 1.2).abs() < 0.02, "{f:?}");
    }

    #[test]
    fn the_holdout_predicts_the_last_part() {
        let m = Model::fotd(1.6, 70.0, 6.0);
        let u = steps_input();
        let (t, y) = record(&m, 30.0, &u, 0.0);
        let rec = Record {
            t: &t,
            u: &u,
            y: &y,
        };
        let f = fit(rec, opts(Order::First)).unwrap();
        let h = holdout_rmse(rec, opts(Order::First), 600.0, &f).unwrap();
        assert!(h < 0.05, "{h}");
        let (tp, yp) = predict(rec, opts(Order::First), &f).unwrap();
        assert_eq!(tp.len(), yp.len());
        assert!((yp[700] - y[700]).abs() < 0.05);
    }
}
