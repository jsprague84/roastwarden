//! What Complete stores about a roast: its times and temperatures from the telemetry and the
//! landmarks, the RoR of each phase, the AUC, and how its BT tracked the profile.

use anyhow::Result;
use sqlx::{Row, SqlitePool};

use crate::roasts::types::{RoastEvent, RoastEventType, RoastSession};

/// BT against a profile curve (seconds from CHARGE, °C) over roast time
/// `from..=to`: (RMS, largest absolute deviation, mean deviation), °C.
/// Samples are (seconds from CHARGE, BT); only times the curve covers count.
pub fn tracking_stats(
    samples: &[(f64, f64)],
    curve: &[(f64, f64)],
    from: f64,
    to: f64,
) -> Option<(f64, f64, f64)> {
    let end = curve.last()?.0;
    let mut n = 0usize;
    let (mut sq, mut max, mut sum) = (0.0f64, 0.0f64, 0.0f64);
    for &(t, bt) in samples {
        if t < from || t > to || t > end || !bt.is_finite() {
            continue;
        }
        let Some(p) = crate::replay::engine::interpolate(curve, t) else {
            continue;
        };
        let d = bt - p;
        n += 1;
        sq += d * d;
        sum += d;
        max = max.max(d.abs());
    }
    (n >= 10).then(|| ((sq / n as f64).sqrt(), max, sum / n as f64))
}

/// What Complete stores about a roast (`roast_stats`).
pub(crate) struct RoastStats {
    pub(crate) total_time_seconds: Option<i32>,
    pub(crate) max_temp: Option<f32>,
    pub(crate) max_ror: Option<f32>,
    pub(crate) first_crack_time: Option<i32>,
    pub(crate) development_time_ratio: Option<f32>,
    pub(crate) weight_loss_pct: Option<f32>,
    pub(crate) avg_ror_drying: Option<f32>,
    pub(crate) avg_ror_maillard: Option<f32>,
    pub(crate) avg_ror_development: Option<f32>,
    pub(crate) drying_end_time: Option<i32>,
    pub(crate) drying_end_temp: Option<f32>,
    pub(crate) auc_value: Option<f32>,
    pub(crate) profile_rmse: Option<f32>,
    pub(crate) profile_max_dev: Option<f32>,
    pub(crate) profile_bias: Option<f32>,
}

/// Everything Complete stores about a roast, from its telemetry and events.
pub(crate) async fn roast_stats(
    db: &SqlitePool,
    id: &str,
    existing: &RoastSession,
) -> Result<RoastStats> {
    // Calculate total time, max temperature, and max RoR from telemetry
    let stats = sqlx::query(
        r"
            SELECT
                MAX(elapsed_seconds) as total_seconds,
                MAX(bean_temp) as max_temp,
                MAX(rate_of_rise) as max_ror
            FROM session_telemetry
            WHERE session_id = ?
            ",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    let (total_time_seconds, max_temp, max_ror) = if let Some(row) = &stats {
        let total_seconds: Option<f32> = row.try_get("total_seconds").ok();
        let max_temp: Option<f32> = row.try_get("max_temp").ok();
        let max_ror: Option<f32> = row.try_get("max_ror").ok();
        (total_seconds.map(|s| s as i32), max_temp, max_ror)
    } else {
        (None, None, None)
    };

    // Fetch roast events for phase boundaries
    let events = crate::roasts::events::get_roast_events(db, id).await?;

    let drying_end_event = events
        .iter()
        .find(|e| e.event_type == RoastEventType::DryingEnd);
    let first_crack_event = events
        .iter()
        .find(|e| e.event_type == RoastEventType::FirstCrackStart);
    let drop_event = events.iter().find(|e| e.event_type == RoastEventType::Drop);

    // Roast time runs CHARGE -> DROP (telemetry goes on through cooling,
    // and CHARGE can come after the session start).
    let charge_at = events
        .iter()
        .find(|e| e.event_type == RoastEventType::Charge)
        .map_or(0.0, |e| e.elapsed_seconds);
    let total_time_seconds = drop_event
        .map(|d| (d.elapsed_seconds - charge_at).round() as i32)
        .or(total_time_seconds);

    // DTR = (DROP - FC) / (DROP - CHARGE)
    let development_time_ratio = match (drop_event, first_crack_event) {
        (Some(d), Some(fc)) if d.elapsed_seconds > charge_at => {
            Some((d.elapsed_seconds - fc.elapsed_seconds) / (d.elapsed_seconds - charge_at))
        }
        (None, Some(fc)) => total_time_seconds
            .filter(|t| *t > 0)
            .map(|t| (t as f32 - (fc.elapsed_seconds - charge_at)) / t as f32),
        _ => None,
    };

    // First crack and dry end are counted from CHARGE, as the roast time is (events
    // are stamped from the session start, which comes a few seconds or more earlier).
    let first_crack_time =
        first_crack_event.map(|fc| (fc.elapsed_seconds - charge_at).round() as i32);

    // Compute weight_loss_pct
    let weight_loss_pct = match (existing.green_weight, existing.roasted_weight) {
        (Some(green), Some(roasted)) if green > 0.0 => Some((green - roasted) / green * 100.0),
        _ => None,
    };

    // Drying end time and temp from events
    let drying_end_time = drying_end_event.map(|e| (e.elapsed_seconds - charge_at).round() as i32);
    let drying_end_temp = drying_end_event.and_then(|e| e.temperature);

    // Phase boundaries (elapsed_seconds, session time):
    //   Drying:      CHARGE → drying_end
    //   Maillard:    drying_end → first_crack_start
    //   Development: first_crack_start → drop (or total_time)
    let end_seconds = drop_event
        .map(|e| e.elapsed_seconds)
        .or(total_time_seconds.map(|t| t as f32));

    let avg_ror_drying = if let Some(de) = drying_end_event {
        avg_ror_in_range(db, id, charge_at, de.elapsed_seconds).await?
    } else {
        None
    };

    let avg_ror_maillard = match (drying_end_event, first_crack_event) {
        (Some(de), Some(fc)) => {
            avg_ror_in_range(db, id, de.elapsed_seconds, fc.elapsed_seconds).await?
        }
        _ => None,
    };

    let avg_ror_development = match (first_crack_event, end_seconds) {
        (Some(fc), Some(end)) => avg_ror_in_range(db, id, fc.elapsed_seconds, end).await?,
        _ => None,
    };

    // Compute AUC (Area Under the Curve) using trapezoidal rule
    let auc_value = compute_auc(db, id, &events).await?;

    let tracking = profile_tracking(db, id, existing, &events, charge_at, drop_event).await?;
    let (profile_rmse, profile_max_dev, profile_bias) = match tracking {
        Some((r, m, b)) => (Some(r as f32), Some(m as f32), Some(b as f32)),
        None => (None, None, None),
    };

    Ok(RoastStats {
        total_time_seconds,
        max_temp,
        max_ror,
        first_crack_time,
        development_time_ratio,
        weight_loss_pct,
        avg_ror_drying,
        avg_ror_maillard,
        avg_ror_development,
        drying_end_time,
        drying_end_temp,
        auc_value,
        profile_rmse,
        profile_max_dev,
        profile_bias,
    })
}

/// How BT tracked the session's profile, from TP (else CHARGE + 60 s) to DROP: (RMS, largest
/// absolute deviation, mean deviation), °C. `None` without a profile or enough samples.
async fn profile_tracking(
    db: &SqlitePool,
    id: &str,
    existing: &RoastSession,
    events: &[RoastEvent],
    charge_at: f32,
    drop_event: Option<&RoastEvent>,
) -> Result<Option<(f64, f64, f64)>> {
    Ok(match existing.profile_id.as_deref() {
        Some(pid) => match crate::profiles::store::get_profile_with_points(db, pid).await? {
            Some(p) => {
                let curve = crate::replay::engine::Plan::from_profile(&p).bt;
                let tp = events
                    .iter()
                    .find(|e| e.event_type == RoastEventType::TurningPoint)
                    .map_or(60.0, |e| f64::from(e.elapsed_seconds - charge_at));
                let to =
                    drop_event.map_or(f64::INFINITY, |d| f64::from(d.elapsed_seconds - charge_at));
                let rows: Vec<(f32, f32)> = sqlx::query_as(
                    "SELECT elapsed_seconds, bean_temp FROM session_telemetry
                         WHERE session_id = ? AND bean_temp IS NOT NULL ORDER BY elapsed_seconds",
                )
                .bind(id)
                .fetch_all(db)
                .await?;
                let samples: Vec<(f64, f64)> = rows
                    .iter()
                    .map(|(t, b)| (f64::from(*t - charge_at), f64::from(*b)))
                    .collect();
                tracking_stats(&samples, &curve, tp, to)
            }
            None => None,
        },
        None => None,
    })
}

/// Compute average `rate_of_rise` within a time range from session telemetry.
async fn avg_ror_in_range(
    db: &SqlitePool,
    session_id: &str,
    start: f32,
    end: f32,
) -> Result<Option<f32>> {
    let row = sqlx::query(
        r"
            SELECT AVG(rate_of_rise) as avg_ror
            FROM session_telemetry
            WHERE session_id = ? AND elapsed_seconds >= ? AND elapsed_seconds < ?
              AND rate_of_rise IS NOT NULL
            ",
    )
    .bind(session_id)
    .bind(start)
    .bind(end)
    .fetch_optional(db)
    .await?;

    Ok(row.and_then(|r| r.try_get("avg_ror").ok()))
}

/// AUC of BT above `auc_base_temp`, in °C·min, from the `auc_start_event` landmark
/// (CHARGE when unset or not marked) to DROP (else the last sample): the same rule as
/// the dashboard's `lib/roast/stats.ts` (docs/statistics.md).
async fn compute_auc(
    db: &SqlitePool,
    session_id: &str,
    events: &[RoastEvent],
) -> Result<Option<f32>> {
    let base_temp: f64 =
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'auc_base_temp'")
            .fetch_optional(db)
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
    let start_event: String =
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'auc_start_event'")
            .fetch_optional(db)
            .await?
            .unwrap_or_else(|| "charge".to_string());

    let at = |t: RoastEventType| {
        events
            .iter()
            .find(|e| e.event_type == t)
            .map(|e| e.elapsed_seconds as f64)
    };
    let charge = at(RoastEventType::Charge).unwrap_or(0.0);
    let start = match start_event.as_str() {
        "turning_point" => at(RoastEventType::TurningPoint),
        "drying_end" => at(RoastEventType::DryingEnd),
        "first_crack_start" => at(RoastEventType::FirstCrackStart),
        _ => None,
    }
    .unwrap_or(charge);
    let end = at(RoastEventType::Drop).unwrap_or(f64::INFINITY);

    let rows = sqlx::query(
        "SELECT elapsed_seconds, bean_temp FROM session_telemetry \
             WHERE session_id = ? AND bean_temp IS NOT NULL ORDER BY elapsed_seconds",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;
    let mut pts = Vec::with_capacity(rows.len());
    for r in &rows {
        let t: f32 = r.try_get("elapsed_seconds")?;
        let bt: f32 = r.try_get("bean_temp")?;
        pts.push((t as f64, bt as f64));
    }
    Ok(auc_between(&pts, start, end, base_temp).map(|v| v as f32))
}

/// Area of BT above `base` between `from` and `to` (trapezoids, the ends interpolated, a
/// segment crossing the base counted above it only), in °C·min. `None` with fewer than two
/// samples in range. Mirrors `aucBetween` in the dashboard's `lib/roast/stats.ts`.
pub fn auc_between(pts: &[(f64, f64)], from: f64, to: f64, base: f64) -> Option<f64> {
    if pts.len() < 2 {
        return None;
    }
    let a = from.max(pts[0].0);
    let b = to.min(pts[pts.len() - 1].0);
    if b <= a {
        return None;
    }
    let value_at = |t: f64| -> f64 {
        let i = pts.partition_point(|p| p.0 <= t).clamp(1, pts.len() - 1);
        let (t0, v0) = pts[i - 1];
        let (t1, v1) = pts[i];
        if t1 == t0 {
            v0
        } else {
            v0 + (v1 - v0) * (t - t0) / (t1 - t0)
        }
    };
    let mut seq = vec![(a, value_at(a))];
    seq.extend(pts.iter().copied().filter(|p| p.0 > a && p.0 < b));
    seq.push((b, value_at(b)));
    let mut sum = 0.0;
    for w in seq.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        let (d0, d1) = (v0 - base, v1 - base);
        if d0 >= 0.0 && d1 >= 0.0 {
            sum += (t1 - t0) * (d0 + d1) / 2.0;
        } else if d0 > 0.0 || d1 > 0.0 {
            let up = d0.max(d1);
            sum += (t1 - t0) * (up / (up - d0.min(d1))) * up / 2.0;
        }
    }
    Some(sum / 60.0)
}

#[cfg(test)]
mod tests;
