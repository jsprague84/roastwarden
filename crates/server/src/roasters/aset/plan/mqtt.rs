//! A machine on MQTT (Artisan device 201): an ingest mapping from the channels' paths, read
//! from the server's own broker.

use serde_json::json;

use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::Protocol;

use super::inputs::{Etypes, EXTRAS_MQTT};
use super::{channel_fields, finish, Plan};
use crate::roasters::aset::ini::Ini;

/// A machine on MQTT (Artisan device 201): an ingest mapping from the channels' paths, read from
/// the server's own broker.
pub(super) fn plan_mqtt(ini: &Ini, plan: &mut Plan) {
    let et = Etypes::read(ini);
    let mut notes = Vec::new();
    let s = "MQTT";
    let nodes = ini.list(s, "channel_nodes");
    let modes = ini.list(s, "channel_modes");
    let topics = ini.list(s, "channel_topics");
    let at = |l: &Vec<String>, n: usize| {
        l.get(n - 1)
            .map(|x| x.trim().to_string())
            .unwrap_or_default()
    };
    let units = if at(&modes, 1) == "F" || (at(&nodes, 1).is_empty() && at(&modes, 2) == "F") {
        "F"
    } else {
        "C"
    };
    let (mut fields, channels) = channel_fields(
        ini,
        &et,
        &EXTRAS_MQTT,
        &|n| Some(at(&nodes, n)).filter(|p| !p.is_empty()),
        &|n| match at(&modes, n).as_str() {
            "C" => Some("C"),
            "F" => Some("F"),
            _ => None,
        },
        units,
        &mut notes,
    );
    // A channel with a topic of its own is read from it.
    for (key, f) in &mut fields {
        let n = f["channel"].as_u64().unwrap_or(0) as usize;
        if let Some(t) = Some(at(&topics, n)).filter(|t| !t.is_empty() && !t.contains(['+', '#'])) {
            f["topic"] = json!(t);
        } else if n > 0 && !at(&topics, n).is_empty() {
            notes.push(format!(
                "{key}: its topic `{}` has a wildcard; read from the main topic",
                at(&topics, n)
            ));
        }
    }
    for f in fields.values_mut() {
        if let Some(o) = f.as_object_mut() {
            o.remove("channel");
        }
    }
    let bt_topic = fields
        .get("bt")
        .or_else(|| fields.get("et"))
        .is_some_and(|f| f.get("topic").is_some());
    if !bt_topic {
        plan.needs.push("telemetry_topic".into());
    }
    let host = ini.text(s, "host").unwrap_or_default();
    let port = ini.int(s, "port").unwrap_or(1883);
    notes.insert(
        0,
        format!(
            "Artisan reads it from the MQTT broker {host}:{port}{} with your login there; \
             Roastwarden reads the server's own broker: bridge the roaster's topic into it (a \
             Mosquitto bridge), then give that topic{}",
            if ini.flag(s, "tls") { " over TLS" } else { "" },
            if bt_topic {
                ""
            } else {
                " (`--telemetry-topic`)"
            }
        ),
    );
    plan.protocol = Some(Protocol::Mqtt);
    plan.config = Some(json!({ "qos": 0, "ingest": { "units": units, "fields": fields } }));
    plan.notes.extend(notes);
    finish(ini, plan, Outputs::default(), channels, Kind::DrumGas);
}
