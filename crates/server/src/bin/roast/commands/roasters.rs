//! The roasters: `roast roasters`, one roaster's setup, contract, tuning and model, and what each
//! reads now (`roast live`). Only reads; no command here controls a roaster.

use std::fmt::Write as _;

use clap::Subcommand;
use serde_json::{Value, json};

use crate::api::{Api, enc};
use crate::cli::Wait;
use crate::commands::adding;
use crate::commands::firmware::{FirmwareArgs, PasswordCommand, firmware, firmware_info, password};
use crate::commands::prediction::{auto_drop_brief, drive_line, prediction_lines, roaster_model};
use crate::out::{Failure, Out, code};
use crate::text::duration;

#[derive(Subcommand, Debug)]
pub enum RoasterCommand {
    /// Its setup: connections and transports, what CHARGE and DROP do, fan assist, whether
    /// tokens may control it
    Show {
        /// The roaster's id (esp32_roaster_01) or name
        roaster: String,
    },
    /// Against the device contract (docs/control.md): its firmware, the readings it sends,
    /// what it can do (caps), and whether its PID gains are the ones the server holds, with
    /// the last check after a boot. Only reads
    Contract { roaster: String },
    /// Add a roaster (docs/add-roaster.md): the roaster, its connection and, with --kind, its
    /// setup at control off (admin). It never raises control (`roaster control` asks the
    /// owner). Then `roaster map --try`, `roaster map`, `roaster check`
    Add(adding::AddArgs),
    /// Artisan's machine catalogue (docs/artisan-machines.md): the machines Roastwarden can set
    /// up from Artisan's own setup files (`--all`: every one, with why not). QUERY filters
    /// by brand, model or device; then `roaster add ID --artisan BRAND/MODEL`
    Machines {
        /// Words that must all match (coffed sr5)
        query: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Map its telemetry to Roastwarden's fields (docs/ingest-mapping.md): JSON paths, units,
    /// per-field topics, TC4 columns, Modbus registers by role, command templates. --try
    /// writes nothing and shows the newest message and what the mapping makes of it (admin to
    /// save)
    Map(adding::MapArgs),
    /// Its control level (docs/roaster-setup.md): off (logs only), manual or full (admin).
    /// Switching it on waits for the owner's approval; off runs at once
    Control {
        roaster: String,
        #[arg(value_parser = ["off", "manual", "full"])]
        level: String,
        #[command(flatten)]
        wait: Wait,
    },
    /// Firmware for an ESP32 roaster you build (docs/esp32-starter.md, docs/esp32-fluid-bed.md):
    /// with no roaster, what this server can fill in (the kinds, the MQTT port, the ids it has
    /// passwords for); with one, the sketch with its id, MQTT password, the server's address and
    /// the Wi-Fi filled in, a PlatformIO project in a zip the Arduino IDE opens too (admin)
    Firmware {
        /// The roaster's id, as it will be in Roastwarden (its MQTT user)
        roaster: Option<String>,
        /// starter (reads BT and ET) or fluid-bed (heater, fan, its own PID and safety)
        #[arg(long, default_value = "starter", value_parser = ["starter", "fluid-bed"])]
        kind: String,
        /// The address the ESP32 reaches the MQTT broker at; default the host `roast` calls
        #[arg(long)]
        address: Option<String>,
        /// The broker's port; default the built-in broker's, else 1883
        #[arg(long)]
        port: Option<u16>,
        /// The 2.4 GHz Wi-Fi network's name; left out, the sketch keeps its placeholder
        #[arg(long)]
        wifi: Option<String>,
        /// Read the Wi-Fi password from stdin (one line); none: an open network
        #[arg(long, requires = "wifi", conflicts_with = "password_stdin")]
        wifi_password_stdin: bool,
        /// Read the roaster's MQTT password from stdin; default the server's (`roaster password`)
        #[arg(long)]
        password_stdin: bool,
        /// The thermocouple board: max31855 or max6675
        #[arg(long, default_value = "max31855", value_parser = ["max31855", "max6675"])]
        board: String,
        /// One probe only (BT), no ET
        #[arg(long)]
        no_et: bool,
        /// Where to write the zip (`-`: stdout); default the server's name in this folder
        #[arg(short, long, value_name = "FILE")]
        output: Option<std::path::PathBuf>,
    },
    /// Who may sign in to the built-in MQTT broker as a roaster (docs/add-roaster.md, "MQTT
    /// passwords"): list them; add a new roaster's password (a random one, or
    /// --password-stdin), which counts at once, no restart; give one a new password (signs it
    /// out); remove one. Those in MQTT_DEVICES change in .env (admin)
    Password {
        #[command(subcommand)]
        action: PasswordCommand,
    },
    /// Against the interface standard: rate, BT and ET, units, outputs and channels echoed,
    /// caps, control; exit 0 ok, 1 warn or fail. --write (the owner key's, a cold roaster)
    /// sends heater 10 % and always ends with emergency stop
    Check {
        roaster: String,
        #[arg(long)]
        write: bool,
    },
    /// Its thermal model, fitted from its own roasts (docs/prediction.md): how well it
    /// predicts BT two minutes ahead (each roast left out of the fit in turn) beside today's
    /// projection, where BT settles at full heater, and how the live prediction did on the
    /// recent roasts. --fit refits it now (roast); the server refits by itself after a roast
    Model {
        roaster: String,
        #[arg(long)]
        fit: bool,
    },
    /// Its PID gains over time, the gain schedule, autotune runs and a test running
    Tuning {
        roaster: String,
        /// One autotune run: the gains each rule gives, or (a server run) its model, checks
        /// and designs
        #[arg(long)]
        run: Option<String>,
        /// With --run: preview a design on a server run's model (simc_pi, amigo_pi, zn_pi,
        /// tyreus_luyben_pi); read-only
        #[arg(long, requires = "run")]
        rule: Option<String>,
        /// With --run: SIMC's knob, τc as a multiple of the dead time (1 tight … 3 smooth,
        /// default 1.5); read-only
        #[arg(long, requires = "run")]
        tau_c: Option<f64>,
        /// With --run: a server run's record (time, heater as applied, BT, fan)
        #[arg(long, requires = "run")]
        samples: bool,
    },
}

/// `roast roaster …`: the subcommand to its function.
pub async fn run(api: &Api, out: &Out, cmd: RoasterCommand) -> Result<u8, Failure> {
    match cmd {
        RoasterCommand::Show { roaster } => roaster_show(api, out, roaster).await,
        RoasterCommand::Contract { roaster } => roaster_contract(api, out, roaster).await,
        RoasterCommand::Add(a) => adding::add(api, out, a).await,
        RoasterCommand::Map(m) => adding::map(api, out, m).await,
        RoasterCommand::Machines { query, all } => {
            let query = (!query.is_empty()).then(|| query.join(" "));
            adding::machines(api, out, query, all).await
        }
        RoasterCommand::Control {
            roaster,
            level,
            wait,
        } => adding::control(api, out, roaster, level, wait).await,
        RoasterCommand::Firmware {
            roaster,
            kind,
            address,
            port,
            wifi,
            wifi_password_stdin,
            password_stdin,
            board,
            no_et,
            output,
        } => match roaster {
            None => firmware_info(api, out).await,
            Some(roaster) => {
                let f = FirmwareArgs {
                    roaster,
                    kind,
                    address,
                    port,
                    wifi,
                    wifi_password_stdin,
                    password_stdin,
                    board,
                    no_et,
                    output,
                };
                firmware(api, out, f).await
            }
        },
        RoasterCommand::Password { action } => password(api, out, action).await,
        RoasterCommand::Check { roaster, write } => adding::check(api, out, roaster, write).await,
        RoasterCommand::Model { roaster, fit } => roaster_model(api, out, roaster, fit).await,
        RoasterCommand::Tuning {
            roaster,
            run,
            rule,
            tau_c,
            samples,
        } => {
            let view = match (run, samples, rule.is_some() || tau_c.is_some()) {
                (Some(id), true, _) => RunView::Samples(id),
                (Some(id), false, true) => RunView::Design { id, rule, tau_c },
                (Some(id), false, false) => RunView::Run(id),
                (None, ..) => RunView::None,
            };
            roaster_tuning(api, out, roaster, view).await
        }
    }
}

/// Seconds since a Unix time.
fn age(ts: i64) -> i64 {
    chrono::Utc::now().timestamp() - ts
}

/// When a roaster was last heard: its newest telemetry, else its status message.
async fn heard(
    api: &Api,
    device_id: &str,
    registry_last_seen: Option<i64>,
) -> (Option<i64>, Value) {
    let latest = api
        .get(&format!("/api/roaster/{}/telemetry/latest", enc(device_id)))
        .await
        .unwrap_or(Value::Null);
    let ts = latest["timestamp"]
        .as_i64()
        .into_iter()
        .chain(registry_last_seen)
        .max();
    (ts, latest)
}

pub async fn roasters(api: &Api, out: &Out) -> Result<u8, Failure> {
    let devices = api.get("/api/devices").await?;
    let registry = api
        .get("/api/devices/registry")
        .await
        .unwrap_or(Value::Null);
    let waiting = api
        .get("/api/devices/discovered")
        .await
        .unwrap_or(Value::Null);
    let reg: Vec<Value> = registry["devices"].as_array().cloned().unwrap_or_default();
    let mut ids: Vec<String> = devices
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["device_id"].as_str().map(str::to_string))
        .collect();
    let waiting: Vec<String> = waiting
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["device_id"].as_str().map(str::to_string))
        .collect();
    for id in reg
        .iter()
        .filter_map(|d| d["device_id"].as_str().map(str::to_string))
        .chain(waiting.iter().cloned())
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let mut rows = Vec::new();
    for id in ids {
        let dev = devices
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["device_id"] == id.as_str());
        let r = reg.iter().find(|d| d["device_id"] == id.as_str());
        let (ts, _) = heard(api, &id, r.and_then(|r| r["last_seen"].as_i64())).await;
        // What Roastwarden may send it (docs/roaster-setup.md): off logs only
        let setup = optional(api, &format!("/api/roaster/{}/setup", enc(&id))).await;
        rows.push(json!({
            "device_id": id,
            "name": dev.map(|d| d["name"].clone()),
            "status": dev.map(|d| d["status"].clone()),
            "configured": dev.is_some(),
            "online": ts.is_some_and(|t| age(t) < 60),
            "last_heard_s_ago": ts.map(age),
            "firmware": r.map(|r| r["version"].clone()),
            "kind": setup["kind"].clone(),
            "control": setup["control"].clone(),
            // Announced itself; the owner sets it up in Roasters
            "waiting_to_be_set_up": waiting.contains(&id),
        }));
    }
    out.emit(&rows, |rows| {
        let lines: Vec<String> = rows
            .iter()
            .map(|r| {
                format!(
                    "{:<20} {:<24} {}{}{}{}",
                    r["device_id"].as_str().unwrap_or("?"),
                    r["name"].as_str().unwrap_or("(not set up)"),
                    if r["online"] == json!(true) {
                        "online".to_string()
                    } else {
                        r["last_heard_s_ago"]
                            .as_i64()
                            .map_or("never heard".into(), |s| {
                                format!("offline, last heard {} ago", duration(s))
                            })
                    },
                    r["firmware"]
                        .as_str()
                        .map(|v| format!(", firmware {v}"))
                        .unwrap_or_default(),
                    control_text(&r["control"]),
                    if r["waiting_to_be_set_up"] == json!(true) {
                        ", waiting to be set up (Roasters)"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        if lines.is_empty() {
            "No roasters".into()
        } else {
            lines.join("\n")
        }
    });
    Ok(code::OK)
}

/// The control level in words, after a comma (docs/roaster-setup.md).
fn control_text(c: &Value) -> String {
    match c.as_str() {
        Some("off") => ", control off (logs only)".into(),
        Some("manual") => ", control manual (heater, fan, drum, damper, STOP)".into(),
        Some("full") => ", control full".into(),
        _ => String::new(),
    }
}

/// A serial TC4 connection's port, `CHAN` mapping, reply format and fan output, read the
/// way the server reads a setting left out (hardware-integration-guide.md, "Serial
/// Connection"); empty for any other connection.
fn tc4_text(c: &Value) -> String {
    if c["protocol"] != "serial_tc4" {
        return String::new();
    }
    let cfg = &c["config"];
    let chan = |k: &str, d: u64| cfg[k].as_u64().unwrap_or(d).min(9);
    format!(
        " {} CHAN;{}{}{}{}, {} replies, fan {}",
        cfg["port"].as_str().unwrap_or("?"),
        chan("et_channel", 1),
        chan("bt_channel", 2),
        chan("t3_channel", 0),
        chan("t4_channel", 0),
        cfg["reply_format"].as_str().unwrap_or("fixed"),
        cfg["fan_output"].as_str().unwrap_or("io3"),
    )
}

/// A Phidget22 connection's server, board and thermocouple type (docs/phidgets.md), and the
/// default mapping when it has none; empty for any other connection.
fn phidget_text(c: &Value) -> String {
    if c["protocol"] != "phidget22" {
        return String::new();
    }
    let cfg = &c["config"];
    let mut s = format!(
        " {}:{}",
        cfg["host"].as_str().unwrap_or("?"),
        cfg["port"].as_u64().unwrap_or(5661)
    );
    match (cfg["serial_number"].as_i64(), cfg["hub_port"].as_i64()) {
        (Some(n), Some(p)) => {
            let _ = write!(s, ", board #{n} port {p}");
        }
        (Some(n), None) => {
            let _ = write!(s, ", board #{n}");
        }
        (None, Some(p)) => {
            let _ = write!(s, ", hub port {p}");
        }
        (None, None) => s.push_str(", the first board"),
    }
    let _ = write!(
        s,
        ", type {}",
        cfg["thermocouple_type"].as_str().unwrap_or("K")
    );
    if cfg["password"].is_string() {
        s.push_str(", password saved");
    }
    if !cfg["ingest"].is_object() {
        s.push_str(", BT ch0, ET ch1");
    }
    s
}

/// A connection's own field names, units, topics and command templates in a line
/// (docs/ingest-mapping.md): `mapping °F; bt ← MAX31855.Temperature, fan ← Fan ×0.39;
/// telemetry tele/roaster/SENSOR; templates heater drum`. Empty when it has none.
fn mapping_text(c: &Value, device_id: &str) -> String {
    let cfg = &c["config"];
    let ingest = &cfg["ingest"];
    let mut parts: Vec<String> = Vec::new();
    if ingest.is_object() {
        let mut fields: Vec<String> = Vec::new();
        if let Some(cols) = ingest["columns"].as_array() {
            let names: Vec<&str> = cols
                .iter()
                .map(|c| c.as_str().filter(|s| !s.is_empty()).unwrap_or("-"))
                .collect();
            fields.push(format!("columns {}", names.join(",")));
        }
        for (key, spec) in ingest["fields"].as_object().into_iter().flatten() {
            let path = spec
                .as_str()
                .or_else(|| spec["path"].as_str())
                .unwrap_or("$");
            let mut s = format!("{key} ← {path}");
            if let Some(t) = spec["topic"].as_str() {
                let _ = write!(s, " on {t}");
            }
            if let Some(k) = spec["scale"].as_f64().filter(|k| *k != 1.0) {
                let _ = write!(s, " ×{k}");
            }
            if let Some(o) = spec["offset"].as_f64().filter(|o| *o != 0.0) {
                let _ = write!(s, " {o:+}");
            }
            if let Some(u) = spec["unit"].as_str() {
                let _ = write!(s, " °{u}");
            }
            fields.push(s);
        }
        let units = ingest["units"].as_str().unwrap_or("C");
        parts.push(if fields.is_empty() {
            format!("°{units}")
        } else {
            format!("°{units}; {}", fields.join(", "))
        });
    }
    // Topics only where they are not the contract's `roaster/<id>/…`.
    let default_prefix = format!("roaster/{device_id}");
    let prefix = cfg["topic_prefix"]
        .as_str()
        .map_or(default_prefix.as_str(), |p| p.trim_end_matches('/'));
    if let Some(t) = cfg["telemetry_topic"]
        .as_str()
        .filter(|t| *t != format!("{prefix}/telemetry"))
    {
        parts.push(format!("telemetry {t}"));
    }
    if prefix != default_prefix {
        parts.push(format!("commands under {prefix}/control"));
    }
    if let Some(p) = cfg["poll"]["message"].as_str() {
        parts.push(format!(
            "polls {p} every {} ms",
            cfg["poll"]["interval_ms"].as_u64().unwrap_or(1000)
        ));
    }
    if let Some(cmds) = cfg["commands"].as_object().filter(|m| !m.is_empty()) {
        parts.push(format!(
            "templates {}",
            cmds.keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("mapping {}", parts.join("; "))
    }
}

/// A read that may not apply to this roaster (no fan assist on a Modbus box): Null then.
async fn optional(api: &Api, path: &str) -> Value {
    api.get(path).await.unwrap_or(Value::Null)
}

pub async fn roaster_show(api: &Api, out: &Out, roaster: String) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let base = format!("/api/roaster/{}", enc(&device_id));
    // Only heard on MQTT, not set up in Roasters: no device record
    let mut one = match r["registry_id"].as_str() {
        Some(reg_id) => api.get(&format!("/api/devices/{}", enc(reg_id))).await?,
        None => json!({ "device_id": device_id, "name": device_id, "status": "not set up" }),
    };
    one["transports"] = optional(api, &format!("{base}/transports")).await;
    one["sequence"] = optional(api, &format!("{base}/sequence")).await;
    one["fan_assist"] = optional(api, &format!("{base}/fan_assist")).await;
    one["token_access"] = optional(api, &format!("{base}/token-access")).await;
    one["setup"] = optional(api, &format!("{base}/setup")).await;
    out.emit(&one, |d| {
        let mut lines = vec![format!(
            "{} ({}) {}",
            d["device_id"].as_str().unwrap_or("?"),
            d["name"].as_str().unwrap_or("?"),
            d["status"].as_str().unwrap_or("?")
        )];
        for c in d["connections"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  connection {}{}{}{}",
                c["protocol"].as_str().unwrap_or("?"),
                tc4_text(c),
                phidget_text(c),
                if c["enabled"] == json!(false) {
                    " (off)"
                } else {
                    ""
                }
            ));
            let mapping = mapping_text(c, d["device_id"].as_str().unwrap_or_default());
            if !mapping.is_empty() {
                lines.push(format!("    {mapping}"));
            }
        }
        lines.push(format!(
            "  {}{}",
            d["setup"]["kind"].as_str().unwrap_or("kind unknown"),
            control_text(&d["setup"]["control"])
        ));
        lines.push(format!("  {}", outputs_text(&d["setup"])));
        lines.push(format!("  {}", channels_text(&d["setup"])));
        lines.push(format!(
            "  tokens may control it: {}",
            d["token_access"]["tokens_may_control"]
        ));
        lines.join("\n")
    });
    Ok(code::OK)
}

/// The declared channels in a line (docs/channels.md):
/// `channels t3 Exhaust °C, burner_kpa Gas kPa (top 4)`.
fn channels_text(setup: &Value) -> String {
    let list: Vec<String> = setup["channels"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            let unit = match c["unit"].as_str().unwrap_or("?") {
                "C" => "°C",
                u => u,
            };
            format!(
                "{} {} {unit}{}",
                c["key"].as_str().unwrap_or("?"),
                c["label"].as_str().unwrap_or("?"),
                c["max"]
                    .as_f64()
                    .map_or(String::new(), |m| format!(" (top {m})"))
            )
        })
        .collect();
    if list.is_empty() {
        "channels: none declared (extras stay live only)".into()
    } else {
        format!("channels {}", list.join(", "))
    }
}

/// The outputs and fan floor in a line (docs/roaster-setup.md, "Outputs"):
/// `outputs Heater 0–100 %, Fan 0–100 %; fan floor 45 % while heating`.
fn outputs_text(setup: &Value) -> String {
    let one = |o: &Value| -> Option<String> {
        if o.is_null() {
            return None;
        }
        let step = o["step"].as_f64().unwrap_or(0.0);
        let n = |v: &Value| v.as_f64().map_or("?".into(), |x| x.to_string());
        Some(format!(
            "{} {}–{} %{}",
            o["label"].as_str().unwrap_or("?"),
            n(&o["min"]),
            n(&o["max"]),
            if step > 0.0 {
                format!(" in {step} % steps")
            } else {
                String::new()
            }
        ))
    };
    let outs = &setup["outputs"];
    if outs.is_null() {
        return "outputs unknown".into();
    }
    // Drum and damper (docs/ingest-mapping.md) only when declared: absent is null.
    let parts: Vec<String> = [
        &outs["heater"],
        &outs["fan"],
        &outs["drum"],
        &outs["damper"],
    ]
    .into_iter()
    .filter_map(one)
    .collect();
    if parts.is_empty() {
        return "no outputs: nothing is ever sent but STOP".into();
    }
    let floor = setup["fan_floor_pct"].as_f64().unwrap_or(0.0);
    format!(
        "outputs {}{}",
        parts.join(", "),
        if floor > 0.0 {
            format!("; fan floor {floor} % while heating")
        } else {
            String::new()
        }
    )
}

/// Gains in a line, `Kp 5.561 Ki 0.08471 Kd 0`.
fn gains_text(g: &Value) -> String {
    if g.is_null() {
        return "none".into();
    }
    format!("Kp {} Ki {} Kd {}", g["kp"], g["ki"], g["kd"])
}

pub async fn roaster_contract(api: &Api, out: &Out, roaster: String) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let c = api
        .get(&format!("/api/roaster/{}/contract", enc(&device_id)))
        .await?;
    out.emit(&c, |c| {
        let yes = |v: &Value| v == &json!(true);
        let reads: Vec<&str> = ["bt", "et", "heater", "fan"]
            .into_iter()
            .filter(|k| !yes(&c["reads"][*k]))
            .collect();
        let caps: Vec<&str> = ["pid", "ff", "lease", "tpc", "ror"]
            .into_iter()
            .filter(|k| yes(&c["caps"][*k]))
            .collect();
        let g = &c["gains"];
        let mut lines = vec![
            format!(
                "{} firmware {}",
                c["device_id"].as_str().unwrap_or("?"),
                c["firmware"].as_str().unwrap_or("unknown")
            ),
            if reads.is_empty() {
                "  reads BT, ET, heater and fan".into()
            } else {
                format!("  missing in its latest sample: {}", reads.join(", "))
            },
            format!(
                "  caps {} ({})",
                if caps.is_empty() {
                    "none".into()
                } else {
                    caps.join(", ")
                },
                if yes(&c["caps"]["reported"]) {
                    "it reports them"
                } else {
                    "inferred"
                }
            ),
            format!(
                "  gains reported {}, held {}{}",
                gains_text(&g["reported"]),
                gains_text(&g["held"]),
                match g["matches"].as_bool() {
                    Some(true) => ": they match",
                    Some(false) => ": they differ",
                    None => "",
                }
            ),
        ];
        let b = &g["last_boot"];
        if !b.is_null() {
            lines.push(format!(
                "  last boot check {} ({}): came up with {}, {}",
                b["at"].as_str().unwrap_or("?"),
                b["why"].as_str().unwrap_or("?"),
                gains_text(&b["reported"]),
                if b["sent"].is_null() {
                    "nothing sent".to_string()
                } else {
                    format!("sent {}", gains_text(&b["sent"]))
                }
            ));
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

/// What `roaster tuning --run` reads.
pub enum RunView {
    None,
    Run(String),
    Design {
        id: String,
        rule: Option<String>,
        tau_c: Option<f64>,
    },
    Samples(String),
}

pub async fn roaster_tuning(
    api: &Api,
    out: &Out,
    roaster: String,
    view: RunView,
) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let base = format!("/api/roaster/{}", enc(&device_id));
    match view {
        RunView::Run(run) => {
            let one = api
                .get(&format!("{base}/autotune/runs/{}", enc(&run)))
                .await?;
            out.emit(&one, |r| {
                if r["source"] == "server" {
                    server_run_text(r)
                } else {
                    serde_json::to_string_pretty(r).unwrap_or_default()
                }
            });
            return Ok(code::OK);
        }
        RunView::Design { id, rule, tau_c } => {
            let mut q = Vec::new();
            if let Some(r) = rule {
                q.push(format!("rule={}", enc(&r)));
            }
            if let Some(t) = tau_c {
                q.push(format!("tau_c={t}"));
            }
            let d = api
                .get(&format!(
                    "{base}/autotune/runs/{}/design?{}",
                    enc(&id),
                    q.join("&")
                ))
                .await?;
            out.emit(&d, |d| design_line(&d["design"], &d["gains"]));
            return Ok(code::OK);
        }
        RunView::Samples(run) => {
            let s = api
                .get(&format!("{base}/autotune/runs/{}/samples", enc(&run)))
                .await?;
            out.emit(&s, |s| serde_json::to_string(s).unwrap_or_default());
            return Ok(code::OK);
        }
        RunView::None => {}
    }
    let answer = json!({
        "device_id": device_id,
        "pid": optional(api, &format!("{base}/control/pid/history")).await,
        "gain_schedule": optional(api, &format!("{base}/gain_schedule")).await,
        "test": optional(api, &format!("{base}/tune")).await,
        "runs": optional(api, &format!("{base}/autotune/runs")).await,
    });
    out.emit(&answer, |a| {
        serde_json::to_string_pretty(a).unwrap_or_default()
    });
    Ok(code::OK)
}

/// One design in a line: its gains (the roaster's units) and what they cost.
fn design_line(d: &Value, gains: &Value) -> String {
    let n = |v: &Value| v.as_f64();
    let r = &d["robustness"];
    let fmt =
        |v: Option<f64>, digits: usize| v.map_or("-".to_string(), |x| format!("{x:.digits$}"));
    format!(
        "{}: Kp {} Ki {} (roaster units) | Ms {} GM {} PM {}° | jitter ±{} % | ramp lag {} °C at 10 °C/min | {}{}",
        d["label"].as_str().unwrap_or("?"),
        fmt(n(&gains["kp"]), 2),
        fmt(n(&gains["ki"]), 4),
        fmt(n(&r["ms"]), 2),
        fmt(n(&r["gain_margin"]), 1),
        fmt(n(&r["phase_margin_deg"]), 0),
        fmt(n(&d["jitter"]), 1),
        fmt(n(&d["ramp_lag"]), 1),
        d["verdict"].as_str().unwrap_or("?"),
        d["notes"]
            .as_array()
            .filter(|a| !a.is_empty())
            .map(|a| format!(
                " ({})",
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
            .unwrap_or_default()
    )
}

/// A server run in words: the test, the model, the checks, every design.
fn server_run_text(r: &Value) -> String {
    let a = &r["analysis"];
    let mut lines = vec![format!(
        "Autotune {} ({}), {} at {} °C, fan {} %: {}",
        r["id"].as_str().unwrap_or("?"),
        r["created_at"].as_str().unwrap_or("?"),
        r["state"].as_str().unwrap_or("?"),
        r["test_temperature"]
            .as_f64()
            .map_or("?".into(), |v| format!("{v:.1}")),
        r["fan_pct"]
            .as_f64()
            .map_or("?".into(), |v| format!("{v:.0}")),
        r["quality"].as_str().unwrap_or("not analysed"),
    )];
    if a.is_null() {
        return lines.join("\n");
    }
    let m = &a["design_model"];
    if !m.is_null() {
        let kv = if m["integrating"] == json!(true) {
            m["k"].as_f64()
        } else {
            m["k"].as_f64().zip(m["t1"].as_f64()).map(|(k, t)| k / t)
        };
        lines.push(format!(
            "  model: {} | kv {} °C/s per % | dead time {} s | holding power {} %",
            a["design_basis"].as_str().unwrap_or(""),
            kv.map_or("?".into(), |v| format!("{v:.4}")),
            m["l"].as_f64().map_or("?".into(), |v| format!("{v:.1}")),
            a["fitted_holding_power"]
                .as_f64()
                .or(a["holding_power"].as_f64())
                .map_or("?".into(), |v| format!("{v:.0}")),
        ));
    }
    for f in a["fits"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  fit {}: {:.1} % | residual {} × noise | AIC {:.0}",
            f["order"].as_str().unwrap_or("?"),
            f["fit_pct"].as_f64().unwrap_or(0.0),
            f["residual_ratio"]
                .as_f64()
                .map_or("?".into(), |v| format!("{v:.2}")),
            f["aic"].as_f64().unwrap_or(0.0),
        ));
    }
    for w in a["warnings"].as_array().into_iter().flatten() {
        lines.push(format!("  ! {}", w["text"].as_str().unwrap_or("")));
    }
    let rec = a["recommended"].as_u64();
    for (i, d) in a["designs"].as_array().into_iter().flatten().enumerate() {
        let r = &d["pid"];
        let gains = json!({
            "kp": r["kp"].as_f64().map(|v| v * 2.55),
            "ki": r["ki"].as_f64().map(|v| v * 2.55),
        });
        lines.push(format!(
            "  {} {}",
            if Some(i as u64) == rec { "*" } else { " " },
            design_line(d, &gains)
        ));
    }
    lines.join("\n")
}

pub async fn live(api: &Api, out: &Out, roaster: Option<String>) -> Result<u8, Failure> {
    let ids: Vec<String> = match roaster {
        Some(r) => vec![
            api.roaster(&r).await?["device_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        ],
        None => {
            let reg = api.get("/api/devices/registry").await?;
            reg["devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d["device_id"].as_str().map(str::to_string))
                .collect()
        }
    };
    let drives = api.get("/api/replay").await.unwrap_or(Value::Null);
    let mut rows = Vec::new();
    for id in ids {
        let (ts, latest) = heard(api, &id, None).await;
        let roasts = api
            .get(&format!("/api/sessions?device_id={}&limit=5", enc(&id)))
            .await
            .unwrap_or(Value::Null);
        let active = roasts
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| matches!(s["status"].as_str(), Some("active" | "paused")));
        let drive = drives
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["device_id"] == id.as_str())
            .cloned()
            .or(match active {
                Some(s) => api
                    .get(&format!(
                        "/api/sessions/{}/replay",
                        enc(s["id"].as_str().unwrap_or_default())
                    ))
                    .await
                    .ok()
                    // Running, or stopped by a heater cut or a restart with a resume offered
                    .filter(|d| {
                        d["active"] == json!(true)
                            || d["stopped_by_device"] == json!(true)
                            || d["stopped_by_restart"] == json!(true)
                    }),
                None => None,
            });
        let prediction = match active {
            Some(_) => api
                .get(&format!("/api/roaster/{}/prediction", enc(&id)))
                .await
                .ok(),
            None => None,
        };
        rows.push(json!({
            "device_id": id,
            "online": ts.is_some_and(|t| age(t) < 60),
            "prediction": prediction,
            "reading_age_s": latest["timestamp"].as_i64().map(age),
            "reading": latest["telemetry"],
            "roast": active.map(|s| json!({ "id": s["id"], "name": s["name"],
                "status": s["status"], "start_time": s["start_time"] })),
            "drive": drive.map(|d| {
                let mut v = json!({
                    "profile_name": d["profile_name"], "active": d["active"],
                    "elapsed_seconds": d["elapsed_seconds"],
                    "profile_t": d["profile_t"],
                    "last_sent_setpoint": d["last_sent_setpoint"],
                    "stopped_by_device": d["stopped_by_device"],
                    "stopped_by_restart": d["stopped_by_restart"],
                    "hold_seconds": d["config"]["hold"]["seconds"],
                    "auto_drop": auto_drop_brief(&d),
                });
                if d["active"] != json!(true) {
                    v["stopped_reason"] = d["stopped_reason"].clone();
                    // Heater on and the drive with its old settings (a restart's at profile_t)
                    v["resume"] = json!(format!(
                        "roast api POST /api/sessions/{}/drive/resume",
                        d["session_id"].as_str().unwrap_or_default()
                    ));
                }
                v
            }),
        }));
    }
    out.emit(&rows, |rows| {
        if rows.is_empty() {
            return "No roasters heard yet".into();
        }
        rows.iter()
            .map(|r| {
                let t = &r["reading"];
                let n = |k: &str| t[k].as_f64().map_or("-".to_string(), |v| format!("{v:.1}"));
                format!(
                    "{}: {}  BT {}  ET {}  RoR {}  setpoint {}  heater {}  fan {}{}{}",
                    r["device_id"].as_str().unwrap_or("?"),
                    if r["online"] == json!(true) {
                        "online"
                    } else {
                        "offline"
                    },
                    n("beanTemp"),
                    n("envTemp"),
                    n("rateOfRise"),
                    n("setpoint"),
                    n("heater"),
                    n("fan"),
                    r["roast"]["name"]
                        .as_str()
                        .map(|s| format!(
                            "\n  roast: {s} ({})",
                            r["roast"]["status"].as_str().unwrap_or("?")
                        ))
                        .unwrap_or_default(),
                    r["drive"]["profile_name"]
                        .as_str()
                        .map(|p| drive_line(p, &r["drive"]))
                        .unwrap_or_default(),
                ) + &prediction_lines(&r["prediction"])
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(code::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_in_a_line() {
        let two = json!({ "channels": [
            { "key": "t3", "label": "Exhaust", "unit": "C", "max": null },
            { "key": "burner_kpa", "label": "Gas", "unit": "kPa", "max": 4.0 }
        ] });
        assert_eq!(
            channels_text(&two),
            "channels t3 Exhaust °C, burner_kpa Gas kPa (top 4)"
        );
        assert!(channels_text(&json!({ "channels": [] })).starts_with("channels: none"));
    }

    #[test]
    fn outputs_in_a_line() {
        let fb = json!({
            "outputs": {
                "heater": { "label": "Heater", "min": 0.0, "max": 100.0, "step": 0.0 },
                "fan": { "label": "Fan", "min": 0.0, "max": 100.0, "step": 0.0 }
            },
            "fan_floor_pct": 45.0
        });
        assert_eq!(
            outputs_text(&fb),
            "outputs Heater 0–100 %, Fan 0–100 %; fan floor 45 % while heating"
        );
        let logger = json!({ "outputs": { "heater": null, "fan": null }, "fan_floor_pct": 0.0 });
        assert!(outputs_text(&logger).starts_with("no outputs"));
        let gas = json!({
            "outputs": { "heater": { "label": "Burner", "min": 20, "max": 80, "step": 10 }, "fan": null },
            "fan_floor_pct": 0
        });
        assert_eq!(outputs_text(&gas), "outputs Burner 20–80 % in 10 % steps");
        let drum = json!({
            "outputs": { "heater": { "label": "Burner", "min": 0, "max": 100, "step": 0 },
                "fan": null, "drum": { "label": "Drum", "min": 30, "max": 70, "step": 0 },
                "damper": null },
            "fan_floor_pct": 0
        });
        assert_eq!(outputs_text(&drum), "outputs Burner 0–100 %, Drum 30–70 %");
    }

    #[test]
    fn a_connection_shows_its_mapping_in_a_line() {
        let tasmota = json!({ "protocol": "mqtt", "config": {
            "telemetry_topic": "tele/roaster/SENSOR",
            "ingest": { "units": "F", "fields": {
                "bt": "MAX31855.Temperature",
                "heater": { "path": "out.burner", "scale": 0.39 },
                "et": { "topic": "node/et/state" } } },
            "commands": { "heater": { "topic": "cmnd/roaster/Dimmer" }, "drum": {} } } });
        assert_eq!(
            mapping_text(&tasmota, "tasmota"),
            "mapping °F; bt ← MAX31855.Temperature, et ← $ on node/et/state, heater ← \
             out.burner ×0.39; telemetry tele/roaster/SENSOR; templates drum heater"
        );
        let tc4 = json!({ "protocol": "serial_tc4", "config": { "port": "x",
            "ingest": { "columns": ["", "et", "bt"] } } });
        assert_eq!(mapping_text(&tc4, "x"), "mapping °C; columns -,et,bt");
        let ws = json!({ "protocol": "websocket", "config": { "url": "ws://x",
            "poll": { "message": "{\"command\":\"getData\"}", "interval_ms": 500 } } });
        assert_eq!(
            mapping_text(&ws, "x"),
            "mapping polls {\"command\":\"getData\"} every 500 ms"
        );
        // The ESP32's default config has none.
        let esp = json!({ "protocol": "mqtt", "config": { "topic_prefix": "roaster/esp32_roaster_01", "qos": 0 } });
        assert_eq!(mapping_text(&esp, "esp32_roaster_01"), "");
        let moved = json!({ "protocol": "mqtt", "config": { "topic_prefix": "home/roaster/" } });
        assert_eq!(
            mapping_text(&moved, "r"),
            "mapping commands under home/roaster/control"
        );
    }

    #[test]
    fn a_tc4_connection_shows_its_mapping_reply_and_fan() {
        let old = json!({ "protocol": "serial_tc4", "config": { "port": "/dev/ttyUSB0" } });
        assert_eq!(
            tc4_text(&old),
            " /dev/ttyUSB0 CHAN;1200, fixed replies, fan io3"
        );
        let stock = json!({ "protocol": "serial_tc4", "config": { "port": "COM5",
            "et_channel": 1, "bt_channel": 2, "t3_channel": 3, "reply_format": "active",
            "fan_output": "ot2" } });
        assert_eq!(tc4_text(&stock), " COM5 CHAN;1230, active replies, fan ot2");
        assert_eq!(tc4_text(&json!({ "protocol": "mqtt", "config": {} })), "");
        let ph = json!({ "protocol": "phidget22", "config": { "host": "sbc.local",
            "serial_number": 370_001, "password": "********" } });
        assert_eq!(
            phidget_text(&ph),
            " sbc.local:5661, board #370001, type K, password saved, BT ch0, ET ch1"
        );
    }
}
