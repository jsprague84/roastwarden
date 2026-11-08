//! A profile's completed roasts, with the landmark times and temperatures roast guidance judges
//! first crack, drop and total time against.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::roasts::types::{RoastEvent, RoastEventType, RoastSession};

/// One completed roast of a profile, as `GET /api/profiles/{id}/history`
/// returns it. Times are seconds from CHARGE; temperatures °C.
#[derive(Debug, Clone, serde::Serialize, PartialEq, utoipa::ToSchema)]
pub struct ProfileHistoryRoast {
    pub session_id: String,
    pub name: String,
    pub device_id: String,
    pub batch_number: Option<i32>,
    pub roasted_at: DateTime<Utc>,
    pub fc_s: Option<f32>,
    pub fc_bt: Option<f32>,
    pub drop_s: Option<f32>,
    pub drop_bt: Option<f32>,
    pub dry_s: Option<f32>,
    pub dry_bt: Option<f32>,
    pub development_s: Option<f32>,
    pub delta_t_fc_drop: Option<f32>,
    pub total_s: Option<f32>,
    pub weight_loss_pct: Option<f32>,
    pub color_ground: Option<f32>,
    pub color_scale: Option<String>,
    pub rmse: Option<f32>,
}

impl ProfileHistoryRoast {
    pub(crate) fn from_roast(s: &RoastSession, events: &[RoastEvent]) -> Self {
        let find = |t: RoastEventType| events.iter().find(|e| e.event_type == t);
        let charge = find(RoastEventType::Charge).map_or(0.0, |e| e.elapsed_seconds);
        let at = |e: Option<&RoastEvent>| e.map(|e| e.elapsed_seconds - charge);
        let temp = |e: Option<&RoastEvent>| e.and_then(|e| e.temperature);
        let (fc, drop, dry) = (
            find(RoastEventType::FirstCrackStart),
            find(RoastEventType::Drop),
            find(RoastEventType::DryingEnd),
        );
        let weight_loss_pct = s
            .weight_loss_pct
            .or(match (s.green_weight, s.roasted_weight) {
                (Some(g), Some(r)) if g > 0.0 && r > 0.0 => Some((g - r) / g * 100.0),
                _ => None,
            });
        Self {
            session_id: s.id.clone(),
            name: s.name.clone(),
            device_id: s.device_id.clone(),
            batch_number: s.batch_number,
            roasted_at: s.start_time.unwrap_or(s.created_at),
            fc_s: at(fc),
            fc_bt: temp(fc),
            drop_s: at(drop),
            drop_bt: temp(drop),
            dry_s: at(dry),
            dry_bt: temp(dry),
            development_s: at(drop).zip(at(fc)).map(|(d, f)| d - f),
            delta_t_fc_drop: temp(drop).zip(temp(fc)).map(|(d, f)| d - f),
            total_s: at(drop).or(s.total_time_seconds.map(|t| t as f32)),
            weight_loss_pct,
            color_ground: s.color_ground,
            color_scale: s.color_scale.clone(),
            rmse: s.profile_rmse,
        }
    }
}

/// A profile's completed roasts on one roaster (or all), newest first, at
/// most 20, with landmark times from CHARGE and temperatures: what roast
/// guidance judges FC, drop and total time against (docs/roast-guidance.md).
pub async fn profile_history(
    db: &SqlitePool,
    profile_id: &str,
    device_id: Option<&str>,
) -> Result<Vec<ProfileHistoryRoast>> {
    let sessions = sqlx::query_as::<_, RoastSession>(
        "SELECT * FROM roast_sessions
             WHERE profile_id = ? AND status = 'completed' AND (? IS NULL OR device_id = ?)
             ORDER BY COALESCE(start_time, created_at) DESC LIMIT 20",
    )
    .bind(profile_id)
    .bind(device_id)
    .bind(device_id)
    .fetch_all(db)
    .await?;
    let mut out = Vec::with_capacity(sessions.len());
    for s in sessions {
        let events = crate::roasts::events::get_roast_events(db, &s.id).await?;
        out.push(ProfileHistoryRoast::from_roast(&s, &events));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasts::types::{CreateRoastEventRequest, CreateSessionRequest};

    #[tokio::test]
    async fn profile_history_lists_completed_roasts_on_that_roaster() {
        let db = crate::db::open_memory().await;
        let mut ids = Vec::new();
        for (device, status) in [
            ("esp32_roaster_01", "completed"),
            ("esp32_roaster_01", "cancelled"),
            ("virtual_roaster", "completed"),
        ] {
            let s = crate::roasts::sessions::create_session(
                &db,
                CreateSessionRequest {
                    name: "Roast".into(),
                    device_id: device.into(),
                    profile_id: Some("fluidbed-espresso".into()),
                    green_weight: Some(100.0),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            for (kind, t, temp) in [
                (RoastEventType::Charge, 5.4, 25.0),
                (RoastEventType::DryingEnd, 243.3, 150.0),
                (RoastEventType::FirstCrackStart, 473.1, 194.5),
                (RoastEventType::Drop, 719.2, 212.3),
            ] {
                crate::roasts::events::create_roast_event(
                    &db,
                    &s.id,
                    CreateRoastEventRequest {
                        event_type: kind,
                        elapsed_seconds: t,
                        temperature: Some(temp),
                        notes: None,
                    },
                )
                .await
                .unwrap();
            }
            sqlx::query("UPDATE roast_sessions SET status = ?, roasted_weight = 78.3 WHERE id = ?")
                .bind(status)
                .bind(&s.id)
                .execute(&db)
                .await
                .unwrap();
            ids.push(s.id);
        }

        let mine = crate::profiles::history::profile_history(
            &db,
            "fluidbed-espresso",
            Some("esp32_roaster_01"),
        )
        .await
        .unwrap();
        assert_eq!(mine.len(), 1, "cancelled and other roasters are left out");
        let r = &mine[0];
        assert_eq!(r.session_id, ids[0]);
        assert_eq!(r.batch_number, Some(1));
        let near = |a: Option<f32>, b: f32| (a.unwrap() - b).abs() < 0.01;
        assert!(near(r.fc_s, 467.7));
        assert!(near(r.drop_s, 713.8));
        assert!(near(r.total_s, 713.8));
        assert!(near(r.development_s, 246.1));
        assert!(near(r.delta_t_fc_drop, 17.8));
        assert!(near(r.dry_s, 237.9));
        assert!(near(r.weight_loss_pct, 21.7));

        let all = crate::profiles::history::profile_history(&db, "fluidbed-espresso", None)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(
            crate::profiles::history::profile_history(&db, "nope", None)
                .await
                .unwrap(),
            [] as [ProfileHistoryRoast; 0]
        );
    }
}
