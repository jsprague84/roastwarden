//! A roast's stored telemetry (`session_telemetry`): a sample written as it arrives, read back
//! whole for the roast page, the exports and the statistics.

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::roasts::types::SessionTelemetry;

// Telemetry Management
#[allow(clippy::too_many_arguments)]
pub async fn add_telemetry_point(
    db: &SqlitePool,
    session_id: &str,
    elapsed_seconds: f32,
    bean_temp: Option<f32>,
    env_temp: Option<f32>,
    rate_of_rise: Option<f32>,
    heater: Option<f64>,
    fan: Option<f64>,
    setpoint: Option<f32>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r"
            INSERT INTO session_telemetry (
                id, session_id, timestamp, elapsed_seconds, bean_temp, env_temp,
                rate_of_rise, heater, fan, setpoint
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
    )
    .bind(id)
    .bind(session_id)
    .bind(Utc::now())
    .bind(elapsed_seconds)
    .bind(bean_temp)
    .bind(env_temp)
    .bind(rate_of_rise)
    .bind(heater)
    .bind(fan)
    .bind(setpoint)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn get_session_telemetry(
    db: &SqlitePool,
    session_id: &str,
) -> Result<Vec<SessionTelemetry>> {
    let telemetry = sqlx::query_as::<_, SessionTelemetry>(
        "SELECT * FROM session_telemetry WHERE session_id = ? ORDER BY elapsed_seconds",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;

    Ok(telemetry)
}
