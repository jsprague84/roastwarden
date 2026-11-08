//! A recorded roast as an Artisan dict: its samples, landmarks and events, the control
//! curves as Artisan's events, the roaster's declared channels as extra devices.

use serde_json::{json, Map, Value};

use crate::roasters::channels::{self, Channel};
use crate::roasts::types::{RoastEvent, RoastEventType, RoastSession, SessionTelemetry};

use super::round1;

/// Artisan event types we emit for control curves.
pub(super) const ETYPE_AIR: i64 = 0;
pub(super) const ETYPE_BURNER: i64 = 3;

/// Encode a percentage the way Artisan stores event values
/// (`eventsExternal2InternalValue`): 0 is `0.0`, else `p / 10 + 1`.
pub(super) fn artisan_event_value(percent: f64) -> f64 {
    if percent.round() == 0.0 {
        return 0.0;
    }
    ((percent / 10.0 + 1.0) * 10.0).round() / 10.0
}

/// Artisan's `timeindex` from landmark indices: CHARGE is −1 when unset,
/// the other slots (DRY, FCs, FCe, SCs, SCe, DROP, COOL) use 0 for unset.
pub(super) fn timeindex_slots(charge: Option<i64>, rest: [Option<i64>; 7]) -> Vec<i64> {
    let mut v = vec![charge.unwrap_or(-1)];
    v.extend(rest.iter().map(|i| i.filter(|x| *x > 0).unwrap_or(0)));
    v
}

/// A 32-digit hex id for `roastUUID`, from a UUID string when there is one.
pub(super) fn roast_uuid(id: &str) -> String {
    let hex: String = id.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() == 32 {
        hex.to_ascii_lowercase()
    } else {
        uuid::Uuid::new_v4().simple().to_string()
    }
}

/// `computed` entries for landmarks: `<CODE>_time` (seconds from CHARGE)
/// and `<CODE>_BT`. Artisan recomputes these on load except `TP_time`,
/// which a background profile reads.
pub(super) fn computed_entries(out: &mut Map<String, Value>, code: &str, t: f64, bt: f64) {
    out.insert(format!("{code}_time"), json!((t * 10.0).round() / 10.0));
    out.insert(format!("{code}_BT"), json!((bt * 10.0).round() / 10.0));
}

pub(super) fn nearest_index(timex: &[f64], t: f64) -> Option<i64> {
    if timex.is_empty() {
        return None;
    }
    let mut best = 0usize;
    let mut best_d = f64::MAX;
    for (i, &x) in timex.iter().enumerate() {
        let d = (x - t).abs();
        if d < best_d {
            best = i;
            best_d = d;
        }
    }
    Some(best as i64)
}

/// Artisan's `+Virtual` device (`device_registry.py`, id 25): what Artisan's own importers
/// (Cropster, …) give curves that came from a file rather than a device.
const ARTISAN_VIRTUAL_DEVICE: i64 = 25;

/// Line colours for the extra curves, distinct from Artisan's BT and ET defaults.
const EXTRA_COLOURS: [&str; 8] = [
    "#e69f00", "#882255", "#44aa99", "#999933", "#aa4499", "#332288", "#117733", "#661100",
];

/// The declared channels as Artisan extra devices (`docs/channels.md`): one `+Virtual` device
/// per pair, curve 1 and 2, the keys Artisan writes with them (its `getProfile`, read back by
/// `setProfile`), -1 where a sample has no value. An odd channel's pair has a hidden empty
/// second curve, as Artisan's Cropster import makes. With no channel, nothing: an empty list
/// would replace the user's own extra-device setup when the file is loaded.
fn extra_devices(
    channels: &[Channel],
    telemetry: &[SessionTelemetry],
    timex: &[f64],
) -> Map<String, Value> {
    let mut m = Map::new();
    if channels.is_empty() {
        return m;
    }
    let curve = |c: &Channel| -> Vec<f64> {
        telemetry
            .iter()
            .map(|t| channels::stored(t.extras.as_ref(), &c.key).map_or(-1.0, round1))
            .collect()
    };
    let (mut devices, mut timexs) = (Vec::new(), Vec::new());
    let (mut name1, mut name2, mut temp1, mut temp2) = (vec![], vec![], vec![], vec![]);
    let (mut vis1, mut vis2, mut hint1, mut hint2) = (vec![], vec![], vec![], vec![]);
    let (mut colour1, mut colour2) = (vec![], vec![]);
    for (pair, two) in channels.chunks(2).enumerate() {
        devices.push(json!(ARTISAN_VIRTUAL_DEVICE));
        timexs.push(json!(timex));
        let a = &two[0];
        name1.push(json!(a.label));
        temp1.push(json!(curve(a)));
        vis1.push(json!(true));
        hint1.push(json!(!a.is_temp()));
        colour1.push(json!(EXTRA_COLOURS[(pair * 2) % EXTRA_COLOURS.len()]));
        match two.get(1) {
            Some(b) => {
                name2.push(json!(b.label));
                temp2.push(json!(curve(b)));
                vis2.push(json!(true));
                hint2.push(json!(!b.is_temp()));
            }
            None => {
                name2.push(json!("Extra 2"));
                temp2.push(json!(vec![-1.0; timex.len()]));
                vis2.push(json!(false));
                hint2.push(json!(true));
            }
        }
        colour2.push(json!(EXTRA_COLOURS[(pair * 2 + 1) % EXTRA_COLOURS.len()]));
    }
    let n = devices.len();
    let each = |v: Value| json!(vec![v; n]);
    // Artisan pads its per-LCD lists to its LCD count itself.
    for (k, v) in [
        ("extradevices", json!(devices)),
        ("extraname1", json!(name1)),
        ("extraname2", json!(name2)),
        ("extratimex", json!(timexs)),
        ("extratemp1", json!(temp1)),
        ("extratemp2", json!(temp2)),
        ("extramathexpression1", each(json!(""))),
        ("extramathexpression2", each(json!(""))),
        ("extradevicecolor1", json!(colour1)),
        ("extradevicecolor2", json!(colour2)),
        ("extraLCDvisibility1", json!(vis1)),
        ("extraLCDvisibility2", json!(vis2)),
        ("extraCurveVisibility1", json!(vis1)),
        ("extraCurveVisibility2", json!(vis2)),
        ("extraDelta1", each(json!(false))),
        ("extraDelta2", each(json!(false))),
        ("extraFill1", each(json!(0))),
        ("extraFill2", each(json!(0))),
        ("extraNoneTempHint1", json!(hint1)),
        ("extraNoneTempHint2", json!(hint2)),
    ] {
        m.insert(k.into(), v);
    }
    m
}

/// Build the Artisan profile dict for a recorded session; `channels` are the roaster's
/// declared channels (docs/channels.md), exported as extra devices.
pub fn build_alog(
    session: &RoastSession,
    telemetry: &[SessionTelemetry],
    events: &[RoastEvent],
    channels: &[Channel],
) -> Value {
    let timex: Vec<f64> = telemetry.iter().map(|t| t.elapsed_seconds as f64).collect();
    let temp1: Vec<f64> = telemetry
        .iter()
        .map(|t| t.env_temp.map_or(-1.0, f64::from))
        .collect();
    let temp2: Vec<f64> = telemetry
        .iter()
        .map(|t| t.bean_temp.map_or(-1.0, f64::from))
        .collect();

    let event_index = |kind: RoastEventType| -> Option<i64> {
        events
            .iter()
            .find(|e| e.event_type == kind)
            .and_then(|e| nearest_index(&timex, e.elapsed_seconds as f64))
    };

    // CHARGE: explicit event if marked, else the first sample (session start).
    let charge_idx =
        event_index(RoastEventType::Charge).or(if timex.is_empty() { None } else { Some(0) });
    let timeindex = timeindex_slots(
        charge_idx,
        [
            event_index(RoastEventType::DryingEnd),
            event_index(RoastEventType::FirstCrackStart),
            event_index(RoastEventType::FirstCrackEnd),
            event_index(RoastEventType::SecondCrackStart),
            event_index(RoastEventType::SecondCrackEnd),
            event_index(RoastEventType::Drop),
            event_index(RoastEventType::CoolEnd),
        ],
    );

    // Landmark times and BT for `computed` (TP is read by Artisan backgrounds).
    let t_charge = charge_idx
        .and_then(|i| timex.get(i as usize).copied())
        .unwrap_or(0.0);
    let mut computed = Map::new();
    for (kind, code) in [
        (RoastEventType::Charge, "CHARGE"),
        (RoastEventType::TurningPoint, "TP"),
        (RoastEventType::DryingEnd, "DRY"),
        (RoastEventType::FirstCrackStart, "FCs"),
        (RoastEventType::FirstCrackEnd, "FCe"),
        (RoastEventType::SecondCrackStart, "SCs"),
        (RoastEventType::SecondCrackEnd, "SCe"),
        (RoastEventType::Drop, "DROP"),
    ] {
        if let Some(i) = event_index(kind) {
            let i = i as usize;
            if let (Some(t), Some(bt)) = (timex.get(i), temp2.get(i).filter(|v| **v > -1.0)) {
                computed_entries(&mut computed, code, t - t_charge, *bt);
            }
        }
    }

    // Control curves become Air (fan) and Burner (heater) events on change.
    let mut sp_idx: Vec<i64> = Vec::new();
    let mut sp_type: Vec<i64> = Vec::new();
    let mut sp_value: Vec<f64> = Vec::new();
    let mut sp_str: Vec<String> = Vec::new();
    let mut last_fan: Option<i64> = None;
    let mut last_heater: Option<i64> = None;
    for (i, t) in telemetry.iter().enumerate() {
        if let Some(fan) = t.fan {
            let pct = fan.round().clamp(0.0, 100.0) as i64;
            if last_fan != Some(pct) {
                last_fan = Some(pct);
                sp_idx.push(i as i64);
                sp_type.push(ETYPE_AIR);
                sp_value.push(artisan_event_value(pct as f64));
                sp_str.push(format!("Air {pct}%"));
            }
        }
        if let Some(heater) = t.heater {
            let pct = heater.round().clamp(0.0, 100.0) as i64;
            if last_heater != Some(pct) {
                last_heater = Some(pct);
                sp_idx.push(i as i64);
                sp_type.push(ETYPE_BURNER);
                sp_value.push(artisan_event_value(pct as f64));
                sp_str.push(format!("Burner {pct}%"));
            }
        }
    }
    // Custom notes become "--" (type 4) events with no value.
    for e in events
        .iter()
        .filter(|e| e.event_type == RoastEventType::Custom)
    {
        if let Some(idx) = nearest_index(&timex, e.elapsed_seconds as f64) {
            sp_idx.push(idx);
            sp_type.push(4);
            sp_value.push(0.0);
            sp_str.push(e.notes.clone().unwrap_or_else(|| "note".into()));
        }
    }

    let start = session.start_time.unwrap_or(session.created_at);
    let sampling_interval = if timex.len() > 1 {
        let span = timex[timex.len() - 1] - timex[0];
        (span / (timex.len() - 1) as f64 * 10.0).round() / 10.0
    } else {
        1.0
    };

    // `version` stays below 4.2.0: official Artisan 4.2+ builds only check a
    // signature for files claiming 4.2 or later, and refuse unsigned ones as
    // backgrounds. No `extra*` keys unless the roaster declares channels: an empty
    // extra-device list would replace the user's extra-device setup in Artisan.
    let extras = extra_devices(channels, telemetry, &timex);
    let mut alog = json!({
        "version": "2.10.0",
        "revision": "",
        "build": "",
        "mode": "C",
        "roastUUID": roast_uuid(&session.id),
        "title": session.name,
        // Batch number per roaster (docs/batches.md); Artisan's own counter fields.
        "roastbatchnr": session.batch_number.unwrap_or(0),
        "roastbatchprefix": "",
        "roastbatchpos": 1,
        "beans": session.bean_origin.clone().unwrap_or_default(),
        "weight": [
            session.green_weight.unwrap_or(0.0),
            session.roasted_weight.unwrap_or(0.0),
            "g"
        ],
        "roastisodate": start.format("%Y-%m-%d").to_string(),
        "roasttime": start.format("%H:%M:%S").to_string(),
        "roastepoch": start.timestamp(),
        "roasttzoffset": 0,
        "roastingnotes": session.notes.clone().unwrap_or_default(),
        "ambientTemp": session.ambient_temp.unwrap_or(0.0),
        "ambient_humidity": session.humidity.unwrap_or(0.0),
        "samplinginterval": sampling_interval,
        "timex": timex,
        "temp1": temp1,
        "temp2": temp2,
        "timeindex": timeindex,
        "etypes": ["Air", "Drum", "Damper", "Burner", "--"],
        "specialevents": sp_idx,
        "specialeventstype": sp_type,
        "specialeventsvalue": sp_value,
        "specialeventsStrings": sp_str,
        "computed": computed,
    });
    if let Some(o) = alog.as_object_mut() {
        o.extend(extras);
    }
    alog
}
