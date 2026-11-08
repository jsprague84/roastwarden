//! A support bundle (docs/support.md): one JSON file an owner can attach to an issue.
//! What runs (versions, the environment's names, the broker, Home Assistant), the roasters
//! (connections, setups, the device contract, the newest reading and raw messages), the newest
//! roast with its events and telemetry, settings, problems and the recent log lines.
//!
//! Every secret is removed, three ways over: the parts are read through the API's own read
//! routes (which already leave secrets out), then every field named like a secret is masked
//! (audit.rs's rule: password, token, secret, key…), then every secret value the server knows
//! (the owner key, MQTT passwords, connection passwords, secret settings, `rr_` tokens) is cut
//! out of the text wherever it appears, the log lines included. Left out entirely: green
//! coffee, purchases, orders and their receipts (an address, card digits), tokens, the audit
//! log, label designs and printers.

use axum::body::{to_bytes, Body};
use axum::http::Request;
use axum::Router;
use serde_json::{json, Map, Value};
use tower::ServiceExt;

use crate::app::state::AppState;

/// The bundle's own format, bumped when its shape changes
pub const FORMAT: u32 = 1;
const MAX_BODY: usize = 32 << 20;
/// What replaces a secret
const MASK: &str = "***";

/// Environment variables worth knowing, by prefix; a secret one shows only that it is set
const ENV_PREFIXES: &[&str] = &["ROASTWARDEN_", "MQTT_", "RUST_LOG", "TZ"];

fn env_is_secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["PASSWORD", "KEY", "TOKEN", "SECRET"]
        .iter()
        .any(|s| n.contains(s))
}

/// The environment as the bundle shows it: names and plain values; secrets as "(set)";
/// `MQTT_DEVICES` as its device ids only.
pub fn environment(vars: impl Iterator<Item = (String, String)>) -> Map<String, Value> {
    let mut out = Map::new();
    let mut vars: Vec<(String, String)> = vars
        .filter(|(k, _)| ENV_PREFIXES.iter().any(|p| k.starts_with(p)))
        .collect();
    vars.sort();
    for (k, v) in vars {
        let shown = if k == "MQTT_DEVICES" {
            let ids: Vec<&str> = v
                .split(',')
                .filter_map(|p| p.split_once(':').map(|(id, _)| id.trim()))
                .filter(|id| !id.is_empty())
                .collect();
            json!({ "device_ids": ids })
        } else if env_is_secret(&k) {
            json!(if v.is_empty() { "(empty)" } else { "(set)" })
        } else {
            json!(v)
        };
        out.insert(k, shown);
    }
    out
}

/// Settings without anything that points at the owner's accounts: secret keys are already
/// left out by the API; URLs, notification topics and rooms show only that they are set.
pub fn settings(mut s: Map<String, Value>) -> Map<String, Value> {
    for (k, v) in &mut s {
        let text = v.as_str().unwrap_or_default();
        let plain = matches!(text, "true" | "false") || text.trim().parse::<f64>().is_ok();
        let personal = text.starts_with("http://")
            || text.starts_with("https://")
            || (k.starts_with("notify_") && !plain)
            || k == "public_url";
        if personal && !text.is_empty() {
            *v = json!("(set)");
        }
    }
    s
}

/// Every field named like a secret, masked (the audit log's rule).
fn mask(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if crate::auth::audit::is_secret_key(k) && !val.is_null() && *val != json!("") {
                    *val = json!(MASK);
                } else {
                    mask(val);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask),
        _ => {}
    }
}

/// Every known secret cut out of the text, and anything shaped like an API token.
pub fn scrub(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    let mut secrets: Vec<&String> = secrets.iter().filter(|s| s.len() >= 4).collect();
    // The longest first, so a secret containing another goes whole
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for s in secrets {
        out = out.replace(s.as_str(), MASK);
        // As JSON escapes it inside a string
        let escaped = serde_json::to_string(s).unwrap_or_default();
        let escaped = escaped.trim_matches('"');
        if escaped != s.as_str() && !escaped.is_empty() {
            out = out.replace(escaped, MASK);
        }
    }
    scrub_tokens(&out)
}

/// `rr_` and 20 or more letters and digits: a token (tokens.rs)
fn scrub_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(crate::auth::tokens::TOKEN_PREFIX) {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let body = &from[crate::auth::tokens::TOKEN_PREFIX.len()..];
        let n = body.chars().take_while(char::is_ascii_alphanumeric).count();
        let starts_word = before
            .chars()
            .last()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if n >= 20 && starts_word {
            out.push_str(crate::auth::tokens::TOKEN_PREFIX);
            out.push_str(MASK);
            rest = &body[n..];
        } else {
            out.push_str(crate::auth::tokens::TOKEN_PREFIX);
            rest = body;
        }
    }
    out.push_str(rest);
    out
}

/// One read route, as the owner, without the auth layer (the caller is the owner already).
async fn read(router: &Router, path: &str) -> Value {
    let req = match Request::get(path).body(Body::empty()) {
        Ok(r) => r,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let resp = match router.clone().oneshot(req).await {
        Ok(r) => r,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), MAX_BODY)
        .await
        .unwrap_or_default();
    let body: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes).into_owned()));
    if status.is_success() {
        body
    } else {
        json!({ "error": status.as_u16(), "body": body })
    }
}

/// The secrets the server knows, to cut out of the text.
async fn known_secrets(state: &AppState) -> Vec<String> {
    let mut s: Vec<String> = Vec::new();
    if let Some(k) = state.auth.owner_key() {
        s.push(k.to_string());
    }
    for (name, value) in std::env::vars() {
        if name == "MQTT_DEVICES" {
            s.extend(
                crate::ops::broker::parse_devices(&value)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(_, pw)| pw),
            );
        } else if env_is_secret(&name) && !value.is_empty() {
            s.push(value);
        }
    }
    // Secret settings (notification tokens, the webhook URL, Home Assistant's password)
    if let Ok(rows) = sqlx::query_as::<_, (String, String)>("SELECT key, value FROM settings")
        .fetch_all(&state.db)
        .await
    {
        s.extend(
            rows.into_iter()
                .filter(|(k, v)| crate::auth::settings::is_secret(k) && !v.is_empty())
                .map(|(_, v)| v),
        );
    }
    // A connection's password (a Phidget22 server's, a device's)
    if let Ok(configs) = sqlx::query_scalar::<_, String>("SELECT config FROM device_connections")
        .fetch_all(&state.db)
        .await
    {
        for c in configs {
            if let Ok(v) = serde_json::from_str::<Value>(&c) {
                collect_secret_values(&v, &mut s);
            }
        }
    }
    s.retain(|v| !v.trim().is_empty());
    s.sort();
    s.dedup();
    s
}

fn collect_secret_values(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                match val {
                    Value::String(s) if crate::auth::audit::is_secret_key(k) => out.push(s.clone()),
                    _ => collect_secret_values(val, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_secret_values(x, out)),
        _ => {}
    }
}

/// The bundle, as text ready to save.
pub async fn bundle(state: &AppState) -> String {
    let router = crate::app::build::api_router().with_state(state.clone());
    let enc = |s: &str| urlencoding(s);

    let mut roasters = Vec::new();
    let devices = read(&router, "/api/devices").await;
    for d in devices.as_array().into_iter().flatten() {
        let (Some(id), Some(dev)) = (d["id"].as_str(), d["device_id"].as_str()) else {
            continue;
        };
        let base = format!("/api/roaster/{}", enc(dev));
        roasters.push(json!({
            "device": read(&router, &format!("/api/devices/{}", enc(id))).await,
            "setup": read(&router, &format!("{base}/setup")).await,
            "contract": read(&router, &format!("{base}/contract")).await,
            "transports": read(&router, &format!("{base}/transports")).await,
            "sequence": read(&router, &format!("{base}/sequence")).await,
            "latest": read(&router, &format!("{base}/telemetry/latest")).await,
            "raw": read(&router, &format!("{base}/raw")).await,
        }));
    }

    let newest = read(&router, "/api/sessions?limit=1").await;
    let roast = match newest.as_array().and_then(|a| a.first()) {
        Some(s) => {
            let id = enc(s["id"].as_str().unwrap_or_default());
            json!({
                "session": read(&router, &format!("/api/sessions/{id}")).await,
                "events": read(&router, &format!("/api/sessions/{id}/events")).await,
                "telemetry": read(&router, &format!("/api/sessions/{id}/telemetry")).await,
            })
        }
        None => Value::Null,
    };

    let notifications = read(&router, "/api/notifications").await;
    // Each channel's on/off and last send, not where it points
    let channels: Map<String, Value> = notifications["channels"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, c)| {
                    (
                        k.clone(),
                        json!({ "enabled": c["enabled"], "status": c["status"] }),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let mut b = json!({
        "bundle": {
            "format": FORMAT,
            "made_at": chrono::Utc::now(),
            "note": "Made by Roastwarden for a bug report: secrets removed (docs/support.md). \
                     Read it before you share it: roast names, notes and roaster names are in it.",
            "left_out": ["green coffee, purchases, orders and receipts", "tokens and the audit log",
                         "label designs, printers and share links", "every password, key and token"],
        },
        "about": read(&router, "/api/about").await,
        "system": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        },
        "environment": environment(std::env::vars()),
        "busy": read(&router, "/api/busy").await,
        "roasters": roasters,
        "discovered": read(&router, "/api/devices/discovered").await,
        "rings": read(&router, "/api/telemetry/rings").await,
        "newest_roast": roast,
        "settings": settings(read(&router, "/api/settings").await.as_object().cloned().unwrap_or_default()),
        "notifications": { "channels": channels, "events": notifications["events"], "problems": notifications["problems"] },
        "home_assistant": read(&router, "/api/home-assistant").await,
        "backups": read(&router, "/api/backups").await,
        "log": crate::ops::logbuf::lines(),
    });
    mask(&mut b);
    let text = serde_json::to_string_pretty(&b).unwrap_or_default();
    scrub(&text, &known_secrets(state).await)
}

/// A path segment, percent-encoded
fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_shows_names_not_secrets() {
        let vars = [
            ("ROASTWARDEN_API_KEY", "owner-key-123456789"),
            ("ROASTWARDEN_HTTP_ADDR", "0.0.0.0:8080"),
            ("MQTT_PASSWORD", "mqtt-pw"),
            ("MQTT_DEVICES", "esp32_roaster_01:dev-pw-1, drum:dev-pw-2"),
            ("MQTT_HOME_ASSISTANT_PASSWORD", ""),
            ("HOME", "/root"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let e = environment(vars.into_iter());
        assert_eq!(e["ROASTWARDEN_API_KEY"], "(set)");
        assert_eq!(e["ROASTWARDEN_HTTP_ADDR"], "0.0.0.0:8080");
        assert_eq!(e["MQTT_PASSWORD"], "(set)");
        assert_eq!(e["MQTT_HOME_ASSISTANT_PASSWORD"], "(empty)");
        assert_eq!(
            e["MQTT_DEVICES"],
            json!({"device_ids": ["esp32_roaster_01", "drum"]})
        );
        assert!(!e.contains_key("HOME"));
        let text = Value::Object(e).to_string();
        assert!(
            !text.contains("pw") && !text.contains("owner-key"),
            "{text}"
        );
    }

    #[test]
    fn settings_keep_numbers_and_flags_not_where_things_point() {
        let s = json!({
            "notify_ntfy_topic": "my-private-topic",
            "notify_ntfy_enabled": "true",
            "notify_offline_minutes": "3",
            "public_url": "https://roast.example",
            "ha_broker": "mqtt://192.168.1.2:1883",
            "currency": "USD",
        });
        let out = settings(s.as_object().unwrap().clone());
        assert_eq!(out["notify_ntfy_topic"], "(set)");
        assert_eq!(out["notify_ntfy_enabled"], "true");
        assert_eq!(out["notify_offline_minutes"], "3");
        assert_eq!(out["public_url"], "(set)");
        assert_eq!(out["ha_broker"], "mqtt://192.168.1.2:1883");
        assert_eq!(out["currency"], "USD");
    }

    #[test]
    fn known_secrets_and_tokens_are_cut_out_everywhere() {
        let text = r#"{"log":["signed in with rr_AbCdEfGhIjKlMnOpQrStUvWxYz0123 ok", "pass p\"w-long here"],
            "x":"the key owner-key-123456789", "short":"rr_abc", "carrot_rr_ZyXwVuTsRqPoNmLkJiHgFe": 1}"#;
        let out = scrub(
            text,
            &[
                "owner-key-123456789".into(),
                "p\"w-long".into(),
                "ab".into(),
            ],
        );
        assert!(!out.contains("owner-key-123456789"), "{out}");
        assert!(!out.contains("AbCdEfGhIj"), "{out}");
        assert!(out.contains("rr_*** ok"), "{out}");
        assert!(
            !out.contains(r#"p\"w-long"#),
            "the JSON-escaped form too: {out}"
        );
        assert!(out.contains("rr_abc"), "too short for a token: {out}");
        assert!(out.contains("carrot_rr_"), "not a token's start: {out}");
        assert!(
            out.contains("\"short\""),
            "a secret under 4 characters is not hunted: {out}"
        );
    }

    #[test]
    fn fields_named_like_secrets_are_masked() {
        let mut v = json!({"config": {"host": "10.0.0.2", "password": "hunter2", "auth_token": "t"},
                           "tokens_may_control": false, "list": [{"api_key": "k"}]});
        mask(&mut v);
        assert_eq!(v["config"]["password"], MASK);
        assert_eq!(v["config"]["auth_token"], MASK);
        assert_eq!(v["config"]["host"], "10.0.0.2");
        assert_eq!(v["tokens_may_control"], false);
        assert_eq!(v["list"][0]["api_key"], MASK);
    }

    /// The whole server: a bundle with a roaster, a roast, secrets set everywhere; none of
    /// them in it, and no receipt.
    mod server {
        use super::super::*;
        use crate::test_support::{Harness, OWNER_KEY};

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn a_bundle_has_the_roaster_and_roast_and_no_secret() {
            let h = Harness::new().await;
            let r = h
                .owner(
                    "POST",
                    "/api/devices",
                    Some(json!({"name": "Bench", "device_id": "bench_01"})),
                )
                .await;
            let id = r.json()["id"].as_str().unwrap().to_string();
            let r = h
                .owner(
                    "POST",
                    &format!("/api/devices/{id}/connections"),
                    Some(json!({"protocol": "phidget22", "enabled": false, "config":
                        {"host": "127.0.0.1", "port": 5661, "password": "phidget-secret-pw"}})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let r = h
                .owner(
                    "PUT",
                    "/api/settings",
                    Some(
                        json!({"values": {"notify_gotify_token": "gotify-secret-tok",
                        "notify_ntfy_topic": "secret-topic-name", "ha_password": "ha-secret-pw",
                        "public_url": "https://private.example"}}),
                    ),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let s = h
                .owner(
                    "POST",
                    "/api/sessions",
                    Some(json!({"name": "Bundle roast", "device_id": "virtual_roaster"})),
                )
                .await;
            assert!(s.status.is_success(), "{:?}", s.json());
            tracing::info!("a log line with {OWNER_KEY} in it");

            let r = h.owner("GET", "/api/support-bundle", None).await;
            assert!(r.status.is_success());
            assert!(r
                .content_type
                .as_deref()
                .is_some_and(|c| c.starts_with("application/json")));
            let text = match &r.body {
                crate::test_support::ReplyBody::Json(v) => v.to_string(),
                _ => panic!("a JSON body"),
            };
            for secret in [
                OWNER_KEY,
                "phidget-secret-pw",
                "gotify-secret-tok",
                "secret-topic-name",
                "ha-secret-pw",
                "private.example",
            ] {
                assert!(!text.contains(secret), "{secret} is in the bundle");
            }
            let b = r.json();
            assert_eq!(b["bundle"]["format"], FORMAT);
            assert_eq!(b["about"]["version"], env!("CARGO_PKG_VERSION"));
            assert!(b["roasters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["device"]["device_id"] == "bench_01"));
            assert_eq!(b["newest_roast"]["session"]["name"], "Bundle roast");
            assert!(b["log"].is_array());
            // Only the note says what is left out
            let mut rest = b.clone();
            rest.as_object_mut().unwrap().remove("bundle");
            let rest = rest.to_string();
            for word in ["receipt", "\"orders\"", "\"purchases\"", "\"beans\""] {
                let at = rest.find(word);
                assert!(
                    at.is_none(),
                    "{word}: …{}…",
                    &rest[at.unwrap().saturating_sub(200)..at.unwrap() + 50]
                );
            }

            // Not for a read-only token
            let t = h.token("read_only").await;
            let r = crate::test_support::call(&h.app, "GET", "/api/support-bundle", Some(&t), None)
                .await;
            assert_eq!(r.status.as_u16(), 403);
        }
    }
}
