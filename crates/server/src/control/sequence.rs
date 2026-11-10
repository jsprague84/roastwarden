//! Roast sequence (`docs/roast-sequence.md`): what the server does to the roaster when a
//! roast's CHARGE or DROP is marked, from any screen, an alarm or the auto events.
//!
//! - CHARGE: heater power on, the fan to `charge_fan_pct`, and Profile drive started live
//!   when the roast has a profile (it then takes the fan from the profile).
//! - DROP: the drive stops, heater power off, the fan to `cool_fan_pct`; once BT is below
//!   `cool_end_bt_c` (BT and ET) the fan goes off and COOL END is marked.
//! - Cancel (`POST /api/sessions/{id}/cancel`): the drive stops, heater power off, cooling
//!   if the chamber is hot, and the roast is deleted.
//!
//! - Auto DROP (`docs/replay.md`, "Auto DROP"): when a drive's countdown starts, a notice
//!   (as an alarm: the Roast page, phones, the channels); when it ends, DROP is marked, and
//!   DROP's step runs as for any mark.
//!
//! Each step is per roaster and can be switched off. It reacts to landmark changes through
//! the alarm service's `session_events` broadcast, so a mark from a button, an alarm action
//! or an auto event all count; only marks made in the last minute act, so a restart does not
//! replay old ones.

use crate::roasters::setup::Control;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::{Mutex, broadcast};

use crate::analysis::alarms::{ActionDeps, AlarmEvent, AlarmService};
use crate::analysis::telemetry::TelemetryService;
use crate::replay::auto_drop::DropState;
use crate::replay::engine::Channel;
use crate::replay::{ReplayState, StartReplay};
use crate::roasts::types::{CreateRoastEventRequest, RoastEventType, SessionStatus};
use crate::transports::transport::SendOptions;
use roastwarden_core::{ControlMode, DeviceCommand};

/// Marks older than this do nothing (a restart, a reload of old events).
const ACT_WITHIN_SECS: i64 = 60;
/// Cooling gives up (leaving the fan as it is) after this long.
const COOL_TIMEOUT: Duration = Duration::from_mins(20);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(default)]
pub struct SequenceConfig {
    /// At CHARGE: heater power on and the fan to `charge_fan_pct`.
    pub charge_enabled: bool,
    pub charge_fan_pct: f64,
    /// At CHARGE: start Profile drive (live) when the roast has a profile.
    pub charge_drive: bool,
    /// The heater % the drive's PID starts from. The firmware starts its PID bumplessly from
    /// the output it has, which is 0 with heater power just switched on: that loaded an
    /// integral of about -96 % and held the heater low for the first two minutes. The
    /// sequence sets this in manual first, so the PID takes over from it. 0: no start value.
    /// 60 % (about the holding power autotune measured at 100 % fan) tracked the profile
    /// best on the virtual roaster: −13 °C at 0:20 from 0 %, +6.6 °C at 0:40 from 100 %.
    pub charge_heater_pct: f64,
    /// At DROP: stop the drive, heater power off, the fan to `cool_fan_pct`.
    pub drop_enabled: bool,
    pub cool_fan_pct: f64,
    /// Fan off and COOL END once BT and ET are below this, °C; 0 leaves the fan running.
    pub cool_end_bt_c: f64,
}

impl Default for SequenceConfig {
    fn default() -> Self {
        Self {
            charge_enabled: true,
            charge_fan_pct: 100.0,
            charge_drive: true,
            charge_heater_pct: 60.0,
            drop_enabled: true,
            cool_fan_pct: 100.0,
            cool_end_bt_c: 45.0,
        }
    }
}

impl SequenceConfig {
    /// A kind's sequence (docs/roaster-setup.md, "Kinds' defaults"): the fluid bed's acts at
    /// CHARGE and DROP; every other kind's records only.
    pub fn for_kind(kind: crate::roasters::setup::Kind) -> Self {
        if kind.sequence_acts() {
            return Self::default();
        }
        Self {
            charge_enabled: false,
            charge_drive: false,
            drop_enabled: false,
            ..Self::default()
        }
    }

    /// Check the settings; the fans must be at or above the roaster's fan floor while
    /// heating (`fan_floor_pct`, 45 % on the fluid bed: a margin above its 40 % interlock).
    pub fn validate(&self, fan_floor_pct: f64) -> Result<(), String> {
        let floor = fan_floor_pct.clamp(0.0, 100.0);
        for (name, v) in [
            ("charge_fan_pct", self.charge_fan_pct),
            ("cool_fan_pct", self.cool_fan_pct),
        ] {
            if !(v.is_finite() && (floor..=100.0).contains(&v)) {
                return Err(if floor > 0.0 {
                    format!("{name} must be {floor}..100 % (the roaster's fan floor while heating)")
                } else {
                    format!("{name} must be 0..100 %")
                });
            }
        }
        if !(self.charge_heater_pct.is_finite() && (0.0..=100.0).contains(&self.charge_heater_pct))
        {
            return Err("charge_heater_pct must be 0..100 %".into());
        }
        let c = self.cool_end_bt_c;
        if !(c.is_finite() && (c == 0.0 || (25.0..=120.0).contains(&c))) {
            return Err("cool_end_bt_c must be 0 (off) or 25..120 °C".into());
        }
        Ok(())
    }
}

/// One thing the sequence does, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Fan(f64),
    HeaterPower(bool),
    Mode(ControlMode),
    /// Manual heater %.
    Heater(f64),
    StartDrive,
    StopDrive,
    /// Watch BT and end cooling (fan off, COOL END) below the threshold.
    Cool {
        until_bt_c: f64,
        mark: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Charge,
    Drop,
    Cancel,
    /// Resume Profile drive after the roaster cut its heater mid-roast.
    Resume,
}

#[derive(Debug)]
pub enum ResumeError {
    NotFound,
    Invalid(String),
}

/// The steps a roaster's control level lets run (docs/roaster-setup.md): none with `off`
/// (the roast records as usual), the fan and heater ones with `manual` (no mode, no drive),
/// all with `full`.
pub fn for_control(acts: Vec<Act>, control: Control) -> Vec<Act> {
    match control {
        Control::Off => Vec::new(),
        Control::Manual => acts
            .into_iter()
            .filter(|a| {
                matches!(
                    a,
                    Act::Fan(_) | Act::HeaterPower(_) | Act::Heater(_) | Act::Cool { .. }
                )
            })
            .collect(),
        Control::Full => acts,
    }
}

/// The steps a roaster's outputs let run (docs/roaster-setup.md, "Outputs"): the fan and
/// cooling ones need a fan; heater power, heater %, mode and the drive need a heater.
pub fn for_outputs(acts: Vec<Act>, outputs: &crate::roasters::setup::Outputs) -> Vec<Act> {
    acts.into_iter()
        .filter(|a| match a {
            Act::Fan(_) | Act::Cool { .. } => outputs.fan.is_some(),
            Act::HeaterPower(_) | Act::Heater(_) | Act::Mode(_) | Act::StartDrive => {
                outputs.heater.is_some()
            }
            Act::StopDrive => true,
        })
        .collect()
}

/// What a step does. `has_profile`: the roast has one; `drive_running`: a drive (live or
/// ghost) already runs for the roast, which the sequence leaves alone; `hot`: the hotter of
/// the latest BT and ET (a hot-air heater wants air until the chamber is cool, not only
/// the beans).
pub fn plan(
    step: Step,
    cfg: &SequenceConfig,
    has_profile: bool,
    drive_running: bool,
    hot: Option<f64>,
) -> Vec<Act> {
    match step {
        Step::Charge if cfg.charge_enabled => {
            let mut v = Vec::new();
            // The fan before the heater: the firmware heats only above its fan interlock.
            if !drive_running {
                v.push(Act::Fan(cfg.charge_fan_pct));
            }
            let drive = cfg.charge_drive && has_profile && !drive_running;
            if drive && cfg.charge_heater_pct > 0.0 {
                // Heater power first (the firmware drops a manual % set while it is off),
                // then the start output in manual; the drive's switch to auto takes over
                // from it instead of from 0.
                v.push(Act::Mode(ControlMode::Manual));
                v.push(Act::HeaterPower(true));
                v.push(Act::Heater(cfg.charge_heater_pct));
            } else {
                v.push(Act::HeaterPower(true));
            }
            if drive {
                v.push(Act::StartDrive);
            }
            v
        }
        Step::Drop if cfg.drop_enabled => {
            // Manual at 0 %, so switching heater power back on later does not start the PID
            // heating towards the last setpoint.
            let mut v = vec![
                Act::StopDrive,
                Act::HeaterPower(false),
                Act::Mode(ControlMode::Manual),
                Act::Heater(0.0),
                Act::Fan(cfg.cool_fan_pct),
            ];
            if cfg.cool_end_bt_c > 0.0 {
                v.push(Act::Cool {
                    until_bt_c: cfg.cool_end_bt_c,
                    mark: true,
                });
            }
            v
        }
        // The fan stays: the drive sets the profile's within a second (and the cut left it
        // at 100 %, the safe state). Start output as at CHARGE, so the PID does not start
        // from 0 (see Starting the PID).
        Step::Resume => {
            let mut v = Vec::new();
            if cfg.charge_heater_pct > 0.0 {
                v.push(Act::Mode(ControlMode::Manual));
                v.push(Act::HeaterPower(true));
                v.push(Act::Heater(cfg.charge_heater_pct));
            } else {
                v.push(Act::HeaterPower(true));
            }
            v.push(Act::StartDrive);
            v
        }
        Step::Cancel => {
            let mut v = vec![
                Act::StopDrive,
                Act::HeaterPower(false),
                Act::Mode(ControlMode::Manual),
                Act::Heater(0.0),
            ];
            // A hot chamber cools first; a cold one just stops.
            let threshold = if cfg.cool_end_bt_c > 0.0 {
                cfg.cool_end_bt_c
            } else {
                SequenceConfig::default().cool_end_bt_c
            };
            if hot.is_some_and(|t| t > threshold) {
                v.push(Act::Fan(cfg.cool_fan_pct));
                v.push(Act::Cool {
                    until_bt_c: threshold,
                    mark: false,
                });
            } else {
                v.push(Act::Fan(0.0));
            }
            v
        }
        _ => Vec::new(),
    }
}

/// Start settings that repeat a stopped run's, live.
fn resume_request(c: &crate::replay::engine::RunConfig) -> StartReplay {
    use crate::replay::LoopPref;
    use crate::replay::engine::LoopAt;
    StartReplay {
        mode: Some(c.mode),
        trigger: Some(c.trigger),
        align: Some(c.align),
        fan: Some(c.fan),
        ghost: false,
        lookahead_seconds: Some(c.lookahead_seconds),
        adapt_lookahead: Some(c.adapt_lookahead),
        loop_pref: match c.loop_at {
            LoopAt::Device => LoopPref::Device,
            LoopAt::Server => LoopPref::Server,
            LoopAt::Open => LoopPref::Auto,
        },
        fan_assist: Some(c.fan_assist.as_ref().is_some_and(|f| f.enabled)),
        resume_at_profile_seconds: None,
        // A cancelled auto DROP stays cancelled for the roast.
        auto_drop: c.auto_drop.as_ref().map(|a| !a.cancelled),
    }
}

/// The hotter of BT and ET in a sample.
fn hotter(s: &roastwarden_core::Sample) -> Option<f64> {
    match (s.bt, s.et) {
        (Some(b), Some(e)) => Some(b.max(e)),
        (b, e) => b.or(e),
    }
}

// ------------------------------------------------------------ settings --

fn setting_key(device_id: &str) -> String {
    format!("roast_sequence:{device_id}")
}

/// A roaster's saved sequence settings, if it has any.
pub async fn load_saved(db: &SqlitePool, device_id: &str) -> Option<SequenceConfig> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(setting_key(device_id))
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    raw.and_then(|r| serde_json::from_str::<SequenceConfig>(&r).ok())
        .filter(|c| c.validate(0.0).is_ok())
}

struct Cooling {
    session_id: Option<String>,
    until_bt_c: f64,
    mark: bool,
    since: tokio::time::Instant,
}

pub struct SequenceService {
    db: SqlitePool,
    deps: ActionDeps,
    alarms: Arc<AlarmService>,
    telemetry: TelemetryService,
    /// Landmarks already acted on, per session.
    handled: Mutex<HashMap<String, HashSet<&'static str>>>,
    cooling: Mutex<HashMap<String, Cooling>>,
    /// Auto DROP steps done, per drive run: `notice`, `drop`.
    auto_drop_done: Mutex<HashSet<(String, &'static str)>>,
}

impl SequenceService {
    pub fn new(
        db: SqlitePool,
        deps: ActionDeps,
        alarms: Arc<AlarmService>,
        telemetry: TelemetryService,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            deps,
            alarms,
            telemetry,
            handled: Mutex::new(HashMap::new()),
            cooling: Mutex::new(HashMap::new()),
            auto_drop_done: Mutex::new(HashSet::new()),
        })
    }

    /// The roaster's sequence: what it saved, else its kind's (a roaster with no setup is
    /// `other`, whose sequence records only).
    pub async fn config(&self, device_id: &str) -> SequenceConfig {
        match load_saved(&self.db, device_id).await {
            Some(c) => c,
            None => SequenceConfig::for_kind(self.deps.transports.limits(device_id).await.kind),
        }
    }

    pub async fn set_config(&self, device_id: &str, c: &SequenceConfig) -> Result<(), String> {
        c.validate(self.deps.transports.limits(device_id).await.fan_floor_pct)?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(setting_key(device_id))
        .bind(serde_json::to_string(c).unwrap_or_default())
        .execute(&self.db)
        .await
        .map(|_| ())
        .map_err(|e| format!("database error: {e}"))
    }

    /// Whether the roaster is cooling after a DROP or a cancel.
    pub async fn cooling(&self, device_id: &str) -> bool {
        self.cooling.lock().await.contains_key(device_id)
    }

    /// The roast the roaster is cooling after its DROP, if any: its samples are stored
    /// until COOL END, Complete or not.
    pub async fn cooling_roast(&self, device_id: &str) -> Option<String> {
        self.cooling
            .lock()
            .await
            .get(device_id)
            .and_then(|c| c.session_id.clone())
    }

    /// Cooling starts: the fan runs until the chamber is cold, and the roast's samples are
    /// stored until then even once it is complete (a roast completed a second after DROP
    /// otherwise ends its trend at DROP while the roaster cools for minutes).
    async fn start_cooling(&self, device_id: &str, c: Cooling) {
        if let Some(sid) = &c.session_id {
            self.telemetry.record_cooling(device_id, sid);
        }
        self.cooling.lock().await.insert(device_id.to_string(), c);
    }

    /// Cooling is over, however it ended: the roast records no more.
    async fn remove_cooling(&self, device_id: &str) -> Option<Cooling> {
        let c = self.cooling.lock().await.remove(device_id);
        self.telemetry.cooling_over(device_id);
        c
    }

    /// End the last roast's cooling without its fan-off: something else (a new roast, the
    /// server's autotune) now runs the roaster, and a fan-off landing in it would gate the
    /// heater. Returns whether it was cooling.
    pub async fn end_cooling(&self, device_id: &str, why: &str) -> bool {
        let ended = self.remove_cooling(device_id).await.is_some();
        if ended {
            tracing::info!(device_id, why, "cooling of the last roast ends");
        }
        ended
    }

    /// Run forever: landmark changes and telemetry (for the end of cooling).
    pub async fn run(self: Arc<Self>) {
        let mut marks = self.alarms.subscribe_marks();
        let mut samples = self.telemetry.subscribe();
        let mut drives = self.deps.replay.subscribe();
        loop {
            tokio::select! {
                d = drives.recv() => match d {
                    Ok(st) => self.on_drive(&st).await,
                    // Each tick sends the state again: a lagged one is caught up by the next.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                m = marks.recv() => match m {
                    Ok(ch) => self.on_marks(&ch.session_id).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "roast sequence lagged behind landmark changes");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                s = samples.recv() => match s {
                    Ok(evt) => self.on_sample(&evt.device_id, evt.sample.bt, hotter(&evt.sample)).await,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    }

    /// A drive's tick: auto DROP's notice when its countdown starts, DROP when it ends.
    async fn on_drive(&self, st: &ReplayState) {
        let Some(ad) = &st.view.auto_drop else {
            return;
        };
        if !st.active || st.config.ghost {
            return;
        }
        let step = match ad.state {
            DropState::Counting => "notice",
            DropState::Due => "drop",
            DropState::Armed | DropState::Cancelled => return,
        };
        if !self
            .auto_drop_done
            .lock()
            .await
            .insert((st.run_id.clone(), step))
        {
            return;
        }
        let Ok(Some(session)) =
            crate::roasts::sessions::get_session(&self.db, &st.session_id).await
        else {
            return;
        };
        if session.status != SessionStatus::Active {
            return;
        }
        let bt = self
            .telemetry
            .latest_sample(&st.device_id)
            .and_then(|s| s.bt);
        if step == "notice" {
            let secs = ad.eta_seconds.unwrap_or(ad.config.countdown_seconds);
            self.alarms.announce(AlarmEvent {
                device_id: st.device_id.clone(),
                session_id: st.session_id.clone(),
                session_name: session.name.clone(),
                name: "Auto DROP".into(),
                condition_type: "auto_drop".into(),
                threshold: ad.config.at,
                message: format!(
                    "DROP in {secs:.0} s, {}. Cancel on the Roast page to roast on.",
                    ad.describe
                ),
                value: bt,
                elapsed_seconds: st.view.clock.live_t,
                fired_at: Utc::now(),
                actions: Vec::new(),
            });
            return;
        }
        let Ok(events) = crate::roasts::events::get_roast_events(&self.db, &st.session_id).await
        else {
            return;
        };
        if events.iter().any(|e| e.event_type == RoastEventType::Drop) {
            return;
        }
        let Some(start) = session.start_time else {
            return;
        };
        let req = CreateRoastEventRequest {
            event_type: RoastEventType::Drop,
            elapsed_seconds: (Utc::now() - start).num_milliseconds() as f32 / 1000.0,
            temperature: bt.map(|b| b as f32),
            notes: Some(format!("marked by auto DROP ({})", ad.describe)),
        };
        match crate::roasts::events::create_roast_event(&self.db, &st.session_id, req).await {
            Ok(_) => {
                tracing::info!(session_id = %st.session_id, device_id = %st.device_id, by = ad.config.by.as_str(), "auto DROP marked DROP");
                self.alarms.invalidate_events(&st.session_id).await;
            }
            Err(e) => {
                tracing::warn!(session_id = %st.session_id, error = %e, "auto DROP could not mark DROP; tries again next tick");
                self.auto_drop_done
                    .lock()
                    .await
                    .remove(&(st.run_id.clone(), "drop"));
            }
        }
    }

    async fn on_marks(&self, session_id: &str) {
        let Ok(Some(session)) = crate::roasts::sessions::get_session(&self.db, session_id).await
        else {
            return;
        };
        if session.status != SessionStatus::Active {
            return;
        }
        let Ok(events) = crate::roasts::events::get_roast_events(&self.db, session_id).await else {
            return;
        };
        let now = Utc::now();
        let mut todo = Vec::new();
        {
            let mut handled = self.handled.lock().await;
            let seen = handled.entry(session_id.to_string()).or_default();
            for (kind, name, step) in [
                (RoastEventType::Charge, "charge", Step::Charge),
                (RoastEventType::Drop, "drop", Step::Drop),
            ] {
                let Some(ev) = events.iter().find(|e| e.event_type == kind) else {
                    continue;
                };
                if !seen.insert(name) {
                    continue;
                }
                if (now - ev.created_at).num_seconds() <= ACT_WITHIN_SECS {
                    todo.push(step);
                }
            }
        }
        for step in todo {
            self.act(
                step,
                &session.device_id,
                Some(session_id),
                session.profile_id.is_some(),
            )
            .await;
        }
    }

    /// Resume Profile drive on an active, charged, not dropped roast with a profile, when
    /// no drive runs (typically after the roaster cut its heater).
    pub async fn resume(&self, session_id: &str) -> Result<(), ResumeError> {
        let session = crate::roasts::sessions::get_session(&self.db, session_id)
            .await
            .map_err(|e| ResumeError::Invalid(e.to_string()))?
            .ok_or(ResumeError::NotFound)?;
        if session.status != SessionStatus::Active {
            return Err(ResumeError::Invalid("the roast is not running".into()));
        }
        if session.profile_id.is_none() {
            return Err(ResumeError::Invalid(
                "the roast has no profile to drive".into(),
            ));
        }
        if self
            .deps
            .replay
            .state(session_id)
            .await
            .is_some_and(|s| s.active)
        {
            return Err(ResumeError::Invalid(
                "Profile drive is already running".into(),
            ));
        }
        let events = crate::roasts::events::get_roast_events(&self.db, session_id)
            .await
            .map_err(|e| ResumeError::Invalid(e.to_string()))?;
        if events.iter().any(|e| e.event_type == RoastEventType::Drop) {
            return Err(ResumeError::Invalid("DROP is already marked".into()));
        }
        self.act(Step::Resume, &session.device_id, Some(session_id), true)
            .await;
        Ok(())
    }

    /// Cancel a roast: stop, cool if hot, delete it.
    pub async fn cancel(&self, session_id: &str) -> Result<bool, String> {
        let Some(session) = crate::roasts::sessions::get_session(&self.db, session_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(false);
        };
        self.act(Step::Cancel, &session.device_id, None, false)
            .await;
        let deleted = crate::roasts::sessions::delete_session(&self.db, session_id)
            .await
            .map_err(|e| e.to_string())?;
        self.handled.lock().await.remove(session_id);
        Ok(deleted)
    }

    async fn act(&self, step: Step, device_id: &str, session_id: Option<&str>, has_profile: bool) {
        // A new CHARGE (or a resume) ends the last roast's cooling: left running, its
        // fan-off would land in this roast and gate the heater.
        if matches!(step, Step::Charge | Step::Resume) {
            self.end_cooling(device_id, "a new roast is on").await;
        }
        let cfg = self.config(device_id).await;
        let drive_running = match session_id {
            Some(sid) => self.deps.replay.state(sid).await.is_some_and(|s| s.active),
            None => false,
        };
        let hot = self
            .telemetry
            .latest_sample(device_id)
            .and_then(|s| hotter(&s));
        let limits = self.deps.transports.limits(device_id).await;
        let acts = for_outputs(
            for_control(
                plan(step, &cfg, has_profile, drive_running, hot),
                limits.control,
            ),
            &limits.outputs,
        );
        if acts.is_empty() {
            return;
        }
        let mut done = Vec::new();
        for a in acts {
            let r = self.apply(&a, device_id, session_id).await;
            done.push(match r {
                Ok(()) => format!("{a:?}"),
                Err(e) => {
                    tracing::warn!(device_id, step = ?step, act = ?a, error = %e, "roast sequence step failed");
                    format!("{a:?}: failed: {e}")
                }
            });
        }
        tracing::info!(device_id, step = ?step, acts = ?done, "roast sequence");
    }

    async fn apply(
        &self,
        a: &Act,
        device_id: &str,
        session_id: Option<&str>,
    ) -> Result<(), String> {
        let d = &self.deps;
        let send = |cmd: DeviceCommand| async move {
            d.transports
                .send(device_id, &cmd, SendOptions::default())
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        match a {
            Act::Fan(pct) => {
                d.replay
                    .manual_override(device_id, &[Channel::Fan], "fan", Some(*pct))
                    .await;
                let r = send(DeviceCommand::Fan { percent: *pct }).await;
                if r.is_ok() {
                    d.fan_assist.operator_fan(device_id, *pct).await;
                    d.scheduler.fan_changed(device_id, *pct).await;
                }
                r
            }
            Act::HeaterPower(on) => send(DeviceCommand::HeaterEnable { enabled: *on }).await,
            Act::Mode(m) => send(DeviceCommand::Mode { mode: *m }).await,
            Act::Heater(pct) => send(DeviceCommand::Heater { percent: *pct }).await,
            Act::StartDrive => {
                let sid = session_id.ok_or("no roast to drive")?;
                // A resume takes the stopped run's settings; a fresh start the defaults.
                // One a restart cut picks up at the profile time where it stopped.
                let req = match d.replay.last_stopped(sid).await {
                    Some(st) => StartReplay {
                        resume_at_profile_seconds: st
                            .stopped_by_restart
                            .then_some(st.view.clock.profile_t),
                        ..resume_request(&st.config)
                    },
                    None => StartReplay::default(),
                };
                d.replay
                    .start(sid, req)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Act::StopDrive => {
                d.replay.stop_for_device(device_id, "roast sequence").await;
                Ok(())
            }
            Act::Cool { until_bt_c, mark } => {
                self.start_cooling(
                    device_id,
                    Cooling {
                        session_id: session_id.map(str::to_string),
                        until_bt_c: *until_bt_c,
                        mark: *mark,
                        since: tokio::time::Instant::now(),
                    },
                )
                .await;
                Ok(())
            }
        }
    }

    /// Cooling ends when the hotter of BT and ET is below the threshold; `bt` is what a
    /// COOL END mark records.
    async fn on_sample(&self, device_id: &str, bt: Option<f64>, hot: Option<f64>) {
        let outcome = {
            let cooling = self.cooling.lock().await;
            let Some(c) = cooling.get(device_id) else {
                return;
            };
            if c.since.elapsed() > COOL_TIMEOUT {
                Some(false)
            } else {
                match hot {
                    Some(t) if t < c.until_bt_c => Some(true),
                    _ => None,
                }
            }
        };
        let Some(reached) = outcome else { return };
        let Some(c) = self.remove_cooling(device_id).await else {
            return;
        };
        if !reached {
            tracing::warn!(
                device_id,
                "cooling did not reach its end temperature; fan left as it is"
            );
            return;
        }
        let fan = self.apply(&Act::Fan(0.0), device_id, None).await;
        if let Err(e) = fan {
            tracing::warn!(device_id, error = %e, "cooling: could not switch the fan off");
        }
        if let (true, Some(sid)) = (c.mark, c.session_id) {
            self.mark_cool_end(&sid, bt).await;
        }
        tracing::info!(device_id, bt = ?bt, hottest = ?hot, "cooling done: fan off");
    }

    async fn mark_cool_end(&self, session_id: &str, bt: Option<f64>) {
        let Ok(Some(s)) = crate::roasts::sessions::get_session(&self.db, session_id).await else {
            return;
        };
        // Complete may come before the chamber is cold: COOL END still lands on the roast.
        if !matches!(s.status, SessionStatus::Active | SessionStatus::Completed) {
            return;
        }
        let Ok(events) = crate::roasts::events::get_roast_events(&self.db, session_id).await else {
            return;
        };
        if events
            .iter()
            .any(|e| e.event_type == RoastEventType::CoolEnd)
        {
            return;
        }
        let Some(start) = s.start_time else { return };
        let elapsed = (Utc::now() - start).num_milliseconds() as f32 / 1000.0;
        let req = CreateRoastEventRequest {
            event_type: RoastEventType::CoolEnd,
            elapsed_seconds: elapsed,
            temperature: bt.map(|b| b as f32),
            notes: Some("marked by the roast sequence".into()),
        };
        if crate::roasts::events::create_roast_event(&self.db, session_id, req)
            .await
            .is_ok()
        {
            self.alarms.invalidate_events(session_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_control_level_limits_the_steps() {
        let c = SequenceConfig::default();
        let charge = plan(Step::Charge, &c, true, false, Some(25.0));
        assert!(charge.contains(&Act::StartDrive));
        assert_eq!(
            for_control(charge.clone(), Control::Off),
            [] as [crate::control::sequence::Act; 0]
        );
        // A logger has no outputs: nothing runs; a heater with no fan keeps the heater steps.
        use crate::roasters::setup::{Kind, Outputs};
        assert_eq!(
            for_outputs(charge.clone(), &Kind::Logger.outputs()),
            [] as [crate::control::sequence::Act; 0]
        );
        let heater_only = Outputs {
            fan: None,
            ..Kind::DrumElectric.outputs()
        };
        assert!(
            for_outputs(charge.clone(), &heater_only)
                .iter()
                .all(|a| !matches!(a, Act::Fan(_) | Act::Cool { .. }))
        );
        assert_eq!(
            for_outputs(charge.clone(), &Kind::FluidBedElectric.outputs()),
            charge
        );
        let manual = for_control(charge.clone(), Control::Manual);
        assert_ne!(manual, [] as [crate::control::sequence::Act; 0]);
        assert!(
            manual
                .iter()
                .all(|a| matches!(a, Act::Fan(_) | Act::HeaterPower(_) | Act::Heater(_)))
        );
        assert_eq!(for_control(charge.clone(), Control::Full), charge);
        let drop = plan(Step::Drop, &c, true, true, Some(212.0));
        let manual_drop = for_control(drop, Control::Manual);
        assert!(manual_drop.iter().any(|a| matches!(a, Act::Cool { .. })));
        assert!(!manual_drop.contains(&Act::StopDrive));
    }

    #[test]
    fn charge_sets_the_fan_before_the_heater_and_starts_the_drive() {
        let c = SequenceConfig::default();
        assert_eq!(
            plan(Step::Charge, &c, true, false, Some(25.0)),
            vec![
                Act::Fan(100.0),
                Act::Mode(ControlMode::Manual),
                Act::HeaterPower(true),
                Act::Heater(60.0),
                Act::StartDrive
            ]
        );
        // No start output: the old order.
        let zero = SequenceConfig {
            charge_heater_pct: 0.0,
            ..SequenceConfig::default()
        };
        assert_eq!(
            plan(Step::Charge, &zero, true, false, None),
            vec![Act::Fan(100.0), Act::HeaterPower(true), Act::StartDrive]
        );
        // No profile: no drive.
        assert_eq!(
            plan(Step::Charge, &c, false, false, None),
            vec![Act::Fan(100.0), Act::HeaterPower(true)]
        );
    }

    #[test]
    fn charge_leaves_a_running_drive_alone() {
        let c = SequenceConfig::default();
        assert_eq!(
            plan(Step::Charge, &c, true, true, None),
            vec![Act::HeaterPower(true)]
        );
    }

    #[test]
    fn drop_stops_cuts_the_heater_and_cools() {
        let c = SequenceConfig::default();
        assert_eq!(
            plan(Step::Drop, &c, true, true, Some(212.0)),
            vec![
                Act::StopDrive,
                Act::HeaterPower(false),
                Act::Mode(ControlMode::Manual),
                Act::Heater(0.0),
                Act::Fan(100.0),
                Act::Cool {
                    until_bt_c: 45.0,
                    mark: true
                }
            ]
        );
        let keep = SequenceConfig {
            cool_end_bt_c: 0.0,
            ..c
        };
        assert_eq!(plan(Step::Drop, &keep, true, true, None).len(), 5);
    }

    #[test]
    fn switched_off_steps_do_nothing_but_cancel_always_stops() {
        let off = SequenceConfig {
            charge_enabled: false,
            drop_enabled: false,
            ..SequenceConfig::default()
        };
        assert_eq!(
            plan(Step::Charge, &off, true, false, None),
            [] as [crate::control::sequence::Act; 0]
        );
        assert_eq!(
            plan(Step::Drop, &off, true, false, None),
            [] as [crate::control::sequence::Act; 0]
        );
        assert_eq!(
            plan(Step::Cancel, &off, true, true, Some(30.0)),
            vec![
                Act::StopDrive,
                Act::HeaterPower(false),
                Act::Mode(ControlMode::Manual),
                Act::Heater(0.0),
                Act::Fan(0.0)
            ]
        );
        assert_eq!(
            plan(Step::Cancel, &off, true, true, Some(150.0))[4],
            Act::Fan(100.0)
        );
        // Cooling off after DROP still cools a hot chamber on cancel.
        let no_cool = SequenceConfig {
            cool_end_bt_c: 0.0,
            ..SequenceConfig::default()
        };
        assert_eq!(
            plan(Step::Cancel, &no_cool, true, false, Some(83.0))[5],
            Act::Cool {
                until_bt_c: 45.0,
                mark: false
            }
        );
    }

    #[test]
    fn resume_starts_from_the_start_output_and_leaves_the_fan() {
        let c = SequenceConfig::default();
        assert_eq!(
            plan(Step::Resume, &c, true, false, Some(130.0)),
            vec![
                Act::Mode(ControlMode::Manual),
                Act::HeaterPower(true),
                Act::Heater(60.0),
                Act::StartDrive
            ]
        );
    }

    #[test]
    fn fans_stay_above_the_heater_interlock() {
        let mut c = SequenceConfig::default();
        assert!(c.validate(45.0).is_ok());
        c.charge_fan_pct = 40.0;
        assert!(c.validate(45.0).is_err());
        // A roaster with no floor (a drum) takes any fan.
        assert!(c.validate(0.0).is_ok());
        c.charge_fan_pct = 70.0;
        c.cool_end_bt_c = 10.0;
        assert!(c.validate(45.0).is_err());
        c.cool_end_bt_c = 0.0;
        assert!(c.validate(45.0).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_autotune_start_ends_the_last_roasts_cooling() {
        use crate::test_support::{Harness, eventually};
        use serde_json::json;
        let h = Harness::new().await;
        let vr = crate::sim::virtual_roaster::DEVICE_ID;
        let on = h
            .owner(
                "PUT",
                "/api/virtual-roaster",
                Some(json!({ "enabled": true })),
            )
            .await;
        assert!(on.status.is_success());
        let latest = format!("/api/roaster/{vr}/telemetry/latest");
        let fan =
            || async { h.owner("GET", &latest, None).await.json()["telemetry"]["fan"].as_f64() };
        assert!(eventually(|| async { fan().await.is_some() }).await);
        // A roast that just dropped, still cooling. The roaster is cold, so without the fix
        // the cooling's fan-off lands on the next sample, inside the test.
        h.state
            .sequence
            .start_cooling(
                vr,
                Cooling {
                    session_id: None,
                    until_bt_c: 45.0,
                    mark: false,
                    since: tokio::time::Instant::now(),
                },
            )
            .await;
        let r = h
            .owner(
                "POST",
                &format!("/api/roaster/{vr}/tune/start"),
                Some(json!({ "setpoint": 160.0, "fan_pct": 100.0 })),
            )
            .await;
        assert!(r.status.is_success(), "{}", r.json());
        assert!(
            !h.state.sequence.cooling(vr).await,
            "the test ended the cooling"
        );
        assert!(eventually(|| async { fan().await == Some(100.0) }).await);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        assert_eq!(
            fan().await,
            Some(100.0),
            "the fan stays on through the test"
        );
        h.owner("POST", &format!("/api/roaster/{vr}/tune/stop"), None)
            .await;
    }

    /// Roast #18: DROP, Complete a second later, and the trend ended at DROP while the
    /// roaster cooled for two minutes. Complete while cooling keeps the roast's samples
    /// until the chamber is cold, then COOL END lands on the completed roast.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn complete_while_cooling_keeps_the_trend_to_cool_end() {
        use crate::test_support::{Harness, eventually};
        use serde_json::json;
        let h = Harness::new().await;
        let vr = crate::sim::virtual_roaster::DEVICE_ID;
        let on = h
            .owner(
                "PUT",
                "/api/virtual-roaster",
                Some(json!({ "enabled": true })),
            )
            .await;
        assert!(on.status.is_success());
        let r = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({ "name": "cooling kept", "device_id": vr })),
            )
            .await;
        let sid = r.json()["id"].as_str().expect("a roast").to_string();
        h.owner("POST", &format!("/api/sessions/{sid}/start"), None)
            .await;
        let telemetry = format!("/api/sessions/{sid}/telemetry");
        let rows = || async {
            h.owner("GET", &telemetry, None).await.json()["telemetry"]
                .as_array()
                .map_or(0, Vec::len)
        };
        assert!(
            eventually(|| async { rows().await >= 2 }).await,
            "the roast records"
        );
        // DROP's cooling, as the sequence sets it: a threshold no sample reaches yet.
        h.state
            .sequence
            .start_cooling(
                vr,
                Cooling {
                    session_id: Some(sid.clone()),
                    until_bt_c: 1.0,
                    mark: true,
                    since: tokio::time::Instant::now(),
                },
            )
            .await;
        let done = h
            .owner("POST", &format!("/api/sessions/{sid}/complete"), None)
            .await;
        assert_eq!(done.json()["status"], "completed", "{}", done.json());
        let seq = h
            .owner("GET", &format!("/api/roaster/{vr}/sequence"), None)
            .await
            .json();
        assert_eq!(seq["cooling"], true);
        assert_eq!(seq["cooling_roast_id"], sid.as_str());
        let at_complete = rows().await;
        assert!(
            eventually(|| async { rows().await > at_complete }).await,
            "a completed roast still records while the roaster cools"
        );
        let events = format!("/api/sessions/{sid}/events");
        let cool_end = || async {
            h.owner("GET", &events, None)
                .await
                .json()
                .as_array()
                .is_some_and(|evs| evs.iter().any(|e| e["event_type"] == "cool_end"))
        };
        assert!(!cool_end().await);
        // The chamber is cold against a threshold every sample is under: the next sample ends it.
        h.state
            .sequence
            .start_cooling(
                vr,
                Cooling {
                    session_id: Some(sid.clone()),
                    until_bt_c: 500.0,
                    mark: true,
                    since: tokio::time::Instant::now(),
                },
            )
            .await;
        assert!(
            eventually(cool_end).await,
            "COOL END lands on the completed roast"
        );
        assert!(!h.state.sequence.cooling(vr).await);
        assert_eq!(
            h.owner("GET", &format!("/api/roaster/{vr}/sequence"), None)
                .await
                .json()["cooling_roast_id"],
            serde_json::Value::Null
        );
        let at_end = rows().await;
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        assert_eq!(rows().await, at_end, "no sample is stored after COOL END");
        // The statistics are DROP-based and stay as Complete stored them.
        let shown = h
            .owner("GET", &format!("/api/sessions/{sid}"), None)
            .await
            .json();
        assert_eq!(shown["status"], "completed");
    }
}
