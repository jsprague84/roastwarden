//! Roast profiles stored: created, listed, read with their points, events and curves, updated
//! (events and curves only when the request carries them), deleted.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::profiles::types::{
    CreateProfilePointRequest, CreateProfileRequest, ProfileCurves, ProfileEvent, ProfilePoint,
    ProfileWithPoints, RoastProfile, ValidProfileEvent,
};
use crate::profiles::validate::{
    non_blank, validate_auto_drop, validate_guidance, validate_profile_input, validate_roaster_kind,
};

// Profile Management
pub async fn create_profile(
    db: &SqlitePool,
    req: CreateProfileRequest,
) -> Result<ProfileWithPoints> {
    let events = validate_profile_input(&req)?;
    let guidance = validate_guidance(&req)?;
    let auto_drop = validate_auto_drop(&req, None)?.flatten();
    let kind = validate_roaster_kind(req.roaster_kind.as_ref())?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let mut tx = db.begin().await?;

    sqlx::query(
        r"
            INSERT INTO roast_profiles (
                id, name, description, created_at, updated_at, is_public,
                target_total_time, target_first_crack, target_end_temp,
                preheat_temp, charge_temp, beans, batch_size_g, roaster,
                roast_level, roast_purpose, expectations, auto_drop, roaster_kind
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .bind(false) // Default to private
    .bind(req.target_total_time)
    .bind(req.target_first_crack)
    .bind(req.target_end_temp)
    .bind(req.preheat_temp)
    .bind(req.charge_temp)
    .bind(non_blank(req.beans.as_deref()))
    .bind(req.batch_size_g.filter(|g| g.is_finite() && *g > 0.0))
    .bind(non_blank(req.roaster.as_deref()))
    .bind(guidance.level.clone().flatten())
    .bind(guidance.purpose.clone().flatten())
    .bind(guidance.expectations.clone().flatten())
    .bind(auto_drop)
    .bind(kind.flatten())
    .execute(&mut *tx)
    .await?;

    insert_points(&mut tx, &id, &req.points, now).await?;
    if let Some(events) = &events {
        insert_events(&mut tx, &id, events, now).await?;
    }
    if let Some(Some(curves)) = &req.curves {
        upsert_curves(&mut tx, &id, curves).await?;
    }
    tx.commit().await?;

    get_profile_with_points(db, &id)
        .await?
        .ok_or_else(|| anyhow!("profile vanished after insert"))
}

pub async fn list_profiles(db: &SqlitePool, include_private: bool) -> Result<Vec<RoastProfile>> {
    let query = if include_private {
        "SELECT * FROM roast_profiles ORDER BY created_at DESC"
    } else {
        "SELECT * FROM roast_profiles WHERE is_public = 1 ORDER BY created_at DESC"
    };

    let profiles = sqlx::query_as::<_, RoastProfile>(query)
        .fetch_all(db)
        .await?;

    Ok(profiles)
}

pub async fn get_profile_with_points(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<ProfileWithPoints>> {
    let profile = sqlx::query_as::<_, RoastProfile>("SELECT * FROM roast_profiles WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;

    let Some(profile) = profile else {
        return Ok(None);
    };

    let points = sqlx::query_as::<_, ProfilePoint>(
        "SELECT * FROM profile_points WHERE profile_id = ? ORDER BY time_seconds",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    let events = sqlx::query_as::<_, ProfileEvent>(
        "SELECT * FROM profile_events WHERE profile_id = ?
             ORDER BY COALESCE(at_seconds, 1e9), COALESCE(at_bt, 1e9), created_at",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    let curves = sqlx::query(
        "SELECT source_session_id, interval_seconds, bt, et, ror, heater, fan
             FROM profile_curves WHERE profile_id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .map(|r| {
        let arr = |col: &str| -> Vec<Option<f32>> {
            r.get::<Option<String>, _>(col)
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default()
        };
        ProfileCurves {
            source_session_id: r.get("source_session_id"),
            interval_seconds: r.get("interval_seconds"),
            bt: arr("bt"),
            et: arr("et"),
            ror: arr("ror"),
            heater: arr("heater"),
            fan: arr("fan"),
        }
    });

    Ok(Some(ProfileWithPoints {
        profile,
        points,
        events,
        curves,
    }))
}

pub async fn delete_profile(db: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM roast_profiles WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Replace a profile's metadata and points. Events and curves are
/// replaced only when the request carries them (see `CreateProfileRequest`).
pub async fn update_profile(
    db: &SqlitePool,
    id: &str,
    req: CreateProfileRequest,
) -> Result<Option<ProfileWithPoints>> {
    let events = validate_profile_input(&req)?;
    let guidance = validate_guidance(&req)?;
    // A kept auto DROP must still find its target in the new values.
    let stored: Option<String> =
        sqlx::query_scalar("SELECT auto_drop FROM roast_profiles WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?
            .flatten();
    let auto_drop = validate_auto_drop(&req, stored)?;
    let kind = validate_roaster_kind(req.roaster_kind.as_ref())?;
    let now = Utc::now();
    let mut tx = db.begin().await?;

    // Update profile metadata
    let result = sqlx::query(
        r"
            UPDATE roast_profiles SET
                name = ?, description = ?, updated_at = ?,
                target_total_time = ?, target_first_crack = ?, target_end_temp = ?,
                preheat_temp = ?, charge_temp = ?,
                beans = ?, batch_size_g = ?, roaster = ?
            WHERE id = ?
            ",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(now)
    .bind(req.target_total_time)
    .bind(req.target_first_crack)
    .bind(req.target_end_temp)
    .bind(req.preheat_temp)
    .bind(req.charge_temp)
    .bind(non_blank(req.beans.as_deref()))
    .bind(req.batch_size_g.filter(|g| g.is_finite() && *g > 0.0))
    .bind(non_blank(req.roaster.as_deref()))
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    for (col, value) in [
        ("roast_level", guidance.level),
        ("roast_purpose", guidance.purpose),
        ("expectations", guidance.expectations),
        ("auto_drop", auto_drop),
        ("roaster_kind", kind),
    ] {
        if let Some(v) = value {
            sqlx::query(&format!("UPDATE roast_profiles SET {col} = ? WHERE id = ?"))
                .bind(v)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Delete old points and insert new ones atomically
    sqlx::query("DELETE FROM profile_points WHERE profile_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_points(&mut tx, id, &req.points, now).await?;

    if let Some(events) = &events {
        sqlx::query("DELETE FROM profile_events WHERE profile_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_events(&mut tx, id, events, now).await?;
    }
    match &req.curves {
        None => {}
        Some(None) => {
            sqlx::query("DELETE FROM profile_curves WHERE profile_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        Some(Some(curves)) => upsert_curves(&mut tx, id, curves).await?,
    }

    tx.commit().await?;

    // Fetch and return updated profile with points
    get_profile_with_points(db, id).await
}

async fn insert_points(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile_id: &str,
    points: &[CreateProfilePointRequest],
    now: DateTime<Utc>,
) -> Result<()> {
    for p in points {
        sqlx::query(
            r"
            INSERT INTO profile_points (
                id, profile_id, time_seconds, target_temp, fan_speed, notes, created_at,
                target_env_temp, heater_pct
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(profile_id)
        .bind(p.time_seconds)
        .bind(p.target_temp)
        .bind(p.fan_speed)
        .bind(&p.notes)
        .bind(now)
        .bind(p.target_env_temp)
        .bind(p.heater_pct)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile_id: &str,
    events: &[ValidProfileEvent],
    now: DateTime<Utc>,
) -> Result<()> {
    for e in events {
        sqlx::query(
            r"
            INSERT INTO profile_events (
                id, profile_id, trigger_kind, at_seconds, at_bt, action, value, label, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(profile_id)
        .bind(e.trigger_kind)
        .bind(e.at_seconds)
        .bind(e.at_bt)
        .bind(e.action)
        .bind(e.value)
        .bind(&e.label)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn upsert_curves(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile_id: &str,
    c: &ProfileCurves,
) -> Result<()> {
    let json = |v: &Vec<Option<f32>>| -> Option<String> {
        (!v.is_empty()).then(|| serde_json::to_string(v).unwrap_or_else(|_| "[]".into()))
    };
    sqlx::query(
        r"
        INSERT INTO profile_curves (profile_id, source_session_id, interval_seconds, bt, et, ror, heater, fan)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(profile_id) DO UPDATE SET
            source_session_id = excluded.source_session_id,
            interval_seconds = excluded.interval_seconds,
            bt = excluded.bt, et = excluded.et, ror = excluded.ror,
            heater = excluded.heater, fan = excluded.fan,
            created_at = CURRENT_TIMESTAMP
        ",
    )
    .bind(profile_id)
    .bind(&c.source_session_id)
    .bind(c.interval_seconds)
    .bind(json(&c.bt).unwrap_or_else(|| "[]".into()))
    .bind(json(&c.et))
    .bind(json(&c.ror))
    .bind(json(&c.heater))
    .bind(json(&c.fan))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
