//! What a setup reads: Artisan's twelve Modbus inputs, its extra devices and curves, and
//! the readings they become (BT, ET, the setpoint, the channels).

use std::collections::BTreeMap;

use crate::roasters::channels::Channel;
use crate::roasters::types::ModbusDataType;

use super::names::{key_of, output_role, setpoint_name, temp_name, unit_in};
use crate::roasters::aset::ini::Ini;
use crate::roasters::aset::machines::device_name;

/// One of Artisan's twelve Modbus inputs.
#[derive(Debug, Clone)]
pub(crate) struct Input {
    pub(crate) n: usize,
    pub(crate) unit: u8,
    pub(crate) register: i64,
    pub(crate) code: i64,
    pub(crate) div: i64,
    pub(crate) mode: String,
    pub(crate) float: bool,
    pub(crate) int32: bool,
    /// Binary-coded decimal in two registers (`inputNbcd`)
    pub(crate) bcd32: bool,
    /// Binary-coded decimal in one register (`inputNBCDsAsInt`)
    pub(crate) bcd16: bool,
    pub(crate) signed: bool,
}

impl Input {
    /// Input `n` (1–12) when it is in use (a slave other than 0).
    pub(crate) fn read(ini: &Ini, n: usize) -> Option<Input> {
        let k = |s: &str| format!("input{n}{s}");
        let unit = ini.int("Modbus", &k("deviceId")).unwrap_or(0);
        if !(1..=247).contains(&unit) {
            return None;
        }
        Some(Input {
            n,
            unit: unit as u8,
            register: ini.int("Modbus", &k("register")).unwrap_or(0),
            code: ini.int("Modbus", &k("code")).unwrap_or(3),
            div: ini.int("Modbus", &k("div")).unwrap_or(0),
            mode: ini.text("Modbus", &k("mode")).unwrap_or_else(|| "C".into()),
            float: ini.flag("Modbus", &k("float")),
            int32: ini.flag("Modbus", &k("FloatsAsInt")),
            bcd32: ini.flag("Modbus", &k("bcd")),
            bcd16: ini.flag("Modbus", &k("BCDsAsInt")),
            signed: ini.flag("Modbus", &k("Signed")),
        })
    }

    /// Its data type, as Artisan reads it: float, then 32-bit integer, then BCD.
    pub(crate) fn data_type(&self) -> ModbusDataType {
        if self.float {
            ModbusDataType::Float32
        } else if self.int32 && self.signed {
            ModbusDataType::Int32
        } else if self.int32 {
            ModbusDataType::Uint32
        } else if self.bcd32 {
            ModbusDataType::Bcd32
        } else if self.bcd16 {
            ModbusDataType::Bcd16
        } else if self.signed {
            ModbusDataType::Int16
        } else {
            ModbusDataType::Uint16
        }
    }
}

/// Artisan's extra devices that read two more channels of the main device: (device, the first
/// curve's channel, the second's). The first curve (`extraname1`) shows the lower channel: the
/// device returns the pair high first and Artisan's `sample()` unpacks it the other way round
/// (`extratx, extrat2, extrat1`).
pub(super) const EXTRAS_MODBUS: [(i64, usize, usize); 5] = [
    (33, 3, 4),
    (55, 5, 6),
    (109, 7, 8),
    (150, 9, 10),
    (207, 11, 12),
];
pub(super) const EXTRAS_WEBSOCKET: [(i64, usize, usize); 4] =
    [(112, 3, 4), (113, 5, 6), (118, 7, 8), (119, 9, 10)];
pub(super) const EXTRAS_MQTT: [(i64, usize, usize); 5] = [
    (202, 3, 4),
    (203, 5, 6),
    (204, 7, 8),
    (205, 9, 10),
    (206, 11, 12),
];
/// A TC4's channels 3 and 4 (`TC4 34`); 32 (its heater and fan) and 44 (its SV and ambient)
/// are in every TC4 reply already.
pub(super) const EXTRAS_TC4: [(i64, usize, usize); 1] = [(28, 3, 4)];

/// Artisan's default event types, when the setup names none.
const ETYPES: [&str; 5] = ["Air", "Drum", "Damper", "Burner", "--"];

/// The setup's event types: the sliders' names, and `{n}` in a curve's name.
pub(super) struct Etypes(Vec<String>);

impl Etypes {
    pub(super) fn read(ini: &Ini) -> Etypes {
        let mut etypes: Vec<String> = ini.list("events", "etypes");
        for (i, d) in ETYPES.iter().enumerate() {
            match etypes.get_mut(i) {
                Some(e) if e.trim().is_empty() => *e = d.to_string(),
                Some(_) => {}
                None => etypes.push(d.to_string()),
            }
        }
        Etypes(etypes)
    }

    pub(super) fn get(&self, i: usize) -> String {
        self.0
            .get(i)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    /// A curve's name with `{n}` filled in.
    pub(super) fn named(&self, raw: &str) -> String {
        let mut s = raw.to_string();
        for (i, e) in self.0.iter().enumerate() {
            s = s.replace(&format!("{{{i}}}"), e.trim());
        }
        s.trim().to_string()
    }

    /// `{n}` alone: the event type's read-back.
    pub(super) fn template(&self, raw: &str) -> Option<usize> {
        (0..self.0.len()).find(|i| raw.trim() == format!("{{{i}}}"))
    }

    /// The event type a name mentions.
    pub(super) fn mentioned(&self, raw: &str) -> Option<usize> {
        (0..self.0.len()).find(|i| raw.contains(&format!("{{{i}}}")))
    }
}

/// The extra curves on the main device's channels: channel → (Artisan's name, its formula).
/// Other extra devices are noted, but for Artisan's formula curves and `same` (the main
/// device's own readings, as a TC4's heater and fan).
pub(super) fn extra_curves(
    ini: &Ini,
    et: &Etypes,
    pairs: &[(i64, usize, usize)],
    same: &[i64],
    notes: &mut Vec<String>,
) -> BTreeMap<usize, (String, String)> {
    let extras = ini.list("ExtraDev", "extradevices");
    let names1 = ini.list("ExtraDev", "extraname1");
    let names2 = ini.list("ExtraDev", "extraname2");
    let maths1 = ini.list("ExtraDev", "extramathexpression1");
    let maths2 = ini.list("ExtraDev", "extramathexpression2");
    let mut shown: BTreeMap<usize, (String, String)> = BTreeMap::new();
    let mut other: Vec<String> = Vec::new();
    for (i, d) in extras.iter().enumerate() {
        let Ok(d) = d.trim().parse::<i64>() else {
            continue;
        };
        let at = |l: &Vec<String>| l.get(i).cloned().unwrap_or_default();
        if let Some((_, first, second)) = pairs.iter().find(|(id, ..)| *id == d) {
            shown.insert(*first, (at(&names1), at(&maths1)));
            shown.insert(*second, (at(&names2), at(&maths2)));
        } else if same.contains(&d) {
        } else if d != 25 && d != 50 && d != 92 {
            other.push(format!("{} ({})", device_name(d), et.named(&at(&names1))));
        } else if d == 25 && !(at(&names1).trim().is_empty() && at(&names2).trim().is_empty()) {
            other.push(format!(
                "a formula curve ({}{})",
                et.named(&at(&names1)),
                if at(&names2).trim().is_empty() {
                    String::new()
                } else {
                    format!(", {}", et.named(&at(&names2)))
                }
            ));
        }
    }
    if !other.is_empty() {
        notes.push(format!(
            "left out, not read through this connection: {}",
            other.join("; ")
        ));
    }
    shown
}

/// The reading side of a plan as it is built: the roles read, the channels declared and every
/// name taken (a register's or a field's, whatever its case).
#[derive(Default)]
pub(super) struct Reads {
    pub(super) roles: Vec<&'static str>,
    pub(super) channels: Vec<Channel>,
    pub(super) used: Vec<String>,
}

/// What one of Artisan's curves becomes.
pub(super) struct Curve {
    /// Its name, `{n}` filled in
    pub(super) name: String,
    /// An output's read-back (heater, fan, drum, damper) or the ambient temperature
    pub(super) role: Option<&'static str>,
    /// A temperature (°C, or the device's unit for a role)
    pub(super) temp: bool,
    /// The declared channel's key, when it is a channel
    pub(super) key: Option<String>,
    /// Artisan's formula when it is only a scale (`x*1000`): applied
    pub(super) scale: Option<f64>,
}

impl Reads {
    /// Decide what Artisan's curve `raw` (its name, `{n}` an event type) is. `temp_mode`: the
    /// device marks it a temperature (a Modbus input's °C or °F, a WebSocket or MQTT channel's
    /// mode); `hint`: where it is read (an MQTT path naming `rpm`); `n`: its input or channel,
    /// for an unnamed curve (`{unnamed} {n}`) and a key taken twice.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn curve(
        &mut self,
        ini: &Ini,
        et: &Etypes,
        raw: &str,
        math: &str,
        temp_mode: bool,
        hint: &str,
        n: usize,
        unnamed: &str,
        notes: &mut Vec<String>,
    ) -> Curve {
        let mut name = et.named(raw);
        if name.is_empty() || name == "--" {
            name = format!("{unnamed} {n}");
        }
        let percent_named = raw.contains('%') || et.template(raw).is_some();
        let hint_unit = unit_in(hint).filter(|u| *u != "%");
        let rate = rate_name(&name);
        // An event type's read-back (`{3}` is the Burner), an output by its name, else a
        // temperature or a channel in its own unit.
        let role = if setpoint_name(&name) || hint_unit.is_some() || rate {
            None
        } else if percent_named || (!temp_name(&name) && unit_in(&name).is_none()) {
            output_role(&name).filter(|_| percent_named || !temp_mode)
        } else {
            None
        }
        .or_else(|| {
            let n = name.to_lowercase();
            (n.contains("ambient") || n.contains("room"))
                .then_some("ambient")
                .filter(|_| temp_mode)
        });
        let role = role.filter(|r| !self.roles.contains(r));
        let temp = !rate
            && (role == Some("ambient")
                || (role.is_none()
                    && unit_in(&name).is_none()
                    && hint_unit.is_none()
                    && !percent_named
                    && (temp_name(&name) || temp_mode)));
        let math = math.trim();
        let scale = if math.is_empty() || temp {
            None
        } else {
            formula_scale(math)
        };
        if !math.is_empty() {
            notes.push(match scale {
                Some(_) => {
                    format!("{name}: Artisan's formula `{math}` is a scale; Roastwarden applies it")
                }
                None => format!(
                    "{name}: Artisan applies the formula `{math}` to it; Roastwarden keeps the raw \
                     value"
                ),
            });
        }
        if let Some(r) = role {
            self.roles.push(r);
            return Curve {
                name,
                role: Some(r),
                temp,
                key: None,
                scale,
            };
        }
        // The channel's key is its register's or field's name too, so it is unique among both.
        let mut key = key_of(&name);
        while !self.key_free(&key) {
            key = format!("{key}_{n}").chars().take(32).collect();
        }
        self.used.push(key.clone());
        let unit = if temp {
            "C".to_string()
        } else if rate {
            "C/min".to_string()
        } else if let Some(u) = unit_in(&name).or(hint_unit) {
            u.to_string()
        } else if let Some(t) = et.mentioned(raw) {
            let u = ini
                .list("Sliders", "eventsliderunits")
                .get(t)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if u.is_empty() || u.chars().count() > 8 {
                "%".into()
            } else {
                u
            }
        } else {
            "#".into()
        };
        self.channels.push(Channel {
            key: key.clone(),
            label: name.chars().take(24).collect(),
            unit,
            max: None,
        });
        Curve {
            name,
            role: None,
            temp,
            key: Some(key),
            scale,
        }
    }

    /// A key no register, field, role or sample field has.
    pub(super) fn key_free(&self, key: &str) -> bool {
        !self.channels.iter().any(|c| c.key == key)
            && !self.used.iter().any(|u| u.eq_ignore_ascii_case(key))
            && crate::transports::ingest::Role::parse(key).is_none()
            && !crate::transports::ingest::is_reserved(key)
            && !crate::roasters::channels::is_reserved(key)
    }

    /// At most `MAX_CHANNELS` channels: the rest are noted and dropped; the keys kept.
    pub(super) fn cap(&mut self, notes: &mut Vec<String>) -> Vec<String> {
        let max = crate::roasters::channels::MAX_CHANNELS;
        if self.channels.len() > max {
            notes.push(format!(
                "{} extra curves: the first {max} are channels, the rest left out ({})",
                self.channels.len(),
                self.channels[max..]
                    .iter()
                    .map(|c| c.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            self.channels.truncate(max);
        }
        self.channels.iter().map(|c| c.key.clone()).collect()
    }
}

/// A formula that is only a scale: `x*1000`, `x/10`, `0.1*x` (Artisan's `extramathexpression`).
pub(super) fn formula_scale(f: &str) -> Option<f64> {
    let f: String = f
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();
    let num = |k: &str| k.parse::<f64>().ok();
    let k = match (
        f.strip_prefix("x*"),
        f.strip_prefix("x/"),
        f.strip_suffix("*x"),
    ) {
        (Some(k), _, _) | (_, _, Some(k)) => num(k)?,
        (_, Some(k), _) => 1.0 / num(k)?,
        _ => return None,
    };
    (k.is_finite() && k != 0.0).then_some(k)
}

/// A rate of rise by its name (`RoR`, `ET RoR`): °C a minute, not a temperature.
pub(super) fn rate_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.split(|c: char| !c.is_alphanumeric()).any(|w| w == "ror")
        || n.contains("rate of rise")
        || n.contains("/min")
}
