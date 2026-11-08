//! Fixtures for the tests of several modules: a roast recorded into a memory database.

use sqlx::SqlitePool;

use crate::roasts::types::{
    CreateRoastEventRequest, CreateSessionRequest, RoastEventType, RoastSession,
};

// ---- Profile model v2 ----

pub(crate) async fn recorded_session(db: &SqlitePool) -> RoastSession {
    let session = crate::roasts::sessions::create_session(
        db,
        CreateSessionRequest {
            bean_id: None,
            planned_roast_id: None,
            roast_purpose: None,
            name: "Kenya AA".to_string(),
            device_id: "esp32-001".to_string(),
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
    crate::roasts::sessions::start_session(db, &session.id)
        .await
        .unwrap();
    // Charge at 20 s; fan 80 % then 60 % from 200 s.
    for i in 0..=560 {
        let r = i as f32 - 20.0;
        let bt = if r < 0.0 {
            190.0
        } else if r < 60.0 {
            190.0 - r * 1.5
        } else {
            100.0 + (r - 60.0) * 0.22
        };
        let fan = if r < 200.0 { 80.0 } else { 60.0 };
        crate::roasts::telemetry_store::add_telemetry_point(
            db,
            &session.id,
            i as f32,
            Some(bt),
            Some(bt + 35.0),
            Some(10.0),
            Some(75.0),
            Some(fan),
            None,
        )
        .await
        .unwrap();
    }
    for (kind, t) in [
        (RoastEventType::Charge, 20.0),
        (RoastEventType::FirstCrackStart, 440.0),
        (RoastEventType::Drop, 540.0),
    ] {
        crate::roasts::events::create_roast_event(
            db,
            &session.id,
            CreateRoastEventRequest {
                event_type: kind,
                elapsed_seconds: t,
                temperature: None,
                notes: None,
            },
        )
        .await
        .unwrap();
    }
    session
}
