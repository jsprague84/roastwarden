//! A roast's record: made from a plan or a bean, started, paused, completed with its
//! statistics (`stats.rs`), read with everything the roast page shows, deleted.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::profiles::validate::non_blank;
use crate::roasts::types::{
    CreateSessionRequest, RoastSession, SessionStatus, SessionWithTelemetry, UpdateSessionRequest,
};

/// Owned bind value for dynamically built UPDATE statements.
enum SqlValue {
    Text(String),
    Real(f32),
    Int(i32),
    Null,
}

// Session Management
/// What the plan, the profile and the bean fill in on a new roast; returns the request with
/// them and the bean's moisture and density. A plan that is gone or already ran, or a bean
/// that is gone, is dropped from the request.
async fn fill_from_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mut req: CreateSessionRequest,
) -> Result<(CreateSessionRequest, Option<f32>, Option<f32>)> {
    // A plan that is gone or already ran is ignored: starting the roast matters more.
    if let Some(plan_id) = req.planned_roast_id.clone() {
        type Plan = (
            Option<String>,
            Option<String>,
            Option<f32>,
            Option<String>,
            Option<String>,
        );
        let plan: Option<Plan> = sqlx::query_as(
            "SELECT profile_id, bean_id, green_weight_g, name, notes FROM planned_roasts \
                 WHERE id = ? AND session_id IS NULL",
        )
        .bind(&plan_id)
        .fetch_optional(&mut **tx)
        .await?;
        match plan {
            Some((profile_id, bean_id, weight, name, notes)) => {
                req.profile_id = req.profile_id.or(profile_id);
                req.bean_id = req.bean_id.or(bean_id);
                req.green_weight = req.green_weight.or(weight);
                req.notes = req.notes.or(notes);
                if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                    if req.name.trim().is_empty() || req.name.starts_with("Roast ") {
                        req.name = n;
                    }
                }
            }
            None => req.planned_roast_id = None,
        }
    }

    // The profile says what the roast is for (docs/roast-guidance.md).
    if let Some(profile_id) = req.profile_id.clone() {
        let profile: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT roast_level, roast_purpose FROM roast_profiles WHERE id = ?")
                .bind(&profile_id)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some((level, purpose)) = profile {
            req.target_roast_level = non_blank(req.target_roast_level.as_deref()).or(level);
            req.roast_purpose = non_blank(req.roast_purpose.as_deref()).or(purpose);
        }
    }

    let mut moisture: Option<f32> = None;
    let mut density: Option<f32> = None;
    if let Some(bean_id) = req.bean_id.clone() {
        type Bean = (Option<String>, Option<String>, Option<f32>, Option<f32>);
        let bean: Option<Bean> = sqlx::query_as(
            "SELECT origin, variety, moisture_pct, density_g_l FROM beans WHERE id = ?",
        )
        .bind(&bean_id)
        .fetch_optional(&mut **tx)
        .await?;
        match bean {
            Some((origin, variety, m, d)) => {
                req.bean_origin = req.bean_origin.filter(|v| !v.trim().is_empty()).or(origin);
                req.bean_variety = req
                    .bean_variety
                    .filter(|v| !v.trim().is_empty())
                    .or(variety);
                moisture = m;
                density = d;
            }
            None => req.bean_id = None,
        }
    }
    Ok((req, moisture, density))
}

/// A new roast (status planning). It gets the next batch number on its roaster; a
/// planned roast fills what the request leaves out and is linked to it; a bean fills
/// origin, variety, moisture and density, and the roast draws its green weight from
/// the bean's stock (a trigger, `018_batches_beans.sql`; docs/batches.md).
pub async fn create_session(db: &SqlitePool, req: CreateSessionRequest) -> Result<RoastSession> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    // Reads (the plan, the bean), then writes: the write lock first (db::begin_write).
    let mut tx = crate::db::begin_write(db).await?;

    let (req, moisture, density) = fill_from_links(&mut tx, req).await?;

    sqlx::query(
        r"
            INSERT INTO roast_sessions (
                id, name, device_id, profile_id, status, start_time, created_at, updated_at,
                bean_origin, bean_variety, green_weight, target_roast_level,
                notes, ambient_temp, humidity, bean_id, planned_roast_id,
                moisture_green, density_green, roast_purpose, batch_number
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                (SELECT COALESCE(MAX(batch_number), 0) + 1 FROM roast_sessions WHERE device_id = ?))
            ",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.device_id)
    .bind(&req.profile_id)
    .bind(SessionStatus::Planning.to_string())
    .bind(None::<DateTime<Utc>>) // NULL for planning sessions
    .bind(now)
    .bind(now)
    .bind(&req.bean_origin)
    .bind(&req.bean_variety)
    .bind(req.green_weight)
    .bind(&req.target_roast_level)
    .bind(&req.notes)
    .bind(req.ambient_temp)
    .bind(req.humidity)
    .bind(&req.bean_id)
    .bind(&req.planned_roast_id)
    .bind(moisture)
    .bind(density)
    .bind(&req.roast_purpose)
    .bind(&req.device_id)
    .execute(&mut *tx)
    .await?;
    if let Some(plan_id) = &req.planned_roast_id {
        sqlx::query("UPDATE planned_roasts SET session_id = ?, updated_at = ? WHERE id = ?")
            .bind(&id)
            .bind(now)
            .bind(plan_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    get_session(db, &id)
        .await?
        .context("new roast not found after insert")
}

pub async fn list_sessions(
    db: &SqlitePool,
    device_id: Option<&str>,
    limit: Option<i32>,
) -> Result<Vec<RoastSession>> {
    let mut query = "SELECT * FROM roast_sessions".to_string();
    let mut conditions = Vec::new();

    if device_id.is_some() {
        conditions.push("device_id = ?");
    }

    if !conditions.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&conditions.join(" AND "));
    }

    query.push_str(" ORDER BY created_at DESC");

    if let Some(limit) = limit {
        let _ = write!(query, " LIMIT {limit}");
    }

    let mut query_builder = sqlx::query_as::<_, RoastSession>(AssertSqlSafe(query.as_str()));

    if let Some(device_id) = device_id {
        query_builder = query_builder.bind(device_id);
    }

    let sessions = query_builder.fetch_all(db).await?;
    Ok(sessions)
}

pub async fn get_session(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let session = sqlx::query_as::<_, RoastSession>("SELECT * FROM roast_sessions WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;

    Ok(session)
}

pub async fn get_session_with_telemetry(
    db: &SqlitePool,
    id: &str,
) -> Result<Option<SessionWithTelemetry>> {
    let Some(session) = get_session(db, id).await? else {
        return Ok(None);
    };

    let telemetry = crate::roasts::telemetry_store::get_session_telemetry(db, id).await?;
    let profile = if let Some(profile_id) = &session.profile_id {
        crate::profiles::store::get_profile_with_points(db, profile_id).await?
    } else {
        None
    };

    let cupping = crate::roasts::cupping::get_cupping(db, id).await?;
    let channels = crate::roasters::channels::for_roast(db, id, &session.device_id).await;

    Ok(Some(SessionWithTelemetry {
        session,
        telemetry,
        profile,
        cupping,
        channels,
    }))
}

/// The (column, value) pairs an update writes: one for every field the request carries.
fn session_updates(req: UpdateSessionRequest) -> Vec<(&'static str, SqlValue)> {
    let mut sets: Vec<(&str, SqlValue)> = Vec::new();
    if let Some(v) = req.name {
        sets.push(("name", SqlValue::Text(v)));
    }
    if let Some(v) = req.bean_origin {
        sets.push(("bean_origin", SqlValue::Text(v)));
    }
    if let Some(v) = req.bean_variety {
        sets.push(("bean_variety", SqlValue::Text(v)));
    }
    if let Some(v) = req.green_weight {
        sets.push(("green_weight", SqlValue::Real(v)));
    }
    if let Some(v) = req.roasted_weight {
        sets.push(("roasted_weight", SqlValue::Real(v)));
    }
    if let Some(v) = req.target_roast_level {
        sets.push(("target_roast_level", SqlValue::Text(v)));
    }
    if let Some(v) = req.ambient_temp {
        sets.push(("ambient_temp", SqlValue::Real(v)));
    }
    if let Some(v) = req.humidity {
        sets.push(("humidity", SqlValue::Real(v)));
    }
    if let Some(v) = req.notes {
        sets.push(("notes", SqlValue::Text(v)));
    }
    if let Some(v) = req.first_crack_time {
        sets.push(("first_crack_time", SqlValue::Int(v)));
    }
    if let Some(v) = req.development_time_ratio {
        sets.push(("development_time_ratio", SqlValue::Real(v)));
    }
    let text = |v: Option<String>| match v.map(|t| t.trim().to_string()) {
        Some(t) if !t.is_empty() => SqlValue::Text(t),
        _ => SqlValue::Null,
    };
    let real = |v: Option<f32>| {
        v.filter(|x| x.is_finite())
            .map_or(SqlValue::Null, SqlValue::Real)
    };
    for (col, v) in [
        ("roast_purpose", req.roast_purpose),
        ("color_scale", req.color_scale),
        ("roast_flags", req.roast_flags),
    ] {
        if let Some(v) = v {
            sets.push((col, text(v)));
        }
    }
    for (col, v) in [
        ("color_whole", req.color_whole),
        ("color_ground", req.color_ground),
        ("moisture_green", req.moisture_green),
        ("moisture_roasted", req.moisture_roasted),
        ("density_green", req.density_green),
    ] {
        if let Some(v) = v {
            sets.push((col, real(v)));
        }
    }
    if let Some(v) = req.bean_id {
        sets.push(("bean_id", text(v)));
    }
    if let Some(v) = req.batch_number {
        sets.push((
            "batch_number",
            v.filter(|n| *n > 0).map_or(SqlValue::Null, SqlValue::Int),
        ));
    }
    if let Some(v) = req.rating {
        sets.push((
            "rating",
            v.map_or(SqlValue::Null, |r| SqlValue::Int(r.clamp(1, 5))),
        ));
    }
    sets
}

pub async fn update_session(
    db: &SqlitePool,
    id: &str,
    req: UpdateSessionRequest,
) -> Result<Option<RoastSession>> {
    let sets = session_updates(req);
    if sets.is_empty() {
        return get_session(db, id).await;
    }

    let mut query = "UPDATE roast_sessions SET updated_at = ?".to_string();
    for (col, _) in &sets {
        let _ = write!(query, ", {col} = ?");
    }
    query.push_str(" WHERE id = ?");

    let mut tx = db.begin().await?;
    let mut q = sqlx::query(AssertSqlSafe(query.as_str())).bind(Utc::now());
    for (_, v) in sets {
        q = match v {
            SqlValue::Text(t) => q.bind(t),
            SqlValue::Real(r) => q.bind(r),
            SqlValue::Int(i) => q.bind(i),
            SqlValue::Null => q.bind(Option::<String>::None),
        };
    }
    q.bind(id).execute(&mut *tx).await?;
    // Weight loss follows the weights whenever both are known. A second statement, so it
    // sees the weights just written: in one UPDATE the columns still hold the old values,
    // and a first roasted weight left weight loss empty until the next save.
    let session = sqlx::query_as::<_, RoastSession>(
        "UPDATE roast_sessions SET weight_loss_pct = CASE \
             WHEN green_weight > 0 AND roasted_weight IS NOT NULL \
             THEN (green_weight - roasted_weight) / green_weight * 100.0 \
             ELSE weight_loss_pct END WHERE id = ? RETURNING *",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?
    .pop();
    tx.commit().await?;
    Ok(session)
}

/// Start a planned roast. It keeps its roaster's declared channels as they are now
/// (docs/channels.md, migration 049): `[]` for none, so a later declaration does not
/// reach back.
pub async fn start_session(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let session = sqlx::query_as::<_, RoastSession>(
        r"
            UPDATE roast_sessions
            SET status = ?, start_time = ?, updated_at = ?,
                channels = COALESCE((SELECT s.channels FROM roaster_setup s
                                     WHERE s.device_id = roast_sessions.device_id), '[]')
            WHERE id = ? AND status = ?
            RETURNING *
            ",
    )
    .bind(SessionStatus::Active.to_string())
    .bind(Utc::now())
    .bind(Utc::now())
    .bind(id)
    .bind(SessionStatus::Planning.to_string())
    .fetch_all(db)
    .await?
    .pop();

    Ok(session)
}

pub async fn pause_session(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let session = sqlx::query_as::<_, RoastSession>(
        r"
            UPDATE roast_sessions 
            SET status = ?, updated_at = ?
            WHERE id = ? AND status = ?
            RETURNING *
            ",
    )
    .bind(SessionStatus::Paused.to_string())
    .bind(Utc::now())
    .bind(id)
    .bind(SessionStatus::Active.to_string())
    .fetch_all(db)
    .await?
    .pop();

    Ok(session)
}

pub async fn resume_session(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let session = sqlx::query_as::<_, RoastSession>(
        r"
            UPDATE roast_sessions 
            SET status = ?, updated_at = ?
            WHERE id = ? AND status = ?
            RETURNING *
            ",
    )
    .bind(SessionStatus::Active.to_string())
    .bind(Utc::now())
    .bind(id)
    .bind(SessionStatus::Paused.to_string())
    .fetch_all(db)
    .await?
    .pop();

    Ok(session)
}

pub async fn complete_session(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let now = Utc::now();

    // Fetch the session first to get green_weight / roasted_weight
    let Some(existing) = get_session(db, id).await? else {
        return Ok(None);
    };
    let st = crate::roasts::stats::roast_stats(db, id, &existing).await?;

    let session = sqlx::query_as::<_, RoastSession>(
        r"
            UPDATE roast_sessions
            SET status = ?, end_time = ?, updated_at = ?,
                total_time_seconds = ?, max_temp = ?, max_ror = ?,
                first_crack_time = COALESCE(?, first_crack_time),
                development_time_ratio = COALESCE(?, development_time_ratio),
                weight_loss_pct = ?,
                avg_ror_drying = ?, avg_ror_maillard = ?, avg_ror_development = ?,
                drying_end_time = ?, drying_end_temp = ?,
                auc_value = ?,
                profile_rmse = ?, profile_max_dev = ?, profile_bias = ?
            WHERE id = ? AND status IN (?, ?)
            RETURNING *
            ",
    )
    .bind(SessionStatus::Completed.to_string())
    .bind(now)
    .bind(now)
    .bind(st.total_time_seconds)
    .bind(st.max_temp)
    .bind(st.max_ror)
    .bind(st.first_crack_time)
    .bind(st.development_time_ratio)
    .bind(st.weight_loss_pct)
    .bind(st.avg_ror_drying)
    .bind(st.avg_ror_maillard)
    .bind(st.avg_ror_development)
    .bind(st.drying_end_time)
    .bind(st.drying_end_temp)
    .bind(st.auc_value)
    .bind(st.profile_rmse)
    .bind(st.profile_max_dev)
    .bind(st.profile_bias)
    .bind(id)
    .bind(SessionStatus::Active.to_string())
    .bind(SessionStatus::Paused.to_string())
    .fetch_all(db)
    .await?
    .pop();

    Ok(session)
}

/// A finished roast's statistics again, from its events and telemetry, after a landmark
/// was added, moved or removed (e.g. first crack moved off a lone early crack). Status and
/// end time stay. `None` for a roast that is not finished: Complete computes them.
pub async fn recompute_statistics(db: &SqlitePool, id: &str) -> Result<Option<RoastSession>> {
    let existing = match get_session(db, id).await? {
        Some(s) if s.status == SessionStatus::Completed => s,
        _ => return Ok(None),
    };
    let st = crate::roasts::stats::roast_stats(db, id, &existing).await?;
    // The events decide on a finished roast: a removed first crack clears its time and DTR
    let session = sqlx::query_as::<_, RoastSession>(
        r"
            UPDATE roast_sessions
            SET updated_at = ?,
                total_time_seconds = ?, max_temp = ?, max_ror = ?,
                first_crack_time = ?,
                development_time_ratio = ?,
                weight_loss_pct = ?,
                avg_ror_drying = ?, avg_ror_maillard = ?, avg_ror_development = ?,
                drying_end_time = ?, drying_end_temp = ?,
                auc_value = ?,
                profile_rmse = ?, profile_max_dev = ?, profile_bias = ?
            WHERE id = ? AND status = ?
            RETURNING *
            ",
    )
    .bind(Utc::now())
    .bind(st.total_time_seconds)
    .bind(st.max_temp)
    .bind(st.max_ror)
    .bind(st.first_crack_time)
    .bind(st.development_time_ratio)
    .bind(st.weight_loss_pct)
    .bind(st.avg_ror_drying)
    .bind(st.avg_ror_maillard)
    .bind(st.avg_ror_development)
    .bind(st.drying_end_time)
    .bind(st.drying_end_temp)
    .bind(st.auc_value)
    .bind(st.profile_rmse)
    .bind(st.profile_max_dev)
    .bind(st.profile_bias)
    .bind(id)
    .bind(SessionStatus::Completed.to_string())
    .fetch_all(db)
    .await?
    .pop();
    Ok(session)
}

pub async fn delete_session(db: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM roast_sessions WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;

    Ok(result.rows_affected() > 0)
}

// Utility functions
#[allow(dead_code)] // Read by the MQTT consumer
pub async fn get_active_session(db: &SqlitePool, device_id: &str) -> Result<Option<RoastSession>> {
    let session = sqlx::query_as::<_, RoastSession>(
        r"
            SELECT * FROM roast_sessions 
            WHERE device_id = ? AND status IN (?, ?)
            ORDER BY start_time DESC
            LIMIT 1
            ",
    )
    .bind(device_id)
    .bind(SessionStatus::Active.to_string())
    .bind(SessionStatus::Paused.to_string())
    .fetch_optional(db)
    .await?;

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasters::types::{CreateDeviceProfileRequest, CreateDeviceRequest, DeviceStatus};

    /// The other `RETURNING` writes (session lifecycle, devices, device profiles) must be
    /// committed when they return too, for the same reason as the landmarks above: they
    /// used `fetch_one` / `fetch_optional`, which leave the commit to the statement's reset.
    #[tokio::test]
    async fn returning_writes_are_visible_to_other_connections_at_once() {
        let dir = std::env::temp_dir().join(format!("roastwarden-ret-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let path = path.to_str().unwrap();
        let writer = crate::db::open(path).await.expect("open db");
        let reader = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{path}"))
            .await
            .expect("second pool");
        let status = |id: String| {
            let reader = reader.clone();
            async move {
                sqlx::query_scalar::<_, String>("SELECT status FROM roast_sessions WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&reader)
                    .await
                    .unwrap()
            }
        };

        let s = crate::roasts::sessions::create_session(
            &writer,
            CreateSessionRequest {
                bean_id: None,
                planned_roast_id: None,
                roast_purpose: None,
                name: "t".into(),
                device_id: "virtual_roaster".into(),
                profile_id: None,
                bean_origin: None,
                bean_variety: None,
                green_weight: None,
                target_roast_level: None,
                notes: None,
                ambient_temp: None,
                humidity: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            status(s.id.clone()).await.as_deref(),
            Some("planning"),
            "create"
        );
        crate::roasts::sessions::start_session(&writer, &s.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            status(s.id.clone()).await.as_deref(),
            Some("active"),
            "start"
        );
        crate::roasts::sessions::pause_session(&writer, &s.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            status(s.id.clone()).await.as_deref(),
            Some("paused"),
            "pause"
        );
        crate::roasts::sessions::resume_session(&writer, &s.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            status(s.id.clone()).await.as_deref(),
            Some("active"),
            "resume"
        );
        crate::roasts::sessions::complete_session(&writer, &s.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            status(s.id.clone()).await.as_deref(),
            Some("completed"),
            "complete"
        );

        let d = crate::roasters::registry::create_device(
            &writer,
            CreateDeviceRequest {
                name: "r".into(),
                device_id: "r1".into(),
                profile_id: None,
                description: None,
                location: None,
            },
        )
        .await
        .unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = ?")
            .bind(&d.id)
            .fetch_one(&reader)
            .await
            .unwrap();
        assert_eq!(n, 1, "device not visible yet");
        crate::roasters::registry::update_device_status(&writer, &d.id, DeviceStatus::Active)
            .await
            .unwrap();
        let st: String = sqlx::query_scalar("SELECT status FROM devices WHERE id = ?")
            .bind(&d.id)
            .fetch_one(&reader)
            .await
            .unwrap();
        assert_eq!(
            st,
            DeviceStatus::Active.to_string(),
            "device status not visible yet"
        );
        let p = crate::roasters::registry::create_device_profile(
            &writer,
            CreateDeviceProfileRequest {
                name: "p".into(),
                description: None,
                default_control_mode: None,
                default_setpoint: None,
                default_fan: None,
                default_kp: None,
                default_ki: None,
                default_kd: None,
                max_temp: None,
                min_fan: None,
                telemetry_interval_ms: None,
            },
        )
        .await
        .unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_profiles WHERE id = ?")
            .bind(&p.id)
            .fetch_one(&reader)
            .await
            .unwrap();
        assert_eq!(n, 1, "device profile not visible yet");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Entering the roasted weight on a finished roast computes weight loss at once (it
    /// stayed empty until a second save: the UPDATE's CASE read the old, NULL weight).
    #[tokio::test]
    async fn first_roasted_weight_sets_weight_loss() {
        let db = crate::db::open_memory().await;
        let session = crate::roasts::sessions::create_session(
            &db,
            CreateSessionRequest {
                bean_id: None,
                planned_roast_id: None,
                roast_purpose: None,
                name: "w".to_string(),
                device_id: "virtual_roaster".to_string(),
                profile_id: None,
                bean_origin: None,
                bean_variety: None,
                green_weight: Some(100.0),
                target_roast_level: None,
                notes: None,
                ambient_temp: None,
                humidity: None,
            },
        )
        .await
        .unwrap();
        let updated = crate::roasts::sessions::update_session(
            &db,
            &session.id,
            UpdateSessionRequest {
                roasted_weight: Some(86.0),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.roasted_weight, Some(86.0));
        let loss = updated.weight_loss_pct.expect("weight loss computed");
        assert!((loss - 14.0).abs() < 1e-3, "{loss}");
    }

    #[tokio::test]
    async fn new_roast_takes_level_and_purpose_from_its_profile() {
        let db = crate::db::open_memory().await;
        let req = |level: Option<&str>| CreateSessionRequest {
            name: "Roast 1".into(),
            device_id: "virtual_roaster".into(),
            profile_id: Some("fluidbed-espresso".into()),
            target_roast_level: level.map(Into::into),
            ..Default::default()
        };
        let s = crate::roasts::sessions::create_session(&db, req(None))
            .await
            .unwrap();
        assert_eq!(s.target_roast_level.as_deref(), Some("light"));
        assert_eq!(s.roast_purpose.as_deref(), Some("espresso"));
        // What the request says wins.
        let s = crate::roasts::sessions::create_session(&db, req(Some("medium")))
            .await
            .unwrap();
        assert_eq!(s.target_roast_level.as_deref(), Some("medium"));
    }
}
