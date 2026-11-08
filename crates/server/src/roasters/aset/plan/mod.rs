//! A roaster planned from a machine setup: the plan, the entry per Artisan device, and what
//! every plan ends with (the setup at control off, the channels, the notes).

use serde::Serialize;
use serde_json::{json, Map, Value};
use utoipa::ToSchema;

use crate::roasters::channels::Channel;
use crate::roasters::setup::{Control, Kind, Outputs};
use crate::roasters::types::{CreateRegisterMapEntry, Protocol};
use crate::transports::ingest::Role;

mod fuji;
mod inputs;
mod modbus;
mod mqtt;
mod names;
mod phidget;
mod sliders;
mod tc4;
#[cfg(test)]
mod tests;
mod websocket;

use fuji::plan_fuji;
use inputs::{extra_curves, Etypes, Reads};
use modbus::plan_modbus;
use mqtt::plan_mqtt;
use phidget::plan_phidget;
use tc4::plan_tc4;
use websocket::plan_websocket;

pub(crate) use inputs::Input;

use super::catalogue_source;
use super::ini::Ini;
use super::machines::{machine_text, summary, Machine};

/// A roaster planned from an Artisan machine setup. Nothing is made: `roast roaster add
/// --artisan` and the dashboard's Add roaster apply it (device, connection, register map,
/// setup at control off).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Plan {
    pub machine: Machine,
    /// Where the setup came from: Artisan's catalogue at its commit, or the file sent
    pub source: String,
    /// `modbus_rtu` (ASCII framing too), `modbus_tcp`, `serial_tc4`, `websocket` or `mqtt`; null
    /// when it is not supported
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,
    /// The connection's config (as `POST /api/devices/{id}/connections` takes it): for a TC4,
    /// WebSocket or MQTT machine with its ingest mapping and command templates
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub config: Option<Value>,
    /// The register map (as `PUT /api/devices/{id}/register-map` takes it); Modbus only
    pub registers: Vec<CreateRegisterMapEntry>,
    /// The setup (as `PUT /api/roaster/{id}/setup` takes it): control always `off`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<PlanSetup>,
    /// What the file cannot know and the owner must give: `host` (Modbus TCP, a WebSocket's
    /// address), `port` (the serial port on the server), `telemetry_topic` (MQTT: the topic the
    /// roaster's data reaches the server's broker on)
    pub needs: Vec<String>,
    /// What was read, guessed or left out, one sentence each
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanSetup {
    pub kind: Kind,
    pub control: Control,
    pub outputs: Outputs,
    pub fan_floor_pct: f64,
    pub channels: Vec<Channel>,
}

/// Plan a roaster from a catalogue machine (`Brand/Model`).
pub fn plan_machine(key: &str) -> Option<Plan> {
    let (key, text) = machine_text(key)?;
    let mut p = plan(text, key);
    p.source = format!("Artisan's machine catalogue, {}", catalogue_source());
    Some(p)
}

/// Plan a roaster from a machine setup's text. `key` is the catalogue's (`Brand/Model`) or a
/// file name.
pub fn plan(text: &str, key: &str) -> Plan {
    let ini = Ini::parse(text);
    let machine = summary(key, &ini);
    let mut plan = Plan {
        source: format!("the setup file {key}"),
        machine,
        protocol: None,
        config: None,
        registers: Vec::new(),
        setup: None,
        needs: Vec::new(),
        notes: Vec::new(),
    };
    if !plan.machine.supported {
        return plan;
    }
    match plan.machine.artisan_device {
        0 => plan_fuji(&ini, &mut plan),
        19 => plan_tc4(&ini, &mut plan),
        111 => plan_websocket(&ini, &mut plan),
        201 => plan_mqtt(&ini, &mut plan),
        34 | 52 | 58 | 61 | 68 => plan_phidget(&ini, &mut plan),
        _ => plan_modbus(&ini, &mut plan),
    }
    plan
}

/// Artisan's sampling interval (`[General] Delay`, ms) as a poll interval.
fn poll_ms(ini: &Ini, min: i64) -> i64 {
    ini.int("General", "Delay")
        .filter(|d| *d > 0)
        .unwrap_or(1000)
        .clamp(min, 10_000)
}

/// A Modbus serial connection (RTU, or ASCII framing) from a setup's serial settings, in
/// `[Modbus]` or `[SerialPort]`.
fn serial_config(ini: &Ini, section: &str, unit: u8, ascii: bool, plan: &mut Plan) -> Value {
    let parity = match ini.text(section, "parity").as_deref().map(str::trim) {
        Some("E") => "even",
        Some("O") => "odd",
        _ => "none",
    };
    let baud = ini
        .int(section, "baudrate")
        .filter(|b| (300..=1_000_000).contains(b))
        .unwrap_or(if ascii { 9600 } else { 115_200 });
    let stop = ini
        .int(section, "stopbits")
        .filter(|s| (1..=2).contains(s))
        .unwrap_or(1);
    let bits = if ascii {
        ini.int(section, "bytesize")
            .filter(|b| (7..=8).contains(b))
            .unwrap_or(7)
    } else {
        8
    };
    let timeout_ms = ini
        .num(section, "timeout")
        .map_or(1000, |s| (s * 1000.0).round() as i64)
        .clamp(200, 10_000);
    plan.needs.push("port".into());
    plan.notes.push(format!(
        "Modbus {} at {baud} baud, {bits}{}{stop}: give the serial port on the server (the \
         file's {} is the maker's computer)",
        if ascii { "ASCII" } else { "RTU" },
        parity[..1].to_uppercase(),
        ini.text(section, "comport")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "port".into())
    ));
    plan.protocol = Some(Protocol::ModbusRtu);
    let mut c = json!({
        "baud": baud,
        "parity": parity,
        "data_bits": bits,
        "stop_bits": stop,
        "unit_id": unit,
        "poll_interval_ms": poll_ms(ini, 100),
        "timeout_ms": timeout_ms,
    });
    if ascii {
        c["framing"] = json!("ascii");
    }
    c
}

/// The ingest fields of a WebSocket or MQTT machine: channel 1 BT, 2 ET, the extra curves on
/// channels 3–12, each by its path (`path(n)`), a temperature extra in its own unit
/// (`unit(n)`). Each field keeps its `channel` for the caller. The fields and the channels.
#[allow(clippy::type_complexity)]
fn channel_fields(
    ini: &Ini,
    et: &Etypes,
    pairs: &[(i64, usize, usize)],
    path: &dyn Fn(usize) -> Option<String>,
    unit: &dyn Fn(usize) -> Option<&'static str>,
    units: &str,
    notes: &mut Vec<String>,
) -> (Map<String, Value>, Vec<Channel>) {
    let mut fields = Map::new();
    let mut reads = Reads::default();
    let readable = |n: usize, what: &str, notes: &mut Vec<String>| -> Option<String> {
        let p = path(n)?;
        match crate::transports::ingest::check_path(&p) {
            Ok(()) => Some(p),
            Err(_) => {
                notes.push(format!(
                    "{what} (channel {n}): `{p}` is more than a path Roastwarden reads (keys and \
                     [n] indexes, as data.bt or probes[0].value); left out"
                ));
                None
            }
        }
    };
    for (n, role) in [(1, "bt"), (2, "et")] {
        if let Some(p) = readable(n, &role.to_uppercase(), notes) {
            if unit(n).is_some_and(|u| u != units) {
                notes.push(format!(
                    "{} is in °{}, the device's other temperatures in °{units}: check it",
                    role.to_uppercase(),
                    unit(n).unwrap_or("C")
                ));
            }
            fields.insert(role.into(), json!({ "path": p, "channel": n }));
            reads.roles.push(role);
            reads.used.push(role.into());
        }
    }
    let shown = extra_curves(ini, et, pairs, &[], notes);
    for (n, (raw, math)) in &shown {
        let n = *n;
        let Some(p) = readable(n, &et.named(raw), notes) else {
            if path(n).is_none() && !raw.trim().is_empty() {
                notes.push(format!(
                    "{} (channel {n}): Artisan reads no field for it; left out",
                    et.named(raw)
                ));
            }
            continue;
        };
        let c = reads.curve(
            ini,
            et,
            raw,
            math,
            unit(n).is_some(),
            &p,
            n,
            "Channel",
            notes,
        );
        let mut f = json!({ "path": p, "channel": n });
        if let Some(k) = c.scale {
            f["scale"] = json!(k);
        }
        if c.temp && c.role.is_none() {
            f["unit"] = json!(unit(n).unwrap_or("C"));
        }
        match (c.role, c.key) {
            (Some(r), _) => fields.insert(r.into(), f),
            (None, Some(k)) => fields.insert(k, f),
            _ => None,
        };
    }
    for n in 3..=12 {
        if path(n).is_some() && !shown.contains_key(&n) {
            notes.push(format!(
                "channel {n} ({}) is read by Artisan but drawn as no curve; left out",
                path(n).unwrap_or_default()
            ));
        }
    }
    let keep = reads.cap(notes);
    fields.retain(|k, _| Role::parse(k).is_some() || keep.contains(k));
    (fields, reads.channels)
}

/// The setup a plan proposes: the kind from Artisan's heating (else `guess` with outputs, a
/// logger without), control off, the outputs and channels.
fn finish(ini: &Ini, plan: &mut Plan, outputs: Outputs, channels: Vec<Channel>, guess: Kind) {
    let heating = ini.int("General", "roasterheating_setup").unwrap_or(0);
    let kind = match heating {
        1 | 2 => Kind::DrumGas,
        3 => Kind::DrumElectric,
        _ if outputs.heater.is_none() && outputs.fan.is_none() => Kind::Logger,
        _ => guess,
    };
    plan.notes.push(format!(
        "kind {} from the file's heating{}; change it under Setup if it is not right",
        kind.as_str(),
        match heating {
            1 => " (LPG)",
            2 => " (natural gas)",
            3 => " (electric)",
            _ => " (not given: a guess)",
        }
    ));
    let declared: Vec<&str> = [
        ("heater", outputs.heater.is_some()),
        ("fan", outputs.fan.is_some()),
        ("drum", outputs.drum.is_some()),
        ("damper", outputs.damper.is_some()),
    ]
    .iter()
    .filter(|(_, on)| *on)
    .map(|(n, _)| *n)
    .collect();
    plan.notes.push(if declared.is_empty() {
        "no outputs: Roastwarden only records it".into()
    } else {
        format!(
            "outputs {} from Artisan's sliders; control is off until the owner checks the \
             readings (`roast roaster check`) and switches it on",
            declared.join(", ")
        )
    });
    plan.setup = Some(PlanSetup {
        kind,
        control: Control::Off,
        outputs,
        fan_floor_pct: 0.0,
        channels,
    });
}

/// A register name not used yet (names are unique in a map, whatever their case).
fn unique(used: &mut Vec<String>, name: &str) -> String {
    let base: String = name.chars().take(60).collect();
    let mut n = base.clone();
    let mut k = 2;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&n)) {
        n = format!("{base} {k}");
        k += 1;
    }
    used.push(n.clone());
    n
}

/// `writeSingle(1,100,{})`, `write([1,100,{}])`, `writeWord(…)`, `writeLong(…)`, `writem(…)`,
/// `wcoil(…)` → (function, slave, register). Only `{}` as the value.
pub(super) fn parse_write(cmd: &str) -> Option<(&'static str, u8, i64)> {
    let (func, args) = cmd.split_once('(')?;
    let func = match func.trim() {
        "write" | "writeSingle" | "writem" => "writeSingle",
        "writeWord" => "writeWord",
        "writeLong" => "writeLong",
        "wcoil" => "wcoil",
        _ => return None,
    };
    let args = args.trim().strip_suffix(')')?.trim();
    let args = args
        .strip_prefix('[')
        .and_then(|a| a.strip_suffix(']'))
        .unwrap_or(args);
    let parts: Vec<&str> = args.split(',').map(str::trim).collect();
    if parts.len() != 3 || parts[2] != "{}" {
        return None;
    }
    let slave: u8 = parts[0].parse().ok().filter(|s| (1..=247).contains(s))?;
    let register: i64 = parts[1].parse().ok().filter(|r| (0..=65535).contains(r))?;
    Some((func, slave, register))
}
