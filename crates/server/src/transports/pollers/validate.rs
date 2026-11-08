//! The checks on what is saved: a connection's config for its protocol and a register map,
//! before either reaches the database. Each error is the 400's message.

use std::collections::HashMap;

use crate::roasters::types::{
    CreateRegisterMapEntry, ModbusDataType, ModbusRegisterType, Protocol,
};
use crate::transports::ingest::{Kind, MqttRoute, Role};
use crate::transports::types::{
    ModbusFraming, ModbusRtuConnectionConfig, ModbusTcpConnectionConfig, MqttConnectionConfig,
    SerialTc4ConnectionConfig, WebSocketConnectionConfig,
};

use super::modbus::is_bits;
use super::websocket::MIN_WS_POLL_MS;
use super::MIN_POLL_MS;

/// Check a connection's config for its protocol before it is saved. The error is the 400's
/// message, written for the person filling in the form.
pub(crate) fn check_connection_config(
    protocol: &Protocol,
    config: &serde_json::Value,
) -> Result<(), String> {
    fn parse<T: serde::de::DeserializeOwned>(
        config: &serde_json::Value,
        what: &str,
    ) -> Result<T, String> {
        serde_json::from_value(config.clone()).map_err(|e| format!("{what} config: {e}"))
    }
    fn poll_and_timeout(poll_ms: u64, timeout_ms: u64) -> Result<(), String> {
        if poll_ms < MIN_POLL_MS {
            return Err(format!(
                "poll_interval_ms is {poll_ms}: at least {MIN_POLL_MS}"
            ));
        }
        if !(50..=10_000).contains(&timeout_ms) {
            return Err(format!(
                "timeout_ms is {timeout_ms}: 50 to 10000 (a request's wait for its answer)"
            ));
        }
        Ok(())
    }
    if !config.is_object() {
        return Err("config must be a JSON object".into());
    }
    // The device's own field names and command syntax (docs/ingest-mapping.md): MQTT, the
    // WebSocket client and a TC4 take them; a Modbus register takes a role in its map.
    let mapped = config.get("ingest").is_some_and(|v| !v.is_null())
        || config.get("commands").is_some_and(|v| !v.is_null());
    match protocol {
        Protocol::Mqtt => crate::transports::ingest::check_config(Kind::Mqtt, config)?,
        Protocol::WebSocket => crate::transports::ingest::check_config(Kind::WebSocket, config)?,
        Protocol::SerialTc4 => crate::transports::ingest::check_config(Kind::Tc4, config)?,
        Protocol::Phidget22 if config.get("commands").is_some_and(|v| !v.is_null()) => {
            return Err("a Phidget connection reads temperatures only: it takes no commands".into())
        }
        Protocol::Phidget22 => crate::transports::ingest::check_config(Kind::Phidget, config)?,
        Protocol::ModbusTcp | Protocol::ModbusRtu if mapped => {
            return Err(
                "a Modbus connection takes no ingest or commands: give each register \
                 a role in the register map instead"
                    .into(),
            )
        }
        Protocol::ModbusTcp | Protocol::ModbusRtu => {}
    }
    match protocol {
        Protocol::Mqtt => {
            let _: MqttConnectionConfig = parse(config, "MQTT")?;
            // Prefix and telemetry topic: 1–256 characters, no wildcards.
            MqttRoute::from_config("device", config).map(|_| ())
        }
        Protocol::WebSocket => {
            let c: WebSocketConnectionConfig = parse(config, "WebSocket")?;
            let rest = c
                .url
                .strip_prefix("ws://")
                .or_else(|| c.url.strip_prefix("wss://"));
            match rest {
                Some(host) if !host.trim().is_empty() => {}
                _ => {
                    return Err(format!(
                        "WebSocket url '{}' must be ws://host… or wss://host…",
                        c.url
                    ))
                }
            }
            if let Some(p) = &c.poll {
                let n = p.message.chars().count();
                if !(1..=1024).contains(&n) {
                    return Err(format!("poll.message is {n} characters: 1 to 1024"));
                }
                if p.interval_ms < MIN_WS_POLL_MS {
                    return Err(format!(
                        "poll.interval_ms is {}: at least {MIN_WS_POLL_MS}",
                        p.interval_ms
                    ));
                }
            }
            Ok(())
        }
        Protocol::ModbusTcp => {
            let c: ModbusTcpConnectionConfig = parse(config, "Modbus TCP")?;
            if c.host.trim().is_empty() || c.host.contains(char::is_whitespace) {
                return Err("Modbus TCP needs a host (an IP address or a name)".into());
            }
            if c.port == 0 {
                return Err("Modbus TCP port 0 is not a port (502 is the usual one)".into());
            }
            poll_and_timeout(c.poll_interval_ms, c.timeout_ms)
        }
        Protocol::ModbusRtu => {
            let c: ModbusRtuConnectionConfig = parse(config, "Modbus RTU")?;
            if c.port.trim().is_empty() {
                return Err("Modbus RTU needs a serial port, e.g. /dev/ttyUSB0".into());
            }
            if !(300..=1_000_000).contains(&c.baud) {
                return Err(format!(
                    "baud {} is not a serial speed (9600, 19200, …)",
                    c.baud
                ));
            }
            match c.framing {
                ModbusFraming::Rtu if c.data_bits != 8 => {
                    return Err(format!(
                        "data_bits is {}: Modbus RTU frames are 8 data bits (Modbus ASCII \
                         takes 7 or 8: framing ascii)",
                        c.data_bits
                    ))
                }
                ModbusFraming::Ascii if !(7..=8).contains(&c.data_bits) => {
                    return Err(format!(
                        "data_bits is {}: Modbus ASCII is 7 or 8 data bits",
                        c.data_bits
                    ))
                }
                _ => {}
            }
            if !(1..=2).contains(&c.stop_bits) {
                return Err(format!("stop_bits is {}: 1 or 2", c.stop_bits));
            }
            if !(1..=247).contains(&c.unit_id) {
                return Err(format!(
                    "unit_id is {}: a Modbus RTU slave address is 1 to 247",
                    c.unit_id
                ));
            }
            poll_and_timeout(c.poll_interval_ms, c.timeout_ms)
        }
        Protocol::SerialTc4 => {
            let c: SerialTc4ConnectionConfig = parse(config, "Serial TC4")?;
            if c.port.trim().is_empty() {
                return Err("Serial TC4 needs a serial port, e.g. /dev/ttyUSB0".into());
            }
            if c.baud == 0 {
                return Err("baud 0 is not a serial speed".into());
            }
            // The dashboard's rules (lib/roaster/tc4.ts, tc4Problems): 0 is off, 1–4 a TC4
            // input, each input feeds one channel, and ET or BT has one.
            let channels = [
                ("ET", c.et_channel),
                ("BT", c.bt_channel),
                ("T3", c.t3_channel),
                ("T4", c.t4_channel),
            ];
            if let Some((name, v)) = channels.iter().find(|(_, v)| *v > 4) {
                return Err(format!("{name} channel is {v}: 0 (off) to 4"));
            }
            if c.et_channel == 0 && c.bt_channel == 0 {
                return Err("ET or BT needs a channel".into());
            }
            let used: Vec<u8> = channels
                .iter()
                .map(|(_, v)| *v)
                .filter(|v| *v > 0)
                .collect();
            if (1..used.len()).any(|i| used[..i].contains(&used[i])) {
                return Err("each TC4 input can feed one channel only".into());
            }
            Ok(())
        }
        Protocol::Phidget22 => check_phidget(config),
    }
}

/// A Phidget22 connection's settings (docs/phidgets.md).
fn check_phidget(config: &serde_json::Value) -> Result<(), String> {
    let s = crate::transports::phidget::Setup::from_config(config)?;
    let c = &s.config;
    if c.host.trim().is_empty() || c.host.contains(char::is_whitespace) {
        return Err(
            "a Phidget connection needs the host of its Phidget22 network server (the \
             computer the board is plugged into)"
                .into(),
        );
    }
    if c.port == 0 {
        return Err("port 0 is not a port (the Phidget22 network server's is 5661)".into());
    }
    if c.password.as_ref().is_some_and(|p| p.chars().count() > 64) {
        return Err("password: at most 64 characters".into());
    }
    if c.poll_interval_ms < MIN_POLL_MS {
        return Err(format!(
            "poll_interval_ms is {}: at least {MIN_POLL_MS}",
            c.poll_interval_ms
        ));
    }
    if !(8..=60_000).contains(&c.data_interval_ms) {
        return Err(format!(
            "data_interval_ms is {}: 8 to 60000 (how often the board reports each input)",
            c.data_interval_ms
        ));
    }
    if let Some(p) = c.hub_port.filter(|p| !(0..=5).contains(p)) {
        return Err(format!("hub_port is {p}: a VINT hub's ports are 0 to 5"));
    }
    if let Some(k) = c
        .thermocouple_types
        .iter()
        .flat_map(|m| m.keys())
        .find(|k| crate::transports::ingest::phidget_input(k).is_none())
    {
        return Err(format!(
            "thermocouple_types: '{k}' is not a board input (ch0 to ch7)"
        ));
    }
    if !(2..=4).contains(&c.rtd_wires) {
        return Err(format!("rtd_wires is {}: 2, 3 or 4", c.rtd_wires));
    }
    Ok(())
}

/// The most registers a map may have.
const MAX_MAP: usize = 1000;
const BYTE_ORDERS: [&str; 6] = ["AB", "BA", "ABCD", "CDAB", "BADC", "DCBA"];

/// Check a register map before it is saved: names, kinds, data types, addresses, word order,
/// scale. The error names the register (its position and name) and what is wrong with it.
pub(crate) fn check_register_map(entries: &[CreateRegisterMapEntry]) -> Result<(), String> {
    if entries.len() > MAX_MAP {
        return Err(format!(
            "{} registers: a map has at most {MAX_MAP}",
            entries.len()
        ));
    }
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut roles: HashMap<(Role, bool), usize> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let n = i + 1;
        let name = e.name.trim();
        let at = |what: String| format!("register {n} ('{name}'): {what}");
        if name.is_empty() {
            return Err(format!("register {n}: it needs a name"));
        }
        if name.len() > 64 {
            return Err(at("the name is longer than 64 characters".into()));
        }
        if let Some(first) = names.insert(name.to_ascii_lowercase(), n) {
            return Err(at(format!(
                "register {first} has the same name; each value needs its own"
            )));
        }
        let bits = is_bits(e.register_type);
        if bits && e.data_type != ModbusDataType::Bool {
            return Err(at(format!(
                "a {} is one bit: its data type is bool, not {}",
                e.register_type, e.data_type
            )));
        }
        let words = e.data_type.words() as i32;
        if e.address < 0 || e.address + words - 1 > u16::MAX as i32 {
            return Err(at(format!(
                "address {} is outside 0 to 65535{}",
                e.address,
                if words == 2 {
                    " (a 32-bit value takes two)"
                } else {
                    ""
                }
            )));
        }
        if e.writable == Some(true)
            && matches!(
                e.register_type,
                ModbusRegisterType::Input | ModbusRegisterType::Discrete
            )
        {
            return Err(at(format!(
                "a {} register is read-only on every device; only holding registers and coils \
                 are writable",
                e.register_type
            )));
        }
        if let Some(u) = e.unit_id {
            if !(1..=247).contains(&u) {
                return Err(at(format!(
                    "slave {u}: a Modbus slave address is 1 to 247 (leave it out for the \
                     connection's)"
                )));
            }
        }
        if let Some(order) = e.byte_order.as_deref().map(str::trim) {
            if !order.is_empty() && !BYTE_ORDERS.iter().any(|o| o.eq_ignore_ascii_case(order)) {
                return Err(at(format!(
                    "byte order '{order}' is not one of {}",
                    BYTE_ORDERS.join(", ")
                )));
            }
        }
        match e.scale_factor {
            Some(s) if !s.is_finite() || s == 0.0 => {
                return Err(at(format!(
                    "scale factor {s}: a finite number other than 0"
                )))
            }
            _ => {}
        }
        if e.offset.is_some_and(|o| !o.is_finite()) {
            return Err(at("the offset is not a finite number".into()));
        }
        // Roles (docs/ingest-mapping.md, "Modbus TCP and RTU"): one readable and one writable
        // register per role at most; emergency stop is a write role only.
        if let Some(role) = e.role.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            let r = Role::parse(role).ok_or_else(|| {
                at(format!(
                    "role '{role}' is not one of {}",
                    crate::transports::ingest::ROLES
                        .iter()
                        .map(|(n, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            let writable = e.writable == Some(true)
                && matches!(
                    e.register_type,
                    ModbusRegisterType::Holding | ModbusRegisterType::Coil
                );
            if !r.readable() && !writable {
                return Err(at(format!(
                    "the role {role} is written, never read: mark the holding register or coil \
                     writable"
                )));
            }
            if let Some(first) = roles.insert((r, writable), n) {
                return Err(at(format!(
                    "register {first} already has the role {role} as a {} register; a role is on \
                     one readable and one writable register at most",
                    if writable { "writable" } else { "readable" }
                )));
            }
        }
    }
    Ok(())
}
