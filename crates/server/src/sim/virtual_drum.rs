//! The simulated drum roaster (`docs/virtual-roaster.md`, "The simulated drum").
//!
//! A second simulator beside the fluid bed (`virtual_roaster.rs`), and a stranger's roaster on
//! purpose: it speaks no Roastwarden JSON. It publishes `sim/drum/state` once a second in °F,
//! `{"temps":{"bean","exhaust","inlet"},"burner","air","drum"}`, and takes Tasmota-style
//! commands, a plain number on `sim/drum/cmnd/burner|air|drum` and anything on
//! `sim/drum/cmnd/stop`. It has no PID and sends no caps, so Profile drive runs the server's
//! loop on it. The server reads it through exactly the path a real one takes: an MQTT
//! connection with an `ingest` mapping and command templates, and a setup (kind `drum_gas`,
//! outputs Burner, Air, Drum), which the server writes once, when the drum is first switched
//! on. Nothing in the gate knows it is simulated.
//!
//! The physics are lumped and not fitted to any machine; they only behave like one: preheated
//! at 60 % burner and charged with cold beans, the turning point comes near 1:30 and first
//! crack (196 °C) near 9 min, with the exhaust above BT throughout.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use roastwarden_mqtt::{MqttEvent, MqttService};
use rumqttc::QoS;
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::roasters::channels::Channel;
use crate::roasters::setup::{Control, Kind, Output, Outputs, SetupChange, Setups};
use crate::roasters::types::{
    CreateConnectionRequest, CreateDeviceRequest, DeviceStatus, Protocol,
};
use crate::transports::ingest::c_to_f;
use crate::transports::pollers::Pollers;

pub const DEVICE_ID: &str = "virtual_drum";
/// Where it publishes its state.
pub const STATE_TOPIC: &str = "sim/drum/state";
/// Where it takes commands: `<root>/burner`, `/air`, `/drum`, `/stop`.
pub const COMMAND_ROOT: &str = "sim/drum/cmnd";
const TICK: Duration = Duration::from_secs(1);
const SETTING: &str = "virtual_drum";
/// How long the server's presence may read offline while the burner is on before it stops.
const SILENCE: Duration = Duration::from_secs(10);

const AMBIENT_C: f64 = 25.0;
/// Inlet gas rise above ambient at full burner and full air flow, °C.
const BURNER_GAIN_C: f64 = 340.0;
const TAU_INLET_S: f64 = 4.0;
/// Heat capacities (drum metal, a batch of beans), arbitrary units per °C.
const DRUM_CAPACITY: f64 = 300.0;
const BEAN_CAPACITY: f64 = 220.0;
/// Gas → drum, per unit of air flow.
const GAS_TO_DRUM: f64 = 2.0;
/// Drum → beans by contact, at the drum's usual 55 %.
const DRUM_TO_BEANS: f64 = 1.2;
/// Gas → beans by convection, per unit of air flow.
const GAS_TO_BEANS: f64 = 0.4;
/// Drum → room.
const DRUM_LOSS: f64 = 0.6;
/// How far the exhaust cools from the inlet toward the drum and beans (0..1).
const EXHAUST_EXCHANGE: f64 = 0.55;
/// The bean probe's lag in the bean mass and in the empty drum's air.
const TAU_PROBE_LOADED_S: f64 = 50.0;
const TAU_PROBE_EMPTY_S: f64 = 8.0;
/// The usual drum speed: contact heat transfer is its tuned value there.
const DRUM_SPEED_REF: f64 = 55.0;
/// The model's own step, s.
const SUBSTEP_S: f64 = 0.5;

// ------------------------------------------------------------------ model --

/// The drum's thermal state, °C.
#[derive(Debug, Clone)]
pub struct Model {
    /// Hot gas entering the drum.
    pub inlet: f64,
    /// The drum's metal.
    pub drum_metal: f64,
    /// The beans themselves.
    pub beans: f64,
    /// Air leaving the drum.
    pub exhaust: f64,
    /// What the bean probe reads.
    pub probe: f64,
    pub loaded: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            inlet: AMBIENT_C,
            drum_metal: AMBIENT_C,
            beans: AMBIENT_C,
            exhaust: AMBIENT_C,
            probe: AMBIENT_C,
            loaded: false,
        }
    }
}

impl Model {
    /// Air flow through the drum (0.25 natural draft .. 1.0).
    fn flow(air_pct: f64) -> f64 {
        0.25 + 0.75 * air_pct.clamp(0.0, 100.0) / 100.0
    }

    /// The inlet temperature the burner and air settle at: more air, cooler gas.
    pub fn inlet_steady(burner_pct: f64, air_pct: f64) -> f64 {
        AMBIENT_C
            + BURNER_GAIN_C * burner_pct.clamp(0.0, 100.0) / 100.0 / Self::flow(air_pct).sqrt()
    }

    /// Advance `dt` seconds with these outputs (%).
    pub fn step(&mut self, dt: f64, burner: f64, air: f64, drum_speed: f64) {
        let mut left = dt.clamp(0.0, 5.0);
        while left > 1e-9 {
            let h = left.min(SUBSTEP_S);
            self.substep(h, burner, air, drum_speed);
            left -= h;
        }
    }

    fn substep(&mut self, dt: f64, burner: f64, air: f64, drum_speed: f64) {
        let flow = Self::flow(air);
        let target = Self::inlet_steady(burner, air);
        self.inlet += (target - self.inlet) * dt / (TAU_INLET_S + dt);
        // A turning drum tumbles the beans against its wall: contact heat follows its speed.
        let contact = DRUM_TO_BEANS * (0.7 + 0.3 * drum_speed.clamp(0.0, 100.0) / DRUM_SPEED_REF);
        let to_beans = if self.loaded {
            contact * (self.drum_metal - self.beans)
        } else {
            0.0
        };
        let q_drum = GAS_TO_DRUM * flow * (self.inlet - self.drum_metal)
            - to_beans
            - DRUM_LOSS * (self.drum_metal - AMBIENT_C);
        self.drum_metal += q_drum * dt / DRUM_CAPACITY;
        let (probe_target, tau) = if self.loaded {
            let q_beans = to_beans + GAS_TO_BEANS * flow * (self.inlet - self.beans);
            self.beans += q_beans * dt / BEAN_CAPACITY;
            let mean = f64::midpoint(self.drum_metal, self.beans);
            self.exhaust = self.inlet - EXHAUST_EXCHANGE * (self.inlet - mean);
            (self.beans, TAU_PROBE_LOADED_S)
        } else {
            self.exhaust = self.inlet - EXHAUST_EXCHANGE * (self.inlet - self.drum_metal);
            // In the empty drum the probe sits in the air between the wall and the exhaust.
            (
                f64::midpoint(self.drum_metal, self.exhaust),
                TAU_PROBE_EMPTY_S,
            )
        };
        self.probe += (probe_target - self.probe) * dt / (tau + dt);
    }

    /// Cold beans into the drum.
    pub fn charge(&mut self) {
        self.loaded = true;
        self.beans = AMBIENT_C;
    }

    /// Beans out.
    pub fn dump(&mut self) {
        self.loaded = false;
        self.beans = AMBIENT_C;
    }
}

// ----------------------------------------------------------------- device --

/// The drum's controller: three outputs and a stop.
#[derive(Debug, Clone)]
pub struct Device {
    pub model: Model,
    pub burner: f64,
    pub air: f64,
    pub drum: f64,
    /// Since when the server's presence has read offline.
    offline_since: Option<Instant>,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            model: Model::default(),
            burner: 0.0,
            air: 0.0,
            drum: DRUM_SPEED_REF,
            offline_since: None,
        }
    }
}

fn parse_pct(p: &str) -> Option<f64> {
    p.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
}

impl Device {
    /// Burner off, air full: what STOP and the silence rule do.
    pub fn stop(&mut self) {
        self.burner = 0.0;
        self.air = 100.0;
    }

    /// One command (`command` is the topic's last segment); out of range is ignored.
    pub fn command(&mut self, command: &str, payload: &str) {
        match command {
            "burner" => {
                if let Some(v) = parse_pct(payload) {
                    self.burner = v;
                }
            }
            "air" => {
                if let Some(v) = parse_pct(payload) {
                    self.air = v;
                }
            }
            "drum" => {
                if let Some(v) = parse_pct(payload) {
                    self.drum = v;
                }
            }
            "stop" => self.stop(),
            _ => {}
        }
    }

    /// The server's presence message (`roastwarden/server/status`).
    pub fn presence(&mut self, payload: &str, now: Instant) {
        let offline = serde_json::from_str::<serde_json::Value>(payload)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string))
            .is_some_and(|s| s == "offline");
        if !offline {
            self.offline_since = None;
        } else if self.offline_since.is_none() {
            self.offline_since = Some(now);
        }
    }

    /// Advance `dt` seconds; answers the state message, in °F.
    pub fn tick(&mut self, dt: f64, now: Instant) -> serde_json::Value {
        if self.burner > 0.0
            && self
                .offline_since
                .is_some_and(|t| now.duration_since(t) >= SILENCE)
        {
            tracing::warn!("simulated drum: the server is offline, burner off");
            self.stop();
        }
        self.model.step(dt, self.burner, self.air, self.drum);
        self.state()
    }

    /// The state message, in °F as the drum speaks.
    pub fn state(&self) -> serde_json::Value {
        let f = |c: f64| (c_to_f(c) * 10.0).round() / 10.0;
        json!({
            "temps": {
                "bean": f(self.model.probe),
                "exhaust": f(self.model.exhaust),
                "inlet": f(self.model.inlet),
            },
            "burner": self.burner,
            "air": self.air,
            "drum": self.drum,
        })
    }
}

// ---------------------------------------------------------------- service --

/// The drum as the API shows it, °C.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DrumState {
    pub enabled: bool,
    pub device_id: &'static str,
    /// Beans in the drum
    pub loaded: bool,
    /// What the bean probe reads, °C
    pub bt: f64,
    /// °C
    pub exhaust: f64,
    /// Hot gas entering the drum, °C
    pub inlet: f64,
    /// %
    pub burner: f64,
    /// %
    pub air: f64,
    /// Drum speed, %
    pub drum: f64,
}

/// What first switching the drum on writes, through the normal device and setup paths.
pub struct Provisioning {
    pub setups: Arc<Setups>,
    pub pollers: Arc<Pollers>,
}

pub struct VirtualDrum {
    db: SqlitePool,
    mqtt: MqttService,
    device: Mutex<Device>,
    enabled: Mutex<bool>,
    forced: bool,
    provisioning: OnceLock<Provisioning>,
    /// One provisioning at a time.
    provision_lock: Mutex<()>,
}

/// The MQTT connection the server writes for the drum (docs/ingest-mapping.md).
pub fn connection_config() -> serde_json::Value {
    json!({
        "topic_prefix": "sim/drum",
        "telemetry_topic": STATE_TOPIC,
        "qos": 0,
        "ingest": {
            "units": "F",
            "fields": {
                "bt": "temps.bean",
                "et": "temps.exhaust",
                "inlet": { "path": "temps.inlet", "unit": "F" },
                "heater": "burner",
                "fan": "air",
                "drum": "drum"
            }
        },
        "commands": {
            "heater": { "topic": format!("{COMMAND_ROOT}/burner"), "decimals": 0 },
            "fan": { "topic": format!("{COMMAND_ROOT}/air"), "decimals": 0 },
            "drum": { "topic": format!("{COMMAND_ROOT}/drum"), "decimals": 0 },
            "emergency_stop": { "topic": format!("{COMMAND_ROOT}/stop"), "payload": "1" }
        }
    })
}

/// The setup the server writes for the drum: a gas drum under full control.
pub fn setup() -> SetupChange {
    let out = |label: &str, min: f64, max: f64, step: f64| Output {
        label: label.into(),
        min,
        max,
        step,
    };
    SetupChange {
        kind: Kind::DrumGas,
        control: Control::Full,
        outputs: Some(Outputs {
            heater: Some(out("Burner", 0.0, 100.0, 5.0)),
            fan: Some(out("Air", 0.0, 100.0, 0.0)),
            drum: Some(out("Drum", 40.0, 70.0, 0.0)),
            damper: None,
        }),
        fan_floor_pct: Some(0.0),
        // The inlet gas temperature, kept with each roast and drawn (docs/channels.md).
        channels: Some(vec![Channel {
            key: "inlet".into(),
            label: "Inlet".into(),
            unit: "C".into(),
            max: None,
        }]),
    }
}

impl VirtualDrum {
    pub fn new(db: SqlitePool, mqtt: MqttService) -> Arc<Self> {
        let forced =
            std::env::var("ROASTWARDEN_VIRTUAL_DRUM").is_ok_and(|v| v == "1" || v == "true");
        Arc::new(Self {
            db,
            mqtt,
            device: Mutex::new(Device::default()),
            enabled: Mutex::new(false),
            forced,
            provisioning: OnceLock::new(),
            provision_lock: Mutex::new(()),
        })
    }

    /// Give it what it needs to write its roaster (app.rs, once the pollers exist).
    pub fn attach(&self, p: Provisioning) {
        let _ = self.provisioning.set(p);
    }

    async fn setting_on(&self) -> bool {
        if self.forced {
            return true;
        }
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
            .bind(SETTING)
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten()
            .is_some_and(|v| v == "true")
    }

    pub async fn state(&self) -> DrumState {
        let enabled = *self.enabled.lock().await;
        let d = self.device.lock().await;
        DrumState {
            enabled,
            device_id: DEVICE_ID,
            loaded: d.model.loaded,
            bt: d.model.probe,
            exhaust: d.model.exhaust,
            inlet: d.model.inlet,
            burner: d.burner,
            air: d.air,
            drum: d.drum,
        }
    }

    pub async fn set_enabled(&self, on: bool) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(SETTING)
        .bind(if on { "true" } else { "false" })
        .execute(&self.db)
        .await?;
        self.apply_enabled(on || self.forced).await;
        Ok(())
    }

    async fn apply_enabled(&self, on: bool) {
        let was = std::mem::replace(&mut *self.enabled.lock().await, on);
        if was == on {
            return;
        }
        if on {
            *self.device.lock().await = Device::default();
            self.provision().await;
            tracing::info!("simulated drum on ({DEVICE_ID})");
        } else {
            tracing::info!("simulated drum off");
        }
    }

    /// Write its roaster, MQTT connection and setup, unless the roaster exists (the owner may
    /// have changed them since: they are left alone).
    async fn provision(&self) {
        let Some(p) = self.provisioning.get() else {
            return;
        };
        let _one = self.provision_lock.lock().await;
        match crate::roasters::registry::get_device_by_device_id(&self.db, DEVICE_ID).await {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "simulated drum: could not look up its roaster");
                return;
            }
        }
        if let Err(e) = provision(&self.db, p).await {
            tracing::warn!(error = %format!("{e:#}"), "simulated drum: could not write its roaster");
        }
    }

    pub async fn charge(&self) {
        self.device.lock().await.model.charge();
    }

    pub async fn dump(&self) {
        self.device.lock().await.model.dump();
    }

    pub async fn run(self: Arc<Self>) {
        let mut events = self.mqtt.events();
        let control = format!("{COMMAND_ROOT}/#");
        for topic in [control.as_str(), crate::SERVER_STATUS_TOPIC] {
            if let Err(e) = self.mqtt.subscribe(topic, QoS::AtMostOnce).await {
                tracing::warn!(topic, error = ?e, "simulated drum: subscribe failed");
            }
        }
        let prefix = format!("{COMMAND_ROOT}/");
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last = Instant::now();
        let mut checked: Option<Instant> = None;
        loop {
            tokio::select! {
                ev = events.recv() => {
                    let Ok(MqttEvent::Publish { topic, payload }) = ev else { continue };
                    let text = String::from_utf8_lossy(&payload).to_string();
                    if topic == crate::SERVER_STATUS_TOPIC {
                        self.device.lock().await.presence(&text, Instant::now());
                        continue;
                    }
                    let Some(cmd) = topic.strip_prefix(&prefix) else { continue };
                    if !*self.enabled.lock().await {
                        continue;
                    }
                    self.device.lock().await.command(cmd, &text);
                }
                _ = tick.tick() => {
                    if checked.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
                        checked = Some(Instant::now());
                        let on = self.setting_on().await;
                        self.apply_enabled(on).await;
                    }
                    let now = Instant::now();
                    let dt = now.duration_since(last).as_secs_f64();
                    last = now;
                    if !*self.enabled.lock().await {
                        continue;
                    }
                    let doc = self.device.lock().await.tick(dt, now);
                    if let Err(e) = self.mqtt.publish(STATE_TOPIC, QoS::AtMostOnce, false, doc.to_string()).await {
                        tracing::debug!(error = %e, "simulated drum: state not sent");
                    }
                }
            }
        }
    }
}

async fn provision(db: &SqlitePool, p: &Provisioning) -> anyhow::Result<()> {
    let dev = crate::roasters::registry::create_device(
        db,
        CreateDeviceRequest {
            name: "Simulated drum".into(),
            device_id: DEVICE_ID.into(),
            profile_id: None,
            description: Some(
                "Roastwarden's simulated gas drum roaster (docs/virtual-roaster.md)".into(),
            ),
            location: None,
        },
    )
    .await?;
    crate::roasters::registry::update_device_status(db, &dev.id, DeviceStatus::Active).await?;
    crate::roasters::registry::add_connection(
        db,
        &dev.id,
        CreateConnectionRequest {
            protocol: Protocol::Mqtt,
            enabled: Some(true),
            priority: Some(0),
            config: connection_config(),
        },
    )
    .await?;
    p.setups
        .set(DEVICE_ID, setup())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Into the MQTT route table: subscribes to sim/drum/state.
    p.pollers.sync_device(&dev.id).await;
    tracing::info!("simulated drum: wrote its roaster, connection and setup");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preheat at 60 % burner, 50 % air, drum 55 %; charge; hold. (TP s, TP BT, FC s,
    /// exhaust above BT from TP to FC, charge BT).
    fn roast() -> (f64, f64, Option<f64>, bool, f64) {
        let mut m = Model::default();
        for _ in 0..1800 {
            m.step(1.0, 60.0, 50.0, 55.0);
        }
        let charge_bt = m.probe;
        let drum_before = m.drum_metal;
        m.charge();
        let (mut tp, mut low, mut fc, mut exhaust_above) = (0.0, f64::MAX, None, true);
        for i in 1..=720 {
            m.step(1.0, 60.0, 50.0, 55.0);
            let t = i as f64;
            if m.probe < low {
                low = m.probe;
                tp = t;
            }
            if fc.is_none() && t > tp && m.probe >= 196.0 {
                fc = Some(t);
            }
            if t > tp + 5.0 && fc.is_none() && m.exhaust <= m.probe {
                exhaust_above = false;
            }
            if i == 60 {
                assert!(
                    m.drum_metal < drum_before - 10.0,
                    "the charge cools the drum"
                );
            }
        }
        (tp, low, fc, exhaust_above, charge_bt)
    }

    #[test]
    fn a_roast_at_60_percent_turns_near_1_30_and_cracks_near_9_min() {
        let (tp, low, fc, exhaust_above, charge) = roast();
        assert!((190.0..240.0).contains(&charge), "charge BT {charge}");
        assert!((60.0..=120.0).contains(&tp), "turning point at {tp} s");
        assert!((70.0..130.0).contains(&low), "turning point BT {low}");
        let fc = fc.expect("first crack within 12 min");
        assert!((480.0..=630.0).contains(&fc), "first crack at {fc} s");
        assert!(exhaust_above, "exhaust above BT from TP to FC");
    }

    /// The drum seed profiles (migration 051, `docs/profiles.md`) on the simulated drum: preheated
    /// to the profile's charge BT, charged, and run open loop on the profile's own burner and
    /// air steps, the drum follows the BT curve roughly: within 10 °C from the turning point on,
    /// first crack within 45 s of the profile's, the drop within 6 °C of its temperature.
    #[tokio::test]
    async fn the_drum_seeds_run_open_loop_on_the_simulated_drum() {
        use crate::profiles::types::ProfilePoint;
        let db = crate::db::open_memory().await;
        for id in ["drum-light-filter", "drum-medium-omni"] {
            let p = crate::profiles::store::get_profile_with_points(&db, id)
                .await
                .unwrap()
                .unwrap();
            let pts = &p.points;
            let charge = f64::from(p.profile.charge_temp.unwrap());
            // Preheat: the lowest burner (0.5 % steps) that holds the probe at the charge BT.
            let mut m = (80..=200)
                .map(|b| {
                    let mut m = Model::default();
                    for _ in 0..1800 {
                        m.step(1.0, f64::from(b) / 2.0, 40.0, DRUM_SPEED_REF);
                    }
                    m
                })
                .find(|m| m.probe >= charge)
                .expect("the drum preheats to the charge BT");
            m.charge();
            // The outputs step at each point (as a drum roaster sets them); BT is the line.
            let output = |t: i32, f: fn(&ProfilePoint) -> Option<i32>| {
                let p = pts.iter().rev().find(|p| p.time_seconds <= t).unwrap();
                f64::from(f(p).unwrap())
            };
            let bt = |t: i32| {
                let i = pts.iter().position(|p| p.time_seconds >= t).unwrap();
                let (a, b) = (&pts[i.saturating_sub(1)], &pts[i]);
                if b.time_seconds == a.time_seconds {
                    return f64::from(b.target_temp);
                }
                let x = f64::from(t - a.time_seconds) / f64::from(b.time_seconds - a.time_seconds);
                f64::from(a.target_temp) + x * f64::from(b.target_temp - a.target_temp)
            };
            let tp = pts
                .iter()
                .min_by(|a, b| a.target_temp.total_cmp(&b.target_temp))
                .unwrap()
                .time_seconds;
            let (fc, total) = (
                p.profile.target_first_crack.unwrap(),
                p.profile.target_total_time.unwrap(),
            );
            let (mut worst, mut fc_at) = (0.0_f64, None);
            for t in 1..=total {
                m.step(
                    1.0,
                    output(t - 1, |p| p.heater_pct),
                    output(t - 1, |p| p.fan_speed),
                    DRUM_SPEED_REF,
                );
                if t >= tp {
                    worst = worst.max((m.probe - bt(t)).abs());
                    if fc_at.is_none() && m.probe >= 196.0 {
                        fc_at = Some(t);
                    }
                }
            }
            assert!(worst < 10.0, "{id}: {worst:.1} °C off the curve");
            let fc_at = fc_at.unwrap_or_else(|| panic!("{id}: no first crack"));
            assert!(
                (fc_at - fc).abs() <= 45,
                "{id}: FC at {fc_at} s, profile {fc} s"
            );
            let drop = f64::from(p.profile.target_end_temp.unwrap());
            assert!((m.probe - drop).abs() < 6.0, "{id}: drop {:.1} °C", m.probe);
        }
    }

    #[test]
    fn more_burner_roasts_faster_and_no_burner_cools() {
        let fc_at = |burner: f64| {
            let mut m = Model::default();
            for _ in 0..1800 {
                m.step(1.0, 60.0, 50.0, 55.0);
            }
            m.charge();
            (1..=1200)
                .find(|_| {
                    m.step(1.0, burner, 50.0, 55.0);
                    m.probe >= 196.0 && m.beans > 150.0
                })
                .unwrap_or(1200)
        };
        assert!(fc_at(80.0) < fc_at(60.0));
        assert!(fc_at(60.0) < fc_at(45.0));
        let mut m = Model::default();
        for _ in 0..1800 {
            m.step(1.0, 60.0, 50.0, 55.0);
        }
        let hot = m.probe;
        for _ in 0..600 {
            m.step(1.0, 0.0, 100.0, 55.0);
        }
        assert!(m.probe < hot - 80.0, "{hot} → {}", m.probe);
    }

    #[test]
    fn commands_set_outputs_ignore_nonsense_and_stop_cuts_the_burner() {
        let mut d = Device::default();
        d.command("burner", "60");
        d.command("air", " 45 ");
        d.command("drum", "62.5");
        assert_eq!((d.burner, d.air, d.drum), (60.0, 45.0, 62.5));
        d.command("burner", "150");
        d.command("air", "-3");
        d.command("drum", "fast");
        d.command("heater", "10");
        assert_eq!((d.burner, d.air, d.drum), (60.0, 45.0, 62.5));
        d.command("stop", "1");
        assert_eq!((d.burner, d.air), (0.0, 100.0));
        assert_eq!(d.drum, 62.5, "STOP leaves the drum turning");
    }

    #[test]
    fn the_server_offline_for_10_s_with_the_burner_on_stops_it() {
        let mut d = Device::default();
        let t0 = Instant::now();
        d.command("burner", "60");
        d.presence(r#"{"status":"online"}"#, t0);
        d.tick(1.0, t0 + Duration::from_secs(30));
        assert_eq!(d.burner, 60.0);
        d.presence(r#"{"status":"offline"}"#, t0);
        d.tick(1.0, t0 + Duration::from_secs(9));
        assert_eq!(d.burner, 60.0, "not yet");
        d.tick(1.0, t0 + Duration::from_secs(10));
        assert_eq!((d.burner, d.air), (0.0, 100.0));
        // Back online: it takes commands again and keeps them
        d.presence(r#"{"status":"online","version":"x"}"#, t0);
        d.command("burner", "40");
        d.tick(1.0, t0 + Duration::from_secs(60));
        assert_eq!(d.burner, 40.0);
    }

    #[test]
    fn the_state_message_is_in_fahrenheit_with_its_own_keys() {
        let d = Device::default();
        let s = d.state();
        assert_eq!(s["temps"]["bean"], 77.0, "25 °C");
        assert_eq!(s["temps"]["exhaust"], 77.0);
        assert_eq!(s["temps"]["inlet"], 77.0);
        assert_eq!(s["burner"], 0.0);
        assert_eq!(s["air"], 0.0);
        assert_eq!(s["drum"], 55.0);
        assert!(s.get("beanTemp").is_none(), "no Roastwarden JSON");
    }

    #[test]
    fn its_connection_and_setup_pass_the_servers_checks() {
        crate::transports::pollers::check_connection_config(&Protocol::Mqtt, &connection_config())
            .unwrap();
        let s = setup();
        crate::roasters::setup::validate(s.outputs.as_ref().unwrap(), 0.0).unwrap();
        // The mapping reads a state message into °C.
        let ingest = crate::transports::ingest::parse_ingest(
            &connection_config()["ingest"],
            crate::transports::ingest::Kind::Mqtt,
        )
        .unwrap();
        let mut d = Device::default();
        d.command("burner", "60");
        d.command("drum", "55");
        let m = ingest.map(&d.state()).unwrap();
        assert_eq!(m["beanTemp"], 25.0);
        assert_eq!(m["envTemp"], 25.0);
        assert_eq!(m["inlet"], 25.0);
        assert_eq!(m["heater"], 60.0);
        assert_eq!(m["drum"], 55.0);
    }

    async fn drum_on(h: &crate::test_support::Harness) {
        let r = h
            .owner("PUT", "/api/virtual-roaster", Some(json!({"drum": true})))
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        assert_eq!(r.json()["drum"]["enabled"], true, "{}", r.json());
        assert_eq!(r.json()["enabled"], false, "the fluid bed stays off");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn switching_it_on_writes_its_roaster_once_and_it_is_driven_through_the_gate() {
        use crate::test_support::{Harness, eventually};
        let h = Harness::new().await;
        drum_on(&h).await;

        // Its roaster, connection and setup, written through the normal paths
        let dev = h
            .owner("GET", "/api/devices/virtual_drum", None)
            .await
            .json();
        assert_eq!(dev["status"], "active", "{dev}");
        let conns = dev["connections"].as_array().unwrap();
        assert_eq!(conns.len(), 1, "{dev}");
        assert_eq!(conns[0]["protocol"], "mqtt");
        assert_eq!(conns[0]["config"]["telemetry_topic"], STATE_TOPIC);
        let setup = h
            .owner("GET", "/api/roaster/virtual_drum/setup", None)
            .await
            .json();
        assert_eq!(setup["kind"], "drum_gas", "{setup}");
        assert_eq!(setup["control"], "full");
        assert_eq!(setup["fixed"], false, "a normal setup row");
        assert_eq!(setup["outputs"]["heater"]["label"], "Burner");
        assert_eq!(setup["outputs"]["heater"]["step"], 5.0);
        assert_eq!(setup["outputs"]["drum"]["min"], 40.0);
        assert_eq!(setup["outputs"]["drum"]["max"], 70.0);
        assert_eq!(
            setup["channels"],
            json!([{"key": "inlet", "label": "Inlet", "unit": "C", "max": null}]),
            "{setup}"
        );

        // Samples arrive in °C through the mapping
        let latest = || async {
            h.owner("GET", "/api/roaster/virtual_drum/telemetry/latest", None)
                .await
                .json()
        };
        assert!(eventually(|| async { latest().await["telemetry"]["beanTemp"].is_number() }).await);
        let l = latest().await;
        let bt = l["telemetry"]["beanTemp"].as_f64().unwrap();
        assert!((20.0..30.0).contains(&bt), "BT {bt} °C: {l}");
        assert_eq!(l["telemetry"]["drum"], 55.0, "{l}");
        let inlet = l["telemetry"]["inlet"].as_f64().unwrap();
        assert!((20.0..30.0).contains(&inlet), "the inlet channel, °C: {l}");

        // Commands go through the gate and the templates: burner in steps of 5, drum 40–70
        let r = h
            .owner(
                "POST",
                "/api/roaster/virtual_drum/control/heater",
                Some(json!({"percent": 62})),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        let r = h
            .owner(
                "POST",
                "/api/roaster/virtual_drum/control/drum",
                Some(json!({"percent": 90})),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        let drum = &h.state.virtual_roaster.drum;
        assert!(
            eventually(|| async {
                let s = drum.state().await;
                s.burner == 60.0 && s.drum == 70.0
            })
            .await,
            "{:?}",
            drum.state().await
        );
        // STOP: burner off, air full
        let r = h
            .owner(
                "POST",
                "/api/roaster/virtual_drum/control/emergency_stop",
                None,
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        assert!(
            eventually(|| async {
                let s = drum.state().await;
                s.burner == 0.0 && s.air == 100.0
            })
            .await
        );

        // The owner changes its setup; switching it off and on again leaves that alone
        let r = h
            .owner(
                "PUT",
                "/api/roaster/virtual_drum/setup",
                Some(json!({"kind": "drum_gas", "control": "manual"})),
            )
            .await;
        assert!(r.status.is_success(), "{:?}", r.json());
        h.owner("PUT", "/api/virtual-roaster", Some(json!({"drum": false})))
            .await;
        drum_on(&h).await;
        let setup = h
            .owner("GET", "/api/roaster/virtual_drum/setup", None)
            .await
            .json();
        assert_eq!(setup["control"], "manual", "{setup}");
        let devices = h.owner("GET", "/api/devices", None).await.json();
        let drums = devices
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["device_id"] == DEVICE_ID)
            .count();
        assert_eq!(drums, 1);

        // Charge and dump by the query
        let r = h
            .owner(
                "POST",
                "/api/virtual-roaster/charge?roaster=virtual_drum",
                None,
            )
            .await;
        assert_eq!(r.json()["drum"]["loaded"], true, "{}", r.json());
        assert_eq!(r.json()["loaded"], false, "the fluid bed's beans are not");
        let r = h
            .owner(
                "POST",
                "/api/virtual-roaster/dump?roaster=virtual_drum",
                None,
            )
            .await;
        assert_eq!(r.json()["drum"]["loaded"], false);
        let r = h
            .owner("POST", "/api/virtual-roaster/charge?roaster=nope", None)
            .await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
    }
}
