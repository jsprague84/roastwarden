//! A machine on Artisan's WebSocket (device 141): the frames it sends as a poll message and
//! command templates, its channels as an ingest mapping.

use serde_json::{Map, Value, json};

use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::Protocol;

use super::inputs::{EXTRAS_WEBSOCKET, Etypes};
use super::sliders::{py_format, sliders, slot};
use super::{Plan, channel_fields, finish, poll_ms};
use crate::roasters::aset::ini::Ini;

/// Placeholders in a JSON frame, swapped for `{value}` and `{id}` once it is text.
const VALUE_MARK: &str = "1234567.25";
const ID_MARK: &str = "7654321987";

/// A frame as Artisan's WebSocket sends it: the request, its id node (`{id}`, the client's
/// counter) and the machine node. `None` when the request is not a JSON object.
fn ws_frame(request: &str, id_node: &str, machine_node: &str, machine_id: i64) -> Option<String> {
    let mut v: Value = serde_json::from_str(request).ok()?;
    let obj = v.as_object_mut()?;
    if !id_node.is_empty() {
        obj.insert(id_node.into(), serde_json::from_str(ID_MARK).ok()?);
    }
    if !machine_node.is_empty() {
        obj.insert(machine_node.into(), json!(machine_id));
    }
    Some(
        serde_json::to_string(&v)
            .ok()?
            .replace(ID_MARK, "{id}")
            .replace(VALUE_MARK, "{value}"),
    )
}

/// A machine on Artisan's WebSocket protocol (device 111): the client's URL, its `getData` poll,
/// an ingest mapping from the channels' nodes, the sliders' `send(…)` as templates.
pub(super) fn plan_websocket(ini: &Ini, plan: &mut Plan) {
    let et = Etypes::read(ini);
    let mut notes = Vec::new();
    let s = "WebSocket";
    let text = |k: &str, d: &str| {
        ini.text(s, k)
            .map_or_else(|| d.to_string(), |v| v.trim().to_string())
    };
    let host = text("host", "");
    let port = ini
        .int(s, "port")
        .filter(|p| (1..=65535).contains(p))
        .unwrap_or(80);
    let path = text("path", "WebSocket");
    let local = host.is_empty() || host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost");
    let url = format!(
        "ws://{}:{port}/{}",
        if local { "127.0.0.1" } else { &host },
        path.trim_start_matches('/')
    );
    if local {
        plan.needs.push("host".into());
        notes.push(format!(
            "WebSocket {url}: give the roaster's address (the file has none of its own)"
        ));
    } else {
        notes.push(format!(
            "WebSocket to {url}, the maker's example address: check it is your roaster's"
        ));
    }
    let nodes = ini.list(s, "channel_nodes");
    let modes: Vec<i64> = ini
        .list(s, "channel_modes")
        .iter()
        .map(|m| m.trim().parse().unwrap_or(0))
        .collect();
    let mode = |n: usize| modes.get(n - 1).copied().unwrap_or(0);
    let data_node = text("data_node", "data");
    let path_of = |node: &str| {
        if data_node.is_empty() {
            node.to_string()
        } else {
            format!("{data_node}.{node}")
        }
    };
    let node = |n: usize| {
        nodes
            .get(n - 1)
            .map(|x| x.trim().to_string())
            .unwrap_or_default()
    };
    let units = if mode(1) == 2 || (node(1).is_empty() && mode(2) == 2) {
        "F"
    } else {
        "C"
    };
    let fields = channel_fields(
        ini,
        &et,
        &EXTRAS_WEBSOCKET,
        &|n| (!node(n).is_empty()).then(|| path_of(&node(n))),
        &|n| match mode(n) {
            1 => Some("C"),
            2 => Some("F"),
            _ => None,
        },
        units,
        &mut notes,
    );
    let (mut fields, channels) = fields;
    for f in fields.values_mut() {
        if let Some(o) = f.as_object_mut() {
            o.remove("channel");
        }
    }
    let mut c = json!({
        "url": url,
        "reconnect_interval_ms": ini
            .num(s, "reconnect_interval")
            .map_or(5000, |v| (v * 1000.0).round() as i64)
            .clamp(500, 60_000),
        "ingest": { "units": units, "fields": fields },
    });
    let (id_node, machine_node) = (text("id_node", "id"), text("machine_node", "roasterID"));
    let machine_id = ini.int(s, "machineID").unwrap_or(0);
    let request = text("request_data_command", "");
    if request.is_empty() {
        notes.push("Artisan waits for the roaster to push its data; so does Roastwarden".into());
    } else if let Some(message) = ws_frame(
        &Value::Object(Map::from_iter([(
            text("command_node", "command"),
            Value::String(request),
        )]))
        .to_string(),
        &id_node,
        &machine_node,
        machine_id,
    ) {
        c["poll"] = json!({ "message": message, "interval_ms": poll_ms(ini, 200) });
    }
    if ini
        .list(s, "channel_requests")
        .iter()
        .any(|r| !r.trim().is_empty())
    {
        notes
            .push("Artisan's requests per channel are not imported: Roastwarden polls once".into());
    }
    let mut outputs = Outputs::default();
    let mut commands = Map::new();
    for sl in sliders(ini, &et, "18", "WebSocket command", &mut notes) {
        let label = &sl.label;
        let frame = sl
            .first
            .strip_prefix("send(")
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|r| py_format(r, VALUE_MARK))
            .and_then(|r| ws_frame(&r, &id_node, &machine_node, machine_id));
        let Some(frame) = frame else {
            notes.push(format!(
                "slider {label}: `{}` is not a send(…) of JSON Roastwarden can fill; left out",
                sl.cmd
            ));
            continue;
        };
        if sl.o != 0.0 {
            notes.push(format!(
                "slider {label}: its offset {} cannot go into a command; left out",
                sl.o
            ));
            continue;
        }
        let out = slot(&mut outputs, sl.role);
        if out.is_some() {
            notes.push(format!(
                "slider {label}: a second {} slider; left out",
                sl.role
            ));
            continue;
        }
        *out = Some(sl.output.clone());
        let mut t = json!({ "payload": frame });
        if sl.k != 1.0 {
            t["scale"] = json!(sl.k);
        }
        commands.insert(sl.role.to_string(), t);
        if !sl.rest.is_empty() {
            notes.push(format!(
                "slider {label} also sends `{}`; Roastwarden sends the first frame only",
                sl.rest.join("; ")
            ));
        }
    }
    if !commands.is_empty() {
        c["commands"] = Value::Object(commands);
    }
    let (charge, drop) = (text("charge_message", ""), text("drop_message", ""));
    if !charge.is_empty() || !drop.is_empty() {
        notes.push(format!(
            "Artisan sends {} to the roaster; Roastwarden does not: start and stop the roast on \
             the machine",
            [("CHARGE", &charge), ("DROP", &drop)]
                .iter()
                .filter(|(_, m)| !m.is_empty())
                .map(|(w, m)| format!("`{m}` at {w}"))
                .collect::<Vec<_>>()
                .join(" and ")
        ));
    }
    if ["DRY_node", "FCs_node", "FCe_node", "SCs_node", "SCe_node"]
        .iter()
        .any(|k| !text(k, "").is_empty())
    {
        notes.push(
            "the roaster's own events (dry end, first crack, …) are not read: mark them in \
             Roastwarden"
                .into(),
        );
    }
    plan.protocol = Some(Protocol::WebSocket);
    plan.config = Some(c);
    plan.notes.extend(notes);
    finish(ini, plan, outputs, channels, Kind::DrumGas);
}
