//! Home Assistant discovery (docs/home-assistant.md): each roaster as a Home Assistant
//! device over MQTT discovery, read-only. Off unless `ha_enabled` is switched on.
//!
//! One retained discovery message per roaster on `<prefix>/device/roastwarden_<id>/config`
//! (Home Assistant 2024.11's device discovery: one message, every entity), its state as JSON on
//! `roastwarden/ha/<id>/state` every few seconds, and its availability on
//! `roastwarden/ha/<id>/availability` (retained). Home Assistant shows a roaster available while
//! it reports and the server is up (`roastwarden/server/status` on the server's own broker, its
//! last will; `roastwarden/ha/status` on a separate one).
//!
//! Nothing comes back: no entity sends a command, and the loop subscribes to nothing. Control
//! stays with the dashboard, the CLI and the control gate (control.md).
//!
//! Where it publishes: through the server's own MQTT client (the broker the roasters use,
//! built-in or not) unless `ha_broker` names another (`mqtt://host:port`, Home Assistant's
//! own), with `ha_username` and `ha_password`. With the built-in broker Home Assistant signs in
//! as `homeassistant` with `MQTT_HOME_ASSISTANT_PASSWORD` and may read only these topics
//! (broker.rs).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use roastwarden_core::Sample;
use roastwarden_mqtt::{MqttConfig, MqttService, StatusAnnouncement};
use rumqttc::QoS;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use utoipa::ToSchema;

use crate::app::state::AppState;
use crate::roasts::types::{RoastEvent, RoastEventType, RoastSession};

pub const ENABLED: &str = "ha_enabled";
pub const PREFIX: &str = "ha_prefix";
pub const BROKER: &str = "ha_broker";
pub const USERNAME: &str = "ha_username";
pub const PASSWORD: &str = "ha_password";
pub const VIRTUAL: &str = "ha_virtual";
pub const INTERVAL: &str = "ha_interval_seconds";

/// Home Assistant's default discovery prefix
pub const DEFAULT_PREFIX: &str = "homeassistant";
pub const DEFAULT_INTERVAL_S: u64 = 5;
pub const INTERVAL_RANGE: (i64, i64) = (1, 60);
/// Home Assistant's user on the built-in broker (broker.rs, docker/mosquitto/acl.in)
pub const HA_USER: &str = "homeassistant";
/// Where the state and availability go, under the roaster's id
pub const ROOT: &str = "roastwarden/ha";
/// The bridge's own presence on a separate broker (its last will)
pub const BRIDGE_STATUS_TOPIC: &str = "roastwarden/ha/status";
/// A roaster silent this long is unavailable
const FRESH_MS: u64 = 30_000;
/// Discovery and availability are sent again this often (a broker without persistence
/// forgets retained messages when it restarts)
const REFRESH: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// A separate broker: `mqtt://host[:port]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broker {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub prefix: String,
    /// None: the server's own broker
    pub broker: Option<Broker>,
    /// The simulated roasters too
    pub include_virtual: bool,
    pub interval_s: u64,
}

/// `mqtt://host[:port]` → (host, port). Only plain MQTT: Home Assistant's broker is on the LAN.
pub fn parse_broker(url: &str) -> Result<(String, u16), String> {
    let v = url.trim();
    let rest = v
        .strip_prefix("mqtt://")
        .ok_or("ha_broker is mqtt://host or mqtt://host:port (mqtts is not offered)")?;
    let rest = rest.trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
            let port = p
                .parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .ok_or("ha_broker's port is a number from 1 to 65535")?;
            (h, port)
        }
        _ => (rest, 1883),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host.contains(['/', '@', ' ']) {
        return Err("ha_broker is mqtt://host or mqtt://host:port".into());
    }
    Ok((host.to_string(), port))
}

/// A discovery prefix: topic levels without wildcards
pub fn check_prefix(p: &str) -> Result<(), String> {
    let ok = !p.is_empty()
        && p.len() <= 64
        && !p.starts_with('/')
        && !p.ends_with('/')
        && !p.contains("//")
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/'));
    if ok {
        Ok(())
    } else {
        Err(
            "ha_prefix is a topic such as homeassistant: letters, digits, _ - and /, no wildcards"
                .into(),
        )
    }
}

impl Config {
    pub fn from_settings(s: &HashMap<String, String>) -> Self {
        let get = |k: &str| s.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());
        let broker = get(BROKER)
            .and_then(|u| parse_broker(u).ok())
            .map(|(host, port)| Broker {
                host,
                port,
                username: get(USERNAME).map(str::to_string),
                password: get(PASSWORD).map(str::to_string),
            });
        Self {
            enabled: get(ENABLED) == Some("true"),
            prefix: get(PREFIX)
                .filter(|p| check_prefix(p).is_ok())
                .unwrap_or(DEFAULT_PREFIX)
                .to_string(),
            broker,
            include_virtual: get(VIRTUAL) == Some("true"),
            interval_s: get(INTERVAL)
                .and_then(|v| v.parse::<i64>().ok())
                .map_or(DEFAULT_INTERVAL_S, |v| {
                    v.clamp(INTERVAL_RANGE.0, INTERVAL_RANGE.1) as u64
                }),
        }
    }
}

// ---------------------------------------------------------------------------
// Topics and payloads (pure)
// ---------------------------------------------------------------------------

/// A device id as Home Assistant's ids want it: letters, digits, _ and -
pub fn object_id(device_id: &str) -> String {
    device_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub fn config_topic(prefix: &str, device_id: &str) -> String {
    format!(
        "{prefix}/device/roastwarden_{}/config",
        object_id(device_id)
    )
}

pub fn state_topic(device_id: &str) -> String {
    format!("{ROOT}/{}/state", object_id(device_id))
}

pub fn availability_topic(device_id: &str) -> String {
    format!("{ROOT}/{}/availability", object_id(device_id))
}

/// Where a roast is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Drying,
    Maillard,
    Development,
    Cooling,
}

pub const PHASES: [&str; 5] = ["idle", "drying", "maillard", "development", "cooling"];
pub const WARNINGS: [&str; 4] = ["none", "crash", "flick", "stall"];

/// The phase from the open roast's events, or cooling after DROP.
pub fn phase(open: Option<&[RoastEvent]>, cooling: bool) -> Phase {
    let Some(events) = open else {
        return if cooling { Phase::Cooling } else { Phase::Idle };
    };
    let has = |t: RoastEventType| events.iter().any(|e| e.event_type == t);
    if has(RoastEventType::Drop) || has(RoastEventType::CoolEnd) {
        Phase::Cooling
    } else if has(RoastEventType::FirstCrackStart) || has(RoastEventType::DevelopmentStart) {
        Phase::Development
    } else if has(RoastEventType::DryingEnd) {
        Phase::Maillard
    } else {
        Phase::Drying
    }
}

/// What a roaster reports, so only its entities are announced
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Channels {
    pub bt: bool,
    pub et: bool,
    pub heater: bool,
    pub fan: bool,
    pub setpoint: bool,
}

impl Channels {
    pub fn of(s: &Sample) -> Self {
        Self {
            bt: s.bt.is_some(),
            et: s.et.is_some(),
            heater: s.heater.is_some(),
            fan: s.fan.is_some(),
            setpoint: s.setpoint.is_some(),
        }
    }

    /// Once seen, a channel stays announced (a sample without it is not a new roaster)
    pub fn union(self, o: Self) -> Self {
        Self {
            bt: self.bt || o.bt,
            et: self.et || o.et,
            heater: self.heater || o.heater,
            fan: self.fan || o.fan,
            setpoint: self.setpoint || o.setpoint,
        }
    }
}

/// The roaster as discovery describes it
#[derive(Debug, Clone, PartialEq)]
pub struct Describe {
    pub device_id: String,
    pub name: String,
    pub firmware: Option<String>,
    pub channels: Channels,
    /// The roaster's page on the dashboard (`public_url`), if known
    pub url: Option<String>,
    /// The topic and payload that say the server (or the bridge) is up
    pub server_status_topic: String,
}

/// The device discovery message.
pub fn discovery(d: &Describe) -> Value {
    let oid = object_id(&d.device_id);
    let state = state_topic(&d.device_id);
    let availability = json!([
        {"topic": availability_topic(&d.device_id)},
        {"topic": d.server_status_topic, "value_template": "{{ value_json.status }}"}
    ]);
    let mut cmps = serde_json::Map::new();
    let mut add = |key: &str, platform: &str, name: &str, extra: Value| {
        let mut c = json!({
            "platform": platform,
            "name": name,
            "unique_id": format!("roastwarden_{oid}_{key}"),
            "state_topic": state,
            "availability": availability,
            "availability_mode": "all",
            "value_template": format!("{{{{ value_json.{key} }}}}"),
        });
        if let (Some(c), Value::Object(e)) = (c.as_object_mut(), extra) {
            c.extend(e);
        }
        cmps.insert(key.to_string(), c);
    };
    let temp = |icon: &str| {
        json!({"device_class": "temperature", "unit_of_measurement": "°C",
               "state_class": "measurement", "suggested_display_precision": 1, "icon": icon})
    };
    let pct = |icon: &str| {
        json!({"unit_of_measurement": "%", "state_class": "measurement",
               "suggested_display_precision": 0, "icon": icon})
    };
    let ch = d.channels;
    if ch.bt {
        add("bt", "sensor", "Bean temperature", temp("mdi:coffee"));
        add(
            "ror",
            "sensor",
            "Rate of rise",
            json!({"unit_of_measurement": "°C/min", "state_class": "measurement",
                   "suggested_display_precision": 1, "icon": "mdi:chart-line-variant"}),
        );
    }
    if ch.et {
        add(
            "et",
            "sensor",
            "Environment temperature",
            temp("mdi:thermometer"),
        );
    }
    if ch.setpoint {
        add("setpoint", "sensor", "Setpoint", temp("mdi:target"));
    }
    if ch.heater {
        add("heater", "sensor", "Heater", pct("mdi:fire"));
    }
    if ch.fan {
        add("fan", "sensor", "Fan", pct("mdi:fan"));
    }
    add(
        "phase",
        "sensor",
        "Phase",
        json!({"device_class": "enum", "options": PHASES, "icon": "mdi:progress-clock"}),
    );
    add(
        "roast_time",
        "sensor",
        "Roast time",
        json!({"device_class": "duration", "unit_of_measurement": "s", "icon": "mdi:timer-outline"}),
    );
    add(
        "roasting",
        "binary_sensor",
        "Roasting",
        json!({"device_class": "running",
               "value_template": "{{ 'ON' if value_json.roasting else 'OFF' }}"}),
    );
    add(
        "roast",
        "sensor",
        "Roast",
        json!({"icon": "mdi:coffee-maker-outline"}),
    );
    if ch.bt {
        add(
            "ror_warning",
            "sensor",
            "RoR warning",
            json!({"device_class": "enum", "options": WARNINGS, "icon": "mdi:alert-outline",
                   "json_attributes_topic": state,
                   "json_attributes_template": "{{ {'reason': value_json.ror_warning_reason} | tojson }}"}),
        );
    }
    let mut device = json!({
        "identifiers": [format!("roastwarden_{oid}")],
        "name": d.name,
        "manufacturer": "Roastwarden",
        "model": "Coffee roaster",
    });
    if let Some(fw) = &d.firmware {
        device["sw_version"] = json!(fw);
    }
    if let Some(url) = &d.url {
        device["configuration_url"] = json!(url);
    }
    json!({
        "device": device,
        "origin": {
            "name": "Roastwarden",
            "sw_version": env!("CARGO_PKG_VERSION"),
            "support_url": "https://roastwarden.com/docs/home-assistant/",
        },
        "components": cmps,
        "qos": 0,
    })
}

/// What the roast is doing, beside the sample
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub phase: Option<Phase>,
    /// Seconds since CHARGE, while a roast is open
    pub roast_time_s: Option<i64>,
    pub roast: Option<String>,
    pub batch: Option<i32>,
    /// The first RoR warning: kind and its reason
    pub warning: Option<(&'static str, String)>,
}

fn round(v: Option<f64>, places: i32) -> Value {
    let m = 10f64.powi(places);
    v.filter(|v| v.is_finite())
        .map_or(Value::Null, |v| json!((v * m).round() / m))
}

/// The state message. A value the roaster did not send is null (Home Assistant: unknown).
pub fn state(s: Option<&Sample>, live: &Live) -> Value {
    let (kind, reason) = match &live.warning {
        Some((k, r)) => (*k, r.as_str()),
        None => ("none", ""),
    };
    json!({
        "bt": round(s.and_then(|s| s.bt), 1),
        "et": round(s.and_then(|s| s.et), 1),
        "ror": round(s.and_then(|s| s.ror_bt), 1),
        "heater": round(s.and_then(|s| s.heater), 0),
        "fan": round(s.and_then(|s| s.fan), 0),
        "setpoint": round(s.and_then(|s| s.setpoint), 1),
        "phase": live.phase.unwrap_or(Phase::Idle),
        "roast_time": live.roast_time_s,
        "roasting": live.roast_time_s.is_some(),
        "roast": live.roast,
        "batch": live.batch,
        "ror_warning": kind,
        "ror_warning_reason": reason,
    })
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// What Settings → Home Assistant and `roast home-assistant` show
#[derive(Debug, Clone, Serialize, ToSchema, Default)]
pub struct Status {
    pub enabled: bool,
    /// "server": the server's own broker; else host:port of Home Assistant's
    pub broker: String,
    pub username: Option<String>,
    pub password_set: bool,
    pub connected: bool,
    pub prefix: String,
    pub include_virtual: bool,
    pub interval_seconds: u64,
    /// With the built-in broker: the user Home Assistant signs in as, when
    /// MQTT_HOME_ASSISTANT_PASSWORD gives it a password
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    pub built_in_user: Option<String>,
    /// The built-in broker runs, but has no Home Assistant user yet
    pub built_in_without_user: bool,
    pub roasters: Vec<HaRoaster>,
    /// Heard, but waiting to be set up (Roasters): not announced until then
    pub waiting: Vec<String>,
    pub last_published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Why nothing is published, if so
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HaRoaster {
    pub device_id: String,
    pub name: String,
    /// Home Assistant shows it available (it reports)
    pub available: bool,
    pub config_topic: String,
    pub state_topic: String,
    /// The entities announced
    pub entities: Vec<String>,
}

struct Link {
    broker: Option<Broker>,
    mqtt: MqttService,
    was_ready: bool,
}

#[derive(Default)]
struct Published {
    config_topic: String,
    payload: String,
    channels: Channels,
    available: Option<bool>,
    at: Option<Instant>,
}

#[derive(Default)]
struct Inner {
    link: Option<Link>,
    published: BTreeMap<String, Published>,
    last_state: Option<Instant>,
}

pub struct HomeAssistant {
    own: MqttService,
    inner: Mutex<Inner>,
    status: std::sync::Mutex<Status>,
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn is_virtual(id: &str) -> bool {
    crate::sim::virtual_roaster::is_simulated(id) || id == crate::sim::virtual_drum::DEVICE_ID
}

impl HomeAssistant {
    /// `own`: the server's MQTT client
    pub fn new(own: MqttService) -> Arc<Self> {
        Arc::new(Self {
            own,
            inner: Mutex::new(Inner::default()),
            status: std::sync::Mutex::new(Status::default()),
        })
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .expect("the Home Assistant status lock is poisoned")
            .clone()
    }

    /// Every second: the settings, then (each interval) the roasters.
    pub async fn run(self: Arc<Self>, state: AppState) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            self.tick(&state).await;
        }
    }

    /// One pass. Public to the crate so a test need not wait for the timer.
    pub(crate) async fn tick(&self, state: &AppState) {
        let stored = crate::auth::settings::with_prefix(&state.db, "ha_")
            .await
            .unwrap_or_default();
        let cfg = Config::from_settings(&stored);
        let built_in = crate::routes::about::broker_info();
        let mut inner = self.inner.lock().await;

        // A different broker, or switched off: take everything back from the old one
        let keep = inner
            .link
            .as_ref()
            .is_some_and(|l| cfg.enabled && l.broker == cfg.broker);
        if !keep && let Some(link) = inner.link.take() {
            withdraw_all(&link.mqtt, &mut inner.published).await;
            if link.broker.is_some() {
                link.mqtt.close().await;
            }
        }

        let mut status = Status {
            enabled: cfg.enabled,
            broker: cfg
                .broker
                .as_ref()
                .map_or_else(|| "server".into(), |b| format!("{}:{}", b.host, b.port)),
            username: cfg.broker.as_ref().and_then(|b| b.username.clone()),
            password_set: stored.get(PASSWORD).is_some_and(|v| !v.trim().is_empty()),
            prefix: cfg.prefix.clone(),
            include_virtual: cfg.include_virtual,
            interval_seconds: cfg.interval_s,
            built_in_user: built_in
                .as_ref()
                .filter(|b| b.home_assistant && cfg.broker.is_none())
                .map(|_| HA_USER.to_string()),
            built_in_without_user: built_in
                .as_ref()
                .is_some_and(|b| !b.home_assistant && cfg.broker.is_none()),
            last_published_at: self
                .status
                .lock()
                .expect("the Home Assistant status lock is poisoned")
                .last_published_at,
            ..Default::default()
        };
        if stored
            .get(BROKER)
            .is_some_and(|v| !v.trim().is_empty() && cfg.broker.is_none())
        {
            status.problem = Some("ha_broker is not an mqtt://host:port address".into());
        }
        if !cfg.enabled {
            *self
                .status
                .lock()
                .expect("the Home Assistant status lock is poisoned") = status;
            return;
        }

        if inner.link.is_none() {
            let mqtt = match &cfg.broker {
                None => Ok(self.own.clone()),
                Some(b) => connect(b).await,
            };
            match mqtt {
                Ok(mqtt) => {
                    inner.link = Some(Link {
                        broker: cfg.broker.clone(),
                        mqtt,
                        was_ready: false,
                    });
                }
                Err(e) => {
                    status.problem = Some(e);
                    *self
                        .status
                        .lock()
                        .expect("the Home Assistant status lock is poisoned") = status;
                    return;
                }
            }
        }
        let Inner {
            link,
            published,
            last_state,
        } = &mut *inner;
        let link = link.as_mut().expect("made above");
        status.connected = link.mqtt.is_ready();
        if !status.connected {
            link.was_ready = false;
            status.problem.get_or_insert_with(|| match &cfg.broker {
                Some(b) => format!("not connected to {}:{} yet", b.host, b.port),
                None => "the server is not connected to its MQTT broker".into(),
            });
            *self
                .status
                .lock()
                .expect("the Home Assistant status lock is poisoned") = status;
            return;
        }
        // (Re)connected: send everything again
        if !link.was_ready {
            link.was_ready = true;
            for p in published.values_mut() {
                p.payload.clear();
                p.available = None;
            }
        }
        let server_status_topic = if cfg.broker.is_some() {
            BRIDGE_STATUS_TOPIC
        } else {
            crate::SERVER_STATUS_TOPIC
        };

        let due = last_state.is_none_or(|t| t.elapsed() >= Duration::from_secs(cfg.interval_s));
        let public_url = crate::auth::settings::get(&state.db, "public_url").await;
        let registry = state.device_registry.read().await.clone();
        let now = now_ms();
        let mut seen = Vec::new();
        for info in state.telemetry_service.ring_summary() {
            let id = info.device_id;
            if is_virtual(&id) && !cfg.include_virtual {
                continue;
            }
            let device = crate::roasters::registry::get_device_by_device_id(&state.db, &id)
                .await
                .ok()
                .flatten();
            if device.as_ref().is_some_and(|d| {
                matches!(
                    d.device.status,
                    crate::roasters::types::DeviceStatus::Pending
                )
            }) {
                status.waiting.push(id.clone());
                continue;
            }
            let Some(sample) = state.telemetry_service.latest_sample(&id) else {
                continue;
            };
            seen.push(id.clone());
            let p = published.entry(id.clone()).or_default();
            let fresh = now.saturating_sub(sample.ts_ms) <= FRESH_MS;
            let channels = p.channels.union(Channels::of(&sample));
            let describe = Describe {
                device_id: id.clone(),
                name: device
                    .as_ref()
                    .map(|d| d.device.name.clone())
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| id.clone()),
                firmware: registry.get(&id).and_then(|r| r.version.clone()),
                channels,
                url: public_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .map(|u| format!("{}/roasters/{}", u.trim_end_matches('/'), id)),
                server_status_topic: server_status_topic.to_string(),
            };
            let topic = config_topic(&cfg.prefix, &id);
            let payload = discovery(&describe).to_string();
            let stale = p.at.is_none_or(|t| t.elapsed() >= REFRESH);
            if p.config_topic != topic || p.payload != payload || stale {
                // A new prefix: the old topic is cleared first
                if !p.config_topic.is_empty() && p.config_topic != topic {
                    let _ = link
                        .mqtt
                        .publish(&p.config_topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
                        .await;
                }
                if let Err(e) = link
                    .mqtt
                    .publish(&topic, QoS::AtLeastOnce, true, payload.clone())
                    .await
                {
                    status.problem = Some(format!("could not publish discovery: {e}"));
                    continue;
                }
                p.config_topic = topic;
                p.payload = payload;
                p.channels = channels;
                p.at = Some(Instant::now());
                p.available = None;
            }
            if p.available != Some(fresh) {
                let word = if fresh { "online" } else { "offline" };
                let _ = link
                    .mqtt
                    .publish(&availability_topic(&id), QoS::AtLeastOnce, true, word)
                    .await;
                p.available = Some(fresh);
            }
            if due && fresh {
                let live = live(state, &id).await;
                let body = self::state(Some(&sample), &live).to_string();
                let _ = link
                    .mqtt
                    .publish(&state_topic(&id), QoS::AtMostOnce, false, body)
                    .await;
            }
            status.roasters.push(HaRoaster {
                device_id: id.clone(),
                name: describe.name.clone(),
                available: fresh,
                config_topic: p.config_topic.clone(),
                state_topic: state_topic(&id),
                entities: entity_keys(&describe),
            });
        }
        // Gone (the simulated ones switched off, a roaster made pending): taken back
        let gone: Vec<String> = published
            .keys()
            .filter(|k| !seen.contains(k))
            .cloned()
            .collect();
        for id in gone {
            if let Some(p) = published.remove(&id) {
                withdraw(&link.mqtt, &id, &p).await;
            }
        }
        if due {
            *last_state = Some(Instant::now());
            if !status.roasters.is_empty() {
                status.last_published_at = Some(chrono::Utc::now());
            }
        }
        *self
            .status
            .lock()
            .expect("the Home Assistant status lock is poisoned") = status;
    }
}

fn entity_keys(d: &Describe) -> Vec<String> {
    discovery(d)["components"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// The open roast's phase, time, name and RoR warning; cooling after DROP.
async fn live(state: &AppState, id: &str) -> Live {
    let cooling = state.sequence.cooling(id).await;
    let session: Option<RoastSession> = crate::roasts::sessions::get_active_session(&state.db, id)
        .await
        .ok()
        .flatten();
    let Some(session) = session else {
        return Live {
            phase: Some(phase(None, cooling)),
            ..Default::default()
        };
    };
    let events = crate::roasts::events::get_roast_events(&state.db, &session.id)
        .await
        .unwrap_or_default();
    let roast_time_s = session
        .start_time
        .map(|t| (chrono::Utc::now() - t).num_seconds().max(0));
    let warning = state.predict.live(id).await.and_then(|p| {
        p.ror.warnings.first().map(|w| {
            let kind = match w.kind {
                crate::predict::ror_watch::RorWarningKind::Crash => "crash",
                crate::predict::ror_watch::RorWarningKind::Flick => "flick",
                crate::predict::ror_watch::RorWarningKind::Stall => "stall",
            };
            (kind, w.reason.clone())
        })
    });
    Live {
        phase: Some(phase(Some(&events), cooling)),
        roast_time_s,
        roast: Some(session.name.clone()),
        batch: session.batch_number,
        warning,
    }
}

async fn withdraw(mqtt: &MqttService, id: &str, p: &Published) {
    if !p.config_topic.is_empty() {
        let _ = mqtt
            .publish(&p.config_topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
            .await;
    }
    let _ = mqtt
        .publish(
            &availability_topic(id),
            QoS::AtLeastOnce,
            true,
            Vec::<u8>::new(),
        )
        .await;
}

/// Every roaster's discovery cleared: Home Assistant removes the devices.
async fn withdraw_all(mqtt: &MqttService, published: &mut BTreeMap<String, Published>) {
    if mqtt.is_ready() {
        for (id, p) in published.iter() {
            withdraw(mqtt, id, p).await;
        }
    }
    published.clear();
}

/// A client of its own on Home Assistant's broker, with the bridge's presence as its will.
async fn connect(b: &Broker) -> Result<MqttService, String> {
    let mut cfg = MqttConfig {
        host: b.host.clone(),
        port: b.port,
        username: b.username.clone(),
        password: b.password.clone(),
        ..MqttConfig::default()
    };
    cfg.client_id = format!("{}-ha", cfg.client_id);
    cfg.status = Some(StatusAnnouncement {
        topic: BRIDGE_STATUS_TOPIC.to_string(),
        online: json!({"status": "online", "version": env!("CARGO_PKG_VERSION")})
            .to_string()
            .into_bytes(),
        offline: json!({"status": "offline"}).to_string().into_bytes(),
    });
    MqttService::connect(cfg)
        .await
        .map_err(|e| format!("Home Assistant's broker: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: RoastEventType) -> RoastEvent {
        RoastEvent {
            id: "e".into(),
            session_id: "s".into(),
            event_type: t,
            elapsed_seconds: 0.0,
            temperature: None,
            notes: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn the_broker_address() {
        assert_eq!(
            parse_broker("mqtt://ha.local").unwrap(),
            ("ha.local".into(), 1883)
        );
        assert_eq!(
            parse_broker("mqtt://192.168.1.2:1884/").unwrap(),
            ("192.168.1.2".into(), 1884)
        );
        assert_eq!(
            parse_broker("mqtt://[::1]:1883").unwrap(),
            ("::1".into(), 1883)
        );
        assert!(parse_broker("mqtts://ha.local").is_err());
        assert!(parse_broker("http://ha.local").is_err());
        assert!(parse_broker("mqtt://ha.local:0").is_err());
        assert!(parse_broker("mqtt://user@ha.local").is_err());
        assert!(parse_broker("mqtt://").is_err());
    }

    #[test]
    fn the_prefix() {
        assert!(check_prefix("homeassistant").is_ok());
        assert!(check_prefix("ha/discovery").is_ok());
        for bad in ["", "/ha", "ha/", "ha/#", "ha/+/x", "a//b", "h a"] {
            assert!(check_prefix(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn settings_to_config() {
        let s: HashMap<String, String> = [
            (ENABLED, "true"),
            (BROKER, "mqtt://ha.lan"),
            (USERNAME, "rr"),
            (PASSWORD, "pw"),
            (PREFIX, "bad/#"),
            (INTERVAL, "500"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let c = Config::from_settings(&s);
        assert!(c.enabled);
        assert_eq!(c.prefix, DEFAULT_PREFIX, "a bad prefix falls back");
        assert_eq!(c.interval_s, 60);
        let b = c.broker.unwrap();
        assert_eq!((b.host.as_str(), b.port), ("ha.lan", 1883));
        assert_eq!(b.password.as_deref(), Some("pw"));
        let off = Config::from_settings(&HashMap::new());
        assert!(!off.enabled && off.broker.is_none() && !off.include_virtual);
        assert_eq!(off.interval_s, DEFAULT_INTERVAL_S);
    }

    #[test]
    fn the_phase_follows_the_events() {
        use RoastEventType::*;
        assert_eq!(phase(None, false), Phase::Idle);
        assert_eq!(phase(None, true), Phase::Cooling);
        assert_eq!(phase(Some(&[]), false), Phase::Drying);
        assert_eq!(
            phase(Some(&[ev(Charge), ev(DryingEnd)]), false),
            Phase::Maillard
        );
        assert_eq!(
            phase(Some(&[ev(DryingEnd), ev(FirstCrackStart)]), false),
            Phase::Development
        );
        assert_eq!(
            phase(Some(&[ev(FirstCrackStart), ev(Drop)]), false),
            Phase::Cooling
        );
    }

    #[test]
    fn topics_use_a_safe_id() {
        assert_eq!(object_id("ESP32 Roaster.01"), "esp32_roaster_01");
        assert_eq!(
            config_topic("homeassistant", "esp32_roaster_01"),
            "homeassistant/device/roastwarden_esp32_roaster_01/config"
        );
        assert_eq!(
            state_topic("esp32_roaster_01"),
            "roastwarden/ha/esp32_roaster_01/state"
        );
    }

    fn describe(ch: Channels) -> Describe {
        Describe {
            device_id: "esp32_roaster_01".into(),
            name: "Fluid bed".into(),
            firmware: Some("3.6.0".into()),
            channels: ch,
            url: Some("https://rr.example/roasters/esp32_roaster_01".into()),
            server_status_topic: crate::SERVER_STATUS_TOPIC.into(),
        }
    }

    #[test]
    fn discovery_announces_what_the_roaster_reports_and_nothing_to_command() {
        let all = Channels {
            bt: true,
            et: true,
            heater: true,
            fan: true,
            setpoint: true,
        };
        let d = discovery(&describe(all));
        let c = d["components"].as_object().unwrap();
        let mut keys: Vec<&str> = c.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "bt",
                "et",
                "fan",
                "heater",
                "phase",
                "roast",
                "roast_time",
                "roasting",
                "ror",
                "ror_warning",
                "setpoint"
            ]
        );
        // Read-only: no entity has a command topic
        assert!(!d.to_string().contains("command_topic"), "{d}");
        assert_eq!(c["bt"]["device_class"], "temperature");
        assert_eq!(c["bt"]["unit_of_measurement"], "°C");
        assert_eq!(c["bt"]["value_template"], "{{ value_json.bt }}");
        assert_eq!(c["bt"]["unique_id"], "roastwarden_esp32_roaster_01_bt");
        assert_eq!(
            c["bt"]["state_topic"],
            "roastwarden/ha/esp32_roaster_01/state"
        );
        assert_eq!(c["roasting"]["platform"], "binary_sensor");
        assert_eq!(c["phase"]["options"], json!(PHASES));
        assert_eq!(c["bt"]["availability_mode"], "all");
        assert_eq!(
            c["bt"]["availability"][1]["topic"],
            "roastwarden/server/status"
        );
        assert_eq!(
            d["device"]["identifiers"][0],
            "roastwarden_esp32_roaster_01"
        );
        assert_eq!(d["device"]["sw_version"], "3.6.0");
        assert_eq!(
            d["device"]["configuration_url"],
            "https://rr.example/roasters/esp32_roaster_01"
        );

        // A thermocouple board with BT only: no ET, heater, fan or setpoint
        let bt_only = discovery(&describe(Channels {
            bt: true,
            ..Default::default()
        }));
        let c = bt_only["components"].as_object().unwrap();
        assert!(c.contains_key("bt") && c.contains_key("ror"));
        assert!(!c.contains_key("et") && !c.contains_key("heater") && !c.contains_key("fan"));
    }

    #[test]
    fn the_state_rounds_and_leaves_unknowns_null() {
        let s = Sample {
            bt: Some(187.456),
            ror_bt: Some(9.04),
            heater: Some(41.6),
            ..Default::default()
        };
        let live = Live {
            phase: Some(Phase::Maillard),
            roast_time_s: Some(301),
            roast: Some("Kenya E".into()),
            batch: Some(14),
            warning: Some(("crash", "RoR falls 9 °C/min within a minute".into())),
        };
        let v = state(Some(&s), &live);
        assert_eq!(v["bt"], 187.5);
        assert_eq!(v["ror"], 9.0);
        assert_eq!(v["heater"], 42.0);
        assert!(v["et"].is_null() && v["fan"].is_null());
        assert_eq!(v["phase"], "maillard");
        assert_eq!(v["roasting"], true);
        assert_eq!(v["roast_time"], 301);
        assert_eq!(v["ror_warning"], "crash");
        let idle = state(Some(&s), &Live::default());
        assert_eq!(idle["phase"], "idle");
        assert_eq!(idle["roasting"], false);
        assert!(idle["roast_time"].is_null());
        assert_eq!(idle["ror_warning"], "none");
    }

    /// The whole server: switched on, the virtual roaster appears; off, it is taken back.
    mod server {
        use super::super::*;
        use crate::test_support::{Harness, eventually};
        use roastwarden_mqtt::MqttEvent;

        /// Home Assistant's messages received so far (the roasters' own are left out)
        fn drain(rx: &mut tokio::sync::broadcast::Receiver<MqttEvent>) -> Vec<(String, Vec<u8>)> {
            let mut out = Vec::new();
            while let Ok(e) = rx.try_recv() {
                if let MqttEvent::Publish { topic, payload } = e
                    && (topic.starts_with("homeassistant/") || topic.starts_with("roastwarden/ha/"))
                {
                    out.push((topic, payload));
                }
            }
            out
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn switched_on_it_announces_and_switched_off_it_takes_back() {
            let h = Harness::new().await;
            let mqtt = h.state.mqtt.clone();
            mqtt.subscribe("homeassistant/#", QoS::AtMostOnce)
                .await
                .unwrap();
            mqtt.subscribe("roastwarden/ha/#", QoS::AtMostOnce)
                .await
                .unwrap();
            let mut rx = mqtt.events();
            let r = h
                .owner(
                    "POST",
                    "/api/devices",
                    Some(json!({"name": "Fluid bed", "device_id": "esp32_roaster_01"})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            // Added through the API it is pending, and a pending roaster is not announced
            let id = r.json()["id"].as_str().unwrap().to_string();
            let r = h
                .owner(
                    "PUT",
                    &format!("/api/devices/{id}"),
                    Some(json!({"status": "active"})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let r = h
                .owner(
                    "POST",
                    "/api/test/emit-telemetry/esp32_roaster_01",
                    Some(
                        json!({"beanTemp": 150.5, "envTemp": 210.2, "rateOfRise": 12.0,
                                "heater": 40, "fan": 80}),
                    ),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let ts = h.state.telemetry_service.clone();
            assert!(
                eventually(|| {
                    let heard = ts.latest_sample("esp32_roaster_01").is_some();
                    async move { heard }
                })
                .await
            );

            // Off by default: nothing
            h.state.home_assistant.tick(&h.state).await;
            let st = h.owner("GET", "/api/home-assistant", None).await.json();
            assert_eq!(st["enabled"], false, "{st}");
            assert!(drain(&mut rx).is_empty(), "nothing is published while off");

            let r = h
                .owner(
                    "PUT",
                    "/api/settings",
                    Some(json!({"values": {"ha_enabled": "true"}})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            h.state.home_assistant.tick(&h.state).await;

            let got: HashMap<String, Vec<u8>> = drain(&mut rx).into_iter().collect();
            let status = h.owner("GET", "/api/home-assistant", None).await.json();
            assert!(!got.is_empty(), "{status}");
            let cfg = got
                .get("homeassistant/device/roastwarden_esp32_roaster_01/config")
                .expect("discovery");
            let cfg: Value = serde_json::from_slice(cfg).unwrap();
            assert!(cfg["components"]["et"].is_object(), "{cfg}");
            assert!(cfg["components"].get("setpoint").is_none(), "{cfg}");
            assert_eq!(
                got.get("roastwarden/ha/esp32_roaster_01/availability")
                    .map(Vec::as_slice),
                Some(&b"online"[..])
            );
            let st: Value =
                serde_json::from_slice(&got["roastwarden/ha/esp32_roaster_01/state"]).unwrap();
            assert_eq!(st["bt"], 150.5);
            assert_eq!(st["phase"], "idle");

            let status = h.owner("GET", "/api/home-assistant", None).await.json();
            assert_eq!(status["connected"], true, "{status}");
            assert_eq!(status["broker"], "server");
            assert_eq!(status["roasters"][0]["device_id"], "esp32_roaster_01");
            assert_eq!(status["roasters"][0]["name"], "Fluid bed");
            assert_eq!(status["roasters"][0]["available"], true);

            // The simulated roaster only when asked for
            assert!(
                status["roasters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|r| r["device_id"] != "virtual_roaster")
            );

            // Off: the discovery cleared (retained empty message)
            h.owner(
                "PUT",
                "/api/settings",
                Some(json!({"values": {"ha_enabled": "false"}})),
            )
            .await;
            h.state.home_assistant.tick(&h.state).await;
            let cleared = eventually(|| {
                let found = drain(&mut rx).iter().any(|(t, p)| {
                    t.ends_with("roastwarden_esp32_roaster_01/config") && p.is_empty()
                });
                async move { found }
            })
            .await;
            assert!(cleared, "switched off, the discovery is cleared");
        }

        #[tokio::test]
        async fn its_settings_are_checked() {
            let h = Harness::new().await;
            for (k, v) in [
                ("ha_broker", "http://ha.lan"),
                ("ha_prefix", "ha/#"),
                ("ha_interval_seconds", "0"),
                ("ha_enabled", "yes"),
            ] {
                let mut body = serde_json::Map::new();
                body.insert(k.to_string(), json!(v));
                let r = h
                    .owner(
                        "PUT",
                        "/api/settings",
                        Some(json!({"values": Value::Object(body)})),
                    )
                    .await;
                assert_eq!(r.status.as_u16(), 400, "{k}={v}: {:?}", r.json());
            }
            let r = h
                .owner(
                    "PUT",
                    "/api/settings",
                    Some(json!({"values": {"ha_password": "s3cret", "ha_broker": "mqtt://ha.lan:1883"}})),
                )
                .await;
            assert!(r.status.is_success(), "{:?}", r.json());
            let all = h.owner("GET", "/api/settings", None).await.json();
            assert!(
                !all.to_string().contains("s3cret"),
                "the password is never read back"
            );
        }
    }
}
