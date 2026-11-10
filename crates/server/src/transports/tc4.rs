//! TC4 / aArtisan serial protocol.
//!
//! The TC4 shield and its `aArtisan` firmware define the de-facto serial
//! protocol for DIY roasters, and it is what Artisan speaks when a device is
//! configured as "TC4". It is line oriented, ASCII, and simple enough for an
//! ESP32 to implement over USB CDC in an afternoon, which makes it a good
//! second transport next to MQTT: no Wi-Fi, no broker, and the same firmware
//! works with Artisan directly.
//!
//! Master → device (each line ends with `\n`):
//!
//! | Command              | Meaning                                              |
//! |----------------------|------------------------------------------------------|
//! | `CHAN;ijkl`          | map logical channels 1..4 to physical inputs (0 = off) |
//! | `UNITS;C`            | report Celsius                                       |
//! | `READ`               | request one sample                                   |
//! | `OT1;<0-100>`        | heater duty (%)                                      |
//! | `OT2;<0-100>`        | second output duty (%); the fan on some boards       |
//! | `IO3;<0-100>`        | fan duty (%), the default fan output                 |
//! | `DCFAN;<0-100>`      | fan on IO3, rising at most 25 points a second        |
//! | `PID;ON` / `PID;OFF` | enable or disable the device PID loop                |
//! | `PID;SV;<temp>`      | PID setpoint                                         |
//! | `PID;T;<p>;<i>;<d>`  | PID tunings                                          |
//!
//! Device → master: a `READ` is answered by one CSV line, in one of two
//! layouts ([`ReplyFormat`]):
//!
//! - `fixed` (Roastwarden's firmware and mock, the default):
//!   `ambient,T1,T2,T3,T4[,heater,fan,sv[,pid_on,heater_en]]`, all four
//!   logical channels always printed (`-1` for none);
//! - `active` (stock aArtisan): `ambient`, then only the logical channels
//!   `CHAN` turned on, in channel order, then `heater,fan` and `sv` (aArtisan
//!   3.11 prints SV only while its PID runs; `aArtisanQ_PID` always, 0 when off).
//!
//! Logical channel 1 is ET and 2 is BT (Artisan's convention, `CHAN;1200`);
//! 3 and 4 are carried as the extra fields `t3` and `t4`. `pid_on` and
//! `heater_en` are a Roastwarden extension Artisan ignores. Command
//! acknowledgements and chatter start with `#` and are ignored.
//!
//! Sources: aArtisan 3.11 `aArtisan.ino` `logger()`, `user.h` and
//! `commands.txt`; `aArtisanQ_PID` 6.8 `logger()` and `user.h`
//! (github.com/greencardigan/TC4-shield, `applications/Artisan/`); Artisan's
//! `ARDUINOTC4temperature` in `src/artisanlib/comm.py`.

use std::time::Duration;

use roastwarden_core::{ControlMode, DeviceCommand};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::transports::ingest::{Column, Kind, Units, f_to_c};
use crate::transports::transport::TransportError;
use crate::transports::types::SerialTc4ConnectionConfig;

/// The layout of the device's answer to `READ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplyFormat {
    /// `ambient,T1,T2,T3,T4,…`: every logical channel printed (Roastwarden's firmware).
    #[default]
    Fixed,
    /// `ambient,<active channels>,heater,fan[,sv]`: stock aArtisan.
    Active,
}

/// The TC4 output a fan command drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FanOutput {
    /// `IO3;n`: aArtisan's default fan output (`FAN_DUTY levelIO3`).
    #[default]
    Io3,
    /// `OT2;n`: the second SSR / phase-angle output (`aArtisanQ_PID`'s default fan).
    Ot2,
    /// `DCFAN;n`: IO3 with aArtisan's slew limit (Hottop and other DC fans).
    Dcfan,
    /// No fan the server drives: fan commands are refused.
    None,
}

/// Which field each logical channel feeds, channel 1 first.
const CHANNEL_FIELDS: [&str; 4] = ["envTemp", "beanTemp", "t3", "t4"];

/// What the server knows about one TC4 connection: the `CHAN` mapping it
/// sends, how replies are laid out, where the fan is, and the device's own
/// unit and columns (docs/ingest-mapping.md, "Serial TC4").
#[derive(Debug, Clone, PartialEq)]
pub struct Tc4Setup {
    /// Physical input of logical channels 1..4 (0 = off), as sent in `CHAN`.
    pub chan: [u8; 4],
    pub reply: ReplyFormat,
    pub fan: FanOutput,
    /// `F`: `UNITS;F` at start, and the reply converted.
    pub units: Units,
    /// The reply's layout by position, instead of `reply`'s.
    pub columns: Option<Vec<Option<Column>>>,
    /// The roaster's declared °C channels (docs/channels.md): an extra column naming one,
    /// with no unit of its own, follows `units`. The poller refreshes it from the setup.
    pub temp_channels: Vec<String>,
}

impl Default for Tc4Setup {
    /// A connection with no settings: `CHAN;1200`, fixed replies, fan on IO3, °C.
    fn default() -> Self {
        Self {
            chan: [1, 2, 0, 0],
            reply: ReplyFormat::Fixed,
            fan: FanOutput::Io3,
            units: Units::C,
            columns: None,
            temp_channels: Vec::new(),
        }
    }
}

impl Tc4Setup {
    pub fn from_config(c: &SerialTc4ConnectionConfig) -> Self {
        let (ingest, _) = c
            .ingest
            .as_ref()
            .map(|i| crate::transports::ingest::read_config(Kind::Tc4, &json!({ "ingest": i })))
            .unwrap_or_default();
        let (units, columns) = ingest.map_or((Units::C, None), |i| (i.units, i.columns));
        Self {
            chan: [c.et_channel, c.bt_channel, c.t3_channel, c.t4_channel],
            reply: c.reply_format,
            fan: c.fan_output,
            units,
            columns,
            temp_channels: Vec::new(),
        }
    }

    /// The `CHAN;ijkl` line: one digit per logical channel.
    pub fn chan_line(&self) -> String {
        let d = self.chan.map(|c| c.min(9));
        format!("CHAN;{}{}{}{}", d[0], d[1], d[2], d[3])
    }

    /// The fields of the logical channels aArtisan turns on (physical input
    /// 1..4; anything else turns the channel off), in channel order.
    fn active_fields(&self) -> Vec<&'static str> {
        self.chan
            .iter()
            .zip(CHANNEL_FIELDS)
            .filter(|(c, _)| (1..=4).contains(*c))
            .map(|(_, f)| f)
            .collect()
    }

    /// Lines sent once after the port opens.
    pub fn init_lines(&self) -> Vec<String> {
        vec![format!("UNITS;{}", self.units.as_str()), self.chan_line()]
    }
}

/// Write one protocol line (`\n` terminated) and flush.
pub async fn write_line<W: AsyncWrite + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

/// Read lines until one parses as a sample or `timeout` elapses. Chatter
/// (`#` acknowledgements) is skipped. `Ok(None)` means timeout; an error
/// means the port is unusable. The sample comes with the line it was read from.
pub async fn read_sample<R: AsyncBufRead + Unpin>(
    r: &mut R,
    setup: &Tc4Setup,
    timeout: Duration,
) -> std::io::Result<Option<(String, Map<String, Value>)>> {
    read_sample_seen(r, setup, timeout, &mut None).await
}

/// [`read_sample`], keeping in `seen` the last line that was not chatter, whether it parsed
/// or not (the newest raw message, docs/add-roaster.md: a device that prints its own CSV is
/// mapped from it).
pub async fn read_sample_seen<R: AsyncBufRead + Unpin>(
    r: &mut R,
    setup: &Tc4Setup,
    timeout: Duration,
    seen: &mut Option<String>,
) -> std::io::Result<Option<(String, Map<String, Value>)>> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut line = String::new();
    loop {
        line.clear();
        match tokio::time::timeout_at(deadline, r.read_line(&mut line)).await {
            Ok(Ok(0)) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "serial port closed",
                ));
            }
            Ok(Ok(_)) => {
                let t = line.trim();
                if !t.is_empty() && !t.starts_with('#') {
                    *seen = Some(t.to_string());
                }
                if let Some(sample) = parse_read_line(&line, setup) {
                    return Ok(Some((line.trim().to_string(), sample)));
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => return Ok(None),
        }
    }
}

fn whole_pct(v: f64) -> u8 {
    if v.is_finite() {
        v.clamp(0.0, 100.0).round() as u8
    } else {
        0
    }
}

/// Translate a command into TC4 lines, the fan's on `fan`. An empty vector
/// means "nothing to send but the command is satisfied" (heater enable has no
/// TC4 equivalent).
pub fn command_lines(cmd: &DeviceCommand, fan: FanOutput) -> Result<Vec<String>, TransportError> {
    let unsupported = || TransportError::Unsupported {
        transport: "serial_tc4",
        command: cmd.name(),
    };
    Ok(match cmd {
        DeviceCommand::Setpoint { value } => vec![format!("PID;SV;{:.1}", value)],
        // TC4 takes whole percent.
        DeviceCommand::Heater { percent } => vec![format!("OT1;{}", whole_pct(*percent))],
        DeviceCommand::Fan { percent } => {
            let pct = whole_pct(*percent);
            match fan {
                FanOutput::Io3 => vec![format!("IO3;{pct}")],
                FanOutput::Ot2 => vec![format!("OT2;{pct}")],
                FanOutput::Dcfan => vec![format!("DCFAN;{pct}")],
                FanOutput::None => return Err(unsupported()),
            }
        }
        DeviceCommand::Mode { mode } => match mode {
            ControlMode::Auto => vec!["PID;ON".to_string()],
            ControlMode::Manual => vec!["PID;OFF".to_string()],
        },
        DeviceCommand::HeaterEnable { enabled } => {
            if *enabled {
                Vec::new()
            } else {
                vec!["PID;OFF".to_string(), "OT1;0".to_string()]
            }
        }
        // Heat off and the fan, wherever it is, to full; OT2 off unless it is the fan.
        DeviceCommand::EmergencyStop => {
            let mut lines = vec!["PID;OFF".to_string(), "OT1;0".to_string()];
            match fan {
                FanOutput::Io3 => lines.extend(["OT2;0".to_string(), "IO3;100".to_string()]),
                FanOutput::Ot2 => lines.push("OT2;100".to_string()),
                FanOutput::Dcfan => lines.extend(["OT2;0".to_string(), "DCFAN;100".to_string()]),
                FanOutput::None => lines.push("OT2;0".to_string()),
            }
            lines
        }
        DeviceCommand::Pid { kp, ki, kd, .. } => vec![format!("PID;T;{kp};{ki};{kd}")],
        // No aArtisan line: a connection's template carries them (transport::SerialSink).
        DeviceCommand::Drum { .. }
        | DeviceCommand::Damper { .. }
        | DeviceCommand::Feedforward { .. }
        | DeviceCommand::Lease { .. }
        | DeviceCommand::RorLimit { .. } => return Err(unsupported()),
    })
}

/// Parse a `READ` response into Roastwarden telemetry fields. Returns `None`
/// for chatter (`#` lines), blank lines, or anything that is not a sample.
///
/// The columns follow `setup`: in the `fixed` layout logical channels 1..4
/// are columns 1..4 whatever `CHAN` says; in the `active` layout only the
/// channels `CHAN` turned on are printed, so the outputs start right after
/// them. Channel 1 is ET, 2 BT, 3 and 4 the extras `t3` and `t4`. A value of
/// `-1` or an unparsable field means the probe is missing and the field is
/// omitted.
pub fn parse_read_line(line: &str, setup: &Tc4Setup) -> Option<Map<String, Value>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // The device's own CSV, by `ingest.columns`.
    if let Some(cols) = &setup.columns {
        return crate::transports::ingest::read_columns(
            line,
            cols,
            setup.units,
            &setup.temp_channels,
        );
    }
    let mut out = parse_layout(line, setup)?;
    if setup.units == Units::F {
        // The probes and the setpoint; the outputs are %.
        for key in ["beanTemp", "envTemp", "ambientTemp", "t3", "t4", "setpoint"] {
            if let Some(v) = out.get(key).and_then(Value::as_f64) {
                out.insert(key.into(), json!(round1(f_to_c(v))));
            }
        }
    }
    Some(out)
}

/// The reply read by `reply_format`'s layout.
fn parse_layout(line: &str, setup: &Tc4Setup) -> Option<Map<String, Value>> {
    let fields: Vec<Option<f64>> = line
        .split(',')
        .map(|f| f.trim().parse::<f64>().ok())
        .collect();
    let num = |i: usize| fields.get(i).copied().flatten().filter(|v| v.is_finite());
    let temp = |i: usize| num(i).filter(|v| *v > -1.0);

    let channels: Vec<&'static str> = match setup.reply {
        ReplyFormat::Fixed if fields.len() < 3 => return None,
        ReplyFormat::Fixed => CHANNEL_FIELDS.to_vec(),
        ReplyFormat::Active => {
            let active = setup.active_fields();
            if active.is_empty() || fields.len() < 1 + active.len() {
                return None;
            }
            active
        }
    };
    let mut out = Map::new();
    for (i, field) in channels.iter().enumerate() {
        if let Some(v) = temp(1 + i) {
            out.insert((*field).into(), json!(round1(v)));
        }
    }
    if !out.contains_key("beanTemp") && !out.contains_key("envTemp") {
        return None;
    }
    if let Some(v) = temp(0) {
        out.insert("ambientTemp".into(), json!(round1(v)));
    }
    let outputs = 1 + channels.len();
    if let Some(v) = num(outputs) {
        out.insert("heater".into(), json!(v.clamp(0.0, 100.0)));
    }
    if let Some(v) = num(outputs + 1) {
        out.insert("fan".into(), json!(v.clamp(0.0, 100.0)));
    }
    // aArtisanQ_PID prints SV 0 while its PID is off: no setpoint, not 0 °C.
    let sv = num(outputs + 2).filter(|v| setup.reply == ReplyFormat::Fixed || *v > 0.0);
    if let Some(v) = sv {
        out.insert("setpoint".into(), json!(round1(v)));
    }
    // Roastwarden extension (ignored by Artisan): PID on/off and heater enable flags.
    if let Some(v) = num(outputs + 3) {
        out.insert("controlMode".into(), json!(i32::from(v != 0.0)));
    }
    if let Some(v) = num(outputs + 4) {
        out.insert("heaterEnable".into(), json!(i32::from(v != 0.0)));
    }
    Some(out)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// One line on what a parsed sample says, for the connection test: what each
/// column was read as.
pub fn summary(sample: &Map<String, Value>) -> String {
    const NAMES: [(&str, &str); 9] = [
        ("beanTemp", "BT"),
        ("envTemp", "ET"),
        ("t3", "T3"),
        ("t4", "T4"),
        ("heater", "heater"),
        ("fan", "fan"),
        ("drum", "drum"),
        ("damper", "damper"),
        ("setpoint", "SV"),
    ];
    NAMES
        .iter()
        .map(|(key, name)| match sample.get(*key) {
            Some(v) => format!("{name} {v}"),
            None if matches!(*key, "beanTemp" | "envTemp") => format!("{name} -"),
            None => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(chan: [u8; 4], reply: ReplyFormat) -> Tc4Setup {
        Tc4Setup {
            chan,
            reply,
            ..Tc4Setup::default()
        }
    }

    /// The newest raw line is kept whether it parses or not (docs/add-roaster.md); chatter
    /// is not.
    #[tokio::test]
    async fn the_last_line_read_is_seen() {
        let mut r = tokio::io::BufReader::new(&b"#ack\n12,x\n"[..]);
        let mut seen = None;
        let got = read_sample_seen(
            &mut r,
            &Tc4Setup::default(),
            Duration::from_millis(200),
            &mut seen,
        )
        .await;
        assert!(got.is_err(), "the port closed");
        assert_eq!(seen.as_deref(), Some("12,x"));
        let mut r = tokio::io::BufReader::new(&b"21.0,180.5,200.25,0,0,60,80,0\n"[..]);
        let mut seen = None;
        let got = read_sample_seen(
            &mut r,
            &Tc4Setup::default(),
            Duration::from_millis(200),
            &mut seen,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(seen.as_deref(), Some(got.0.as_str()));
    }

    /// docs/ingest-mapping.md, "Serial TC4": `UNITS;F` and the reply converted; `columns`
    /// replaces the reply format's layout.
    #[test]
    fn a_fahrenheit_device_and_its_own_columns() {
        let c = config(json!({"port": "/dev/ttyUSB0", "ingest": {"units": "F"}}));
        let s = Tc4Setup::from_config(&c);
        assert_eq!(s.init_lines(), vec!["UNITS;F", "CHAN;1200"]);
        let m = parse_read_line("68.0,392.0,212.0,-1,-1,42,60,401.0", &s).unwrap();
        assert_eq!(m["ambientTemp"], 20.0);
        assert_eq!(m["envTemp"], 200.0);
        assert_eq!(m["beanTemp"], 100.0);
        assert_eq!(m["setpoint"], 205.0);
        assert_eq!(m["heater"], 42.0, "a % is not converted");

        let c = config(json!({"port": "COM3", "reply_format": "active",
            "ingest": {"columns": ["", "bt", "et", "drum", "exhaust"]}}));
        let s = Tc4Setup::from_config(&c);
        assert_eq!(s.init_lines()[0], "UNITS;C");
        let m = parse_read_line("21.0,180.5,210.25,55,140", &s).unwrap();
        assert_eq!(m["beanTemp"], 180.5);
        assert_eq!(m["envTemp"], 210.3);
        assert_eq!(m["drum"], 55.0);
        assert_eq!(m["exhaust"], 140.0);
        assert!(!m.contains_key("ambientTemp"), "the skipped column");
        assert!(parse_read_line("# ack", &s).is_none());
        assert!(parse_read_line("21.0,-1,-1,55", &s).is_none());
        // A config with a bad mapping is refused before it is saved; read anyway, it is
        // today's setup.
        let c = config(json!({"port": "x", "ingest": {"columns": ["heater"]}}));
        assert_eq!(Tc4Setup::from_config(&c), Tc4Setup::default());
    }

    fn config(json: Value) -> SerialTc4ConnectionConfig {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn init_maps_channels() {
        assert_eq!(
            Tc4Setup::default().init_lines(),
            vec!["UNITS;C", "CHAN;1200"]
        );
        assert_eq!(
            setup([2, 1, 0, 0], ReplyFormat::Fixed).init_lines()[1],
            "CHAN;2100"
        );
        assert_eq!(
            setup([1, 2, 3, 4], ReplyFormat::Active).chan_line(),
            "CHAN;1234"
        );
        assert_eq!(
            setup([0, 3, 0, 0], ReplyFormat::Active).chan_line(),
            "CHAN;0300"
        );
    }

    #[test]
    fn a_config_with_no_new_settings_is_todays_setup() {
        let c = config(json!({"port": "/dev/ttyUSB0"}));
        assert_eq!(Tc4Setup::from_config(&c), Tc4Setup::default());
        let c = config(json!({"port": "/dev/ttyUSB0", "et_channel": 2, "bt_channel": 1}));
        let s = Tc4Setup::from_config(&c);
        assert_eq!(s.init_lines(), vec!["UNITS;C", "CHAN;2100"]);
        assert_eq!((s.reply, s.fan), (ReplyFormat::Fixed, FanOutput::Io3));

        let c = config(json!({
            "port": "COM5", "t3_channel": 3, "t4_channel": 4,
            "reply_format": "active", "fan_output": "ot2"
        }));
        let s = Tc4Setup::from_config(&c);
        assert_eq!(s.chan_line(), "CHAN;1234");
        assert_eq!((s.reply, s.fan), (ReplyFormat::Active, FanOutput::Ot2));
        assert!(
            serde_json::from_value::<SerialTc4ConnectionConfig>(
                json!({"port": "x", "fan_output": "io4"})
            )
            .is_err()
        );
    }

    #[test]
    fn commands_render_as_aartisan_lines() {
        let io3 = FanOutput::Io3;
        assert_eq!(
            command_lines(&DeviceCommand::Setpoint { value: 200.25 }, io3).unwrap(),
            vec!["PID;SV;200.2"]
        );
        assert_eq!(
            command_lines(&DeviceCommand::Heater { percent: 55.4 }, io3).unwrap(),
            vec!["OT1;55"]
        );
        assert_eq!(
            command_lines(&DeviceCommand::Fan { percent: 100.0 }, io3).unwrap(),
            vec!["IO3;100"]
        );
        assert_eq!(
            command_lines(&DeviceCommand::Fan { percent: 49.6 }, io3).unwrap(),
            vec!["IO3;50"]
        );
        assert_eq!(
            command_lines(
                &DeviceCommand::Mode {
                    mode: ControlMode::Auto
                },
                io3
            )
            .unwrap(),
            vec!["PID;ON"]
        );
        assert_eq!(
            command_lines(&DeviceCommand::HeaterEnable { enabled: true }, io3).unwrap(),
            [] as [std::string::String; 0]
        );
        assert_eq!(
            command_lines(&DeviceCommand::EmergencyStop, io3).unwrap(),
            vec!["PID;OFF", "OT1;0", "OT2;0", "IO3;100"]
        );
        assert!(matches!(
            command_lines(&DeviceCommand::Lease { ms: 10_000 }, io3),
            Err(TransportError::Unsupported { .. })
        ));
        assert!(matches!(
            command_lines(&DeviceCommand::Drum { percent: 50.0 }, io3),
            Err(TransportError::Unsupported {
                command: "drum",
                ..
            })
        ));
    }

    #[test]
    fn the_fan_goes_to_its_configured_output() {
        let fan = DeviceCommand::Fan { percent: 72.0 };
        assert_eq!(command_lines(&fan, FanOutput::Ot2).unwrap(), vec!["OT2;72"]);
        assert_eq!(
            command_lines(&fan, FanOutput::Dcfan).unwrap(),
            vec!["DCFAN;72"]
        );
        assert!(matches!(
            command_lines(&fan, FanOutput::None),
            Err(TransportError::Unsupported { command: "fan", .. })
        ));
        // Emergency stop: heat off, the fan to full wherever it is, never OT2 to 0 when OT2 is the fan.
        let stop = DeviceCommand::EmergencyStop;
        assert_eq!(
            command_lines(&stop, FanOutput::Ot2).unwrap(),
            vec!["PID;OFF", "OT1;0", "OT2;100"]
        );
        assert_eq!(
            command_lines(&stop, FanOutput::Dcfan).unwrap(),
            vec!["PID;OFF", "OT1;0", "OT2;0", "DCFAN;100"]
        );
        assert_eq!(
            command_lines(&stop, FanOutput::None).unwrap(),
            vec!["PID;OFF", "OT1;0", "OT2;0"]
        );
        // The heater stays on OT1 whatever the fan does.
        assert_eq!(
            command_lines(&DeviceCommand::Heater { percent: 30.0 }, FanOutput::Ot2).unwrap(),
            vec!["OT1;30"]
        );
    }

    #[test]
    fn parses_fixed_read_responses() {
        let fixed = Tc4Setup::default();
        let m = parse_read_line("23.5,180.2,165.75,-1,-1,42,60,170.0,1,1\n", &fixed).unwrap();
        assert_eq!(m["ambientTemp"], 23.5);
        assert_eq!(m["envTemp"], 180.2);
        assert_eq!(m["beanTemp"], 165.8);
        assert_eq!(m["heater"], 42.0);
        assert_eq!(m["fan"], 60.0);
        assert_eq!(m["setpoint"], 170.0);
        assert_eq!(m["controlMode"], 1);
        assert_eq!(m["heaterEnable"], 1);
        assert!(!m.contains_key("t3") && !m.contains_key("t4"));

        // Minimal five-field response, missing BT probe
        let m = parse_read_line("22.0,150.0,-1,-1,-1", &fixed).unwrap();
        assert!(m.contains_key("envTemp"));
        assert!(!m.contains_key("beanTemp"));
        assert!(!m.contains_key("heater"));

        assert!(parse_read_line("# OT1 = 50", &fixed).is_none());
        assert!(parse_read_line("", &fixed).is_none());
        assert!(parse_read_line("garbage", &fixed).is_none());
        assert!(parse_read_line("1,-1,-1,-1,-1", &fixed).is_none());
        assert!(parse_read_line("21.0,180.0", &fixed).is_none());
    }

    #[test]
    fn fixed_replies_keep_t3_and_t4() {
        let m = parse_read_line(
            "23.5,180.2,165.8,95.4,40.1,42,60,170.0",
            &Tc4Setup::default(),
        )
        .unwrap();
        assert_eq!(m["t3"], 95.4);
        assert_eq!(m["t4"], 40.1);
        assert_eq!(m["heater"], 42.0);
        assert_eq!(m["fan"], 60.0);
        assert_eq!(m["setpoint"], 170.0);
    }

    /// Stock aArtisan 3.11 on `CHAN;1200`: `ambient,ET,BT,heater,fan` and `,SV`
    /// while its PID runs.
    #[test]
    fn active_replies_with_two_channels() {
        let s = setup([1, 2, 0, 0], ReplyFormat::Active);
        let m = parse_read_line("21.50,182.25,165.10,45,60", &s).unwrap();
        assert_eq!(m["ambientTemp"], 21.5);
        assert_eq!(m["envTemp"], 182.3);
        assert_eq!(m["beanTemp"], 165.1);
        assert_eq!(m["heater"], 45.0);
        assert_eq!(m["fan"], 60.0);
        assert!(!m.contains_key("setpoint"));
        assert!(!m.contains_key("controlMode"));

        // PID on: SV is appended, never read as the heater.
        let m = parse_read_line("21.50,182.25,165.10,73,60,200.00", &s).unwrap();
        assert_eq!(m["heater"], 73.0);
        assert_eq!(m["fan"], 60.0);
        assert_eq!(m["setpoint"], 200.0);

        // aArtisanQ_PID prints SV 0 while its PID is off.
        let m = parse_read_line("21.50,182.25,165.10,40,80,0", &s).unwrap();
        assert_eq!(m["fan"], 80.0);
        assert!(!m.contains_key("setpoint"));

        // Temperatures only (an old or trimmed firmware): no outputs, still a sample.
        let m = parse_read_line("21.5,182.2,165.1", &s).unwrap();
        assert_eq!(m["beanTemp"], 165.1);
        assert!(!m.contains_key("heater"));

        // Fewer columns than active channels: not a sample.
        assert!(parse_read_line("21.5,182.2", &s).is_none());
    }

    /// The same stock reply read the old fixed way: the bug this layout fixes.
    #[test]
    fn a_stock_reply_read_as_fixed_misplaces_the_outputs() {
        let m = parse_read_line("21.50,182.25,165.10,73,60,200.00", &Tc4Setup::default()).unwrap();
        assert_eq!(m["heater"], 100.0); // SV, clamped
        assert_eq!(m["t3"], 73.0); // the heater
    }

    #[test]
    fn active_replies_with_three_channels() {
        let s = setup([1, 2, 3, 0], ReplyFormat::Active);
        let m = parse_read_line("22.0,190.0,170.5,120.25,50,70", &s).unwrap();
        assert_eq!(m["envTemp"], 190.0);
        assert_eq!(m["beanTemp"], 170.5);
        assert_eq!(m["t3"], 120.3);
        assert!(!m.contains_key("t4"));
        assert_eq!(m["heater"], 50.0);
        assert_eq!(m["fan"], 70.0);

        let m = parse_read_line("22.0,190.0,170.5,120.25,50,70,205.5", &s).unwrap();
        assert_eq!(m["setpoint"], 205.5);
    }

    #[test]
    fn active_replies_with_four_channels() {
        let s = setup([1, 2, 3, 4], ReplyFormat::Active);
        let m = parse_read_line("22.0,190.0,170.5,120.0,35.5", &s).unwrap();
        assert_eq!(m["t3"], 120.0);
        assert_eq!(m["t4"], 35.5);
        assert!(!m.contains_key("heater"));

        let m = parse_read_line("22.0,190.0,170.5,120.0,35.5,65,55,210.0", &s).unwrap();
        assert_eq!(m["t4"], 35.5);
        assert_eq!(m["heater"], 65.0);
        assert_eq!(m["fan"], 55.0);
        assert_eq!(m["setpoint"], 210.0);

        // Roastwarden's extension fields still follow SV.
        let m = parse_read_line("22.0,190.0,170.5,120.0,35.5,65,55,210.0,1,1", &s).unwrap();
        assert_eq!(m["controlMode"], 1);
        assert_eq!(m["heaterEnable"], 1);
    }

    #[test]
    fn active_replies_follow_off_channels() {
        // BT only (no ET probe): CHAN;0100 prints ambient,BT,heater,fan.
        let s = setup([0, 1, 0, 0], ReplyFormat::Active);
        let m = parse_read_line("21.0,150.5,30,100", &s).unwrap();
        assert_eq!(m["beanTemp"], 150.5);
        assert!(!m.contains_key("envTemp"));
        assert_eq!(m["heater"], 30.0);
        assert_eq!(m["fan"], 100.0);

        // ET, BT and channel 4 on, channel 3 off: the third column is t4.
        let s = setup([1, 2, 0, 3], ReplyFormat::Active);
        let m = parse_read_line("21.0,200.0,180.0,60.0,30,90", &s).unwrap();
        assert_eq!(m["t4"], 60.0);
        assert!(!m.contains_key("t3"));
        assert_eq!(m["heater"], 30.0);

        // A physical input above 4 turns the channel off, as aArtisan does.
        let s = setup([1, 2, 7, 0], ReplyFormat::Active);
        let m = parse_read_line("21.0,200.0,180.0,30,90", &s).unwrap();
        assert_eq!(m["heater"], 30.0);

        // Nothing on: never a sample.
        let s = setup([0, 0, 0, 0], ReplyFormat::Active);
        assert!(parse_read_line("21.0,30,90", &s).is_none());
    }

    #[test]
    fn summary_names_each_column() {
        let s = setup([1, 2, 3, 0], ReplyFormat::Active);
        let m = parse_read_line("22.0,190.0,170.5,120.0,50,70,205.5", &s).unwrap();
        assert_eq!(
            summary(&m),
            "BT 170.5, ET 190.0, T3 120.0, heater 50.0, fan 70.0, SV 205.5"
        );
        let m = parse_read_line("22.0,150.0,-1,-1,-1", &Tc4Setup::default()).unwrap();
        assert_eq!(summary(&m), "BT -, ET 150.0");
    }
}
