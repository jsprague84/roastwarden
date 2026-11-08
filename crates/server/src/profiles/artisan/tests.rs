use serde_json::{json, Value};

use crate::profiles::types::{ProfileAction, ProfileWithPoints, TriggerKind};

use crate::roasters::channels::Channel;
use crate::roasts::types::{RoastSession, SessionTelemetry};

use super::literal::*;
use super::parse::*;
use super::profile::*;
use super::roast::*;
use super::*;

#[test]
fn deep_nesting_is_refused_not_a_stack_overflow() {
    // A million brackets would overflow the stack of a recursive parser.
    let deep = "[".repeat(1_000_000);
    let e = parse_alog(&deep).unwrap_err();
    assert!(format!("{e:#}").contains("nested more than 64"), "{e:#}");
    let deep_dicts = "{'a': ".repeat(100) + "1" + &"}".repeat(100);
    assert!(parse_python_literal(&deep_dicts).is_err());
    // What a real file needs is far below the limit.
    let fine = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
    assert!(parse_python_literal(&fine).is_ok());
}

#[test]
fn parses_python_literal_dict() {
    let src = r"{'title': 'Ethiopia (washed) \'Yirg\'', 'mode': 'C', 'timex': [0.0, 1.5, 3],
            'timeindex': [0, 10, -1, -1, -1, -1, 20, 0], 'flag': True, 'none': None,
            'weight': (250.0, 212.5, 'g'), 'computed': {'FCs_time': 480.5}, 'u': u'x', 'neg': -1,
            'exp': 1e-3, 'nan': nan}";
    let v = parse_python_literal(src).unwrap();
    assert_eq!(v["title"], "Ethiopia (washed) 'Yirg'");
    assert_eq!(v["timex"][2], 3);
    assert_eq!(v["timeindex"][1], 10);
    assert_eq!(v["flag"], true);
    assert!(v["none"].is_null());
    assert_eq!(v["weight"][2], "g");
    assert_eq!(v["computed"]["FCs_time"], 480.5);
    assert_eq!(v["u"], "x");
    assert_eq!(v["neg"], -1);
    assert_eq!(v["exp"], 0.001);
    assert!(v["nan"].is_null());
}

#[test]
fn parses_escapes_and_rejects_garbage() {
    let v = parse_python_literal(r"{'s': 'a\nb\x41é\\'}").unwrap();
    assert_eq!(v["s"], "a\nbAé\\");
    assert!(parse_python_literal("{'a': }").is_err());
    assert!(parse_python_literal("{'a': 1} trailing").is_err());
}

#[test]
fn python_literal_round_trips() {
    let v = json!({"a": [1, 2.5, "it's", null, true], "b": {"c": -3.0}});
    let s = to_python_literal(&v);
    assert_eq!(s, r"{'a': [1, 2.5, 'it\'s', None, True], 'b': {'c': -3.0}}");
    let back = parse_python_literal(&s).unwrap();
    assert_eq!(back, v);
}

#[test]
fn parse_profile_rebases_on_charge_and_reads_timeindex() {
    // Samples every second; CHARGE at index 2 (t=2.0), DRY at 12, DROP at 22.
    let timex: Vec<f64> = (0..30).map(|i| i as f64).collect();
    let temp2: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 5.0).collect();
    let temp1: Vec<f64> = (0..30).map(|i| 150.0 + i as f64 * 2.0).collect();
    let alog = json!({
        "title": "T", "mode": "C", "timex": timex, "temp1": temp1, "temp2": temp2,
        "timeindex": [2, 12, -1, -1, -1, -1, 22, -1],
        "computed": {"TP_time": 3.0, "TP_BT": 105.0, "TP_ET": 151.0}
    });
    let p = parse_profile(&to_python_literal(&alog)).unwrap();
    assert_eq!(p.events[0].time, 0.0);
    assert_eq!(p.events[0].bean_temp, 110.0);
    let codes: Vec<&str> = p.events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(codes, vec!["CHARGE", "TP", "DRY", "DROP"]);
    let dry = p.events.iter().find(|e| e.event_type == "DRY").unwrap();
    assert_eq!(dry.time, 10.0);
    assert_eq!(dry.bean_temp, 160.0);
}

#[test]
fn parse_profile_converts_fahrenheit() {
    let alog = json!({
        "mode": "F", "timex": [0.0, 10.0], "temp1": [392.0, 392.0], "temp2": [212.0, 212.0],
        "timeindex": [0, -1, -1, -1, -1, -1, -1, -1]
    });
    let p = parse_profile(&alog.to_string()).unwrap();
    assert!((p.events[0].bean_temp - 100.0).abs() < 0.01);
    assert!((p.events[0].env_temp - 200.0).abs() < 0.01);
}

#[test]
fn parse_session_rebases_converts_and_reads_control_events() {
    let timex: Vec<f64> = (0..40).map(|i| i as f64 * 2.0).collect(); // 2 s sampling
    let temp2: Vec<f64> = (0..40)
        .map(|i| 400.0 - (i as f64 - 10.0).abs() * 2.0)
        .collect();
    let temp1: Vec<f64> = (0..40).map(|_| 450.0).collect();
    let alog = json!({
        "title": "Import me", "mode": "F", "timex": timex, "temp1": temp1, "temp2": temp2,
        "timeindex": [5, 15, 25, 0, 0, 0, 35, 0],
        "etypes": ["Air", "Drum", "Damper", "Burner"],
        "specialevents": [5, 20], "specialeventstype": [0, 3], "specialeventsvalue": [6.0, 11.0],
        "weight": [1.0, 0.85, "Kg"], "beans": "Brazil", "roastepoch": 1_759_600_000,
        "roastingnotes": "nice", "ambientTemp": 68.0, "ambient_humidity": 40.0
    });
    let p = parse_session(&to_python_literal(&alog)).unwrap();
    assert_eq!(p.title, "Import me");
    assert_eq!(p.samples.len(), 35); // indices 5..40
    assert_eq!(p.samples[0].elapsed, 0.0);
    assert!((p.samples[0].bean_temp.unwrap() - ((390.0 - 32.0) * 5.0 / 9.0) as f32).abs() < 0.01);
    assert_eq!(p.samples[0].fan, Some(50.0)); // Air 50 % at charge
    assert_eq!(p.samples[0].heater, None);
    assert_eq!(p.samples[15].heater, Some(100.0)); // Burner 100 % from index 20
    assert_eq!(p.green_weight_g, Some(1000.0));
    assert_eq!(p.roasted_weight_g, Some(850.0));
    assert_eq!(p.beans.as_deref(), Some("Brazil"));
    assert!((p.ambient_temp.unwrap() - 20.0).abs() < 0.01);
    assert_eq!(p.roast_start.unwrap().timestamp(), 1_759_600_000);
    let codes: Vec<&str> = p.events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(codes, vec!["CHARGE", "DRY", "FCs", "DROP"]);
    let drop = p.events.iter().find(|e| e.event_type == "DROP").unwrap();
    assert_eq!(drop.time, 60.0);
}

#[test]
fn event_value_encoding() {
    // Artisan stores 0 % as 0.0 and p % as p/10 + 1.
    assert_eq!(artisan_event_value(0.0), 0.0);
    assert_eq!(artisan_event_value(50.0), 6.0);
    assert_eq!(artisan_event_value(100.0), 11.0);
    for p in [0.0, 1.0, 35.0, 85.0, 100.0] {
        assert_eq!(artisan_event_percent(artisan_event_value(p)), Some(p));
    }
    // Older files (and early Roastwarden versions) wrote 0 % as 1.0.
    assert_eq!(artisan_event_percent(1.0), Some(0.0));
    assert_eq!(artisan_event_percent(-3.0), Some(0.0));
}

#[test]
fn burner_off_events_are_kept_on_import() {
    let src = "{'mode': 'C', 'timex': [0.0, 1.0, 2.0, 3.0], 'temp1': [200.0, 201.0, 202.0, 203.0],
            'temp2': [100.0, 101.0, 102.0, 103.0], 'timeindex': [0, 0, 0, 0, 0, 0, 3, 0],
            'specialevents': [0, 2], 'specialeventstype': [3, 3], 'specialeventsvalue': [9.0, 0.0],
            'etypes': ['Air', 'Drum', 'Damper', 'Burner', '--']}";
    let s = parse_session(src).unwrap();
    let heater: Vec<Option<f64>> = s.samples.iter().map(|x| x.heater).collect();
    assert_eq!(heater, vec![Some(80.0), Some(80.0), Some(0.0), Some(0.0)]);
}

fn profile_fixture() -> ProfileWithPoints {
    use crate::profiles::types::{ProfileEvent, ProfilePoint, RoastProfile};
    let now = chrono::Utc::now();
    let pt = |t: i32, bt: f32, et: f32, fan: i32, heater: Option<i32>| ProfilePoint {
        id: format!("p{t}"),
        profile_id: "x".into(),
        time_seconds: t,
        target_temp: bt,
        fan_speed: Some(fan),
        notes: None,
        created_at: now,
        target_env_temp: Some(et),
        heater_pct: heater,
    };
    let ev = |kind: TriggerKind,
              at_s: Option<f64>,
              at_bt: Option<f64>,
              action: ProfileAction,
              value: Option<f64>,
              label: &str| ProfileEvent {
        id: label.into(),
        profile_id: "x".into(),
        trigger_kind: kind,
        at_seconds: at_s,
        at_bt,
        action,
        value,
        label: Some(label.into()),
        created_at: now,
    };
    ProfileWithPoints {
        profile: RoastProfile {
            id: "0b9d6f1e-4c55-4c2a-9f43-5d7b8e2a1c33".into(),
            name: "Light espresso".into(),
            description: Some("for the fluid bed".into()),
            created_by: None,
            created_at: now,
            updated_at: now,
            is_public: false,
            target_total_time: Some(600),
            target_first_crack: Some(480),
            target_end_temp: Some(212.0),
            preheat_temp: None,
            charge_temp: Some(30.0),
            beans: Some("Ethiopia Guji".into()),
            batch_size_g: Some(120.0),
            roaster: Some("Fluid bed".into()),
            roast_level: None,
            roast_purpose: None,
            expectations: None,
            auto_drop: None,
            roaster_kind: None,
        },
        points: vec![
            pt(0, 30.0, 240.0, 100, Some(100)),
            pt(120, 120.0, 245.0, 100, Some(100)),
            pt(240, 160.0, 250.0, 100, Some(90)),
            pt(480, 200.0, 255.0, 85, Some(80)),
            pt(600, 212.0, 255.0, 80, Some(70)),
        ],
        events: vec![
            ev(
                TriggerKind::Bt,
                None,
                Some(160.0),
                ProfileAction::Landmark,
                None,
                "drying_end",
            ),
            ev(
                TriggerKind::Time,
                Some(300.0),
                None,
                ProfileAction::Setpoint,
                Some(185.0),
                "sp",
            ),
            ev(
                TriggerKind::Time,
                Some(540.0),
                None,
                ProfileAction::Heater,
                Some(0.0),
                "cut",
            ),
        ],
        curves: None,
    }
}

#[test]
fn profile_export_is_an_artisan_profile() {
    let v = build_profile_alog(&profile_fixture()).unwrap();
    // Unsigned files must claim a version below 4.2.0, and have no extra devices.
    assert_eq!(v["version"], "2.10.0");
    assert!(v.get("extratimex").is_none() && v.get("extradevices").is_none());
    assert_eq!(v["roastUUID"], "0b9d6f1e4c554c2a9f435d7b8e2a1c33");
    assert_eq!(v["beans"], "Ethiopia Guji");
    assert_eq!(v["weight"], json!([120.0, 0.0, "g"]));
    assert_eq!(v["roastertype"], "Fluid bed");

    let timex = v["timex"].as_array().unwrap();
    let temp2 = v["temp2"].as_array().unwrap();
    assert_eq!(timex.len(), 601);
    assert_eq!(v["temp1"].as_array().unwrap().len(), 601);
    // A value everywhere (no None: Artisan types these as list[float]),
    // through the points, and rising (monotone spline, no overshoot).
    let bt: Vec<f64> = temp2.iter().map(|x| x.as_f64().unwrap()).collect();
    assert_eq!(bt[0], 30.0);
    assert_eq!(bt[480], 200.0);
    assert!(bt.windows(2).all(|w| w[1] >= w[0]));

    // CHARGE at 0; DRY where BT reaches 160; FCs and DROP from the targets.
    let ti: Vec<i64> = v["timeindex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect();
    assert_eq!(ti, vec![0, 240, 480, 0, 0, 0, 600, 0]);
    assert_eq!(v["computed"]["FCs_time"], 480.0);
    assert_eq!(v["computed"]["DROP_BT"], 212.0);

    // Fan and heater become Air (0) and Burner (3) events; setpoint a labelled one.
    let types: Vec<i64> = v["specialeventstype"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect();
    let strings: Vec<&str> = v["specialeventsStrings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(strings.contains(&"Air 85%") && strings.contains(&"Burner 90%"));
    assert!(strings.contains(&"Setpoint 185C"));
    assert!(strings.contains(&"Burner 0%"));
    assert_eq!(types.iter().filter(|t| **t == 0).count(), 3); // 100, 85, 80
    let idx: Vec<i64> = v["specialevents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap())
        .collect();
    assert!(idx.windows(2).all(|w| w[1] >= w[0]), "events in time order");
}

#[test]
fn exported_profile_reads_back() {
    let body = render_alog(&build_profile_alog(&profile_fixture()).unwrap());
    // Valid Python literal (what Artisan's loader needs) with no None values.
    assert!(body.starts_with('{') && !body.contains("None"));
    let s = parse_session(&body).unwrap();
    assert_eq!(s.title, "Light espresso");
    assert_eq!(s.beans.as_deref(), Some("Ethiopia Guji"));
    assert_eq!(s.green_weight_g, Some(120.0));
    assert_eq!(s.roaster_type.as_deref(), Some("Fluid bed"));
    assert_eq!(s.samples.len(), 601);
    let at = |t: f64| {
        s.samples
            .iter()
            .find(|x| (x.elapsed - t).abs() < 0.01)
            .unwrap()
    };
    assert_eq!(at(10.0).fan, Some(100.0));
    assert_eq!(at(500.0).fan, Some(85.0));
    assert_eq!(at(550.0).heater, Some(0.0));
    let fc = s.events.iter().find(|e| e.event_type == "FCs").unwrap();
    assert_eq!(fc.time, 480.0);
    assert_eq!(fc.bean_temp, 200.0);
}

#[test]
fn session_export_has_no_extra_devices_and_a_uuid() {
    let session: RoastSession = serde_json::from_value(json!({
            "id": "aba92bf3-811f-4437-b1eb-d4942df4235d", "name": "r", "device_id": "d",
            "status": "completed", "created_at": "2026-10-22T10:00:00Z", "updated_at": "2026-10-22T10:00:00Z",
            "batch_number": 12
        }))
        .unwrap();
    let v = build_alog(&session, &[], &[], &[]);
    assert!(v.get("extradevices").is_none());
    assert!(v.get("extratimex").is_none());
    assert!(v
        .as_object()
        .unwrap()
        .keys()
        .all(|k| !k.starts_with("extra")));
    assert_eq!(v["roastUUID"], "aba92bf3811f4437b1ebd4942df4235d");
    assert_eq!(v["roastbatchnr"], 12);
    assert_eq!(v["timeindex"], json!([-1, 0, 0, 0, 0, 0, 0, 0]));
}

/// Declared channels become Artisan extra devices: two per `+Virtual` device, curves that
/// match the samples (-1 where one has none), an odd channel's partner hidden; the file
/// still reads back as a Python literal.
#[test]
fn declared_channels_export_as_extra_devices() {
    let session: RoastSession = serde_json::from_value(json!({
        "id": "s", "name": "r", "device_id": "d", "status": "completed",
        "created_at": "2026-10-31T10:00:00Z", "updated_at": "2026-10-31T10:00:00Z"
    }))
    .unwrap();
    let sample = |t: f32, x: Value| -> SessionTelemetry {
        serde_json::from_value(json!({
            "id": format!("p{t}"), "session_id": "s", "timestamp": "2026-10-31T10:00:00Z",
            "elapsed_seconds": t, "bean_temp": 150.0, "env_temp": 200.0, "extras": x
        }))
        .unwrap()
    };
    let tel = vec![
        sample(0.0, json!({ "t3": 120.04, "burner_kpa": 2.5, "rpm": 60 })),
        sample(1.0, json!({ "t3": 121.0 })),
    ];
    let ch = |key: &str, label: &str, unit: &str| Channel {
        key: key.into(),
        label: label.into(),
        unit: unit.into(),
        max: None,
    };
    let two = [ch("t3", "Exhaust", "C"), ch("burner_kpa", "Gas", "kPa")];
    let v = build_alog(&session, &tel, &[], &two);
    assert_eq!(v["extradevices"], json!([25]));
    assert_eq!(v["extraname1"], json!(["Exhaust"]));
    assert_eq!(v["extraname2"], json!(["Gas"]));
    assert_eq!(v["extratimex"], json!([[0.0, 1.0]]));
    assert_eq!(v["extratemp1"], json!([[120.0, 121.0]]));
    assert_eq!(v["extratemp2"], json!([[2.5, -1.0]]));
    assert_eq!(v["extraNoneTempHint1"], json!([false]));
    assert_eq!(v["extraNoneTempHint2"], json!([true]));
    assert_eq!(v["extraCurveVisibility2"], json!([true]));
    for k in [
        "extramathexpression1",
        "extraLCDvisibility1",
        "extraDelta2",
        "extraFill1",
    ] {
        assert_eq!(v[k].as_array().unwrap().len(), 1, "{k}");
    }
    // Three: a second device whose second curve is an empty, hidden placeholder.
    let three = [two[0].clone(), two[1].clone(), ch("rpm", "Drum", "rpm")];
    let v = build_alog(&session, &tel, &[], &three);
    assert_eq!(v["extradevices"], json!([25, 25]));
    assert_eq!(v["extraname1"], json!(["Exhaust", "Drum"]));
    assert_eq!(v["extratemp1"][1], json!([60.0, -1.0]));
    assert_eq!(v["extratemp2"][1], json!([-1.0, -1.0]));
    assert_eq!(v["extraCurveVisibility2"], json!([true, false]));
    let back = parse_python_literal(&render_alog(&v)).unwrap();
    assert_eq!(back["extratemp1"], v["extratemp1"]);
    for k in ["extratemp1", "extratemp2", "extratimex"] {
        assert!(
            v[k].as_array()
                .unwrap()
                .iter()
                .flat_map(|c| c.as_array().unwrap())
                .all(Value::is_number),
            "no None in {k}"
        );
    }
}
