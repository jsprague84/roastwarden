//! The statistics Complete stores, on roasts recorded into a memory database.

use super::*;
use crate::roasts::types::{
    CreateRoastEventRequest, CreateSessionRequest, SessionStatus, UpdateSessionRequest,
};

// ---- Session Completion Statistics Tests ----

#[tokio::test]
async fn landmark_times_count_from_charge() {
    let db = crate::db::open_memory().await;
    let session = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            name: "Charged late".to_string(),
            device_id: "esp32-001".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    crate::roasts::sessions::start_session(&db, &session.id)
        .await
        .unwrap();
    for i in 0..=62 {
        let t = i as f32 * 10.0;
        // Room temperature until CHARGE at 8 s, then RoR 12.
        let ror = if t < 8.0 { 0.0 } else { 12.0 };
        crate::roasts::telemetry_store::add_telemetry_point(
            &db,
            &session.id,
            t,
            Some(25.0 + t / 5.0),
            None,
            Some(ror),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    }
    for (event_type, at) in [
        (RoastEventType::Charge, 8.0),
        (RoastEventType::DryingEnd, 238.0),
        (RoastEventType::FirstCrackStart, 458.0),
        (RoastEventType::Drop, 608.0),
    ] {
        crate::roasts::events::create_roast_event(
            &db,
            &session.id,
            CreateRoastEventRequest {
                event_type,
                elapsed_seconds: at,
                temperature: None,
                notes: None,
            },
        )
        .await
        .unwrap();
    }
    let done = crate::roasts::sessions::complete_session(&db, &session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.total_time_seconds, Some(600));
    assert_eq!(
        done.first_crack_time,
        Some(450),
        "from CHARGE, as the roast time"
    );
    assert_eq!(done.drying_end_time, Some(230));
    assert!((done.development_time_ratio.unwrap() - 0.25).abs() < 1e-4);
    // The drying RoR starts at CHARGE: the pre-charge sample does not pull it down.
    assert!(
        (done.avg_ror_drying.unwrap() - 12.0).abs() < 0.1,
        "{:?}",
        done.avg_ror_drying
    );
}

#[tokio::test]
async fn test_complete_session_computes_dtr_and_weight_loss() {
    let db = crate::db::open_memory().await;

    // Create a session with green and roasted weights
    let session = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            bean_id: None,
            planned_roast_id: None,
            roast_purpose: None,
            name: "Stats Test Roast".to_string(),
            device_id: "esp32-001".to_string(),
            profile_id: None,
            bean_origin: Some("Ethiopia".to_string()),
            bean_variety: Some("Yirgacheffe".to_string()),
            green_weight: Some(200.0),
            target_roast_level: Some("medium".to_string()),
            notes: None,
            ambient_temp: None,
            humidity: None,
        },
    )
    .await
    .unwrap();

    // Start the session
    let session = crate::roasts::sessions::start_session(&db, &session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, SessionStatus::Active);

    // Set roasted weight via update
    crate::roasts::sessions::update_session(
        &db,
        &session.id,
        UpdateSessionRequest {
            name: None,
            roasted_weight: Some(170.0),
            notes: None,
            first_crack_time: None,
            development_time_ratio: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Add telemetry points spanning 600 seconds with varying RoR
    for i in 0..=600 {
        if i % 10 == 0 {
            let elapsed = i as f32;
            let bean_temp = 100.0 + (elapsed / 600.0) * 120.0; // 100→220°C
            let ror = if elapsed < 200.0 {
                15.0 // drying phase
            } else if elapsed < 400.0 {
                12.0 // maillard phase
            } else {
                8.0 // development phase
            };
            crate::roasts::telemetry_store::add_telemetry_point(
                &db,
                &session.id,
                elapsed,
                Some(bean_temp),
                Some(bean_temp + 20.0),
                Some(ror),
                Some(80.0),
                Some(78.4),
                Some(bean_temp),
            )
            .await
            .unwrap();
        }
    }

    // Add roast events for phase boundaries
    crate::roasts::events::create_roast_event(
        &db,
        &session.id,
        CreateRoastEventRequest {
            event_type: RoastEventType::DryingEnd,
            elapsed_seconds: 200.0,
            temperature: Some(140.0),
            notes: None,
        },
    )
    .await
    .unwrap();

    crate::roasts::events::create_roast_event(
        &db,
        &session.id,
        CreateRoastEventRequest {
            event_type: RoastEventType::FirstCrackStart,
            elapsed_seconds: 400.0,
            temperature: Some(200.0),
            notes: None,
        },
    )
    .await
    .unwrap();

    // Complete the session
    let completed = crate::roasts::sessions::complete_session(&db, &session.id)
        .await
        .unwrap()
        .expect("session should be returned");

    assert_eq!(completed.status, SessionStatus::Completed);

    // DTR: (600 - 400) / 600 = 200/600 ≈ 0.333
    let dtr = completed.development_time_ratio.unwrap();
    assert!(
        (dtr - 0.333).abs() < 0.01,
        "DTR should be ~0.333, got {dtr}"
    );

    // Weight loss: (200 - 170) / 200 * 100 = 15%
    let wl = completed.weight_loss_pct.unwrap();
    assert!(
        (wl - 15.0).abs() < 0.1,
        "Weight loss should be 15%, got {wl}"
    );

    // Max RoR should be 15.0
    let max_ror = completed.max_ror.unwrap();
    assert!(
        (max_ror - 15.0).abs() < 0.1,
        "Max RoR should be 15.0, got {max_ror}"
    );

    // First crack time should be 400 seconds
    assert_eq!(completed.first_crack_time, Some(400));

    // Drying end fields
    assert_eq!(completed.drying_end_time, Some(200));
    assert_eq!(completed.drying_end_temp, Some(140.0));

    // Phase avg RoR values
    let avg_dry = completed.avg_ror_drying.unwrap();
    assert!(
        (avg_dry - 15.0).abs() < 0.1,
        "Avg RoR drying should be ~15.0, got {avg_dry}"
    );

    let avg_mail = completed.avg_ror_maillard.unwrap();
    assert!(
        (avg_mail - 12.0).abs() < 0.1,
        "Avg RoR maillard should be ~12.0, got {avg_mail}"
    );

    let avg_dev = completed.avg_ror_development.unwrap();
    assert!(
        (avg_dev - 8.0).abs() < 0.1,
        "Avg RoR development should be ~8.0, got {avg_dev}"
    );
}

#[tokio::test]
async fn test_complete_session_computes_auc() {
    let db = crate::db::open_memory().await;

    // Create and start a session
    let session = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            bean_id: None,
            planned_roast_id: None,
            roast_purpose: None,
            name: "AUC Test Roast".to_string(),
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
    crate::roasts::sessions::start_session(&db, &session.id)
        .await
        .unwrap();

    // 3-point linear curve: (0s, 100°C), (60s, 100°C), (120s, 200°C)
    // With base_temp=0 (default):
    //   Segment 1: dt=60, avg_temp=(100+100)/2=100 → area = 60*100 = 6000 °C·s
    //   Segment 2: dt=60, avg_temp=(100+200)/2=150 → area = 60*150 = 9000 °C·s
    //   Total = 15000 °C·s = 250 °C·min
    crate::roasts::telemetry_store::add_telemetry_point(
        &db,
        &session.id,
        0.0,
        Some(100.0),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    crate::roasts::telemetry_store::add_telemetry_point(
        &db,
        &session.id,
        60.0,
        Some(100.0),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    crate::roasts::telemetry_store::add_telemetry_point(
        &db,
        &session.id,
        120.0,
        Some(200.0),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let completed = crate::roasts::sessions::complete_session(&db, &session.id)
        .await
        .unwrap()
        .expect("session should be returned");

    let auc = completed.auc_value.unwrap();
    assert!(
        (auc - 250.0).abs() < 0.1,
        "AUC should be 250.0 °C·min, got {auc}"
    );
}

/// AUC runs from CHARGE to DROP: the minutes recorded before the charge and the cooling
/// after the drop do not count (they did: 2000-odd °C·min on a real roast).
#[test]
fn auc_runs_from_charge_to_drop() {
    let pts: Vec<(f64, f64)> = (0..=300).map(|t| (t as f64, 100.0)).collect();
    // 60 s .. 180 s at 100 °C = 200 °C·min
    let a = auc_between(&pts, 60.0, 180.0, 0.0).unwrap();
    assert!((a - 200.0).abs() < 1e-9, "{a}");
    // Base 50: half of it.
    assert!((auc_between(&pts, 60.0, 180.0, 50.0).unwrap() - 100.0).abs() < 1e-9);
    // A ramp 0 → 120 °C over 2 min, base 60: a triangle of 60 °C × 1 min / 2.
    let ramp = [(0.0, 0.0), (120.0, 120.0)];
    assert!((auc_between(&ramp, 0.0, 120.0, 60.0).unwrap() - 30.0).abs() < 1e-9);
    assert_eq!(auc_between(&pts, 400.0, 500.0, 0.0), None);
}

#[tokio::test]
async fn completed_auc_ignores_before_charge_and_after_drop() {
    let db = crate::db::open_memory().await;
    let session = crate::roasts::sessions::create_session(
        &db,
        CreateSessionRequest {
            name: "auc".into(),
            device_id: "virtual_roaster".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    crate::roasts::sessions::start_session(&db, &session.id)
        .await
        .unwrap();
    for t in 0..=240 {
        crate::roasts::telemetry_store::add_telemetry_point(
            &db,
            &session.id,
            t as f32,
            Some(100.0),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    }
    for (ty, at) in [
        (RoastEventType::Charge, 60.0),
        (RoastEventType::Drop, 180.0),
    ] {
        crate::roasts::events::create_roast_event(
            &db,
            &session.id,
            CreateRoastEventRequest {
                event_type: ty,
                elapsed_seconds: at,
                temperature: Some(100.0),
                notes: None,
            },
        )
        .await
        .unwrap();
    }
    let done = crate::roasts::sessions::complete_session(&db, &session.id)
        .await
        .unwrap()
        .unwrap();
    let auc = done.auc_value.unwrap();
    assert!(
        (auc - 200.0).abs() < 0.1,
        "AUC from CHARGE to DROP is 200, got {auc}"
    );
}

#[test]
fn tracking_stats_compare_bt_with_the_profile_where_it_exists() {
    let curve = vec![(0.0, 100.0), (600.0, 200.0)];
    // BT 2 °C under the profile from 60 s to 700 s.
    let samples: Vec<(f64, f64)> = (0..=700)
        .map(|t| {
            let t = t as f64;
            (t, 100.0 + t / 6.0 - 2.0)
        })
        .collect();
    let (rmse, max, bias) = tracking_stats(&samples, &curve, 60.0, 650.0).unwrap();
    assert!((rmse - 2.0).abs() < 1e-9);
    assert!((max - 2.0).abs() < 1e-9);
    assert!((bias + 2.0).abs() < 1e-9);
    // Too few points: none.
    assert!(tracking_stats(&samples[..5], &curve, 0.0, 10.0).is_none());
    assert!(tracking_stats(&samples, &[], 0.0, 600.0).is_none());
}
