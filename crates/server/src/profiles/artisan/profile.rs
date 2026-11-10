//! A profile as an Artisan dict: its points interpolated without overshoot (PCHIP) at one
//! second, its events and landmarks in Artisan's slots, its background curves.

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use crate::profiles::types::{ProfileAction, ProfileWithPoints, TriggerKind};

use super::roast::{
    ETYPE_AIR, ETYPE_BURNER, artisan_event_value, computed_entries, nearest_index, roast_uuid,
    timeindex_slots,
};
use super::round1;

/// Monotone cubic interpolation (Fritsch–Carlson PCHIP): smooth like the
/// scope's spline, but never overshoots between points, so a profile's RoR
/// computed by Artisan has no ripples the points do not have.
struct Pchip {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
}

impl Pchip {
    fn new(points: &[(f64, f64)]) -> Option<Self> {
        let mut pts: Vec<(f64, f64)> = points
            .iter()
            .copied()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        // Same time twice: the later point wins.
        pts.dedup_by(|later, kept| {
            if (later.0 - kept.0).abs() < 1e-9 {
                kept.1 = later.1;
                true
            } else {
                false
            }
        });
        let n = pts.len();
        if n < 2 {
            return None;
        }
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let h: Vec<f64> = (0..n - 1).map(|i| xs[i + 1] - xs[i]).collect();
        let d: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / h[i]).collect();
        let mut ms = vec![0.0; n];
        if n == 2 {
            ms[0] = d[0];
            ms[1] = d[0];
        } else {
            for i in 1..n - 1 {
                if d[i - 1] * d[i] > 0.0 {
                    let w1 = 2.0 * h[i] + h[i - 1];
                    let w2 = h[i] + 2.0 * h[i - 1];
                    ms[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
                }
            }
            ms[0] = Self::end_slope(h[0], h[1], d[0], d[1]);
            ms[n - 1] = Self::end_slope(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
        }
        Some(Self { xs, ys, ms })
    }

    /// Three-point end slope, kept shape-preserving.
    fn end_slope(h0: f64, h1: f64, d0: f64, d1: f64) -> f64 {
        let m = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
        if m * d0 <= 0.0 {
            0.0
        } else if d0 * d1 <= 0.0 && m.abs() > (3.0 * d0).abs() {
            3.0 * d0
        } else {
            m
        }
    }

    fn first_x(&self) -> f64 {
        self.xs[0]
    }

    fn last_x(&self) -> f64 {
        self.xs[self.xs.len() - 1]
    }

    /// The curve at `t`, or `None` outside the points.
    fn at(&self, t: f64) -> Option<f64> {
        let n = self.xs.len();
        if t < self.xs[0] - 1e-9 || t > self.xs[n - 1] + 1e-9 {
            return None;
        }
        let i = self
            .xs
            .partition_point(|x| *x <= t)
            .saturating_sub(1)
            .min(n - 2);
        let h = self.xs[i + 1] - self.xs[i];
        let s = (t - self.xs[i]) / h;
        let (s2, s3) = (s * s, s * s * s);
        Some(
            (2.0 * s3 - 3.0 * s2 + 1.0) * self.ys[i]
                + (s3 - 2.0 * s2 + s) * h * self.ms[i]
                + (-2.0 * s3 + 3.0 * s2) * self.ys[i + 1]
                + (s3 - s2) * h * self.ms[i + 1],
        )
    }
}

/// Seconds between samples in an exported profile.
const PROFILE_EXPORT_STEP: f64 = 1.0;

/// A value from a recorded curve at `t` (nearest sample).
fn curve_at(arr: &[Option<f32>], interval: f64, t: f64) -> Option<f64> {
    if interval <= 0.0 || t < 0.0 {
        return None;
    }
    arr.get((t / interval).round() as usize)
        .copied()
        .flatten()
        .map(f64::from)
}

/// Index of the turning point: the BT minimum, if BT dips after CHARGE.
fn turning_point_index(temp2: &[f64]) -> Option<usize> {
    let search = temp2.len() / 2;
    let (idx, _) = temp2
        .iter()
        .take(search.max(1))
        .enumerate()
        .filter(|(_, v)| **v > -1.0)
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    (idx > 0).then_some(idx)
}

/// First sample at or above `bt` on the way up (after the turning point).
fn index_at_bt(temp2: &[f64], bt: f64) -> Option<usize> {
    let from = turning_point_index(temp2).unwrap_or(0);
    (from..temp2.len()).find(|&i| temp2[i] > -1.0 && temp2[i] >= bt)
}

/// Artisan `timeindex` slot (1..=7) for a Roastwarden landmark name.
fn landmark_slot(label: &str) -> Option<usize> {
    Some(match label {
        "drying_end" => 1,
        "first_crack_start" => 2,
        "first_crack_end" => 3,
        "second_crack_start" => 4,
        "second_crack_end" => 5,
        "drop" => 6,
        "cool_end" => 7,
        _ => return None,
    })
}

/// Build an Artisan `.alog` dict from a profile, loadable in Artisan as a
/// roast or as a background (Roast → Background) to replay its events.
///
/// - BT and ET: a monotone spline through the points every second from
///   CHARGE (t = 0); the recorded curves fill in where the points have no
///   values (no points at all, or no ET).
/// - Landmarks (`timeindex`): the profile's landmark events (by time or by
///   BT), else its first-crack target; DROP is the drop landmark, else the
///   target total time, else the last point.
/// - Fan and heater from the points and the profile's events become Air and
///   Burner special events (what Artisan replays from a background);
///   setpoint, mode and note events become labelled `--` events.
/// - Metadata: title, beans, batch size, roaster and description.
pub fn build_profile_alog(p: &ProfileWithPoints) -> Result<Value> {
    let mut points: Vec<_> = p.points.iter().collect();
    points.sort_by_key(|q| q.time_seconds);
    let bt_curve = Pchip::new(
        &points
            .iter()
            .map(|q| (f64::from(q.time_seconds), f64::from(q.target_temp)))
            .collect::<Vec<_>>(),
    );
    let et_curve = Pchip::new(
        &points
            .iter()
            .filter_map(|q| Some((f64::from(q.time_seconds), f64::from(q.target_env_temp?))))
            .collect::<Vec<_>>(),
    );
    let curves = p.curves.as_ref().filter(|c| !c.bt.is_empty());

    let end = match (&bt_curve, curves) {
        (Some(c), _) => c.last_x(),
        (None, Some(c)) => (c.bt.len() - 1) as f64 * c.interval_seconds,
        (None, None) => bail!("the profile has no points to export"),
    };
    let start = bt_curve.as_ref().map_or(0.0, |c| c.first_x().min(0.0));
    let n = ((end - start) / PROFILE_EXPORT_STEP).floor() as usize + 1;
    let timex: Vec<f64> = (0..n)
        .map(|i| round1(start + i as f64 * PROFILE_EXPORT_STEP))
        .collect();
    let sample = |pchip: &Option<Pchip>, rec: Option<&Vec<Option<f32>>>, t: f64| -> f64 {
        pchip
            .as_ref()
            .and_then(|c| c.at(t))
            .or_else(|| {
                let c = curves?;
                curve_at(rec?, c.interval_seconds, t)
            })
            .map_or(-1.0, round1)
    };
    let temp2: Vec<f64> = timex
        .iter()
        .map(|&t| sample(&bt_curve, curves.map(|c| &c.bt), t))
        .collect();
    let temp1: Vec<f64> = timex
        .iter()
        .map(|&t| sample(&et_curve, curves.map(|c| &c.et), t))
        .collect();

    // When an event fires: by time, or where the BT curve reaches its BT.
    let event_index = |trigger: TriggerKind, at_s: Option<f64>, at_bt: Option<f64>| {
        let by_time = at_s.and_then(|t| nearest_index(&timex, t));
        let by_bt = at_bt
            .and_then(|bt| index_at_bt(&temp2, bt))
            .map(|i| i as i64);
        match trigger {
            TriggerKind::Time => by_time.or(by_bt),
            TriggerKind::Bt => by_bt.or(by_time),
        }
    };

    // Landmarks
    let charge_idx = nearest_index(&timex, 0.0).unwrap_or(0);
    let mut slots: [Option<i64>; 7] = [None; 7];
    let mut tp_idx = turning_point_index(&temp2).map(|i| i as i64);
    for e in p
        .events
        .iter()
        .filter(|e| e.action == ProfileAction::Landmark)
    {
        let label = e.label.as_deref().unwrap_or("");
        let idx = event_index(e.trigger_kind, e.at_seconds, e.at_bt);
        if label == "turning_point" {
            tp_idx = idx.or(tp_idx);
        } else if let Some(slot) = landmark_slot(label) {
            slots[slot - 1] = idx.or(slots[slot - 1]);
        }
    }
    if slots[1].is_none() {
        slots[1] = p
            .profile
            .target_first_crack
            .and_then(|t| nearest_index(&timex, f64::from(t)));
    }
    let last = timex.len() as i64 - 1;
    if slots[5].is_none() {
        slots[5] = p
            .profile
            .target_total_time
            .and_then(|t| nearest_index(&timex, f64::from(t)))
            .or(Some(last));
    }
    // Artisan drops a slot that comes before the one ahead of it.
    let mut floor = charge_idx;
    for s in &mut slots {
        match *s {
            Some(i) if i > floor => floor = i,
            Some(_) => *s = None,
            None => {}
        }
    }
    let timeindex = timeindex_slots(Some(charge_idx), slots);

    // Special events
    let mut sp: Vec<(i64, i64, f64, String)> = Vec::new();
    let push_pct = |sp: &mut Vec<(i64, i64, f64, String)>, idx: i64, ty: i64, pct: f64| {
        let pct = pct.round().clamp(0.0, 100.0);
        let name = if ty == ETYPE_AIR { "Air" } else { "Burner" };
        sp.push((idx, ty, artisan_event_value(pct), format!("{name} {pct}%")));
    };
    type PointValue = fn(&crate::profiles::types::ProfilePoint) -> Option<i32>;
    let channels: [(i64, PointValue); 2] = [
        (ETYPE_AIR, |q| q.fan_speed),
        (ETYPE_BURNER, |q| q.heater_pct),
    ];
    for (ty, value) in channels {
        let mut last_pct: Option<i32> = None;
        for q in &points {
            let Some(v) = value(q) else { continue };
            if last_pct == Some(v) {
                continue;
            }
            last_pct = Some(v);
            if let Some(idx) = nearest_index(&timex, f64::from(q.time_seconds)) {
                push_pct(&mut sp, idx, ty, f64::from(v));
            }
        }
    }
    for e in &p.events {
        let Some(idx) = event_index(e.trigger_kind, e.at_seconds, e.at_bt) else {
            continue;
        };
        match (e.action, e.value) {
            (ProfileAction::Fan, Some(v)) => push_pct(&mut sp, idx, ETYPE_AIR, v),
            (ProfileAction::Heater, Some(v)) => push_pct(&mut sp, idx, ETYPE_BURNER, v),
            (ProfileAction::Setpoint, Some(v)) => {
                sp.push((idx, 4, 0.0, format!("Setpoint {}C", round1(v))));
            }
            (ProfileAction::Mode, _) => sp.push((
                idx,
                4,
                0.0,
                format!("Mode {}", e.label.as_deref().unwrap_or("")),
            )),
            (ProfileAction::Note, _) => sp.push((
                idx,
                4,
                0.0,
                e.label.clone().unwrap_or_else(|| "note".into()),
            )),
            _ => {}
        }
    }
    sp.sort_by_key(|e| e.0);

    // Landmark times and BT for `computed`
    let mut computed = Map::new();
    let t0 = timex[charge_idx as usize];
    let mut put = |code: &str, idx: Option<i64>| {
        if let Some(i) = idx.map(|i| i as usize)
            && let (Some(t), Some(bt)) = (timex.get(i), temp2.get(i).filter(|v| **v > -1.0))
        {
            computed_entries(&mut computed, code, t - t0, *bt);
        }
    };
    put("CHARGE", Some(charge_idx));
    put("TP", tp_idx);
    for (k, code) in ["DRY", "FCs", "FCe", "SCs", "SCe", "DROP"]
        .iter()
        .enumerate()
    {
        put(code, slots[k]);
    }
    if let Some(d) = slots[5] {
        computed.insert("totaltime".into(), json!(round1(timex[d as usize] - t0)));
    }

    let meta = &p.profile;
    let when = meta.updated_at;
    Ok(json!({
        "version": "2.10.0",
        "revision": "",
        "build": "",
        "mode": "C",
        "roastUUID": roast_uuid(&meta.id),
        "title": meta.name,
        "beans": meta.beans.clone().unwrap_or_default(),
        "weight": [meta.batch_size_g.map_or(0.0, f64::from), 0.0, "g"],
        "roastertype": meta.roaster.clone().unwrap_or_default(),
        "roastingnotes": meta.description.clone().unwrap_or_default(),
        "roastisodate": when.format("%Y-%m-%d").to_string(),
        "roasttime": when.format("%H:%M:%S").to_string(),
        "roastepoch": when.timestamp(),
        "roasttzoffset": 0,
        "samplinginterval": PROFILE_EXPORT_STEP,
        "timex": timex,
        "temp1": temp1,
        "temp2": temp2,
        "timeindex": timeindex,
        "etypes": ["Air", "Drum", "Damper", "Burner", "--"],
        "specialevents": sp.iter().map(|e| e.0).collect::<Vec<_>>(),
        "specialeventstype": sp.iter().map(|e| e.1).collect::<Vec<_>>(),
        "specialeventsvalue": sp.iter().map(|e| e.2).collect::<Vec<_>>(),
        "specialeventsStrings": sp.iter().map(|e| e.3.clone()).collect::<Vec<_>>(),
        "computed": computed,
    }))
}
