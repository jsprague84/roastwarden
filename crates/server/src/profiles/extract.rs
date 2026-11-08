//! Turn a recorded roast into a profile: feedforward points, replayable
//! events and 1 Hz background curves.
//!
//! Used by "save session as profile" and by the Artisan profile import, so a
//! profile made either way has the same shape. Everything is rebased so that
//! CHARGE is `t = 0` (session start when no CHARGE was marked) and the
//! profile ends at DROP (or the last sample).
//!
//! Points come from a Ramer-Douglas-Peucker pass over smoothed BT, ET,
//! heater and fan curves at once: a sample is kept when dropping it would
//! move any channel by more than its tolerance. Landmarks always get a point.
//! Events are the CHARGE-relative landmarks plus fan and heater *steps* (a
//! new value that holds for a few seconds). A PID-driven heater changes every
//! second and produces no steps; its shape lives in the points instead.

use crate::profiles::types::{
    CreateProfileEventRequest, CreateProfilePointRequest, ProfileAction, ProfileCurves, TriggerKind,
};

use crate::roasts::types::RoastEventType;

/// One recorded sample. `t` is seconds from session start.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RecordedSample {
    pub t: f64,
    pub bt: Option<f32>,
    pub et: Option<f32>,
    /// °C/min.
    pub ror: Option<f32>,
    pub heater_pct: Option<f32>,
    pub fan_pct: Option<f32>,
}

/// A marked landmark. `t` is seconds from session start.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedLandmark {
    pub kind: RoastEventType,
    pub t: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedProfile {
    pub points: Vec<CreateProfilePointRequest>,
    pub events: Vec<CreateProfileEventRequest>,
    pub curves: ProfileCurves,
    pub total_time: i32,
    pub first_crack: Option<i32>,
    pub charge_temp: Option<f32>,
    pub end_temp: Option<f32>,
}

/// Tolerances for keeping a point.
const TOL_BT: f64 = 1.0; // °C
const TOL_ET: f64 = 3.0; // °C
const TOL_HEATER: f64 = 6.0; // %
const TOL_FAN: f64 = 4.0; // %
/// Longest gap between points, so heater/fan feedforward keeps some detail.
const MAX_POINT_GAP: usize = 120;
/// Samples further apart than this are a gap, not something to interpolate.
const MAX_INTERP_GAP: f64 = 10.0;
/// More steps than this on one channel means continuous control, not steps.
const MAX_STEPS_PER_CHANNEL: usize = 40;

/// Extract a profile, or `None` when fewer than two BT samples fall between
/// CHARGE and DROP.
pub fn extract(
    samples: &[RecordedSample],
    landmarks: &[RecordedLandmark],
) -> Option<ExtractedProfile> {
    let mut samples: Vec<RecordedSample> = samples
        .iter()
        .copied()
        .filter(|s| s.t.is_finite())
        .collect();
    samples.sort_by(|a, b| a.t.total_cmp(&b.t));

    let mark = |k: RoastEventType| landmarks.iter().find(|l| l.kind == k).map(|l| l.t);
    let charge = mark(RoastEventType::Charge).unwrap_or(0.0);
    let last_t = samples.last()?.t;
    let drop = mark(RoastEventType::Drop)
        .filter(|d| *d > charge)
        .unwrap_or(last_t)
        .min(last_t);

    let window: Vec<RecordedSample> = samples
        .into_iter()
        .filter(|s| s.t >= charge - 0.5 && s.t <= drop + 0.5)
        .map(|s| RecordedSample {
            t: (s.t - charge).max(0.0),
            ..s
        })
        .collect();
    if window.iter().filter(|s| s.bt.is_some()).count() < 2 {
        return None;
    }

    let end = (drop - charge).max(1.0).round() as usize;
    let len = (end + 1).min(crate::profiles::types::MAX_CURVE_LEN);
    let bt = resample(&window, len, |s| s.bt, Interp::Linear);
    let et = resample(&window, len, |s| s.et, Interp::Linear);
    let ror = resample(&window, len, |s| s.ror, Interp::Linear);
    let heater = resample(&window, len, |s| s.heater_pct, Interp::Hold);
    let fan = resample(&window, len, |s| s.fan_pct, Interp::Hold);

    let bt_s = smooth(&bt, 2);
    let et_s = smooth(&et, 2);
    let heater_s = smooth(&heater, 7);

    // Landmarks inside the profile, CHARGE-relative.
    let marks: Vec<(RoastEventType, usize)> = landmarks
        .iter()
        .filter(|l| l.kind != RoastEventType::Custom)
        .filter_map(|l| {
            let t = l.t - charge;
            (t >= 0.0 && t <= (len - 1) as f64).then(|| (l.kind.clone(), t.round() as usize))
        })
        .collect();

    // Points: RDP over all channels, plus landmarks.
    let channels: [(&[Option<f32>], f64); 4] = [
        (&bt_s, TOL_BT),
        (&et_s, TOL_ET),
        (&heater_s, TOL_HEATER),
        (&fan, TOL_FAN),
    ];
    let mut keep = vec![false; len];
    let valid: Vec<usize> = (0..len).filter(|&i| bt_s[i].is_some()).collect();
    if let (Some(&first), Some(&last)) = (valid.first(), valid.last()) {
        keep[first] = true;
        keep[last] = true;
        rdp(&valid, &channels, &mut keep);
    }
    for (_, i) in &marks {
        if bt_s[*i].is_some() {
            keep[*i] = true;
        }
    }
    cap_gaps(&mut keep, &bt_s);

    let round1 = |v: f32| (v * 10.0).round() / 10.0;
    let mut points: Vec<CreateProfilePointRequest> = (0..len)
        .filter(|&i| keep[i])
        .filter_map(|i| {
            Some(CreateProfilePointRequest {
                time_seconds: i as i32,
                target_temp: round1(bt_s[i]?),
                fan_speed: fan[i].map(|v| v.round().clamp(0.0, 100.0) as i32),
                notes: None,
                target_env_temp: et_s[i].map(round1),
                heater_pct: heater_s[i].map(|v| v.round().clamp(0.0, 100.0) as i32),
            })
        })
        .collect();
    for (kind, i) in &marks {
        if let Some(p) = points.iter_mut().find(|p| p.time_seconds as usize == *i) {
            let name = landmark_name(kind);
            p.notes = Some(match p.notes.take() {
                Some(n) => format!("{n}, {name}"),
                None => name.to_string(),
            });
        }
    }

    // Events: landmarks (CHARGE is t = 0 by definition), then control steps.
    let bt_at = |i: usize| bt_s.get(i).copied().flatten().map(|v| f64::from(round1(v)));
    let mut events: Vec<CreateProfileEventRequest> = marks
        .iter()
        .filter(|(k, _)| *k != RoastEventType::Charge)
        .map(|(k, i)| CreateProfileEventRequest {
            trigger_kind: Some(TriggerKind::Time),
            at_seconds: Some(*i as f64),
            at_bt: bt_at(*i),
            action: Some(ProfileAction::Landmark),
            value: None,
            label: Some(k.to_string()),
        })
        .collect();
    for (series, action, min_step, hold) in [
        (&fan, ProfileAction::Fan, 2.0, 3),
        (&heater, ProfileAction::Heater, 5.0, 10),
    ] {
        let steps = steps(series, min_step, hold);
        if steps.len() <= MAX_STEPS_PER_CHANNEL {
            events.extend(steps.into_iter().map(|(i, v)| CreateProfileEventRequest {
                trigger_kind: Some(TriggerKind::Time),
                at_seconds: Some(i as f64),
                at_bt: bt_at(i),
                action: Some(action),
                value: Some(f64::from(v.round())),
                label: None,
            }));
        }
    }
    events.sort_by(|a, b| {
        a.at_seconds
            .unwrap_or(0.0)
            .total_cmp(&b.at_seconds.unwrap_or(0.0))
    });

    points.dedup_by_key(|p| p.time_seconds);
    let first_crack = marks
        .iter()
        .find(|(k, _)| *k == RoastEventType::FirstCrackStart)
        .map(|(_, i)| *i as i32);
    let charge_temp = bt.first().copied().flatten().map(round1);
    let end_temp = bt.iter().rev().find_map(|v| *v).map(round1);
    let r2 = |v: Vec<Option<f32>>| -> Vec<Option<f32>> {
        v.into_iter()
            .map(|x| x.map(|y| (y * 100.0).round() / 100.0))
            .collect()
    };
    let nonempty = |v: Vec<Option<f32>>| {
        if v.iter().all(Option::is_none) {
            Vec::new()
        } else {
            v
        }
    };

    Some(ExtractedProfile {
        points,
        events,
        curves: ProfileCurves {
            source_session_id: None,
            interval_seconds: 1.0,
            bt: r2(bt),
            et: nonempty(r2(et)),
            ror: nonempty(r2(ror)),
            heater: nonempty(r2(heater)),
            fan: nonempty(r2(fan)),
        },
        total_time: (len - 1) as i32,
        first_crack,
        charge_temp,
        end_temp,
    })
}

fn landmark_name(kind: &RoastEventType) -> &'static str {
    match kind {
        RoastEventType::Charge => "Charge",
        RoastEventType::TurningPoint => "Turning Point",
        RoastEventType::DryingEnd => "Dry End",
        RoastEventType::FirstCrackStart => "First Crack Start",
        RoastEventType::FirstCrackEnd => "First Crack End",
        RoastEventType::SecondCrackStart => "Second Crack Start",
        RoastEventType::SecondCrackEnd => "Second Crack End",
        RoastEventType::DevelopmentStart => "Development Start",
        RoastEventType::Drop => "Drop",
        RoastEventType::CoolEnd => "Cool End",
        RoastEventType::Custom => "Event",
    }
}

#[derive(Clone, Copy)]
enum Interp {
    Linear,
    /// Previous value (control outputs are steps).
    Hold,
}

/// Resample onto whole seconds `0..len`, with `None` across gaps.
fn resample(
    samples: &[RecordedSample],
    len: usize,
    get: impl Fn(&RecordedSample) -> Option<f32>,
    interp: Interp,
) -> Vec<Option<f32>> {
    let pts: Vec<(f64, f32)> = samples
        .iter()
        .filter_map(|s| get(s).filter(|v| v.is_finite()).map(|v| (s.t, v)))
        .collect();
    let mut out = vec![None; len];
    if pts.is_empty() {
        return out;
    }
    let mut j = 0;
    for (i, slot) in out.iter_mut().enumerate() {
        let x = i as f64;
        while j + 1 < pts.len() && pts[j + 1].0 <= x {
            j += 1;
        }
        let (t0, v0) = pts[j];
        if x < t0 - 0.5 {
            continue; // before the first sample
        }
        match pts.get(j + 1) {
            Some(&(t1, v1)) if x >= t0 => {
                if t1 - t0 > MAX_INTERP_GAP {
                    if x - t0 <= 1.0 {
                        *slot = Some(v0);
                    }
                    continue;
                }
                *slot = Some(match interp {
                    Interp::Linear => v0 + (v1 - v0) * ((x - t0) / (t1 - t0)) as f32,
                    Interp::Hold => v0,
                });
            }
            _ => {
                if (x - t0).abs() <= 1.0 {
                    *slot = Some(v0);
                }
            }
        }
    }
    out
}

/// Centered moving average with half-width `k`, skipping gaps. The window
/// shrinks symmetrically at the ends so the first and last values stay put.
fn smooth(v: &[Option<f32>], k: usize) -> Vec<Option<f32>> {
    (0..v.len())
        .map(|i| {
            v[i]?;
            let k = k.min(i).min(v.len() - 1 - i);
            let lo = i - k;
            let hi = i + k;
            let (sum, n) = v[lo..=hi]
                .iter()
                .flatten()
                .fold((0.0f64, 0usize), |(s, n), x| (s + f64::from(*x), n + 1));
            Some((sum / n as f64) as f32)
        })
        .collect()
}

/// Iterative RDP on index ranges of `valid`, with the error normalised per
/// channel so one pass covers all of them.
fn rdp(valid: &[usize], channels: &[(&[Option<f32>], f64)], keep: &mut [bool]) {
    let mut stack = vec![(0usize, valid.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (ia, ib) = (valid[a], valid[b]);
        let mut worst = (0.0f64, a);
        for (k, &i) in valid.iter().enumerate().take(b).skip(a + 1) {
            let f = (i - ia) as f64 / (ib - ia) as f64;
            let mut err = 0.0f64;
            for (series, tol) in channels {
                if let (Some(va), Some(vb), Some(vi)) = (series[ia], series[ib], series[i]) {
                    let line = f64::from(va) + (f64::from(vb) - f64::from(va)) * f;
                    err = err.max((f64::from(vi) - line).abs() / tol);
                }
            }
            if err > worst.0 {
                worst = (err, k);
            }
        }
        if worst.0 > 1.0 {
            keep[valid[worst.1]] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
}

/// Add points so no two kept points are more than `MAX_POINT_GAP` apart.
fn cap_gaps(keep: &mut [bool], bt: &[Option<f32>]) {
    let mut last: Option<usize> = None;
    let mut i = 0;
    while i < keep.len() {
        if keep[i] {
            if let Some(l) = last {
                let mut at = l + MAX_POINT_GAP;
                while at < i {
                    if bt[at].is_some() {
                        keep[at] = true;
                    }
                    at += MAX_POINT_GAP;
                }
            }
            last = Some(i);
        }
        i += 1;
    }
}

/// Step changes: a value that differs from the last step by at least
/// `min_step` and holds (within ±1) for `hold` samples. The first value
/// that holds is a step at its index.
fn steps(v: &[Option<f32>], min_step: f32, hold: usize) -> Vec<(usize, f32)> {
    let mut out: Vec<(usize, f32)> = Vec::new();
    let mut last: Option<f32> = None;
    let mut i = 0;
    while i < v.len() {
        let Some(x) = v[i] else {
            i += 1;
            continue;
        };
        let differs = last.is_none_or(|l| (x - l).abs() >= min_step);
        let holds = i + hold <= v.len()
            && (i..i + hold).all(|j| v[j].is_some_and(|y| (y - x).abs() <= 1.0));
        if differs && holds {
            out.push((i, x));
            last = Some(x);
            i += hold.max(1);
            continue;
        }
        i += 1;
    }
    out
}

use anyhow::Result;
use sqlx::SqlitePool;

use crate::profiles::types::{
    CreateProfileRequest, ProfileWithPoints, SaveSessionAsProfileRequest,
};
use crate::profiles::validate::{is_roaster_kind, ProfileInputError, ROAST_LEVELS, ROAST_PURPOSES};

/// Save a recorded session as a new profile: feedforward points, events
/// and background curves, all CHARGE-relative. `Ok(None)` when the
/// session does not exist.
pub async fn save_session_as_profile(
    db: &SqlitePool,
    session_id: &str,
    req: SaveSessionAsProfileRequest,
) -> Result<Option<ProfileWithPoints>> {
    let Some(session) = crate::roasts::sessions::get_session(db, session_id).await? else {
        return Ok(None);
    };
    let telemetry = crate::roasts::telemetry_store::get_session_telemetry(db, session_id).await?;
    let events = crate::roasts::events::get_roast_events(db, session_id).await?;
    let samples: Vec<crate::profiles::extract::RecordedSample> = telemetry
        .iter()
        .map(|t| crate::profiles::extract::RecordedSample {
            t: f64::from(t.elapsed_seconds),
            bt: t.bean_temp,
            et: t.env_temp,
            ror: t.rate_of_rise,
            heater_pct: t.heater.map(|v| v as f32),
            fan_pct: t.fan.map(|v| v as f32),
        })
        .collect();
    let landmarks: Vec<crate::profiles::extract::RecordedLandmark> = events
        .iter()
        .map(|e| crate::profiles::extract::RecordedLandmark {
            kind: e.event_type.clone(),
            t: f64::from(e.elapsed_seconds),
        })
        .collect();
    let extracted = crate::profiles::extract::extract(&samples, &landmarks).ok_or_else(|| {
        ProfileInputError("session has too little bean-temperature data for a profile".into())
    })?;

    let name = req
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("{} (from session)", session.name));
    let description = req.description.or_else(|| {
        let date = session.start_time.unwrap_or(session.created_at);
        Some(format!(
            "Saved from session {} on {}",
            session.name,
            date.format("%Y-%m-%d")
        ))
    });
    let mut curves = extracted.curves;
    curves.source_session_id = Some(session.id.clone());
    // A profile from a roast is for the kind of roaster it was roasted on.
    let roaster_kind: Option<String> =
        if crate::sim::virtual_roaster::is_simulated(&session.device_id) {
            Some("fluid_bed_electric".into())
        } else {
            sqlx::query_scalar("SELECT kind FROM roaster_setup WHERE device_id = ?")
                .bind(&session.device_id)
                .fetch_optional(db)
                .await?
        };
    let beans = match (&session.bean_origin, &session.bean_variety) {
        (Some(o), Some(v)) if !v.trim().is_empty() => Some(format!("{o} {v}")),
        (Some(o), _) => Some(o.clone()),
        (None, v) => v.clone(),
    };
    let created = crate::profiles::store::create_profile(
        db,
        CreateProfileRequest {
            name,
            beans,
            batch_size_g: session.green_weight,
            roaster: Some(session.device_id.clone()),
            roast_level: Some(
                session
                    .target_roast_level
                    .clone()
                    .filter(|l| ROAST_LEVELS.contains(&l.as_str())),
            ),
            roast_purpose: Some(
                session
                    .roast_purpose
                    .clone()
                    .filter(|p| ROAST_PURPOSES.contains(&p.as_str())),
            ),
            expectations: None,
            auto_drop: None,
            roaster_kind: Some(roaster_kind.filter(|k| is_roaster_kind(k))),
            description,
            target_total_time: Some(extracted.total_time),
            target_first_crack: extracted.first_crack,
            target_end_temp: extracted.end_temp,
            preheat_temp: None,
            charge_temp: extracted.charge_temp,
            points: extracted.points,
            events: Some(extracted.events),
            curves: Some(Some(curves)),
        },
    )
    .await?;
    Ok(Some(created))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasts::testing::recorded_session;

    /// A plausible 10 min roast at 1 Hz: charge at 30 s after session start,
    /// TP, FC at 420 s after charge, drop at 540 s; fan steps 70 → 60 → 50,
    /// heater steps 90 → 70.
    fn roast() -> (Vec<RecordedSample>, Vec<RecordedLandmark>) {
        let charge = 30.0;
        let samples = (0..=620)
            .map(|i| {
                let t = i as f64;
                let r = t - charge;
                let bt = if r < 0.0 {
                    200.0
                } else if r < 60.0 {
                    200.0 - r * 1.6
                } else {
                    104.0 + (r - 60.0) * 0.21
                };
                let fan = if r < 120.0 {
                    70.0
                } else if r < 300.0 {
                    60.0
                } else {
                    50.0
                };
                let heater = if r < 240.0 { 90.0 } else { 70.0 };
                RecordedSample {
                    t,
                    bt: Some(bt as f32),
                    et: Some((bt + 40.0) as f32),
                    ror: Some(12.0),
                    heater_pct: Some(heater),
                    fan_pct: Some(fan),
                }
            })
            .collect();
        let landmarks = vec![
            RecordedLandmark {
                kind: RoastEventType::Charge,
                t: charge,
            },
            RecordedLandmark {
                kind: RoastEventType::TurningPoint,
                t: charge + 60.0,
            },
            RecordedLandmark {
                kind: RoastEventType::FirstCrackStart,
                t: charge + 420.0,
            },
            RecordedLandmark {
                kind: RoastEventType::Drop,
                t: charge + 540.0,
            },
        ];
        (samples, landmarks)
    }

    #[test]
    fn rebases_on_charge_and_stops_at_drop() {
        let (s, l) = roast();
        let p = extract(&s, &l).unwrap();
        assert_eq!(p.total_time, 540);
        assert_eq!(p.curves.bt.len(), 541);
        assert_eq!(p.first_crack, Some(420));
        assert_eq!(p.points.first().unwrap().time_seconds, 0);
        assert_eq!(p.points.last().unwrap().time_seconds, 540);
        assert!((p.charge_temp.unwrap() - 200.0).abs() < 0.5);
        // Smoothing leaves the end points where they were.
        assert!((p.points[0].target_temp - 200.0).abs() < 0.2);
        assert!((p.curves.bt[60].unwrap() - 104.0).abs() < 0.5);
    }

    #[test]
    fn points_follow_the_curve_within_tolerance() {
        let (s, l) = roast();
        let p = extract(&s, &l).unwrap();
        // Two straight segments plus landmarks and gap caps: few points.
        assert!(p.points.len() < 20, "{} points", p.points.len());
        let tp = p.points.iter().find(|p| p.time_seconds == 60).unwrap();
        assert!(tp.notes.as_deref().unwrap().contains("Turning Point"));
        // Interpolating the points reproduces BT within ~1 °C.
        for i in 0..=540 {
            let w = p
                .points
                .windows(2)
                .find(|w| w[0].time_seconds <= i && w[1].time_seconds >= i)
                .unwrap();
            let f = (i - w[0].time_seconds) as f32
                / (w[1].time_seconds - w[0].time_seconds).max(1) as f32;
            let v = w[0].target_temp + (w[1].target_temp - w[0].target_temp) * f;
            let truth = p.curves.bt[i as usize].unwrap();
            assert!((v - truth).abs() < 2.0, "t={i}: {v} vs {truth}");
        }
        assert!(p
            .points
            .iter()
            .all(|p| p.heater_pct.is_some() && p.fan_speed.is_some()));
    }

    #[test]
    fn events_are_landmarks_and_control_steps() {
        let (s, l) = roast();
        let p = extract(&s, &l).unwrap();
        let of = |a: ProfileAction| {
            p.events
                .iter()
                .filter(|e| e.action == Some(a))
                .collect::<Vec<_>>()
        };
        let marks = of(ProfileAction::Landmark);
        assert_eq!(marks.len(), 3); // TP, FC, DROP; CHARGE is t = 0
        assert_eq!(marks[1].label.as_deref(), Some("first_crack_start"));
        assert_eq!(marks[1].at_seconds, Some(420.0));
        assert!((marks[1].at_bt.unwrap() - 179.6).abs() < 1.0);
        let fan: Vec<(f64, f64)> = of(ProfileAction::Fan)
            .iter()
            .map(|e| (e.at_seconds.unwrap(), e.value.unwrap()))
            .collect();
        assert_eq!(fan, vec![(0.0, 70.0), (120.0, 60.0), (300.0, 50.0)]);
        let heater: Vec<(f64, f64)> = of(ProfileAction::Heater)
            .iter()
            .map(|e| (e.at_seconds.unwrap(), e.value.unwrap()))
            .collect();
        assert_eq!(heater, vec![(0.0, 90.0), (240.0, 70.0)]);
        for e in &p.events {
            e.validate().unwrap();
        }
    }

    #[test]
    fn pid_heater_yields_no_heater_events() {
        let (mut s, l) = roast();
        for (i, x) in s.iter_mut().enumerate() {
            x.heater_pct = Some(50.0 + ((i * 37) % 23) as f32); // jitters every second
        }
        let p = extract(&s, &l).unwrap();
        assert!(p
            .events
            .iter()
            .all(|e| e.action != Some(ProfileAction::Heater)));
    }

    #[test]
    fn no_charge_mark_uses_session_start_and_gaps_stay_empty() {
        let (mut s, _) = roast();
        s.retain(|x| !(200.0..230.0).contains(&x.t));
        let p = extract(&s, &[]).unwrap();
        assert_eq!(p.total_time, 620);
        assert!(p.curves.bt[215].is_none());
        assert!(p.curves.bt[199].is_some());
        assert!(p
            .events
            .iter()
            .all(|e| e.action != Some(ProfileAction::Landmark)));
    }

    #[test]
    fn too_little_data_is_none() {
        assert!(extract(&[], &[]).is_none());
        let one = [RecordedSample {
            t: 0.0,
            bt: Some(100.0),
            ..Default::default()
        }];
        assert!(extract(&one, &[]).is_none());
    }

    #[tokio::test]
    async fn test_save_session_as_profile() {
        let db = crate::db::open_memory().await;
        let session = recorded_session(&db).await;

        let p = crate::profiles::extract::save_session_as_profile(
            &db,
            &session.id,
            SaveSessionAsProfileRequest::default(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(p.profile.name, "Kenya AA (from session)");
        assert_eq!(p.profile.target_total_time, Some(520));
        assert_eq!(p.profile.target_first_crack, Some(420));
        assert_eq!(p.points.first().unwrap().time_seconds, 0);
        assert_eq!(p.points.last().unwrap().time_seconds, 520);
        assert!(p.points.iter().all(|pt| pt.heater_pct == Some(75)));
        assert_eq!(p.points[0].fan_speed, Some(80));

        let curves = p.curves.as_ref().unwrap();
        assert_eq!(
            curves.source_session_id.as_deref(),
            Some(session.id.as_str())
        );
        assert_eq!(curves.bt.len(), 521);
        assert_eq!(curves.et.len(), 521);

        let fan: Vec<(f64, f64)> = p
            .events
            .iter()
            .filter(|e| e.action == ProfileAction::Fan)
            .map(|e| (e.at_seconds.unwrap(), e.value.unwrap()))
            .collect();
        assert_eq!(fan, vec![(0.0, 80.0), (200.0, 60.0)]);
        let fc = p
            .events
            .iter()
            .find(|e| e.label.as_deref() == Some("first_crack_start"))
            .unwrap();
        assert_eq!(fc.trigger_kind, TriggerKind::Time);
        assert_eq!(fc.at_seconds, Some(420.0));

        // Missing session is None, not an error.
        assert!(crate::profiles::extract::save_session_as_profile(
            &db,
            "nope",
            SaveSessionAsProfileRequest::default()
        )
        .await
        .unwrap()
        .is_none());
    }
}
