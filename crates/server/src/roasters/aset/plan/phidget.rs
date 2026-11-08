//! A machine on a Phidgets board (Artisan devices 35, 36, 59, 60 and their kin): the board,
//! its inputs and thermocouple types as a Phidget22 connection.

use serde_json::{json, Map, Value};

use crate::roasters::channels::Channel;
use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::Protocol;

use super::inputs::{extra_curves, Etypes, Reads};
use super::{finish, poll_ms, Plan};
use crate::roasters::aset::ini::Ini;

/// A board's extra inputs as Artisan's extra devices read them: the 1048's and TMP1101's
/// inputs 2 and 3 (`_34`), their own sensor (`_AT`, one curve: [`NO_INPUT`] for the second).
const EXTRAS_PHIDGET: [(i64, usize, usize); 4] =
    [(35, 2, 3), (59, 2, 3), (36, 4, NO_INPUT), (60, 4, NO_INPUT)];
const NO_INPUT: usize = 99;

/// Artisan's thermocouple codes (1 K, 2 J, 3 E, 4 T) as Phidgets name them.
pub(super) fn thermocouple_of(code: &str) -> &'static str {
    match code.trim() {
        "2" => "J",
        "3" => "E",
        "4" => "T",
        _ => "K",
    }
}

/// A Phidgets temperature board through the Phidget22 network server (docs/phidgets.md):
/// Artisan's 1048 and TMP1101 read BT on input 0 and ET on 1; a 1051 or TMP1100 has one
/// thermocouple (BT) and the board's own sensor, which Artisan draws as ET; a TMP1200 one RTD.
pub(super) fn plan_phidget(ini: &Ini, plan: &mut Plan) {
    let device = plan.machine.artisan_device;
    let et_names = Etypes::read(ini);
    let mut notes = Vec::new();
    let (board, four) = match device {
        34 => ("a 1048 (USB, four thermocouples)", true),
        58 => ("a TMP1101 (VINT, four thermocouples)", true),
        52 => ("a 1051 (USB, one thermocouple)", false),
        61 => ("a TMP1100 (VINT, one thermocouple)", false),
        _ => ("a TMP1200 (VINT, one RTD)", false),
    };
    let mut fields = Map::new();
    fields.insert("bt".into(), json!("ch0"));
    let mut reads = Reads {
        roles: vec!["bt"],
        used: vec!["bt".into()],
        ..Reads::default()
    };
    if four {
        fields.insert("et".into(), json!("ch1"));
        reads.roles.push("et");
        reads.used.push("et".into());
    } else if device != 68 {
        // Artisan's ET here is the board's own temperature (near the room's): a channel.
        reads.used.push("board".into());
        reads.channels.push(Channel {
            key: "board".into(),
            label: "Board".into(),
            unit: "C".into(),
            max: None,
        });
        fields.insert("board".into(), json!("ch1"));
        notes.push(
            "Artisan draws the board's own sensor (input 1) as ET; Roastwarden keeps it as the \
             channel Board and has no ET"
                .into(),
        );
    }
    if four {
        let shown = extra_curves(ini, &et_names, &EXTRAS_PHIDGET, &[], &mut notes);
        for (n, (raw, math)) in &shown {
            let name = et_names.named(raw);
            let unnamed = name.is_empty()
                || name == "--"
                || name
                    .strip_prefix("Extra ")
                    .is_some_and(|d| d.chars().all(|c| c.is_ascii_digit()));
            if *n == NO_INPUT || unnamed {
                continue;
            }
            let path = format!("ch{n}");
            let c = reads.curve(
                ini, &et_names, raw, math, true, &path, *n, "Input", &mut notes,
            );
            let mut f = json!({ "path": path });
            if let Some(k) = c.scale {
                f["scale"] = json!(k);
            }
            match (c.role, c.key) {
                (Some(r), _) => fields.insert(r.into(), f),
                (None, Some(k)) => fields.insert(k, f),
                _ => None,
            };
        }
    } else if !ini.list("ExtraDev", "extradevices").is_empty() {
        extra_curves(ini, &et_names, &[], &[], &mut notes);
    }
    let keep = reads.cap(&mut notes);
    fields.retain(|k, _| crate::transports::ingest::Role::parse(k).is_some() || keep.contains(k));

    let mut c = json!({
        "poll_interval_ms": poll_ms(ini, 100),
        "ingest": { "fields": fields },
    });
    if device == 68 {
        let rtd = match ini
            .text("Device", "phidget1200_formula")
            .unwrap_or_default()
            .trim()
        {
            "1" => "pt100_3920",
            "2" => "pt1000_3850",
            "3" => "pt1000_3920",
            _ => "pt100_3850",
        };
        let wires = match ini
            .text("Device", "phidget1200_wire")
            .unwrap_or_default()
            .trim()
        {
            "1" => 3,
            "2" => 4,
            _ => 2,
        };
        c["rtd_type"] = json!(rtd);
        c["rtd_wires"] = json!(wires);
        if let Some(ms) = ini
            .int("Device", "phidget1200_dataRate")
            .filter(|m| *m >= 8)
        {
            c["data_interval_ms"] = json!(ms.min(60_000));
        }
        notes.push(format!(
            "the RTD is a {rtd} on {wires} wires, as the file has it"
        ));
    } else {
        let types: Vec<&'static str> = ini
            .list("Device", "phidget1048_types")
            .iter()
            .map(|t| thermocouple_of(t))
            .collect();
        let used: Vec<usize> = fields
            .values()
            .filter_map(|f| {
                f.as_str()
                    .or_else(|| f["path"].as_str())
                    .map(str::to_string)
            })
            .filter_map(|p| crate::transports::ingest::phidget_input(&p).map(usize::from))
            .filter(|n| *n < 4)
            .collect();
        let type_of = |n: usize| types.get(n).copied().unwrap_or("K");
        let first = type_of(0);
        c["thermocouple_type"] = json!(first);
        let own: Map<String, Value> = used
            .iter()
            .filter(|n| type_of(**n) != first)
            .map(|n| (format!("ch{n}"), json!(type_of(*n))))
            .collect();
        if !own.is_empty() {
            c["thermocouple_types"] = Value::Object(own);
        }
        if let Some(ms) = ini
            .int("Device", "phidget1048_dataRate")
            .filter(|m| *m >= 8)
        {
            c["data_interval_ms"] = json!(ms.min(60_000));
        }
        notes.push(format!(
            "type {first} thermocouples{}, as the file has them",
            if c.get("thermocouple_types").is_some() {
                " (some inputs their own type)"
            } else {
                ""
            }
        ));
    }
    // Artisan reads a board on its own USB, or through a network server when it is remote.
    let remote = ini
        .text("Device", "phidgetRemoteFlag")
        .is_some_and(|f| f.trim().eq_ignore_ascii_case("true"));
    let server = ini
        .text("Device", "phidgetServerID")
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty() && h != "127.0.0.1" && h != "localhost");
    match server.filter(|_| remote) {
        Some(h) => {
            c["host"] = json!(h);
            if let Some(p) = ini
                .int("Device", "phidgetPort")
                .filter(|p| (1..=65_535).contains(p))
            {
                c["port"] = json!(p);
            }
        }
        None => plan.needs.push("host".into()),
    }
    if ini
        .list("Sliders", "slideractions")
        .iter()
        .any(|a| !matches!(a.trim(), "" | "0"))
    {
        notes.push(
            "the sliders write through another device; a Phidget connection only reads: \
             left out"
                .into(),
        );
    }
    notes.insert(
        0,
        format!(
            "{board} read through the Phidget22 network server: run it on the computer the \
             board is plugged into (Phidgets' phidget22networkserver) and give that computer's \
             host; BT on input 0{}",
            if four { ", ET on input 1" } else { "" }
        ),
    );
    notes.push(
        "Artisan's change triggers and async mode are not used: Roastwarden takes each reading \
         and makes each sample the mean of those since the last"
            .into(),
    );
    plan.protocol = Some(Protocol::Phidget22);
    plan.config = Some(c);
    plan.notes.extend(notes);
    let channels = std::mem::take(&mut reads.channels);
    finish(ini, plan, Outputs::default(), channels, Kind::DrumGas);
}
