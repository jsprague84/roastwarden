//! A roast's record: weighed, its first crack moved, edited, cupped, exported.

use roastwarden_server::test_support::Harness;
use serde_json::Value;

use crate::common::{error_code, finished_roast, roast};

/// After a roast on the real roaster, an agent records the roasted weight and moves first crack
/// off a lone early crack (as a recorded roast needed). The record of a finished roast reaches
/// no roaster, so the real-hardware rule lets a token edit it; a roast still running stays
/// behind the switch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_weighs_a_finished_roast_and_moves_first_crack() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let t = h.token("roast").await;
    let t = Some(t.as_str());
    let real = "esp32_roaster_01";

    let s = h
        .owner(
            "POST",
            "/api/sessions",
            Some(
                serde_json::json!({ "name": "batch 4", "device_id": real, "green_weight": 100.0 }),
            ),
        )
        .await
        .json();
    let id = s["id"].as_str().unwrap().to_string();
    h.owner("POST", &format!("/api/sessions/{id}/start"), None)
        .await;
    for sec in (0..=620).step_by(5) {
        let bt = 25.0 + 0.3 * f64::from(sec);
        h.owner(
            "POST",
            &format!("/api/sessions/{id}/telemetry"),
            Some(serde_json::json!({ "elapsed_seconds": sec, "bean_temp": bt })),
        )
        .await;
    }
    for (kind, at) in [
        ("charge", 5.0),
        ("first_crack_start", 381.0),
        ("drop", 611.0),
    ] {
        h.owner(
            "POST",
            &format!("/api/sessions/{id}/events"),
            Some(serde_json::json!({ "event_type": kind, "elapsed_seconds": at })),
        )
        .await;
    }

    // Still roasting: the switch holds even a landmark
    let r = roast(
        &url,
        t,
        &[
            "landmark",
            "latest",
            "fc",
            "--at",
            "7:30",
            "--roaster",
            real,
        ],
    )
    .await;
    assert_eq!(
        (r.code, error_code(&r)),
        (5, "REAL_HARDWARE"),
        "{:?}",
        r.json
    );

    h.owner("POST", &format!("/api/sessions/{id}/complete"), None)
        .await;

    let r = roast(&url, t, &["weigh", "latest", "85.75g", "--roaster", real]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let loss = r.json["weight_loss_pct"].as_f64().unwrap();
    assert!((loss - 14.25).abs() < 0.01, "{loss}");

    let r = roast(
        &url,
        t,
        &[
            "landmark",
            &id,
            "fc",
            "--at",
            "7:30",
            "--note",
            "steady cracking",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    // 7:30 from CHARGE at 5 s is 455 s from the start; BT read from the roast itself
    assert_eq!(r.json["event"]["elapsed_seconds"], 455.0);
    let bt = r.json["event"]["temperature"].as_f64().unwrap();
    assert!((bt - (25.0 + 0.3 * 455.0)).abs() < 1.6, "{bt}");
    // First crack is counted from CHARGE, as the agent gave it.
    assert_eq!(r.json["first_crack_time"], 450);
    let dtr = r.json["development_time_ratio"].as_f64().unwrap();
    assert!((dtr - 156.0 / 606.0).abs() < 1e-3, "{dtr}");

    let r = roast(&url, t, &["landmark", &id, "crackle", "--at", "7:30"]).await;
    assert_eq!(r.code, 2, "{:?}", r.json);
    let r = roast(&url, t, &["landmark", &id, "fc", "--at", "7:75"]).await;
    assert_eq!(r.code, 2, "{:?}", r.json);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_keeps_a_roasts_record() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let roast_t = h.token("roast").await;
    let admin = h.token("admin").await;
    let inv = h.token("inventory").await;
    let t = Some(roast_t.as_str());
    let real = "esp32_roaster_01";
    let id = finished_roast(&h, real, "batch 7").await;
    let dir = h.db_path().parent().unwrap().join("files");
    std::fs::create_dir_all(&dir).unwrap();

    // Notes, rating, colour on the real roaster's finished roast
    let r = roast(
        &url,
        t,
        &[
            "edit",
            "latest",
            "--roaster",
            real,
            "--notes",
            "bright",
            "--rating",
            "4",
            "--color-ground",
            "62",
            "--color-scale",
            "agtron",
            "--level",
            "Medium-light",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["rating"], 4);
    assert_eq!(r.json["target_roast_level"], "medium_light");
    let r = roast(&url, t, &["edit", &id, "--add-note", "a bit thin"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["notes"], "bright\na bit thin");
    let r = roast(&url, t, &["edit", &id]).await;
    assert_eq!((r.code, error_code(&r)), (2, "USAGE"), "{:?}", r.json);
    let r = roast(&url, t, &["edit", &id, "--rating", "9"]).await;
    assert_eq!(r.code, 2, "clap refuses a rating above 5");
    // An inventory token keeps the green coffee, not a roast's record
    let r = roast(&url, Some(&inv), &["edit", &id, "--notes", "x"]).await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"), "{:?}", r.json);

    // Cupping: a first score fills the rest with the form's start (6.0, and 10 for the cup
    // attributes), a second keeps what is there
    let r = roast(&url, t, &["cup", &id]).await;
    assert_eq!((r.code, &r.json), (0, &Value::Null));
    let r = roast(
        &url,
        t,
        &[
            "cup",
            &id,
            "acidity=7.5",
            "aroma=8",
            "--notes",
            "stone fruit",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let score = |j: &Value, name: &str| {
        j["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["attribute_name"] == name)
            .and_then(|a| a["score"].as_f64())
    };
    assert_eq!(score(&r.json, "fragrance_aroma"), Some(8.0));
    assert_eq!(score(&r.json, "body"), Some(6.0));
    assert_eq!(score(&r.json, "sweetness"), Some(10.0));
    // 8 + 7.5 + 6 × 5 + 10 × 3 = 75.5
    assert_eq!(r.json["total_score"], 75.5);
    assert_eq!(r.json["final_score"], 75.5);
    let r = roast(
        &url,
        t,
        &[
            "cup",
            "latest",
            "--roaster",
            real,
            "body=7",
            "clean=8",
            "--taint",
            "1",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(score(&r.json, "acidity"), Some(7.5), "kept");
    assert_eq!(score(&r.json, "body"), Some(7.0));
    assert_eq!(r.json["notes"], "stone fruit", "kept");
    // 75.5 + 1 − 2 = 74.5, less one tainted cup's 2 = 72.5
    assert_eq!(r.json["total_score"], 74.5);
    assert_eq!(
        (r.json["taint_cups"].as_i64(), r.json["defects"].as_i64()),
        (Some(1), Some(2))
    );
    assert_eq!(r.json["final_score"], 72.5);
    assert_eq!(r.json["band"], "below specialty");
    let r = roast(&url, t, &["cup", &id, "--fault", "1"]).await;
    assert_eq!(r.json["taint_cups"], 1, "kept");
    assert_eq!(r.json["final_score"], 68.5);
    let r = roast(&url, t, &["cup", &id, "crema=7"]).await;
    assert_eq!(r.code, 2);
    let r = roast(&url, t, &["cup", &id, "sweetness=7"]).await;
    assert_eq!(r.code, 2, "2 points a cup");
    let r = roast(&url, t, &["cup", &id, "--taint", "6"]).await;
    assert_eq!(r.code, 2, "five cups");
    let r = roast(&url, t, &["cup", &id, "--taint", "3", "--fault", "3"]).await;
    assert_eq!((r.code, error_code(&r)), (6, "BAD_REQUEST"), "{:?}", r.json);
    // `show` has it, and takes `latest`
    let r = roast(&url, None, &["show", "latest", "--roaster", real]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["id"], id.as_str());
    assert!(r.json["cupping"]["attributes"].is_array());

    // A wrong landmark out, and back
    let r = roast(&url, t, &["landmark", &id, "dry", "--rm"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let events = h
        .owner("GET", &format!("/api/sessions/{id}/events"), None)
        .await
        .json();
    assert!(
        !events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event_type"] == "drying_end")
    );
    let r = roast(&url, t, &["landmark", &id, "dry", "--rm"]).await;
    assert_eq!(r.code, 4, "{:?}", r.json);
    let r = roast(&url, t, &["landmark", &id, "dry"]).await;
    assert_eq!(r.code, 2, "--at or --rm");

    // Export to a file, to stdout; import it back as another roast (admin)
    let file = dir.join("b7.alog");
    let r = roast(&url, None, &["export", &id, "-o", file.to_str().unwrap()]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert!(r.json["bytes"].as_u64().unwrap() > 100);
    let alog = std::fs::read_to_string(&file).unwrap();
    assert!(alog.contains("timex"), "an Artisan file");
    let r = roast(&url, None, &["export", &id, "--csv", "-o", "-"]).await;
    assert!(
        r.json.as_str().is_some_and(|t| t.lines().count() > 100),
        "CSV on stdout"
    );
    let r = roast(
        &url,
        t,
        &["import", file.to_str().unwrap(), "--name", "again"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"));
    let r = roast(
        &url,
        Some(&admin),
        &[
            "import",
            file.to_str().unwrap(),
            "--name",
            "again",
            "--roaster",
            "virtual_roaster",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let again = r.json["id"].as_str().unwrap().to_string();
    let r = roast(&url, Some(&admin), &["import", "/nonexistent.alog"]).await;
    assert_eq!((r.code, error_code(&r)), (4, "NO_FILE"));

    // The roast as a profile; its history; the profile's targets
    let r = roast(
        &url,
        Some(&admin),
        &["profile", "save", &id, "--name", "Batch 7 curve"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &[
            "profile",
            "edit",
            "Batch 7 curve",
            "--target-fc",
            "7:50",
            "--target-total",
            "10:05",
            "--target-drop-temp",
            "210",
            "--level",
            "light",
            "--purpose",
            "espresso",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["target_first_crack"], 470);
    assert_eq!(r.json["target_total_time"], 605);
    assert_eq!(r.json["roast_level"], "light");
    assert_eq!(r.json["auto_drop"], Value::Null, "auto DROP starts off");
    // Auto DROP by development: drop minus first crack, after first crack is marked
    let r = roast(
        &url,
        Some(&admin),
        &[
            "profile",
            "edit",
            "Batch 7 curve",
            "--auto-drop",
            "development",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["auto_drop"], "development");
    let r = roast(
        &url,
        Some(&admin),
        &["--text", "profile", "show", "Batch 7 curve"],
    )
    .await;
    assert!(
        r.json
            .as_str()
            .is_some_and(|t| t.contains("auto DROP 2:15 after first crack")),
        "{:?}",
        r.json
    );
    // The profile's own development range, over its level's 1:00–2:00
    let r = roast(
        &url,
        Some(&admin),
        &[
            "profile",
            "edit",
            "Batch 7 curve",
            "--expect",
            "development=1:45-2:45",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(
        r.json["expectations"],
        serde_json::json!({ "development_s": [105.0, 165.0] })
    );
    let r = roast(
        &url,
        Some(&admin),
        &["--text", "profile", "show", "Batch 7 curve"],
    )
    .await;
    assert!(
        r.json
            .as_str()
            .is_some_and(|t| t.contains("own ranges: development 1:45–2:45")),
        "{:?}",
        r.json
    );
    let r = roast(
        &url,
        Some(&admin),
        &[
            "profile",
            "edit",
            "Batch 7 curve",
            "--expect",
            "development=off",
        ],
    )
    .await;
    assert_eq!(r.json["expectations"], Value::Null, "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &["profile", "edit", "Batch 7 curve", "--auto-drop", "off"],
    )
    .await;
    assert_eq!(r.json["auto_drop"], Value::Null, "{:?}", r.json);
    // The roaster kind it is for: the spelling people use, `any` clears it, a wrong one is refused
    let r = roast(
        &url,
        Some(&admin),
        &["profile", "edit", "Batch 7 curve", "--kind", "drum-gas"],
    )
    .await;
    assert_eq!(r.json["roaster_kind"], "drum_gas", "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &["profile", "edit", "Batch 7 curve", "--kind", "wood fired"],
    )
    .await;
    assert_ne!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &["profile", "edit", "Batch 7 curve", "--kind", "any"],
    )
    .await;
    assert_eq!(r.json["roaster_kind"], Value::Null, "{:?}", r.json);
    let p = roast(&url, None, &["profile", "show", "Batch 7 curve"])
        .await
        .json;
    assert!(
        p["points"].as_array().unwrap().len() > 5,
        "the curve stayed"
    );
    let pid = p["id"].as_str().unwrap().to_string();
    h.owner(
        "PUT",
        &format!("/api/sessions/{id}"),
        Some(serde_json::json!({ "notes": "x" })),
    )
    .await;
    let profile_file = dir.join("p.alog");
    let r = roast(
        &url,
        None,
        &[
            "profile",
            "export",
            &pid,
            "--artisan",
            "-o",
            profile_file.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &[
            "profile",
            "import",
            profile_file.to_str().unwrap(),
            "--name",
            "Imported",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(&url, None, &["profile", "history", "fluidbed-espresso"]).await;
    assert_eq!((r.code, r.json.as_array().map(Vec::len)), (0, Some(0)));
    let r = roast(&url, Some(&admin), &["profile", "rm", "Imported"]).await;
    assert_eq!(r.code, 3, "a delete waits for the owner: {:?}", r.json);

    // Deleting a roast: by id only, and the owner approves it
    let r = roast(&url, Some(&admin), &["rm", "latest"]).await;
    assert_eq!(r.code, 2);
    let r = roast(&url, Some(&admin), &["rm", &again]).await;
    assert_eq!(r.code, 3, "{:?}", r.json);
    let approval = r.json["approval"]["id"].as_str().unwrap().to_string();
    h.owner("POST", &format!("/api/approvals/{approval}/approve"), None)
        .await;
    let r = roast(&url, Some(&admin), &["approval", &approval]).await;
    assert_eq!(
        (r.code, r.json["status"].as_str()),
        (0, Some("done")),
        "{:?}",
        r.json
    );
    let gone = h
        .owner("GET", &format!("/api/sessions/{again}"), None)
        .await;
    assert_eq!(gone.status, 404);
}
