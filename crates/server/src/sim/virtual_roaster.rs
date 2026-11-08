//! Virtual roaster (`docs/virtual-roaster.md`).
//!
//! A simulated roaster inside the server that speaks MQTT exactly like the
//! ESP32 firmware: it publishes `roaster/virtual_roaster/telemetry` and a
//! retained status with capabilities, and obeys
//! `roaster/virtual_roaster/control/*`. Replay, alarms, fan assist, the gain
//! schedule and the dashboard therefore treat it as a real roaster, so a
//! profile, an alarm set or a fan setting can be rehearsed without beans.
//! Unlike Artisan's simulator (playback of a recorded roast) it is closed
//! loop: the heater and fan change what happens.
//!
//! The heat model is fitted to this roaster's autotunes: at
//! 100 % fan the heater holds 165 °C with 88 % and tops out near 185 °C;
//! at 75 % fan 78 % holds 190 °C. The air (ET) follows the heater with an
//! 8 s lag; the beans follow the air with a 60 s lag (6 s with an empty
//! chamber, where the probe sits in the air).

use std::sync::Arc;
use std::time::{Duration, Instant};

use roastwarden_mqtt::{MqttEvent, MqttService};
use rumqttc::QoS;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::replay::engine::{PidGains, ServerPid};

pub const DEVICE_ID: &str = "virtual_roaster";

/// A simulated fluid bed: the shared `virtual_roaster`, or a demo visitor's own (`demo.rs`,
/// `virtual_` and 8 hex digits), which the server treats the same way.
pub fn is_simulated(id: &str) -> bool {
    id == DEVICE_ID || crate::ops::demo::is_visitor_roaster(id)
}
const TICK: Duration = Duration::from_secs(1);
const STATUS_EVERY: Duration = Duration::from_secs(30);
const SETTING: &str = "virtual_roaster";

const AMBIENT_C: f64 = 25.0;
/// ET rise above ambient per heater fraction at 100 % fan, °C.
const AIR_GAIN_C: f64 = 164.0;
/// The bean probe reads this much under the air at equilibrium, °C.
const PROBE_OFFSET_C: f64 = 5.0;
const TAU_AIR_S: f64 = 8.0;
const TAU_BEANS_S: f64 = 60.0;
const TAU_EMPTY_S: f64 = 6.0;
/// The firmware cuts the heater below this fan (`SAFETY_MIN_FAN_PCT`).
const MIN_FAN_PCT: f64 = 40.0;
const MAX_ET_C: f64 = 320.0;

// ------------------------------------------------------------------ model --

/// Thermal state of the chamber and beans.
#[derive(Debug, Clone)]
pub struct Model {
    pub bt: f64,
    pub et: f64,
    /// Beans in the chamber.
    pub loaded: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            bt: AMBIENT_C,
            et: AMBIENT_C,
            loaded: false,
        }
    }
}

impl Model {
    /// Air temperature the heater and fan settle at, °C.
    pub fn et_steady(heater_pct: f64, fan_pct: f64) -> f64 {
        let h = heater_pct.clamp(0.0, 100.0) / 100.0;
        let f = fan_pct.clamp(0.0, 100.0) / 100.0;
        if f < 0.05 {
            return AMBIENT_C;
        }
        (AMBIENT_C + h * AIR_GAIN_C / f).min(MAX_ET_C)
    }

    pub fn step(&mut self, dt: f64, heater_applied_pct: f64, fan_pct: f64) {
        let dt = dt.clamp(0.0, 5.0);
        let et_ss = Self::et_steady(heater_applied_pct, fan_pct);
        self.et += (et_ss - self.et) * (dt / (TAU_AIR_S + dt));
        let tau = if self.loaded {
            TAU_BEANS_S
        } else {
            TAU_EMPTY_S
        };
        let target = self.et - PROBE_OFFSET_C * (self.et - AMBIENT_C) / AIR_GAIN_C.max(1.0);
        self.bt += (target - self.bt) * (dt / (tau + dt));
    }

    /// Cold beans into the chamber: the probe now reads the beans.
    pub fn charge(&mut self) {
        self.loaded = true;
        self.bt = AMBIENT_C;
    }

    /// Beans out: the probe is back in the air.
    pub fn dump(&mut self) {
        self.loaded = false;
    }
}

// ----------------------------------------------------------------- device --

/// The firmware's control behaviour (`heater_control.cpp`, safety.cpp).
#[derive(Debug, Clone)]
pub struct Device {
    pub model: Model,
    pub auto: bool,
    pub setpoint: f64,
    /// Manual heater command, %.
    pub heater_cmd: f64,
    /// Heater output the loop or the operator commands, %.
    pub heater_out: f64,
    pub fan: f64,
    pub heater_enabled: bool,
    /// Gains in firmware units (0..255 output).
    pub gains: (f64, f64, f64),
    pid: ServerPid,
    pid_started: bool,
    pub ff: Option<f64>,
    lease_ms: u32,
    lease_at: Option<Instant>,
    pub ror_limit: (f64, f64),
    history: std::collections::VecDeque<(f64, f64)>,
    pub uptime: f64,
    noise: u64,
}

impl Default for Device {
    fn default() -> Self {
        // The fluid bed's own PI gains at 100 % fan, from its server autotune (SIMC);
        // the PID with D it had before made the heater jump with every noisy BT reading.
        let gains = (8.355, 0.22269, 0.0);
        Self {
            model: Model::default(),
            auto: false,
            setpoint: 200.0,
            heater_cmd: 0.0,
            heater_out: 0.0,
            fan: 0.0,
            heater_enabled: false,
            gains,
            pid: ServerPid::new(pct_gains(gains)),
            pid_started: false,
            ff: None,
            lease_ms: 0,
            lease_at: None,
            ror_limit: (0.0, 0.0),
            history: std::collections::VecDeque::new(),
            uptime: 0.0,
            noise: 0x9e37_79b9_7f4a_7c15,
        }
    }
}

fn pct_gains(g: (f64, f64, f64)) -> PidGains {
    PidGains {
        kp: g.0 / 2.55,
        ki: g.1 / 2.55,
        kd: g.2 / 2.55,
    }
}

fn parse_pct(p: &str) -> Option<f64> {
    p.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
}

impl Device {
    /// The heater power that reaches the element, %.
    pub fn heater_applied(&self) -> f64 {
        if !self.heater_enabled || self.fan < MIN_FAN_PCT {
            0.0
        } else {
            self.heater_out
        }
    }

    fn safe_state(&mut self) {
        self.heater_enabled = false;
        self.auto = false;
        self.heater_cmd = 0.0;
        self.heater_out = 0.0;
        self.ff = None;
        self.lease_ms = 0;
        self.lease_at = None;
    }

    /// One control message (`command` is the last topic segment).
    pub fn command(&mut self, command: &str, payload: &str, now: Instant) {
        let p = payload.trim();
        match command {
            "setpoint" => {
                if let Some(v) = p.parse::<f64>().ok().filter(|v| (0.0..=300.0).contains(v)) {
                    self.setpoint = v;
                }
            }
            "fan" => {
                if let Some(v) = parse_pct(p) {
                    self.fan = v;
                }
            }
            "heater" => {
                if let Some(v) = parse_pct(p) {
                    self.heater_cmd = v;
                    // The firmware sets its output at once in manual, so a switch to auto
                    // right after starts the PID from this value.
                    if !self.auto {
                        self.heater_out = v;
                    }
                }
            }
            "mode" => match p {
                "auto" if !self.auto => {
                    self.auto = true;
                    self.pid_started = false;
                }
                "manual" => {
                    self.auto = false;
                    self.heater_cmd = self.heater_out;
                }
                _ => {}
            },
            "heater_enable" => self.heater_enabled = p == "1" || p.eq_ignore_ascii_case("true"),
            "pid" => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(p) {
                    let g = |k: &str| {
                        v.get(k)
                            .and_then(serde_json::Value::as_f64)
                            .filter(|x| x.is_finite() && *x >= 0.0)
                    };
                    if let (Some(kp), Some(ki), Some(kd)) = (g("kp"), g("ki"), g("kd")) {
                        self.gains = (kp, ki, kd);
                        self.pid.gains = pct_gains(self.gains);
                    }
                }
            }
            "emergency_stop" => {
                self.safe_state();
                self.fan = 100.0;
            }
            "feedforward" => {
                self.ff = if p == "off" { None } else { parse_pct(p) };
            }
            "lease" => {
                self.lease_ms = p.parse().unwrap_or(0);
                self.lease_at = (self.lease_ms > 0).then_some(now);
            }
            "ror_limit" => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(p) {
                    let g = |k: &str| v.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                    self.ror_limit = (g("max"), g("min_temp"));
                }
            }
            _ => {}
        }
    }

    /// Uniform in (0, 1) from xorshift64.
    fn uniform(&mut self) -> f64 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        ((self.noise >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// One thermocouple read's noise: σ 0.25 °C, as the real roaster's MAX6675 measured idle
    /// (about one 0.25 °C step).
    fn noise(&mut self) -> f64 {
        let (a, b) = (self.uniform(), self.uniform());
        0.25 * (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
    }

    /// One sample as firmware 3.4.0 makes it: four reads in 0.25 °C steps, averaged.
    fn read(&mut self, truth: f64) -> f64 {
        let sum: f64 = (0..4)
            .map(|_| ((truth + self.noise()) * 4.0).round() / 4.0)
            .sum();
        ((sum / 4.0) * 100.0).round() / 100.0
    }

    /// Advance `dt` seconds; returns the telemetry JSON the firmware sends.
    pub fn tick(&mut self, dt: f64, now: Instant) -> serde_json::Value {
        self.uptime += dt;
        if self.lease_ms > 0
            && self
                .lease_at
                .is_some_and(|t| now.duration_since(t).as_millis() > u128::from(self.lease_ms))
        {
            self.safe_state();
        }
        // One sample per tick, as firmware 3.4.0 makes it (four MAX6675 reads in 0.25 °C steps,
        // averaged). The PID works from it and it is the BT the telemetry reports.
        let bt_read = self.read(self.model.bt);
        let ff = self.ff.unwrap_or(0.0);
        // As the firmware (heater_control.cpp): with heater power off the output is held at 0
        // (a manual heater % is lost too) and the PID starts again, bumplessly from that 0,
        // when heating resumes in auto. That start is what loaded a large negative integral
        // at CHARGE on the real roaster (docs/roast-sequence.md, "Starting the PID").
        if !self.heater_enabled {
            self.heater_out = 0.0;
            self.heater_cmd = 0.0;
            self.pid_started = false;
        }
        self.heater_out = if !self.heater_enabled {
            0.0
        } else if self.auto {
            if !self.pid_started {
                self.pid.reset(self.setpoint, bt_read, self.heater_out, ff);
                self.pid_started = true;
            }
            self.pid.compute(self.setpoint, Some(bt_read), dt, ff)
        } else {
            self.heater_cmd
        };
        let applied = self.heater_applied();
        self.model.step(dt, applied, self.fan);

        let bt = bt_read;
        let et = self.read(self.model.et);
        self.history.push_back((self.uptime, bt));
        while self.history.len() > 10 {
            self.history.pop_front();
        }
        let ror = match (self.history.front(), self.history.back()) {
            (Some(a), Some(b)) if b.0 > a.0 => (b.1 - a.1) / (b.0 - a.0) * 60.0,
            _ => 0.0,
        };
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        let mut doc = serde_json::json!({
            "uptimeMs": (self.uptime * 1000.0) as u64,
            "uptime": self.uptime as u64,
            "beanTemp": bt,
            "envTemp": et,
            "rateOfRise": (ror * 100.0).round() / 100.0,
            "heater": r1(self.heater_out),
            "heaterApplied": r1(applied),
            "rorLimited": 0,
            "btSpikes": 0,
            "etSpikes": 0,
            "fan": r1(self.fan),
            "setpoint": r1(self.setpoint),
            "controlMode": u8::from(self.auto),
            "heaterEnable": u8::from(self.heater_enabled),
            "Kp": self.gains.0,
            "Ki": self.gains.1,
            "Kd": self.gains.2,
        });
        if let Some(f) = self.ff {
            doc["ff"] = serde_json::json!(r1(f));
        }
        doc
    }

    pub fn status(&self, online: bool) -> serde_json::Value {
        self.status_as(DEVICE_ID, online)
    }

    /// The retained status as `id` (the shared roaster, or a demo visitor's).
    pub fn status_as(&self, id: &str, online: bool) -> serde_json::Value {
        serde_json::json!({
            "status": if online { "online" } else { "offline" },
            "id": id,
            "ip": "virtual",
            "version": "virtual-1",
            "caps": ["pid", "ff", "lease", "ror"],
            "ror_limit": { "max": self.ror_limit.0, "min_temp": self.ror_limit.1 },
        })
    }
}

// ---------------------------------------------------------------- service --

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct VirtualState {
    pub enabled: bool,
    /// `virtual_roaster`, or the demo visitor's own simulated roaster asked for
    pub device_id: String,
    pub loaded: bool,
    pub bt: f64,
    pub et: f64,
    pub heater_applied: f64,
    pub fan: f64,
    /// The simulated drum roaster beside it (`virtual_drum`, docs/virtual-roaster.md)
    pub drum: crate::sim::virtual_drum::DrumState,
}

pub struct VirtualRoaster {
    /// The simulated drum, switched on beside it (`virtual_drum.rs`)
    pub drum: Arc<crate::sim::virtual_drum::VirtualDrum>,
    db: SqlitePool,
    mqtt: MqttService,
    device: Mutex<Device>,
    /// Demo visitors' own simulated roasters (demo.rs), by device id
    visitors: Mutex<std::collections::HashMap<String, Device>>,
    enabled: Mutex<bool>,
    forced: bool,
}

impl VirtualRoaster {
    pub fn new(db: SqlitePool, mqtt: MqttService) -> Arc<Self> {
        let forced =
            std::env::var("ROASTWARDEN_VIRTUAL_ROASTER").is_ok_and(|v| v == "1" || v == "true");
        Arc::new(Self {
            drum: crate::sim::virtual_drum::VirtualDrum::new(db.clone(), mqtt.clone()),
            db,
            mqtt,
            device: Mutex::new(Device::default()),
            visitors: Mutex::new(std::collections::HashMap::new()),
            enabled: Mutex::new(forced),
            forced,
        })
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

    pub async fn state(&self) -> VirtualState {
        self.state_of(DEVICE_ID)
            .await
            .expect("the shared simulated roaster is always there")
    }

    /// The state of the shared roaster or of a demo visitor's; `None` for a visitor's roaster
    /// that is not running.
    pub async fn state_of(&self, id: &str) -> Option<VirtualState> {
        let enabled = *self.enabled.lock().await;
        let drum = self.drum.state().await;
        let of = |d: &Device| VirtualState {
            drum: drum.clone(),
            enabled,
            device_id: id.to_string(),
            loaded: d.model.loaded,
            bt: d.model.bt,
            et: d.model.et,
            heater_applied: d.heater_applied(),
            fan: d.fan,
        };
        if id == DEVICE_ID {
            return Some(of(&*self.device.lock().await));
        }
        self.visitors.lock().await.get(id).map(of)
    }

    /// A demo visitor's own simulated roaster starts (a fresh one, unless it runs already).
    pub async fn add_visitor(&self, id: &str) {
        let body = {
            let mut v = self.visitors.lock().await;
            v.entry(id.to_string())
                .or_default()
                .status_as(id, true)
                .to_string()
        };
        self.publish(&roastwarden_core::status_topic(id), true, body)
            .await;
        tracing::info!(roaster = %id, "demo: a visitor's simulated roaster started");
    }

    /// A demo visitor's roaster stops; its status says offline.
    pub async fn remove_visitor(&self, id: &str) {
        let Some(d) = self.visitors.lock().await.remove(id) else {
            return;
        };
        let body = d.status_as(id, false).to_string();
        self.publish(&roastwarden_core::status_topic(id), true, body)
            .await;
    }

    async fn publish(&self, topic: &str, retain: bool, body: String) {
        let qos = if retain {
            QoS::AtLeastOnce
        } else {
            QoS::AtMostOnce
        };
        if let Err(e) = self.mqtt.publish(topic, qos, retain, body).await {
            tracing::debug!(error = %e, %topic, "virtual roaster: not sent");
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
            tracing::info!("virtual roaster on ({DEVICE_ID})");
        } else {
            tracing::info!("virtual roaster off");
        }
        self.publish_status(on).await;
    }

    pub async fn charge(&self) {
        self.device.lock().await.model.charge();
    }

    pub async fn dump(&self) {
        self.device.lock().await.model.dump();
    }

    /// Beans into a demo visitor's roaster; false when it is not running.
    pub async fn charge_visitor(&self, id: &str) -> bool {
        self.visitors
            .lock()
            .await
            .get_mut(id)
            .map(|d| d.model.charge())
            .is_some()
    }

    /// Beans out of a demo visitor's roaster; false when it is not running.
    pub async fn dump_visitor(&self, id: &str) -> bool {
        self.visitors
            .lock()
            .await
            .get_mut(id)
            .map(|d| d.model.dump())
            .is_some()
    }

    async fn publish_status(&self, online: bool) {
        let body = self.device.lock().await.status(online).to_string();
        let topic = roastwarden_core::status_topic(DEVICE_ID);
        if let Err(e) = self
            .mqtt
            .publish(&topic, QoS::AtLeastOnce, true, body)
            .await
        {
            tracing::warn!(error = %e, "virtual roaster: could not publish status");
        }
    }

    pub async fn run(self: Arc<Self>) {
        let mut events = self.mqtt.events();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let control = format!("{}/", roastwarden_core::control_root(DEVICE_ID));
        let telemetry_topic = roastwarden_core::telemetry_topic(DEVICE_ID);
        let mut last = Instant::now();
        let mut last_status: Option<Instant> = None;
        let mut checked: Option<Instant> = None;
        loop {
            tokio::select! {
                ev = events.recv() => {
                    let Ok(MqttEvent::Publish { topic, payload }) = ev else { continue };
                    let text = String::from_utf8_lossy(&payload).to_string();
                    if let Some(cmd) = topic.strip_prefix(&control) {
                        if *self.enabled.lock().await {
                            self.device.lock().await.command(cmd, &text, Instant::now());
                        }
                    } else if let Some((id, cmd)) = visitor_control(&topic) {
                        // A visitor's roaster runs while its visitor is there, whatever the
                        // shared one's switch
                        if let Some(d) = self.visitors.lock().await.get_mut(id) {
                            d.command(cmd, &text, Instant::now());
                        }
                    }
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
                    let status_due = last_status.is_none_or(|t| t.elapsed() >= STATUS_EVERY);
                    if status_due {
                        last_status = Some(now);
                    }
                    if *self.enabled.lock().await {
                        if status_due {
                            self.publish_status(true).await;
                        }
                        let doc = self.device.lock().await.tick(dt, now);
                        if let Err(e) = self.mqtt.publish(&telemetry_topic, QoS::AtMostOnce, false, doc.to_string()).await {
                            tracing::debug!(error = %e, "virtual roaster: telemetry not sent");
                        }
                    }
                    // The demo visitors' own roasters, one sample each
                    let out: Vec<(String, String, Option<String>)> = self
                        .visitors
                        .lock()
                        .await
                        .iter_mut()
                        .map(|(id, d)| {
                            let doc = d.tick(dt, now).to_string();
                            let status = status_due.then(|| d.status_as(id, true).to_string());
                            (id.clone(), doc, status)
                        })
                        .collect();
                    for (id, doc, status) in out {
                        if let Some(st) = status {
                            self.publish(&roastwarden_core::status_topic(&id), true, st).await;
                        }
                        self.publish(&roastwarden_core::telemetry_topic(&id), false, doc).await;
                    }
                }
            }
        }
    }
}

/// `roaster/<visitor's roaster>/control/<command>`: the roaster and the command.
fn visitor_control(topic: &str) -> Option<(&str, &str)> {
    let (id, cmd) = topic.strip_prefix("roaster/")?.split_once("/control/")?;
    crate::ops::demo::is_visitor_roaster(id).then_some((id, cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_visitors_roaster_takes_its_own_commands() {
        assert_eq!(
            visitor_control("roaster/virtual_3fa9c1d2/control/heater"),
            Some(("virtual_3fa9c1d2", "heater"))
        );
        assert_eq!(
            visitor_control("roaster/esp32_roaster_01/control/heater"),
            None
        );
        assert_eq!(visitor_control("roaster/virtual_3fa9c1d2/telemetry"), None);
        assert!(is_simulated("virtual_roaster") && is_simulated("virtual_3fa9c1d2"));
        assert!(!is_simulated("virtual_drum") && !is_simulated("esp32_roaster_01"));
    }

    fn settle(d: &mut Device, secs: usize) {
        let now = Instant::now();
        for _ in 0..secs {
            d.tick(1.0, now);
        }
    }

    #[test]
    fn steady_state_matches_the_autotunes() {
        // 88 % at 100 % fan holds ~165 °C; 78 % at 75 % fan holds ~190 °C;
        // full heat at full fan tops out in the 180s.
        let bt = |h: f64, f: f64| {
            let et = Model::et_steady(h, f);
            et - PROBE_OFFSET_C * (et - AMBIENT_C) / AIR_GAIN_C
        };
        assert!((bt(88.0, 100.0) - 165.0).abs() < 3.0, "{}", bt(88.0, 100.0));
        assert!((bt(78.0, 75.0) - 190.0).abs() < 3.0, "{}", bt(78.0, 75.0));
        assert!((180.0..190.0).contains(&bt(100.0, 100.0)));
        assert_eq!(Model::et_steady(100.0, 0.0), AMBIENT_C);
    }

    #[test]
    fn manual_heat_warms_and_the_interlock_and_enable_cut_it() {
        let mut d = Device::default();
        let now = Instant::now();
        d.command("fan", "100", now);
        d.command("heater", "60", now);
        settle(&mut d, 30);
        assert!(d.model.bt < 30.0, "heater not enabled yet");
        d.command("heater_enable", "1", now);
        settle(&mut d, 5);
        // As the firmware: a heater % set while heater power was off is lost.
        assert_eq!(
            d.heater_applied(),
            0.0,
            "manual % survived heater power off"
        );
        d.command("heater", "60", now);
        settle(&mut d, 120);
        assert!(d.model.bt > 100.0, "{}", d.model.bt);
        d.command("fan", "30", now);
        assert_eq!(d.heater_applied(), 0.0, "fan interlock");
    }

    #[test]
    fn auto_mode_holds_the_setpoint_with_the_firmware_gains() {
        let mut d = Device::default();
        let now = Instant::now();
        for (c, p) in [
            ("fan", "100"),
            ("heater_enable", "1"),
            ("setpoint", "160"),
            ("mode", "auto"),
        ] {
            d.command(c, p, now);
        }
        d.model.charge();
        settle(&mut d, 900);
        assert!((d.model.bt - 160.0).abs() < 2.0, "BT {}", d.model.bt);
        let doc = d.tick(1.0, now);
        assert_eq!(doc["controlMode"], 1);
        assert!(doc["heater"].as_f64().unwrap() > 50.0);
        // Above the heat limit it cannot get there at 100 % fan...
        d.command("setpoint", "200", now);
        settle(&mut d, 600);
        assert!(
            d.model.bt < 190.0 && d.heater_applied() > 99.0,
            "BT {}",
            d.model.bt
        );
        // ...but can at 75 %.
        d.command("fan", "75", now);
        settle(&mut d, 900);
        assert!((d.model.bt - 200.0).abs() < 2.5, "BT {}", d.model.bt);
    }

    #[test]
    fn lease_expiry_and_emergency_stop_reach_the_safe_state() {
        let mut d = Device::default();
        let t0 = Instant::now();
        for (c, p) in [
            ("fan", "100"),
            ("heater_enable", "1"),
            ("mode", "auto"),
            ("lease", "10"),
        ] {
            d.command(c, p, t0);
        }
        d.tick(1.0, t0 + Duration::from_millis(50));
        assert!(!d.heater_enabled && !d.auto, "lease ran out");
        d.command("heater_enable", "1", t0);
        d.command("emergency_stop", "1", t0);
        assert!(!d.heater_enabled);
        assert_eq!(d.fan, 100.0);
        d.command("pid", r#"{"kp":5,"ki":0.1,"kd":20,"persist":false}"#, t0);
        assert_eq!(d.gains, (5.0, 0.1, 20.0));
        d.command("fan", "150", t0);
        assert_eq!(d.fan, 100.0, "out of range ignored");
        assert_eq!(d.status(true)["caps"][0], "pid");
    }
}
