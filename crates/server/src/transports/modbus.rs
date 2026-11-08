//! The Modbus TCP slave: a roaster's readings as Modbus registers, so Artisan (or a SCADA/HMI)
//! can be a screen for Roastwarden (docs/artisan-screen.md).
//!
//! Off unless `ROASTWARDEN_MODBUS_ADDR` is set (`0.0.0.0:502`). It serves one roaster:
//! `ROASTWARDEN_MODBUS_DEVICE_ID`, or else the one real roaster heard in the last minute.
//! Read-only unless `ROASTWARDEN_MODBUS_WRITES=on`: a write is refused (illegal function) by
//! default; switched on, it becomes a command through the roaster's control gate, like any
//! other (transport.rs), and each one goes in the audit log as `MODBUS /modbus/holding/<n>`.
//! A read while the roaster is silent (nothing for 10 s) is refused with "gateway target
//! failed to respond", so Artisan draws a gap rather than a 0 °C line.
//!
//! The register map is "Roastwarden Standard" (docs/hardware-integration-guide.md, "Modbus TCP
//! Connection"): float32 big-endian (ABCD) for temperatures, whole % for heater and fan.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use roastwarden_core::{ControlMode, DeviceCommand, Sample};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio_modbus::prelude::*;
use tokio_modbus::server::tcp::{accept_tcp_connection, Server};
use utoipa::ToSchema;

use crate::app::state::AppState;
use crate::transports::transport::{SendOptions, TransportError};

/// Input registers (function code 4): the roaster's readings.
mod input_reg {
    /// BT float32, 0x0000-0x0001
    pub const BT: u16 = 0x0000;
    /// ET float32, 0x0002-0x0003
    pub const ET: u16 = 0x0002;
    /// BT's rate of rise float32, 0x0004-0x0005
    pub const ROR: u16 = 0x0004;
    /// heater, whole %
    pub const HEATER: u16 = 0x0006;
    /// fan, whole %
    pub const FAN: u16 = 0x0007;
    pub const COUNT: u16 = 8;
}

/// Holding registers (function codes 3, 6 and 16): read the roaster's current values, write
/// a command (when writes are on).
mod holding_reg {
    /// setpoint float32 °C, 0x0000-0x0001
    pub const SETPOINT_HI: u16 = 0x0000;
    pub const SETPOINT_LO: u16 = 0x0001;
    /// fan, whole % (0-100)
    pub const FAN: u16 = 0x0002;
    /// heater, whole % (0-100)
    pub const HEATER: u16 = 0x0003;
    /// 0 = manual, 1 = auto
    pub const CONTROL_MODE: u16 = 0x0004;
    /// 0 = off, 1 = on
    pub const HEATER_ENABLE: u16 = 0x0005;
    /// write 1 to stop (0x0006-0x000B are reserved: read 0, a write is refused)
    pub const EMERGENCY_STOP: u16 = 0x000C;
    pub const COUNT: u16 = 0x000D;
}

/// Telemetry older than this is a silent roaster: reads are refused.
const STALE_SECS: u64 = 10;
/// A real roaster heard within this long can be the one served by default.
const HEARD_SECS: u64 = 60;

/// The slave's settings and what it has seen (`GET /api/about`'s `artisan_screen`).
#[derive(Debug)]
pub struct Screen {
    pub addr: SocketAddr,
    /// `ROASTWARDEN_MODBUS_DEVICE_ID`; none: the one real roaster heard lately
    pub roaster: Option<String>,
    /// `ROASTWARDEN_MODBUS_WRITES=on`
    pub writes: bool,
    /// When a client last read, Unix seconds (0: never), and from where
    last_read: AtomicU64,
    last_client: Mutex<Option<String>>,
}

impl Screen {
    pub fn new(addr: SocketAddr, roaster: Option<String>, writes: bool) -> Self {
        Self {
            addr,
            roaster,
            writes,
            last_read: AtomicU64::new(0),
            last_client: Mutex::new(None),
        }
    }

    /// From the environment; none when `ROASTWARDEN_MODBUS_ADDR` is unset, empty or bad.
    pub fn from_env() -> Option<Self> {
        let addr = std::env::var("ROASTWARDEN_MODBUS_ADDR")
            .ok()
            .filter(|a| !a.trim().is_empty())?;
        let addr: SocketAddr = match addr.trim().parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(%addr, %e, "ROASTWARDEN_MODBUS_ADDR is not an address:port; the Modbus slave is off");
                return None;
            }
        };
        let roaster = std::env::var("ROASTWARDEN_MODBUS_DEVICE_ID")
            .ok()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        let writes = std::env::var("ROASTWARDEN_MODBUS_WRITES").is_ok_and(|w| {
            matches!(
                w.trim().to_ascii_lowercase().as_str(),
                "on" | "true" | "1" | "yes"
            )
        });
        Some(Self::new(addr, roaster, writes))
    }

    fn saw_read(&self, peer: SocketAddr) {
        self.last_read.store(now_secs(), Ordering::Relaxed);
        *self
            .last_client
            .lock()
            .expect("the Modbus screen's lock is poisoned") = Some(peer.ip().to_string());
    }
}

/// What `GET /api/about` says of the Modbus slave (null: off).
#[derive(Debug, Serialize, ToSchema)]
pub struct ScreenStatus {
    /// Where it listens (`ROASTWARDEN_MODBUS_ADDR`)
    pub addr: String,
    pub port: u16,
    /// The roaster it was told to serve (`ROASTWARDEN_MODBUS_DEVICE_ID`); null: the one real
    /// roaster heard in the last minute
    pub roaster: Option<String>,
    /// The roaster it serves now; null when there is none to serve (none heard, or more than
    /// one and no `ROASTWARDEN_MODBUS_DEVICE_ID`): reads are refused
    pub serving: Option<String>,
    /// Why there is no roaster to serve, when there is none
    pub problem: Option<String>,
    /// Writes become commands (`ROASTWARDEN_MODBUS_WRITES=on`); false: read-only
    pub writes: bool,
    /// When a client (Artisan) last read, and its address
    pub last_read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_client: Option<String>,
}

/// The slave's status, for About and `roast status`.
pub async fn status(state: &AppState) -> Option<ScreenStatus> {
    let screen = state.modbus.as_ref()?;
    let (serving, problem) = match serving(state, screen).await {
        Ok(id) => (Some(id), None),
        Err(p) => (None, Some(p)),
    };
    let last = screen.last_read.load(Ordering::Relaxed);
    Some(ScreenStatus {
        addr: screen.addr.to_string(),
        port: screen.addr.port(),
        roaster: screen.roaster.clone(),
        serving,
        problem,
        writes: screen.writes,
        last_read_at: (last > 0)
            .then(|| chrono::DateTime::from_timestamp(last as i64, 0))
            .flatten(),
        last_client: screen
            .last_client
            .lock()
            .expect("the Modbus screen's lock is poisoned")
            .clone(),
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn is_virtual(id: &str) -> bool {
    crate::sim::virtual_roaster::is_simulated(id) || id == crate::sim::virtual_drum::DEVICE_ID
}

/// The roaster served: the one named, or the one real roaster heard in the last minute.
async fn serving(state: &AppState, screen: &Screen) -> Result<String, String> {
    if let Some(id) = &screen.roaster {
        return Ok(id.clone());
    }
    let now = now_secs();
    let cache = state.telemetry_cache.read().await;
    let mut heard: Vec<&String> = cache
        .iter()
        .filter(|(id, (_, ts))| !is_virtual(id) && now.saturating_sub(*ts) <= HEARD_SECS)
        .map(|(id, _)| id)
        .collect();
    heard.sort();
    match heard.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(
            "no roaster heard in the last minute (set ROASTWARDEN_MODBUS_DEVICE_ID to name one)"
                .into(),
        ),
        many => Err(format!(
            "{} roasters heard ({}): set ROASTWARDEN_MODBUS_DEVICE_ID to the one to serve",
            many.len(),
            many.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The served roaster's newest sample, if it is fresh.
async fn fresh_sample(
    state: &AppState,
    screen: &Screen,
) -> Result<(String, Sample), ExceptionCode> {
    let id = serving(state, screen)
        .await
        .map_err(|_| ExceptionCode::GatewayTargetDevice)?;
    let cache = state.telemetry_cache.read().await;
    match cache.get(&id) {
        Some((json, ts)) if now_secs().saturating_sub(*ts) <= STALE_SECS => {
            Ok((id, Sample::from_telemetry_json(0, ts * 1000, json)))
        }
        _ => Err(ExceptionCode::GatewayTargetDevice),
    }
}

// ---------------------------------------------------------------------------
// Registers
// ---------------------------------------------------------------------------

/// An f32 as two registers, high word first (ABCD).
fn f32_to_registers(val: f32) -> [u16; 2] {
    let bits = val.to_bits();
    [(bits >> 16) as u16, (bits & 0xFFFF) as u16]
}

fn registers_to_f32(hi: u16, lo: u16) -> f32 {
    f32::from_bits(((hi as u32) << 16) | (lo as u32))
}

/// A float32 reading; NaN when the roaster does not send it.
fn float(v: Option<f64>) -> [u16; 2] {
    f32_to_registers(v.map_or(f32::NAN, |v| v as f32))
}

fn pct(v: Option<f64>) -> u16 {
    v.unwrap_or(0.0).clamp(0.0, 100.0).round() as u16
}

fn input_image(s: &Sample) -> [u16; input_reg::COUNT as usize] {
    let mut img = [0u16; input_reg::COUNT as usize];
    let mut put2 = |at: u16, r: [u16; 2]| {
        img[at as usize] = r[0];
        img[at as usize + 1] = r[1];
    };
    put2(input_reg::BT, float(s.bt));
    put2(input_reg::ET, float(s.et));
    put2(input_reg::ROR, float(s.ror_bt));
    img[input_reg::HEATER as usize] = pct(s.heater);
    img[input_reg::FAN as usize] = pct(s.fan);
    img
}

/// The holding registers read back what the roaster reports now.
fn holding_image(s: &Sample) -> [u16; holding_reg::COUNT as usize] {
    let mut img = [0u16; holding_reg::COUNT as usize];
    let sp = float(s.setpoint);
    img[holding_reg::SETPOINT_HI as usize] = sp[0];
    img[holding_reg::SETPOINT_LO as usize] = sp[1];
    img[holding_reg::FAN as usize] = pct(s.fan);
    img[holding_reg::HEATER as usize] = pct(s.heater);
    img[holding_reg::CONTROL_MODE as usize] = s.mode.unwrap_or(0) as u16;
    img[holding_reg::HEATER_ENABLE as usize] =
        s.heater_enable
            .unwrap_or_else(|| s.heater.is_some_and(|h| h > 0.0)) as u16;
    img
}

fn slice(img: &[u16], addr: u16, cnt: u16) -> Result<Vec<u16>, ExceptionCode> {
    let end = addr as usize + cnt as usize;
    if cnt == 0 || end > img.len() {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    Ok(img[addr as usize..end].to_vec())
}

/// The commands a write of `values` at `addr` asks for. A setpoint needs both of its
/// registers in one write (function 16); a write that touches a reserved register is refused.
fn commands_for(addr: u16, values: &[u16]) -> Result<Vec<(u16, DeviceCommand)>, ExceptionCode> {
    let end = addr as usize + values.len();
    if values.is_empty() || end > holding_reg::COUNT as usize {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    let at = |reg: u16| values[(reg - addr) as usize];
    let mut out = Vec::new();
    let mut reg = addr;
    while (reg as usize) < end {
        match reg {
            holding_reg::SETPOINT_HI | holding_reg::SETPOINT_LO => {
                if reg != holding_reg::SETPOINT_HI || (reg as usize + 1) >= end {
                    // half a float
                    return Err(ExceptionCode::IllegalDataValue);
                }
                let v =
                    registers_to_f32(at(holding_reg::SETPOINT_HI), at(holding_reg::SETPOINT_LO));
                if !v.is_finite() {
                    return Err(ExceptionCode::IllegalDataValue);
                }
                out.push((
                    reg,
                    DeviceCommand::Setpoint {
                        value: f64::from(v),
                    },
                ));
                reg += 2;
                continue;
            }
            holding_reg::FAN | holding_reg::HEATER => {
                let v = at(reg);
                if v > 100 {
                    return Err(ExceptionCode::IllegalDataValue);
                }
                let percent = f64::from(v);
                out.push((
                    reg,
                    if reg == holding_reg::FAN {
                        DeviceCommand::Fan { percent }
                    } else {
                        DeviceCommand::Heater { percent }
                    },
                ));
            }
            holding_reg::CONTROL_MODE => out.push((
                reg,
                DeviceCommand::Mode {
                    mode: if at(reg) == 0 {
                        ControlMode::Manual
                    } else {
                        ControlMode::Auto
                    },
                },
            )),
            holding_reg::HEATER_ENABLE => out.push((
                reg,
                DeviceCommand::HeaterEnable {
                    enabled: at(reg) != 0,
                },
            )),
            holding_reg::EMERGENCY_STOP => {
                if at(reg) == 1 {
                    out.push((reg, DeviceCommand::EmergencyStop));
                }
            }
            _ => return Err(ExceptionCode::IllegalDataAddress),
        }
        reg += 1;
    }
    Ok(out)
}

/// A write: refused while writes are off; otherwise each command goes to the roaster through
/// its control gate, and in the audit log.
async fn write(
    state: &AppState,
    screen: &Screen,
    peer: SocketAddr,
    addr: u16,
    values: &[u16],
) -> Result<(), ExceptionCode> {
    let ip = Some(peer.ip().to_string());
    let path = format!("/modbus/holding/{addr}");
    let body = serde_json::json!({ "register": addr, "values": values }).to_string();
    if !screen.writes {
        crate::auth::audit::record_other(
            &state.db,
            "modbus",
            ip,
            "MODBUS",
            &path,
            None,
            403,
            Some(format!(
                "{body} (read-only: ROASTWARDEN_MODBUS_WRITES is off)"
            )),
        )
        .await;
        return Err(ExceptionCode::IllegalFunction);
    }
    let commands = match commands_for(addr, values) {
        Ok(c) => c,
        Err(e) => {
            crate::auth::audit::record_other(
                &state.db,
                "modbus",
                ip,
                "MODBUS",
                &path,
                None,
                400,
                Some(body),
            )
            .await;
            return Err(e);
        }
    };
    let id = match serving(state, screen).await {
        Ok(id) => id,
        Err(problem) => {
            crate::auth::audit::record_other(
                &state.db,
                "modbus",
                ip,
                "MODBUS",
                &path,
                None,
                503,
                Some(format!("{body} ({problem})")),
            )
            .await;
            return Err(ExceptionCode::GatewayTargetDevice);
        }
    };
    if commands.is_empty() {
        // a 0 to the stop register: nothing to send, logged all the same
        crate::auth::audit::record_other(
            &state.db,
            "modbus",
            ip,
            "MODBUS",
            &path,
            Some(&id),
            200,
            Some(format!("{body} (nothing to send)")),
        )
        .await;
        return Ok(());
    }
    for (reg, cmd) in commands {
        let path = format!("/modbus/holding/{reg}");
        let body = serde_json::to_string(&cmd).unwrap_or_default();
        let sent = state
            .transports
            .send(&id, &cmd, SendOptions::default())
            .await;
        let status = match &sent {
            Ok(_) => 200,
            Err(TransportError::ControlOff { .. } | TransportError::NotDeclared { .. }) => 403,
            Err(_) => 502,
        };
        crate::auth::audit::record_other(
            &state.db,
            "modbus",
            ip.clone(),
            "MODBUS",
            &path,
            Some(&id),
            status,
            Some(match &sent {
                Ok(_) => body,
                Err(e) => format!("{body} ({e})"),
            }),
        )
        .await;
        if let Err(e) = sent {
            tracing::warn!(device_id = %id, command = cmd.name(), %e, "Modbus write refused");
            return Err(ExceptionCode::ServerDeviceFailure);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

struct Service {
    state: AppState,
    screen: Arc<Screen>,
    peer: SocketAddr,
}

impl tokio_modbus::server::Service for Service {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = Pin<Box<dyn Future<Output = Result<Response, ExceptionCode>> + Send>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let (state, screen, peer) = (self.state.clone(), self.screen.clone(), self.peer);
        Box::pin(async move {
            match req {
                Request::ReadInputRegisters(addr, cnt) => {
                    let (_, s) = fresh_sample(&state, &screen).await?;
                    screen.saw_read(peer);
                    Ok(Response::ReadInputRegisters(slice(
                        &input_image(&s),
                        addr,
                        cnt,
                    )?))
                }
                Request::ReadHoldingRegisters(addr, cnt) => {
                    let (_, s) = fresh_sample(&state, &screen).await?;
                    screen.saw_read(peer);
                    Ok(Response::ReadHoldingRegisters(slice(
                        &holding_image(&s),
                        addr,
                        cnt,
                    )?))
                }
                Request::WriteSingleRegister(addr, value) => {
                    write(&state, &screen, peer, addr, &[value]).await?;
                    Ok(Response::WriteSingleRegister(addr, value))
                }
                Request::WriteMultipleRegisters(addr, values) => {
                    write(&state, &screen, peer, addr, &values).await?;
                    Ok(Response::WriteMultipleRegisters(addr, values.len() as u16))
                }
                _ => Err(ExceptionCode::IllegalFunction),
            }
        })
    }
}

/// Start the slave when `state.modbus` is set (`ROASTWARDEN_MODBUS_ADDR`).
pub async fn start(state: AppState) -> Option<tokio::task::JoinHandle<()>> {
    let Some(screen) = state.modbus.clone() else {
        tracing::info!("Modbus slave off (ROASTWARDEN_MODBUS_ADDR not set)");
        return None;
    };
    let listener = match TcpListener::bind(screen.addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(addr = %screen.addr, %e, "could not open the Modbus slave's port");
            return None;
        }
    };
    tracing::info!(
        addr = %screen.addr,
        roaster = screen.roaster.as_deref().unwrap_or("(the one heard)"),
        writes = screen.writes,
        "Modbus slave listening (docs/artisan-screen.md)"
    );
    Some(tokio::spawn(serve(listener, state, screen)))
}

pub(crate) async fn serve(listener: TcpListener, state: AppState, screen: Arc<Screen>) {
    let server = Server::new(listener);
    let on_connected = |stream, peer: SocketAddr| {
        let (state, screen) = (state.clone(), screen.clone());
        async move {
            tracing::info!(%peer, "Modbus client connected");
            accept_tcp_connection(stream, peer, |peer| {
                Ok(Some(Service {
                    state: state.clone(),
                    screen: screen.clone(),
                    peer,
                }))
            })
        }
    };
    let on_process_error = |err| tracing::warn!(%err, "Modbus connection error");
    if let Err(e) = server.serve(&on_connected, on_process_error).await {
        tracing::error!(?e, "the Modbus slave stopped");
    }
}

// ---------------------------------------------------------------------------
// Artisan's setup for the slave
// ---------------------------------------------------------------------------

/// The host of a `Host` header: `roastwarden.lan:8080` → `roastwarden.lan`, `[::1]:8080` → `::1`.
pub fn host_of(header: &str) -> String {
    let h = header.trim();
    if let Some(rest) = h.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest).to_string();
    }
    match h.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !host.contains(':') => {
            host.to_string()
        }
        _ => h.to_string(),
    }
}

/// A host name or an IP address, nothing else (it goes into an INI file).
pub fn plain_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
}

/// The setup checked against Artisan 4.2.0 itself, run headless: BT and ET
/// as inputs 1 and 2 (float32, word order big), heater and fan as the extra device `MODBUS_34`
/// (no temperature conversion), one block read a second. Its last section, `[Sliders]`, holds
/// them still; [`SLIDERS`] replaces it when writes are on. Artisan's reasons for each key:
/// docs/artisan-screen.md, "The setup file".
const TEMPLATE: &str = include_str!("../../assets/artisan/screen.aset");
/// The Air slider writes the fan (holding 2), the Burner slider the heater (holding 3).
const SLIDERS: &str = include_str!("../../assets/artisan/screen-sliders.aset");

/// An Artisan setup that reads the slave at `host:port`, in °F when `fahrenheit` (Artisan's
/// unit follows the file's `Mode`), with the Air and Burner sliders when `writes`.
pub fn aset(host: &str, port: u16, writes: bool, fahrenheit: bool) -> String {
    let mut out = String::with_capacity(TEMPLATE.len() + SLIDERS.len());
    for line in TEMPLATE.lines() {
        let line = match line {
            "[Sliders]" if writes => {
                out.push_str(SLIDERS);
                break;
            }
            l if l.starts_with("; Roastwarden as a Modbus TCP roaster") && writes => {
                "; Roastwarden as a Modbus TCP roaster for Artisan (Roastwarden Standard map), with \
                 control: the Air slider sets the fan, the Burner slider the heater."
                    .to_string()
            }
            "Mode=C" if fahrenheit => "Mode=F".to_string(),
            "host=SERVER_ADDRESS" => format!("host={host}"),
            "port=502" => format!("port={port}"),
            l => l.to_string(),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_are_high_word_first() {
        assert_eq!(f32_to_registers(185.5), [0x4339, 0x8000]);
        assert_eq!(registers_to_f32(0x4339, 0x8000), 185.5);
    }

    #[test]
    fn a_write_becomes_its_commands() {
        let c = commands_for(holding_reg::HEATER, &[40]).unwrap();
        assert!(matches!(c[..], [(3, DeviceCommand::Heater { percent })] if percent == 40.0));
        let sp = f32_to_registers(200.0);
        let c = commands_for(0, &[sp[0], sp[1], 75]).unwrap();
        assert!(matches!(c[0], (0, DeviceCommand::Setpoint { value }) if value == 200.0));
        assert!(matches!(c[1], (2, DeviceCommand::Fan { percent }) if percent == 75.0));
        // half a float, over 100 %, a reserved register, past the end
        assert_eq!(
            commands_for(0, &[1]).unwrap_err(),
            ExceptionCode::IllegalDataValue
        );
        assert_eq!(
            commands_for(1, &[1]).unwrap_err(),
            ExceptionCode::IllegalDataValue
        );
        assert_eq!(
            commands_for(3, &[101]).unwrap_err(),
            ExceptionCode::IllegalDataValue
        );
        assert_eq!(
            commands_for(7, &[1]).unwrap_err(),
            ExceptionCode::IllegalDataAddress
        );
        assert_eq!(
            commands_for(12, &[0, 0]).unwrap_err(),
            ExceptionCode::IllegalDataAddress
        );
        // a 0 to the stop register asks for nothing
        assert_eq!(
            commands_for(12, &[0]).unwrap(),
            [] as [(u16, roastwarden_core::DeviceCommand); 0]
        );
    }

    #[test]
    fn the_images_follow_the_sample() {
        let s = Sample {
            bt: Some(150.5),
            et: Some(210.25),
            ror_bt: Some(12.0),
            heater: Some(39.6),
            fan: Some(80.0),
            setpoint: Some(155.0),
            mode: Some(1),
            ..Default::default()
        };
        let i = input_image(&s);
        assert_eq!(registers_to_f32(i[0], i[1]), 150.5);
        assert_eq!(registers_to_f32(i[2], i[3]), 210.25);
        assert_eq!(&i[6..8], &[40, 80]);
        let h = holding_image(&s);
        assert_eq!(registers_to_f32(h[0], h[1]), 155.0);
        assert_eq!(&h[2..6], &[80, 40, 1, 1]);
        assert!(registers_to_f32(input_image(&Sample::default())[0], 0).is_nan());
        assert_eq!(
            slice(&i, 6, 3).unwrap_err(),
            ExceptionCode::IllegalDataAddress
        );
    }

    #[test]
    fn the_setup_fills_in_host_port_unit_and_sliders() {
        // Every line the generator changes is in the file once
        for l in ["host=SERVER_ADDRESS", "port=502", "Mode=C", "[Sliders]"] {
            assert_eq!(TEMPLATE.lines().filter(|x| *x == l).count(), 1, "{l}");
        }
        assert!(
            !TEMPLATE.contains("[System]"),
            "a [System] artisan_version >= 4.2 makes Artisan ask for a signature"
        );
        assert_eq!(
            TEMPLATE
                .split("[Sliders]")
                .nth(1)
                .unwrap()
                .matches('[')
                .count(),
            0,
            "[Sliders] is the last section"
        );
        let ro = aset("192.168.1.10", 5020, false, false);
        assert!(ro.contains("\nhost=192.168.1.10\nport=5020\n"), "{ro}");
        assert!(ro.contains("\nMode=C\n") && ro.contains("slideractions=0, 0, 0, 0"));
        assert!(!ro.contains("writeSingle"));
        let rw = aset("roastwarden.lan", 502, true, true);
        assert!(rw.contains("\nMode=F\n"), "{rw}");
        assert!(rw.contains(
            r#"slidercommands="writeSingle(1,2,{})", , , "writeSingle(1,5,1);writeSingle(1,3,{})""#
        ));
        assert_eq!(rw.matches("[Sliders]").count(), 1);
        assert!(rw.contains("with control"));
        assert!(plain_host("192.168.1.10") && plain_host("roastwarden.lan") && plain_host("::1"));
        assert!(!plain_host("a b") && !plain_host("x\nport=1") && !plain_host(""));
        assert_eq!(host_of("roastwarden.lan:8080"), "roastwarden.lan");
        assert_eq!(host_of("[::1]:8080"), "::1");
        assert_eq!(host_of("10.0.0.2"), "10.0.0.2");
    }

    /// A Modbus client against the slave in the whole server, as Artisan reads it.
    mod server {
        use super::super::*;
        use crate::test_support::{eventually, Harness};
        use serde_json::json;
        use tokio_modbus::client::{tcp, Context, Reader, Writer};

        async fn client(addr: SocketAddr) -> Context {
            tcp::connect_slave(addr, Slave(1)).await.unwrap()
        }

        fn exception<T: std::fmt::Debug>(r: tokio_modbus::Result<T>) -> ExceptionCode {
            match r {
                Ok(Err(e)) => e,
                other => panic!("expected an exception, got {other:?}"),
            }
        }

        async fn emit(h: &Harness, id: &str) {
            let r = h
                .owner(
                    "POST",
                    &format!("/api/test/emit-telemetry/{id}"),
                    Some(
                        json!({"beanTemp": 150.5, "envTemp": 210.25, "rateOfRise": 12.0,
                        "heater": 40, "fan": 80, "setpoint": 155.0, "controlMode": 1}),
                    ),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
        }

        async fn audit(h: &Harness) -> serde_json::Value {
            h.owner("GET", "/api/audit?kind=control", None).await.json()
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn read_only_it_serves_the_one_real_roaster_and_refuses_writes() {
            let (h, addr) = Harness::with_modbus(false, None).await;
            let mut c = client(addr).await;

            // Nobody heard yet: refused, and About says why
            assert_eq!(
                exception(c.read_input_registers(0, 8).await),
                ExceptionCode::GatewayTargetDevice
            );
            let about = h.owner("GET", "/api/about", None).await.json();
            let s = &about["artisan_screen"];
            assert_eq!(s["writes"], false, "{about}");
            assert!(s["serving"].is_null(), "{s}");
            assert!(
                s["problem"].as_str().unwrap().contains("no roaster heard"),
                "{s}"
            );

            // The virtual roaster never counts; the real one is served
            emit(&h, "virtual_roaster").await;
            emit(&h, "esp32_test").await;
            let mut regs = Vec::new();
            for _ in 0..50 {
                if let Ok(Ok(r)) = c.read_input_registers(0, 8).await {
                    regs = r;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert_eq!(regs.len(), 8, "the real roaster is served");
            assert_eq!(registers_to_f32(regs[0], regs[1]), 150.5);
            assert_eq!(registers_to_f32(regs[2], regs[3]), 210.25);
            assert_eq!(registers_to_f32(regs[4], regs[5]), 12.0);
            assert_eq!(&regs[6..], &[40, 80]);
            // Artisan's optimizer reads blocks: BT+ET, then heater+fan
            assert_eq!(
                c.read_input_registers(6, 2).await.unwrap().unwrap(),
                vec![40, 80]
            );
            let hold = c.read_holding_registers(0, 5).await.unwrap().unwrap();
            assert_eq!(registers_to_f32(hold[0], hold[1]), 155.0);
            assert_eq!(&hold[2..], &[80, 40, 1]);
            let about = h.owner("GET", "/api/about", None).await.json();
            let s = &about["artisan_screen"];
            assert_eq!(s["serving"], "esp32_test", "{s}");
            assert_eq!(s["last_client"], "127.0.0.1", "{s}");
            assert!(s["last_read_at"].is_string(), "{s}");

            // Read-only: a write is refused and logged
            assert_eq!(
                exception(c.write_single_register(3, 40).await),
                ExceptionCode::IllegalFunction
            );
            let a = audit(&h).await;
            let row = &a[0];
            assert_eq!(row["method"], "MODBUS", "{a}");
            assert_eq!(row["path"], "/modbus/holding/3", "{a}");
            assert_eq!(row["actor"], "modbus");
            assert_eq!(row["status"], 403);
            assert_eq!(row["kind"], "control");

            // A second real roaster: which one is ambiguous, so nothing is served
            emit(&h, "esp32_other").await;
            let mut refused = false;
            for _ in 0..50 {
                if matches!(
                    c.read_input_registers(0, 2).await,
                    Ok(Err(ExceptionCode::GatewayTargetDevice))
                ) {
                    refused = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert!(refused, "two real roasters: none served");
            let about = h.owner("GET", "/api/about", None).await.json();
            let problem = about["artisan_screen"]["problem"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(problem.contains("esp32_other, esp32_test"), "{problem}");
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_writes_on_a_write_goes_through_the_gate() {
            // A roaster nobody set up: control is off, so the gate refuses
            let (h, addr) = Harness::with_modbus(true, Some("esp32_test")).await;
            emit(&h, "esp32_test").await;
            let mut c = client(addr).await;
            assert_eq!(
                exception(c.write_single_register(3, 40).await),
                ExceptionCode::ServerDeviceFailure
            );
            let a = audit(&h).await;
            let row = &a[0];
            assert_eq!(row["device_id"], "esp32_test", "{a}");
            assert_eq!(row["status"], 403, "{a}");
            assert!(
                row["body"].as_str().unwrap().contains("control is off"),
                "{a}"
            );
            // A reserved register, half a setpoint
            assert_eq!(
                exception(c.write_single_register(8, 1).await),
                ExceptionCode::IllegalDataAddress
            );
            assert_eq!(
                exception(c.write_single_register(0, 0x4339).await),
                ExceptionCode::IllegalDataValue
            );
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn the_virtual_roaster_is_read_and_driven() {
            let (h, addr) = Harness::with_modbus(true, Some("virtual_roaster")).await;
            let r = h
                .owner(
                    "PUT",
                    "/api/virtual-roaster",
                    Some(json!({"enabled": true})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let mut c = client(addr).await;
            let mut bt = None;
            for _ in 0..50 {
                if let Ok(Ok(r)) = c.read_input_registers(0, 4).await {
                    bt = Some(registers_to_f32(r[0], r[1]));
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert!(bt.is_some_and(|t| t > 0.0), "BT {bt:?}");
            c.write_single_register(2, 70).await.unwrap().unwrap();
            let vr = &h.state.virtual_roaster;
            assert!(
                eventually(|| async { vr.state().await.fan == 70.0 }).await,
                "{:?}",
                vr.state().await
            );
            let a = audit(&h).await;
            let row = &a[0];
            assert_eq!(row["status"], 200, "{a}");
            assert_eq!(row["path"], "/modbus/holding/2", "{a}");
            assert_eq!(row["device_id"], "virtual_roaster", "{a}");
        }
    }
}
