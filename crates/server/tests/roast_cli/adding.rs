//! Adding a roaster: a logger mapped and checked, one found streaming, machines from Artisan's catalogue, Phidgets boards.

use roastwarden_server::test_support::{Harness, OWNER_KEY};

use crate::common::{error_code, roast};

/// Adding a roaster the way an agent would (docs/add-roaster.md): `roaster add` per transport
/// and preset writes the roaster, its connection and its setup at control off; `map --try`
/// writes nothing, `map` saves; `check` reads; `check --write` is the owner key's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_adds_a_logger_and_maps_and_checks_it() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    let ro = h.token("read_only").await;

    let r = roast(
        &url,
        Some(ro.as_str()),
        &[
            "roaster",
            "add",
            "tc4_popper",
            "--transport",
            "serial_tc4",
            "--port",
            "/dev/ttyUSB0",
        ],
    )
    .await;
    assert_eq!(r.code, 5, "{:?}", r.json);

    let r = roast(
        &url,
        Some(admin.as_str()),
        &[
            "roaster",
            "add",
            "tc4_popper",
            "--transport",
            "serial_tc4",
            "--port",
            "/dev/ttyUSB0",
            "--preset",
            "stock-aartisan",
            "--kind",
            "logger",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["control"], "off");
    let dev = h.owner("GET", "/api/devices/tc4_popper", None).await.json();
    assert_eq!(dev["status"], "active", "{dev}");
    let conn = &dev["connections"][0];
    assert_eq!(conn["protocol"], "serial_tc4");
    assert_eq!(conn["config"]["port"], "/dev/ttyUSB0");
    assert_eq!(conn["config"]["reply_format"], "active");
    let setup = h
        .owner("GET", "/api/roaster/tc4_popper/setup", None)
        .await
        .json();
    assert_eq!(
        (setup["kind"].as_str(), setup["control"].as_str()),
        (Some("logger"), Some("off"))
    );

    // A preset for another transport is a usage error, before anything is made
    let r = roast(
        &url,
        Some(admin.as_str()),
        &[
            "roaster",
            "add",
            "x1",
            "--transport",
            "mqtt",
            "--preset",
            "stock-aartisan",
        ],
    )
    .await;
    assert_eq!(r.code, 2, "{:?}", r.json);

    // Tasmota over MQTT: its telemetry topic and a starting mapping
    let r = roast(
        &url,
        Some(admin.as_str()),
        &[
            "roaster",
            "add",
            "tas1",
            "--transport",
            "mqtt",
            "--preset",
            "tasmota",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/tas1", None).await.json();
    assert_eq!(
        dev["connections"][0]["config"]["telemetry_topic"],
        "tele/tas1/SENSOR"
    );
    // No setup without --kind: off by default
    let setup = h.owner("GET", "/api/roaster/tas1/setup", None).await.json();
    assert_eq!(setup["control"], "off");

    // --try writes nothing; with no message yet it makes no sample (exit 1)
    let r = roast(
        &url,
        Some(ro.as_str()),
        &[
            "roaster",
            "map",
            "tas1",
            "--try",
            "--field",
            "et=probe.ET",
            "--units",
            "F",
        ],
    )
    .await;
    assert_eq!(r.code, 1, "{:?} {}", r.json, r.stderr);
    assert!(
        r.json["fields"].as_array().is_some_and(|f| f.len() == 2),
        "{:?}",
        r.json
    );
    let dev = h.owner("GET", "/api/devices/tas1", None).await.json();
    assert!(
        dev["connections"][0]["config"]["ingest"]["fields"]
            .get("et")
            .is_none()
    );
    // Saving it merges with what is there
    let r = roast(
        &url,
        Some(admin.as_str()),
        &[
            "roaster",
            "map",
            "tas1",
            "--field",
            "et=probe.ET",
            "--units",
            "F",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/tas1", None).await.json();
    let ingest = &dev["connections"][0]["config"]["ingest"];
    assert_eq!(ingest["units"], "F");
    assert_eq!(ingest["fields"]["bt"], "MAX31855.Temperature");
    assert_eq!(ingest["fields"]["et"], "probe.ET");

    // The check reads (no samples: fail, exit 1); the write check is the owner key's
    let r = roast(&url, None, &["roaster", "check", "tas1"]).await;
    assert_eq!(r.code, 1, "{:?}", r.json);
    assert_eq!(r.json["verdict"], "fail");
    let r = roast(
        &url,
        Some(admin.as_str()),
        &["roaster", "check", "tas1", "--write"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (5, "OWNER_ONLY"), "{:?}", r.json);
    let r = roast(
        &url,
        Some(OWNER_KEY),
        &["roaster", "check", "tas1", "--write"],
    )
    .await;
    assert_eq!((r.code, error_code(&r)), (5, "CONTROL_OFF"), "{:?}", r.json);
}

/// A machine from Artisan's catalogue (docs/artisan-machines.md): `roaster machines` finds it,
/// `add --artisan --dry-run` makes nothing, `add` without the serial port is a usage error,
/// and with it the roaster, its Modbus RTU connection, a register map on several slaves and a
/// setup at control off with the sliders' outputs. A machine Artisan reads through a device
/// Roastwarden has no transport for says why.
/// A roaster the server found streaming (MQTT auto-discovery: pending, the MQTT connection it
/// was heard on) is set up in place by `roaster add` under its own id, as the wizard's Set up
/// does (docs/connect-your-roaster.md); one added already is a conflict, never a second roaster.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_roaster_found_streaming_is_set_up_in_place() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    // What discovery leaves: the roaster pending, with its MQTT connection
    let found = |id: &'static str| {
        let h = &h;
        async move {
            let d = h
                .owner(
                    "POST",
                    "/api/devices",
                    Some(serde_json::json!({ "name": id, "device_id": id, "description": "Auto-discovered via MQTT" })),
                )
                .await
                .json();
            let c = h
                .owner(
                    "POST",
                    &format!("/api/devices/{}/connections", d["id"].as_str().unwrap()),
                    Some(serde_json::json!({ "protocol": "mqtt", "config": { "topic_prefix": format!("roaster/{id}"), "qos": 0 } })),
                )
                .await
                .json();
            c["id"].as_str().unwrap().to_string()
        }
    };
    let heard_on = found("diy_popper").await;
    let add = [
        "roaster",
        "add",
        "diy_popper",
        "--transport",
        "mqtt",
        "--name",
        "DIY popper",
        "--kind",
        "fluid_bed_electric",
    ];
    let r = roast(&url, Some(admin.as_str()), &add).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["found"], true, "{:?}", r.json);
    let dev = h.owner("GET", "/api/devices/diy_popper", None).await.json();
    assert_eq!(
        (dev["status"].as_str(), dev["name"].as_str()),
        (Some("active"), Some("DIY popper"))
    );
    let conns = dev["connections"].as_array().unwrap();
    assert_eq!(conns.len(), 1, "changed, not doubled: {dev}");
    assert_eq!(conns[0]["id"], heard_on.as_str());
    assert_eq!(conns[0]["config"]["topic_prefix"], "roaster/diy_popper");
    let setup = h
        .owner("GET", "/api/roaster/diy_popper/setup", None)
        .await
        .json();
    assert_eq!(
        (setup["kind"].as_str(), setup["control"].as_str()),
        (Some("fluid_bed_electric"), Some("off"))
    );

    // Added already: a conflict, nothing changed; the API says so too (it was a 500)
    let r = roast(&url, Some(admin.as_str()), &add).await;
    assert_eq!(r.code, 6, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["error"]["code"], "CONFLICT");
    let r = h
        .owner(
            "POST",
            "/api/devices",
            Some(serde_json::json!({ "name": "x", "device_id": "diy_popper" })),
        )
        .await;
    assert_eq!(r.status, 409, "{:?}", r.json());

    // Found on MQTT, added as something else: the connection is replaced
    found("diy_ws").await;
    let r = roast(
        &url,
        Some(admin.as_str()),
        &[
            "roaster",
            "add",
            "diy_ws",
            "--transport",
            "websocket",
            "--device-url",
            "ws://192.168.1.61:81/",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/diy_ws", None).await.json();
    let conns = dev["connections"].as_array().unwrap();
    assert_eq!(conns.len(), 1, "{dev}");
    assert_eq!(conns[0]["protocol"], "websocket");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_adds_a_roaster_from_artisans_machine_catalogue() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;

    let r = roast(&url, None, &["roaster", "machines", "berto", "autonics"]).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json[0]["key"], "Berto/Autonics_Control", "{:?}", r.json);
    let r = roast(&url, None, &["roaster", "machines", "giesen"]).await;
    assert_eq!(r.code, 4, "S7 machines are not listed without --all");
    let r = roast(&url, None, &["roaster", "machines", "giesen", "--all"]).await;
    assert_eq!(r.code, 0);
    assert_eq!(r.json[0]["supported"], false);
    assert!(r.json[0]["why"].as_str().unwrap().contains("Siemens S7"));

    let add = |extra: &'static [&'static str]| {
        let mut a = vec![
            "roaster",
            "add",
            "berto1",
            "--artisan",
            "Berto/Autonics_Control",
        ];
        a.extend_from_slice(extra);
        a
    };
    let r = roast(&url, Some(admin.as_str()), &add(&["--dry-run"])).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["made"], false);
    assert_eq!(r.json["missing"], serde_json::json!(["port"]));
    assert_eq!(
        h.owner("GET", "/api/devices/berto1", None).await.status,
        404,
        "--dry-run makes nothing"
    );
    let r = roast(&url, Some(admin.as_str()), &add(&[])).await;
    assert_eq!(r.code, 2, "the serial port is needed: {:?}", r.json);

    let r = roast(
        &url,
        Some(admin.as_str()),
        &add(&["--port", "/dev/ttyNONE"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["control"], "off");
    let dev = h.owner("GET", "/api/devices/berto1", None).await.json();
    assert_eq!(dev["name"], "Berto Autonics Control");
    let conn = &dev["connections"][0];
    assert_eq!(conn["protocol"], "modbus_rtu");
    assert_eq!(conn["config"]["port"], "/dev/ttyNONE");
    assert_eq!(conn["config"]["baud"], 9600);
    let id = dev["id"].as_str().unwrap();
    let map = h
        .owner("GET", &format!("/api/devices/{id}/register-map"), None)
        .await
        .json();
    let regs = map.as_array().unwrap();
    let et = regs.iter().find(|r| r["role"] == "et").unwrap();
    assert_eq!(et["unit_id"], 2, "ET is a controller of its own: {et}");
    assert!(
        regs.iter()
            .any(|r| r["role"] == "heater" && r["writable"] == true)
    );
    let setup = h
        .owner("GET", "/api/roaster/berto1/setup", None)
        .await
        .json();
    assert_eq!(setup["control"], "off");
    assert_eq!(setup["outputs"]["heater"]["label"], "Burner");
    assert_eq!(setup["outputs"]["fan"]["label"], "Air");
    let keys: Vec<&str> = setup["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["dt", "it"], "{setup}");

    // Unsupported: why, and nothing made
    let r = roast(
        &url,
        Some(admin.as_str()),
        &["roaster", "add", "g1", "--artisan", "Giesen/W6A"],
    )
    .await;
    assert_eq!(r.code, 1, "{:?}", r.json);
    assert_eq!(h.owner("GET", "/api/devices/g1", None).await.status, 404);
}

/// Phidgets boards (docs/phidgets.md): by hand with --transport phidget22, and
/// a Diedrich's 1048 from Artisan's catalogue with --host and --password; the password is saved
/// and never shown, the board's inputs mapped as Artisan has them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phidgets_boards_are_added_by_hand_and_from_artisan() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    let admin = Some(admin.as_str());

    let r = roast(
        &url,
        admin,
        &["roaster", "add", "drum1", "--transport", "phidget22"],
    )
    .await;
    assert_eq!(r.code, 2, "{:?} {}", r.json, r.stderr);
    let said = format!("{} {}", r.json, r.stderr);
    assert!(said.contains("--host"), "{said}");
    let r = roast(
        &url,
        admin,
        &[
            "roaster",
            "add",
            "drum1",
            "--transport",
            "phidget22",
            "--host",
            "127.0.0.1",
            "--port",
            "1",
            "--password",
            "pw1",
            "--serial-number",
            "370001",
            "--thermocouple",
            "J",
            "--kind",
            "logger",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/drum1", None).await.json();
    let c = &dev["connections"][0];
    assert_eq!(c["protocol"], "phidget22");
    assert_eq!(c["config"]["password"], "********", "never shown");
    assert_eq!(c["config"]["thermocouple_type"], "J");
    let r = roast(&url, admin, &["--text", "roaster", "show", "drum1"]).await;
    let text = r.json.as_str().unwrap_or_default().to_string();
    assert!(
        text.contains("connection phidget22 127.0.0.1:1, board #370001, type J, password saved, BT ch0, ET ch1"),
        "{text}"
    );
    // An extra input keeps BT and ET.
    let r = roast(
        &url,
        admin,
        &["roaster", "map", "drum1", "--field", "exhaust=ch2"],
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/drum1", None).await.json();
    let c = &dev["connections"][0]["config"];
    assert_eq!(
        c["ingest"]["fields"],
        serde_json::json!({ "bt": "ch0", "et": "ch1", "exhaust": "ch2" })
    );
    assert_eq!(c["password"], "********", "the map kept the saved password");

    let add = |extra: &[&'static str]| {
        let mut a = vec![
            "roaster",
            "add",
            "diedrich",
            "--artisan",
            "Diedrich/6_Sensor_Data_Logging",
        ];
        a.extend_from_slice(extra);
        a
    };
    let r = roast(&url, admin, &add(&["--dry-run"])).await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["protocol"], "phidget22");
    assert_eq!(r.json["missing"], serde_json::json!(["host"]));
    let r = roast(
        &url,
        admin,
        &add(&["--host", "192.168.1.60", "--password", "beans42"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/diedrich", None).await.json();
    let c = &dev["connections"][0]["config"];
    assert_eq!(c["host"], "192.168.1.60");
    assert_eq!(c["password"], "********");
    assert_eq!(c["ingest"]["fields"]["inlet_temp"]["path"], "ch3");
    let setup = h
        .owner("GET", "/api/roaster/diedrich/setup", None)
        .await
        .json();
    assert_eq!(setup["control"], "off");
    let keys: Vec<&str> = setup["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["total_exhaust", "inlet_temp", "board_temp"]);
}

/// Artisan's machines on other transports: a WebSocket Probat takes its
/// address with --host and keeps its poll, mapping and templates; a ROEST needs the MQTT topic
/// its data reaches this server's broker on; a Kaleido is a TC4 with its commands; a Delta DTA
/// line is Modbus ASCII.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn artisans_websocket_mqtt_tc4_and_ascii_machines_are_added() {
    let h = Harness::with_open_reads().await;
    let url = h.serve().await;
    let admin = h.token("admin").await;
    let add = |id: &'static str, machine: &'static str, extra: &[&'static str]| {
        let mut a = vec!["roaster", "add", id, "--artisan", machine];
        a.extend_from_slice(extra);
        a
    };
    let conn = |dev: &serde_json::Value| dev["connections"][0].clone();

    // WebSocket: the address is the owner's; the dry run shows the mapping.
    let r = roast(
        &url,
        Some(admin.as_str()),
        &add("probat1", "Probat/P_Series_III", &["--dry-run"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    assert_eq!(r.json["missing"], serde_json::json!(["host"]));
    assert_eq!(r.json["protocol"], "websocket");
    let r = roast(
        &url,
        Some(admin.as_str()),
        &add("probat1", "Probat/P_Series_III", &["--host", "127.0.0.2"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let dev = h.owner("GET", "/api/devices/probat1", None).await.json();
    let c = conn(&dev);
    assert_eq!(c["protocol"], "websocket");
    assert_eq!(c["config"]["url"], "ws://127.0.0.2:80/WebSocket");
    assert_eq!(
        c["config"]["ingest"]["fields"]["bt"]["path"],
        "data.beanTemp"
    );
    assert!(
        c["config"]["poll"]["message"]
            .as_str()
            .unwrap()
            .contains("getCurrentRoastingStep")
    );
    assert!(
        c["config"]["commands"]["heater"]["payload"]
            .as_str()
            .unwrap()
            .contains("{value}")
    );
    let setup = h
        .owner("GET", "/api/roaster/probat1/setup", None)
        .await
        .json();
    assert_eq!(setup["control"], "off");
    assert!(setup["outputs"]["drum"].is_object(), "{setup}");

    // MQTT: the topic is the owner's (bridged from ROEST's broker).
    let r = roast(&url, Some(admin.as_str()), &add("roest1", "ROEST/100", &[])).await;
    assert_eq!(r.code, 2, "the topic is needed: {:?} {}", r.json, r.stderr);
    let said = format!("{} {}", r.json, r.stderr);
    assert!(said.contains("--telemetry-topic"), "{said}");
    let r = roast(
        &url,
        Some(admin.as_str()),
        &add(
            "roest1",
            "ROEST/100",
            &["--telemetry-topic", "roest/r100/data"],
        ),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let c = conn(&h.owner("GET", "/api/devices/roest1", None).await.json());
    assert_eq!(c["protocol"], "mqtt");
    assert_eq!(c["config"]["telemetry_topic"], "roest/r100/data");
    assert_eq!(c["config"]["ingest"]["fields"]["et"]["path"], "data.et");

    // TC4: the serial port; the Kaleido's drum is its IO3 template.
    let r = roast(
        &url,
        Some(admin.as_str()),
        &add("kaleido1", "Kaleido/Legacy", &["--port", "/dev/ttyNONE"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let c = conn(&h.owner("GET", "/api/devices/kaleido1", None).await.json());
    assert_eq!(c["protocol"], "serial_tc4");
    assert_eq!(c["config"]["port"], "/dev/ttyNONE");
    assert_eq!(c["config"]["commands"]["drum"]["payload"], "IO3,{value}");

    // Modbus ASCII: a Delta DTA line at 7E1.
    let r = roast(
        &url,
        Some(admin.as_str()),
        &add("delta1", "Nordic/DeltaDTA", &["--port", "/dev/ttyNONE"]),
    )
    .await;
    assert_eq!(r.code, 0, "{:?} {}", r.json, r.stderr);
    let c = conn(&h.owner("GET", "/api/devices/delta1", None).await.json());
    assert_eq!(c["protocol"], "modbus_rtu");
    assert_eq!(c["config"]["framing"], "ascii");
    assert_eq!(c["config"]["data_bits"], 7);
}
