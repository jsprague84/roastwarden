//! A finished roast's numbers follow its landmarks: moving first crack off a lone early crack
//! (as a recorded roast needed) recomputes first crack time and DTR, and leaves
//! the roast's status and end time alone. The whole server (`test_support::Harness`).

use roastwarden_server::test_support::Harness;
use serde_json::{json, Value};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_first_crack_on_a_finished_roast_recomputes_its_numbers() {
    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "stats", "device_id": "bench_roaster" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    let r = h
        .owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());

    // 10 minutes of BT rising 0.3 °C/s from 25 °C
    for t in (0..=600).step_by(10) {
        let bt = 25.0 + 0.3 * f64::from(t);
        let r = h
            .owner(
                "POST",
                &format!("/api/sessions/{id}/telemetry"),
                Some(json!({ "elapsed_seconds": t, "bean_temp": bt, "rate_of_rise": 18.0 })),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
    }
    let mut fc_id = String::new();
    for (kind, at) in [
        ("charge", 0.0),
        ("first_crack_start", 376.0),
        ("drop", 600.0),
    ] {
        let e = h
            .owner(
                "POST",
                &format!("/api/sessions/{id}/events"),
                Some(json!({ "event_type": kind, "elapsed_seconds": at })),
            )
            .await
            .json();
        if kind == "first_crack_start" {
            fc_id = e["id"].as_str().unwrap().to_string();
        }
    }
    let done: Value = h
        .owner("POST", &format!("/api/sessions/{id}/complete"), None)
        .await
        .json();
    assert_eq!(done["status"], "completed");
    assert_eq!(done["first_crack_time"], 376);
    let dtr = done["development_time_ratio"].as_f64().unwrap();
    assert!((dtr - 224.0 / 600.0).abs() < 1e-3, "{dtr}");

    // The lone early crack at 6:16 was not first crack: steady cracking began at 7:30
    let r = h
        .owner(
            "PUT",
            &format!("/api/sessions/{id}/events/{fc_id}"),
            Some(json!({ "elapsed_seconds": 450.0 })),
        )
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let after = h
        .owner("GET", &format!("/api/sessions/{id}"), None)
        .await
        .json();
    let after = after.get("session").cloned().unwrap_or(after);
    assert_eq!(after["first_crack_time"], 450);
    let dtr = after["development_time_ratio"].as_f64().unwrap();
    assert!((dtr - 150.0 / 600.0).abs() < 1e-3, "{dtr}");
    assert_eq!(after["status"], "completed");
    assert_eq!(after["end_time"], done["end_time"], "the end time stays");

    // Removing first crack clears what depended on it
    let r = h
        .owner(
            "DELETE",
            &format!("/api/sessions/{id}/events/{fc_id}"),
            None,
        )
        .await;
    assert!(r.status.is_success());
    let gone = h
        .owner("GET", &format!("/api/sessions/{id}"), None)
        .await
        .json();
    let gone = gone.get("session").cloned().unwrap_or(gone);
    assert!(gone["first_crack_time"].is_null(), "{gone}");
    assert!(gone["development_time_ratio"].is_null(), "{gone}");
}

/// A roast has one CHARGE and one DROP. A page that missed an auto DROP (a hidden tab, in a
/// recorded roast) still showed the DROP button, and its tap ended the roast a second time at
/// 45 °C.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_charge_or_drop_is_refused() {
    let h = Harness::new().await;
    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(json!({ "name": "twice", "device_id": "bench_roaster" })),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    let r = h
        .owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    assert!(r.status.is_success(), "{:?}", r.json());
    let path = format!("/api/sessions/{id}/events");
    for (kind, at, again) in [("charge", 0.0, 5.0), ("drop", 600.0, 760.0)] {
        let first = h
            .owner(
                "POST",
                &path,
                Some(json!({ "event_type": kind, "elapsed_seconds": at })),
            )
            .await;
        assert_eq!(first.status.as_u16(), 201, "{:?}", first.json());
        let second = h
            .owner(
                "POST",
                &path,
                Some(json!({ "event_type": kind, "elapsed_seconds": again })),
            )
            .await;
        assert_eq!(second.status.as_u16(), 409, "{kind}: {:?}", second.json());
        assert_eq!(second.json()["error"]["code"], "ALREADY_MARKED");
    }
    // Other landmarks are the dashboard's to keep single; custom events repeat
    for _ in 0..2 {
        let r = h
            .owner(
                "POST",
                &path,
                Some(json!({ "event_type": "custom", "elapsed_seconds": 300.0 })),
            )
            .await;
        assert_eq!(r.status.as_u16(), 201);
    }
    let events: Value = h.owner("GET", &path, None).await.json();
    let count = |k: &str| {
        events
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["event_type"] == k)
            .count()
    };
    assert_eq!((count("charge"), count("drop"), count("custom")), (1, 1, 2));
}
