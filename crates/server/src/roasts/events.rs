//! A roast's landmarks (`roast_events`): CHARGE, dry end, first crack, DROP and the rest, as
//! marked by hand, by the roast sequence or by an auto event.

use anyhow::{Result, anyhow};
use chrono::Utc;
use sqlx::AssertSqlSafe;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::roasts::types::{CreateRoastEventRequest, RoastEvent, UpdateRoastEventRequest};

// Roast Events CRUD operations
pub async fn create_roast_event(
    db: &SqlitePool,
    session_id: &str,
    req: CreateRoastEventRequest,
) -> Result<RoastEvent> {
    let event_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let event = sqlx::query_as::<_, RoastEvent>(
            r"
            INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, notes, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            RETURNING *
            "
        )
        .bind(&event_id)
        .bind(session_id)
        .bind(req.event_type.to_string())
        .bind(req.elapsed_seconds)
        .bind(req.temperature)
        .bind(&req.notes)
        .bind(now)
        // fetch_all, not fetch_one: sqlx-sqlite stops stepping a RETURNING
        // statement after the first row, and the implicit commit lands only
        // when the statement is reset. Dashboards reload the landmarks the
        // moment they hear about the change, and read the old list in that
        // gap (a DROP marked on a phone did not reach the laptop).
        .fetch_all(db)
        .await?
        .pop()
        .ok_or_else(|| anyhow!("insert returned no row"))?;

    Ok(event)
}

pub async fn get_roast_events(db: &SqlitePool, session_id: &str) -> Result<Vec<RoastEvent>> {
    let events = sqlx::query_as::<_, RoastEvent>(
        "SELECT * FROM roast_events WHERE session_id = ?1 ORDER BY elapsed_seconds ASC",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;

    Ok(events)
}

pub async fn update_roast_event(
    db: &SqlitePool,
    event_id: &str,
    req: UpdateRoastEventRequest,
) -> Result<Option<RoastEvent>> {
    let mut updates = Vec::new();

    if req.elapsed_seconds.is_some() {
        updates.push("elapsed_seconds = ?".to_string());
    }

    if req.temperature.is_some() {
        updates.push("temperature = ?".to_string());
    }

    if req.notes.is_some() {
        updates.push("notes = ?".to_string());
    }

    if updates.is_empty() {
        return Err(anyhow::anyhow!("No fields to update"));
    }

    // roast_events has no updated_at column
    let query = format!(
        "UPDATE roast_events SET {} WHERE id = ? RETURNING *",
        updates.join(", ")
    );

    let mut query_builder = sqlx::query_as::<_, RoastEvent>(AssertSqlSafe(query.as_str()));

    // Bind parameters in the same order as they appear in the updates
    if let Some(elapsed_seconds) = req.elapsed_seconds {
        query_builder = query_builder.bind(elapsed_seconds);
    }
    if let Some(temperature) = req.temperature {
        query_builder = query_builder.bind(temperature);
    }
    if let Some(notes) = req.notes {
        query_builder = query_builder.bind(notes);
    }

    // Bind the event_id for the WHERE clause
    query_builder = query_builder.bind(event_id);

    // fetch_all: see create_roast_event (commit before dashboards reload).
    Ok(query_builder.fetch_all(db).await?.pop())
}

pub async fn delete_roast_event(db: &SqlitePool, event_id: &str) -> Result<()> {
    let rows_affected = sqlx::query("DELETE FROM roast_events WHERE id = ?1")
        .bind(event_id)
        .execute(db)
        .await?
        .rows_affected();

    if rows_affected == 0 {
        return Err(anyhow::anyhow!("Roast event not found"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasts::types::{RoastEventType, RoastSession};

    /// A landmark must be readable from another connection the moment
    /// `create_roast_event` returns: the API then tells every dashboard to
    /// reload, and a phone's DROP did not reach a laptop when the insert's
    /// commit trailed the broadcast.
    #[tokio::test]
    async fn created_event_is_visible_to_other_connections_at_once() {
        let dir = std::env::temp_dir().join(format!("roastwarden-evt-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let path = path.to_str().unwrap();
        let writer = crate::db::open(path).await.expect("open db");
        let reader = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{path}"))
            .await
            .expect("second pool");
        // Plain INSERT: sqlx 0.7 mis-sizes `RETURNING *` rows on a connection
        // that just ran the ALTER TABLE migrations.
        let sid = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO roast_sessions (id, name, device_id, status, created_at) VALUES (?, 't', 'virtual_roaster', 'active', CURRENT_TIMESTAMP)")
            .bind(&sid)
            .execute(&writer)
            .await
            .unwrap();

        for (i, t) in [RoastEventType::Charge, RoastEventType::Drop]
            .into_iter()
            .enumerate()
        {
            crate::roasts::events::create_roast_event(
                &writer,
                &sid,
                CreateRoastEventRequest {
                    event_type: t,
                    elapsed_seconds: i as f32 * 600.0,
                    temperature: None,
                    notes: None,
                },
            )
            .await
            .unwrap();
            let n: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM roast_events WHERE session_id = ?")
                    .bind(&sid)
                    .fetch_one(&reader)
                    .await
                    .unwrap();
            assert_eq!(
                n,
                i as i64 + 1,
                "event {i} not visible to another connection yet"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The raw quirk behind the two tests around it: under sqlx 0.7 an
    /// `INSERT … RETURNING` row read with `fetch_one` kept its statement open, so the
    /// autocommit landed only when the row was dropped and another connection did not see
    /// the row yet (20 of 20 tries). sqlx 0.8 commits at once; this keeps it so. Also covers
    /// `RETURNING *` into a typed row on the pool that just ran the ALTER migrations, which
    /// mis-sized rows under 0.7.
    #[tokio::test]
    async fn returning_row_from_fetch_one_is_committed_at_once() {
        let dir = std::env::temp_dir().join(format!("roastwarden-ret1-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let path = path.to_str().unwrap();
        let writer = crate::db::open(path).await.expect("open db");
        let reader = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{path}"))
            .await
            .expect("second pool");
        for i in 0..20 {
            let row: RoastSession = sqlx::query_as(
                "INSERT INTO roast_sessions (id, name, device_id, status, created_at) \
                 VALUES (?, 't', 'virtual_roaster', 'planning', CURRENT_TIMESTAMP) RETURNING *",
            )
            .bind(Uuid::new_v4().to_string())
            .fetch_one(&writer)
            .await
            .unwrap();
            let seen: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roast_sessions WHERE id = ?")
                .bind(&row.id)
                .fetch_one(&reader)
                .await
                .unwrap();
            assert_eq!(
                seen, 1,
                "try {i}: the row was not committed when fetch_one returned"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
