//! Is Roastwarden up and what is it doing: `roast status`, `busy` and `artisan-screen`.

use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{enc, Api};
use crate::commands::{records, settings};
use crate::out::{code, Failure, Out};
use crate::text::{duration, grams};

/// `GET /api/busy`, once or until idle. Exit 1 while a real roaster is busy.
pub async fn busy(api: &Api, out: &Out, wait_min: Option<u64>) -> Result<u8, Failure> {
    let deadline = wait_min
        .map(|m| std::time::Instant::now() + std::time::Duration::from_secs(m.saturating_mul(60)));
    let answer = loop {
        let b = api.get("/api/busy").await?;
        let busy = b["busy"] == json!(true);
        match deadline {
            Some(d) if busy && std::time::Instant::now() < d => {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
            _ => break b,
        }
    };
    let busy = answer["busy"] == json!(true);
    out.emit(&answer, |a| {
        let mut lines = vec![if busy {
            "busy: wait before a deploy, restart or flash".to_string()
        } else {
            "idle: nothing a restart would cut".to_string()
        }];
        for r in a["roasters"].as_array().into_iter().flatten() {
            let reasons: Vec<&str> = r["reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str())
                .collect();
            lines.push(format!(
                "{}{}: {}{}",
                r["device_id"].as_str().unwrap_or("?"),
                if r["virtual"] == json!(true) {
                    " (virtual, does not count)"
                } else {
                    ""
                },
                reasons.join(", "),
                r["bt_c"]
                    .as_f64()
                    .map(|t| format!(", BT {t:.0} °C"))
                    .unwrap_or_default()
            ));
        }
        lines.join("\n")
    });
    Ok(if busy { code::FAILED } else { code::OK })
}

/// One line for the Modbus slave (`GET /api/about`'s `artisan_screen`).
pub fn screen_line(s: &Value) -> String {
    if s.is_null() {
        return "Artisan screen: off (ROASTWARDEN_MODBUS_ADDR is not set)".into();
    }
    let serving = match (s["serving"].as_str(), s["problem"].as_str()) {
        (Some(id), _) => format!("serving {id}"),
        (None, Some(p)) => format!("serving nothing: {p}"),
        _ => "serving nothing".into(),
    };
    let read = match (s["last_read_at"].as_str(), s["last_client"].as_str()) {
        (Some(at), Some(c)) => format!("last read {} by {c}", at.get(..19).unwrap_or(at)),
        _ => "not read yet".into(),
    };
    format!(
        "Artisan screen: Modbus TCP {}, {serving}, {}, {read}",
        s["addr"].as_str().unwrap_or("?"),
        if s["writes"] == json!(true) {
            "writes on"
        } else {
            "read-only"
        }
    )
}

#[derive(Args, Debug)]
pub struct ArtisanScreenArgs {
    /// Save the Artisan setup (.aset) here (`-` for stdout)
    #[arg(long, value_name = "FILE")]
    pub aset: Option<PathBuf>,
    /// The server's address as Artisan reaches it (default: the host in ROASTWARDEN_URL)
    #[arg(long)]
    pub host: Option<String>,
    /// Artisan in °F (loading the file sets Artisan's unit; default °C)
    #[arg(long)]
    pub fahrenheit: bool,
}

/// `roast artisan-screen`: the Modbus slave Artisan reads, or the setup file for it.
pub async fn artisan_screen(api: &Api, out: &Out, a: ArtisanScreenArgs) -> Result<u8, Failure> {
    let ArtisanScreenArgs {
        aset,
        host,
        fahrenheit,
    } = a;
    if let Some(file) = aset {
        // No host: the server puts in the one this call was sent to
        let mut query = vec![];
        if let Some(h) = host {
            query.push(format!("host={}", enc(&h)));
        }
        if fahrenheit {
            query.push("unit=F".to_string());
        }
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        };
        return records::save_file(
            api,
            out,
            &format!("/api/artisan/screen.aset{query}"),
            Some(file),
            "Roastwarden.aset",
        )
        .await;
    }
    let about = api.get("/api/about").await?;
    let answer = about["artisan_screen"].clone();
    out.emit(&answer, screen_line);
    Ok(if answer.is_null() {
        code::FAILED
    } else {
        code::OK
    })
}

pub async fn status(api: &Api, out: &Out) -> Result<u8, Failure> {
    let about = api.get("/api/about").await?;
    let registry = api
        .get("/api/devices/registry")
        .await
        .unwrap_or(Value::Null);
    let now = chrono::Utc::now().timestamp();
    // Heard from: the newest telemetry, else the status message (firmware sends it retained,
    // on connect only)
    let mut roasters = Vec::new();
    for d in registry["devices"].as_array().into_iter().flatten() {
        let id = d["device_id"].as_str().unwrap_or_default();
        let latest = api
            .get(&format!("/api/roaster/{}/telemetry/latest", enc(id)))
            .await
            .unwrap_or(Value::Null);
        let heard = latest["timestamp"]
            .as_i64()
            .unwrap_or(0)
            .max(d["last_seen"].as_i64().unwrap_or(0));
        roasters.push(json!({
            "id": id,
            "online": now - heard < 60,
            "last_heard_s_ago": now - heard,
            "firmware": d["version"],
        }));
    }
    let plan = api.get("/api/plan").await.unwrap_or(Value::Null);
    let beans = api.get("/api/beans").await.unwrap_or(Value::Null);
    // What this credential is: which token and preset, so an agent knows what it may do
    let auth = api.get("/api/auth/status").await.unwrap_or(Value::Null);
    let low: Vec<Value> = beans
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["low"] == json!(true))
        .map(|b| json!({ "name": b["name"], "stock_g": b["stock_g"] }))
        .collect();
    let answer = json!({
        "version": about["version"],
        "revision": about["revision"],
        "build": about["build"],
        "uptime_seconds": about["uptime_seconds"],
        "mqtt_ready": about["mqtt_ready"],
        "db_ready": about["db_ready"],
        "problems": about["problems"],
        "last_backup": about["last_backup"],
        "busy": about["busy"],
        "roasters": roasters,
        "planned_roasts": plan.as_array().map(Vec::len),
        "low_beans": low,
        "credential": auth["credential"],
        "token": auth["token"],
        // What the dashboard shows (docs/lean-home.md): an agent does not offer what is off
        "modules": about["modules"],
        "preset": about["preset"],
        "first_run": about["first_run"],
        // AGPL-3.0-or-later, and the source of this revision (null: the build's repository
        // is not known); notices at /licenses/ on the server
        "licence": about["licence"],
        "source_url": about["source_url"],
        // The Modbus slave Artisan reads (docs/artisan-screen.md); null: off
        "artisan_screen": about["artisan_screen"],
        // The MQTT broker the server runs itself (docs/DEPLOY.md, "One container"); null: an
        // outside broker
        "mqtt_broker": about["mqtt_broker"],
    });
    out.emit(&answer, |a| {
        // "0.4.0 + 66 commits": the version moves only at a release
        let version = match a["build"]["commits_since_release"].as_u64() {
            Some(n) if n > 0 => format!(
                "{} + {n} commit{}",
                a["version"].as_str().unwrap_or("?"),
                if n == 1 { "" } else { "s" }
            ),
            _ => a["version"].as_str().unwrap_or("?").to_string(),
        };
        let mut lines = vec![format!(
            "Roastwarden {} ({}), up {}, MQTT {}, database {}",
            version,
            a["revision"]
                .as_str()
                .map_or("?", |r| r.get(..8).unwrap_or(r)),
            duration(a["uptime_seconds"].as_i64().unwrap_or(0)),
            if a["mqtt_ready"] == json!(true) {
                "ready"
            } else {
                "NOT READY"
            },
            if a["db_ready"] == json!(true) {
                "ready"
            } else {
                "NOT READY"
            },
        )];
        for p in a["problems"].as_array().into_iter().flatten() {
            lines.push(format!("problem: {}", p["title"].as_str().unwrap_or("?")));
        }
        if a["busy"]["busy"] == json!(true) {
            lines.push(
                "busy: a roaster is roasting, driving, tuning or cooling (`roast busy`)".into(),
            );
        }
        if let Some(m) = a["modules"].as_array() {
            lines.push(settings::modules_line(m, &a["preset"]));
        }
        if let Some(t) = a["token"]["name"].as_str() {
            lines.push(format!(
                "token {t} ({})",
                a["token"]["preset"].as_str().unwrap_or("?")
            ));
        } else if let Some(c) = a["credential"].as_str() {
            lines.push(format!("credential: {c}"));
        }
        for r in a["roasters"].as_array().into_iter().flatten() {
            lines.push(format!(
                "roaster {}: {}",
                r["id"].as_str().unwrap_or("?"),
                if r["online"] == json!(true) {
                    "online".to_string()
                } else {
                    format!(
                        "offline (last heard {} ago)",
                        duration(r["last_heard_s_ago"].as_i64().unwrap_or(0))
                    )
                }
            ));
        }
        if !a["artisan_screen"].is_null() {
            lines.push(screen_line(&a["artisan_screen"]));
        }
        if let Some(addr) = a["mqtt_broker"]["addr"].as_str() {
            let devices: Vec<&str> = a["mqtt_broker"]["devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d.as_str())
                .collect();
            lines.push(format!(
                "MQTT broker: built in, on {addr}; roasters that may sign in: {}",
                if devices.is_empty() {
                    "none (MQTT_DEVICES)".to_string()
                } else {
                    devices.join(", ")
                }
            ));
        }
        if let Some(n) = a["planned_roasts"].as_u64() {
            lines.push(format!("{n} planned roasts"));
        }
        for b in a["low_beans"].as_array().into_iter().flatten() {
            lines.push(format!(
                "low: {} ({})",
                b["name"].as_str().unwrap_or("?"),
                grams(b["stock_g"].as_f64().unwrap_or(0.0))
            ));
        }
        let lb = &a["last_backup"];
        lines.push(if lb.is_null() {
            "no backup yet".into()
        } else if let Some(e) = lb["error"].as_str() {
            format!("last backup FAILED: {e}")
        } else {
            format!(
                "last backup {}",
                lb["started_at"]
                    .as_str()
                    .unwrap_or("?")
                    .get(..16)
                    .unwrap_or("?")
            )
        });
        if let Some(l) = a["licence"].as_str() {
            lines.push(match a["source_url"].as_str() {
                Some(src) => format!("licence {l}, source {src}"),
                None => format!("licence {l}"),
            });
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

/// `roast health`: is the server up and, with --ready, are MQTT and the database.
pub async fn health(api: &Api, out: &Out, ready: bool) -> Result<u8, Failure> {
    let up = api
        .send(Method::GET, "/healthz", None, Duration::from_secs(5))
        .await?;
    let version = api
        .send(Method::GET, "/version", None, Duration::from_secs(5))
        .await
        .map_or(Value::Null, |r| r.body);
    let mut answer = json!({
        "status": up.body.as_str().unwrap_or("ok"),
        "version": version["version"],
        "revision": version["revision"],
    });
    if ready {
        let r = api
            .send(Method::GET, "/readyz", None, Duration::from_secs(5))
            .await;
        if let Err(f) = r {
            if f.status != Some(503) {
                return Err(f);
            }
            return Err(Failure::new(
                "NOT_READY",
                "Roastwarden runs, but MQTT or the database is not ready",
            ));
        }
        answer["ready"] = json!(true);
    }
    out.emit(&answer, |a| {
        format!(
            "Roastwarden {} ({}){}",
            a["version"].as_str().unwrap_or("?"),
            a["status"].as_str().unwrap_or("?"),
            if ready { ", ready" } else { "" }
        )
    });
    Ok(code::OK)
}
