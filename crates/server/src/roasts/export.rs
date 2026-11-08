//! A roast as a file: CSV, or an Artisan .alog with its declared channels as extra devices.

use std::fmt::Write as _;

use anyhow::Result;
use sqlx::SqlitePool;

// ---- Data Export (AP-014) ----

pub async fn export_csv(db: &SqlitePool, id: &str) -> Result<Option<(String, String)>> {
    let Some(session) = crate::roasts::sessions::get_session(db, id).await? else {
        return Ok(None);
    };
    let telemetry = crate::roasts::telemetry_store::get_session_telemetry(db, id).await?;
    let profile_name = if let Some(pid) = &session.profile_id {
        crate::profiles::store::get_profile_with_points(db, pid)
            .await?
            .map(|p| p.profile.name)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let mut csv = String::new();
    let _ = writeln!(csv, "# Session: {}", session.name);
    if let Some(ref st) = session.start_time {
        let _ = writeln!(csv, "# Date: {st}");
    }
    if let Some(ref origin) = session.bean_origin {
        let variety = session.bean_variety.as_deref().unwrap_or("");
        if variety.is_empty() {
            let _ = writeln!(csv, "# Bean: {origin}");
        } else {
            let _ = writeln!(csv, "# Bean: {origin} ({variety})");
        }
    }
    if !profile_name.is_empty() {
        let _ = writeln!(csv, "# Profile: {profile_name}");
    }
    if let Some(gw) = session.green_weight {
        let _ = writeln!(csv, "# Green Weight: {gw}g");
    }
    if let Some(rw) = session.roasted_weight {
        let _ = writeln!(csv, "# Roasted Weight: {rw}g");
    }

    csv.push_str("elapsed_seconds,bean_temp,env_temp,rate_of_rise,heater_pct,fan_pct,setpoint\n");
    for t in &telemetry {
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{}",
            t.elapsed_seconds,
            t.bean_temp.map(|v| v.to_string()).unwrap_or_default(),
            t.env_temp.map(|v| v.to_string()).unwrap_or_default(),
            t.rate_of_rise.map(|v| v.to_string()).unwrap_or_default(),
            t.heater.map(|v| v.to_string()).unwrap_or_default(),
            t.fan.map(|v| v.to_string()).unwrap_or_default(),
            t.setpoint.map(|v| v.to_string()).unwrap_or_default(),
        );
    }

    let date_str = session.start_time.map_or_else(
        || session.created_at.format("%Y-%m-%d").to_string(),
        |dt| dt.format("%Y-%m-%d").to_string(),
    );
    let filename = format!("{}_{}.csv", session.name.replace(' ', "_"), date_str);

    Ok(Some((csv, filename)))
}

/// Build an Artisan profile dict for a session. The HTTP layer renders it
/// as a Python literal (`.alog`); tests inspect the JSON value directly.
pub async fn export_alog(db: &SqlitePool, id: &str) -> Result<Option<(serde_json::Value, String)>> {
    let Some(session) = crate::roasts::sessions::get_session(db, id).await? else {
        return Ok(None);
    };
    let telemetry = crate::roasts::telemetry_store::get_session_telemetry(db, id).await?;
    let events = crate::roasts::events::get_roast_events(db, id).await?;

    // The roast's declared channels become Artisan extra devices (docs/channels.md).
    let channels = crate::roasters::channels::for_roast(db, id, &session.device_id).await;
    let alog = crate::profiles::artisan::build_alog(&session, &telemetry, &events, &channels);

    let date_str = session.start_time.map_or_else(
        || session.created_at.format("%Y-%m-%d").to_string(),
        |dt| dt.format("%Y-%m-%d").to_string(),
    );
    let filename = format!("{}_{}.alog", session.name.replace(' ', "_"), date_str);

    Ok(Some((alog, filename)))
}

#[cfg(test)]
mod tests {
    use crate::roasts::types::{CreateRoastEventRequest, CreateSessionRequest, RoastEventType};

    #[tokio::test]
    async fn test_export_csv_and_artisan() {
        let db = crate::db::open_memory().await;

        // Create a session with metadata
        let session = crate::roasts::sessions::create_session(
            &db,
            CreateSessionRequest {
                bean_id: None,
                planned_roast_id: None,
                roast_purpose: None,
                name: "Export Test".to_string(),
                device_id: "esp32-001".to_string(),
                profile_id: None,
                bean_origin: Some("Colombia".to_string()),
                bean_variety: Some("Caturra".to_string()),
                green_weight: Some(200.0),
                target_roast_level: None,
                notes: None,
                ambient_temp: None,
                humidity: None,
            },
        )
        .await
        .unwrap();

        // Start session
        crate::roasts::sessions::start_session(&db, &session.id)
            .await
            .unwrap();

        // Add telemetry
        for i in 0..5 {
            let elapsed = i as f32 * 60.0;
            let bt = 100.0 + i as f32 * 20.0;
            let et = bt + 30.0;
            crate::roasts::telemetry_store::add_telemetry_point(
                &db,
                &session.id,
                elapsed,
                Some(bt),
                Some(et),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Add an event
        crate::roasts::events::create_roast_event(
            &db,
            &session.id,
            CreateRoastEventRequest {
                event_type: RoastEventType::FirstCrackStart,
                elapsed_seconds: 180.0,
                temperature: Some(180.0),
                notes: None,
            },
        )
        .await
        .unwrap();

        // Test CSV export
        let (csv, csv_filename) = crate::roasts::export::export_csv(&db, &session.id)
            .await
            .unwrap()
            .unwrap();
        assert!(csv_filename.starts_with("Export_Test_"));
        assert!(csv_filename.ends_with(".csv"));
        assert!(csv.contains("# Session: Export Test"));
        assert!(csv.contains("# Bean: Colombia (Caturra)"));
        assert!(csv.contains("elapsed_seconds,bean_temp,env_temp"));
        // Check we have data rows (header + 5 telemetry points)
        let data_lines: Vec<&str> = csv.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(data_lines.len(), 6); // 1 header + 5 data rows

        // Drop at 240s (last sample)
        crate::roasts::events::create_roast_event(
            &db,
            &session.id,
            CreateRoastEventRequest {
                event_type: RoastEventType::Drop,
                elapsed_seconds: 240.0,
                temperature: Some(180.0),
                notes: None,
            },
        )
        .await
        .unwrap();

        // Test Artisan export
        let (alog, alog_filename) = crate::roasts::export::export_alog(&db, &session.id)
            .await
            .unwrap()
            .unwrap();
        assert!(alog_filename.starts_with("Export_Test_"));
        assert!(alog_filename.ends_with(".alog"));
        assert_eq!(alog["title"], "Export Test");
        assert_eq!(alog["beans"], "Colombia");
        let timex = alog["timex"].as_array().unwrap();
        assert_eq!(timex.len(), 5);
        let temp2 = alog["temp2"].as_array().unwrap();
        assert_eq!(temp2.len(), 5);
        // First BT should be 100.0
        assert!((temp2[0].as_f64().unwrap() - 100.0).abs() < 0.1);
        // timeindex should have FCs mapped
        let timeindex = alog["timeindex"].as_array().unwrap();
        assert_eq!(timeindex.len(), 8);
        // CHARGE at index 0
        assert_eq!(timeindex[0], 0);
        // FCs should be at index 3 in telemetry (180s is closest to 180.0)
        assert_eq!(timeindex[2], 3);
        // DROP maps to the last sample
        assert_eq!(timeindex[6], 4);
        assert_eq!(alog["mode"], "C");
        // The file body must be a Python literal Artisan can literal_eval
        let body = crate::profiles::artisan::render_alog(&alog);
        assert!(body.starts_with('{'));
        assert!(!body.contains("null"));
        let back = crate::profiles::artisan::literal::parse_python_literal(&body).unwrap();
        assert_eq!(back["timeindex"][6], 4);
    }
}
