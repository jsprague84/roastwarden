//! Numbers for the charts a report design draws (`report.typ`'s `roast-chart`): axes with
//! round tick values, series thinned to what paper can show, a smoothed rate of rise. The
//! design only scales and strokes them.

use serde::Serialize;

/// A point: (x, y) in the axis' units
pub type Point = [f64; 2];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Tick {
    pub v: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Axis {
    pub min: f64,
    pub max: f64,
    pub ticks: Vec<Tick>,
}

/// A step of 1, 2, 2.5 or 5 × 10ⁿ that gives about `n` intervals over `span`
pub fn nice_step(span: f64, n: usize) -> f64 {
    let raw = (span / n.max(1) as f64).max(f64::MIN_POSITIVE);
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if raw <= m * mag * (1.0 + 1e-9) {
            return m * mag;
        }
    }
    10.0 * mag
}

/// An axis from `lo` to `hi` widened to whole steps, with about `n` ticks labelled by `label`
pub fn axis(lo: f64, hi: f64, n: usize, label: impl Fn(f64) -> String) -> Axis {
    let (lo, hi) = if hi > lo {
        (lo, hi)
    } else {
        (lo - 1.0, lo + 1.0)
    };
    let step = nice_step(hi - lo, n);
    let min = (lo / step).floor() * step;
    let max = (hi / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut i = 0;
    loop {
        let v = min + step * i as f64;
        if v > max + step * 1e-6 {
            break;
        }
        // Rounded so 0.30000000000000004 prints as 0.3
        let v = (v / step).round() * step;
        ticks.push(Tick { v, label: label(v) });
        i += 1;
    }
    Axis { min, max, ticks }
}

/// A number without trailing zeros: `25`, `2.5`
pub fn plain(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Largest-Triangle-Three-Buckets (Steinarsson 2013): at most `n` points that keep the shape.
pub fn lttb(points: &[Point], n: usize) -> Vec<Point> {
    if n >= points.len() || n < 3 {
        return points.to_vec();
    }
    let every = (points.len() - 2) as f64 / (n - 2) as f64;
    let mut out = Vec::with_capacity(n);
    let mut a = 0usize;
    out.push(points[0]);
    for i in 0..n - 2 {
        let start = (i as f64 * every) as usize + 1;
        let end = (((i + 1) as f64 * every) as usize + 1).min(points.len() - 1);
        // The next bucket's mean
        let n_start = end;
        let n_end = (((i + 2) as f64 * every) as usize + 1).min(points.len());
        let next = &points[n_start..n_end.max(n_start + 1).min(points.len())];
        let (mx, my) = next
            .iter()
            .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
        let k = next.len().max(1) as f64;
        let (mx, my) = (mx / k, my / k);
        let pa = points[a];
        let mut best = start;
        let mut best_area = -1.0;
        for (j, p) in points
            .iter()
            .enumerate()
            .take(end.max(start + 1))
            .skip(start)
        {
            let area = ((pa[0] - mx) * (p[1] - pa[1]) - (pa[0] - p[0]) * (my - pa[1])).abs();
            if area > best_area {
                best_area = area;
                best = j;
            }
        }
        out.push(points[best]);
        a = best;
    }
    out.push(*points.last().expect("more than n points"));
    out
}

/// The rate of rise per minute at each point: the least-squares slope over ±`half` seconds,
/// where the window holds at least five points.
pub fn ror(points: &[Point], half: f64) -> Vec<Point> {
    let mut out = Vec::with_capacity(points.len());
    let (mut lo, mut hi) = (0usize, 0usize);
    for (i, p) in points.iter().enumerate() {
        let t = p[0];
        while lo < i && points[lo][0] < t - half {
            lo += 1;
        }
        hi = hi.max(i);
        while hi + 1 < points.len() && points[hi + 1][0] <= t + half {
            hi += 1;
        }
        let w = &points[lo..=hi];
        if w.len() < 5 {
            continue;
        }
        let k = w.len() as f64;
        let mx = w.iter().map(|q| q[0]).sum::<f64>() / k;
        let my = w.iter().map(|q| q[1]).sum::<f64>() / k;
        let (mut num, mut den) = (0.0, 0.0);
        for q in w {
            num += (q[0] - mx) * (q[1] - my);
            den += (q[0] - mx).powi(2);
        }
        if den > 0.0 {
            out.push([t, num / den * 60.0]);
        }
    }
    out
}

/// The mean of the last `window` seconds at each point (a PID's heater output, smoothed)
pub fn trailing_mean(points: &[Point], window: f64) -> Vec<Point> {
    let mut out = Vec::with_capacity(points.len());
    let (mut lo, mut sum) = (0usize, 0.0);
    for (i, p) in points.iter().enumerate() {
        sum += p[1];
        while points[lo][0] < p[0] - window {
            sum -= points[lo][1];
            lo += 1;
        }
        out.push([p[0], sum / (i - lo + 1) as f64]);
    }
    out
}

/// Points at least `dt` apart (the first kept)
pub fn every(points: &[Point], dt: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_none_or(|q| p[0] - q[0] >= dt - 1e-9) {
            out.push(*p);
        }
    }
    out
}

/// The value `q` (0–1) of the way up the sorted values: a maximum one spike cannot set.
/// Values evenly spaced in time (from [`every`]) weigh each moment alike.
pub fn quantile(values: impl IntoIterator<Item = f64>, q: f64) -> Option<f64> {
    let mut v: Vec<f64> = values.into_iter().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let k = ((v.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
    Some(v[k])
}

/// Rounded for the data: time to 0.1 s, values to `digits`
pub fn rounded(points: &[Point], digits: i32) -> Vec<Point> {
    let f = 10f64.powi(digits);
    points
        .iter()
        .map(|p| [(p[0] * 10.0).round() / 10.0, (p[1] * f).round() / f])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_have_round_ticks() {
        let a = axis(0.0, 213.0, 5, plain);
        assert_eq!((a.min, a.max), (0.0, 250.0));
        assert_eq!(
            a.ticks.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(),
            ["0", "50", "100", "150", "200", "250"]
        );
        let b = axis(0.0, 0.9, 3, plain);
        assert_eq!(b.ticks.last().unwrap().label, "1");
        assert_eq!(nice_step(60.0, 6), 10.0);
        assert_eq!(nice_step(11.0, 5), 2.5);
        assert_eq!(plain(0.300_000_000_000_000_04), "0.3");
    }

    #[test]
    fn lttb_keeps_the_ends_and_the_peak() {
        let pts: Vec<Point> = (0..1000)
            .map(|i| [i as f64, if i == 500 { 100.0 } else { 0.0 }])
            .collect();
        let out = lttb(&pts, 50);
        assert_eq!(out.len(), 50);
        assert_eq!(out[0], [0.0, 0.0]);
        assert_eq!(out[49], [999.0, 0.0]);
        assert!(out.contains(&[500.0, 100.0]), "the spike survives");
        assert_eq!(lttb(&pts[..10], 50).len(), 10);
    }

    #[test]
    fn ror_of_a_straight_rise() {
        // 0.5 °C a second is 30 °C/min wherever there are five points
        let pts: Vec<Point> = (0..120)
            .map(|t| [t as f64, 20.0 + 0.5 * t as f64])
            .collect();
        let r = ror(&pts, 15.0);
        assert!(r.iter().all(|p| (p[1] - 30.0).abs() < 1e-9));
        assert_eq!(r.len(), 120);
    }

    #[test]
    fn trailing_mean_and_every() {
        let pts: Vec<Point> = (0..10)
            .map(|t| [t as f64, if t % 2 == 0 { 0.0 } else { 100.0 }])
            .collect();
        let m = trailing_mean(&pts, 3.0);
        assert_eq!(m[9][1], 50.0);
        assert_eq!(every(&pts, 2.0).len(), 5);
    }

    #[test]
    fn quantile_ignores_a_spike() {
        let mut v: Vec<f64> = (0..100).map(f64::from).collect();
        v.push(1000.0);
        assert_eq!(quantile(v.clone(), 0.95), Some(95.0));
        assert_eq!(quantile(v, 1.0), Some(1000.0));
        assert_eq!(quantile([], 0.5), None);
    }
}
