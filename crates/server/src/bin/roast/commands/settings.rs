//! The server's own settings: `roast settings`, `modules`, `notifications` and `home-assistant`.

use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{enc, Api};
use crate::out::{code, Failure, Out};
use roastwarden_server::modules::{ALL as MODULES, PRESETS};

#[derive(Subcommand, Debug)]
pub enum SettingsCommand {
    /// Write settings, KEY=VALUE (several at once: all or none); VALUE `-` reads stdin, e.g.
    /// the alarm rules: roast settings set roast_alarms=- < alarms.json
    Set {
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
    },
}

pub async fn notifications(api: &Api, out: &Out, test: Option<String>) -> Result<u8, Failure> {
    if let Some(channel) = test {
        let channel = channel.trim().to_ascii_lowercase();
        let r = api
            .send(
                Method::POST,
                "/api/notifications/test",
                Some(&json!({ "channel": channel, "values": {} })),
                Duration::from_secs(60),
            )
            .await?;
        out.emit(&r.body, |b| match b {
            Value::String(s) => s.clone(),
            other => format!("sent through {channel}: {other}"),
        });
        return Ok(code::OK);
    }
    let mut o = api.get("/api/notifications").await?;
    o["push_devices"] = api
        .get("/api/push/subscriptions")
        .await
        .map_or(Value::Null, |list| {
            Value::Array(
                list.as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| json!({ "id": d["id"], "name": d["name"], "created_at": d["created_at"] }))
                    .collect(),
            )
        });
    out.emit(&o, |o| {
        let mut lines = Vec::new();
        for (name, c) in o["channels"].as_object().into_iter().flatten() {
            let on = c["enabled"] == json!(true);
            let st = &c["status"];
            lines.push(format!(
                "{:<8} {}{}",
                name,
                if on { "on" } else { "off" },
                match (
                    st["last_error"].as_str(),
                    st["last_error_at"].as_str(),
                    st["last_ok_at"].as_str()
                ) {
                    (Some(e), Some(at), ok) if ok.is_none_or(|ok| at > ok) => {
                        format!(", last send FAILED ({at}): {e}")
                    }
                    (_, _, Some(ok)) => format!(", last sent {ok}"),
                    _ => String::new(),
                }
            ));
        }
        for p in o["problems"].as_array().into_iter().flatten() {
            lines.push(format!("problem: {}", p["title"].as_str().unwrap_or("?")));
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

/// A setting's value as JSON when it is (the alarm rules are), else the text.
fn value_of(v: &Value) -> Value {
    match v {
        Value::String(s) if s.trim_start().starts_with(['[', '{']) => {
            serde_json::from_str(s).unwrap_or_else(|_| v.clone())
        }
        other => other.clone(),
    }
}

/// "modules: reports, cupping (custom); off: tuning, agents, costs, labels"
pub fn modules_line(on: &[Value], preset: &Value) -> String {
    let on: Vec<&str> = on.iter().filter_map(Value::as_str).collect();
    let off: Vec<&str> = MODULES
        .iter()
        .copied()
        .filter(|m| !on.contains(m))
        .collect();
    format!(
        "modules: {} ({}){}",
        if on.is_empty() {
            "none".to_string()
        } else {
            on.join(", ")
        },
        preset.as_str().unwrap_or("?"),
        if off.is_empty() {
            String::new()
        } else {
            format!("; off: {}", off.join(", "))
        }
    )
}

/// The modules with --preset, --on and --off applied, in their order.
fn modules_after(
    current: &[String],
    preset: Option<&str>,
    on: &[String],
    off: &[String],
) -> Result<Vec<&'static str>, Failure> {
    for m in on.iter().chain(off) {
        if !MODULES.contains(&m.as_str()) {
            return Err(Failure::new(
                "USAGE",
                format!("{m:?} is not a module: they are {}", MODULES.join(", ")),
            ));
        }
    }
    if let Some(m) = on.iter().find(|m| off.contains(m)) {
        return Err(Failure::new("USAGE", format!("{m} is both --on and --off")));
    }
    let start: Vec<&str> = match preset {
        Some(p) => PRESETS
            .iter()
            .find(|(name, _)| *name == p)
            .map(|(_, list)| list.to_vec())
            .unwrap_or_default(),
        None => current.iter().map(String::as_str).collect(),
    };
    Ok(MODULES
        .iter()
        .copied()
        .filter(|m| (start.contains(m) || on.iter().any(|o| o == m)) && !off.iter().any(|o| o == m))
        .collect())
}

/// `roast modules`: show the dashboard's optional parts, or change them (admin).
pub async fn modules(
    api: &Api,
    out: &Out,
    preset: Option<String>,
    on: Vec<String>,
    off: Vec<String>,
) -> Result<u8, Failure> {
    let about = api.get("/api/about").await?;
    let changing = preset.is_some() || !on.is_empty() || !off.is_empty();
    let about = if changing {
        let current: Vec<String> = about["modules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect();
        let next = modules_after(&current, preset.as_deref(), &on, &off)?;
        api.call(
            Method::PUT,
            &format!("/api/settings/{}", roastwarden_server::modules::SETTING),
            Some(&json!({ "value": json!(next).to_string() })),
        )
        .await?;
        api.get("/api/about").await?
    } else {
        about
    };
    let answer = json!({
        "modules": about["modules"],
        "preset": about["preset"],
        "first_run": about["first_run"],
        "changed": changing,
    });
    out.emit(&answer, |a| {
        modules_line(
            a["modules"].as_array().map_or(&[][..], Vec::as_slice),
            &a["preset"],
        )
    });
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct HomeAssistantArgs {
    #[arg(long, conflicts_with = "off")]
    pub on: bool,
    #[arg(long)]
    pub off: bool,
    /// Home Assistant's own broker, mqtt://host[:port]; "server" (or empty): the server's
    /// own broker, the one the roasters use
    #[arg(long, value_name = "URL")]
    pub broker: Option<String>,
    /// The user on that broker
    #[arg(long)]
    pub username: Option<String>,
    /// Read that user's password from stdin (never an argument: it would land in history)
    #[arg(long)]
    pub password_stdin: bool,
    /// The discovery prefix (Home Assistant's default: homeassistant)
    #[arg(long)]
    pub prefix: Option<String>,
    /// Announce the simulated roasters too (true or false)
    #[arg(long = "virtual", value_name = "BOOL")]
    pub include_virtual: Option<bool>,
    /// Seconds between state messages, 1 to 60 (default 5)
    #[arg(long, value_name = "S")]
    pub interval: Option<u32>,
}

/// `roast home-assistant`: the discovery's state, or a change to it (admin).
pub async fn home_assistant(api: &Api, out: &Out, a: HomeAssistantArgs) -> Result<u8, Failure> {
    let HomeAssistantArgs {
        on,
        off,
        broker,
        username,
        password_stdin,
        prefix,
        include_virtual,
        interval,
    } = a;
    let change = HaChange {
        enabled: if on {
            Some(true)
        } else if off {
            Some(false)
        } else {
            None
        },
        broker,
        username,
        password_stdin,
        prefix,
        include_virtual,
        interval,
    };
    home_assistant_change(api, out, change).await
}

/// What `roast home-assistant` changes; nothing given: it only reads
pub struct HaChange {
    pub enabled: Option<bool>,
    pub broker: Option<String>,
    pub username: Option<String>,
    pub password_stdin: bool,
    pub prefix: Option<String>,
    pub include_virtual: Option<bool>,
    pub interval: Option<u32>,
}

async fn home_assistant_change(api: &Api, out: &Out, c: HaChange) -> Result<u8, Failure> {
    let mut values = serde_json::Map::new();
    if let Some(on) = c.enabled {
        values.insert("ha_enabled".into(), json!(on.to_string()));
    }
    if let Some(b) = c.broker {
        let b = b.trim();
        let b = if b.eq_ignore_ascii_case("server") {
            ""
        } else {
            b
        };
        values.insert("ha_broker".into(), json!(b));
    }
    if let Some(u) = c.username {
        values.insert("ha_username".into(), json!(u.trim()));
    }
    if c.password_stdin {
        let mut pw = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut pw)
            .map_err(|e| Failure::new("USAGE", format!("could not read stdin: {e}")))?;
        let pw = pw.trim_end_matches(['\r', '\n']);
        if pw.is_empty() {
            return Err(Failure::new(
                "USAGE",
                "--password-stdin read an empty password",
            ));
        }
        values.insert("ha_password".into(), json!(pw));
    }
    if let Some(p) = c.prefix {
        values.insert("ha_prefix".into(), json!(p.trim()));
    }
    if let Some(v) = c.include_virtual {
        values.insert("ha_virtual".into(), json!(v.to_string()));
    }
    if let Some(i) = c.interval {
        values.insert("ha_interval_seconds".into(), json!(i.to_string()));
    }
    let changed = !values.is_empty();
    if changed {
        api.call(
            Method::PUT,
            "/api/settings",
            Some(&json!({ "values": Value::Object(values) })),
        )
        .await?;
        // The server reads the settings every second; give it two
        tokio::time::sleep(Duration::from_millis(2200)).await;
    }
    let mut st = api.get("/api/home-assistant").await?;
    st["changed"] = json!(changed);
    out.emit(&st, |s| {
        let mut lines = Vec::new();
        let on = s["enabled"] == json!(true);
        let broker = match s["broker"].as_str() {
            Some("server") | None => "the server's own broker".to_string(),
            Some(b) => b.to_string(),
        };
        lines.push(format!(
            "Home Assistant discovery: {}{}",
            if on { "on" } else { "off" },
            if on {
                format!(
                    ", {broker}, {}, prefix {}, every {} s",
                    if s["connected"] == json!(true) { "connected" } else { "not connected" },
                    s["prefix"].as_str().unwrap_or("homeassistant"),
                    s["interval_seconds"]
                )
            } else {
                String::new()
            }
        ));
        if let Some(u) = s["built_in_user"].as_str() {
            lines.push(format!(
                "Home Assistant signs in to the built-in broker as {u} (MQTT_HOME_ASSISTANT_PASSWORD)"
            ));
        }
        if s["built_in_without_user"] == json!(true) {
            lines.push(
                "The built-in broker has no Home Assistant user: set MQTT_HOME_ASSISTANT_PASSWORD"
                    .into(),
            );
        }
        for r in s["roasters"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  {} ({}): {}; {}",
                r["name"].as_str().unwrap_or(""),
                r["device_id"].as_str().unwrap_or(""),
                if r["available"] == json!(true) { "available" } else { "unavailable" },
                r["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let waiting: Vec<&str> = s["waiting"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !waiting.is_empty() {
            lines.push(format!(
                "Not announced until set up (roast roaster add): {}",
                waiting.join(", ")
            ));
        }
        if let Some(p) = s["problem"].as_str() {
            lines.push(format!("Problem: {p}"));
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

pub async fn settings(api: &Api, out: &Out, key: Option<String>) -> Result<u8, Failure> {
    match key {
        Some(k) => {
            let mut one = api.get(&format!("/api/settings/{}", enc(&k))).await?;
            one["value"] = value_of(&one["value"]);
            out.emit(&one, |s| match &s["value"] {
                // Known to the server but never set, or cleared
                _ if s["is_set"] != json!(true) => format!("{k}: unset"),
                Value::String(_) if s["secret"] == json!(true) => format!("{k}: (set, secret)"),
                Value::String(v) => format!("{k}: {v}"),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            });
        }
        None => {
            let all = api.get("/api/settings").await?;
            out.emit(&all, |all| {
                all.as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| {
                        let v = v.as_str().map_or(v.to_string(), str::to_string);
                        let v: String = v.chars().take(80).collect();
                        format!("{k:<32} {v}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
    }
    Ok(code::OK)
}

/// `key=value` pairs; a value `-` is read from stdin (for a JSON value such as the alarms).
pub async fn settings_set(api: &Api, out: &Out, pairs: Vec<String>) -> Result<u8, Failure> {
    let mut values = serde_json::Map::new();
    for p in &pairs {
        let (k, v) = p.split_once('=').ok_or_else(|| {
            Failure::new(
                "USAGE",
                format!("'{p}': use KEY=VALUE, e.g. dose_espresso_g=18"),
            )
        })?;
        let v = if v == "-" {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .map_err(|e| Failure::new("USAGE", e.to_string()))?;
            text.trim_end().to_string()
        } else {
            v.to_string()
        };
        values.insert(k.trim().to_string(), Value::String(v));
    }
    if values.len() == 1 {
        let (k, v) = values.iter().next().expect("one value: checked just above");
        api.call(
            Method::PUT,
            &format!("/api/settings/{}", enc(k)),
            Some(&json!({ "value": v })),
        )
        .await?;
    } else {
        api.call(
            Method::PUT,
            "/api/settings",
            Some(&json!({ "values": values })),
        )
        .await?;
    }
    let keys: Vec<String> = values.keys().cloned().collect();
    out.emit(&json!({ "set": keys }), |_| {
        format!("set {}", keys.join(", "))
    });
    Ok(code::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modules_change_from_the_current_or_a_preset() {
        let s = |v: &[&str]| {
            v.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        };
        let all = s(&MODULES);
        // An agent's ask: costs, labels and agents off, the rest kept
        assert_eq!(
            modules_after(&all, None, &[], &s(&["costs", "labels", "agents"])).unwrap(),
            ["reports", "cupping", "tuning"]
        );
        assert_eq!(
            modules_after(&all, Some("home"), &s(&["cupping"]), &[]).unwrap(),
            ["cupping"]
        );
        assert_eq!(
            modules_after(&[], Some("enthusiast"), &[], &s(&["agents"])).unwrap(),
            ["reports", "cupping", "tuning"]
        );
        assert!(modules_after(&all, None, &s(&["stats"]), &[]).is_err());
        assert!(modules_after(&all, None, &s(&["costs"]), &s(&["costs"])).is_err());
        assert_eq!(
            modules_line(&[json!("reports")], &json!("custom")),
            "modules: reports (custom); off: cupping, tuning, agents, costs, labels"
        );
        assert_eq!(
            modules_line(&[], &json!("home")),
            "modules: none (home); off: reports, cupping, tuning, agents, costs, labels"
        );
    }
}
