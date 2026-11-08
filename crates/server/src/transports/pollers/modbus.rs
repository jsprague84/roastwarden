//! The Modbus master: a connection polled over TCP or RTU (serial), its register map
//! planned into block reads, each register decoded, writes carried to the device.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_modbus::prelude::*;
use tokio_serial::SerialPortBuilderExt;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::types::{
    DeviceConnection, DeviceStatus, ModbusDataType, ModbusRegisterMap, ModbusRegisterType, Protocol,
};
use crate::transports::ingest::Role;
use crate::transports::transport::{
    in_fahrenheit, is_writable, role_of, ModbusSink, ModbusWrite, ModbusWriteRequest,
};
use crate::transports::types::{
    ModbusFraming, ModbusRtuConnectionConfig, ModbusTcpConnectionConfig, SerialParity,
};

use super::validate::check_connection_config;
use super::{Tracked, MIN_POLL_MS};

/// Where a Modbus device is and how a request reaches it.
pub(super) enum ModbusLink {
    Tcp(ModbusTcpConnectionConfig),
    Rtu(ModbusRtuConnectionConfig),
}

impl ModbusLink {
    fn from_config(protocol: &Protocol, config: &serde_json::Value) -> Result<Self, String> {
        match protocol {
            Protocol::ModbusTcp => serde_json::from_value(config.clone())
                .map(ModbusLink::Tcp)
                .map_err(|e| format!("invalid Modbus TCP config: {e}")),
            Protocol::ModbusRtu => serde_json::from_value(config.clone())
                .map(ModbusLink::Rtu)
                .map_err(|e| format!("invalid Modbus RTU config: {e}")),
            _ => Err(format!("{protocol} is not a Modbus connection")),
        }
    }

    /// The transport's name, as the registry and the logs show it.
    pub(super) fn name(&self) -> &'static str {
        match self {
            ModbusLink::Tcp(_) => "modbus_tcp",
            ModbusLink::Rtu(_) => "modbus_rtu",
        }
    }

    pub(super) fn describe(&self) -> String {
        match self {
            ModbusLink::Tcp(c) => format!("{}:{} unit {}", c.host, c.port, c.unit_id),
            ModbusLink::Rtu(c) => format!(
                "{}{} {} {}{}{} unit {}",
                c.port,
                if c.framing == ModbusFraming::Ascii {
                    " ASCII"
                } else {
                    ""
                },
                c.baud,
                c.data_bits,
                match c.parity {
                    SerialParity::None => 'N',
                    SerialParity::Even => 'E',
                    SerialParity::Odd => 'O',
                },
                c.stop_bits,
                c.unit_id
            ),
        }
    }

    /// The connection's slave address: a register with no `unit_id` of its own is on it.
    fn unit(&self) -> u8 {
        match self {
            ModbusLink::Tcp(c) => c.unit_id,
            ModbusLink::Rtu(c) => c.unit_id,
        }
    }

    pub(super) fn poll_interval(&self) -> Duration {
        let ms = match self {
            ModbusLink::Tcp(c) => c.poll_interval_ms,
            ModbusLink::Rtu(c) => c.poll_interval_ms,
        };
        Duration::from_millis(ms.max(MIN_POLL_MS))
    }

    /// How long one request waits for its answer.
    fn timeout(&self) -> Duration {
        let ms = match self {
            ModbusLink::Tcp(c) => c.timeout_ms,
            ModbusLink::Rtu(c) => c.timeout_ms,
        };
        Duration::from_millis(ms.clamp(50, 10_000))
    }

    /// Open the link: a TCP connection (the host resolved each time, so a DHCP name follows
    /// the device) or the serial port with the configured framing.
    async fn connect(&self) -> std::io::Result<tokio_modbus::client::Context> {
        match self {
            ModbusLink::Tcp(c) => {
                let addr: SocketAddr = tokio::net::lookup_host((c.host.as_str(), c.port))
                    .await?
                    .next()
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("{} has no address", c.host),
                        )
                    })?;
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio_modbus::client::tcp::connect_slave(addr, Slave(c.unit_id)),
                )
                .await
                {
                    Ok(ctx) => ctx,
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("no answer from {addr} in 5 s"),
                    )),
                }
            }
            ModbusLink::Rtu(c) => {
                let port = rtu_port_builder(c)?.open_native_async()?;
                Ok(match c.framing {
                    ModbusFraming::Rtu => {
                        tokio_modbus::client::rtu::attach_slave(port, Slave(c.unit_id))
                    }
                    ModbusFraming::Ascii => {
                        crate::transports::modbus_ascii::attach_slave(port, Slave(c.unit_id))
                    }
                })
            }
        }
    }
}

/// The serial port for a Modbus RTU connection: baud, data bits, parity, stop bits.
pub(super) fn rtu_port_builder(
    c: &ModbusRtuConnectionConfig,
) -> std::io::Result<tokio_serial::SerialPortBuilder> {
    let invalid = |m: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, m);
    let data_bits = match c.data_bits {
        5 => tokio_serial::DataBits::Five,
        6 => tokio_serial::DataBits::Six,
        7 => tokio_serial::DataBits::Seven,
        8 => tokio_serial::DataBits::Eight,
        n => return Err(invalid(format!("{n} data bits (5 to 8)"))),
    };
    let stop_bits = match c.stop_bits {
        1 => tokio_serial::StopBits::One,
        2 => tokio_serial::StopBits::Two,
        n => return Err(invalid(format!("{n} stop bits (1 or 2)"))),
    };
    let parity = match c.parity {
        SerialParity::None => tokio_serial::Parity::None,
        SerialParity::Even => tokio_serial::Parity::Even,
        SerialParity::Odd => tokio_serial::Parity::Odd,
    };
    Ok(tokio_serial::new(&c.port, c.baud)
        .data_bits(data_bits)
        .stop_bits(stop_bits)
        .parity(parity))
}

pub(super) async fn modbus_poller_loop(
    device_id: String,
    conn: DeviceConnection,
    register_map: Vec<ModbusRegisterMap>,
    telemetry_service: TelemetryService,
    transports: Tracked,
) {
    let link = match ModbusLink::from_config(&conn.protocol, &conn.config) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(%device_id, error = %e, "Modbus poller not started");
            return;
        }
    };
    let name = link.name();
    let poll_interval = link.poll_interval();
    let timeout = link.timeout();
    let unit = link.unit();
    let register_map = if register_map.is_empty() {
        tracing::warn!(%device_id, "No register map found, using standard Roastwarden layout");
        default_register_map()
    } else {
        register_map
    };
    let plan = plan_reads(&register_map);

    tracing::info!(%device_id, transport = name, link = %link.describe(), ?poll_interval,
        registers = register_map.len(), reads = plan.len(), "Starting Modbus poller");

    let mut backoff = Duration::from_secs(1);

    loop {
        match link.connect().await {
            Ok(mut ctx) => {
                tracing::info!(%device_id, transport = name, "Modbus connected");
                backoff = Duration::from_secs(1);

                // Control path: commands arrive as register writes on this channel
                // and are executed on the same connection between polls.
                let (write_tx, mut write_rx) = mpsc::channel::<ModbusWriteRequest>(16);
                let sink = Arc::new(ModbusSink::new(name, register_map.clone(), write_tx));
                let handle = transports.register(&device_id, conn.priority, sink).await;

                let mut refused: Vec<String> = Vec::new();
                let mut next_poll = tokio::time::Instant::now();
                loop {
                    tokio::select! {
                        () = tokio::time::sleep_until(next_poll) => {
                            next_poll = tokio::time::Instant::now() + poll_interval;
                            match poll_modbus_registers(&mut ctx, &plan, &register_map, timeout, unit).await {
                                Ok(poll) => {
                                    if poll.refused != refused {
                                        if !poll.refused.is_empty() {
                                            tracing::warn!(%device_id, refused = ?poll.refused,
                                                "Modbus device refused part of the register map");
                                        }
                                        refused = poll.refused;
                                    }
                                    telemetry_service.raw.record(&device_id, Some(&conn.id),
                                        &conn.protocol.to_string(), None, poll.raw.to_string().as_bytes());
                                    telemetry_service
                                        .process_telemetry(&device_id, &poll.payload, Some(&DeviceStatus::Active))
                                        .await;
                                }
                                Err(e) => {
                                    tracing::warn!(%device_id, error = %e, "Modbus read failed, reconnecting");
                                    break;
                                }
                            }
                        }
                        Some(req) = write_rx.recv() => {
                            let result = write_on(&mut ctx, req.unit, unit, &req.write, timeout).await;
                            let failed_io = matches!(&result, Err(e) if e.starts_with("io:"));
                            let _ = req.reply.send(result);
                            if failed_io {
                                tracing::warn!(%device_id, "Modbus write failed at transport level, reconnecting");
                                break;
                            }
                        }
                    }
                }

                transports.unregister(&handle).await;
            }
            Err(e) => {
                tracing::warn!(%device_id, transport = name, error = %e,
                    backoff_secs = backoff.as_secs(), "Modbus connect failed, retrying");
            }
        }

        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

/// One register write on the register's own slave (`unit`), or the connection's (`default`);
/// the context is left addressed to the connection's.
pub(super) async fn write_on(
    ctx: &mut tokio_modbus::client::Context,
    unit: Option<u8>,
    default: u8,
    write: &ModbusWrite,
    timeout: Duration,
) -> Result<(), String> {
    ctx.set_slave(Slave(unit.unwrap_or(default)));
    let result = execute_modbus_write(ctx, write, timeout).await;
    ctx.set_slave(Slave(default));
    result
}

/// Execute one register write. Errors prefixed `io:` mean the connection is
/// unusable (a timeout too: on a serial line a late answer would be read as the next one's);
/// others are Modbus exceptions from the device.
pub(super) async fn execute_modbus_write(
    ctx: &mut tokio_modbus::client::Context,
    write: &ModbusWrite,
    timeout: Duration,
) -> Result<(), String> {
    let outcome = tokio::time::timeout(timeout, async {
        match write {
            ModbusWrite::Holding { address, words } if words.len() == 1 => {
                ctx.write_single_register(*address, words[0]).await
            }
            ModbusWrite::Holding { address, words } => {
                ctx.write_multiple_registers(*address, words).await
            }
            ModbusWrite::Coil { address, on } => ctx.write_single_coil(*address, *on).await,
        }
    })
    .await;
    match outcome {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(exception))) => Err(format!("Modbus exception: {exception:?}")),
        Ok(Err(e)) => Err(format!("io: {e}")),
        Err(_) => Err(format!("io: no answer in {} ms", timeout.as_millis())),
    }
}

/// Try a Modbus connection's settings before they are saved: open the link, read the register
/// map once (the standard layout when there is none yet) and say what came back.
pub(crate) async fn probe_modbus(
    protocol: &Protocol,
    config: &serde_json::Value,
    map: Vec<ModbusRegisterMap>,
) -> Result<String, String> {
    check_connection_config(protocol, config)?;
    let link = ModbusLink::from_config(protocol, config)?;
    let map = if map.is_empty() {
        default_register_map()
    } else {
        map
    };
    let mut ctx = link
        .connect()
        .await
        .map_err(|e| format!("could not open {}: {e}", link.describe()))?;
    let poll = poll_modbus_registers(
        &mut ctx,
        &plan_reads(&map),
        &map,
        link.timeout(),
        link.unit(),
    )
    .await
    .map_err(|e| format!("{} opened, but the read failed: {e}", link.describe()))?;
    let mut values: Vec<String> = poll
        .payload
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| k.as_str() != "timestamp")
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    values.sort();
    let more = values.len().saturating_sub(8);
    values.truncate(8);
    Ok(format!(
        "{} answered: {}{}{}",
        link.describe(),
        values.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        },
        if poll.refused.is_empty() {
            String::new()
        } else {
            format!(" (refused: {})", poll.refused.join("; "))
        }
    ))
}

/// One poll's telemetry, and the reads the device answered with an exception.
pub(super) struct Poll {
    pub(super) payload: serde_json::Value,
    /// Each register read, decoded by its type before scale and role (the raw message,
    /// docs/add-roaster.md): `[{kind, address, name, type, value}]`
    pub(super) raw: serde_json::Value,
    pub(super) refused: Vec<String>,
}

/// Read every register in the plan and build a JSON telemetry payload. A read the device
/// answers with an exception (an address it does not have) leaves its registers out and the
/// rest still go through; when every read is refused, or the link fails or times out, the
/// poll fails and the connection is reopened.
pub(super) async fn poll_modbus_registers(
    ctx: &mut tokio_modbus::client::Context,
    plan: &[ReadRun],
    register_map: &[ModbusRegisterMap],
    timeout: Duration,
    unit: u8,
) -> Result<Poll, Box<dyn std::error::Error + Send + Sync>> {
    let mut words: HashMap<(Option<u8>, ModbusRegisterType, usize), u16> = HashMap::new();
    let mut refused = Vec::new();
    for run in plan {
        ctx.set_slave(Slave(run.unit.unwrap_or(unit)));
        let read = tokio::time::timeout(timeout, async {
            let (start, count) = (run.start, run.count);
            match run.kind {
                ModbusRegisterType::Input => ctx.read_input_registers(start, count).await,
                ModbusRegisterType::Holding => ctx.read_holding_registers(start, count).await,
                ModbusRegisterType::Coil => ctx
                    .read_coils(start, count)
                    .await
                    .map(|r| r.map(|bits| bits.into_iter().map(u16::from).collect::<Vec<u16>>())),
                ModbusRegisterType::Discrete => ctx
                    .read_discrete_inputs(start, count)
                    .await
                    .map(|r| r.map(|bits| bits.into_iter().map(u16::from).collect::<Vec<u16>>())),
            }
        })
        .await
        .map_err(|_| format!("no answer in {} ms", timeout.as_millis()))??;
        match read {
            Ok(data) => {
                let addrs = run.start as usize..run.start as usize + run.count as usize;
                words.extend(addrs.zip(data).map(|(a, w)| ((run.unit, run.kind, a), w)));
            }
            Err(exception) => refused.push(format!(
                "{}{} {}+{}: {exception:?}",
                run.unit.map(|u| format!("slave {u} ")).unwrap_or_default(),
                run.kind,
                run.start,
                run.count
            )),
        }
    }
    ctx.set_slave(Slave(unit));
    if !plan.is_empty() && refused.len() == plan.len() {
        return Err(format!("every read refused: {}", refused.join("; ")).into());
    }

    let mut payload = serde_json::Map::new();
    let data_of = |r: &ModbusRegisterMap| -> HashMap<usize, u16> {
        let (unit, kind) = (unit_of(r), r.register_type);
        words
            .iter()
            .filter(|((u, k, _), _)| *u == unit && *k == kind)
            .map(|((_, _, a), w)| (*a, *w))
            .collect()
    };
    for kind in KINDS {
        for r in register_map
            .iter()
            .filter(|r| r.register_type == kind && role_of(r).is_none())
        {
            decode_registers(&[r], &data_of(r), &mut payload);
        }
    }

    let raw: Vec<serde_json::Value> = register_map
        .iter()
        .filter_map(|r| {
            decoded(r, &data_of(r)).map(|v| {
                let mut one = json!({
                    "kind": r.register_type,
                    "address": r.address,
                    "name": r.name,
                    "type": r.data_type,
                    "value": v,
                });
                if let Some(u) = unit_of(r) {
                    one["unit_id"] = json!(u);
                }
                one
            })
        })
        .collect();

    // Map register names to ESP32/frontend camelCase field names
    normalize_field_names(&mut payload);

    // Registers with a role are their role's field, over any name (docs/ingest-mapping.md).
    // A role's writable register first, so its read-only one (the device's reading) wins.
    let mut roled: Vec<&ModbusRegisterMap> = register_map
        .iter()
        .filter(|r| role_of(r).is_some())
        .collect();
    roled.sort_by_key(|r| !is_writable(r));
    for r in roled {
        decode_registers(&[r], &data_of(r), &mut payload);
    }

    // Add timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    payload.insert("timestamp".to_string(), json!(now));

    Ok(Poll {
        payload: serde_json::Value::Object(payload),
        raw: serde_json::Value::Array(raw),
        refused,
    })
}

pub(super) const KINDS: [ModbusRegisterType; 4] = [
    ModbusRegisterType::Input,
    ModbusRegisterType::Holding,
    ModbusRegisterType::Coil,
    ModbusRegisterType::Discrete,
];

/// The most registers one Modbus read may ask for (function codes 3 and 4).
pub(super) const MAX_REGISTER_READ: usize = 125;
/// The most coils or discrete inputs one read may ask for (function codes 1 and 2).
const MAX_BIT_READ: usize = 2000;

/// One read request: `count` registers (or bits) of one kind from `start`, on the register's
/// own slave (`unit`) or, with none, the connection's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadRun {
    pub unit: Option<u8>,
    pub kind: ModbusRegisterType,
    pub start: u16,
    pub count: u16,
}

/// A register's own slave, when it names one (1 to 247; the check refuses anything else).
pub(crate) fn unit_of(r: &ModbusRegisterMap) -> Option<u8> {
    r.unit_id.and_then(|u| u8::try_from(u).ok())
}

pub(super) fn is_bits(kind: ModbusRegisterType) -> bool {
    matches!(
        kind,
        ModbusRegisterType::Coil | ModbusRegisterType::Discrete
    )
}

/// How many addresses a register takes: two for a 32-bit value in a register, one otherwise
/// (a coil or discrete input is one bit whatever its data type).
fn width(r: &ModbusRegisterMap) -> usize {
    if is_bits(r.register_type) {
        1
    } else {
        r.data_type.words()
    }
}

/// The reads that cover a register map: for each slave (the connection's first, then each
/// register's own in order) and kind (input, holding, coil, discrete), runs of addresses next
/// to or overlapping each other, each read from its first address and split at the protocol's
/// limit (125 registers, 2000 bits). A gap starts a new read, so no read asks for an address
/// the map does not name (devices answer those with an exception).
pub(crate) fn plan_reads(map: &[ModbusRegisterMap]) -> Vec<ReadRun> {
    let mut units: Vec<Option<u8>> = map.iter().map(unit_of).collect();
    units.sort_unstable();
    units.dedup();
    let mut out = Vec::new();
    for unit in units {
        for kind in KINDS {
            let regs: Vec<&ModbusRegisterMap> = map
                .iter()
                .filter(|r| r.register_type == kind && unit_of(r) == unit)
                .collect();
            let max = if is_bits(kind) {
                MAX_BIT_READ
            } else {
                MAX_REGISTER_READ
            };
            out.extend(
                read_blocks(&regs, max)
                    .into_iter()
                    .map(|(start, count)| ReadRun {
                        unit,
                        kind,
                        start,
                        count,
                    }),
            );
        }
    }
    out
}

/// The `(start, count)` reads for registers of one kind, at most `max` each.
pub(super) fn read_blocks(regs: &[&ModbusRegisterMap], max: usize) -> Vec<(u16, u16)> {
    let mut spans: Vec<(usize, usize)> = regs
        .iter()
        .filter(|r| (0..=u16::MAX as i32).contains(&r.address))
        .map(|r| {
            let start = r.address as usize;
            (start, (start + width(r)).min(u16::MAX as usize + 1))
        })
        .collect();
    spans.sort_unstable();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        match runs.last_mut() {
            Some(run) if start <= run.1 => run.1 = run.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    let mut blocks = Vec::new();
    for (mut start, end) in runs {
        while start < end {
            let count = (end - start).min(max);
            blocks.push((start as u16, count as u16));
            start += count;
        }
    }
    blocks
}
/// Map Modbus register names (`snake_case`) to the camelCase telemetry field names
/// that the frontend expects (matching ESP32 JSON output).
pub(super) fn normalize_field_names(payload: &mut serde_json::Map<String, serde_json::Value>) {
    let mappings: &[(&str, &str)] = &[
        ("bean_temp", "beanTemp"),
        ("env_temp", "envTemp"),
        ("rate_of_rise", "rateOfRise"),
        ("control_mode", "controlMode"),
        ("heater_enable", "heaterEnable"),
        ("system_status", "systemStatus"),
        ("free_heap", "freeHeap"),
        ("emergency_stop", "emergencyStop"),
        // Artisan Compatible preset names
        ("BT", "beanTemp"),
        ("ET", "envTemp"),
        ("BT_RoR", "rateOfRise"),
        ("heater_duty", "heater"),
        ("fan_duty", "fan"),
        ("SV", "setpoint"),
        ("pid_mode", "controlMode"),
    ];

    for &(snake, camel) in mappings {
        if let Some(val) = payload.remove(snake) {
            payload.insert(camel.to_string(), val);
        }
    }
}

/// One register's value by its type and word order, before scale, offset and role.
fn decoded(reg: &ModbusRegisterMap, data: &HashMap<usize, u16>) -> Option<f64> {
    let addr = reg.address as usize;
    let one = data.get(&addr).copied();
    let pair = || {
        let (first, second) = (data.get(&addr)?, data.get(&(addr + 1))?);
        let (hi, lo) = word_pair(reg, *first, *second);
        Some(((hi as u32) << 16) | (lo as u32))
    };
    match reg.data_type {
        ModbusDataType::Uint16 => one.map(|v| v as f64),
        ModbusDataType::Int16 => one.map(|v| v as i16 as f64),
        ModbusDataType::Float32 => pair()
            .map(|w| f32::from_bits(w) as f64)
            .filter(|v| v.is_finite()),
        ModbusDataType::Uint32 => pair().map(|w| w as f64),
        ModbusDataType::Int32 => pair().map(|w| w as i32 as f64),
        ModbusDataType::Bool => one.map(|v| if v != 0 { 1.0 } else { 0.0 }),
        ModbusDataType::Bcd16 => one.and_then(|v| from_bcd(v as u32)).map(|v| v as f64),
        ModbusDataType::Bcd32 => pair().and_then(from_bcd).map(|v| v as f64),
    }
}

/// A binary-coded decimal's value (each four bits one digit, `0x1234` → 1234); `None` when a
/// digit is above 9 (a bad read, not a number).
pub(crate) fn from_bcd(mut v: u32) -> Option<u32> {
    let (mut out, mut place) = (0u32, 1u32);
    while v > 0 {
        let d = v & 0xF;
        if d > 9 {
            return None;
        }
        out += d * place;
        place = place.saturating_mul(10);
        v >>= 4;
    }
    Some(out)
}

/// Decode raw register data into named values using the register map.
pub(super) fn decode_registers(
    registers: &[&ModbusRegisterMap],
    data: &HashMap<usize, u16>,
    payload: &mut serde_json::Map<String, serde_json::Value>,
) {
    for reg in registers {
        if let Some(mut val) = decoded(reg, data) {
            // Apply scale factor and offset
            if let Some(scale) = reg.scale_factor {
                val *= scale;
            }
            if let Some(offset) = reg.offset {
                val += offset;
            }
            // A register with a role is its field; a temperature role in °F is converted.
            let role = role_of(reg);
            let key = role
                .map(|r| r.field().to_string())
                .unwrap_or(reg.name.clone());
            let fahrenheit = role.is_some_and(Role::is_temperature) && in_fahrenheit(reg);
            if fahrenheit {
                val = crate::transports::ingest::f_to_c(val);
            }

            // Unscaled 16-bit registers are whole numbers
            if matches!(
                reg.data_type,
                ModbusDataType::Uint16
                    | ModbusDataType::Int16
                    | ModbusDataType::Bool
                    | ModbusDataType::Bcd16
                    | ModbusDataType::Bcd32
            ) && reg.scale_factor.is_none()
                && !fahrenheit
            {
                payload.insert(key, json!(val as i64));
            } else {
                payload.insert(key, json!((val * 100.0).round() / 100.0));
            }
        }
    }
}

/// Honor the register map's word order for 32-bit values.
fn word_pair(reg: &ModbusRegisterMap, first: u16, second: u16) -> (u16, u16) {
    match reg.byte_order.as_deref().map(str::to_ascii_uppercase) {
        Some(o) if o == "CDAB" || o == "DCBA" => (second, first),
        _ => (first, second),
    }
}

pub(super) fn reg(
    register_type: ModbusRegisterType,
    address: i32,
    name: &str,
    data_type: ModbusDataType,
    unit: Option<&str>,
    writable: bool,
) -> ModbusRegisterMap {
    ModbusRegisterMap {
        id: String::new(),
        device_id: String::new(),
        register_type,
        address,
        name: name.into(),
        data_type,
        byte_order: None,
        scale_factor: None,
        offset: None,
        unit: unit.map(Into::into),
        description: None,
        writable,
        role: None,
        unit_id: None,
    }
}

/// Default register map matching the Roastwarden Standard / mock device layout,
/// including the writable holding registers so control works without a
/// configured map.
pub fn default_register_map() -> Vec<ModbusRegisterMap> {
    use ModbusDataType::{Float32, Uint16};
    use ModbusRegisterType::{Holding, Input};

    vec![
        reg(Input, 0, "beanTemp", Float32, Some("°C"), false),
        reg(Input, 2, "envTemp", Float32, Some("°C"), false),
        reg(Input, 4, "rateOfRise", Float32, Some("°C/min"), false),
        reg(Input, 6, "heater", Uint16, Some("%"), false),
        reg(Input, 7, "fan", Uint16, Some("%"), false),
        reg(Holding, 0, "setpoint", Float32, Some("°C"), true),
        reg(Holding, 2, "fan_cmd", Uint16, Some("%"), true),
        reg(Holding, 3, "heater_cmd", Uint16, Some("%"), true),
        reg(Holding, 4, "control_mode", Uint16, None, true),
        reg(Holding, 5, "heater_enable", Uint16, None, true),
        reg(Holding, 12, "emergency_stop", Uint16, None, true),
    ]
}
