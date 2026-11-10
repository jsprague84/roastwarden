//! Device command transports.
//!
//! Telemetry flows *in* through several paths (MQTT consumer, device
//! WebSocket, outbound WebSocket client, Modbus poller) and all land in
//! [`crate::analysis::telemetry::TelemetryService`]. This module is the matching path
//! *out*: every control command goes through a [`TransportRegistry`], which
//! picks the right [`CommandSink`] for the device.
//!
//! Sinks register themselves while they are live (a connected WebSocket, a
//! connected Modbus poller). MQTT is the fallback for devices that are
//! configured for it or unknown to the device registry. A device whose only
//! configured transports are offline gets a [`TransportError::NoTransport`]
//! instead of a command silently published into the void.

use sqlx::SqlitePool;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::setup::{Control, Limits, Refusal, Setups};
use crate::transports::ingest::{in_device_units, Commands, MqttRouter, Role, Units};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use prometheus::IntCounter;
use roastwarden_core::{ControlMode, DeviceCommand};
use roastwarden_mqtt::{MqttEvent, MqttService};
use rumqttc::QoS;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::roasters::types::{ModbusDataType, ModbusRegisterMap, ModbusRegisterType, Protocol};

// ---------------------------------------------------------------------------
// Errors and options
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TransportError {
    /// The device has no live transport and is not configured for MQTT.
    NoTransport {
        device_id: String,
        detail: String,
    },
    /// The transport cannot carry this command (e.g. autotune over Modbus).
    Unsupported {
        transport: &'static str,
        command: &'static str,
    },
    /// The connection has no standard way to write this command (a TC4's drum or damper) and
    /// no template for it (docs/ingest-mapping.md, "`commands`").
    NeedsTemplate {
        transport: &'static str,
        command: &'static str,
        connection: String,
    },
    /// The transport accepted the command but delivery failed.
    Send {
        transport: &'static str,
        detail: String,
    },
    Timeout {
        transport: &'static str,
    },
    /// The roaster's control level does not allow this command (docs/roaster-setup.md).
    ControlOff {
        device_id: String,
        control: Control,
        command: &'static str,
    },
    /// The roaster has not declared the output this command acts on (docs/roaster-setup.md,
    /// "Outputs").
    NotDeclared {
        device_id: String,
        output: &'static str,
    },
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NoTransport { device_id, detail } => {
                write!(
                    f,
                    "no transport available for device '{device_id}': {detail}"
                )
            }
            TransportError::Unsupported { transport, command } => {
                write!(f, "{transport} transport cannot carry '{command}'")
            }
            TransportError::NeedsTemplate {
                transport,
                command,
                connection,
            } => write!(
                f,
                "{transport} {connection} has no standard {command} command: give the \
                 connection a template (config.commands.{command}, docs/ingest-mapping.md)"
            ),
            TransportError::Send { transport, detail } => {
                write!(f, "{transport} send failed: {detail}")
            }
            TransportError::Timeout { transport } => {
                write!(f, "{transport} acknowledgement timed out")
            }
            TransportError::ControlOff {
                device_id,
                control: Control::Off,
                ..
            } => write!(
                f,
                "control is off for {device_id}: Roastwarden only records it \
                 (Roasters → {device_id} → Control)"
            ),
            TransportError::ControlOff {
                device_id,
                control,
                command,
            } => write!(
                f,
                "control is {} for {device_id}: {command} needs full control \
                 (Roasters → {device_id} → Control)",
                control.as_str()
            ),
            TransportError::NotDeclared { device_id, output } => write!(
                f,
                "{device_id} has no {output} output (Roasters → {device_id} → Control)"
            ),
        }
    }
}

impl std::error::Error for TransportError {}

#[derive(Debug, Clone, Copy)]
pub struct SendOptions {
    /// Wait for the transport-level acknowledgement where one exists
    /// (MQTT `PubAck`, Modbus write response).
    pub wait_ack: bool,
    pub timeout: Duration,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            wait_ack: false,
            timeout: Duration::from_millis(1000),
        }
    }
}

/// Whether a sample says the heater is on: `heater_enable`, or heater output above 0 from a
/// roaster that does not report the flag.
pub fn heater_on_in(s: &roastwarden_core::Sample) -> bool {
    s.heater_enable
        .unwrap_or_else(|| s.heater.is_some_and(|h| h > 0.0))
}

// ---------------------------------------------------------------------------
// Sink trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CommandSink: Send + Sync {
    fn name(&self) -> &'static str;
    async fn send(
        &self,
        device_id: &str,
        cmd: &DeviceCommand,
        opts: SendOptions,
    ) -> Result<(), TransportError>;
}

// ---------------------------------------------------------------------------
// MQTT
// ---------------------------------------------------------------------------

pub struct MqttSink {
    mqtt: MqttService,
    tx_total: IntCounter,
    /// A device's own prefix, unit and templates (docs/ingest-mapping.md, "MQTT").
    routes: Arc<MqttRouter>,
}

impl MqttSink {
    pub fn new(mqtt: MqttService, tx_total: IntCounter, routes: Arc<MqttRouter>) -> Self {
        Self {
            mqtt,
            tx_total,
            routes,
        }
    }

    /// The topic and payload of a command: the device's route when it has one, else
    /// `roaster/<id>/control/<command>` and the contract's payload.
    pub fn message(&self, device_id: &str, cmd: &DeviceCommand) -> (String, String) {
        match self.routes.for_device(device_id) {
            Some(route) => route.publish(cmd),
            None => (cmd.topic(device_id), cmd.payload()),
        }
    }
}

#[async_trait]
impl CommandSink for MqttSink {
    fn name(&self) -> &'static str {
        "mqtt"
    }

    async fn send(
        &self,
        device_id: &str,
        cmd: &DeviceCommand,
        opts: SendOptions,
    ) -> Result<(), TransportError> {
        let (topic, payload) = self.message(device_id, cmd);
        // Subscribe before publishing so the ack cannot be missed.
        let mut rx = opts.wait_ack.then(|| self.mqtt.events());
        self.mqtt
            .publish(&topic, QoS::AtLeastOnce, false, payload.into_bytes())
            .await
            .map_err(|e| TransportError::Send {
                transport: "mqtt",
                detail: e.to_string(),
            })?;
        self.tx_total.inc();

        if let Some(rx) = rx.as_mut() {
            let deadline = tokio::time::Instant::now() + opts.timeout;
            loop {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Ok(MqttEvent::PubAck(_))) => return Ok(()),
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) | Err(_) => {
                        return Err(TransportError::Timeout { transport: "mqtt" })
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WebSocket (both the inbound device socket and the outbound client)
// ---------------------------------------------------------------------------

/// Sends control frames down a WebSocket writer task.
///
/// Frame format, shared with the firmware guide:
/// `{"type":"control","topic":"roaster/<id>/control/<cmd>","payload":"<text>"}`.
pub struct WebSocketSink {
    name: &'static str,
    tx: mpsc::UnboundedSender<String>,
    /// The outbound client's templates and unit (docs/ingest-mapping.md); the inbound device
    /// socket is Roastwarden's own protocol and has none.
    commands: Option<Commands>,
    units: Units,
    /// A template's `{id}`: Artisan's WebSocket protocol numbers each request. Its own count,
    /// from a million, apart from the poll's.
    ids: AtomicU64,
}

impl WebSocketSink {
    pub fn new(name: &'static str, tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            name,
            tx,
            commands: None,
            units: Units::C,
            ids: AtomicU64::new(1_000_000),
        }
    }

    /// The device's templates (a template's payload is the whole frame) and unit.
    pub fn with_mapping(mut self, commands: Option<Commands>, units: Units) -> Self {
        self.commands = commands;
        self.units = units;
        self
    }

    pub fn frame(device_id: &str, cmd: &DeviceCommand) -> String {
        serde_json::json!({
            "type": "control",
            "topic": cmd.topic(device_id),
            "payload": cmd.payload(),
        })
        .to_string()
    }

    /// The frame for `cmd`: its template's payload (`{id}` the next request number), else the
    /// control frame (in the device's unit).
    pub fn frame_for(&self, device_id: &str, cmd: &DeviceCommand) -> String {
        let cmd = in_device_units(cmd, self.units);
        match self.commands.as_ref().and_then(|c| c.get(cmd.name())) {
            Some(t) => {
                let frame = crate::transports::ingest::render(t, &cmd);
                if frame.contains("{id}") {
                    let id = self.ids.fetch_add(1, Ordering::Relaxed);
                    frame.replace("{id}", &id.to_string())
                } else {
                    frame
                }
            }
            None => Self::frame(device_id, &cmd),
        }
    }
}

#[async_trait]
impl CommandSink for WebSocketSink {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn send(
        &self,
        device_id: &str,
        cmd: &DeviceCommand,
        _opts: SendOptions,
    ) -> Result<(), TransportError> {
        self.tx
            .send(self.frame_for(device_id, cmd))
            .map_err(|_| TransportError::Send {
                transport: self.name,
                detail: "connection closed".into(),
            })
    }
}

// ---------------------------------------------------------------------------
// Modbus TCP and RTU (writes executed by the poller that owns the connection)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ModbusWrite {
    Holding { address: u16, words: Vec<u16> },
    Coil { address: u16, on: bool },
}

pub struct ModbusWriteRequest {
    pub write: ModbusWrite,
    /// The register's own slave; none: the connection's.
    pub unit: Option<u8>,
    pub reply: oneshot::Sender<Result<(), String>>,
}

pub struct ModbusSink {
    /// `modbus_tcp` or `modbus_rtu`
    name: &'static str,
    registers: Vec<ModbusRegisterMap>,
    tx: mpsc::Sender<ModbusWriteRequest>,
}

impl ModbusSink {
    pub fn new(
        name: &'static str,
        registers: Vec<ModbusRegisterMap>,
        tx: mpsc::Sender<ModbusWriteRequest>,
    ) -> Self {
        Self {
            name,
            registers,
            tx,
        }
    }
}

#[async_trait]
impl CommandSink for ModbusSink {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn send(
        &self,
        _device_id: &str,
        cmd: &DeviceCommand,
        opts: SendOptions,
    ) -> Result<(), TransportError> {
        let transport = self.name;
        let writes = plan_unit_writes(transport, &self.registers, cmd)?;
        for (unit, write) in writes {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(ModbusWriteRequest {
                    write,
                    unit,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| TransportError::Send {
                    transport,
                    detail: "poller stopped".into(),
                })?;
            match tokio::time::timeout(opts.timeout.max(Duration::from_secs(2)), reply_rx).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(e))) => {
                    return Err(TransportError::Send {
                        transport,
                        detail: e,
                    })
                }
                Ok(Err(_)) => {
                    return Err(TransportError::Send {
                        transport,
                        detail: "poller dropped request".into(),
                    })
                }
                Err(_) => return Err(TransportError::Timeout { transport }),
            }
        }
        Ok(())
    }
}

/// Register names, by role, that a device register map may use. Matching is
/// case-insensitive and covers the Roastwarden Standard and Artisan Compatible
/// presets plus a few common controller spellings.
const SETPOINT_NAMES: &[&str] = &[
    "setpoint",
    "sv",
    "target_temp",
    "bean_setpoint",
    "set_value",
];
// Heater and fan commands are %; a register whose device scale differs (a
// 0-255 PWM register, say) converts with its scale_factor.
const FAN_NAMES: &[&str] = &["fan_cmd", "fan_setpoint", "fan", "io3", "air"];
const HEATER_NAMES: &[&str] = &["heater_cmd", "heater_setpoint", "heater", "ot1", "burner"];
const MODE_NAMES: &[&str] = &["control_mode", "pid_mode", "mode"];
const HEATER_ENABLE_NAMES: &[&str] = &["heater_enable", "heater_on", "heater_en"];
const ESTOP_NAMES: &[&str] = &["emergency_stop", "e_stop", "estop"];
const DRUM_NAMES: &[&str] = &["drum_cmd", "drum_speed", "drum"];
const DAMPER_NAMES: &[&str] = &["damper_cmd", "air_damper", "damper"];

/// A register's role (docs/ingest-mapping.md); none, or one this server does not know.
pub fn role_of(r: &ModbusRegisterMap) -> Option<Role> {
    r.role.as_deref().and_then(Role::parse)
}

/// A register a command can write: a holding register or coil marked writable.
pub fn is_writable(r: &ModbusRegisterMap) -> bool {
    matches!(
        r.register_type,
        ModbusRegisterType::Holding | ModbusRegisterType::Coil
    ) && r.writable
}

/// Whether a register's unit says °F.
pub fn in_fahrenheit(r: &ModbusRegisterMap) -> bool {
    r.unit.as_deref().and_then(Units::parse) == Some(Units::F)
}

/// The register a command writes: the writable holding register or coil with the command's
/// role, then the first name (in the role's order) among the registers with no role. A
/// register marked read-only (a read-back of the heater's output, say) is never written,
/// whatever its name or role: when the map has the role only as read-only registers, the
/// command fails and says so rather than being dropped quietly.
fn command_register<'a>(
    transport: &'static str,
    command: &'static str,
    regs: &'a [ModbusRegisterMap],
    role: Option<Role>,
    names: &[&str],
) -> Result<&'a ModbusRegisterMap, TransportError> {
    let has_role = |r: &ModbusRegisterMap| role.is_some() && role_of(r) == role;
    let named = |r: &ModbusRegisterMap, wanted: &str| {
        r.role.is_none() && r.name.eq_ignore_ascii_case(wanted)
    };
    if let Some(reg) = regs.iter().find(|r| is_writable(r) && has_role(r)) {
        return Ok(reg);
    }
    if let Some(reg) = names
        .iter()
        .find_map(|wanted| regs.iter().find(|r| is_writable(r) && named(r, wanted)))
    {
        return Ok(reg);
    }
    match regs.iter().find(|r| has_role(r)).or_else(|| {
        names
            .iter()
            .find_map(|wanted| regs.iter().find(|r| named(r, wanted)))
    }) {
        Some(r) => Err(TransportError::Send {
            transport,
            detail: format!(
                "the register map has '{}' ({} register {}) for {command}, but it is read-only; \
                 nothing was written (mark a holding register or coil writable for {command})",
                r.name, r.register_type, r.address
            ),
        }),
        None => Err(TransportError::Unsupported { transport, command }),
    }
}

/// Translate a command into register writes for the given map. `transport` names the link
/// in errors (`modbus_tcp`, `modbus_rtu`).
#[cfg(test)]
pub fn plan_modbus_writes(
    transport: &'static str,
    regs: &[ModbusRegisterMap],
    cmd: &DeviceCommand,
) -> Result<Vec<ModbusWrite>, TransportError> {
    Ok(plan_unit_writes(transport, regs, cmd)?
        .into_iter()
        .map(|(_, w)| w)
        .collect())
}

/// [`plan_modbus_writes`] with each write's slave: the register's own `unit_id`, or none for
/// the connection's.
pub fn plan_unit_writes(
    transport: &'static str,
    regs: &[ModbusRegisterMap],
    cmd: &DeviceCommand,
) -> Result<Vec<(Option<u8>, ModbusWrite)>, TransportError> {
    let command = cmd.name();
    let role = Role::of_command(cmd);
    let find = |names: &[&str]| command_register(transport, command, regs, role, names);
    let encode = |reg: &ModbusRegisterMap, value: f64| {
        encode_register(reg, value)
            .map(|w| (reg.unit_id.and_then(|u| u8::try_from(u).ok()), w))
            .map_err(|detail| TransportError::Send { transport, detail })
    };
    let one =
        |names: &[&str], value: f64| -> Result<Vec<(Option<u8>, ModbusWrite)>, TransportError> {
            Ok(vec![encode(find(names)?, value)?])
        };

    match cmd {
        DeviceCommand::Setpoint { value } => {
            // A setpoint register in °F gets the value in °F.
            let reg = find(SETPOINT_NAMES)?;
            let v = if in_fahrenheit(reg) {
                crate::transports::ingest::c_to_f(*value)
            } else {
                *value
            };
            Ok(vec![encode(reg, v)?])
        }
        DeviceCommand::Fan { percent } => one(FAN_NAMES, *percent),
        DeviceCommand::Heater { percent } => one(HEATER_NAMES, *percent),
        DeviceCommand::Drum { percent } => one(DRUM_NAMES, *percent),
        DeviceCommand::Damper { percent } => one(DAMPER_NAMES, *percent),
        DeviceCommand::Mode { mode } => {
            let v = match mode {
                ControlMode::Manual => 0.0,
                ControlMode::Auto => 1.0,
            };
            one(MODE_NAMES, v)
        }
        DeviceCommand::HeaterEnable { enabled } => {
            one(HEATER_ENABLE_NAMES, if *enabled { 1.0 } else { 0.0 })
        }
        DeviceCommand::EmergencyStop => one(ESTOP_NAMES, 1.0),
        DeviceCommand::Pid { kp, ki, kd, .. } => {
            // All three or none.
            let (rp, ri, rd) = (
                find(&["kp", "pid_kp", "p"])?,
                find(&["ki", "pid_ki", "i"])?,
                find(&["kd", "pid_kd", "d"])?,
            );
            Ok(vec![encode(rp, *kp)?, encode(ri, *ki)?, encode(rd, *kd)?])
        }
        DeviceCommand::Feedforward { .. }
        | DeviceCommand::Lease { .. }
        | DeviceCommand::RorLimit { .. } => Err(TransportError::Unsupported { transport, command }),
    }
}

/// Encode an engineering value for a register, applying the map's scale and
/// offset in reverse (`raw = (value - offset) / scale`).
pub fn encode_register(reg: &ModbusRegisterMap, value: f64) -> Result<ModbusWrite, String> {
    let address =
        u16::try_from(reg.address).map_err(|_| format!("address {} out of range", reg.address))?;
    let scale = reg.scale_factor.filter(|s| *s != 0.0).unwrap_or(1.0);
    let offset = reg.offset.unwrap_or(0.0);
    let raw = (value - offset) / scale;

    if reg.register_type == ModbusRegisterType::Coil {
        return Ok(ModbusWrite::Coil {
            address,
            on: raw != 0.0,
        });
    }

    let words = match reg.data_type {
        ModbusDataType::Uint16 | ModbusDataType::Bool => {
            let v = raw.round();
            if !(0.0..=65535.0).contains(&v) {
                return Err(format!("{v} out of range for uint16"));
            }
            vec![v as u16]
        }
        ModbusDataType::Int16 => {
            let v = raw.round();
            if !(-32768.0..=32767.0).contains(&v) {
                return Err(format!("{v} out of range for int16"));
            }
            vec![(v as i16) as u16]
        }
        ModbusDataType::Uint32 => {
            let v = raw.round();
            if !(0.0..=4_294_967_295.0).contains(&v) {
                return Err(format!("{v} out of range for uint32"));
            }
            let bits = v as u32;
            order_words(reg, [(bits >> 16) as u16, (bits & 0xFFFF) as u16])
        }
        ModbusDataType::Int32 => {
            let v = raw.round();
            if !(-2_147_483_648.0..=2_147_483_647.0).contains(&v) {
                return Err(format!("{v} out of range for int32"));
            }
            let bits = (v as i32) as u32;
            order_words(reg, [(bits >> 16) as u16, (bits & 0xFFFF) as u16])
        }
        ModbusDataType::Float32 => {
            let bits = (raw as f32).to_bits();
            order_words(reg, [(bits >> 16) as u16, (bits & 0xFFFF) as u16])
        }
        ModbusDataType::Bcd16 => {
            let v = raw.round();
            if !(0.0..=9999.0).contains(&v) {
                return Err(format!("{v} out of range for bcd16 (0 to 9999)"));
            }
            vec![to_bcd(v as u32) as u16]
        }
        ModbusDataType::Bcd32 => {
            let v = raw.round();
            if !(0.0..=99_999_999.0).contains(&v) {
                return Err(format!("{v} out of range for bcd32 (0 to 99999999)"));
            }
            let bits = to_bcd(v as u32);
            order_words(reg, [(bits >> 16) as u16, (bits & 0xFFFF) as u16])
        }
    };
    Ok(ModbusWrite::Holding { address, words })
}

/// A whole number as binary-coded decimal: 1234 → `0x1234`.
fn to_bcd(mut v: u32) -> u32 {
    let (mut out, mut shift) = (0u32, 0u32);
    while v > 0 && shift < 32 {
        out |= (v % 10) << shift;
        v /= 10;
        shift += 4;
    }
    out
}

/// Apply the register map's word order. `ABCD` (big-endian, high word first)
/// is the default; `CDAB` swaps the two words. Byte-swapped orders are rare on
/// roaster controllers and are treated as their word-order equivalents.
fn order_words(reg: &ModbusRegisterMap, words: [u16; 2]) -> Vec<u16> {
    match reg.byte_order.as_deref().map(str::to_ascii_uppercase) {
        Some(o) if o == "CDAB" || o == "DCBA" => vec![words[1], words[0]],
        _ => vec![words[0], words[1]],
    }
}

// ---------------------------------------------------------------------------
// Serial TC4 (lines executed by the task that owns the port)
// ---------------------------------------------------------------------------

pub struct SerialRequest {
    pub lines: Vec<String>,
    pub reply: oneshot::Sender<Result<(), String>>,
}

pub struct SerialSink {
    tx: mpsc::Sender<SerialRequest>,
    /// The output the connection's fan is on (`fan_output`).
    fan: crate::transports::tc4::FanOutput,
    /// The connection's templates (docs/ingest-mapping.md) and unit.
    commands: Option<Commands>,
    units: Units,
    /// The port, to name the connection in errors.
    port: String,
}

impl SerialSink {
    pub fn new(tx: mpsc::Sender<SerialRequest>, fan: crate::transports::tc4::FanOutput) -> Self {
        Self {
            tx,
            fan,
            commands: None,
            units: Units::C,
            port: String::new(),
        }
    }

    /// The connection's templates, unit and port.
    pub fn with_mapping(mut self, commands: Option<Commands>, units: Units, port: &str) -> Self {
        self.commands = commands;
        self.units = units;
        self.port = port.to_string();
        self
    }

    /// The lines for `cmd`: its template's payload split at `\n`, else the aArtisan lines (in
    /// the device's unit). Drum and damper have no aArtisan line: without a template they fail.
    pub fn lines(&self, cmd: &DeviceCommand) -> Result<Vec<String>, TransportError> {
        let cmd = in_device_units(cmd, self.units);
        if let Some(t) = self.commands.as_ref().and_then(|c| c.get(cmd.name())) {
            return Ok(crate::transports::ingest::render(t, &cmd)
                .split('\n')
                .map(|l| l.trim_end_matches('\r').to_string())
                .filter(|l| !l.is_empty())
                .collect());
        }
        if matches!(
            cmd,
            DeviceCommand::Drum { .. } | DeviceCommand::Damper { .. }
        ) {
            return Err(TransportError::NeedsTemplate {
                transport: "serial_tc4",
                command: cmd.name(),
                connection: if self.port.is_empty() {
                    "connection".into()
                } else {
                    format!("on {}", self.port)
                },
            });
        }
        crate::transports::tc4::command_lines(&cmd, self.fan)
    }
}

#[async_trait]
impl CommandSink for SerialSink {
    fn name(&self) -> &'static str {
        "serial_tc4"
    }

    async fn send(
        &self,
        _device_id: &str,
        cmd: &DeviceCommand,
        opts: SendOptions,
    ) -> Result<(), TransportError> {
        let lines = self.lines(cmd)?;
        if lines.is_empty() {
            return Ok(());
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SerialRequest {
                lines,
                reply: reply_tx,
            })
            .await
            .map_err(|_| TransportError::Send {
                transport: "serial_tc4",
                detail: "serial task stopped".into(),
            })?;
        match tokio::time::timeout(opts.timeout.max(Duration::from_secs(2)), reply_rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(e))) => Err(TransportError::Send {
                transport: "serial_tc4",
                detail: e,
            }),
            Ok(Err(_)) => Err(TransportError::Send {
                transport: "serial_tc4",
                detail: "serial task dropped request".into(),
            }),
            Err(_) => Err(TransportError::Timeout {
                transport: "serial_tc4",
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct LiveSink {
    id: u64,
    priority: i32,
    sink: Arc<dyn CommandSink>,
}

/// Handle returned by [`TransportRegistry::register`]; pass it back to
/// [`TransportRegistry::unregister`] when the connection goes away.
#[derive(Debug, Clone)]
pub struct SinkHandle {
    device_id: String,
    id: u64,
}

pub struct TransportRegistry {
    live: RwLock<HashMap<String, Vec<LiveSink>>>,
    next_id: AtomicU64,
    mqtt: Arc<MqttSink>,
    db: SqlitePool,
    setups: Arc<Setups>,
    /// The newest sample says whether the heater is on (the fan floor at the gate).
    telemetry: TelemetryService,
    /// When the gate last sent each roaster heater off or emergency stop.
    heater_off_sent: std::sync::Mutex<HashMap<String, Instant>>,
}

/// After heater off or STOP, samples this young may predate it: the heater counts as off.
const HEATER_OFF_GRACE: Duration = Duration::from_secs(5);
/// A sample older than this says nothing about the heater now.
const HEATER_SAMPLE_MAX_AGE_MS: u64 = 30_000;

impl TransportRegistry {
    pub fn new(
        mqtt: MqttService,
        mqtt_tx_total: IntCounter,
        db: SqlitePool,
        setups: Arc<Setups>,
        telemetry: TelemetryService,
        mqtt_routes: Arc<MqttRouter>,
    ) -> Arc<Self> {
        Arc::new(Self {
            live: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            mqtt: Arc::new(MqttSink::new(mqtt, mqtt_tx_total, mqtt_routes)),
            db,
            setups,
            telemetry,
            heater_off_sent: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// Everything the gate checks for a roaster: control level, outputs, fan floor.
    pub async fn limits(&self, device_id: &str) -> Limits {
        self.setups.limits(device_id).await
    }

    /// Whether the roaster's heater is on, for the fan floor: its newest sample's
    /// `heater_enable` (or heater output above 0 when it does not report that), unless the
    /// gate sent heater off or STOP in the last 5 s.
    fn heater_on(&self, device_id: &str) -> bool {
        if let Some(at) = self
            .heater_off_sent
            .lock()
            .ok()
            .and_then(|m| m.get(device_id).copied())
            && at.elapsed() < HEATER_OFF_GRACE {
                return false;
            }
        let Some(s) = self.telemetry.latest_sample(device_id) else {
            return false;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        if now.saturating_sub(s.ts_ms) > HEATER_SAMPLE_MAX_AGE_MS {
            return false;
        }
        heater_on_in(&s)
    }

    /// Each roaster's setup; its control level is the gate in [`Self::send`].
    pub fn setups(&self) -> &Arc<Setups> {
        &self.setups
    }

    /// Register a live sink. Higher `priority` wins when several are live;
    /// ties go to the most recently registered.
    pub async fn register(
        &self,
        device_id: &str,
        priority: i32,
        sink: Arc<dyn CommandSink>,
    ) -> SinkHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let name = sink.name();
        self.live
            .write()
            .await
            .entry(device_id.to_string())
            .or_default()
            .push(LiveSink { id, priority, sink });
        tracing::info!(device_id, transport = name, "control transport registered");
        SinkHandle {
            device_id: device_id.to_string(),
            id,
        }
    }

    pub async fn unregister(&self, handle: &SinkHandle) {
        let mut live = self.live.write().await;
        if let Some(sinks) = live.get_mut(&handle.device_id) {
            if let Some(pos) = sinks.iter().position(|s| s.id == handle.id) {
                let removed = sinks.remove(pos);
                tracing::info!(device_id = %handle.device_id, transport = removed.sink.name(), "control transport unregistered");
            }
            if sinks.is_empty() {
                live.remove(&handle.device_id);
            }
        }
    }

    /// Names of the live sinks for a device, best first.
    pub async fn live_transports(&self, device_id: &str) -> Vec<&'static str> {
        self.ordered_live(device_id)
            .await
            .iter()
            .map(|s| s.name())
            .collect()
    }

    async fn ordered_live(&self, device_id: &str) -> Vec<Arc<dyn CommandSink>> {
        let live = self.live.read().await;
        let mut sinks: Vec<&LiveSink> = live
            .get(device_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        sinks.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.id.cmp(&a.id)));
        sinks.into_iter().map(|s| s.sink.clone()).collect()
    }

    /// Deliver a command. Returns the name of the transport that carried it.
    pub async fn send(
        &self,
        device_id: &str,
        cmd: &DeviceCommand,
        opts: SendOptions,
    ) -> Result<&'static str, TransportError> {
        // The gate: nothing reaches a roaster its setup does not allow, and what does is
        // shaped to its outputs' ranges and fan floor (docs/roaster-setup.md).
        let limits = self.setups.limits(device_id).await;
        let heater_on = matches!(cmd, DeviceCommand::Fan { .. })
            && limits.fan_floor_pct > 0.0
            && self.heater_on(device_id);
        let shaped = match limits.shape(cmd, heater_on) {
            Ok(c) => c,
            Err(refusal) => {
                tracing::debug!(
                    device_id,
                    command = cmd.name(),
                    control = limits.control.as_str(),
                    ?refusal,
                    "command refused by the roaster's setup"
                );
                return Err(match refusal {
                    Refusal::Control(control) => TransportError::ControlOff {
                        device_id: device_id.to_string(),
                        control,
                        command: cmd.name(),
                    },
                    Refusal::NotDeclared(output) => TransportError::NotDeclared {
                        device_id: device_id.to_string(),
                        output,
                    },
                });
            }
        };
        if &shaped != cmd {
            tracing::debug!(device_id, asked = ?cmd, sent = ?shaped, "command shaped to the roaster's outputs");
        }
        let heater_off = match &shaped {
            DeviceCommand::HeaterEnable { enabled } => !enabled,
            DeviceCommand::EmergencyStop => true,
            DeviceCommand::Heater { percent } => *percent <= 0.0,
            _ => false,
        };
        if heater_off
            && let Ok(mut m) = self.heater_off_sent.lock() {
                m.insert(device_id.to_string(), Instant::now());
            }
        let cmd = &shaped;
        let live = self.ordered_live(device_id).await;
        let mut last_err: Option<TransportError> = None;
        for sink in &live {
            match sink.send(device_id, cmd, opts).await {
                Ok(()) => {
                    tracing::debug!(
                        device_id,
                        command = cmd.name(),
                        transport = sink.name(),
                        "command sent"
                    );
                    return Ok(sink.name());
                }
                Err(e) => {
                    tracing::warn!(device_id, command = cmd.name(), transport = sink.name(), error = %e, "command failed; trying next transport");
                    last_err = Some(e);
                }
            }
        }

        if self.mqtt_allowed(device_id).await {
            return self
                .mqtt
                .send(device_id, cmd, opts)
                .await
                .map(|()| self.mqtt.name());
        }

        Err(last_err.unwrap_or_else(|| TransportError::NoTransport {
            device_id: device_id.to_string(),
            detail: "device is configured for a direct transport that is not connected".into(),
        }))
    }

    /// MQTT is the fallback for devices configured for it, devices with no
    /// connection rows at all (created by hand before connections existed), and
    /// devices the registry has never heard of.
    async fn mqtt_allowed(&self, device_id: &str) -> bool {
        match crate::roasters::registry::get_device_by_device_id(&self.db, device_id).await {
            Ok(Some(dev)) => {
                dev.connections.is_empty()
                    || dev
                        .connections
                        .iter()
                        .any(|c| c.enabled && c.protocol == Protocol::Mqtt)
            }
            Ok(None) => true,
            Err(e) => {
                tracing::warn!(device_id, error = %e, "device lookup failed; falling back to MQTT");
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(
        name: &str,
        rt: ModbusRegisterType,
        dt: ModbusDataType,
        address: i32,
    ) -> ModbusRegisterMap {
        ModbusRegisterMap {
            id: String::new(),
            device_id: String::new(),
            register_type: rt,
            address,
            name: name.into(),
            data_type: dt,
            byte_order: None,
            scale_factor: None,
            offset: None,
            unit: None,
            description: None,
            writable: true,
            role: None,
            unit_id: None,
        }
    }

    #[test]
    fn a_register_marked_read_only_is_never_written() {
        // The heater's read-back is named "heater" but not writable: no write goes to it.
        let mut readback = reg(
            "heater",
            ModbusRegisterType::Holding,
            ModbusDataType::Uint16,
            6,
        );
        readback.writable = false;
        let cmd = DeviceCommand::Heater { percent: 100.0 };
        // Alone, it fails and says why, naming the register.
        match plan_modbus_writes("modbus_rtu", std::slice::from_ref(&readback), &cmd) {
            Err(TransportError::Send { transport, detail }) => {
                assert_eq!(transport, "modbus_rtu");
                assert!(detail.contains("'heater' (holding register 6)"), "{detail}");
                assert!(detail.contains("read-only"), "{detail}");
            }
            other => panic!("expected a read-only error, got {other:?}"),
        }
        // A read-only register earlier in the role's names does not stop a writable one
        // later in them from being used ("fan" read-back, "air" command), and the read-only
        // one is still never written.
        let mut fan_readback = reg(
            "fan",
            ModbusRegisterType::Holding,
            ModbusDataType::Uint16,
            7,
        );
        fan_readback.writable = false;
        let air = reg(
            "air",
            ModbusRegisterType::Holding,
            ModbusDataType::Uint16,
            9,
        );
        let w = plan_modbus_writes(
            "modbus_tcp",
            &[fan_readback, air],
            &DeviceCommand::Fan { percent: 60.0 },
        )
        .unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 9,
                words: vec![60]
            }]
        );
        // With a writable command register beside it, that one is used.
        let command = reg(
            "heater_cmd",
            ModbusRegisterType::Holding,
            ModbusDataType::Uint16,
            3,
        );
        let w = plan_modbus_writes("modbus_tcp", &[readback, command], &cmd).unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 3,
                words: vec![100]
            }]
        );
    }

    #[test]
    fn float32_setpoint_is_two_words_big_endian() {
        let regs = vec![reg(
            "setpoint",
            ModbusRegisterType::Holding,
            ModbusDataType::Float32,
            0,
        )];
        let w = plan_modbus_writes(
            "modbus_tcp",
            &regs,
            &DeviceCommand::Setpoint { value: 185.5 },
        )
        .unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 0,
                words: vec![0x4339, 0x8000]
            }]
        );
    }

    #[test]
    fn scaled_int16_setpoint_uses_artisan_x10_convention() {
        let mut r = reg("SV", ModbusRegisterType::Holding, ModbusDataType::Int16, 0);
        r.scale_factor = Some(0.1);
        let w = plan_modbus_writes(
            "modbus_tcp",
            &[r],
            &DeviceCommand::Setpoint { value: 200.4 },
        )
        .unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 0,
                words: vec![2004]
            }]
        );
    }

    #[test]
    fn fan_percent_goes_out_through_the_register_scale() {
        let mut r = reg(
            "fan_cmd",
            ModbusRegisterType::Holding,
            ModbusDataType::Uint16,
            2,
        );
        r.unit = Some("%".into());
        let w = plan_modbus_writes(
            "modbus_tcp",
            std::slice::from_ref(&r),
            &DeviceCommand::Fan { percent: 75.0 },
        )
        .unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 2,
                words: vec![75]
            }]
        );
        // A 0-255 PWM register: scale_factor 100/255 maps raw to %.
        r.scale_factor = Some(100.0 / 255.0);
        let w =
            plan_modbus_writes("modbus_tcp", &[r], &DeviceCommand::Fan { percent: 100.0 }).unwrap();
        assert_eq!(
            w,
            vec![ModbusWrite::Holding {
                address: 2,
                words: vec![255]
            }]
        );
    }

    #[test]
    fn coil_estop_and_mode() {
        let regs = vec![
            reg("e_stop", ModbusRegisterType::Coil, ModbusDataType::Bool, 2),
            reg(
                "pid_mode",
                ModbusRegisterType::Holding,
                ModbusDataType::Uint16,
                3,
            ),
        ];
        assert_eq!(
            plan_modbus_writes("modbus_tcp", &regs, &DeviceCommand::EmergencyStop).unwrap(),
            vec![ModbusWrite::Coil {
                address: 2,
                on: true
            }]
        );
        assert_eq!(
            plan_modbus_writes(
                "modbus_tcp",
                &regs,
                &DeviceCommand::Mode {
                    mode: ControlMode::Auto
                }
            )
            .unwrap(),
            vec![ModbusWrite::Holding {
                address: 3,
                words: vec![1]
            }]
        );
    }

    #[test]
    fn input_registers_are_never_written_and_autotune_is_unsupported() {
        let regs = vec![reg(
            "setpoint",
            ModbusRegisterType::Input,
            ModbusDataType::Float32,
            0,
        )];
        assert!(matches!(
            plan_modbus_writes("modbus_tcp", &regs, &DeviceCommand::Setpoint { value: 1.0 }),
            Err(TransportError::Send { detail, .. }) if detail.contains("read-only")
        ));
        // A map with no register for the role cannot carry the command at all.
        assert!(matches!(
            plan_modbus_writes("modbus_tcp", &regs, &DeviceCommand::Fan { percent: 50.0 }),
            Err(TransportError::Unsupported { .. })
        ));
        assert!(matches!(
            plan_modbus_writes("modbus_tcp", &regs, &DeviceCommand::Lease { ms: 10_000 }),
            Err(TransportError::Unsupported { .. })
        ));
    }

    #[test]
    fn websocket_frame_matches_guide() {
        let f = WebSocketSink::frame("r1", &DeviceCommand::Setpoint { value: 200.0 });
        let v: serde_json::Value = serde_json::from_str(&f).unwrap();
        assert_eq!(v["type"], "control");
        assert_eq!(v["topic"], "roaster/r1/control/setpoint");
        assert_eq!(v["payload"], "200");
    }

    fn with_role(mut r: ModbusRegisterMap, role: &str) -> ModbusRegisterMap {
        r.role = Some(role.into());
        r
    }

    /// docs/ingest-mapping.md, "Modbus TCP and RTU": the writable register with the command's
    /// role first, then names among the registers with no role; °F setpoints in °F.
    #[test]
    fn modbus_writes_follow_roles_then_names() {
        use ModbusDataType::*;
        use ModbusRegisterType::*;
        // A register named "fan" with the role damper never takes the fan's command.
        let regs = vec![
            with_role(reg("fan", Holding, Uint16, 1), "damper"),
            with_role(reg("OUT_A", Holding, Uint16, 2), "fan"),
            with_role(reg("SPEED", Holding, Uint16, 3), "drum"),
            reg("heater_cmd", Holding, Uint16, 4),
        ];
        let w = |cmd: DeviceCommand| plan_modbus_writes("modbus_tcp", &regs, &cmd);
        let at = |a: u16, v: u16| {
            vec![ModbusWrite::Holding {
                address: a,
                words: vec![v],
            }]
        };
        assert_eq!(w(DeviceCommand::Fan { percent: 60.0 }).unwrap(), at(2, 60));
        assert_eq!(
            w(DeviceCommand::Damper { percent: 25.0 }).unwrap(),
            at(1, 25)
        );
        assert_eq!(w(DeviceCommand::Drum { percent: 55.0 }).unwrap(), at(3, 55));
        assert_eq!(
            w(DeviceCommand::Heater { percent: 40.0 }).unwrap(),
            at(4, 40)
        );
        // Drum and damper by name too.
        let named = vec![
            reg("drum_speed", Holding, Uint16, 9),
            reg("damper", Coil, Bool, 0),
        ];
        assert_eq!(
            plan_modbus_writes("modbus_rtu", &named, &DeviceCommand::Drum { percent: 70.0 })
                .unwrap(),
            at(9, 70)
        );
        assert!(matches!(
            plan_modbus_writes("modbus_rtu", &[], &DeviceCommand::Damper { percent: 1.0 }),
            Err(TransportError::Unsupported {
                command: "damper",
                ..
            })
        ));
        // A role on a read-only register only: refused, naming it.
        let mut ro = with_role(reg("PV_OUT", Holding, Uint16, 5), "heater");
        ro.writable = false;
        match plan_modbus_writes("modbus_tcp", &[ro], &DeviceCommand::Heater { percent: 1.0 }) {
            Err(TransportError::Send { detail, .. }) => assert!(detail.contains("PV_OUT")),
            other => panic!("{other:?}"),
        }
        // A setpoint register in °F gets °F.
        let mut sv = with_role(reg("SV", Holding, Int16, 0x1001), "setpoint");
        sv.unit = Some("°F".into());
        assert_eq!(
            plan_modbus_writes(
                "modbus_rtu",
                &[sv],
                &DeviceCommand::Setpoint { value: 200.0 }
            )
            .unwrap(),
            at(0x1001, 392)
        );
    }

    #[test]
    fn websocket_templates_are_the_whole_frame_and_units_follow_the_device() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cmds = crate::transports::ingest::parse_commands(
            &serde_json::json!({ "heater": { "payload": "{\"command\":\"setBurner\",\"value\":{value}}", "decimals": 0 } }),
            crate::transports::ingest::Kind::WebSocket,
        )
        .unwrap();
        let sink = WebSocketSink::new("websocket", tx).with_mapping(Some(cmds), Units::F);
        assert_eq!(
            sink.frame_for("r1", &DeviceCommand::Heater { percent: 61.6 }),
            "{\"command\":\"setBurner\",\"value\":62}"
        );
        // No template: today's frame, the setpoint in °F; drum on control/drum.
        let v: serde_json::Value =
            serde_json::from_str(&sink.frame_for("r1", &DeviceCommand::Setpoint { value: 100.0 }))
                .unwrap();
        assert_eq!(v["payload"], "212");
        let v: serde_json::Value =
            serde_json::from_str(&sink.frame_for("r1", &DeviceCommand::Drum { percent: 45.0 }))
                .unwrap();
        assert_eq!(v["topic"], "roaster/r1/control/drum");
        assert_eq!(v["payload"], "45");
        // Artisan's protocol numbers each request: `{id}` counts up.
        let (tx, _rx) = mpsc::unbounded_channel();
        let cmds = crate::transports::ingest::parse_commands(
            &serde_json::json!({ "fan": { "payload": "{\"id\":{id},\"fan\":{value}}" } }),
            crate::transports::ingest::Kind::WebSocket,
        )
        .unwrap();
        let sink = WebSocketSink::new("websocket", tx).with_mapping(Some(cmds), Units::C);
        let fan = DeviceCommand::Fan { percent: 70.0 };
        assert_eq!(sink.frame_for("r1", &fan), "{\"id\":1000000,\"fan\":70}");
        assert_eq!(sink.frame_for("r1", &fan), "{\"id\":1000001,\"fan\":70}");
    }

    #[test]
    fn a_tc4_needs_a_template_for_drum_and_damper() {
        let (tx, _rx) = mpsc::channel(1);
        let plain = SerialSink::new(tx.clone(), crate::transports::tc4::FanOutput::Io3)
            .with_mapping(None, Units::C, "/dev/ttyUSB0");
        let e = plain
            .lines(&DeviceCommand::Drum { percent: 50.0 })
            .unwrap_err();
        assert!(matches!(
            e,
            TransportError::NeedsTemplate {
                command: "drum",
                ..
            }
        ));
        assert!(e.to_string().contains("/dev/ttyUSB0"), "{e}");
        assert!(e.to_string().contains("commands.drum"), "{e}");
        let cmds = crate::transports::ingest::parse_commands(
            &serde_json::json!({ "drum": { "payload": "OT2;{value}", "decimals": 0 },
                "damper": { "payload": "IO3;{value}\nACK" } }),
            crate::transports::ingest::Kind::Tc4,
        )
        .unwrap();
        let sink = SerialSink::new(tx, crate::transports::tc4::FanOutput::Io3).with_mapping(
            Some(cmds),
            Units::F,
            "COM3",
        );
        assert_eq!(
            sink.lines(&DeviceCommand::Drum { percent: 49.6 }).unwrap(),
            vec!["OT2;50"]
        );
        assert_eq!(
            sink.lines(&DeviceCommand::Damper { percent: 20.0 })
                .unwrap(),
            vec!["IO3;20", "ACK"]
        );
        // No template: the aArtisan line, the setpoint in °F.
        assert_eq!(
            sink.lines(&DeviceCommand::Setpoint { value: 200.0 })
                .unwrap(),
            vec!["PID;SV;392.0"]
        );
        assert_eq!(
            sink.lines(&DeviceCommand::Heater { percent: 30.0 })
                .unwrap(),
            vec!["OT1;30"]
        );
    }
}
