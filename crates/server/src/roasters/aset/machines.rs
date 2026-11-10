//! The catalogue: every machine's summary (its Artisan device, whether a roaster can be
//! planned from it and why not), and the Fuji PID models.

use std::sync::LazyLock;

use serde::Serialize;
use utoipa::ToSchema;

use super::CATALOGUE;
use super::ini::Ini;
use super::plan::Input;

/// One machine in Artisan's catalogue.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Machine {
    /// `Brand/Model`, as Artisan's file is named (`Giesen/W6A`)
    pub key: String,
    pub brand: String,
    pub model: String,
    /// The machine's name in its setup (`roastertype_setup`), else the model
    pub name: String,
    /// The Artisan device the setup reads it through (29 is Modbus)
    pub artisan_device: i64,
    pub artisan_device_name: String,
    /// Roastwarden can set it up from this file
    pub supported: bool,
    /// Why not, when it cannot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

static MACHINES: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in CATALOGUE.lines() {
        if let Some(key) = line.strip_prefix("@@ ") {
            out.push((key.trim().to_string(), String::new()));
        } else if let Some((_, text)) = out.last_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    out
});

/// Every machine in the catalogue, by brand and model.
pub fn machines() -> Vec<Machine> {
    MACHINES
        .iter()
        .map(|(key, text)| summary(key, &Ini::parse(text)))
        .collect()
}

/// A machine's trimmed setup by its key (`Brand/Model`; case and `_` for spaces ignored).
pub fn machine_text(key: &str) -> Option<(&'static str, &'static str)> {
    let norm = |s: &str| s.trim().to_lowercase().replace(['_', ' '], "");
    let want = norm(key);
    MACHINES
        .iter()
        .find(|(k, _)| norm(k) == want)
        .map(|(k, t)| (k.as_str(), t.as_str()))
}

pub(super) fn summary(key: &str, ini: &Ini) -> Machine {
    let (brand, model) = key.split_once('/').unwrap_or(("", key));
    let device = ini.int("Device", "id").unwrap_or(-1);
    let why = unsupported(ini).err();
    Machine {
        key: key.to_string(),
        brand: brand.to_string(),
        model: model.replace('_', " "),
        name: ini
            .text("General", "roastertype_setup")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                format!("{} {}", brand, model.replace('_', " "))
                    .trim()
                    .into()
            }),
        artisan_device: device,
        artisan_device_name: device_name(device),
        supported: why.is_none(),
        why,
    }
}

/// Artisan's name for a device id (its `devicefunctionlist`), for the ones the catalogue uses
/// and their neighbours.
pub fn device_name(id: i64) -> String {
    match id {
        0 => "Fuji PID (PXG/PXR)",
        1 | 2 | 16 | 66 => "Omega handheld meter",
        3..=10 | 21 | 51 => "Center thermometer",
        11..=14 | 24 | 30 | 67 | 77 | 78 => "Voltcraft thermometer",
        17 | 161 => "Omega HH309",
        18 => "none",
        19 | 28 | 32 | 44 => "Arduino TC4",
        22 => "PID duty cycle",
        25 => "virtual (a formula)",
        26 => "Delta DTA PID",
        27 | 48 | 49 | 88 | 89 => "a program Artisan calls",
        29 | 33 | 55 | 109 | 150 | 207 => "Modbus",
        34..=43
        | 47
        | 52
        | 58..=65
        | 68..=76
        | 95..=98
        | 106
        | 107
        | 114
        | 123..=125
        | 137
        | 146..=149
        | 152..=159
        | 168
        | 169
        | 191..=193 => "Phidgets",
        45 | 46 | 100 | 108 | 120..=122 | 129..=133 | 194 | 195 => "Yocto",
        53 | 54 => "Hottop",
        79..=82 | 110 | 151 => "Siemens S7",
        83..=87 | 99 => "Aillio Bullet R1",
        176 => "Aillio Bullet R2",
        101..=105 => "Behmor",
        111..=113 | 118 | 119 => "WebSocket",
        115..=117 => "HB",
        126..=128 => "ARC",
        134..=136 | 171..=173 | 208 => "Santoker",
        138..=141 => "Kaleido",
        142..=145 | 160 => "IKAWA",
        164..=167 => "Mugma",
        196..=200 => "Orbiter",
        201..=206 => "MQTT",
        209 => "SR900",
        _ => return format!("Artisan device {id}"),
    }
    .to_string()
}

/// Whether Roastwarden can set up a machine from its setup, else why not.
pub(super) fn unsupported(ini: &Ini) -> Result<(), String> {
    let device = ini.int("Device", "id").unwrap_or(-1);
    match device {
        29 => modbus_supported(ini),
        0 => fuji_supported(ini),
        111..=113 | 118 | 119 => {
            let nodes = ini.list("WebSocket", "channel_nodes");
            if device == 111 && nodes.iter().take(2).any(|n| !n.trim().is_empty()) {
                Ok(())
            } else {
                Err("its WebSocket setup names no BT or ET node".into())
            }
        }
        201..=206 => {
            let nodes = ini.list("MQTT", "channel_nodes");
            let readable = nodes.iter().take(2).any(|n| {
                !n.trim().is_empty() && crate::transports::ingest::check_path(n.trim()).is_ok()
            });
            if device == 201 && readable {
                Ok(())
            } else {
                Err("its MQTT setup names no BT or ET field Roastwarden can read".into())
            }
        }
        // A TC4 (19), or a Phidgets temperature board as the main device (docs/phidgets.md).
        19 | 34 | 52 | 58 | 61 | 68 => Ok(()),
        35..=43 | 47 | 59 | 60 | 62..=65 | 69..=76 | 114 | 168 | 169 => Err(format!(
            "Artisan reads it through {}, a Phidget that is not a temperature board's BT and \
             ET (Roastwarden reads Phidgets temperature boards: docs/phidgets.md)",
            device_name(device)
        )),
        d => Err(format!(
            "Artisan reads it through {}, which Roastwarden has no transport for",
            device_name(d)
        )),
    }
}

/// A Modbus machine: RTU, ASCII or TCP, BT or ET read.
fn modbus_supported(ini: &Ini) -> Result<(), String> {
    let kind = ini.int("Modbus", "type").unwrap_or(0);
    match kind {
        0 | 1 | 3 => {}
        2 => {
            return Err(
                "its Modbus is serial binary, which Artisan itself no longer speaks".into(),
            );
        }
        4 => return Err("its Modbus is over UDP; Roastwarden speaks RTU, ASCII and TCP".into()),
        t => return Err(format!("its Modbus type {t} is not one Artisan documents")),
    }
    let bits = ini.int("Modbus", "bytesize").unwrap_or(8);
    if kind == 0 && bits != 8 {
        return Err("its serial frames are not 8 data bits; Modbus RTU needs 8".into());
    }
    if kind == 1 && !(7..=8).contains(&bits) {
        return Err(format!(
            "its serial frames are {bits} data bits; Modbus ASCII is 7 or 8"
        ));
    }
    if Input::read(ini, 1).is_none() && Input::read(ini, 2).is_none() {
        return Err("its setup reads neither BT nor ET (inputs 1 and 2)".into());
    }
    Ok(())
}

/// A Fuji PID's registers (Artisan's `pid_control.py`: `PXG4`, `PXR`, `PXF`) as Modbus
/// addresses: Artisan's 3xxxx and 4xxxx numbers less 30001 and 40001.
#[derive(Debug, Clone, Copy)]
pub(super) struct Fuji {
    pub(super) model: &'static str,
    /// The process value: an input register, × 0.1
    pub(super) pv: i32,
    /// The setpoint in use: an input register, × 0.1
    pub(super) sv: i32,
    /// The output, −3 to 103 %: an input register, × 0.01, signed
    pub(super) mv: i32,
    /// The setpoint Artisan writes (a holding register, × 0.1) and its name on the controller
    pub(super) sv_write: i32,
    pub(super) sv_name: &'static str,
}

/// Artisan's PID types: 0 PXG, 1 PXR, 4 PXF (2 is none, 3 a Delta DTA).
pub(super) fn fuji(t: i64) -> Option<Fuji> {
    Some(match t {
        0 => Fuji {
            model: "PXG",
            pv: 1000,
            sv: 1001,
            mv: 1041,
            sv_write: 1240,
            sv_name: "SV1",
        },
        1 => Fuji {
            model: "PXR",
            pv: 1000,
            sv: 1001,
            mv: 1003,
            sv_write: 1002,
            sv_name: "SV",
        },
        4 => Fuji {
            model: "PXF",
            pv: 2000,
            sv: 2001,
            mv: 2041,
            sv_write: 2240,
            sv_name: "SV1",
        },
        _ => return None,
    })
}

/// `controlETpid` or `readBTpid`: the PID's type and its slave.
pub(super) fn fuji_pid(ini: &Ini, key: &str) -> Option<(i64, u8)> {
    let l = ini.list("Device", key);
    let t: i64 = l.first()?.trim().parse().ok()?;
    let u: i64 = l.get(1)?.trim().parse().ok()?;
    (1..=247).contains(&u).then_some((t, u as u8))
}

fn fuji_supported(ini: &Ini) -> Result<(), String> {
    const DTA: &str = "a Delta DTA on Artisan's own serial protocol, which Roastwarden does not \
                       speak";
    match fuji_pid(ini, "controlETpid") {
        Some((t, _)) if fuji(t).is_some() => {}
        Some((3, _)) => return Err(format!("its ET controller is {DTA}")),
        _ => return Err("its setup names no Fuji controller for ET (controlETpid)".into()),
    }
    if let Some((3, _)) = fuji_pid(ini, "readBTpid") {
        return Err(format!("its BT controller is {DTA}"));
    }
    if ini.int("SerialPort", "bytesize").is_some_and(|b| b != 8) {
        return Err("its serial frames are not 8 data bits; Modbus RTU needs 8".into());
    }
    Ok(())
}
