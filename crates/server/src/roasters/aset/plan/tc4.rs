//! A machine on an Arduino TC4 (Artisan device 19): the serial TC4 connection, its channels
//! 3 and 4 when named, the sliders' serial commands as templates.

use serde_json::{json, Map, Value};

use crate::roasters::channels::Channel;
use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::Protocol;

use super::inputs::{extra_curves, Etypes, EXTRAS_TC4};
use super::sliders::{py_format, sliders, slot};
use super::{finish, poll_ms, Plan};
use crate::roasters::aset::ini::Ini;

/// A machine on an Arduino TC4 (Artisan device 19): the serial TC4 connection as stock
/// aArtisan answers, its channels 3 and 4 when named, the sliders' serial commands as templates.
pub(super) fn plan_tc4(ini: &Ini, plan: &mut Plan) {
    let et = Etypes::read(ini);
    let mut notes = Vec::new();
    let chan = |key: &str, d: u8| -> u8 {
        match ini.text("Device", key) {
            None => d,
            Some(v) => v
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|c| (1..=4).contains(c))
                .unwrap_or(0),
        }
    };
    let (mut et_ch, mut bt_ch) = (chan("arduinoETChannel", 1), chan("arduinoBTChannel", 2));
    if (et_ch == 0 && bt_ch == 0) || et_ch == bt_ch {
        (et_ch, bt_ch) = (1, 2);
    }
    let baud = ini
        .int("SerialPort", "baudrate")
        .filter(|b| *b > 0)
        .unwrap_or(115_200);
    let mut c = json!({
        "baud": baud,
        "poll_interval_ms": poll_ms(ini, 100),
        "reply_format": "active",
        "et_channel": et_ch,
        "bt_channel": bt_ch,
    });
    let mut channels = Vec::new();
    for (n, (raw, _)) in extra_curves(ini, &et, &EXTRAS_TC4, &[32, 44], &mut notes) {
        let name = et.named(&raw);
        let default = name.is_empty()
            || name == "--"
            || name
                .strip_prefix("Extra ")
                .is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()));
        let input = n as u8;
        if default || [et_ch, bt_ch].contains(&input) {
            notes.push(format!(
                "TC4 channel {n} ({}): no probe named; left out",
                if name.is_empty() { "unnamed" } else { &name }
            ));
            continue;
        }
        let key = format!("t{n}");
        c[format!("{key}_channel")] = json!(input);
        channels.push(Channel {
            key,
            label: name.chars().take(24).collect(),
            unit: "C".into(),
            max: None,
        });
    }
    let mut outputs = Outputs::default();
    let mut commands = Map::new();
    for s in sliders(ini, &et, "1", "serial command", &mut notes) {
        let label = &s.label;
        let lines: Option<Vec<String>> = std::iter::once(&s.first)
            .chain(&s.rest)
            .map(|p| py_format(p, "{value}").or_else(|| (!p.contains('{')).then(|| p.clone())))
            .collect();
        let Some(lines) = lines.filter(|l| l.iter().any(|x| x.contains("{value}"))) else {
            notes.push(format!(
                "slider {label}: `{}` takes no value Roastwarden can fill; left out",
                s.cmd
            ));
            continue;
        };
        if s.o != 0.0 {
            notes.push(format!(
                "slider {label}: its offset {} cannot go into a command; left out",
                s.o
            ));
            continue;
        }
        let out = slot(&mut outputs, s.role);
        if out.is_some() {
            notes.push(format!(
                "slider {label}: a second {} slider; left out",
                s.role
            ));
            continue;
        }
        *out = Some(s.output.clone());
        if s.role == "fan" {
            let word = s
                .first
                .split([',', ';', ' '])
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            match word.as_str() {
                "IO3" => c["fan_output"] = json!("io3"),
                "OT2" => c["fan_output"] = json!("ot2"),
                "DCFAN" => c["fan_output"] = json!("dcfan"),
                _ => {}
            }
        }
        let mut t = json!({ "payload": lines.join("\n"), "decimals": 0 });
        if s.k != 1.0 {
            t["scale"] = json!(s.k);
        }
        commands.insert(s.role.to_string(), t);
    }
    if !commands.is_empty() {
        c["commands"] = Value::Object(commands);
    }
    notes.insert(
        0,
        format!(
            "Arduino TC4 (aArtisan) at {baud} baud: give the serial port on the server; ET on \
             its input {et_ch}, BT on {bt_ch}, the heater and fan read back in each reply"
        ),
    );
    plan.needs.push("port".into());
    plan.protocol = Some(Protocol::SerialTc4);
    plan.config = Some(c);
    plan.notes.extend(notes);
    finish(ini, plan, outputs, channels, Kind::DrumElectric);
}
