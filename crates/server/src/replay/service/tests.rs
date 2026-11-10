//! The service on the whole server: a drive cut by a restart resumes, auto DROP drops.

mod restart_tests {
    use crate::test_support::{Harness, eventually};
    use serde_json::{Value, json};
    use std::time::Duration;

    const VR: &str = crate::sim::virtual_roaster::DEVICE_ID;

    async fn charged_roast(h: &Harness, name: &str) -> String {
        let r = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({ "name": name, "device_id": VR, "profile_id": "fluidbed-espresso" })),
            )
            .await;
        let sid = r.json()["id"]
            .as_str()
            .unwrap_or_else(|| panic!("create {name}: {}", r.status))
            .to_string();
        h.owner("POST", &format!("/api/sessions/{sid}/start"), None)
            .await;
        h.owner(
            "POST",
            &format!("/api/sessions/{sid}/events"),
            Some(json!({ "event_type": "charge", "elapsed_seconds": 0.0 })),
        )
        .await;
        h.owner(
            "POST",
            &format!("/api/sessions/{sid}/replay"),
            Some(json!({})),
        )
        .await;
        let replay = format!("/api/sessions/{sid}/replay");
        assert!(
            eventually(|| async {
                h.owner("GET", &replay, None).await.json()["last_sent_at"].is_string()
            })
            .await,
            "the drive sends"
        );
        sid
    }

    #[tokio::test]
    async fn a_drive_cut_by_a_restart_resumes_where_it_stopped() {
        let h = Harness::new().await;
        h.owner(
            "PUT",
            "/api/virtual-roaster",
            Some(json!({ "enabled": true })),
        )
        .await;
        let latest = format!("/api/roaster/{VR}/telemetry/latest");
        assert!(
            eventually(|| async { h.owner("GET", &latest, None).await.status.is_success() }).await
        );
        let sid = charged_roast(&h, "Cut by a restart").await;
        let replay = format!("/api/sessions/{sid}/replay");
        tokio::time::sleep(Duration::from_secs(2)).await;

        // The process dies; the next one boots on the same database.
        h.state.replay.die_for_test().await;
        h.state.replay.recover_cut_runs().await;
        let st: Value = h.owner("GET", &replay, None).await.json();
        assert_eq!(st["active"], json!(false), "{st}");
        assert_eq!(st["stopped_by_restart"], json!(true), "{st}");
        assert_eq!(st["stopped_reason"], json!("the server restarted"));
        let stopped_at = st["profile_t"].as_f64().unwrap();
        assert!(stopped_at > 1.0, "{st}");
        // The run that was cut (CHARGE's own drive may have been replaced by the test's start)
        let log = h.owner("GET", &format!("{replay}/log"), None).await.json();
        let cut = log["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == st["run_id"])
            .cloned()
            .unwrap_or_default();
        assert_eq!(cut["end_reason"], json!("server restarted"), "{log}");

        // Some time with no drive, then the resume picks up at the stopped profile time.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let r = h
            .owner("POST", &format!("/api/sessions/{sid}/drive/resume"), None)
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        let st = h.owner("GET", &replay, None).await.json();
        assert_eq!(st["active"], json!(true), "{st}");
        let hold = st["config"]["hold"]["seconds"].as_f64().unwrap();
        assert!(hold > 2.0, "the lost time goes on the offset: {st}");
        assert!(
            (st["profile_t"].as_f64().unwrap() - stopped_at).abs() < 2.5,
            "resumed at {} for {stopped_at}",
            st["profile_t"]
        );
        assert!(st["align"]["offset_seconds"].as_f64().unwrap() >= hold);

        // A roast dropped while the server was down offers nothing.
        h.owner("DELETE", &replay, None).await;
        h.owner("POST", &format!("/api/sessions/{sid}/complete"), None)
            .await;
        let sid = charged_roast(&h, "Dropped while down").await;
        h.state.replay.die_for_test().await;
        h.owner(
            "POST",
            &format!("/api/sessions/{sid}/events"),
            Some(json!({ "event_type": "drop", "elapsed_seconds": 5.0 })),
        )
        .await;
        h.state.replay.recover_cut_runs().await;
        let st = h
            .owner("GET", &format!("/api/sessions/{sid}/replay"), None)
            .await
            .json();
        assert_ne!(st["stopped_by_restart"], json!(true), "{st}");
    }
}

mod auto_drop_tests {
    use crate::test_support::{Harness, eventually};
    use serde_json::{Value, json};
    use std::time::Duration;

    const VR: &str = crate::sim::virtual_roaster::DEVICE_ID;

    fn profile(name: &str, auto_drop: Value, total: Value, fc: Value) -> Value {
        json!({
            "name": name,
            "target_total_time": total,
            "target_first_crack": fc,
            "target_end_temp": 210.0,
            "auto_drop": auto_drop,
            "points": [
                { "time_seconds": 0, "target_temp": 30.0 },
                { "time_seconds": 600, "target_temp": 210.0 }
            ]
        })
    }

    /// A roast on the virtual roaster with `profile_id`, charged now, its drive running.
    async fn charged(h: &Harness, profile_id: &str, name: &str) -> String {
        let s = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({ "name": name, "device_id": VR, "profile_id": profile_id })),
            )
            .await
            .json();
        let sid = s["id"].as_str().unwrap().to_string();
        h.owner("POST", &format!("/api/sessions/{sid}/start"), None)
            .await;
        h.owner(
            "POST",
            &format!("/api/sessions/{sid}/events"),
            Some(json!({ "event_type": "charge", "elapsed_seconds": 0.0 })),
        )
        .await;
        let replay = format!("/api/sessions/{sid}/replay");
        assert!(
            eventually(|| async {
                h.owner("GET", &replay, None).await.json()["active"] == json!(true)
            })
            .await,
            "CHARGE starts the drive"
        );
        sid
    }

    async fn drop_mark(h: &Harness, sid: &str) -> Option<Value> {
        let evs: Value = h
            .owner("GET", &format!("/api/sessions/{sid}/events"), None)
            .await
            .json();
        evs.as_array()?
            .iter()
            .find(|e| e["event_type"] == json!("drop"))
            .cloned()
    }

    #[tokio::test]
    async fn a_profile_names_what_its_auto_drop_needs() {
        let h = Harness::new().await;
        let bad = h
            .owner(
                "POST",
                "/api/profiles",
                Some(profile(
                    "No FC",
                    json!("development"),
                    json!(600),
                    Value::Null,
                )),
            )
            .await;
        assert_eq!(bad.status.as_u16(), 400);
        let bad = h
            .owner(
                "POST",
                "/api/profiles",
                Some(profile("Odd", json!("whenever"), json!(600), json!(470))),
            )
            .await;
        assert_eq!(bad.status.as_u16(), 400);
        let p = h
            .owner(
                "POST",
                "/api/profiles",
                Some(profile("By time", json!("time"), json!(600), json!(470))),
            )
            .await
            .json();
        assert_eq!(p["auto_drop"], json!("time"), "{p}");
        let path = format!("/api/profiles/{}", p["id"].as_str().unwrap());
        // An update that leaves auto DROP out keeps it, and must keep what it goes by.
        let mut body = profile("By time", Value::Null, json!(605), json!(470));
        body.as_object_mut().unwrap().remove("auto_drop");
        let kept = h.owner("PUT", &path, Some(body.clone())).await.json();
        assert_eq!(kept["auto_drop"], json!("time"), "{kept}");
        body["target_total_time"] = Value::Null;
        assert_eq!(
            h.owner("PUT", &path, Some(body.clone()))
                .await
                .status
                .as_u16(),
            400
        );
        body["auto_drop"] = Value::Null;
        let off = h.owner("PUT", &path, Some(body)).await.json();
        assert_eq!(off["auto_drop"], Value::Null, "{off}");
    }

    #[tokio::test]
    async fn the_drive_drops_at_the_profiles_drop_unless_cancelled() {
        let h = Harness::new().await;
        h.owner(
            "PUT",
            "/api/virtual-roaster",
            Some(json!({ "enabled": true })),
        )
        .await;
        let latest = format!("/api/roaster/{VR}/telemetry/latest");
        assert!(
            eventually(|| async { h.owner("GET", &latest, None).await.status.is_success() }).await
        );
        let r = h
            .owner(
                "PUT",
                "/api/settings/auto_drop_countdown_seconds",
                Some(json!({ "value": "3" })),
            )
            .await;
        assert!(r.status.is_success());
        let p = h
            .owner(
                "POST",
                "/api/profiles",
                Some(profile("Quick drop", json!("time"), json!(7), json!(4))),
            )
            .await
            .json();
        let pid = p["id"].as_str().unwrap().to_string();
        let mut notices = h.state.alarms.subscribe();

        let sid = charged(&h, &pid, "Auto DROP").await;
        let replay = format!("/api/sessions/{sid}/replay");
        let st = h.owner("GET", &replay, None).await.json();
        assert_eq!(st["auto_drop"]["by"], json!("time"), "{st}");
        assert_eq!(st["auto_drop"]["countdown_seconds"], json!(3.0), "{st}");
        assert!(
            eventually(|| async { drop_mark(&h, &sid).await.is_some() }).await,
            "auto DROP marks DROP"
        );
        let d = drop_mark(&h, &sid).await.unwrap();
        assert!(
            d["notes"].as_str().is_some_and(|n| n.contains("auto DROP")),
            "{d}"
        );
        let at = d["elapsed_seconds"].as_f64().unwrap();
        assert!(
            (6.0..10.0).contains(&at),
            "DROP at the profile's drop (7 s): {at}"
        );
        // The notice went out when the countdown started.
        let mut noticed = false;
        while let Ok(a) = notices.try_recv() {
            noticed |= a.name == "Auto DROP" && a.session_id == sid;
        }
        assert!(noticed, "a notice when the countdown starts");
        // DROP's step ran: the drive stopped and the roaster cools.
        assert!(
            eventually(|| async {
                h.owner("GET", &replay, None).await.json()["active"] != json!(true)
            })
            .await
        );
        // DROP's step cools; after 7 s the chamber may already be below the cooling's end, so
        // it may be over already, with COOL END marked.
        let cool_end = || async {
            let evs: Value = h
                .owner("GET", &format!("/api/sessions/{sid}/events"), None)
                .await
                .json();
            evs.as_array()
                .is_some_and(|a| a.iter().any(|e| e["event_type"] == json!("cool_end")))
        };
        assert!(
            eventually(|| async { h.state.sequence.cooling(VR).await || cool_end().await }).await,
            "DROP's step cools"
        );

        // Cancelled: the roast runs on past the drop, and a resume keeps it cancelled.
        h.owner("POST", &format!("/api/sessions/{sid}/complete"), None)
            .await;
        let sid = charged(&h, &pid, "Cancelled").await;
        let replay = format!("/api/sessions/{sid}/replay");
        let r = h
            .owner("PATCH", &replay, Some(json!({ "auto_drop": false })))
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        assert_eq!(r.json()["auto_drop"]["state"], json!("cancelled"));
        tokio::time::sleep(Duration::from_secs(11)).await;
        assert!(
            drop_mark(&h, &sid).await.is_none(),
            "no DROP once cancelled"
        );
        let st = h.owner("GET", &replay, None).await.json();
        assert_eq!(st["active"], json!(true), "{st}");
        assert_eq!(st["auto_drop"]["state"], json!("cancelled"), "{st}");
        let log = h.owner("GET", &format!("{replay}/log"), None).await.json();
        assert!(
            log.to_string()
                .contains("auto DROP cancelled for this roast"),
            "{log}"
        );
    }

    #[tokio::test]
    async fn auto_drop_stays_off_without_drops_step_or_a_profile_that_has_it() {
        let h = Harness::new().await;
        h.owner(
            "PUT",
            "/api/virtual-roaster",
            Some(json!({ "enabled": true })),
        )
        .await;
        let p = h
            .owner(
                "POST",
                "/api/profiles",
                Some(profile("No step", json!("time"), json!(600), json!(470))),
            )
            .await
            .json();
        let r = h
            .owner(
                "PUT",
                &format!("/api/roaster/{VR}/sequence"),
                Some(json!({ "charge_enabled": true, "charge_fan_pct": 100, "charge_drive": true,
                    "charge_heater_pct": 60, "drop_enabled": false, "cool_fan_pct": 100, "cool_end_bt_c": 45 })),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        let sid = charged(&h, p["id"].as_str().unwrap(), "No DROP step").await;
        let replay = format!("/api/sessions/{sid}/replay");
        let st = h.owner("GET", &replay, None).await.json();
        assert_eq!(st["auto_drop"], Value::Null, "{st}");
        let log = h.owner("GET", &format!("{replay}/log"), None).await.json();
        assert!(
            log.to_string()
                .contains("auto DROP off: the roaster's DROP step is off"),
            "{log}"
        );
        // A drive with no auto DROP refuses a cancel.
        let r = h
            .owner("PATCH", &replay, Some(json!({ "auto_drop": false })))
            .await;
        assert_eq!(r.status.as_u16(), 400);
    }
}
