//! A profile from a file: a profile Roastwarden exported, as it is, or an Artisan roast log
//! extracted the way a recorded roast is.

use anyhow::Result;
use sqlx::SqlitePool;

use crate::profiles::types::{
    CreateProfileEventRequest, CreateProfilePointRequest, CreateProfileRequest,
    ImportArtisanProfileRequest, ProfileCurves, ProfileWithPoints, RoastProfile,
};
use crate::profiles::validate::{ProfileInputError, is_roaster_kind};
use crate::roasts::types::RoastEventType;

pub async fn import_artisan_profile(
    db: &SqlitePool,
    req: ImportArtisanProfileRequest,
) -> Result<ProfileWithPoints> {
    // A Roastwarden profile export (JSON with time_seconds points) imports as is.
    if let Some(native) = parse_native_profile(&req.alog_content) {
        let mut create = native;
        if let Some(name) = req.name.filter(|n| !n.trim().is_empty()) {
            create.name = name;
        }
        return crate::profiles::store::create_profile(db, create).await;
    }

    // An Artisan roast log becomes a profile the same way a recorded
    // session does: points, events and curves from its samples.
    let meta = crate::profiles::artisan::parse_profile(&req.alog_content)?;
    let parsed = crate::profiles::artisan::parse_session(&req.alog_content)?;
    let cfg = crate::analysis::telemetry::RorConfig::default();
    let mut tracker = crate::analysis::telemetry::RorTracker::default();
    let samples: Vec<crate::profiles::extract::RecordedSample> = parsed
        .samples
        .iter()
        .map(|s| {
            let ror = s.bean_temp.and_then(|bt| {
                tracker.push(s.elapsed, f64::from(bt));
                tracker.rate(&cfg).map(|v| v as f32)
            });
            crate::profiles::extract::RecordedSample {
                t: s.elapsed,
                bt: s.bean_temp,
                et: s.env_temp,
                ror,
                heater_pct: s.heater.map(|v| v as f32),
                fan_pct: s.fan.map(|v| v as f32),
            }
        })
        .collect();
    // Samples are already CHARGE-relative, so CHARGE itself is t = 0.
    let landmarks: Vec<crate::profiles::extract::RecordedLandmark> = parsed
        .events
        .iter()
        .filter_map(|e| {
            Some(crate::profiles::extract::RecordedLandmark {
                kind: artisan_event_kind(&e.event_type)?,
                t: e.time,
            })
        })
        .collect();
    let extracted = crate::profiles::extract::extract(&samples, &landmarks).ok_or_else(|| {
        ProfileInputError("Artisan profile contains no usable bean-temperature samples".into())
    })?;

    let profile_name = req
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            if meta.title.is_empty() {
                "Imported Artisan Profile".to_string()
            } else {
                meta.title.clone()
            }
        });

    let create_req = CreateProfileRequest {
        name: profile_name,
        beans: parsed.beans.clone(),
        batch_size_g: parsed.green_weight_g,
        roaster: parsed.roaster_type.clone(),
        roast_level: None,
        roast_purpose: None,
        expectations: None,
        auto_drop: None,
        roaster_kind: None,
        description: Some(format!("Imported from Artisan - Date: {}", meta.roast_date)),
        target_total_time: Some(extracted.total_time),
        target_first_crack: extracted.first_crack,
        target_end_temp: extracted.end_temp,
        preheat_temp: None,
        charge_temp: extracted.charge_temp,
        points: extracted.points,
        events: Some(extracted.events),
        curves: Some(Some(extracted.curves)),
    };

    crate::profiles::store::create_profile(db, create_req).await
}

/// A profile file as Roastwarden exports it: the nested form (`profile`, the API's shape) or
/// the designer's flat fields, each with its points, events and curves.
#[derive(serde::Deserialize)]
struct Export {
    #[serde(default)]
    profile: Option<RoastProfile>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    target_total_time: Option<i32>,
    #[serde(default)]
    target_first_crack: Option<i32>,
    #[serde(default)]
    target_end_temp: Option<f32>,
    #[serde(default)]
    preheat_temp: Option<f32>,
    #[serde(default)]
    charge_temp: Option<f32>,
    #[serde(default)]
    beans: Option<String>,
    #[serde(default)]
    batch_size_g: Option<f32>,
    #[serde(default)]
    roaster: Option<String>,
    #[serde(default)]
    roast_level: Option<String>,
    #[serde(default)]
    roast_purpose: Option<String>,
    #[serde(default)]
    expectations: Option<serde_json::Value>,
    #[serde(default)]
    auto_drop: Option<String>,
    #[serde(default)]
    roaster_kind: Option<String>,
    points: Vec<CreateProfilePointRequest>,
    #[serde(default)]
    events: Vec<CreateProfileEventRequest>,
    #[serde(default)]
    curves: Option<ProfileCurves>,
}

fn parse_native_profile(content: &str) -> Option<CreateProfileRequest> {
    let e: Export = serde_json::from_str(content).ok()?;
    if e.points.is_empty() {
        return None;
    }
    // `profile` is the nested form produced by ProfileWithPoints; flat fields are the designer form.
    let (level, purpose, expectations, auto_drop, kind) = match &e.profile {
        Some(p) => (
            p.roast_level.clone(),
            p.roast_purpose.clone(),
            p.expectations.clone().map(|j| j.0),
            p.auto_drop.clone(),
            p.roaster_kind.clone(),
        ),
        None => (
            e.roast_level,
            e.roast_purpose,
            e.expectations,
            e.auto_drop,
            e.roaster_kind,
        ),
    };
    // Another server's kind it does not know is left out, not refused.
    let kind = kind.filter(|k| is_roaster_kind(k));
    let (name, description, total, fc, end, preheat, charge, beans, batch, roaster) =
        match e.profile {
            Some(p) => (
                p.name,
                p.description,
                p.target_total_time,
                p.target_first_crack,
                p.target_end_temp,
                p.preheat_temp,
                p.charge_temp,
                p.beans,
                p.batch_size_g,
                p.roaster,
            ),
            None => (
                e.name?,
                e.description,
                e.target_total_time,
                e.target_first_crack,
                e.target_end_temp,
                e.preheat_temp,
                e.charge_temp,
                e.beans,
                e.batch_size_g,
                e.roaster,
            ),
        };
    Some(CreateProfileRequest {
        name,
        description,
        beans,
        batch_size_g: batch,
        roaster,
        roast_level: Some(level),
        roast_purpose: Some(purpose),
        expectations: Some(expectations),
        auto_drop: Some(auto_drop),
        roaster_kind: Some(kind),
        target_total_time: total,
        target_first_crack: fc,
        target_end_temp: end,
        preheat_temp: preheat,
        charge_temp: charge,
        points: e.points,
        events: Some(e.events),
        curves: Some(e.curves.map(|c| ProfileCurves {
            // The source session belongs to the exporting server.
            source_session_id: None,
            ..c
        })),
    })
}

/// Map an Artisan landmark code to a roast event type.
pub(crate) fn artisan_event_kind(code: &str) -> Option<RoastEventType> {
    Some(match code {
        "CHARGE" => RoastEventType::Charge,
        "TP" => RoastEventType::TurningPoint,
        "DRY" => RoastEventType::DryingEnd,
        "FCs" => RoastEventType::FirstCrackStart,
        "FCe" => RoastEventType::FirstCrackEnd,
        "SCs" => RoastEventType::SecondCrackStart,
        "SCe" => RoastEventType::SecondCrackEnd,
        "DROP" => RoastEventType::Drop,
        "COOL" => RoastEventType::CoolEnd,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::types::{ProfileAction, SaveSessionAsProfileRequest};
    use crate::roasts::testing::recorded_session;

    #[tokio::test]
    async fn test_profile_export_round_trips_through_import() {
        let db = crate::db::open_memory().await;
        let session = recorded_session(&db).await;
        let p = crate::profiles::extract::save_session_as_profile(
            &db,
            &session.id,
            SaveSessionAsProfileRequest::default(),
        )
        .await
        .unwrap()
        .unwrap();
        let exported = serde_json::to_string(&p).unwrap();
        let back = crate::profiles::import::import_artisan_profile(
            &db,
            ImportArtisanProfileRequest {
                alog_content: exported,
                name: Some("Copy".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(back.profile.name, "Copy");
        assert_eq!(back.points.len(), p.points.len());
        assert_eq!(back.events.len(), p.events.len());
        let c = back.curves.unwrap();
        assert_eq!(c.bt, p.curves.unwrap().bt);
        assert!(c.source_session_id.is_none());
    }

    #[tokio::test]
    async fn test_import_artisan_profile_has_curves_and_events() {
        let db = crate::db::open_memory().await;
        let timex: Vec<f64> = (0..=660).map(|i| i as f64).collect();
        let temp2: Vec<f64> = (0..=660)
            .map(|i| {
                if i < 60 {
                    180.0 - i as f64
                } else {
                    120.0 + (i as f64 - 60.0) * 0.18
                }
            })
            .collect();
        let temp1: Vec<f64> = (0..=660).map(|i| 200.0 + i as f64 * 0.05).collect();
        let alog = serde_json::json!({
            "title": "Colombia 1", "mode": "C", "timex": timex, "temp1": temp1, "temp2": temp2,
            "timeindex": [0, 240, 480, 0, 0, 0, 600, 0],
            "specialevents": [0, 300], "specialeventstype": [3, 3], "specialeventsvalue": [9.0, 6.0]
        });
        let p = crate::profiles::import::import_artisan_profile(
            &db,
            ImportArtisanProfileRequest {
                alog_content: crate::profiles::artisan::to_python_literal(&alog),
                name: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(p.profile.name, "Colombia 1");
        assert_eq!(p.profile.target_total_time, Some(600)); // stops at DROP
        assert_eq!(p.profile.target_first_crack, Some(480));
        assert_eq!(p.curves.as_ref().unwrap().bt.len(), 601);
        let heater: Vec<(f64, f64)> = p
            .events
            .iter()
            .filter(|e| e.action == ProfileAction::Heater)
            .map(|e| (e.at_seconds.unwrap(), e.value.unwrap()))
            .collect();
        assert_eq!(heater, vec![(0.0, 80.0), (300.0, 50.0)]);
        assert!(p.points.len() < 40, "{} points", p.points.len());
    }

    #[tokio::test]
    async fn test_profile_metadata_round_trips_through_alog() {
        let db = crate::db::open_memory().await;
        let timex: Vec<f64> = (0..=600).map(|i| i as f64).collect();
        let temp2: Vec<f64> = (0..=600).map(|i| 30.0 + i as f64 * 0.3).collect();
        let alog = serde_json::json!({
            "title": "Guji", "mode": "C", "timex": timex, "temp1": [], "temp2": temp2,
            "timeindex": [0, 0, 480, 0, 0, 0, 600, 0],
            "beans": "Ethiopia Guji", "weight": [0.12, 0.1, "Kg"], "roastertype": "Fluid bed",
            "specialevents": [], "specialeventstype": [], "specialeventsvalue": []
        });
        let imported = crate::profiles::import::import_artisan_profile(
            &db,
            ImportArtisanProfileRequest {
                alog_content: crate::profiles::artisan::to_python_literal(&alog),
                name: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(imported.profile.beans.as_deref(), Some("Ethiopia Guji"));
        assert_eq!(imported.profile.batch_size_g, Some(120.0));
        assert_eq!(imported.profile.roaster.as_deref(), Some("Fluid bed"));

        // Out as .alog and back in: same metadata and landmarks.
        let body = crate::profiles::artisan::render_alog(
            &crate::profiles::artisan::build_profile_alog(&imported).unwrap(),
        );
        let again = crate::profiles::import::import_artisan_profile(
            &db,
            ImportArtisanProfileRequest {
                alog_content: body,
                name: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(again.profile.name, "Guji");
        assert_eq!(again.profile.beans.as_deref(), Some("Ethiopia Guji"));
        assert_eq!(again.profile.batch_size_g, Some(120.0));
        assert_eq!(again.profile.roaster.as_deref(), Some("Fluid bed"));
        assert_eq!(again.profile.target_first_crack, Some(480));
        assert_eq!(again.profile.target_total_time, Some(600));

        // The Roastwarden JSON export keeps it too.
        let json = serde_json::to_string(&imported).unwrap();
        let native = crate::profiles::import::import_artisan_profile(
            &db,
            ImportArtisanProfileRequest {
                alog_content: json,
                name: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(native.profile.batch_size_g, Some(120.0));
        assert_eq!(native.profile.roaster.as_deref(), Some("Fluid bed"));
    }
}
