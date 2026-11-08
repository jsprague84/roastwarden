//! A Modbus machine (Artisan device 29): its inputs as a register map with roles, its
//! sliders as writable registers, the PID's setpoint.

use serde_json::json;

use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::{
    CreateRegisterMapEntry, ModbusDataType, ModbusRegisterType, Protocol,
};

use super::inputs::{extra_curves, Etypes, Input, Reads, EXTRAS_MODBUS};
use super::sliders::{sliders, slot};
use super::{finish, parse_write, poll_ms, serial_config, unique, Plan};
use crate::roasters::aset::ini::Ini;

/// A Modbus machine (Artisan device 29): its inputs as a register map with roles, its sliders as
/// writable registers, the PID's setpoint.
pub(super) fn plan_modbus(ini: &Ini, plan: &mut Plan) {
    let et = Etypes::read(ini);
    let mut notes = Vec::new();
    let mut registers: Vec<CreateRegisterMapEntry> = Vec::new();
    let mut reads = Reads::default();

    // ---- the connection ----
    let kind = ini.int("Modbus", "type").unwrap_or(0);
    let inputs: Vec<Input> = (1..=12).filter_map(|n| Input::read(ini, n)).collect();
    let unit = inputs.first().map_or(1, |i| i.unit);
    if kind == 3 {
        let timeout_ms = ini
            .num("Modbus", "IP_timeout")
            .map_or(1000, |s| (s * 1000.0).round() as i64)
            .clamp(200, 10_000);
        let host = ini
            .text("Modbus", "host")
            .map(|h| h.trim().to_string())
            .unwrap_or_default();
        let port = ini
            .int("Modbus", "port")
            .filter(|p| (1..=65535).contains(p));
        let mut c = json!({
            "port": port.unwrap_or(502),
            "unit_id": unit,
            "poll_interval_ms": poll_ms(ini, 100),
            "timeout_ms": timeout_ms,
        });
        if host.is_empty() || host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost") {
            plan.needs.push("host".into());
            notes.push(
                "Modbus TCP: give the roaster's address (the file has none of its own)".into(),
            );
        } else {
            c["host"] = json!(host);
            notes.push(format!(
                "Modbus TCP to {host}:{}, the maker's example address: check it is your \
                 roaster's",
                port.unwrap_or(502)
            ));
        }
        plan.protocol = Some(Protocol::ModbusTcp);
        plan.config = Some(c);
    } else {
        plan.config = Some(serial_config(ini, "Modbus", unit, kind == 1, plan));
    }
    let word_order = if ini.text("Modbus", "wordorderLittle").is_none()
        || ini.flag("Modbus", "wordorderLittle")
    {
        "CDAB"
    } else {
        "ABCD"
    };
    let own_unit = |u: u8| (u != unit).then_some(u as i64);

    // ---- reading: inputs → registers ----
    let read_entry = |inp: &Input, name: String, role: Option<&str>, temp: bool, k: Option<f64>| {
        let register_type = match inp.code {
            4 => ModbusRegisterType::Input,
            1 => ModbusRegisterType::Coil,
            2 => ModbusRegisterType::Discrete,
            _ => ModbusRegisterType::Holding,
        };
        let bits = matches!(
            register_type,
            ModbusRegisterType::Coil | ModbusRegisterType::Discrete
        );
        let data_type = if bits {
            ModbusDataType::Bool
        } else {
            inp.data_type()
        };
        let div = match inp.div {
            1 => Some(0.1),
            2 => Some(0.01),
            _ => None,
        };
        // A formula that is a scale multiplies it.
        let div = match (div, k) {
            (d, Some(k)) => Some(d.unwrap_or(1.0) * k),
            (d, None) => d,
        };
        let fahrenheit = temp && inp.mode.trim() == "F";
        // A role converts °F itself (the register's unit); a channel is kept in °C, so its
        // register converts: (raw × div − 32) × 5/9.
        let (scale, offset, unit) = match (fahrenheit, role) {
            (true, Some(_)) => (div, None, Some("F".to_string())),
            (true, None) => (
                Some(div.unwrap_or(1.0) * 5.0 / 9.0),
                Some(-32.0 * 5.0 / 9.0),
                Some("C".to_string()),
            ),
            (false, _) if temp => (div, None, Some("C".to_string())),
            _ => (div, None, None),
        };
        CreateRegisterMapEntry {
            register_type,
            address: inp.register as i32,
            name,
            data_type: data_type.clone(),
            byte_order: (data_type.words() == 2).then(|| word_order.to_string()),
            scale_factor: scale,
            offset,
            unit,
            description: Some(format!(
                "Artisan input {} (slave {}, register {}, function {})",
                inp.n, inp.unit, inp.register, inp.code
            )),
            writable: Some(false),
            role: role.map(str::to_string),
            unit_id: own_unit(inp.unit),
        }
    };
    let out_of_range = |inp: &Input, notes: &mut Vec<String>| {
        let bad = !(0..=65535).contains(&inp.register);
        if bad {
            notes.push(format!(
                "Artisan input {} (slave {}, register {}): its register is outside 0 to 65535; \
                 left out",
                inp.n, inp.unit, inp.register
            ));
        }
        bad
    };
    for inp in inputs.iter().filter(|i| i.n <= 2) {
        let (name, role) = if inp.n == 1 {
            ("BT", "bt")
        } else {
            ("ET", "et")
        };
        if out_of_range(inp, &mut notes) {
            continue;
        }
        let n = unique(&mut reads.used, name);
        registers.push(read_entry(inp, n, Some(role), true, None));
        reads.roles.push(role);
    }
    if !reads.roles.contains(&"bt") {
        notes.push("no BT: Artisan's input 1 is not in use; ET is read alone".into());
    }
    let shown = extra_curves(ini, &et, &EXTRAS_MODBUS, &[], &mut notes);
    for inp in inputs.iter().filter(|i| i.n > 2) {
        let Some((raw_name, math)) = shown.get(&inp.n) else {
            notes.push(format!(
                "Artisan input {} (slave {}, register {}): read by Artisan but drawn as no \
                 curve; left out",
                inp.n, inp.unit, inp.register
            ));
            continue;
        };
        if out_of_range(inp, &mut notes) {
            continue;
        }
        let c = reads.curve(
            ini,
            &et,
            raw_name,
            math,
            !inp.mode.trim().is_empty(),
            "",
            inp.n,
            "Input",
            &mut notes,
        );
        match (c.role, c.key) {
            (Some(r), _) => {
                let reg_name = unique(&mut reads.used, &c.name);
                registers.push(read_entry(inp, reg_name, Some(r), c.temp, c.scale));
            }
            (None, Some(key)) => {
                let label: String = c.name.chars().take(24).collect();
                let mut entry = read_entry(inp, key, None, c.temp, c.scale);
                entry.description = Some(format!(
                    "{label}: {}",
                    entry.description.clone().unwrap_or_default()
                ));
                registers.push(entry);
            }
            (None, None) => {}
        }
    }
    let keep = reads.cap(&mut notes);
    registers.retain(|r| r.role.is_some() || keep.contains(&r.name));

    // ---- writing: sliders → outputs and writable registers ----
    let mut outputs = Outputs::default();
    for s in sliders(ini, &et, "2", "Modbus write", &mut notes) {
        let label = &s.label;
        let Some((func, slave, register)) = parse_write(&s.first) else {
            notes.push(format!(
                "slider {label}: `{}` is not a single register write Roastwarden knows; left out",
                s.cmd
            ));
            continue;
        };
        if func == "wcoil" {
            notes.push(format!(
                "slider {label} switches a coil (`{}`); left out",
                s.first
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
        let data_type = match func {
            "writeWord" => ModbusDataType::Float32,
            "writeLong" => ModbusDataType::Int32,
            _ if s.low < 0.0 => ModbusDataType::Int16,
            _ => ModbusDataType::Uint16,
        };
        let (scale, offset) = (1.0 / s.k, -s.o / s.k);
        let name = unique(&mut reads.used, &format!("{label} out"));
        registers.push(CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Holding,
            address: register as i32,
            name,
            data_type: data_type.clone(),
            byte_order: (data_type.words() == 2).then(|| word_order.to_string()),
            scale_factor: (scale != 1.0).then_some(scale),
            offset: (offset != 0.0).then_some(offset),
            unit: None,
            description: Some(format!("Artisan slider {label}: {}", s.first)),
            writable: Some(true),
            role: Some(s.role.into()),
            unit_id: own_unit(slave),
        });
        if !s.rest.is_empty() {
            notes.push(format!(
                "slider {label} also sends `{}` after the write; Roastwarden writes the value only",
                s.rest.join("; ")
            ));
        }
    }

    // The PID's setpoint (Artisan's PID dialog, when the setup has one).
    let pid_unit = ini.int("Modbus", "PID_device_ID").unwrap_or(0);
    let sv = ini.int("Modbus", "PID_SV_register").unwrap_or(0);
    if (1..=247).contains(&pid_unit) && (1..=65535).contains(&sv) {
        let data_type = if ini.flag("Modbus", "SVwriteFloat") {
            ModbusDataType::Float32
        } else if ini.flag("Modbus", "SVwriteLong") {
            ModbusDataType::Int32
        } else {
            ModbusDataType::Uint16
        };
        let scale = match ini.int("Modbus", "SVmultiplier").unwrap_or(0) {
            1 => Some(0.1),
            2 => Some(0.01),
            _ => None,
        };
        let bt_f = registers
            .iter()
            .any(|r| r.role.as_deref() == Some("bt") && r.unit.as_deref() == Some("F"));
        let name = unique(&mut reads.used, "PID SV");
        registers.push(CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Holding,
            address: sv as i32,
            name,
            data_type: data_type.clone(),
            byte_order: (data_type.words() == 2).then(|| word_order.to_string()),
            scale_factor: scale,
            offset: None,
            unit: Some(if bt_f { "F" } else { "C" }.into()),
            description: Some(format!(
                "Artisan's PID setpoint (slave {pid_unit}, register {sv})"
            )),
            writable: Some(true),
            role: Some("setpoint".into()),
            unit_id: own_unit(pid_unit as u8),
        });
        notes.push(format!(
            "the PID's setpoint is register {sv} on slave {pid_unit}: Roastwarden's setpoint \
             writes it (the PID's own on/off stays with the machine)"
        ));
    }

    // Buttons are the owner's to wire: listed, not imported.
    let labels = ini.list("ExtraEventButtons", "extraeventslabels");
    let button_actions = ini.list("ExtraEventButtons", "extraeventsactions");
    let button_cmds = ini.list("ExtraEventButtons", "extraeventsactionstrings");
    let buttons: Vec<String> = labels
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            button_actions.get(*i).map(|a| a.trim()) == Some("4")
                && button_cmds.get(*i).is_some_and(|c| !c.trim().is_empty())
        })
        .map(|(i, l)| {
            format!(
                "{} → `{}`",
                l.replace('\n', " ").replace('\\', "").trim(),
                button_cmds[i].trim()
            )
        })
        .take(12)
        .collect();
    if !buttons.is_empty() {
        notes.push(format!(
            "Artisan's buttons are not imported (Roastwarden has no custom buttons): {}",
            buttons.join("; ")
        ));
    }
    plan.notes.extend(notes);
    plan.registers = registers;
    finish(ini, plan, outputs, reads.channels, Kind::DrumGas);
}
