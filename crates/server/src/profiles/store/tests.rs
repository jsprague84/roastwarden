//! Profiles stored and read back, their guidance fields and roaster kind, the seeds.

use super::*;
use crate::profiles::types::{
    CreateProfileEventRequest, ProfileAction, SaveSessionAsProfileRequest, TriggerKind,
};
use crate::profiles::validate::ProfileInputError;
use crate::roasts::testing::recorded_session;

// ---- Roast Profile CRUD Tests ----

#[tokio::test]
async fn test_profile_update_round_trip() {
    let db = crate::db::open_memory().await;

    // Create a profile with initial points
    let created = crate::profiles::store::create_profile(
        &db,
        CreateProfileRequest {
            name: "Original Profile".to_string(),
            description: Some("Initial description".to_string()),
            target_total_time: Some(600),
            target_first_crack: None,
            target_end_temp: Some(210.0),
            preheat_temp: None,
            charge_temp: Some(180.0),
            points: vec![
                CreateProfilePointRequest {
                    time_seconds: 0,
                    target_temp: 180.0,
                    fan_speed: Some(80),
                    notes: None,
                    target_env_temp: None,
                    heater_pct: None,
                },
                CreateProfilePointRequest {
                    time_seconds: 300,
                    target_temp: 200.0,
                    fan_speed: None,
                    notes: None,
                    target_env_temp: None,
                    heater_pct: None,
                },
            ],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(created.profile.name, "Original Profile");
    assert_eq!(created.points.len(), 2);

    // Update the profile with new metadata and different points
    let updated = crate::profiles::store::update_profile(
        &db,
        &created.profile.id,
        CreateProfileRequest {
            name: "Updated Profile".to_string(),
            description: Some("Updated description".to_string()),
            target_total_time: Some(720),
            target_first_crack: Some(360),
            target_end_temp: Some(220.0),
            preheat_temp: None,
            charge_temp: Some(185.0),
            points: vec![
                CreateProfilePointRequest {
                    time_seconds: 0,
                    target_temp: 185.0,
                    fan_speed: Some(90),
                    notes: None,
                    target_env_temp: None,
                    heater_pct: None,
                },
                CreateProfilePointRequest {
                    time_seconds: 200,
                    target_temp: 195.0,
                    fan_speed: None,
                    notes: None,
                    target_env_temp: None,
                    heater_pct: None,
                },
                CreateProfilePointRequest {
                    time_seconds: 500,
                    target_temp: 220.0,
                    fan_speed: Some(60),
                    notes: Some("Finish".to_string()),
                    target_env_temp: None,
                    heater_pct: None,
                },
            ],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let updated = updated.expect("Profile should exist");
    assert_eq!(updated.profile.name, "Updated Profile");
    assert_eq!(
        updated.profile.description,
        Some("Updated description".to_string())
    );
    assert_eq!(updated.profile.target_total_time, Some(720));
    assert_eq!(updated.profile.target_first_crack, Some(360));
    assert_eq!(updated.profile.charge_temp, Some(185.0));
    assert_eq!(updated.points.len(), 3);
    assert_eq!(updated.points[0].target_temp, 185.0);
    assert_eq!(updated.points[2].notes, Some("Finish".to_string()));

    // Verify old points were replaced (not accumulated)
    let fetched = crate::profiles::store::get_profile_with_points(&db, &created.profile.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.points.len(), 3);

    // Update non-existent profile returns None
    let missing = crate::profiles::store::update_profile(
        &db,
        "nonexistent-id",
        CreateProfileRequest {
            name: "Ghost".to_string(),
            description: None,
            target_total_time: None,
            target_first_crack: None,
            target_end_temp: None,
            preheat_temp: None,
            charge_temp: None,
            points: vec![],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert!(missing.is_none());
}

#[tokio::test]
async fn test_profile_update_keeps_or_replaces_events_and_curves() {
    let db = crate::db::open_memory().await;
    let session = recorded_session(&db).await;
    let p = crate::profiles::extract::save_session_as_profile(
        &db,
        &session.id,
        SaveSessionAsProfileRequest {
            name: Some("Base".into()),
            description: None,
        },
    )
    .await
    .unwrap()
    .unwrap();
    let n_events = p.events.len();
    assert!(n_events >= 3);
    let points: Vec<CreateProfilePointRequest> = p
        .points
        .iter()
        .map(|pt| CreateProfilePointRequest {
            time_seconds: pt.time_seconds,
            target_temp: pt.target_temp,
            heater_pct: pt.heater_pct,
            ..Default::default()
        })
        .collect();

    // Designer-style update without events/curves keeps both.
    let kept = crate::profiles::store::update_profile(
        &db,
        &p.profile.id,
        CreateProfileRequest {
            name: "Base 2".into(),
            points: points.clone(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(kept.events.len(), n_events);
    assert!(kept.curves.is_some());
    assert_eq!(kept.points[0].heater_pct, Some(75));

    // Explicit events replace; curves: null removes.
    let replaced = crate::profiles::store::update_profile(
        &db,
        &p.profile.id,
        CreateProfileRequest {
            name: "Base 3".into(),
            points,
            events: Some(vec![CreateProfileEventRequest {
                trigger_kind: Some(TriggerKind::Bt),
                at_bt: Some(150.0),
                action: Some(ProfileAction::Mode),
                label: Some("Auto".into()),
                ..Default::default()
            }]),
            curves: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(replaced.events.len(), 1);
    assert_eq!(replaced.events[0].label.as_deref(), Some("auto"));
    assert!(replaced.curves.is_none());

    // Bad events are a ProfileInputError and change nothing.
    let err = crate::profiles::store::update_profile(
        &db,
        &p.profile.id,
        CreateProfileRequest {
            name: "Bad".into(),
            events: Some(vec![CreateProfileEventRequest {
                at_seconds: Some(30.0),
                action: Some(ProfileAction::Fan),
                value: Some(140.0),
                ..Default::default()
            }]),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(err.downcast_ref::<ProfileInputError>().is_some(), "{err}");
    let after = crate::profiles::store::get_profile_with_points(&db, &p.profile.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.profile.name, "Base 3");
}

fn guidance_profile(
    level: Option<&str>,
    expectations: Option<serde_json::Value>,
) -> CreateProfileRequest {
    CreateProfileRequest {
        name: "Guided".into(),
        roast_level: level.map(|l| Some(l.to_string())),
        roast_purpose: Some(Some("espresso".into())),
        expectations: expectations.map(Some),
        points: vec![
            CreateProfilePointRequest {
                time_seconds: 0,
                target_temp: 30.0,
                ..Default::default()
            },
            CreateProfilePointRequest {
                time_seconds: 600,
                target_temp: 210.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn profile_guidance_fields_validate_and_round_trip() {
    let db = crate::db::open_memory().await;
    let p = crate::profiles::store::create_profile(
        &db,
        guidance_profile(
            Some("light"),
            Some(serde_json::json!({"weight_loss_pct": [14, 16], "dtr_pct": null})),
        ),
    )
    .await
    .unwrap();
    let id = p.profile.id.clone();
    assert_eq!(p.profile.roast_level.as_deref(), Some("light"));
    assert_eq!(p.profile.roast_purpose.as_deref(), Some("espresso"));
    assert_eq!(
        p.profile.expectations.as_ref().unwrap().0,
        serde_json::json!({"weight_loss_pct": [14.0, 16.0]})
    );

    // Absent keeps, null clears.
    let mut keep = guidance_profile(None, None);
    keep.roast_purpose = None;
    let kept = crate::profiles::store::update_profile(&db, &id, keep)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.profile.roast_level.as_deref(), Some("light"));
    assert_eq!(kept.profile.roast_purpose.as_deref(), Some("espresso"));
    assert!(kept.profile.expectations.is_some());
    let mut clear = guidance_profile(None, Some(serde_json::Value::Null));
    clear.roast_level = Some(None);
    let cleared = crate::profiles::store::update_profile(&db, &id, clear)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cleared.profile.roast_level, None);
    assert!(cleared.profile.expectations.is_none());

    // Bad input is a ProfileInputError (400), not a 500.
    for bad in [
        guidance_profile(Some("blonde"), None),
        guidance_profile(Some("light"), Some(serde_json::json!({"colour": [1, 2]}))),
        guidance_profile(
            Some("light"),
            Some(serde_json::json!({"dtr_pct": [20, 12]})),
        ),
        guidance_profile(Some("light"), Some(serde_json::json!({"dtr_pct": [12]}))),
        guidance_profile(Some("light"), Some(serde_json::json!([1, 2]))),
    ] {
        let err = crate::profiles::store::create_profile(&db, bad)
            .await
            .unwrap_err();
        assert!(err.downcast_ref::<ProfileInputError>().is_some(), "{err}");
    }
}

#[tokio::test]
async fn seed_profiles_name_their_roaster_kind_and_the_drum_seeds_are_whole() {
    let db = crate::db::open_memory().await;
    let all = crate::profiles::store::list_profiles(&db, true)
        .await
        .unwrap();
    for p in all.iter().filter(|p| p.id.starts_with("fluidbed-")) {
        assert_eq!(
            p.roaster_kind.as_deref(),
            Some("fluid_bed_electric"),
            "{}",
            p.id
        );
    }
    // The legacy generic profiles are for no kind in particular.
    assert!(
        all.iter()
            .filter(|p| p.id.starts_with("default-"))
            .all(|p| p.roaster_kind.is_none())
    );
    for (id, level, purpose) in [
        ("drum-light-filter", "light", "filter"),
        ("drum-medium-omni", "medium", "omni"),
    ] {
        let p = crate::profiles::store::get_profile_with_points(&db, id)
            .await
            .unwrap()
            .unwrap();
        let pr = &p.profile;
        assert_eq!(pr.roaster_kind.as_deref(), Some("drum_gas"), "{id}");
        assert_eq!(pr.roast_level.as_deref(), Some(level), "{id}");
        assert_eq!(pr.roast_purpose.as_deref(), Some(purpose), "{id}");
        let (fc, total) = (
            pr.target_first_crack.unwrap(),
            pr.target_total_time.unwrap(),
        );
        let dev = total - fc;
        let dtr = 100.0 * f64::from(dev) / f64::from(total);
        // Inside the level's development and DTR ranges (docs/roast-guidance.md).
        let (dev_range, dtr_range) = match level {
            "light" => (60..=120, 12.0..=20.0),
            _ => (120..=210, 18.0..=25.0),
        };
        assert!(dev_range.contains(&dev), "{id}: development {dev} s");
        assert!(dtr_range.contains(&dtr), "{id}: DTR {dtr}");
        // A preheated drum: BT falls to the turning point, then only rises.
        let bt = |t: i32| {
            p.points
                .iter()
                .find(|x| x.time_seconds == t)
                .map(|x| x.target_temp)
        };
        assert!(bt(0).unwrap() >= 195.0, "{id}: charged hot");
        let tp = p
            .points
            .iter()
            .min_by(|a, b| a.target_temp.total_cmp(&b.target_temp))
            .unwrap();
        assert!((60..=120).contains(&tp.time_seconds), "{id}: TP");
        assert!((90.0..=110.0).contains(&tp.target_temp), "{id}: TP BT");
        let after: Vec<_> = p
            .points
            .iter()
            .filter(|x| x.time_seconds >= tp.time_seconds)
            .collect();
        assert!(
            after
                .windows(2)
                .all(|w| w[1].target_temp > w[0].target_temp),
            "{id}"
        );
        assert!((bt(fc).unwrap() - 196.0).abs() < 0.2, "{id}: FC BT");
        assert!(
            (bt(total).unwrap() - pr.target_end_temp.unwrap()).abs() < 0.2,
            "{id}"
        );
        // Burner and air on every point, as percentages that only step down / up.
        let burner: Vec<i32> = p.points.iter().map(|x| x.heater_pct.unwrap()).collect();
        let air: Vec<i32> = p.points.iter().map(|x| x.fan_speed.unwrap()).collect();
        assert!(burner.iter().chain(&air).all(|v| (0..=100).contains(v)));
        assert!(
            burner.windows(2).all(|w| w[1] <= w[0]),
            "{id}: burner steps down"
        );
        assert!(air.windows(2).all(|w| w[1] >= w[0]), "{id}: air steps up");
        let landmarks: Vec<&str> = p.events.iter().filter_map(|e| e.label.as_deref()).collect();
        assert_eq!(
            landmarks,
            [
                "charge",
                "turning_point",
                "drying_end",
                "first_crack_start",
                "drop"
            ],
            "{id}"
        );
    }
}

#[tokio::test]
async fn a_profile_roaster_kind_is_a_setup_kind_kept_when_absent_and_cleared_by_null() {
    let db = crate::db::open_memory().await;
    let mut req = guidance_profile(Some("light"), None);
    req.roaster_kind = Some(Some("drum_electric".into()));
    let made = crate::profiles::store::create_profile(&db, req)
        .await
        .unwrap();
    assert_eq!(made.profile.roaster_kind.as_deref(), Some("drum_electric"));
    let id = made.profile.id;

    // Absent keeps it.
    let kept =
        crate::profiles::store::update_profile(&db, &id, guidance_profile(Some("light"), None))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(kept.profile.roaster_kind.as_deref(), Some("drum_electric"));
    // null clears it.
    let mut clear = guidance_profile(Some("light"), None);
    clear.roaster_kind = Some(None);
    let cleared = crate::profiles::store::update_profile(&db, &id, clear)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cleared.profile.roaster_kind, None);

    for bad in ["drum", "Fluid bed", "wood_fired"] {
        let mut req = guidance_profile(Some("light"), None);
        req.roaster_kind = Some(Some(bad.into()));
        let err = crate::profiles::store::create_profile(&db, req)
            .await
            .unwrap_err();
        assert!(
            err.downcast_ref::<ProfileInputError>().is_some(),
            "{bad}: {err}"
        );
        let mut req = guidance_profile(Some("light"), None);
        req.roaster_kind = Some(Some(bad.into()));
        assert!(
            crate::profiles::store::update_profile(&db, &id, req)
                .await
                .is_err(),
            "{bad}"
        );
    }
}

#[tokio::test]
async fn seed_profiles_say_what_they_are_for() {
    let db = crate::db::open_memory().await;
    let p = crate::profiles::store::get_profile_with_points(&db, "fluidbed-espresso")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.profile.roast_level.as_deref(), Some("light"));
    assert_eq!(p.profile.roast_purpose.as_deref(), Some("espresso"));
    let p = crate::profiles::store::get_profile_with_points(&db, "fluidbed-light")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.profile.roast_level.as_deref(), Some("light"));
    for (id, level, purpose) in [
        ("fluidbed-nordic", "extremely_light", "filter"),
        ("fluidbed-cupping", "medium_light", "other"),
        ("fluidbed-espresso-short", "light", "espresso"),
        ("fluidbed-omni", "medium_light", "omni"),
    ] {
        let p = crate::profiles::store::get_profile_with_points(&db, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(p.profile.roast_level.as_deref(), Some(level), "{id}");
        assert_eq!(p.profile.roast_purpose.as_deref(), Some(purpose), "{id}");
        assert!(p.points.len() >= 17, "{id}");
        assert_eq!(p.events.len(), 2, "{id}: DRY and FC landmarks");
    }
}
