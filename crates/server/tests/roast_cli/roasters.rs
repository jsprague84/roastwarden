//! The roasters read out, a roaster's firmware downloaded, its MQTT password added.

use roastwarden_server::test_support::{Harness, OWNER_KEY};
use serde_json::Value;

use crate::common::{error_code, roast};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_outs_of_roasters_settings_and_notifications() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    let read = h.token("read_only").await;

    // Which token this is
    let r = roast(&url, Some(&read), &["status"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    assert_eq!(r.json["credential"], "token");
    assert_eq!(r.json["token"]["preset"], "read_only");

    h.owner(
        "POST",
        "/api/test/emit-telemetry/virtual_roaster",
        Some(
            serde_json::json!({ "beanTemp": 150.0, "envTemp": 200.0, "heater": 40, "fan": 70,
            "setpoint": 200.0, "Kp": 5.5, "Ki": 0.085, "Kd": 0.0, "uptimeMs": 4000 }),
        ),
    )
    .await;
    // The sample reaches the cache a moment after the call returns.
    assert!(
        roastwarden_server::test_support::eventually(|| async {
            h.owner("GET", "/api/roaster/virtual_roaster/contract", None)
                .await
                .status
                .is_success()
        })
        .await
    );

    // The device contract: readings, caps, gains, and the boot check of its first sample
    // (the server holds no gains for it, so nothing was sent)
    let r = roast(
        &url,
        Some(&read),
        &["roaster", "contract", "virtual_roaster"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let c = &r.json;
    assert_eq!(
        c["reads"],
        serde_json::json!({"bt": true, "et": true, "heater": true, "fan": true})
    );
    assert_eq!(c["caps"]["pid"], true, "{c}");
    assert_eq!(c["gains"]["reported"]["kp"], 5.5, "{c}");
    assert!(
        c["gains"]["held"].is_null() && c["gains"]["matches"].is_null(),
        "{c}"
    );
    assert_eq!(c["gains"]["last_boot"]["why"], "server_start", "{c}");
    assert!(c["gains"]["last_boot"]["sent"].is_null(), "{c}");
    let r = roast(&url, Some(&read), &["roaster", "contract", "nope"]).await;
    assert_eq!(r.code, 4);

    // Busy: the virtual roaster's heater (40 %) is listed and holds nothing...
    let r = roast(&url, Some(&read), &["busy"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["busy"], false);
    assert_eq!(r.json["roasters"][0]["device_id"], "virtual_roaster");

    for cmd in [
        &["roasters"][..],
        &["roaster", "show", "virtual_roaster"],
        &["roaster", "contract", "virtual_roaster"],
        &["roaster", "tuning", "virtual_roaster"],
        &["live"],
        &["live", "--roaster", "virtual_roaster"],
        &["notifications"],
        &["settings"],
        &["settings", "roast_alarms"],
    ] {
        let r = roast(&url, Some(&read), cmd).await;
        assert_eq!(r.code, 0, "{cmd:?}: {:?} {}", r.json, r.stderr);
    }
    let r = roast(&url, None, &["roaster", "show", "nope"]).await;
    assert_eq!(r.code, 4);

    // Settings: one, several, a value from stdin is not needed here
    let r = roast(
        &url,
        Some(&read),
        &["settings", "set", "dose_espresso_g=19"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (5, "TOKEN_SCOPE"));
    let r = roast(
        &url,
        Some(&admin),
        &["settings", "set", "dose_espresso_g=19"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(
        &url,
        Some(&admin),
        &["settings", "set", "dose_espresso_g=17", "dose_filter_g=16"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let r = roast(&url, None, &["settings", "dose_filter_g"]).await;
    assert_eq!(r.json["value"], "16");
    let r = roast(
        &url,
        Some(&admin),
        &["settings", "set", "dose_espresso_g=900"],
    )
    .await;
    assert_eq!(r.code, 6, "out of range: {:?}", r.json);
    let r = roast(&url, Some(&admin), &["settings", "set", "nokey"]).await;
    assert_eq!(r.code, 2);
    // A key the server reads but nobody set is unset, a secret too; a typo is not found
    for key in ["label_brand", "notify_gotify_token"] {
        let r = roast(&url, Some(&read), &["settings", key]).await;
        assert_eq!(r.code, 0, "{key}: {:?} {}", r.json, r.stderr);
        assert_eq!(r.json["is_set"], false, "{key}");
        let r = roast(&url, Some(&read), &["--text", "settings", key]).await;
        assert_eq!(r.json, Value::String(format!("{key}: unset\n")), "{key}");
    }
    let r = roast(&url, Some(&read), &["settings", "label_brnad"]).await;
    assert_eq!(r.code, 4, "{:?}", r.json);
    let hint = r.json["error"]["hint"].as_str().unwrap_or_default();
    assert!(hint.contains("spelling"), "{:?}", r.json);

    // A copy written and checked
    let r = roast(&url, Some(&admin), &["backups", "--now"]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);
    let name = r.json["file"]["name"].as_str().unwrap().to_string();
    let r = roast(&url, Some(&admin), &["backups", "--check", &name]).await;
    assert_eq!(r.code, 0, "{:?}", r.json);

    // openapi --find says which command does it
    let r = roast(&url, None, &["openapi", "--offline", "--find", "purchases"]).await;
    let row = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["path"] == "/api/purchases");
    assert_eq!(
        row.map(|o| o["roast"].clone()),
        Some(serde_json::json!("roast order (inventory token)")),
        "{}",
        r.json
    );
}

/// An ESP32's firmware for a roaster the owner builds (docs/esp32-fluid-bed.md): what the server
/// can fill in, then the zip with the id, the server's address and the board filled in, and what
/// is left to fill in by hand.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_downloads_a_roasters_firmware() {
    let h = Harness::new().await;
    let url = h.serve().await;
    let dir = h.db_path().parent().unwrap().join("firmware");
    std::fs::create_dir_all(&dir).unwrap();

    let r = roast(&url, Some(OWNER_KEY), &["roaster", "firmware"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["kinds"][1]["kind"], "fluid_bed");
    assert_eq!(r.json["port"], 1883);

    // roast calls the server at 127.0.0.1: the ESP32 cannot reach that
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "firmware", "new_roaster"],
    )
    .await;
    assert_eq!(r.code, 2, "{:?}", r.json);
    assert_eq!(error_code(&r), "USAGE");

    let file = dir.join("fw.zip").to_str().unwrap().to_string();
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &[
            "roaster",
            "firmware",
            "new_roaster",
            "--kind",
            "fluid-bed",
            "--address",
            "192.168.1.50",
            "--board",
            "max6675",
            "-o",
            &file,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    // No MQTT_DEVICES here and no --wifi: both stay placeholders
    assert_eq!(r.json["missing"], serde_json::json!(["password", "wifi"]));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
    let mut sketch = String::new();
    std::io::Read::read_to_string(
        &mut zip
            .by_name(
                "roastwarden-fluid-bed-new_roaster/roastwarden-fluid-bed/roastwarden-fluid-bed.ino",
            )
            .unwrap(),
        &mut sketch,
    )
    .unwrap();
    assert!(sketch.contains("const char* ROASTER_ID = \"new_roaster\";"));
    assert!(sketch.contains("const char* SERVER_ADDRESS = \"192.168.1.50\";"));
    assert!(sketch.contains("\n#define BOARD_MAX6675\n"));

    // A bad id is the server's 400, with its reason
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "firmware", "a.b", "--address", "192.168.1.50"],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    assert_eq!(error_code(&r), "BAD_REQUEST");
    assert!(r.json.to_string().contains("roaster_id"), "{:?}", r.json);
}

/// A new roaster's MQTT password from `roast` (docs/add-roaster.md, "MQTT passwords"): on the
/// built-in broker it is added at once, printed once, filled into the firmware, changed and
/// removed; `MQTT_DEVICES`' stay as they are; a server on a broker of its own adds none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_adds_a_roasters_mqtt_password() {
    let h = Harness::with_built_in_broker().await;
    let url = h.serve().await;

    let r = roast(&url, Some(OWNER_KEY), &["roaster", "password", "list"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["built_in_broker"], true);
    assert_eq!(r.json["passwords"][0]["roaster_id"], "env_roaster");
    assert_eq!(r.json["passwords"][0]["source"], "env");
    assert!(r.json["passwords"][0].get("password").is_none());

    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "add", "my_roaster"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let first = r.json["password"].as_str().unwrap().to_string();
    assert_eq!(first.len(), 32);

    // The firmware has it now: only the Wi-Fi is left
    let r = roast(&url, Some(OWNER_KEY), &["roaster", "firmware"]).await;
    assert_eq!(r.json["roasters"][1]["id"], "my_roaster", "{:?}", r.json);
    let dir = h.db_path().parent().unwrap().join("pw-firmware");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fw.zip").to_str().unwrap().to_string();
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &[
            "roaster",
            "firmware",
            "my_roaster",
            "--address",
            "192.168.1.50",
            "-o",
            &file,
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["missing"], serde_json::json!(["wifi"]));
    let zip = std::fs::read(&file).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut sketch = String::new();
    std::io::Read::read_to_string(
        &mut zip
            .by_name("roastwarden-starter-my_roaster/roastwarden-starter/roastwarden-starter.ino")
            .unwrap(),
        &mut sketch,
    )
    .unwrap();
    assert!(sketch.contains(&format!("const char* ROASTER_PASSWORD = \"{first}\";")));

    // Twice is a conflict; a bad id the server's 400
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "add", "my_roaster"],
    )
    .await;
    assert_eq!(r.code, 6, "{:?}", r.json);
    assert_eq!(error_code(&r), "CONFLICT");
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "add", "a/b"],
    )
    .await;
    assert_eq!(error_code(&r), "BAD_REQUEST", "{:?}", r.json);

    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "new", "my_roaster"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_ne!(r.json["password"], first.as_str());

    // MQTT_DEVICES' is changed in .env
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "remove", "env_roaster"],
    )
    .await;
    assert_eq!(error_code(&r), "CONFLICT", "{:?}", r.json);
    assert!(r.json.to_string().contains("MQTT_DEVICES"), "{:?}", r.json);

    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "remove", "my_roaster"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let r = roast(&url, Some(OWNER_KEY), &["roaster", "password", "list"]).await;
    assert_eq!(r.json["passwords"].as_array().unwrap().len(), 1);
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "remove", "my_roaster"],
    )
    .await;
    assert_eq!(r.code, 4, "{:?}", r.json);

    // Without a token's admin preset: refused
    let read = h.token("read_only").await;
    let r = roast(&url, Some(&read), &["roaster", "password", "list"]).await;
    assert_eq!(error_code(&r), "TOKEN_SCOPE", "{:?}", r.json);

    // A server on a broker of its own adds none
    let outside = Harness::with_open_reads().await;
    let url = outside.serve().await;
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "password", "add", "my_roaster"],
    )
    .await;
    assert_eq!(error_code(&r), "CONFLICT", "{:?}", r.json);
    assert!(
        r.json.to_string().contains("broker of its own"),
        "{:?}",
        r.json
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
