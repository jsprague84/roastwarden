//! Ingest mapping and command templates (`docs/ingest-mapping.md`): a device
//! speaks its own field names, units, topics and command syntax, and configuration adapts the
//! server to it.
//!
//! Two optional keys on a connection's config, the same shape for every protocol:
//!
//! - `ingest`: how to read the device's telemetry into today's field names ([`Ingest`]):
//!   JSON paths for MQTT and the WebSocket client, column positions for a TC4, units (°F);
//! - `commands`: how to write each command in the device's syntax ([`Commands`]).
//!
//! The mapping produces the JSON `process_telemetry` gets today (`beanTemp`, `envTemp`, …
//! in °C), so nothing downstream changes. Everything here is pure: the MQTT route table
//! ([`MqttRouter`]) holds the parsed routes and the per-field topics' newest values, and the
//! transports call [`render`] for their commands.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use roastwarden_core::{ControlMode, DeviceCommand};
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Roles and units
// ---------------------------------------------------------------------------

/// What a value means. The mapping, the TC4 columns and the Modbus register map use the same
/// words (docs/ingest-mapping.md, "Roles").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Bt,
    Et,
    Ambient,
    Ror,
    Heater,
    HeaterApplied,
    Fan,
    Drum,
    Damper,
    Setpoint,
    Mode,
    HeaterEnable,
    /// A Modbus write role only.
    EmergencyStop,
}

/// Every role, by its name.
pub const ROLES: [(&str, Role); 13] = [
    ("bt", Role::Bt),
    ("et", Role::Et),
    ("ambient", Role::Ambient),
    ("ror", Role::Ror),
    ("heater", Role::Heater),
    ("heater_applied", Role::HeaterApplied),
    ("fan", Role::Fan),
    ("drum", Role::Drum),
    ("damper", Role::Damper),
    ("setpoint", Role::Setpoint),
    ("mode", Role::Mode),
    ("heater_enable", Role::HeaterEnable),
    ("emergency_stop", Role::EmergencyStop),
];

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        ROLES.iter().find(|(n, _)| *n == s).map(|(_, r)| *r)
    }

    /// The telemetry field the role becomes.
    pub fn field(self) -> &'static str {
        match self {
            Role::Bt => "beanTemp",
            Role::Et => "envTemp",
            Role::Ambient => "ambientTemp",
            Role::Ror => "rateOfRise",
            Role::Heater => "heater",
            Role::HeaterApplied => "heaterApplied",
            Role::Fan => "fan",
            Role::Drum => "drum",
            Role::Damper => "damper",
            Role::Setpoint => "setpoint",
            Role::Mode => "controlMode",
            Role::HeaterEnable => "heaterEnable",
            Role::EmergencyStop => "emergencyStop",
        }
    }

    /// A temperature, converted from °F when the device reports °F.
    pub fn is_temperature(self) -> bool {
        matches!(self, Role::Bt | Role::Et | Role::Ambient | Role::Setpoint)
    }

    /// Read from a device (everything but emergency stop).
    pub fn readable(self) -> bool {
        self != Role::EmergencyStop
    }

    /// The command's role (the Modbus register it writes), if it has one.
    pub fn of_command(cmd: &DeviceCommand) -> Option<Role> {
        Some(match cmd {
            DeviceCommand::Setpoint { .. } => Role::Setpoint,
            DeviceCommand::Heater { .. } => Role::Heater,
            DeviceCommand::Fan { .. } => Role::Fan,
            DeviceCommand::Drum { .. } => Role::Drum,
            DeviceCommand::Damper { .. } => Role::Damper,
            DeviceCommand::Mode { .. } => Role::Mode,
            DeviceCommand::HeaterEnable { .. } => Role::HeaterEnable,
            DeviceCommand::EmergencyStop => Role::EmergencyStop,
            DeviceCommand::Pid { .. }
            | DeviceCommand::Feedforward { .. }
            | DeviceCommand::Lease { .. }
            | DeviceCommand::RorLimit { .. } => return None,
        })
    }
}

/// The device's temperature unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Units {
    #[default]
    C,
    F,
}

impl Units {
    /// `C` or `F` (a Modbus register's unit may also say `°F`, `degF`).
    pub fn parse(s: &str) -> Option<Units> {
        match s
            .trim()
            .trim_start_matches('°')
            .to_ascii_lowercase()
            .as_str()
        {
            "c" | "degc" | "celsius" => Some(Units::C),
            "f" | "degf" | "fahrenheit" => Some(Units::F),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Units::C => "C",
            Units::F => "F",
        }
    }
}

pub fn f_to_c(v: f64) -> f64 {
    (v - 32.0) * 5.0 / 9.0
}

pub fn c_to_f(v: f64) -> f64 {
    v * 9.0 / 5.0 + 32.0
}

/// Field names an extra channel may not take: the roles' fields and what the server adds.
const RESERVED: [&str; 17] = [
    "beanTemp",
    "envTemp",
    "ambientTemp",
    "rateOfRise",
    "heaterApplied",
    "setpoint",
    "controlMode",
    "heaterEnable",
    "emergencyStop",
    "rateOfRiseEt",
    "deviceRateOfRise",
    "timestamp",
    "uptime",
    "uptimeMs",
    "heater",
    "fan",
    "drum",
];

/// The most fields (or columns) a mapping has.
pub const MAX_FIELDS: usize = 32;

/// A key that is not a role: an extra channel's name (`t3`, `exhaust`, `burnerGas`).
fn check_extra(key: &str) -> Result<(), String> {
    let mut chars = key.chars();
    let ok = key.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(format!(
            "'{key}' is neither a role ({}) nor an extra channel's name (a letter, then up to 31 \
             letters, digits or _)",
            ROLES
                .iter()
                .filter(|(_, r)| r.readable())
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if RESERVED.contains(&key) || key == "damper" {
        return Err(format!(
            "'{key}' is a field Roastwarden fills itself; map it by its role instead"
        ));
    }
    Ok(())
}

/// Whether `key` is a field Roastwarden fills itself (an extra channel may not take it).
pub(crate) fn is_reserved(key: &str) -> bool {
    RESERVED.contains(&key) || key == "damper"
}

/// Whether a JSON path is one the mapping reads (`data.BT`, `probes[0].value`), else why not.
pub(crate) fn check_path(p: &str) -> Result<(), String> {
    parse_path(p).map(|_| ())
}

/// The field a key becomes and its role (`None`: an extra channel keeping its name).
fn target_of(key: &str) -> Result<(String, Option<Role>), String> {
    match Role::parse(key) {
        Some(Role::EmergencyStop) => {
            Err("'emergency_stop' is a command, not something a device reports".into())
        }
        Some(r) => Ok((r.field().to_string(), Some(r))),
        None => check_extra(key).map(|()| (key.to_string(), None)),
    }
}

// ---------------------------------------------------------------------------
// JSON paths
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// `data.BT`, `probes[0].value`, `$.data.BT`, `$` or an empty path (the whole payload).
fn parse_path(p: &str) -> Result<Vec<Seg>, String> {
    let bad = |why: &str| format!("path '{p}': {why}");
    if p.len() > 256 {
        return Err(bad("longer than 256 characters"));
    }
    let mut rest = p.trim();
    if let Some(r) = rest.strip_prefix('$') {
        rest = r.strip_prefix('.').unwrap_or(r);
    }
    let mut segs = Vec::new();
    if rest.is_empty() {
        return Ok(segs);
    }
    let mut key = String::new();
    let mut chars = rest.chars().peekable();
    // After `]`, only `.`, `[` or the end may follow.
    let mut after_index = false;
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if key.is_empty() && !after_index {
                    return Err(bad(
                        "an empty key (a key that contains a dot is not reachable)",
                    ));
                }
                if !key.is_empty() {
                    segs.push(Seg::Key(std::mem::take(&mut key)));
                }
                after_index = false;
                if chars.peek().is_none() {
                    return Err(bad("it ends with a dot"));
                }
            }
            '[' => {
                if !key.is_empty() {
                    segs.push(Seg::Key(std::mem::take(&mut key)));
                }
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        Some(']') => break,
                        Some(d) if d.is_ascii_digit() => digits.push(d),
                        _ => return Err(bad("an index is [n], n a whole number")),
                    }
                }
                let n = digits
                    .parse::<usize>()
                    .map_err(|_| bad("an index is [n], n a whole number"))?;
                segs.push(Seg::Index(n));
                after_index = true;
            }
            ']' => return Err(bad("a ] with no [")),
            c => {
                if after_index {
                    return Err(bad("a key right after an index needs a dot"));
                }
                key.push(c);
            }
        }
    }
    if !key.is_empty() {
        segs.push(Seg::Key(key));
    }
    Ok(segs)
}

fn lookup<'a>(v: &'a Value, path: &[Seg]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, seg| match seg {
        Seg::Key(k) => v.get(k.as_str()),
        Seg::Index(i) => v.get(*i),
    })
}

/// A number, a string that parses as one (or says on/off, true/false), or a boolean (1 / 0);
/// anything else is no value.
fn number(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => {
            let s = s.trim();
            match s.to_ascii_lowercase().as_str() {
                "on" | "true" => Some(1.0),
                "off" | "false" => Some(0.0),
                _ => s.parse::<f64>().ok(),
            }
        }
        _ => None,
    };
    n.filter(|x| x.is_finite())
}

/// A message's payload as JSON; text that is not JSON (`ON`, ` 415.2 `) as a string.
pub fn parse_payload(bytes: &[u8]) -> Option<Value> {
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return Some(v);
    }
    std::str::from_utf8(bytes)
        .ok()
        .map(|s| Value::String(s.trim().to_string()))
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------------------
// The mapping
// ---------------------------------------------------------------------------

/// Where the config sits: what an `ingest` may hold there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mqtt,
    WebSocket,
    Tc4,
    /// A Phidgets board (docs/phidgets.md): `fields` whose paths are its inputs, `ch0`–`ch7`.
    Phidget,
}

/// A Phidgets board's temperature input named in a mapping: `ch0` to `ch7`.
pub fn phidget_input(path: &str) -> Option<u8> {
    let n: u8 = path.strip_prefix("ch")?.parse().ok()?;
    (n < 8 && path.len() == 3).then_some(n)
}

/// One mapped field.
#[derive(Debug, Clone)]
pub struct Field {
    /// The key in the config: a role's name or an extra's.
    pub key: String,
    /// The path as written (`data.BT`).
    pub path_text: String,
    pub role: Option<Role>,
    /// The telemetry field it becomes (`beanTemp`, `exhaust`).
    pub target: String,
    path: Vec<Seg>,
    scale: f64,
    offset: f64,
    /// An extra channel's own unit (converted from °F when `F`).
    unit: Option<Units>,
    /// MQTT only: the topic it is read from (none: the telemetry topic).
    pub topic: Option<String>,
}

impl Field {
    /// `raw × scale + offset`, then to °C.
    fn value(&self, raw: f64, units: Units) -> Value {
        let v = raw * self.scale + self.offset;
        let v = match self.role {
            Some(Role::Mode | Role::HeaterEnable) => {
                return json!(i32::from(v != 0.0));
            }
            Some(r) if r.is_temperature() && units == Units::F => f_to_c(v),
            Some(Role::Ror) if units == Units::F => v * 5.0 / 9.0,
            None if self.unit == Some(Units::F) => f_to_c(v),
            _ => v,
        };
        json!(round3(v))
    }

    fn read(&self, payload: &Value, units: Units) -> Option<Value> {
        lookup(payload, &self.path)
            .and_then(number)
            .map(|raw| self.value(raw, units))
    }
}

/// A TC4 reply's column: the field it becomes, or skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub target: String,
    pub role: Option<Role>,
    /// An extra column's own unit (`exhaust:F`); `None` follows the roaster's declared
    /// channel: a °C channel is in `ingest.units`, anything else as printed
    pub unit: Option<ColumnUnit>,
}

/// What an extra TC4 column's `:unit` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnUnit {
    /// A temperature in this unit, kept in °C
    Temp(Units),
    /// `none`: kept as printed
    AsPrinted,
}

impl ColumnUnit {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "C" => Some(ColumnUnit::Temp(Units::C)),
            "F" => Some(ColumnUnit::Temp(Units::F)),
            "none" => Some(ColumnUnit::AsPrinted),
            _ => None,
        }
    }
}

/// How to read a device's telemetry into today's field names.
#[derive(Debug, Clone, Default)]
pub struct Ingest {
    pub units: Units,
    /// JSON (MQTT, the WebSocket client).
    pub fields: Vec<Field>,
    /// A TC4's reply by position (`None`: a column skipped).
    pub columns: Option<Vec<Option<Column>>>,
}

fn object<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object()
        .ok_or_else(|| format!("{what} must be a JSON object"))
}

fn unknown_keys(m: &Map<String, Value>, what: &str, allowed: &[&str]) -> Result<(), String> {
    match m.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!(
            "{what}: unknown key '{k}' (it takes {})",
            allowed.join(", ")
        )),
        None => Ok(()),
    }
}

fn finite(v: &Value, what: &str) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what} must be a finite number"))
}

/// Read and check an `ingest` (the 400's message on failure).
pub fn parse_ingest(v: &Value, kind: Kind) -> Result<Ingest, String> {
    let m = object(v, "ingest")?;
    let allowed: &[&str] = match kind {
        Kind::Tc4 => &["units", "columns"],
        Kind::Mqtt | Kind::WebSocket => &["units", "fields"],
        // A Phidget reports °C, so there is no unit to give.
        Kind::Phidget => &["fields"],
    };
    unknown_keys(m, "ingest", allowed)?;
    let units = match m.get("units") {
        None | Some(Value::Null) => Units::C,
        Some(Value::String(s)) if s == "C" || s == "F" => Units::parse(s).unwrap_or_default(),
        Some(other) => return Err(format!("ingest.units is {other}: \"C\" or \"F\"")),
    };
    let mut out = Ingest {
        units,
        ..Ingest::default()
    };
    if kind == Kind::Tc4 {
        if let Some(cols) = m.get("columns").filter(|c| !c.is_null()) {
            out.columns = Some(parse_columns(cols)?);
        }
        return Ok(out);
    }
    let fields = object(
        m.get("fields")
            .ok_or("ingest.fields is missing: map at least bt or et")?,
        "ingest.fields",
    )?;
    if fields.is_empty() || fields.len() > MAX_FIELDS {
        return Err(format!(
            "ingest.fields has {} fields: 1 to {MAX_FIELDS}",
            fields.len()
        ));
    }
    for (key, spec) in fields {
        let f = parse_field(key, spec, kind)?;
        if kind == Kind::Phidget {
            if phidget_input(&f.path_text).is_none() {
                return Err(format!(
                    "ingest.fields.{key}: '{}' is not a board input: ch0 to ch7 (Phidgets' \
                     temperature channel number)",
                    f.path_text
                ));
            }
            if f.unit.is_some() {
                return Err(format!(
                    "ingest.fields.{key}: a Phidget reports °C, so a field takes no unit"
                ));
            }
        }
        out.fields.push(f);
    }
    if !out
        .fields
        .iter()
        .any(|f| matches!(f.role, Some(Role::Bt | Role::Et)))
    {
        return Err("ingest.fields must map bt or et (or both)".into());
    }
    Ok(out)
}

fn parse_field(key: &str, spec: &Value, kind: Kind) -> Result<Field, String> {
    let at = |m: String| format!("ingest.fields.{key}: {m}");
    let (target, role) = target_of(key).map_err(at)?;
    let mut f = Field {
        key: key.to_string(),
        path_text: String::new(),
        role,
        target,
        path: Vec::new(),
        scale: 1.0,
        offset: 0.0,
        unit: None,
        topic: None,
    };
    let path = match spec {
        Value::String(p) => p.clone(),
        Value::Object(m) => {
            unknown_keys(
                m,
                &format!("ingest.fields.{key}"),
                &["path", "scale", "offset", "unit", "topic"],
            )?;
            if let Some(s) = m.get("scale").filter(|v| !v.is_null()) {
                f.scale = finite(s, "scale").map_err(at)?;
                if f.scale == 0.0 {
                    return Err(at("scale must not be 0".into()));
                }
            }
            if let Some(o) = m.get("offset").filter(|v| !v.is_null()) {
                f.offset = finite(o, "offset").map_err(at)?;
            }
            if let Some(u) = m.get("unit").filter(|v| !v.is_null()) {
                if role.is_some() {
                    return Err(at(
                        "unit is for an extra channel; a role follows ingest.units".into(),
                    ));
                }
                f.unit = Some(
                    u.as_str()
                        .filter(|s| *s == "C" || *s == "F")
                        .and_then(Units::parse)
                        .ok_or_else(|| at("unit must be \"C\" or \"F\"".into()))?,
                );
            }
            if let Some(t) = m.get("topic").filter(|v| !v.is_null()) {
                if kind != Kind::Mqtt {
                    return Err(at("topic is for MQTT connections only".into()));
                }
                let t = t
                    .as_str()
                    .ok_or_else(|| at("topic must be a string".into()))?;
                check_topic(t).map_err(at)?;
                f.topic = Some(t.to_string());
            }
            match m.get("path") {
                Some(Value::String(p)) => p.clone(),
                None | Some(Value::Null) if f.topic.is_some() => "$".to_string(),
                None | Some(Value::Null) => {
                    return Err(at("a path is needed (or a topic of its own)".into()))
                }
                Some(_) => return Err(at("path must be a string".into())),
            }
        }
        _ => {
            return Err(at(
                "a path string or { path, scale, offset, unit, topic }".into()
            ))
        }
    };
    f.path = parse_path(&path).map_err(at)?;
    f.path_text = path;
    Ok(f)
}

fn parse_columns(v: &Value) -> Result<Vec<Option<Column>>, String> {
    let cols = v.as_array().ok_or(
        "ingest.columns must be a list, e.g. [\"ambient\", \"et\", \"bt\", \"heater\", \"fan\"]",
    )?;
    if cols.is_empty() || cols.len() > MAX_FIELDS {
        return Err(format!(
            "ingest.columns has {} columns: 1 to {MAX_FIELDS}",
            cols.len()
        ));
    }
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let text = c
            .as_str()
            .ok_or_else(|| format!("ingest.columns[{i}] must be a name, or \"\" to skip it"))?
            .trim();
        if text.is_empty() {
            out.push(None);
            continue;
        }
        // `exhaust:F`: an extra column's own unit
        let (name, unit) = match text.split_once(':') {
            None => (text, None),
            Some((name, u)) => {
                let unit = ColumnUnit::parse(u.trim()).ok_or_else(|| {
                    format!("ingest.columns[{i}]: '{text}': the unit after ':' is C, F or none")
                })?;
                (name.trim(), Some(unit))
            }
        };
        let (target, role) = target_of(name).map_err(|e| format!("ingest.columns[{i}]: {e}"))?;
        if unit.is_some() && role.is_some() {
            return Err(format!(
                "ingest.columns[{i}]: '{text}': a unit is for an extra column; {name} follows ingest.units"
            ));
        }
        if seen.contains(&target) {
            return Err(format!("ingest.columns[{i}]: '{name}' is in two columns"));
        }
        seen.push(target.clone());
        out.push(Some(Column { target, role, unit }));
    }
    if !out
        .iter()
        .flatten()
        .any(|c| matches!(c.role, Some(Role::Bt | Role::Et)))
    {
        return Err("ingest.columns must name bt or et (or both)".into());
    }
    Ok(out)
}

impl Ingest {
    /// Whether any field has a topic of its own (other than the telemetry topic).
    pub fn has_field_topics(&self, telemetry_topic: &str) -> bool {
        self.fields
            .iter()
            .any(|f| f.topic.as_deref().is_some_and(|t| t != telemetry_topic))
    }

    /// The fields read from the message on `topic` (the telemetry topic for a field with
    /// none), mapped: only those with a value.
    fn read_topic(
        &self,
        payload: &Value,
        topic: &str,
        telemetry_topic: &str,
    ) -> Map<String, Value> {
        self.fields
            .iter()
            .filter(|f| f.topic.as_deref().unwrap_or(telemetry_topic) == topic)
            .filter_map(|f| f.read(payload, self.units).map(|v| (f.target.clone(), v)))
            .collect()
    }

    /// One message, one sample: the mapped fields only (nothing else of the payload passes),
    /// or `None` when it carries neither BT nor ET.
    pub fn map(&self, payload: &Value) -> Option<Map<String, Value>> {
        let m: Map<String, Value> = self
            .fields
            .iter()
            .filter_map(|f| f.read(payload, self.units).map(|v| (f.target.clone(), v)))
            .collect();
        has_temperature(&m).then_some(m)
    }

    /// The role whose message cuts a sample from per-field topics: BT, or ET without BT.
    fn trigger(&self) -> &'static str {
        if self.fields.iter().any(|f| f.role == Some(Role::Bt)) {
            Role::Bt.field()
        } else {
            Role::Et.field()
        }
    }
}

fn has_temperature(m: &Map<String, Value>) -> bool {
    m.contains_key(Role::Bt.field()) || m.contains_key(Role::Et.field())
}

/// How long a per-field topic's value stays in samples.
pub const FIELD_MAX_AGE_MS: u64 = 10_000;

/// The newest value of each field read from per-field topics (ESPHome, Home Assistant style).
#[derive(Debug, Default)]
pub struct Assembler {
    latest: HashMap<String, (Value, u64)>,
}

impl Assembler {
    /// Take one message; a sample comes out when it carried BT (ET when nothing maps BT),
    /// from every field's newest value no older than 10 s.
    pub fn on_message(
        &mut self,
        ingest: &Ingest,
        topic: &str,
        telemetry_topic: &str,
        payload: &Value,
        now_ms: u64,
    ) -> Option<Map<String, Value>> {
        let read = ingest.read_topic(payload, topic, telemetry_topic);
        let cut = read.contains_key(ingest.trigger());
        for (k, v) in read {
            self.latest.insert(k, (v, now_ms));
        }
        if !cut {
            return None;
        }
        self.latest
            .retain(|_, (_, at)| now_ms.saturating_sub(*at) <= FIELD_MAX_AGE_MS);
        let m: Map<String, Value> = self
            .latest
            .iter()
            .map(|(k, (v, _))| (k.clone(), v.clone()))
            .collect();
        has_temperature(&m).then_some(m)
    }
}

// ---------------------------------------------------------------------------
// TC4 columns
// ---------------------------------------------------------------------------

/// A TC4 reply read by `columns`: each column the field it names; `-1` (or anything not a
/// number) is no value. Temperatures are rounded to 0.1 and converted from °F with `units`
/// F. An extra column is a temperature when it says so (`exhaust:F`, `exhaust:C`) or, saying
/// nothing, when the roaster declares it a °C channel (`temp_channels`, docs/channels.md);
/// otherwise (`:none`, or another unit) it is kept as printed. `None` without BT or ET.
pub fn read_columns(
    line: &str,
    columns: &[Option<Column>],
    units: Units,
    temp_channels: &[String],
) -> Option<Map<String, Value>> {
    let mut out = Map::new();
    for (raw, col) in line.split(',').zip(columns) {
        let Some(col) = col else { continue };
        let Some(v) = raw.trim().parse::<f64>().ok().filter(|v| v.is_finite()) else {
            continue;
        };
        let temp_in = match (col.role, col.unit) {
            (Some(r), _) if r.is_temperature() => Some(units),
            (None, Some(ColumnUnit::Temp(u))) => Some(u),
            (None, None) if temp_channels.contains(&col.target) => Some(units),
            _ => None,
        };
        let value = match col.role {
            _ if temp_in.is_some() => {
                if v <= -1.0 {
                    continue; // no probe
                }
                let c = if temp_in == Some(Units::F) {
                    f_to_c(v)
                } else {
                    v
                };
                json!((c * 10.0).round() / 10.0)
            }
            Some(Role::Ror) if units == Units::F => json!(round3(v * 5.0 / 9.0)),
            Some(Role::Heater | Role::HeaterApplied | Role::Fan | Role::Drum | Role::Damper) => {
                json!(v.clamp(0.0, 100.0))
            }
            Some(Role::Mode | Role::HeaterEnable) => json!(i32::from(v != 0.0)),
            None if v == -1.0 => continue,
            _ => json!(v),
        };
        out.insert(col.target.clone(), value);
    }
    has_temperature(&out).then_some(out)
}

// ---------------------------------------------------------------------------
// Command templates
// ---------------------------------------------------------------------------

/// The commands a template may be given; PID gains, feedforward, the lease and the RoR limit
/// are the contract's.
pub const COMMAND_KEYS: [&str; 8] = [
    "setpoint",
    "heater",
    "fan",
    "drum",
    "damper",
    "mode",
    "heater_enable",
    "emergency_stop",
];

/// How one command is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// MQTT only: the topic (default `<prefix>/control/<command>`).
    pub topic: Option<String>,
    /// With `{value}` (default: the value alone; mode its word, `auto` / `manual`).
    pub payload: Option<String>,
    pub scale: f64,
    pub decimals: u8,
}

/// A connection's templates, by command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Commands(BTreeMap<String, Template>);

impl Commands {
    pub fn get(&self, command: &str) -> Option<&Template> {
        self.0.get(command)
    }
}

/// Read and check `commands` (the 400's message on failure).
pub fn parse_commands(v: &Value, kind: Kind) -> Result<Commands, String> {
    let m = object(v, "commands")?;
    let mut out = BTreeMap::new();
    for (key, spec) in m {
        if !COMMAND_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "commands: '{key}' takes no template (it takes {}; PID gains, feedforward, the \
                 lease and the RoR limit are the contract's)",
                COMMAND_KEYS.join(", ")
            ));
        }
        let at = |m: String| format!("commands.{key}: {m}");
        let t = object(spec, &format!("commands.{key}"))?;
        unknown_keys(
            t,
            &format!("commands.{key}"),
            &["topic", "payload", "scale", "decimals"],
        )?;
        let mut tpl = Template {
            topic: None,
            payload: None,
            scale: 1.0,
            decimals: 1,
        };
        if let Some(p) = t.get("payload").filter(|v| !v.is_null()) {
            let p = p
                .as_str()
                .ok_or_else(|| at("payload must be a string".into()))?;
            let n = p.chars().count();
            if !(1..=256).contains(&n) {
                return Err(at(format!("payload is {n} characters: 1 to 256")));
            }
            tpl.payload = Some(p.to_string());
        }
        if let Some(s) = t.get("scale").filter(|v| !v.is_null()) {
            tpl.scale = finite(s, "scale").map_err(at)?;
            if tpl.scale == 0.0 {
                return Err(at("scale must not be 0".into()));
            }
        }
        if let Some(d) = t.get("decimals").filter(|v| !v.is_null()) {
            tpl.decimals =
                d.as_u64()
                    .filter(|d| *d <= 3)
                    .ok_or_else(|| at("decimals must be 0 to 3".into()))? as u8;
        }
        if let Some(topic) = t.get("topic").filter(|v| !v.is_null()) {
            if kind != Kind::Mqtt {
                return Err(at("topic is for MQTT connections only".into()));
            }
            let topic = topic
                .as_str()
                .ok_or_else(|| at("topic must be a string".into()))?;
            check_topic(topic).map_err(at)?;
            tpl.topic = Some(topic.to_string());
        }
        out.insert(key.clone(), tpl);
    }
    Ok(Commands(out))
}

/// The command in the device's unit: a setpoint and the RoR limit's temperature in °F, its
/// rate × 9/5, with `F`.
pub fn in_device_units(cmd: &DeviceCommand, units: Units) -> DeviceCommand {
    match (cmd, units) {
        (DeviceCommand::Setpoint { value }, Units::F) => DeviceCommand::Setpoint {
            value: round3(c_to_f(*value)),
        },
        (DeviceCommand::RorLimit { max, min_temp }, Units::F) => DeviceCommand::RorLimit {
            max: round3(max * 9.0 / 5.0),
            min_temp: round3(c_to_f(*min_temp)),
        },
        _ => cmd.clone(),
    }
}

/// A command's `{value}`: heater, fan, drum, damper in %; setpoint as given; mode 0 manual /
/// 1 auto; heater enable 1 / 0; emergency stop 1.
pub fn command_value(cmd: &DeviceCommand) -> Option<f64> {
    Some(match cmd {
        DeviceCommand::Setpoint { value } => *value,
        DeviceCommand::Heater { percent }
        | DeviceCommand::Fan { percent }
        | DeviceCommand::Drum { percent }
        | DeviceCommand::Damper { percent } => *percent,
        DeviceCommand::Mode { mode } => mode.as_u16() as f64,
        DeviceCommand::HeaterEnable { enabled } => {
            if *enabled {
                1.0
            } else {
                0.0
            }
        }
        DeviceCommand::EmergencyStop => 1.0,
        _ => return None,
    })
}

/// `v` to `decimals` places, trailing zeros (and a bare dot) dropped: 62 → `62`, 62.5 → `62.5`.
pub fn format_value(v: f64, decimals: u8) -> String {
    // Half away from zero (62.5 → 63), not `format!`'s half to even.
    let p = 10f64.powi(decimals as i32);
    let s = format!("{:.*}", decimals as usize, (v * p).round() / p);
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// The payload a template writes for `cmd` (already in the device's unit).
pub fn render(t: &Template, cmd: &DeviceCommand) -> String {
    let value = command_value(cmd)
        .map(|v| format_value(v * t.scale, t.decimals))
        .unwrap_or_default();
    match (&t.payload, cmd) {
        (Some(p), _) => p.replace("{value}", &value),
        (None, DeviceCommand::Mode { mode }) => match mode {
            ControlMode::Auto => "auto".into(),
            ControlMode::Manual => "manual".into(),
        },
        (None, _) => value,
    }
}

// ---------------------------------------------------------------------------
// Topics and the MQTT route table
// ---------------------------------------------------------------------------

/// A topic a connection names: 1–256 characters, no wildcards.
pub fn check_topic(t: &str) -> Result<(), String> {
    let n = t.chars().count();
    if !(1..=256).contains(&n) {
        return Err(format!("topic '{t}' is {n} characters: 1 to 256"));
    }
    if t.contains('+') || t.contains('#') {
        return Err(format!(
            "topic '{t}' has a wildcard (+ or #): name one topic"
        ));
    }
    if t.trim() != t || t.contains('\0') {
        return Err(format!(
            "topic '{t}' starts or ends with a space or holds a NUL"
        ));
    }
    Ok(())
}

/// An MQTT connection as the server reads it (docs/ingest-mapping.md, "MQTT").
#[derive(Debug, Clone)]
pub struct MqttRoute {
    /// The roaster's id (`roaster/<id>/…`).
    pub device_id: String,
    /// Commands go to `<prefix>/control/<command>`.
    pub prefix: String,
    /// The topic whose messages are samples.
    pub telemetry_topic: String,
    pub ingest: Option<Ingest>,
    pub commands: Option<Commands>,
    /// The connection it was read from (the raw messages are kept by it).
    pub connection_id: Option<String>,
}

impl MqttRoute {
    /// The default route of a roaster: `roaster/<id>`, today's contract.
    pub fn default_prefix(device_id: &str) -> String {
        format!("{}/{device_id}", roastwarden_core::ROOT)
    }

    /// Read an MQTT connection's config. `Ok(None)`: everything is the default, so the
    /// connection is not in the table and its device goes the way every device goes today.
    pub fn from_config(device_id: &str, config: &Value) -> Result<Option<MqttRoute>, String> {
        let m = object(config, "config")?;
        let text = |k: &str| -> Result<Option<String>, String> {
            match m.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => {
                    let s = s.trim_end_matches('/');
                    check_topic(s).map_err(|e| format!("{k}: {e}"))?;
                    Ok(Some(s.to_string()))
                }
                Some(_) => Err(format!("{k} must be a string")),
            }
        };
        let default_prefix = Self::default_prefix(device_id);
        let prefix = text("topic_prefix")?.unwrap_or_else(|| default_prefix.clone());
        let telemetry_topic =
            text("telemetry_topic")?.unwrap_or_else(|| format!("{prefix}/telemetry"));
        let ingest = match m.get("ingest").filter(|v| !v.is_null()) {
            Some(v) => Some(parse_ingest(v, Kind::Mqtt)?),
            None => None,
        };
        let commands = match m.get("commands").filter(|v| !v.is_null()) {
            Some(v) => Some(parse_commands(v, Kind::Mqtt)?),
            None => None,
        };
        let default_topic = format!("{default_prefix}/telemetry");
        if ingest.is_none()
            && commands.is_none()
            && prefix == default_prefix
            && telemetry_topic == default_topic
        {
            return Ok(None);
        }
        Ok(Some(MqttRoute {
            device_id: device_id.to_string(),
            prefix,
            telemetry_topic,
            ingest,
            commands,
            connection_id: None,
        }))
    }

    /// The topics it reads: the telemetry topic and each field's own.
    pub fn topics(&self) -> Vec<String> {
        let mut t = vec![self.telemetry_topic.clone()];
        for f in self.ingest.iter().flat_map(|i| &i.fields) {
            if let Some(ft) = &f.topic {
                if !t.contains(ft) {
                    t.push(ft.clone());
                }
            }
        }
        t
    }

    pub fn units(&self) -> Units {
        self.ingest.as_ref().map(|i| i.units).unwrap_or_default()
    }

    /// Where and what a command is published: the template's topic and payload, else
    /// `<prefix>/control/<command>` and today's payload (in the device's unit).
    pub fn publish(&self, cmd: &DeviceCommand) -> (String, String) {
        let cmd = in_device_units(cmd, self.units());
        let default_topic = format!("{}/control/{}", self.prefix, cmd.name());
        match self.commands.as_ref().and_then(|c| c.get(cmd.name())) {
            Some(t) => (t.topic.clone().unwrap_or(default_topic), render(t, &cmd)),
            None => (default_topic, cmd.payload()),
        }
    }
}

/// What the route table says about a message's topic.
#[derive(Debug, PartialEq)]
pub enum Routed {
    /// Not a mapped topic: it goes the way every message goes today.
    NotMine,
    /// A mapped device's topic: its sample (none yet, or none in this message).
    Sample {
        device_id: String,
        sample: Option<Value>,
    },
}

#[derive(Default)]
struct Table {
    /// By registry id.
    routes: HashMap<String, Arc<MqttRoute>>,
    /// Topic → registry id.
    topics: HashMap<String, String>,
    /// Per-field topics' newest values, by registry id.
    assemblers: HashMap<String, Assembler>,
}

/// The MQTT route table: device → its topics, mapping and templates, built at start and
/// rebuilt for a device whenever its connections change (the calls that sync the pollers).
#[derive(Default)]
pub struct MqttRouter {
    table: RwLock<Table>,
}

impl MqttRouter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Put a device's route in the table (or take it out with `None`). Answers the topics
    /// that were not in the table before, to subscribe to.
    pub fn set(&self, registry_id: &str, route: Option<MqttRoute>) -> Vec<String> {
        let mut t = self
            .table
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        t.routes.remove(registry_id);
        t.topics.retain(|_, r| r != registry_id);
        t.assemblers.remove(registry_id);
        let Some(route) = route else {
            return Vec::new();
        };
        let mut new = Vec::new();
        for topic in route.topics() {
            if t.topics.contains_key(&topic) {
                // Claimed by another device: the routes' check refuses that, so this is a
                // config saved before the check; the first claim keeps it.
                tracing::warn!(topic, device_id = %route.device_id, "MQTT topic already claimed");
                continue;
            }
            t.topics.insert(topic.clone(), registry_id.to_string());
            new.push(topic);
        }
        t.routes.insert(registry_id.to_string(), Arc::new(route));
        new
    }

    /// The route of a roaster, by its id.
    pub fn for_device(&self, device_id: &str) -> Option<Arc<MqttRoute>> {
        let t = self
            .table
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        t.routes
            .values()
            .find(|r| r.device_id == device_id)
            .cloned()
    }

    /// The route that reads a topic.
    pub fn route_of_topic(&self, topic: &str) -> Option<Arc<MqttRoute>> {
        let t = self
            .table
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        t.topics.get(topic).and_then(|id| t.routes.get(id)).cloned()
    }

    /// The roaster that claims a topic, other than `registry_id`.
    pub fn claimed_by_other(&self, topic: &str, registry_id: &str) -> Option<String> {
        let t = self
            .table
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let owner = t.topics.get(topic)?;
        if owner == registry_id {
            return None;
        }
        t.routes.get(owner).map(|r| r.device_id.clone())
    }

    /// Route one message.
    pub fn on_message(&self, topic: &str, payload: &[u8], now_ms: u64) -> Routed {
        let mut t = self
            .table
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(registry_id) = t.topics.get(topic).cloned() else {
            return Routed::NotMine;
        };
        let Some(route) = t.routes.get(&registry_id).cloned() else {
            return Routed::NotMine;
        };
        let device_id = route.device_id.clone();
        let Some(value) = parse_payload(payload) else {
            return Routed::Sample {
                device_id,
                sample: None,
            };
        };
        let sample = match &route.ingest {
            // Only the topic moved: the payload is today's JSON.
            None => value.is_object().then_some(value),
            Some(ingest) if ingest.has_field_topics(&route.telemetry_topic) => t
                .assemblers
                .entry(registry_id)
                .or_default()
                .on_message(ingest, topic, &route.telemetry_topic, &value, now_ms)
                .map(Value::Object),
            Some(ingest) => ingest.map(&value).map(Value::Object),
        };
        Routed::Sample { device_id, sample }
    }
}

// ---------------------------------------------------------------------------
// The preview: what a mapping makes of the newest raw message (docs/add-roaster.md)
// ---------------------------------------------------------------------------

/// One field of a mapping against a message.
#[derive(Debug, Clone, PartialEq, serde::Serialize, utoipa::ToSchema)]
pub struct PreviewField {
    /// The mapping's key: a role (`bt`), an extra's name, or a TC4 column's name
    pub key: String,
    /// The telemetry field it becomes (`beanTemp`)
    pub field: String,
    /// Where it is read: the path, `column 3`, or `topic …: path`
    pub source: String,
    /// What the message holds there
    #[schema(value_type = Option<Object>)]
    pub raw: Option<Value>,
    /// The value it becomes (°C for a temperature)
    pub value: Option<f64>,
    /// Why there is no value: the path is missing, not a number, no message on its topic
    pub problem: Option<String>,
}

/// A JSON mapping against messages: `(topic, payload)` pairs, the telemetry topic's (or the
/// WebSocket's, topic `None`) and each field topic's newest. Answers each field and the
/// sample the mapping makes (`None` with neither BT nor ET).
pub fn preview_json(
    ingest: &Ingest,
    telemetry_topic: Option<&str>,
    messages: &[(Option<String>, Value)],
) -> (Vec<PreviewField>, Option<Map<String, Value>>) {
    let mut fields = Vec::new();
    let mut sample = Map::new();
    for f in &ingest.fields {
        let topic = f.topic.as_deref().or(telemetry_topic);
        let message = messages
            .iter()
            .find(|(t, _)| topic.is_none() || t.as_deref() == topic)
            .map(|(_, v)| v);
        let source = match &f.topic {
            Some(t) => format!("topic {t}: {}", f.path_text),
            None => f.path_text.clone(),
        };
        let mut row = PreviewField {
            key: f.key.clone(),
            field: f.target.clone(),
            source,
            raw: None,
            value: None,
            problem: None,
        };
        match message {
            None => {
                row.problem = Some(match topic {
                    Some(t) => format!("no message on {t} yet"),
                    None => "no message yet".into(),
                });
            }
            Some(payload) => match lookup(payload, &f.path) {
                None => row.problem = Some(format!("{} is not in the message", f.path_text)),
                Some(raw) => {
                    row.raw = Some(raw.clone());
                    match number(raw) {
                        None => row.problem = Some(format!("not a number: {raw}")),
                        Some(n) => {
                            let v = f.value(n, ingest.units);
                            row.value = v.as_f64();
                            sample.insert(f.target.clone(), v);
                        }
                    }
                }
            },
        }
        fields.push(row);
    }
    let sample = has_temperature(&sample).then_some(sample);
    (fields, sample)
}

/// TC4 columns against a reply line: each column, what it holds and becomes.
pub fn preview_columns(
    line: &str,
    columns: &[Option<Column>],
    units: Units,
    temp_channels: &[String],
) -> Vec<PreviewField> {
    let values: Vec<&str> = line.split(',').map(str::trim).collect();
    let mapped = read_columns(line, columns, units, temp_channels).unwrap_or_default();
    columns
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
        .map(|(i, c)| {
            let raw = values.get(i).copied();
            let value = mapped.get(&c.target).and_then(Value::as_f64);
            let problem = match (raw, value) {
                (None, _) => Some(format!("the line has {} columns", values.len())),
                (Some(_), Some(_)) => None,
                (Some(r), None) if r.parse::<f64>().is_ok() => Some("-1: no probe".into()),
                (Some(r), None) => Some(format!("not a number: {r}")),
            };
            PreviewField {
                key: ROLES
                    .iter()
                    .find(|(_, r)| Some(*r) == c.role)
                    .map_or_else(|| c.target.clone(), |(n, _)| n.to_string()),
                field: c.target.clone(),
                source: format!("column {}", i + 1),
                raw: raw.map(|r| json!(r)),
                value,
                problem,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Checks on a connection's config
// ---------------------------------------------------------------------------

/// Check the `ingest` and `commands` of a connection's config (the 400's message).
pub fn check_config(kind: Kind, config: &Value) -> Result<(), String> {
    if let Some(v) = config.get("ingest").filter(|v| !v.is_null()) {
        parse_ingest(v, kind)?;
    }
    if let Some(v) = config.get("commands").filter(|v| !v.is_null()) {
        parse_commands(v, kind)?;
    }
    Ok(())
}

/// A connection's `ingest` and `commands`, read once a config has passed its checks.
pub fn read_config(kind: Kind, config: &Value) -> (Option<Ingest>, Option<Commands>) {
    let ingest = config
        .get("ingest")
        .filter(|v| !v.is_null())
        .and_then(|v| parse_ingest(v, kind).ok());
    let commands = config
        .get("commands")
        .filter(|v| !v.is_null())
        .and_then(|v| parse_commands(v, kind).ok());
    (ingest, commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ingest(v: Value) -> Ingest {
        parse_ingest(&v, Kind::Mqtt).unwrap()
    }

    fn err(v: Value, kind: Kind) -> String {
        parse_ingest(&v, kind).unwrap_err()
    }

    #[test]
    fn the_preview_says_what_each_field_becomes_or_why_not() {
        let i = ingest(json!({ "units": "F", "fields": {
            "bt": "data.BT", "et": { "path": "data.ET" }, "fan": "data.fan",
            "exhaust": { "topic": "n/sensor/exhaust/state", "unit": "F" } } }));
        let msgs = vec![
            (
                Some("t".to_string()),
                json!({ "data": { "BT": 212.0, "ET": "hot" } }),
            ),
            (Some("n/sensor/exhaust/state".to_string()), json!(392.0)),
        ];
        let (fields, sample) = preview_json(&i, Some("t"), &msgs);
        let f = |k: &str| fields.iter().find(|f| f.key == k).unwrap().clone();
        assert_eq!(f("bt").value, Some(100.0));
        assert_eq!(f("bt").field, "beanTemp");
        assert!(f("et").problem.unwrap().contains("not a number"));
        assert!(f("fan")
            .problem
            .unwrap()
            .contains("data.fan is not in the message"));
        assert_eq!(f("exhaust").value, Some(200.0));
        assert_eq!(f("exhaust").source, "topic n/sensor/exhaust/state: $");
        let s = sample.unwrap();
        assert_eq!(s["beanTemp"], 100.0);
        assert!(!s.contains_key("envTemp"));
        // No message on the field's topic yet
        let (fields, _) = preview_json(&i, Some("t"), &msgs[..1]);
        assert!(fields
            .iter()
            .any(|f| f.problem.as_deref() == Some("no message on n/sensor/exhaust/state yet")));
        // Nothing for BT or ET: no sample
        let (_, none) = preview_json(&i, Some("t"), &[(Some("t".into()), json!({}))]);
        assert!(none.is_none());

        let cols = parse_columns(&json!(["ambient", "et", "bt", "", "exhaust"])).unwrap();
        let p = preview_columns("21.5,-1,200.25,7,x", &cols, Units::C, &[]);
        let f = |k: &str| p.iter().find(|f| f.key == k).unwrap().clone();
        assert_eq!(f("bt").value, Some(200.3));
        assert_eq!(f("bt").source, "column 3");
        assert_eq!(f("et").problem.as_deref(), Some("-1: no probe"));
        assert!(f("exhaust").problem.unwrap().contains("not a number"));
        let short = preview_columns("21.5,180", &cols, Units::C, &[]);
        assert!(short
            .iter()
            .any(|f| f.key == "bt" && f.problem.as_deref() == Some("the line has 2 columns")));
    }

    #[test]
    fn paths_reach_keys_indexes_and_the_whole_payload() {
        let p = json!({"data": {"BT": 201.5}, "probes": [{"value": 1}, {"value": "180.25"}],
            "MAX31855": {"Temperature": 415.2}, "on": true});
        let at = |path: &str| lookup(&p, &parse_path(path).unwrap()).and_then(number);
        assert_eq!(at("data.BT"), Some(201.5));
        assert_eq!(at("$.data.BT"), Some(201.5));
        assert_eq!(at("probes[1].value"), Some(180.25), "a numeric string");
        assert_eq!(at("$.probes[0].value"), Some(1.0));
        assert_eq!(at("on"), Some(1.0), "a boolean is 1 / 0");
        assert_eq!(at("missing.x"), None);
        assert_eq!(at("probes[5].value"), None);
        // `$` and an empty path are the whole payload: a plain number.
        assert_eq!(
            lookup(&json!(415.2), &parse_path("$").unwrap()).and_then(number),
            Some(415.2)
        );
        assert_eq!(
            lookup(&json!("72.5"), &parse_path("").unwrap()).and_then(number),
            Some(72.5)
        );
        assert_eq!(number(&json!(null)), None);
        assert_eq!(number(&json!("hot")), None);
        assert_eq!(number(&json!("OFF")), Some(0.0));
        // A top-level array.
        assert_eq!(
            lookup(&json!([3, 4]), &parse_path("$[1]").unwrap()).and_then(number),
            Some(4.0)
        );
        // Bad paths say why.
        assert!(parse_path("a..b").unwrap_err().contains("empty key"));
        assert!(parse_path("a.").unwrap_err().contains("ends with a dot"));
        assert!(parse_path("a[x]").unwrap_err().contains("[n]"));
        assert!(parse_path("a[0]b").unwrap_err().contains("needs a dot"));
        assert!(parse_path(&"a".repeat(300)).is_err());
    }

    #[test]
    fn only_mapped_fields_pass_with_scale_offset_and_fahrenheit() {
        let i = ingest(json!({
            "units": "F",
            "fields": {
                "bt": "MAX31855.Temperature",
                "et": { "path": "probes[1].value" },
                "heater": { "path": "out.burner", "scale": 0.392_157 },
                "fan": { "path": "Fan", "scale": 1, "offset": 0 },
                "ror": "RoR",
                "setpoint": "SV",
                "exhaust": { "path": "probes[2].value", "unit": "F" },
                "burnerGas": "gas",
                "mode": "pid",
                "heater_enable": "enabled"
            }
        }));
        let m = i
            .map(&json!({
                "MAX31855": {"Temperature": 212.0},
                "probes": [0, {"value": 392.0}, {"value": 302.0}],
                "out": {"burner": 255}, "Fan": "60", "RoR": 18.0, "SV": 410.0,
                "gas": 3.5, "pid": 2, "enabled": false, "other": 99
            }))
            .unwrap();
        assert_eq!(m["beanTemp"], json!(100.0));
        assert_eq!(m["envTemp"], json!(200.0));
        assert_eq!(m["heater"], json!(100.0), "255 × 0.392157");
        assert_eq!(m["fan"], json!(60.0));
        assert_eq!(m["rateOfRise"], json!(10.0), "a rate is × 5/9");
        assert_eq!(m["setpoint"], json!(210.0));
        assert_eq!(m["exhaust"], json!(150.0), "an extra with its own unit F");
        assert_eq!(m["burnerGas"], json!(3.5), "an extra with no unit as sent");
        assert_eq!(m["controlMode"], json!(1));
        assert_eq!(m["heaterEnable"], json!(0));
        assert!(m.get("other").is_none(), "nothing unmapped passes");
        // °C (the default): no conversion; offset applies.
        let c = ingest(json!({ "fields": { "bt": { "path": "t", "offset": -1.5 } } }));
        assert_eq!(c.map(&json!({"t": 200})).unwrap()["beanTemp"], json!(198.5));
        // No BT or ET in a message: no sample.
        assert!(i.map(&json!({"Fan": 50})).is_none());
    }

    #[test]
    fn each_temperature_role_converts_from_fahrenheit() {
        let i = ingest(json!({ "units": "F", "fields": {
            "bt": "a", "et": "b", "ambient": "c", "setpoint": "d", "ror": "e", "heater": "f" } }));
        let m = i
            .map(&json!({"a": 32, "b": 212, "c": 68, "d": 401, "e": 9, "f": 50}))
            .unwrap();
        assert_eq!(m["beanTemp"], json!(0.0));
        assert_eq!(m["envTemp"], json!(100.0));
        assert_eq!(m["ambientTemp"], json!(20.0));
        assert_eq!(m["setpoint"], json!(205.0));
        assert_eq!(m["rateOfRise"], json!(5.0));
        assert_eq!(m["heater"], json!(50.0), "a % never converts");
    }

    #[test]
    fn mapping_checks_are_400s_that_say_why() {
        let m = Kind::Mqtt;
        assert!(err(json!({"fields": {}}), m).contains("1 to 32"));
        assert!(err(json!({"fields": {"heater": "h"}}), m).contains("bt or et"));
        assert!(err(json!({"units": "K", "fields": {"bt": "t"}}), m).contains("\"C\" or \"F\""));
        assert!(err(json!({"fields": {"bt": "t", "beanTemp": "x"}}), m).contains("by its role"));
        assert!(err(json!({"fields": {"bt": "t", "9lives": "x"}}), m).contains("neither a role"));
        assert!(err(json!({"fields": {"bt": "t", "emergency_stop": "x"}}), m).contains("command"));
        assert!(err(json!({"fields": {"bt": {"path": "t", "scale": 0}}}), m).contains("scale"));
        assert!(err(json!({"fields": {"bt": {"path": "t", "unit": "F"}}}), m).contains("extra"));
        assert!(
            err(json!({"fields": {"bt": {"path": "t", "colour": 1}}}), m).contains("unknown key")
        );
        assert!(err(json!({"fields": {"bt": {"scale": 2}}}), m).contains("path is needed"));
        assert!(err(json!({"fields": {"bt": "a..b"}}), m).contains("dot"));
        assert!(err(json!({"fields": {"bt": {"topic": "x/+/y"}}}), m).contains("wildcard"));
        assert!(err(json!({"fields": {"bt": {"topic": "x/y"}}}), Kind::WebSocket).contains("MQTT"));
        assert!(err(json!({"fields": {"bt": "t"}, "columns": []}), m).contains("unknown key"));
        assert!(err(json!({"fields": {"bt": "t"}}), Kind::Tc4).contains("unknown key"));
        let many: Map<String, Value> = (0..33).map(|i| (format!("x{i}"), json!("p"))).collect();
        assert!(err(json!({"fields": many}), m).contains("33 fields"));
        // Fine ones.
        assert!(parse_ingest(&json!({"fields": {"et": "$"}}), Kind::WebSocket).is_ok());
        assert!(parse_ingest(&json!({"units": "F"}), Kind::Tc4).is_ok());
    }

    #[test]
    fn per_field_topics_cut_a_sample_on_bts_message_and_drop_stale_fields() {
        let i = ingest(json!({ "units": "F", "fields": {
            "bt": { "topic": "node/sensor/bean/state" },
            "et": { "topic": "node/sensor/env/state" },
            "fan": { "topic": "node/fan/state", "path": "speed" } } }));
        let tele = "node/telemetry";
        assert!(i.has_field_topics(tele));
        let mut a = Assembler::default();
        // ET and the fan first: no sample yet.
        assert!(a
            .on_message(&i, "node/sensor/env/state", tele, &json!(392.0), 1_000)
            .is_none());
        assert!(a
            .on_message(&i, "node/fan/state", tele, &json!({"speed": 70}), 2_000)
            .is_none());
        // BT's message cuts one, from every field's newest value.
        let s = a
            .on_message(
                &i,
                "node/sensor/bean/state",
                tele,
                &parse_payload(b"212").unwrap(),
                3_000,
            )
            .unwrap();
        assert_eq!(s["beanTemp"], json!(100.0));
        assert_eq!(s["envTemp"], json!(200.0));
        assert_eq!(s["fan"], json!(70.0));
        // 12 s on, ET (last at 1 s) and the fan (2 s) are stale: left out.
        let s = a
            .on_message(&i, "node/sensor/bean/state", tele, &json!(230.0), 13_500)
            .unwrap();
        assert_eq!(s["beanTemp"], json!(110.0));
        assert!(s.get("envTemp").is_none() && s.get("fan").is_none());
        // A BT message with no number cuts nothing.
        assert!(a
            .on_message(&i, "node/sensor/bean/state", tele, &json!("nan"), 14_000)
            .is_none());

        // Without BT, ET's message cuts.
        let et_only = ingest(json!({ "fields": {
            "et": { "topic": "a/et" }, "heater": { "topic": "a/h" } } }));
        let mut a = Assembler::default();
        assert!(a.on_message(&et_only, "a/h", tele, &json!(40), 0).is_none());
        let s = a
            .on_message(&et_only, "a/et", tele, &json!(150), 10)
            .unwrap();
        assert_eq!(s["heater"], json!(40.0));
    }

    #[test]
    fn tc4_columns_read_by_position_in_either_unit() {
        let v = json!({ "units": "F", "columns": ["ambient", "et", "bt", "", "heater", "fan", "exhaust"] });
        let i = parse_ingest(&v, Kind::Tc4).unwrap();
        let cols = i.columns.as_ref().unwrap();
        assert_eq!(cols.len(), 7);
        assert_eq!(cols[3], None);
        let m = read_columns("68,392,212,999,55,120,300", cols, i.units, &[]).unwrap();
        assert_eq!(m["ambientTemp"], json!(20.0));
        assert_eq!(m["envTemp"], json!(200.0));
        assert_eq!(m["beanTemp"], json!(100.0));
        assert_eq!(m["heater"], json!(55.0));
        assert_eq!(m["fan"], json!(100.0), "a % is clamped");
        assert_eq!(m["exhaust"], json!(300.0), "an extra column as printed");
        assert_eq!(m.len(), 6, "the skipped column is not read");
        // -1 is no probe; without BT and ET no sample.
        let m = read_columns("20,-1,180.26,30", cols, Units::C, &[]).unwrap();
        assert!(m.get("envTemp").is_none());
        assert_eq!(m["beanTemp"], json!(180.3));
        assert!(read_columns("20,-1,-1,30", cols, Units::C, &[]).is_none());
        assert!(read_columns("# OT1 ok", cols, Units::C, &[]).is_none());
        // Checks.
        let e = |c: Value| parse_ingest(&json!({ "columns": c }), Kind::Tc4).unwrap_err();
        assert!(e(json!(["heater", "fan"])).contains("bt or et"));
        assert!(e(json!(["bt", "bt"])).contains("two columns"));
        assert!(e(json!(["bt", 3])).contains("skip"));
        assert!(e(json!("bt,et")).contains("a list"));
        assert!(e(json!([])).contains("1 to 32"));
    }

    /// An extra TC4 column's unit: a declared °C channel follows `units`, another
    /// passes as printed; `:C`, `:F` and `:none` override.
    #[test]
    fn tc4_extra_columns_follow_a_declared_temperature_channel_or_their_own_unit() {
        let v = json!({ "units": "F", "columns": ["et", "bt", "exhaust", "inlet:C", "gas", "t4:none", "probe:F"] });
        let i = parse_ingest(&v, Kind::Tc4).unwrap();
        let cols = i.columns.as_ref().unwrap();
        assert_eq!(cols[2].as_ref().unwrap().unit, None);
        assert_eq!(cols[3].as_ref().unwrap().target, "inlet");
        let line = "392,212,302,150.04,2.5,400,212";
        // The caller passes the declared °C channels only (`Setups::temp_channels`)
        let declared = ["exhaust".to_string(), "t4".to_string()];
        let m = read_columns(line, cols, i.units, &declared).unwrap();
        assert_eq!(
            m["exhaust"],
            json!(150.0),
            "a declared °C channel follows units F"
        );
        assert_eq!(m["inlet"], json!(150.0), ":C is °C already (rounded)");
        assert_eq!(m["gas"], json!(2.5), "not a °C channel: as printed");
        assert_eq!(
            m["t4"],
            json!(400.0),
            ":none is as printed, declared or not"
        );
        assert_eq!(
            m["probe"],
            json!(100.0),
            ":F converts whatever the device's unit"
        );
        // Nothing declared: an extra column is as printed, as before
        let m = read_columns(line, cols, i.units, &[]).unwrap();
        assert_eq!(m["exhaust"], json!(302.0));
        // A °C device leaves a declared channel as it is; -1 is no probe for a temperature
        let c = parse_ingest(&json!({ "columns": ["bt", "exhaust"] }), Kind::Tc4).unwrap();
        let cols = c.columns.as_ref().unwrap();
        let ex = ["exhaust".to_string()];
        assert_eq!(
            read_columns("200,180.26", cols, Units::C, &ex).unwrap()["exhaust"],
            json!(180.3)
        );
        assert!(read_columns("200,-1", cols, Units::C, &ex)
            .unwrap()
            .get("exhaust")
            .is_none());
        // Checks
        let e = |c: Value| parse_ingest(&json!({ "columns": c }), Kind::Tc4).unwrap_err();
        assert!(e(json!(["bt", "exhaust:K"])).contains("C, F or none"));
        assert!(e(json!(["bt:F", "et"])).contains("follows ingest.units"));
        assert!(e(json!(["bt", "x:F", "x:C"])).contains("two columns"));
    }

    fn cmds(v: Value, kind: Kind) -> Commands {
        parse_commands(&v, kind).unwrap()
    }

    #[test]
    fn templates_render_value_scale_decimals_and_fahrenheit() {
        let c = cmds(
            json!({
                "heater": { "topic": "cmnd/roaster/Dimmer", "payload": "{value}", "decimals": 0 },
                "drum": { "payload": "OT2;{value}" },
                "fan": { "payload": "{\"pwm\":{value}}", "scale": 2.55, "decimals": 0 },
                "setpoint": { "payload": "SV={value}", "decimals": 2 },
                "mode": { "payload": "PID;{value}" },
                "emergency_stop": { "payload": "STOP" }
            }),
            Kind::Mqtt,
        );
        let r = |cmd: DeviceCommand| render(c.get(cmd.name()).unwrap(), &cmd);
        assert_eq!(r(DeviceCommand::Heater { percent: 62.5 }), "63");
        assert_eq!(r(DeviceCommand::Drum { percent: 40.0 }), "OT2;40");
        assert_eq!(r(DeviceCommand::Fan { percent: 100.0 }), "{\"pwm\":255}");
        assert_eq!(r(DeviceCommand::Setpoint { value: 200.125 }), "SV=200.13");
        assert_eq!(
            r(DeviceCommand::Mode {
                mode: ControlMode::Auto
            }),
            "PID;1"
        );
        assert_eq!(r(DeviceCommand::EmergencyStop), "STOP");
        assert_eq!(
            c.get("heater").unwrap().topic.as_deref(),
            Some("cmnd/roaster/Dimmer")
        );
        // No payload: the value alone, a mode its word.
        let bare = cmds(json!({ "heater": {}, "mode": {} }), Kind::Mqtt);
        assert_eq!(
            render(
                bare.get("heater").unwrap(),
                &DeviceCommand::Heater { percent: 62.0 }
            ),
            "62"
        );
        assert_eq!(
            render(
                bare.get("mode").unwrap(),
                &DeviceCommand::Mode {
                    mode: ControlMode::Manual
                }
            ),
            "manual"
        );
        assert_eq!(format_value(62.0, 1), "62");
        assert_eq!(format_value(62.54, 1), "62.5");
        assert_eq!(format_value(-0.01, 1), "0");
        assert_eq!(format_value(2.5, 0), "3");
        assert_eq!(format_value(0.125, 2), "0.13");
        // °F: a setpoint and the RoR limit go out in the device's unit.
        assert_eq!(
            in_device_units(&DeviceCommand::Setpoint { value: 200.0 }, Units::F),
            DeviceCommand::Setpoint { value: 392.0 }
        );
        assert_eq!(
            in_device_units(
                &DeviceCommand::RorLimit {
                    max: 10.0,
                    min_temp: 150.0
                },
                Units::F
            ),
            DeviceCommand::RorLimit {
                max: 18.0,
                min_temp: 302.0
            }
        );
        assert_eq!(
            in_device_units(&DeviceCommand::Heater { percent: 50.0 }, Units::F),
            DeviceCommand::Heater { percent: 50.0 }
        );
    }

    #[test]
    fn template_checks_are_400s_that_say_why() {
        let e = |v: Value, k: Kind| parse_commands(&v, k).unwrap_err();
        assert!(e(json!({"pid": {"payload": "x"}}), Kind::Mqtt).contains("takes no template"));
        assert!(e(json!({"fan": {"payload": ""}}), Kind::Mqtt).contains("1 to 256"));
        assert!(e(json!({"fan": {"payload": "x".repeat(257)}}), Kind::Mqtt).contains("1 to 256"));
        assert!(e(json!({"fan": {"scale": 0}}), Kind::Mqtt).contains("scale"));
        assert!(e(json!({"fan": {"decimals": 4}}), Kind::Mqtt).contains("0 to 3"));
        assert!(e(json!({"fan": {"topic": "a/#"}}), Kind::Mqtt).contains("wildcard"));
        assert!(e(json!({"fan": {"topic": "a/b"}}), Kind::Tc4).contains("MQTT"));
        assert!(e(json!({"fan": {"verb": "PUT"}}), Kind::WebSocket).contains("unknown key"));
        assert!(e(json!({"fan": "x"}), Kind::Mqtt).contains("JSON object"));
        assert!(check_topic("").is_err());
        assert!(check_topic(&"a".repeat(257)).is_err());
        assert!(check_topic("tele/roaster/SENSOR").is_ok());
    }

    #[test]
    fn an_mqtt_route_is_in_the_table_only_when_it_differs_from_the_default() {
        let none = MqttRoute::from_config(
            "esp32_roaster_01",
            &json!({
            "topic_prefix": "roaster/esp32_roaster_01", "qos": 0 }),
        )
        .unwrap();
        assert!(none.is_none(), "the ESP32's default config is today's path");
        assert!(MqttRoute::from_config("x", &json!({})).unwrap().is_none());
        let r = MqttRoute::from_config(
            "tasmota",
            &json!({
                "topic_prefix": "cmnd/roaster/", "telemetry_topic": "tele/roaster/SENSOR",
                "ingest": { "units": "F", "fields": { "bt": "MAX31855.Temperature" } },
                "commands": { "heater": { "topic": "cmnd/roaster/Dimmer", "decimals": 0 } }
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.prefix, "cmnd/roaster", "a trailing slash is dropped");
        assert_eq!(r.topics(), vec!["tele/roaster/SENSOR".to_string()]);
        assert_eq!(
            r.publish(&DeviceCommand::Heater { percent: 41.6 }),
            ("cmnd/roaster/Dimmer".into(), "42".into())
        );
        assert_eq!(
            r.publish(&DeviceCommand::Setpoint { value: 200.0 }),
            ("cmnd/roaster/control/setpoint".into(), "392".into()),
            "no template: today's payload, in °F, under the prefix"
        );
        assert_eq!(
            r.publish(&DeviceCommand::Drum { percent: 30.0 }),
            ("cmnd/roaster/control/drum".into(), "30".into())
        );
        // Only the prefix moved: in the table, default telemetry topic under it.
        let p = MqttRoute::from_config("x", &json!({ "topic_prefix": "home/roaster" }))
            .unwrap()
            .unwrap();
        assert_eq!(p.telemetry_topic, "home/roaster/telemetry");
        assert!(MqttRoute::from_config("x", &json!({ "telemetry_topic": "a/#" })).is_err());
    }

    #[test]
    fn the_router_maps_claims_and_leaves_other_topics_alone() {
        let router = MqttRouter::new();
        let route = MqttRoute::from_config(
            "tasmota",
            &json!({
            "telemetry_topic": "tele/roaster/SENSOR",
            "ingest": { "units": "F", "fields": { "bt": "MAX31855.Temperature" } } }),
        )
        .unwrap();
        assert_eq!(
            router.set("reg1", route),
            vec!["tele/roaster/SENSOR".to_string()]
        );
        assert_eq!(
            router.on_message(
                "tele/roaster/SENSOR",
                br#"{"MAX31855":{"Temperature":212}}"#,
                0
            ),
            Routed::Sample {
                device_id: "tasmota".into(),
                sample: Some(json!({"beanTemp": 100.0}))
            }
        );
        // A message of the topic without BT: claimed, no sample.
        assert_eq!(
            router.on_message("tele/roaster/SENSOR", b"{\"Time\":\"x\"}", 0),
            Routed::Sample {
                device_id: "tasmota".into(),
                sample: None
            }
        );
        assert_eq!(
            router.on_message("roaster/esp32/telemetry", b"{}", 0),
            Routed::NotMine
        );
        assert_eq!(
            router
                .claimed_by_other("tele/roaster/SENSOR", "reg2")
                .as_deref(),
            Some("tasmota")
        );
        assert_eq!(router.claimed_by_other("tele/roaster/SENSOR", "reg1"), None);
        assert_eq!(router.for_device("tasmota").unwrap().device_id, "tasmota");
        // Taken out: the topic is free again.
        router.set("reg1", None);
        assert_eq!(
            router.on_message("tele/roaster/SENSOR", b"{}", 0),
            Routed::NotMine
        );
        assert!(router.for_device("tasmota").is_none());
    }

    #[test]
    fn roles_name_their_fields_and_commands() {
        for (name, r) in ROLES {
            assert_eq!(Role::parse(name), Some(r));
        }
        assert_eq!(Role::Bt.field(), "beanTemp");
        assert_eq!(
            Role::of_command(&DeviceCommand::Drum { percent: 1.0 }),
            Some(Role::Drum)
        );
        assert_eq!(Role::of_command(&DeviceCommand::Lease { ms: 1 }), None);
        assert!(!Role::EmergencyStop.readable() && Role::Bt.readable());
        assert_eq!(Units::parse("°F"), Some(Units::F));
        assert_eq!(Units::parse("degC"), Some(Units::C));
        assert_eq!(Units::parse("%"), None);
    }
}
