//! Fan assist: valve-position control of the fan (`docs/fan-assist.md`).
//!
//! The roaster is heat-limited: at full airflow the heater cannot carry BT
//! to the top of a roast, and the operator lowers the fan by hand to get
//! there. Fan assist does that for them. While the BT loop's heater output
//! sits above `high_heater_pct` the fan is trimmed down; once the heater has
//! room again (below `low_heater_pct`) the trim is given back. In between
//! nothing moves, so the two loops do not fight.
//!
//! The fan the operator (or the profile) asks for is the ceiling; the assist
//! only ever takes air away, and never below `min_fan_pct`. It arms above
//! `arm_bt_c`, so the heater pinned at 100 % right after CHARGE does not pull
//! the fan down while the beans are drying.
//!
//! [`FanAssist`] is the pure controller. A replay runs it inside its engine
//! (the profile fan is the ceiling); [`FanAssistService`] runs it for a roast
//! on the roaster PID without a replay (the operator's fan is the ceiling).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::analysis::telemetry::TelemetryService;
use crate::control::schedule::GainScheduler;
use crate::replay::ReplayService;
use crate::transports::transport::{SendOptions, TransportRegistry};
use roastwarden_core::DeviceCommand;

/// Heater filter time constant: the 1 Hz time-proportioned heater and PID
/// noise must not move the fan.
const FILTER_TAU_S: f64 = 10.0;
/// Trim rate per % of heater beyond the band, %/min.
const GAIN_PER_MIN: f64 = 1.0;
/// BT this far behind the target (°C) before the lag adds to the rate.
const LAG_DEADBAND_C: f64 = 1.0;
/// The assist disarms this far below `arm_bt_c`.
const ARM_HYSTERESIS_C: f64 = 10.0;
/// A sample older than this is stale: the assist holds.
const STALE_MS: u64 = 5_000;
const TICK: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default)]
pub struct FanAssistConfig {
    pub enabled: bool,
    /// The assist never trims the fan below this, %.
    pub min_fan_pct: f64,
    /// The assist acts only once BT reaches this, °C.
    pub arm_bt_c: f64,
    /// Heater output above this lowers the fan, %.
    pub high_heater_pct: f64,
    /// Heater output below this gives fan back, %.
    pub low_heater_pct: f64,
    /// Fastest fan reduction, %/min.
    pub max_down_pct_per_min: f64,
    /// Fastest fan recovery, %/min.
    pub max_up_pct_per_min: f64,
    /// Extra trim rate per °C BT is behind (or, giving back, ahead of) its
    /// target, beyond a 1 °C deadband, %/min per °C. 0 turns it off.
    pub lag_gain_pct_per_min_per_c: f64,
}

impl Default for FanAssistConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_fan_pct: 70.0,
            arm_bt_c: 150.0,
            high_heater_pct: 92.0,
            low_heater_pct: 80.0,
            max_down_pct_per_min: 15.0,
            max_up_pct_per_min: 6.0,
            lag_gain_pct_per_min_per_c: 2.0,
        }
    }
}

impl FanAssistConfig {
    /// Check the settings; the lowest fan must be at or above the roaster's fan floor while
    /// heating (`fan_floor_pct`: 45 % on the fluid bed, whose firmware cuts the heater below
    /// 40 % fan; docs/roaster-setup.md, "The interlock").
    pub fn validate(&self, fan_floor_pct: f64) -> Result<(), String> {
        let floor = fan_floor_pct.clamp(0.0, 100.0);
        let all = [
            self.min_fan_pct,
            self.arm_bt_c,
            self.high_heater_pct,
            self.low_heater_pct,
            self.max_down_pct_per_min,
            self.max_up_pct_per_min,
        ];
        if all.iter().any(|v| !v.is_finite()) {
            return Err("values must be numbers".into());
        }
        if !(floor..=100.0).contains(&self.min_fan_pct) {
            return Err(format!(
                "min_fan_pct must be {floor:.0}..100 (the roaster's fan floor while heating)"
            ));
        }
        if !(0.0..=250.0).contains(&self.arm_bt_c) {
            return Err("arm_bt_c must be 0..250 °C".into());
        }
        if !(self.low_heater_pct >= 10.0
            && self.high_heater_pct <= 100.0
            && self.high_heater_pct - self.low_heater_pct >= 2.0)
        {
            return Err(
                "heater band: 10 <= low_heater_pct, high_heater_pct <= 100, and high at least 2 above low"
                    .into(),
            );
        }
        if !self.lag_gain_pct_per_min_per_c.is_finite()
            || !(0.0..=10.0).contains(&self.lag_gain_pct_per_min_per_c)
        {
            return Err("lag_gain_pct_per_min_per_c must be 0..10".into());
        }
        for r in [self.max_down_pct_per_min, self.max_up_pct_per_min] {
            if !(0.5..=60.0).contains(&r) {
                return Err("rates must be 0.5..60 %/min".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistStatus {
    /// BT is below the arming temperature.
    Waiting,
    /// No heater output to work from (stale telemetry).
    NoData,
    /// No trim: the fan is at the ceiling.
    AtCeiling,
    /// Trimmed, heater inside the band: holding.
    Holding,
    Lowering,
    Raising,
    /// Trimmed as far as the floor allows; the heater is still pinned.
    AtFloor,
}

impl AssistStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AssistStatus::Waiting => "waiting",
            AssistStatus::NoData => "no_data",
            AssistStatus::AtCeiling => "at_ceiling",
            AssistStatus::Holding => "holding",
            AssistStatus::Lowering => "lowering",
            AssistStatus::Raising => "raising",
            AssistStatus::AtFloor => "at_floor",
        }
    }
}

/// What the assist is doing, for dashboards.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, utoipa::ToSchema)]
pub struct AssistView {
    pub status: AssistStatus,
    /// Fan the operator or profile asks for, %.
    pub ceiling: f64,
    pub floor: f64,
    /// Fan taken away, %.
    pub trim: f64,
    /// Fan the assist commands, %.
    pub fan: f64,
    /// Filtered heater output the assist works from, %.
    pub heater: Option<f64>,
    /// BT behind its target, °C (negative: ahead).
    pub lag: Option<f64>,
    pub armed: bool,
}

/// The controller. `step` once per tick with the ceiling, the heater output
/// of the BT loop and BT; it returns the fan to command.
#[derive(Debug, Clone)]
pub struct FanAssist {
    trim: f64,
    heater: Option<f64>,
    lag: Option<f64>,
    armed: bool,
    status: AssistStatus,
    ceiling: f64,
    floor: f64,
}

impl Default for FanAssist {
    fn default() -> Self {
        Self {
            trim: 0.0,
            heater: None,
            lag: None,
            armed: false,
            status: AssistStatus::Waiting,
            ceiling: 100.0,
            floor: FanAssistConfig::default().min_fan_pct,
        }
    }
}

impl FanAssist {
    /// Give all the trim back at once (operator set the fan, assist off).
    pub fn reset(&mut self) {
        self.trim = 0.0;
        self.heater = None;
    }

    pub fn trim(&self) -> f64 {
        self.trim
    }

    pub fn status(&self) -> AssistStatus {
        self.status
    }

    pub fn view(&self) -> AssistView {
        AssistView {
            status: self.status,
            ceiling: self.ceiling,
            floor: self.floor,
            trim: self.trim,
            fan: self.ceiling - self.trim,
            heater: self.heater,
            lag: self.lag,
            armed: self.armed,
        }
    }

    /// `target` is the BT the loop is chasing now (no lookahead): BT behind
    /// it speeds the trim up, BT ahead of it stops the trim.
    pub fn step(
        &mut self,
        cfg: &FanAssistConfig,
        ceiling: f64,
        heater: Option<f64>,
        bt: Option<f64>,
        target: Option<f64>,
        dt: f64,
    ) -> f64 {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 5.0)
        } else {
            0.0
        };
        let ceiling = ceiling.clamp(0.0, 100.0);
        self.ceiling = ceiling;
        self.floor = cfg.min_fan_pct;

        if let Some(h) = heater
            .filter(|h| h.is_finite())
            .map(|h| h.clamp(0.0, 100.0))
        {
            let a = dt / (FILTER_TAU_S + dt);
            self.heater = Some(match self.heater {
                Some(p) => p + a * (h - p),
                None => h,
            });
        } else {
            self.heater = None;
        }
        self.lag = match (target, bt) {
            (Some(t), Some(b)) if t.is_finite() && b.is_finite() => Some(t - b),
            _ => None,
        };
        // Beyond the deadband, °C behind (+) or ahead (-).
        let behind = self
            .lag
            .map_or(0.0, |l| (l.abs() - LAG_DEADBAND_C).max(0.0) * l.signum());
        let lag_rate = |c: f64| cfg.lag_gain_pct_per_min_per_c * c.max(0.0);
        match bt {
            Some(b) if b >= cfg.arm_bt_c => self.armed = true,
            Some(b) if b < cfg.arm_bt_c - ARM_HYSTERESIS_C => self.armed = false,
            _ => {}
        }

        let span = (ceiling - cfg.min_fan_pct).max(0.0);
        let per_s = |rate: f64| rate * dt / 60.0;
        self.status = if self.armed {
            match self.heater {
                None => AssistStatus::NoData,
                // Heater pinned but BT ahead of the target: the loop is about
                // to back off by itself; taking air away would overshoot.
                Some(u) if u > cfg.high_heater_pct && behind < 0.0 => AssistStatus::Holding,
                Some(u) if u > cfg.high_heater_pct => {
                    let rate = (GAIN_PER_MIN * (u - cfg.high_heater_pct) + lag_rate(behind))
                        .min(cfg.max_down_pct_per_min);
                    self.trim += per_s(rate);
                    if self.trim >= span {
                        AssistStatus::AtFloor
                    } else {
                        AssistStatus::Lowering
                    }
                }
                Some(u) if u < cfg.low_heater_pct && self.trim > 0.0 => {
                    let rate = (GAIN_PER_MIN * (cfg.low_heater_pct - u) + lag_rate(-behind))
                        .min(cfg.max_up_pct_per_min);
                    self.trim -= per_s(rate);
                    AssistStatus::Raising
                }
                Some(_) => AssistStatus::Holding,
            }
        } else {
            self.trim -= per_s(cfg.max_up_pct_per_min);
            AssistStatus::Waiting
        };
        self.trim = self.trim.clamp(0.0, span);
        if self.trim <= 0.0 && matches!(self.status, AssistStatus::Holding | AssistStatus::Raising)
        {
            self.status = AssistStatus::AtCeiling;
        }
        ceiling - self.trim
    }
}

// ------------------------------------------------------------ settings --

pub fn setting_key(device_id: &str) -> String {
    format!("fan_assist:{device_id}")
}

pub async fn load(db: &SqlitePool, device_id: &str) -> FanAssistConfig {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(setting_key(device_id))
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    raw.and_then(|r| serde_json::from_str::<FanAssistConfig>(&r).ok())
        .filter(|c| c.validate(0.0).is_ok())
        .unwrap_or_default()
}

pub async fn save(
    db: &SqlitePool,
    device_id: &str,
    c: &FanAssistConfig,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(setting_key(device_id))
    .bind(serde_json::to_string(c).unwrap_or_default())
    .execute(db)
    .await
    .map(|_| ())
}

// ------------------------------------------------------------- service --

/// Why the standalone assist is or is not acting on a device.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DeviceAssistState {
    /// Who runs the assist now: `replay` or `manual` (the standalone loop).
    pub source: Option<&'static str>,
    /// The assist is driving the fan.
    pub active: bool,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<AssistView>,
}

#[derive(Default)]
struct DeviceRt {
    assist: FanAssist,
    /// The operator's fan, %; the reported fan when the assist first arms.
    ceiling: Option<f64>,
    last_sent: Option<f64>,
    last_tick: Option<Instant>,
    active: bool,
    reason: String,
}

/// Runs the assist for roasts on the roaster PID without a replay.
pub struct FanAssistService {
    db: SqlitePool,
    transports: Arc<TransportRegistry>,
    telemetry: TelemetryService,
    scheduler: Arc<GainScheduler>,
    replay: Arc<ReplayService>,
    configs: Mutex<HashMap<String, FanAssistConfig>>,
    devices: Mutex<HashMap<String, DeviceRt>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl FanAssistService {
    pub fn new(
        db: SqlitePool,
        transports: Arc<TransportRegistry>,
        telemetry: TelemetryService,
        scheduler: Arc<GainScheduler>,
        replay: Arc<ReplayService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            transports,
            telemetry,
            scheduler,
            replay,
            configs: Mutex::new(HashMap::new()),
            devices: Mutex::new(HashMap::new()),
        })
    }

    /// Load every device's saved config.
    pub async fn load_all(&self) {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM settings WHERE key LIKE 'fan_assist:%'")
                .fetch_all(&self.db)
                .await
                .unwrap_or_default();
        let mut cfgs = self.configs.lock().await;
        for (k, v) in rows {
            let Some(id) = k.strip_prefix("fan_assist:") else {
                continue;
            };
            if let Some(c) = serde_json::from_str::<FanAssistConfig>(&v)
                .ok()
                .filter(|c| c.validate(0.0).is_ok())
            {
                cfgs.insert(id.to_string(), c);
            }
        }
    }

    pub async fn config(&self, device_id: &str) -> FanAssistConfig {
        if let Some(c) = self.configs.lock().await.get(device_id) {
            return c.clone();
        }
        load(&self.db, device_id).await
    }

    /// Save a config; a running replay picks it up, and switching the assist
    /// off hands the fan back to the ceiling.
    pub async fn set_config(&self, device_id: &str, c: FanAssistConfig) -> Result<(), String> {
        c.validate(self.transports.limits(device_id).await.fan_floor_pct)?;
        save(&self.db, device_id, &c)
            .await
            .map_err(|e| format!("database error: {e}"))?;
        self.configs
            .lock()
            .await
            .insert(device_id.to_string(), c.clone());
        self.replay.set_fan_assist(device_id, c.clone()).await;
        if !c.enabled {
            self.deactivate(device_id, "fan assist is off", true).await;
        }
        Ok(())
    }

    /// The operator set the fan: that is the new ceiling, with no trim.
    pub async fn operator_fan(&self, device_id: &str, pct: f64) {
        let mut devs = self.devices.lock().await;
        let rt = devs.entry(device_id.to_string()).or_default();
        rt.ceiling = Some(pct);
        rt.last_sent = Some(pct);
        rt.assist.reset();
    }

    /// What the assist is doing on a device: the replay's assist while a
    /// replay drives the roaster, else the standalone loop's.
    pub async fn state(&self, device_id: &str) -> DeviceAssistState {
        if let Some(view) = self.replay.fan_assist_view(device_id).await {
            return DeviceAssistState {
                source: Some("replay"),
                active: view.is_some(),
                reason: if view.is_some() {
                    "the replay runs the assist; the profile fan is the ceiling".into()
                } else {
                    "a replay is driving the roaster without fan assist".into()
                },
                view,
            };
        }
        let devs = self.devices.lock().await;
        match devs.get(device_id) {
            Some(rt) => DeviceAssistState {
                source: rt.active.then_some("manual"),
                active: rt.active,
                reason: rt.reason.clone(),
                view: rt.active.then(|| rt.assist.view()),
            },
            None => DeviceAssistState {
                source: None,
                active: false,
                reason: "idle".into(),
                view: None,
            },
        }
    }

    /// Stop acting on a device. With `restore`, a trimmed fan goes back to
    /// the ceiling.
    async fn deactivate(&self, device_id: &str, reason: &str, restore: bool) {
        let send = {
            let mut devs = self.devices.lock().await;
            let Some(rt) = devs.get_mut(device_id) else {
                return;
            };
            let trimmed = rt.active && rt.assist.trim() >= 0.5;
            rt.active = false;
            rt.reason = reason.to_string();
            rt.assist.reset();
            rt.last_tick = None;
            match (restore && trimmed, rt.ceiling) {
                (true, Some(c)) => {
                    rt.last_sent = Some(c);
                    Some(c)
                }
                _ => None,
            }
        };
        if let Some(c) = send {
            self.send_fan(device_id, c).await;
        }
    }

    async fn send_fan(&self, device_id: &str, pct: f64) -> bool {
        let percent = pct.round().clamp(0.0, 100.0);
        match self
            .transports
            .send(
                device_id,
                &DeviceCommand::Fan { percent },
                SendOptions::default(),
            )
            .await
        {
            Ok(_) => {
                self.scheduler.fan_changed(device_id, percent).await;
                true
            }
            Err(e) => {
                tracing::warn!(device_id, error = %e, "fan assist could not send the fan");
                false
            }
        }
    }

    pub async fn run(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let enabled: Vec<(String, FanAssistConfig)> = self
                .configs
                .lock()
                .await
                .iter()
                .filter(|(_, c)| c.enabled)
                .map(|(k, c)| (k.clone(), c.clone()))
                .collect();
            for (device_id, cfg) in enabled {
                self.tick(&device_id, &cfg).await;
            }
        }
    }

    async fn tick(&self, device_id: &str, cfg: &FanAssistConfig) {
        // Fan assist acts in auto mode, which needs full control (docs/roaster-setup.md).
        let limits = self.transports.limits(device_id).await;
        if limits.control != crate::roasters::setup::Control::Full {
            self.deactivate(device_id, "the roaster's control is below full", false)
                .await;
            return;
        }
        if !limits.has_heater_and_fan() {
            self.deactivate(device_id, "the roaster has no fan output", false)
                .await;
            return;
        }
        if self.replay.drives_device(device_id).await {
            // The replay owns the fan; after it, the fan it left is the ceiling.
            let mut devs = self.devices.lock().await;
            let rt = devs.entry(device_id.to_string()).or_default();
            rt.active = false;
            rt.reason = "a replay is driving the roaster".into();
            rt.ceiling = None;
            rt.last_sent = None;
            rt.last_tick = None;
            rt.assist.reset();
            return;
        }
        let sample = self
            .telemetry
            .latest_sample(device_id)
            .filter(|s| now_ms().saturating_sub(s.ts_ms) <= STALE_MS);
        let Some(sample) = sample else {
            let mut devs = self.devices.lock().await;
            let rt = devs.entry(device_id.to_string()).or_default();
            rt.reason = "no fresh telemetry: holding".into();
            rt.last_tick = None;
            return;
        };
        if sample.heater_enable == Some(false) {
            // Safe state or operator: leave the fan to the roaster.
            self.deactivate(device_id, "the heater is off", false).await;
            return;
        }
        if sample.mode != Some(1) {
            self.deactivate(
                device_id,
                "the roaster is in manual mode (the assist works with its PID in auto)",
                true,
            )
            .await;
            return;
        }

        let send = {
            let mut devs = self.devices.lock().await;
            let rt = devs.entry(device_id.to_string()).or_default();
            let Some(ceiling) = rt.ceiling.or(sample.fan) else {
                rt.reason = "the roaster does not report its fan".into();
                return;
            };
            rt.ceiling = Some(ceiling);
            let dt = rt.last_tick.map_or(0.0, |t| t.elapsed().as_secs_f64());
            rt.last_tick = Some(Instant::now());
            // What the roaster applied (after its RoR limiter), else what it commands.
            let heater = sample
                .extras
                .get("heaterApplied")
                .copied()
                .or(sample.heater);
            // The roaster's own setpoint is the target without a replay.
            let fan = rt
                .assist
                .step(cfg, ceiling, heater, sample.bt, sample.setpoint, dt);
            rt.active = true;
            rt.reason = format!("assisting: {}", rt.assist.status().as_str());
            let changed = rt.last_sent.is_none_or(|p| p.round() != fan.round());
            // Before any trim the fan is the operator's: nothing to send.
            let untouched = rt.last_sent.is_none() && rt.assist.trim() < 0.5;
            if changed && !untouched {
                rt.last_sent = Some(fan);
                Some(fan)
            } else {
                if rt.last_sent.is_none() {
                    rt.last_sent = Some(ceiling);
                }
                None
            }
        };
        if let Some(fan) = send {
            if self.send_fan(device_id, fan).await {
                tracing::info!(device_id, fan, "fan assist moved the fan");
            } else if let Some(rt) = self.devices.lock().await.get_mut(device_id) {
                rt.last_sent = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FanAssistConfig {
        FanAssistConfig {
            enabled: true,
            ..FanAssistConfig::default()
        }
    }

    /// Run `secs` one-second steps at a fixed heater and BT.
    fn run(
        a: &mut FanAssist,
        c: &FanAssistConfig,
        ceiling: f64,
        heater: f64,
        bt: f64,
        secs: usize,
    ) -> f64 {
        let mut fan = ceiling;
        for _ in 0..secs {
            fan = a.step(c, ceiling, Some(heater), Some(bt), None, 1.0);
        }
        fan
    }

    #[test]
    fn defaults_validate_and_floor_respects_the_interlock() {
        assert!(FanAssistConfig::default().validate(45.0).is_ok());
        assert_eq!(FanAssistConfig::default().min_fan_pct, 70.0);
        let bad = FanAssistConfig {
            min_fan_pct: 40.0,
            ..cfg()
        };
        assert!(bad.validate(45.0).is_err());
        // A roaster with no floor takes it.
        assert!(bad.validate(0.0).is_ok());
        let band = FanAssistConfig {
            low_heater_pct: 91.0,
            high_heater_pct: 92.0,
            ..cfg()
        };
        assert!(band.validate(45.0).is_err());
        let rate = FanAssistConfig {
            max_up_pct_per_min: 0.0,
            ..cfg()
        };
        assert!(rate.validate(45.0).is_err());
    }

    #[test]
    fn waits_below_the_arming_temperature() {
        let c = cfg();
        let mut a = FanAssist::default();
        // Pinned heater right after CHARGE: nothing moves.
        assert_eq!(run(&mut a, &c, 100.0, 100.0, 120.0, 120), 100.0);
        assert_eq!(a.status(), AssistStatus::Waiting);
    }

    #[test]
    fn pinned_heater_lowers_the_fan_at_the_rate_limit_down_to_the_floor() {
        let c = cfg();
        let mut a = FanAssist::default();
        // 100 % heater, 8 % over the band, no target: 8 %/min.
        let fan = run(&mut a, &c, 100.0, 100.0, 180.0, 60);
        assert!((fan - 92.0).abs() < 0.5, "fan {fan}");
        assert_eq!(a.status(), AssistStatus::Lowering);
        // Never below the 70 % floor.
        let fan = run(&mut a, &c, 100.0, 100.0, 180.0, 600);
        assert_eq!(fan, 70.0);
        assert_eq!(a.status(), AssistStatus::AtFloor);
    }

    #[test]
    fn heater_in_the_band_holds_and_headroom_gives_the_fan_back() {
        let c = cfg();
        let mut a = FanAssist::default();
        run(&mut a, &c, 100.0, 100.0, 180.0, 120);
        let trimmed = a.view().fan;
        // Heater settles at 85 %: hold (after the filter catches up).
        let fan = run(&mut a, &c, 100.0, 85.0, 180.0, 120);
        assert!(fan <= trimmed && trimmed - fan < 1.0, "{fan} vs {trimmed}");
        let trimmed = fan;
        assert_eq!(a.status(), AssistStatus::Holding);
        // Heater at 60 %: back up at most 6 %/min, all the way to the ceiling.
        let fan = run(&mut a, &c, 100.0, 60.0, 180.0, 60);
        assert!((fan - (trimmed + 6.0)).abs() < 0.6, "{fan}");
        let fan = run(&mut a, &c, 100.0, 60.0, 180.0, 600);
        assert_eq!(fan, 100.0);
        assert_eq!(a.status(), AssistStatus::AtCeiling);
    }

    #[test]
    fn trim_follows_a_moving_ceiling_and_never_raises_above_it() {
        let c = cfg();
        let mut a = FanAssist::default();
        run(&mut a, &c, 100.0, 100.0, 180.0, 90);
        let trim = a.trim();
        // The profile drops the fan to 80 %: the trim carries over, clamped to the floor.
        let fan = a.step(&c, 80.0, Some(85.0), Some(180.0), None, 1.0);
        assert!((fan - (80.0 - trim).max(70.0)).abs() < 0.1, "{fan}");
        // A ceiling below the floor is the operator's call: no trim at all.
        let fan = a.step(&c, 65.0, Some(100.0), Some(180.0), None, 1.0);
        assert_eq!(fan, 65.0);
    }

    #[test]
    fn filter_ignores_a_short_heater_spike_and_stale_data_holds() {
        let c = cfg();
        let mut a = FanAssist::default();
        run(&mut a, &c, 100.0, 80.0, 180.0, 60);
        // Three seconds at 100 % barely move the filtered value.
        let fan = run(&mut a, &c, 100.0, 100.0, 180.0, 3);
        assert!(fan > 99.9, "{fan}");
        let fan = a.step(&c, 100.0, None, Some(180.0), None, 1.0);
        assert!(fan > 99.9);
        assert_eq!(a.status(), AssistStatus::NoData);
    }

    #[test]
    fn disarming_releases_the_trim_slowly() {
        let c = cfg();
        let mut a = FanAssist::default();
        run(&mut a, &c, 100.0, 100.0, 180.0, 120);
        let trimmed = a.view().fan;
        // Next batch: BT drops at CHARGE, the trim returns at the up rate.
        let fan = run(&mut a, &c, 100.0, 100.0, 100.0, 60);
        assert!((fan - (trimmed + 6.0)).abs() < 0.3, "{fan}");
        assert_eq!(a.status(), AssistStatus::Waiting);
    }

    #[test]
    fn bt_behind_the_target_speeds_the_trim_and_ahead_stops_it() {
        let c = cfg();
        let mut a = FanAssist::default();
        // Heater 96 % (4 over) and BT 6 °C behind: 4 + 2·5 = 14 %/min.
        for _ in 0..60 {
            a.step(&c, 100.0, Some(96.0), Some(176.0), Some(182.0), 1.0);
        }
        assert!((a.trim() - 14.0).abs() < 0.5, "trim {}", a.trim());
        assert_eq!(a.view().lag, Some(6.0));
        // Far behind: capped at max_down (15 %/min).
        let mut b = FanAssist::default();
        for _ in 0..60 {
            b.step(&c, 100.0, Some(100.0), Some(170.0), Some(190.0), 1.0);
        }
        assert!((b.trim() - 15.0).abs() < 0.5, "trim {}", b.trim());
        // Heater pinned but BT 3 °C ahead: hold.
        let before = b.trim();
        b.step(&c, 100.0, Some(100.0), Some(193.0), Some(190.0), 1.0);
        assert_eq!(b.status(), AssistStatus::Holding);
        assert_eq!(b.trim(), before);
        // Heater with room and BT well ahead: give back faster than headroom alone.
        let mut d = FanAssist::default();
        for _ in 0..60 {
            d.step(&c, 100.0, Some(100.0), Some(180.0), None, 1.0);
        }
        let mut no_lag = d.clone();
        for _ in 0..60 {
            d.step(&c, 100.0, Some(70.0), Some(190.0), Some(187.0), 1.0);
            no_lag.step(&c, 100.0, Some(70.0), Some(190.0), None, 1.0);
        }
        assert!(
            d.trim() < no_lag.trim() - 0.5,
            "{} vs {}",
            d.trim(),
            no_lag.trim()
        );
    }

    #[tokio::test]
    async fn config_round_trips_through_settings() {
        let db = crate::db::open_memory().await;
        assert_eq!(load(&db, "r1").await, FanAssistConfig::default());
        let c = FanAssistConfig {
            enabled: true,
            min_fan_pct: 72.0,
            ..FanAssistConfig::default()
        };
        save(&db, "r1", &c).await.unwrap();
        assert_eq!(load(&db, "r1").await, c);
    }
}
