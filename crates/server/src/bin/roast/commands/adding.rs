//! Adding a roaster (docs/add-roaster.md): `roaster add` makes the roaster, its connection
//! and (with `--kind`) its setup at control off; `roaster map` writes or tries the mapping;
//! `roaster check` judges it against the interface standard, and `--write` (the owner's,
//! on a cold roaster) checks that it obeys. `add` never raises control; `roaster control`
//! does, and a token's waits for the owner's approval (docs/roaster-setup.md).

use clap::Args;
use reqwest::Method;
use serde_json::{json, Map, Value};

use crate::api::{enc, Api};
use crate::out::{code, Failure, Out};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The roaster's id (letters, digits, `_`, `-`), e.g. tc4_popper
    pub device_id: String,
    /// mqtt, websocket, modbus_tcp, modbus_rtu, serial_tc4 or phidget22 (a Phidgets board
    /// through the Phidget22 network server); with --artisan or --aset, the machine's
    #[arg(long, value_parser = ["mqtt", "websocket", "modbus_tcp", "modbus_rtu", "serial_tc4", "phidget22"],
        required_unless_present_any = ["artisan", "aset"])]
    pub transport: Option<String>,
    /// A machine from Artisan's catalogue, BRAND/MODEL (`roast roaster machines` lists them):
    /// its connection, registers, outputs and channels, at control off
    /// (docs/artisan-machines.md)
    #[arg(long, value_name = "BRAND/MODEL", conflicts_with_all = ["aset", "preset"])]
    pub artisan: Option<String>,
    /// An Artisan machine setup file (.aset), as --artisan
    #[arg(long, value_name = "FILE", conflicts_with = "preset")]
    pub aset: Option<std::path::PathBuf>,
    /// With --artisan or --aset: print the plan and make nothing
    #[arg(long)]
    pub dry_run: bool,
    /// Its name in Roasters (default: the id)
    #[arg(long)]
    pub name: Option<String>,
    /// fluid_bed_electric, drum_electric, drum_gas, logger or other: writes its setup at
    /// control off (outputs follow the kind)
    #[arg(long, value_parser = ["fluid_bed_electric", "drum_electric", "drum_gas", "logger", "other"])]
    pub kind: Option<String>,
    /// MQTT: where commands go (`<prefix>/control/<command>`), default roaster/<id>
    #[arg(long)]
    pub topic_prefix: Option<String>,
    /// MQTT: the topic whose messages are samples, default <prefix>/telemetry
    #[arg(long)]
    pub telemetry_topic: Option<String>,
    /// WebSocket: the device's address, ws://host:port/path (`--url` is the server's)
    #[arg(long = "device-url")]
    pub device_url: Option<String>,
    /// Modbus TCP: the device's host; phidget22: the computer running the Phidget22 network
    /// server, the board plugged into it
    #[arg(long)]
    pub host: Option<String>,
    /// Modbus TCP: the TCP port (default 502); phidget22: the network server's (default
    /// 5661); Modbus RTU and serial TC4: the serial port (/dev/ttyUSB0)
    #[arg(long)]
    pub port: Option<String>,
    /// phidget22: the network server's password, when it has one
    #[arg(long)]
    pub password: Option<String>,
    /// phidget22: the board's serial number (a VINT board's is its hub's); default the first
    /// temperature board the server has
    #[arg(long)]
    pub serial_number: Option<i32>,
    /// phidget22: a VINT board's hub port (0–5)
    #[arg(long, value_parser = clap::value_parser!(i32).range(0..=5))]
    pub hub_port: Option<i32>,
    /// phidget22: the thermocouples' type (default K); BT is input ch0 and ET ch1 unless
    /// `roast roaster map --field bt=ch2` says otherwise
    #[arg(long, value_parser = ["J", "K", "E", "T"])]
    pub thermocouple: Option<String>,
    /// Modbus: the unit id (default 1)
    #[arg(long)]
    pub unit_id: Option<u8>,
    /// Serial: the baud rate
    #[arg(long)]
    pub baud: Option<u32>,
    /// Modbus RTU: none, even or odd
    #[arg(long, value_parser = ["none", "even", "odd"])]
    pub parity: Option<String>,
    /// Modbus serial: rtu (default) or ascii (Modbus ASCII, Delta DTA and DTK controllers)
    #[arg(long, value_parser = ["rtu", "ascii"])]
    pub framing: Option<String>,
    /// Modbus serial: data bits, 8 (RTU) or 7 (most ASCII lines)
    #[arg(long, value_parser = clap::value_parser!(u8).range(7..=8))]
    pub data_bits: Option<u8>,
    /// Defaults for a known device: stock-aartisan (TC4, active replies, CHAN;1200), tasmota
    /// (MQTT tele/<id>/SENSOR, a starting mapping), esphome (MQTT per-sensor topics),
    /// artisan-websocket (the getData poll, data.BT and data.ET)
    #[arg(long, value_parser = ["stock-aartisan", "tasmota", "esphome", "artisan-websocket"])]
    pub preset: Option<String>,
}

#[derive(Args, Debug)]
pub struct MapArgs {
    /// The roaster's id or name
    pub roaster: String,
    /// The connection to map (its id, from `roaster show --json`) when it has several
    #[arg(long)]
    pub connection: Option<String>,
    /// A field: ROLE=PATH or NAME=PATH (bt=data.BT, et=data.ET, exhaust=probes[2].value);
    /// ROLE= with no path takes it out. Repeat it
    #[arg(long = "field", value_name = "KEY=PATH")]
    pub fields: Vec<String>,
    /// The device's temperature unit: C or F
    #[arg(long, value_parser = ["C", "F"])]
    pub units: Option<String>,
    /// MQTT: a field read from a topic of its own, KEY=TOPIC (ESPHome style). Repeat it
    #[arg(long = "topic", value_name = "KEY=TOPIC")]
    pub topics: Vec<String>,
    /// Serial TC4: the reply's columns in order, e.g. ambient,et,bt,heater,fan ("" skips one);
    /// an extra column may say its unit, exhaust:F (C, F or none; else a declared °C channel
    /// follows --units)
    #[arg(long)]
    pub column: Option<String>,
    /// Modbus: a register with a role, KIND:ADDRESS[@SLAVE]:TYPE[*SCALE]:ROLE[:UNIT], e.g.
    /// input:0:float32:bt or holding:4096@2:int16*0.1:et (kinds input, holding, coil, discrete;
    /// types uint16, int16, float32, uint32, int32, bool, bcd16, bcd32; *SCALE the value is the
    /// raw times it; @SLAVE a controller of its own on the bus). Repeat it; a register at the
    /// same kind, address and slave is replaced
    #[arg(long = "register", value_name = "SPEC")]
    pub registers: Vec<String>,
    /// A command template, KEY=PAYLOAD with {value} (heater='OT1;{value}'); keys setpoint,
    /// heater, fan, drum, damper, mode, heater_enable, emergency_stop. Repeat it
    #[arg(long = "command", value_name = "KEY=PAYLOAD")]
    pub commands: Vec<String>,
    /// MQTT: the topic a command template publishes on, KEY=TOPIC
    #[arg(long = "command-topic", value_name = "KEY=TOPIC")]
    pub command_topics: Vec<String>,
    /// Write nothing: show the newest message and what the mapping makes of it
    #[arg(long = "try")]
    pub try_it: bool,
}

fn usage(msg: impl Into<String>) -> Failure {
    Failure::new("USAGE", msg)
}

/// `KEY=VALUE` → (key, value).
fn pair(s: &str, what: &str) -> Result<(String, String), Failure> {
    s.split_once('=')
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| usage(format!("{what} wants KEY=VALUE, got '{s}'")))
}

/// The connection config `add` writes, from the transport, the preset and the flags.
pub fn connection_config(a: &AddArgs) -> Result<Value, Failure> {
    let preset = a.preset.as_deref();
    let transport = a.transport.clone().unwrap_or_default();
    let want = |p: &str, t: &str| -> Result<(), Failure> {
        if transport != t {
            return Err(usage(format!("--preset {p} is for --transport {t}")));
        }
        Ok(())
    };
    let need = |v: &Option<String>, flag: &str| -> Result<String, Failure> {
        v.clone()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| usage(format!("--transport {transport} needs {flag}")))
    };
    let id = &a.device_id;
    let mut c = Map::new();
    match transport.as_str() {
        "mqtt" => {
            if let Some(p) = &a.topic_prefix {
                c.insert("topic_prefix".into(), json!(p));
            }
            if let Some(t) = &a.telemetry_topic {
                c.insert("telemetry_topic".into(), json!(t));
            }
            c.insert("qos".into(), json!(0));
            match preset {
                Some(p @ "tasmota") => {
                    want(p, "mqtt")?;
                    c.entry("telemetry_topic")
                        .or_insert(json!(format!("tele/{id}/SENSOR")));
                    c.insert(
                        "ingest".into(),
                        json!({ "units": "C", "fields": { "bt": "MAX31855.Temperature" } }),
                    );
                }
                Some(p @ "esphome") => {
                    want(p, "mqtt")?;
                    c.insert(
                        "ingest".into(),
                        json!({ "units": "C", "fields": {
                            "bt": { "topic": format!("{id}/sensor/bean_temperature/state") },
                            "et": { "topic": format!("{id}/sensor/environment_temperature/state") },
                        } }),
                    );
                }
                Some(p) => return Err(usage(format!("--preset {p} is not for MQTT"))),
                None => {}
            }
        }
        "websocket" => {
            c.insert("url".into(), json!(need(&a.device_url, "--device-url")?));
            match preset {
                Some(p @ "artisan-websocket") => {
                    want(p, "websocket")?;
                    c.insert(
                        "poll".into(),
                        json!({ "message": "{\"command\":\"getData\",\"id\":{id},\"roasterID\":0}",
                            "interval_ms": 1000 }),
                    );
                    c.insert(
                        "ingest".into(),
                        json!({ "units": "C", "fields": { "bt": "data.BT", "et": "data.ET" } }),
                    );
                }
                Some(p) => return Err(usage(format!("--preset {p} is not for a WebSocket"))),
                None => {}
            }
        }
        "modbus_tcp" => {
            c.insert("host".into(), json!(need(&a.host, "--host")?));
            if let Some(p) = &a.port {
                let n: u16 = p
                    .parse()
                    .map_err(|_| usage(format!("--port {p}: a TCP port number")))?;
                c.insert("port".into(), json!(n));
            }
            if let Some(u) = a.unit_id {
                c.insert("unit_id".into(), json!(u));
            }
            if let Some(p) = preset {
                return Err(usage(format!("--preset {p} is not for Modbus")));
            }
        }
        "modbus_rtu" | "serial_tc4" => {
            c.insert(
                "port".into(),
                json!(need(&a.port, "--port (/dev/ttyUSB0)")?),
            );
            if let Some(b) = a.baud {
                c.insert("baud".into(), json!(b));
            }
            if transport == "modbus_rtu" {
                if let Some(p) = &a.parity {
                    c.insert("parity".into(), json!(p));
                }
                if let Some(u) = a.unit_id {
                    c.insert("unit_id".into(), json!(u));
                }
                if let Some(f) = &a.framing {
                    c.insert("framing".into(), json!(f));
                }
                if let Some(b) = a.data_bits {
                    c.insert("data_bits".into(), json!(b));
                } else if a.framing.as_deref() == Some("ascii") {
                    c.insert("data_bits".into(), json!(7));
                }
                if let Some(p) = preset {
                    return Err(usage(format!("--preset {p} is not for Modbus")));
                }
            } else {
                match preset {
                    Some(p @ "stock-aartisan") => {
                        want(p, "serial_tc4")?;
                        // Stock aArtisan prints only the channels CHAN turns on: CHAN;1200
                        c.insert("reply_format".into(), json!("active"));
                        c.insert("et_channel".into(), json!(1));
                        c.insert("bt_channel".into(), json!(2));
                    }
                    Some(p) => return Err(usage(format!("--preset {p} is not for a TC4"))),
                    None => {}
                }
            }
        }
        "phidget22" => {
            c.insert(
                "host".into(),
                json!(need(&a.host, "--host (the network server's computer)")?),
            );
            phidget_flags(a, &mut c)?;
            if let Some(p) = preset {
                return Err(usage(format!("--preset {p} is not for a Phidget")));
            }
        }
        t => return Err(usage(format!("unknown transport {t}"))),
    }
    if transport != "phidget22" {
        for (flag, set) in [
            ("--password", a.password.is_some()),
            ("--serial-number", a.serial_number.is_some()),
            ("--hub-port", a.hub_port.is_some()),
            ("--thermocouple", a.thermocouple.is_some()),
        ] {
            if set {
                return Err(usage(format!("{flag} is for --transport phidget22")));
            }
        }
    }
    Ok(Value::Object(c))
}

/// A Phidget22 connection's flags: port, password, board, thermocouple type.
fn phidget_flags(a: &AddArgs, c: &mut Map<String, Value>) -> Result<(), Failure> {
    if let Some(p) = &a.port {
        let n: u16 = p
            .parse()
            .map_err(|_| usage(format!("--port {p}: a TCP port number (5661)")))?;
        c.insert("port".into(), json!(n));
    }
    if let Some(p) = &a.password {
        c.insert("password".into(), json!(p));
    }
    if let Some(n) = a.serial_number {
        c.insert("serial_number".into(), json!(n));
    }
    if let Some(p) = a.hub_port {
        c.insert("hub_port".into(), json!(p));
    }
    if let Some(t) = &a.thermocouple {
        c.insert("thermocouple_type".into(), json!(t));
    }
    Ok(())
}

/// A roaster to set up: its registry id, and the connections it came with when the server
/// found it streaming (`found`).
struct Registered {
    reg: String,
    found: Option<Vec<Value>>,
}

/// The roaster's registry entry, made active: a new one, or one the server found streaming
/// (MQTT auto-discovery: pending, with the MQTT connection it was heard on), set up in place
/// as the dashboard's "Set up" does. One added already is refused: it has a setup to keep.
async fn create_or_adopt(
    api: &Api,
    device_id: &str,
    name: &str,
    description: &str,
) -> Result<Registered, Failure> {
    let devices = api.get("/api/devices").await?;
    let existing = devices
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["device_id"] == device_id)
        .cloned();
    let (reg, found) = match existing {
        Some(d) => {
            let reg = d["id"].as_str().unwrap_or_default().to_string();
            if d["status"] != "pending" {
                return Err(Failure::new(
                    "CONFLICT",
                    format!(
                        "{device_id} is added already: see `roast roaster show {device_id}`; change its connection with `roast roaster map` or under Roasters → {device_id}"
                    ),
                ));
            }
            api.call(
                Method::PUT,
                &format!("/api/devices/{}", enc(&reg)),
                Some(&json!({ "name": name, "description": description })),
            )
            .await?;
            let full = api.get(&format!("/api/devices/{}", enc(&reg))).await?;
            let conns = full["connections"].as_array().cloned().unwrap_or_default();
            (reg, Some(conns))
        }
        None => {
            let dev = api
                .call(
                    Method::POST,
                    "/api/devices",
                    Some(&json!({ "device_id": device_id, "name": name, "description": description })),
                )
                .await?;
            (dev["id"].as_str().unwrap_or_default().to_string(), None)
        }
    };
    // Active, so its connection is read at once
    api.call(
        Method::PUT,
        &format!("/api/devices/{}", enc(&reg)),
        Some(&json!({ "status": "active" })),
    )
    .await?;
    Ok(Registered { reg, found })
}

/// Its connection: the one a found roaster came with is changed in place when the protocol
/// is the same, and replaced when not (as the wizard does); a new roaster gets a new one.
async fn save_connection(
    api: &Api,
    r: &Registered,
    protocol: &str,
    config: &Value,
) -> Result<Value, Failure> {
    let reg = enc(&r.reg);
    let mut same = None;
    for c in r.found.iter().flatten() {
        let id = c["id"].as_str().unwrap_or_default();
        if same.is_none() && c["protocol"] == protocol {
            same = Some(id.to_string());
        } else {
            api.call(
                Method::DELETE,
                &format!("/api/devices/{reg}/connections/{}", enc(id)),
                None,
            )
            .await?;
        }
    }
    match same {
        Some(id) => {
            // What it was heard on stays (its topic prefix), what was given goes over it
            let mut merged = r
                .found
                .iter()
                .flatten()
                .find(|c| c["id"] == id.as_str())
                .and_then(|c| c["config"].as_object().cloned())
                .unwrap_or_default();
            if let Some(given) = config.as_object() {
                merged.extend(given.clone());
            }
            api.call(
                Method::PUT,
                &format!("/api/devices/{reg}/connections/{}", enc(&id)),
                Some(&json!({ "enabled": true, "config": merged })),
            )
            .await
        }
        None => {
            api.call(
                Method::POST,
                &format!("/api/devices/{reg}/connections"),
                Some(&json!({ "protocol": protocol, "enabled": true, "priority": 0, "config": config })),
            )
            .await
        }
    }
}

pub async fn add(api: &Api, out: &Out, a: AddArgs) -> Result<u8, Failure> {
    if a.artisan.is_some() || a.aset.is_some() {
        return add_machine(api, out, a).await;
    }
    if a.dry_run {
        return Err(usage("--dry-run is for --artisan or --aset"));
    }
    let transport = a.transport.clone().unwrap_or_default();
    let config = connection_config(&a)?;
    let registered = create_or_adopt(
        api,
        &a.device_id,
        &a.name.clone().unwrap_or_else(|| a.device_id.clone()),
        "Added with roast roaster add",
    )
    .await?;
    let conn = save_connection(api, &registered, &transport, &config).await?;
    let (reg, found) = (registered.reg.clone(), registered.found.is_some());
    let setup = match &a.kind {
        Some(kind) => Some(
            api.call(
                Method::PUT,
                &format!("/api/roaster/{}/setup", enc(&a.device_id)),
                Some(&json!({ "kind": kind, "control": "off" })),
            )
            .await?,
        ),
        None => None,
    };
    let answer = json!({
        "device_id": a.device_id,
        "registry_id": reg,
        "found": found,
        "connection": conn,
        "setup": setup,
        "control": "off",
        "next": [
            format!("roast roaster map {} --try", a.device_id),
            format!("roast roaster check {}", a.device_id),
        ],
    });
    out.emit(&answer, |r| {
        format!(
            "{} {} with a {} connection ({}){}.\nControl is off: Roastwarden only records it; the owner switches control on under Roasters → {} → Setup.\nNext: {} until the values look right, then {}.",
            if found { "Set up" } else { "Added" },
            if found {
                format!("{} (it had streamed already)", a.device_id)
            } else {
                a.device_id.clone()
            },
            transport,
            r["connection"]["id"].as_str().unwrap_or("?"),
            match &a.kind {
                Some(k) => format!(", kind {k}"),
                None => String::new(),
            },
            a.device_id,
            r["next"][0].as_str().unwrap_or(""),
            r["next"][1].as_str().unwrap_or(""),
        )
    });
    Ok(code::OK)
}

/// `roaster add --artisan | --aset`: a roaster planned from an Artisan machine setup
/// (docs/artisan-machines.md): the server's plan (connection, register map with each
/// register's slave, outputs from the sliders, channels from the extra curves, control off),
/// the flags filling what the file cannot know (`--host`, `--port`), then the normal calls.
async fn add_machine(api: &Api, out: &Out, a: AddArgs) -> Result<u8, Failure> {
    if a.preset.is_some() {
        return Err(usage("--preset is not for --artisan or --aset"));
    }
    let body = match (&a.artisan, &a.aset) {
        (Some(m), _) => json!({ "machine": m }),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| Failure::new("NO_FILE", format!("{}: {e}", path.display())))?;
            json!({
                "aset": text,
                "file_name": path.file_name().map(|f| f.to_string_lossy().to_string()),
            })
        }
        _ => unreachable!("add_machine is called with --artisan or --aset"),
    };
    let plan = api
        .call(Method::POST, "/api/artisan/machines/plan", Some(&body))
        .await?;
    let machine = &plan["machine"];
    if machine["supported"] != json!(true) {
        return Err(Failure::new(
            "FAILED",
            format!(
                "{} cannot be set up from its Artisan file: {}",
                machine["name"].as_str().unwrap_or("this machine"),
                machine["why"].as_str().unwrap_or("not supported")
            ),
        ));
    }
    let protocol = plan["protocol"].as_str().unwrap_or_default().to_string();
    if let Some(t) = &a.transport
        && *t != protocol {
            return Err(usage(format!(
                "--transport {t}: this machine's Artisan file speaks {protocol}"
            )));
        }
    let mut config = plan["config"].clone();
    let c = config
        .as_object_mut()
        .ok_or_else(|| Failure::new("BAD_RESPONSE", "the plan has no connection config"))?;
    // What the file cannot know, from the flags: each need the flags fill.
    let mut filled: Vec<&str> = Vec::new();
    match protocol.as_str() {
        "modbus_tcp" => {
            if let Some(h) = &a.host {
                c.insert("host".into(), json!(h));
                filled.push("host");
            }
            if let Some(p) = &a.port {
                let n: u16 = p
                    .parse()
                    .map_err(|_| usage(format!("--port {p}: a TCP port number")))?;
                c.insert("port".into(), json!(n));
            }
        }
        "websocket" => {
            if let Some(u) = &a.device_url {
                c.insert("url".into(), json!(u));
                filled.push("host");
            } else if let Some(h) = &a.host {
                let url = c["url"].as_str().unwrap_or("ws://127.0.0.1:80/");
                c.insert("url".into(), json!(with_host(url, h)));
                filled.push("host");
            }
        }
        "phidget22" => {
            if let Some(h) = &a.host {
                c.insert("host".into(), json!(h));
                filled.push("host");
            }
            phidget_flags(&a, c)?;
        }
        "mqtt" => {
            if let Some(t) = &a.telemetry_topic {
                c.insert("telemetry_topic".into(), json!(t));
                filled.push("telemetry_topic");
            }
            if let Some(p) = &a.topic_prefix {
                c.insert("topic_prefix".into(), json!(p));
            }
        }
        _ => {
            if let Some(p) = &a.port {
                c.insert("port".into(), json!(p));
                filled.push("port");
            }
            if let Some(b) = a.baud {
                c.insert("baud".into(), json!(b));
            }
            if let Some(p) = &a.parity {
                c.insert("parity".into(), json!(p));
            }
            if let Some(f) = &a.framing {
                c.insert("framing".into(), json!(f));
            }
            if let Some(b) = a.data_bits {
                c.insert("data_bits".into(), json!(b));
            }
        }
    }
    if let Some(u) = a.unit_id {
        c.insert("unit_id".into(), json!(u));
    }
    let missing: Vec<&str> = plan["needs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|n| !filled.contains(n))
        .collect();
    let mut setup = plan["setup"].clone();
    if let Some(k) = &a.kind {
        setup["kind"] = json!(k);
    }
    setup["control"] = json!("off");
    let name = a
        .name
        .clone()
        .or_else(|| machine["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| a.device_id.clone());
    let notes: Vec<String> = plan["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let registers = plan["registers"].clone();
    let n_regs = registers.as_array().map_or(0, Vec::len);
    // What it reads: its registers, else its mapped fields, else a TC4's replies.
    let reads = match (
        n_regs,
        config["ingest"]["fields"]
            .as_object()
            .map(serde_json::Map::len),
    ) {
        (n, _) if n > 0 => format!("{n} registers"),
        (_, Some(f)) => format!("{f} mapped fields"),
        _ => "aArtisan's replies".to_string(),
    };

    if a.dry_run {
        let answer = json!({
            "device_id": a.device_id, "name": name, "protocol": protocol, "config": config,
            "registers": registers, "setup": setup, "missing": missing, "notes": notes,
            "made": false,
        });
        out.emit(&answer, |r| {
            let mut lines = vec![format!(
                "{} ({}) as {}: {} over {protocol}, {reads}{}",
                name,
                machine["key"].as_str().unwrap_or(""),
                a.device_id,
                machine["artisan_device_name"].as_str().unwrap_or("?"),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; needs {}",
                        missing
                            .iter()
                            .map(|m| format!("--{}", m.replace('_', "-")))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            )];
            for reg in r["registers"].as_array().into_iter().flatten() {
                lines.push(format!("  {}", register_line(reg)));
            }
            lines.extend(mapping_lines(&r["config"]));
            for n in &notes {
                lines.push(format!("  · {n}"));
            }
            lines.push("Nothing was made (--dry-run).".into());
            lines.join("\n")
        });
        return Ok(code::OK);
    }
    if !missing.is_empty() {
        return Err(usage(format!(
            "this machine needs {} (the Artisan file cannot know {})",
            missing
                .iter()
                .map(|m| match *m {
                    "host" => "--host (the roaster's address)".to_string(),
                    "port" => "--port (the serial port on the server, /dev/ttyUSB0)".to_string(),
                    "telemetry_topic" =>
                        "--telemetry-topic (the topic its data reaches this server's broker on)"
                            .to_string(),
                    m => format!("--{}", m.replace('_', "-")),
                })
                .collect::<Vec<_>>()
                .join(" and "),
            if missing.len() == 1 { "it" } else { "them" }
        )));
    }

    let registered = create_or_adopt(
        api,
        &a.device_id,
        &name,
        &format!(
            "From Artisan's machine setup {}",
            machine["key"].as_str().unwrap_or("")
        ),
    )
    .await?;
    let reg = registered.reg.clone();
    // The register map before the connection, so its poller starts with it.
    if n_regs > 0 {
        api.call(
            Method::PUT,
            &format!("/api/devices/{}/register-map", enc(&reg)),
            Some(&registers),
        )
        .await?;
    }
    let conn = save_connection(api, &registered, &protocol, &config).await?;
    let saved_setup = api
        .call(
            Method::PUT,
            &format!("/api/roaster/{}/setup", enc(&a.device_id)),
            Some(&setup),
        )
        .await?;
    let answer = json!({
        "device_id": a.device_id,
        "registry_id": reg,
        "machine": machine,
        "connection": conn,
        "registers": n_regs,
        "setup": saved_setup,
        "control": "off",
        "notes": notes,
        "next": [
            format!("roast roaster map {} --try", a.device_id),
            format!("roast roaster check {}", a.device_id),
        ],
    });
    out.emit(&answer, |r| {
        let mut lines = vec![format!(
            "Added {} ({}) from Artisan's {}: {protocol}, {reads}, kind {}.",
            a.device_id,
            name,
            machine["key"].as_str().unwrap_or(""),
            r["setup"]["kind"].as_str().unwrap_or("?"),
        )];
        for n in &notes {
            lines.push(format!("  · {n}"));
        }
        lines.push(format!(
            "Control is off: Roastwarden only records it. Next: {} (the newest poll), then {}; the owner switches control on under Roasters → {} → Setup.",
            r["next"][0].as_str().unwrap_or(""),
            r["next"][1].as_str().unwrap_or(""),
            a.device_id
        ));
        lines.join("\n")
    });
    Ok(code::OK)
}

/// `ws://127.0.0.1:80/WebSocket` with another host: `ws://10.0.0.5:80/WebSocket`.
pub fn with_host(url: &str, host: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("ws", url));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let port = authority
        .rsplit_once(':')
        .map(|(_, p)| format!(":{p}"))
        .unwrap_or_default();
    format!("{scheme}://{host}{port}/{path}")
}

/// A planned connection's mapping and templates as lines: `bt ← data.beanTemp`,
/// `heater → OT1,{value}`.
fn mapping_lines(config: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for (k, f) in config["ingest"]["fields"].as_object().into_iter().flatten() {
        let path = f.as_str().or(f["path"].as_str()).unwrap_or("?");
        let extra: Vec<String> = [
            f["scale"].as_f64().map(|s| format!("×{s}")),
            f["unit"].as_str().map(|u| format!("°{u}")),
            f["topic"].as_str().map(|t| format!("on {t}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        lines.push(format!(
            "  {k} ← {path}{}",
            if extra.is_empty() {
                String::new()
            } else {
                format!(" ({})", extra.join(", "))
            }
        ));
    }
    if let Some(p) = config["poll"]["message"].as_str() {
        lines.push(format!("  poll: {p}"));
    }
    for (k, t) in config["commands"].as_object().into_iter().flatten() {
        lines.push(format!(
            "  {k} → {}",
            t["payload"].as_str().unwrap_or("?").replace('\n', " / ")
        ));
    }
    lines
}

/// One register as a line: `holding 4096@2 int16 ×0.1 → et`.
fn register_line(r: &Value) -> String {
    format!(
        "{} {}{} {}{}{} → {}{}",
        r["register_type"].as_str().unwrap_or("?"),
        r["address"],
        r["unit_id"]
            .as_i64()
            .map(|u| format!("@{u}"))
            .unwrap_or_default(),
        r["data_type"].as_str().unwrap_or("?"),
        r["scale_factor"]
            .as_f64()
            .map(|s| format!(" ×{}", (s * 1e6).round() / 1e6))
            .unwrap_or_default(),
        r["unit"]
            .as_str()
            .map(|u| format!(" {u}"))
            .unwrap_or_default(),
        r["role"].as_str().map_or_else(
            || format!("channel {}", r["name"].as_str().unwrap_or("?")),
            str::to_string
        ),
        if r["writable"] == json!(true) {
            " (written)"
        } else {
            ""
        }
    )
}

/// `roaster machines [QUERY]`: Artisan's machine catalogue, the ones Roastwarden can set up
/// (`--all`: every one, with why not).
pub async fn machines(
    api: &Api,
    out: &Out,
    query: Option<String>,
    all: bool,
) -> Result<u8, Failure> {
    let list = api.get("/api/artisan/machines").await?;
    let q = query.as_deref().map(str::to_lowercase);
    let picked: Vec<Value> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| all || m["supported"] == json!(true))
        .filter(|m| {
            q.as_deref().is_none_or(|q| {
                let hay = format!(
                    "{} {} {}",
                    m["key"].as_str().unwrap_or(""),
                    m["name"].as_str().unwrap_or(""),
                    m["artisan_device_name"].as_str().unwrap_or("")
                )
                .to_lowercase();
                q.split_whitespace().all(|w| hay.contains(w))
            })
        })
        .cloned()
        .collect();
    let total = list.as_array().map_or(0, Vec::len);
    let found = !picked.is_empty();
    out.emit(&Value::Array(picked), |l| {
        let mut lines: Vec<String> = l
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                format!(
                    "{}  {} ({}){}",
                    m["key"].as_str().unwrap_or(""),
                    m["name"].as_str().unwrap_or(""),
                    m["artisan_device_name"].as_str().unwrap_or(""),
                    m["why"]
                        .as_str()
                        .map(|w| format!(": not supported, {w}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        lines.push(if lines.is_empty() {
            format!(
                "No machine{} in Artisan's catalogue of {total}{}.",
                if all { "" } else { " Roastwarden can set up" },
                if all { "" } else { " (--all lists every one)" }
            )
        } else {
            "Add one: roast roaster add ID --artisan BRAND/MODEL [--host … | --port …] (--dry-run first)".into()
        });
        lines.join("\n")
    });
    Ok(if found { code::OK } else { code::NOT_FOUND })
}

/// `roaster control`: the setup's control level, the rest of the setup kept. A token leaving
/// control on is held for the owner's approval (`--wait` waits for it); off runs at once.
pub async fn control(
    api: &Api,
    out: &Out,
    roaster: String,
    level: String,
    wait: crate::cli::Wait,
) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let path = format!("/api/roaster/{}/setup", enc(&device_id));
    let setup = api.call(Method::GET, &path, None).await?;
    let reply = api
        .send(
            Method::PUT,
            &path,
            Some(&json!({ "kind": setup["kind"], "control": level })),
            std::time::Duration::from_secs(60),
        )
        .await?;
    crate::commands::approvals::held_or_done(api, out, reply, wait, move |s| {
        format!(
            "{device_id}: control {}",
            s["control"].as_str().unwrap_or("?")
        )
    })
    .await
}

/// A register spec `kind:address:type[*scale]:role[:unit]` as the register map's entry.
pub fn register_entry(spec: &str) -> Result<Value, Failure> {
    let parts: Vec<&str> = spec.split(':').map(str::trim).collect();
    if !(4..=5).contains(&parts.len()) {
        return Err(usage(format!(
            "--register {spec}: KIND:ADDRESS:TYPE[*SCALE]:ROLE[:UNIT], e.g. input:0:float32:bt or holding:0:int16*0.1:bt"
        )));
    }
    // The type may carry its scale: int16*0.1 is a temperature in tenths.
    let (data_type, scale) = match parts[2].split_once('*') {
        Some((t, s)) => (
            t.trim(),
            Some(
                s.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s != 0.0)
                    .ok_or_else(|| {
                        usage(format!(
                            "--register {spec}: the scale after * is a number other than 0"
                        ))
                    })?,
            ),
        ),
        None => (parts[2], None),
    };
    let kind = parts[0].to_ascii_lowercase();
    if !["input", "holding", "coil", "discrete"].contains(&kind.as_str()) {
        return Err(usage(format!(
            "--register {spec}: kind input, holding, coil or discrete"
        )));
    }
    let (address, slave) = match parts[1].split_once('@') {
        Some((a, s)) => (a, Some(s)),
        None => (parts[1], None),
    };
    let address: i32 = address
        .trim()
        .parse()
        .map_err(|_| usage(format!("--register {spec}: the address is a number")))?;
    let slave: Option<u8> = match slave {
        Some(s) => Some(
            s.trim()
                .parse()
                .ok()
                .filter(|s| (1..=247).contains(s))
                .ok_or_else(|| usage(format!("--register {spec}: @SLAVE is 1 to 247")))?,
        ),
        None => None,
    };
    let role = parts[3].to_string();
    const WRITE: [&str; 8] = [
        "heater",
        "fan",
        "drum",
        "damper",
        "setpoint",
        "mode",
        "heater_enable",
        "emergency_stop",
    ];
    let writable = (kind == "holding" || kind == "coil") && WRITE.contains(&role.as_str());
    let mut e = json!({
        "register_type": kind,
        "address": address,
        "name": role,
        "data_type": data_type.to_ascii_lowercase(),
        "writable": writable,
        "role": role,
    });
    if let Some(s) = scale {
        e["scale_factor"] = json!(s);
    }
    if let Some(u) = parts.get(4) {
        e["unit"] = json!(u);
    }
    if let Some(s) = slave {
        e["unit_id"] = json!(s);
        e["name"] = json!(format!("{role}@{s}"));
    }
    Ok(e)
}

/// The mapping flags merged into a connection's config: fields given replace theirs (an
/// empty path takes one out), the rest stay.
pub fn merged_config(config: &Value, protocol: &str, m: &MapArgs) -> Result<Value, Failure> {
    let mut config = config.clone();
    if !config.is_object() {
        config = json!({});
    }
    let mut ingest = config["ingest"].as_object().cloned().unwrap_or_default();
    if let Some(u) = &m.units {
        ingest.insert("units".into(), json!(u));
    }
    if let Some(cols) = &m.column {
        if protocol != "serial_tc4" {
            return Err(usage("--column is for a serial TC4 connection"));
        }
        let cols: Vec<&str> = cols.split(',').map(str::trim).collect();
        ingest.insert("columns".into(), json!(cols));
    }
    // A Phidget with no mapping reads BT ch0, ET ch1: a field changes that, not all of it.
    if protocol == "phidget22" && !m.fields.is_empty() && !ingest.contains_key("fields") {
        ingest.insert("fields".into(), json!({ "bt": "ch0", "et": "ch1" }));
    }
    if !m.fields.is_empty() || !m.topics.is_empty() {
        if protocol == "serial_tc4" {
            return Err(usage(
                "a TC4 is mapped by --column (the reply's layout), not --field",
            ));
        }
        if !m.topics.is_empty() && protocol != "mqtt" {
            return Err(usage("--topic is for an MQTT connection"));
        }
        let mut fields = ingest
            .get("fields")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for f in &m.fields {
            let (k, path) = pair(f, "--field")?;
            if path.trim().is_empty() {
                fields.remove(&k);
            } else {
                match fields.get_mut(&k) {
                    Some(Value::Object(o)) => {
                        o.insert("path".into(), json!(path.trim()));
                    }
                    _ => {
                        fields.insert(k, json!(path.trim()));
                    }
                }
            }
        }
        for t in &m.topics {
            let (k, topic) = pair(t, "--topic")?;
            let mut o = match fields.remove(&k) {
                Some(Value::Object(o)) => o,
                Some(Value::String(p)) => {
                    let mut o = Map::new();
                    o.insert("path".into(), json!(p));
                    o
                }
                _ => Map::new(),
            };
            o.insert("topic".into(), json!(topic.trim()));
            fields.insert(k, Value::Object(o));
        }
        ingest.insert("fields".into(), Value::Object(fields));
    }
    if !ingest.is_empty() {
        config["ingest"] = Value::Object(ingest);
    }
    if !m.commands.is_empty() || !m.command_topics.is_empty() {
        let mut cmds = config["commands"].as_object().cloned().unwrap_or_default();
        let mut entry = |k: &str| -> Map<String, Value> {
            match cmds.remove(k) {
                Some(Value::Object(o)) => o,
                _ => Map::new(),
            }
        };
        let mut set: Vec<(String, Map<String, Value>)> = Vec::new();
        for c in &m.commands {
            let (k, payload) = pair(c, "--command")?;
            let mut o = entry(&k);
            o.insert("payload".into(), json!(payload));
            set.push((k, o));
        }
        for c in &m.command_topics {
            if protocol != "mqtt" {
                return Err(usage("--command-topic is for an MQTT connection"));
            }
            let (k, topic) = pair(c, "--command-topic")?;
            let mut o = match set.iter().position(|(x, _)| *x == k) {
                Some(i) => set.remove(i).1,
                None => entry(&k),
            };
            o.insert("topic".into(), json!(topic.trim()));
            set.push((k, o));
        }
        for (k, o) in set {
            cmds.insert(k, Value::Object(o));
        }
        config["commands"] = Value::Object(cmds);
    }
    Ok(config)
}

/// The roaster's registry id, device id and the connection to map.
async fn connection(
    api: &Api,
    roaster: &str,
    wanted: Option<&str>,
) -> Result<(String, String, Value), Failure> {
    let r = api.roaster(roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let Some(reg) = r["registry_id"].as_str().map(str::to_string) else {
        return Err(Failure::new(
            "NOT_FOUND",
            format!("{device_id} has no connection saved: add it first (roast roaster add)"),
        ));
    };
    let dev = api.get(&format!("/api/devices/{}", enc(&reg))).await?;
    let conns: Vec<Value> = dev["connections"].as_array().cloned().unwrap_or_default();
    let conn = match wanted {
        Some(id) => conns.into_iter().find(|c| c["id"] == id).ok_or_else(|| {
            Failure::new("NOT_FOUND", format!("{device_id} has no connection {id}"))
        })?,
        None => {
            let enabled: Vec<Value> = conns
                .into_iter()
                .filter(|c| c["enabled"] != json!(false))
                .collect();
            match enabled.len() {
                1 => enabled.into_iter().next().unwrap_or_default(),
                0 => {
                    return Err(Failure::new(
                        "NOT_FOUND",
                        format!("{device_id} has no connection on: add one (roast roaster add)"),
                    ))
                }
                _ => {
                    let ids: Vec<String> = enabled
                        .iter()
                        .map(|c| {
                            format!(
                                "{} ({})",
                                c["id"].as_str().unwrap_or("?"),
                                c["protocol"].as_str().unwrap_or("?")
                            )
                        })
                        .collect();
                    return Err(Failure::new(
                        "AMBIGUOUS",
                        format!(
                            "{device_id} has several connections: pick one with --connection ({})",
                            ids.join(", ")
                        ),
                    ));
                }
            }
        }
    };
    Ok((reg, device_id, conn))
}

/// A preview's field in a line: `bt ← data.BT = 415.2 → beanTemp 212.9`.
fn field_line(f: &Value) -> String {
    let raw = match &f["raw"] {
        Value::Null => String::new(),
        v => format!(" = {v}"),
    };
    match f["value"].as_f64() {
        Some(v) => format!(
            "  {} ← {}{raw} → {} {v}",
            f["key"].as_str().unwrap_or("?"),
            f["source"].as_str().unwrap_or("?"),
            f["field"].as_str().unwrap_or("?")
        ),
        None => format!(
            "  {} ← {}{raw}: {}",
            f["key"].as_str().unwrap_or("?"),
            f["source"].as_str().unwrap_or("?"),
            f["problem"].as_str().unwrap_or("no value")
        ),
    }
}

pub async fn map(api: &Api, out: &Out, m: MapArgs) -> Result<u8, Failure> {
    let (reg, device_id, conn) = connection(api, &m.roaster, m.connection.as_deref()).await?;
    let protocol = conn["protocol"].as_str().unwrap_or_default().to_string();
    let conn_id = conn["id"].as_str().unwrap_or_default().to_string();
    let modbus = protocol.starts_with("modbus");

    if modbus {
        if !m.fields.is_empty()
            || !m.topics.is_empty()
            || m.column.is_some()
            || !m.commands.is_empty()
        {
            return Err(usage(
                "a Modbus connection is mapped by --register KIND:ADDRESS:TYPE:ROLE (roles in its register map)",
            ));
        }
        if m.try_it || m.registers.is_empty() {
            let raw = api
                .get(&format!("/api/roaster/{}/raw", enc(&device_id)))
                .await?;
            out.emit(&raw, |r| {
                let mut lines = vec![format!(
                    "{device_id}: the newest Modbus poll (decoded by type, before scale and role)"
                )];
                for msg in r.as_array().into_iter().flatten() {
                    lines.push(format!("  {}", msg["payload"].as_str().unwrap_or("")));
                }
                if lines.len() == 1 {
                    lines.push("  nothing read yet".into());
                }
                lines.join("\n")
            });
            return Ok(code::OK);
        }
        let mut map: Vec<Value> = api
            .get(&format!("/api/devices/{}/register-map", enc(&reg)))
            .await?
            .as_array()
            .cloned()
            .unwrap_or_default();
        for spec in &m.registers {
            let e = register_entry(spec)?;
            let slave = |v: &Value| v.get("unit_id").cloned().unwrap_or(Value::Null);
            map.retain(|r| {
                !(r["register_type"] == e["register_type"]
                    && r["address"] == e["address"]
                    && slave(r) == slave(&e))
            });
            map.push(e);
        }
        let saved = api
            .call(
                Method::PUT,
                &format!("/api/devices/{}/register-map", enc(&reg)),
                Some(&Value::Array(map)),
            )
            .await?;
        out.emit(&saved, |s| {
            format!(
                "{device_id}: {} registers saved. Next: roast roaster check {device_id}",
                s.as_array().map_or(0, Vec::len)
            )
        });
        return Ok(code::OK);
    }
    if !m.registers.is_empty() {
        return Err(usage("--register is for a Modbus connection"));
    }

    let config = merged_config(&conn["config"], &protocol, &m)?;
    if m.try_it {
        let p = api
            .call(
                Method::POST,
                &format!("/api/roaster/{}/ingest/preview", enc(&device_id)),
                Some(&json!({ "connection_id": conn_id, "ingest": config.get("ingest").cloned().unwrap_or(Value::Null) })),
            )
            .await?;
        let made = !p["sample"].is_null();
        out.emit(&p, |p| {
            let mut lines = vec![format!("{device_id} {protocol} {conn_id}")];
            for msg in p["messages"].as_array().into_iter().flatten() {
                lines.push(format!(
                    "  newest{}: {}",
                    msg["topic"]
                        .as_str()
                        .map(|t| format!(" on {t}"))
                        .unwrap_or_default(),
                    msg["payload"].as_str().unwrap_or("")
                ));
            }
            for f in p["fields"].as_array().into_iter().flatten() {
                lines.push(field_line(f));
            }
            if let Some(n) = p["note"].as_str() {
                lines.push(format!("  {n}"));
            }
            lines.push(if p["sample"].is_null() {
                "  no sample: fix the mapping and try again".into()
            } else {
                format!(
                    "  sample {}\nLooks right? Run it again without --try to save it.",
                    p["sample"]
                )
            });
            lines.join("\n")
        });
        return Ok(if made { code::OK } else { code::FAILED });
    }
    let saved = api
        .call(
            Method::PUT,
            &format!("/api/devices/{}/connections/{}", enc(&reg), enc(&conn_id)),
            Some(&json!({ "config": config })),
        )
        .await?;
    out.emit(&saved, |_| {
        format!(
            "{device_id}: the mapping is saved on {conn_id}. Next: roast roaster check {device_id}"
        )
    });
    Ok(code::OK)
}

pub async fn check(api: &Api, out: &Out, roaster: String, write: bool) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    if write {
        let w = api
            .call(
                Method::POST,
                &format!("/api/roaster/{}/check/write", enc(&device_id)),
                None,
            )
            .await?;
        let ok = w["ok"] == json!(true);
        out.emit(&w, |w| {
            let mut lines = vec![format!(
                "{device_id} write check: {}",
                if w["ok"] == json!(true) {
                    "ok"
                } else {
                    "FAILED"
                }
            )];
            for s in w["steps"].as_array().into_iter().flatten() {
                lines.push(format!(
                    "  {:>5} ms {} {}: {}",
                    s["at_ms"],
                    if s["ok"] == json!(true) {
                        "ok  "
                    } else {
                        "FAIL"
                    },
                    s["step"].as_str().unwrap_or("?"),
                    s["message"].as_str().unwrap_or("")
                ));
            }
            lines.join("\n")
        });
        return Ok(if ok { code::OK } else { code::FAILED });
    }
    let c = api
        .get(&format!("/api/roaster/{}/check", enc(&device_id)))
        .await?;
    let ok = c["verdict"] == "ok";
    out.emit(&c, |c| {
        let mut lines = vec![format!(
            "{device_id}: {}",
            c["verdict"].as_str().unwrap_or("?")
        )];
        for l in c["lines"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  {:<4} {:<8} {}",
                l["status"].as_str().unwrap_or("?"),
                l["check"].as_str().unwrap_or("?"),
                l["message"].as_str().unwrap_or("")
            ));
        }
        if let Some(r) = c["reading"].as_object() {
            let parts: Vec<String> = r
                .iter()
                .filter(|(k, _)| *k != "age_s")
                .map(|(k, v)| format!("{k} {v}"))
                .collect();
            lines.push(format!("  reading: {}", parts.join(", ")));
        }
        lines.join("\n")
    });
    Ok(if ok { code::OK } else { code::FAILED })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_args(transport: &str) -> AddArgs {
        AddArgs {
            device_id: "r1".into(),
            transport: Some(transport.into()),
            artisan: None,
            aset: None,
            dry_run: false,
            name: None,
            kind: None,
            topic_prefix: None,
            telemetry_topic: None,
            device_url: None,
            host: None,
            port: None,
            unit_id: None,
            baud: None,
            parity: None,
            framing: None,
            data_bits: None,
            preset: None,
            password: None,
            serial_number: None,
            hub_port: None,
            thermocouple: None,
        }
    }

    #[test]
    fn a_phidget_takes_its_server_and_board() {
        let mut a = add_args("phidget22");
        assert!(connection_config(&a)
            .unwrap_err()
            .message
            .contains("--host"));
        a.host = Some("sbc.local".into());
        a.port = Some("5662".into());
        a.serial_number = Some(370_001);
        a.thermocouple = Some("J".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(
            c,
            json!({ "host": "sbc.local", "port": 5662, "serial_number": 370_001,
                "thermocouple_type": "J" })
        );
        let mut m = add_args("modbus_tcp");
        m.host = Some("h".into());
        m.thermocouple = Some("K".into());
        assert!(connection_config(&m)
            .unwrap_err()
            .message
            .contains("phidget22"));
    }

    #[test]
    fn a_phidget_field_keeps_the_default_mapping() {
        let mut m = map_args();
        m.fields = vec!["exhaust=ch2".into()];
        let c = merged_config(&json!({ "host": "h" }), "phidget22", &m).unwrap();
        assert_eq!(
            c["ingest"]["fields"],
            json!({ "bt": "ch0", "et": "ch1", "exhaust": "ch2" })
        );
    }

    #[test]
    fn modbus_ascii_is_a_framing_of_the_rtu_connection() {
        let mut a = add_args("modbus_rtu");
        a.port = Some("/dev/ttyUSB1".into());
        a.framing = Some("ascii".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(c["framing"], "ascii");
        assert_eq!(c["data_bits"], 7, "ASCII lines are 7 bits unless said");
        a.data_bits = Some(8);
        assert_eq!(connection_config(&a).unwrap()["data_bits"], 8);
    }

    #[test]
    fn a_websocket_url_takes_another_host() {
        assert_eq!(
            with_host("ws://127.0.0.1:80/WebSocket", "10.0.0.5"),
            "ws://10.0.0.5:80/WebSocket"
        );
        assert_eq!(
            with_host("ws://127.0.0.1/", "roaster.local"),
            "ws://roaster.local/"
        );
    }

    #[test]
    fn a_register_may_name_its_own_slave() {
        let e = register_entry("holding:4096@2:int16:et").unwrap();
        assert_eq!(e["address"], 4096);
        assert_eq!(e["unit_id"], 2);
        assert_eq!(e["name"], "et@2");
        assert!(register_entry("holding:4096@0:int16:et").is_err());
        assert!(register_entry("input:0:float32:bt")
            .unwrap()
            .get("unit_id")
            .is_none());
    }

    #[test]
    fn presets_fill_the_connection() {
        let mut a = add_args("serial_tc4");
        assert!(connection_config(&a).is_err(), "a port is needed");
        a.port = Some("/dev/ttyUSB0".into());
        a.preset = Some("stock-aartisan".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(c["port"], "/dev/ttyUSB0");
        assert_eq!(c["reply_format"], "active");
        a.preset = Some("tasmota".into());
        assert!(connection_config(&a).is_err(), "tasmota is MQTT");

        let mut a = add_args("websocket");
        a.device_url = Some("ws://x:80/ws".into());
        a.preset = Some("artisan-websocket".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(c["ingest"]["fields"]["bt"], "data.BT");
        assert!(c["poll"]["message"].as_str().unwrap().contains("{id}"));

        let mut a = add_args("mqtt");
        a.preset = Some("tasmota".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(c["telemetry_topic"], "tele/r1/SENSOR");
        a.preset = Some("esphome".into());
        let c = connection_config(&a).unwrap();
        assert_eq!(
            c["ingest"]["fields"]["et"]["topic"],
            "r1/sensor/environment_temperature/state"
        );

        let mut a = add_args("modbus_tcp");
        a.host = Some("10.0.0.5".into());
        a.port = Some("1502".into());
        a.unit_id = Some(3);
        let c = connection_config(&a).unwrap();
        assert_eq!(c, json!({ "host": "10.0.0.5", "port": 1502, "unit_id": 3 }));
    }

    fn map_args() -> MapArgs {
        MapArgs {
            roaster: "r1".into(),
            connection: None,
            fields: vec![],
            units: None,
            topics: vec![],
            column: None,
            registers: vec![],
            commands: vec![],
            command_topics: vec![],
            try_it: false,
        }
    }

    #[test]
    fn map_merges_fields_topics_units_and_templates() {
        let saved = json!({ "qos": 0, "ingest": { "fields": { "bt": "a.b", "fan": "Fan" } } });
        let mut m = map_args();
        m.fields = vec!["et=data.ET".into(), "fan=".into()];
        m.topics = vec!["exhaust=node/sensor/exhaust/state".into()];
        m.units = Some("F".into());
        m.commands = vec!["heater={value}".into()];
        m.command_topics = vec!["heater=cmnd/x/Dimmer".into()];
        let c = merged_config(&saved, "mqtt", &m).unwrap();
        assert_eq!(c["qos"], 0);
        assert_eq!(c["ingest"]["units"], "F");
        assert_eq!(c["ingest"]["fields"]["bt"], "a.b");
        assert_eq!(c["ingest"]["fields"]["et"], "data.ET");
        assert!(c["ingest"]["fields"].get("fan").is_none());
        assert_eq!(
            c["ingest"]["fields"]["exhaust"]["topic"],
            "node/sensor/exhaust/state"
        );
        assert_eq!(
            c["commands"]["heater"],
            json!({ "payload": "{value}", "topic": "cmnd/x/Dimmer" })
        );
        let mut m = map_args();
        m.column = Some("ambient,et,bt,,fan".into());
        let c = merged_config(&json!({ "port": "/dev/x" }), "serial_tc4", &m).unwrap();
        assert_eq!(
            c["ingest"]["columns"],
            json!(["ambient", "et", "bt", "", "fan"])
        );
        assert!(merged_config(&json!({}), "mqtt", &m).is_err());
    }

    #[test]
    fn register_specs_become_entries() {
        let e = register_entry("input:0:float32:bt:°F").unwrap();
        assert_eq!(e["register_type"], "input");
        assert_eq!(e["role"], "bt");
        assert_eq!(e["unit"], "°F");
        assert_eq!(e["writable"], false);
        assert_eq!(
            register_entry("holding:3:uint16:heater").unwrap()["writable"],
            true
        );
        assert!(register_entry("input:x:float32:bt").is_err());
        assert!(register_entry("input:0:float32").is_err());
        // A scale on the type (docs/connect-your-roaster.md's Modbus example)
        let e = register_entry("holding:0:int16*0.1:bt:°C").unwrap();
        assert_eq!(e["data_type"], "int16");
        assert_eq!(e["scale_factor"], 0.1);
        assert!(register_entry("holding:2:uint16:heater").unwrap()["scale_factor"].is_null());
        assert!(register_entry("holding:0:int16*0:bt").is_err());
        assert!(register_entry("holding:0:int16*x:bt").is_err());
    }
}
