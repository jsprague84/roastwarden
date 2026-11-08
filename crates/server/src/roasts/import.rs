//! An Artisan roast log as a finished roast: its samples with the canonical RoR, its landmarks,
//! the statistics Complete computes, the real roast times kept.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::profiles::import::artisan_event_kind;
use crate::roasts::types::{
    CreateRoastEventRequest, CreateSessionRequest, RoastSession, UpdateSessionRequest,
};

/// The log's samples as the roast's telemetry, in one transaction, the canonical RoR computed
/// as they go; returns the last sample's time. Roasted elsewhere: no batch number
/// (docs/batches.md).
async fn insert_samples(
    db: &SqlitePool,
    session_id: &str,
    parsed: &crate::profiles::artisan::ParsedSession,
    start: chrono::DateTime<Utc>,
) -> Result<f64> {
    let mut tx = db.begin().await?;
    // Roasted elsewhere: no batch number here (docs/batches.md).
    sqlx::query("UPDATE roast_sessions SET batch_number = NULL WHERE id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
    let cfg = crate::analysis::telemetry::RorConfig::default();
    let mut tracker = crate::analysis::telemetry::RorTracker::default();
    let mut last_elapsed = 0.0f64;
    for sample in &parsed.samples {
        if sample.elapsed < 0.0 {
            continue;
        }
        let ror = sample.bean_temp.and_then(|bt| {
            tracker.push(sample.elapsed, f64::from(bt));
            tracker.rate(&cfg)
        });
        let ts = start + chrono::Duration::milliseconds((sample.elapsed * 1000.0) as i64);
        sqlx::query(
                r"INSERT INTO session_telemetry
                   (id, session_id, timestamp, elapsed_seconds, bean_temp, env_temp, rate_of_rise, heater, fan, setpoint)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(session_id)
            .bind(ts)
            .bind(sample.elapsed as f32)
            .bind(sample.bean_temp)
            .bind(sample.env_temp)
            .bind(ror.map(|v| v as f32))
            .bind(sample.heater)
            .bind(sample.fan)
            .execute(&mut *tx)
            .await?;
        last_elapsed = sample.elapsed;
    }
    tx.commit().await?;
    Ok(last_elapsed)
}

/// Import a recorded Artisan roast as a completed session with telemetry,
/// landmarks and statistics. `device_id` defaults to "artisan-import".
pub async fn import_artisan_session(
    db: &SqlitePool,
    content: &str,
    device_id: Option<String>,
    name_override: Option<String>,
) -> Result<RoastSession> {
    let parsed = crate::profiles::artisan::parse_session(content)?;
    if parsed.samples.len() < 2 {
        return Err(anyhow!("roast log contains fewer than two usable samples"));
    }
    let device_id = device_id
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "artisan-import".to_string());
    let name = name_override
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| parsed.title.clone());

    let session = crate::roasts::sessions::create_session(
        db,
        CreateSessionRequest {
            bean_id: None,
            planned_roast_id: None,
            roast_purpose: None,
            name,
            device_id,
            profile_id: None,
            bean_origin: parsed.beans.clone(),
            bean_variety: None,
            green_weight: parsed.green_weight_g,
            target_roast_level: None,
            notes: parsed.notes.clone(),
            ambient_temp: parsed.ambient_temp,
            humidity: parsed.humidity,
        },
    )
    .await?;
    let start = parsed.roast_start.unwrap_or_else(Utc::now);

    let last_elapsed = insert_samples(db, &session.id, &parsed, start).await?;

    for ev in &parsed.events {
        let Some(kind) = artisan_event_kind(&ev.event_type) else {
            continue;
        };
        if ev.time < 0.0 {
            continue;
        }
        crate::roasts::events::create_roast_event(
            db,
            &session.id,
            CreateRoastEventRequest {
                event_type: kind,
                elapsed_seconds: ev.time as f32,
                temperature: Some(ev.bean_temp),
                notes: Some("imported from Artisan".into()),
            },
        )
        .await?;
    }

    // Activate so complete_session accepts it, compute the statistics, then put the
    // real roast times back (complete_session stamps "now").
    crate::roasts::sessions::start_session(db, &session.id).await?;
    if let Some(roasted) = parsed.roasted_weight_g {
        crate::roasts::sessions::update_session(
            db,
            &session.id,
            UpdateSessionRequest {
                roasted_weight: Some(roasted),
                ..Default::default()
            },
        )
        .await?;
    }
    crate::roasts::sessions::complete_session(db, &session.id).await?;
    let end = start + chrono::Duration::milliseconds((last_elapsed * 1000.0) as i64);
    let session = sqlx::query_as::<_, RoastSession>(
            "UPDATE roast_sessions SET start_time = ?, end_time = ?, created_at = ? WHERE id = ? RETURNING *",
        )
        .bind(start)
        .bind(end)
        .bind(start)
        .bind(&session.id)
        .fetch_all(db)
        .await?
        .pop()
        .context("RETURNING gave no row")?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use crate::roasts::types::{RoastEventType, SessionStatus};

    #[tokio::test]
    async fn test_import_artisan_session() {
        let db = crate::db::open_memory().await;
        let timex: Vec<f64> = (0..=600).map(|i| i as f64).collect();
        let temp2: Vec<f64> = (0..=600)
            .map(|i| {
                if i < 60 {
                    180.0 - i as f64
                } else {
                    120.0 + (i as f64 - 60.0) * 0.18
                }
            })
            .collect();
        let temp1: Vec<f64> = (0..=600).map(|i| 200.0 + i as f64 * 0.05).collect();
        let alog = serde_json::json!({
            "title": "Colombia 1", "mode": "C", "timex": timex, "temp1": temp1, "temp2": temp2,
            "timeindex": [0, 240, 480, 0, 0, 0, 600, 0],
            "weight": [250.0, 212.0, "g"], "beans": "Colombia", "roastepoch": 1_759_600_000,
            "specialevents": [0, 300], "specialeventstype": [3, 3], "specialeventsvalue": [9.0, 6.0]
        });
        let session = crate::roasts::import::import_artisan_session(
            &db,
            &crate::profiles::artisan::to_python_literal(&alog),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(session.status, SessionStatus::Completed);
        assert_eq!(session.device_id, "artisan-import");
        assert_eq!(session.start_time.unwrap().timestamp(), 1_759_600_000);
        assert_eq!(session.total_time_seconds, Some(600));
        assert_eq!(session.first_crack_time, Some(480));
        assert!((session.weight_loss_pct.unwrap() - 15.2).abs() < 0.01);
        assert!(session.development_time_ratio.unwrap() > 0.19);
        let tel = crate::roasts::telemetry_store::get_session_telemetry(&db, &session.id)
            .await
            .unwrap();
        assert_eq!(tel.len(), 601);
        assert_eq!(tel[0].heater, Some(80.0));
        assert_eq!(tel[400].heater, Some(50.0));
        assert!(tel[100].rate_of_rise.is_some());
        let events = crate::roasts::events::get_roast_events(&db, &session.id)
            .await
            .unwrap();
        assert!(events.iter().any(|e| e.event_type == RoastEventType::Drop));
    }
}
