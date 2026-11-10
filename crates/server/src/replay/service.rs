//! The drives: one task per replaying session (`run.rs`), started from a profile, stopped on
//! session end, DROP or an emergency stop, steered mid-run; the last stopped run of each
//! session kept so a reload still shows why and a resume reuses its settings.

mod run;
#[cfg(test)]
mod tests;

use crate::roasters::setup::Control;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

use super::engine::{
    AlignOn, Channel, Engine, EventView, Hold, LogEntry, LogKind, LoopAt, Mark, PidGains, Plan,
    ReplayMode, RunConfig,
};
use super::types::{EventOp, ReplayError, ReplayState, StartReplay, UpdateReplay};
use super::{auto_drop, engine, resolve_loop};
use crate::analysis::telemetry::TelemetryService;
use crate::control::fan_assist::{AssistView, FanAssistConfig};
use crate::control::schedule::GainScheduler;
use crate::profiles::types::ProfileWithPoints;
use crate::roasters::caps::{CapsRegistry, DeviceCaps};
use crate::roasts::types::{RoastEventType, RoastSession, SessionStatus};
use crate::transports::transport::{SendOptions, TransportError, TransportRegistry};
use roastwarden_core::DeviceCommand;
use run::replay_loop;

const TICK: Duration = Duration::from_secs(1);
/// A sample older than this is stale: no BT triggers, the server PID holds.
const STALE_MS: u64 = 5_000;
/// Control lease the device enforces while a live run drives it...
const LEASE_MS: u32 = 10_000;
/// ...refreshed this often.
const LEASE_REFRESH: Duration = Duration::from_secs(3);
/// With telemetry this old the lease is no longer renewed, so the roaster
/// falls into its safe state on its own when it cannot be watched...
const LEASE_STALE: Duration = Duration::from_secs(8);
/// ...and the drive stops at this age.
const STALE_STOP: Duration = Duration::from_secs(20);
const DEFAULT_LOOKAHEAD_SECS: f64 = 20.0;
/// The setpoint goes out in steps this small (°C): at 1 °C the device PID
/// saw a step every 7–12 s late in a roast, each a kick of kp·1 °C, and BT
/// lagged half a step on average.
const DEFAULT_HYSTERESIS_C: f64 = 0.2;

struct Meta {
    run_id: String,
    session_id: String,
    device_id: String,
    profile_id: String,
    profile_name: String,
    hysteresis_c: f64,
    last_sent_at: Option<DateTime<Utc>>,
    sends: u64,
    last_error: Option<String>,
    stopped_reason: Option<String>,
    stopped_by_device: bool,
    stopped_by_restart: bool,
    telemetry_stale: bool,
    active: bool,
    caps: DeviceCaps,
    loop_reason: String,
    /// Lease currently held on the device (live run on a lease-capable device).
    lease_held: bool,
    lease_sent_at: Option<Instant>,
    /// The device reported its heater enabled during this run.
    heater_enabled_seen: bool,
    /// The firmware's stop record (`stopUptimeMs`) last seen while heating, so a cut can
    /// tell a new stop from an old one.
    stop_seen: Option<f64>,
    /// Since when the device's telemetry has been stale (none: fresh).
    stale_since: Option<Instant>,
}

struct RunInner {
    engine: Engine,
    meta: Meta,
}

impl RunInner {
    fn snapshot(&self) -> ReplayState {
        let cfg = self.engine.config().clone();
        let view = self.engine.view();
        ReplayState {
            run_id: self.meta.run_id.clone(),
            session_id: self.meta.session_id.clone(),
            device_id: self.meta.device_id.clone(),
            profile_id: self.meta.profile_id.clone(),
            profile_name: self.meta.profile_name.clone(),
            active: self.meta.active,
            lookahead_seconds: cfg.lookahead_seconds,
            hysteresis_c: self.meta.hysteresis_c,
            config: cfg,
            elapsed_seconds: view.clock.elapsed,
            last_sent_setpoint: self.engine.last_sent(Channel::Setpoint),
            last_sent_at: self.meta.last_sent_at,
            sends: self.meta.sends,
            last_error: self.meta.last_error.clone(),
            stopped_reason: self.meta.stopped_reason.clone(),
            stopped_by_device: self.meta.stopped_by_device,
            stopped_by_restart: self.meta.stopped_by_restart,
            telemetry_stale: self.meta.telemetry_stale,
            updated_at: Utc::now(),
            caps: self.meta.caps,
            loop_reason: self.meta.loop_reason.clone(),
            lease: self.meta.lease_held,
            view,
        }
    }
}

struct Run {
    task: JoinHandle<()>,
    inner: Arc<Mutex<RunInner>>,
    device_id: String,
}

pub struct ReplayService {
    runs: Mutex<HashMap<String, Run>>,
    /// The last run of each session once it stopped, so a reload still shows why and a
    /// resume can reuse its settings.
    stopped: Mutex<HashMap<String, ReplayState>>,
    transports: Arc<TransportRegistry>,
    telemetry: TelemetryService,
    caps: Arc<CapsRegistry>,
    scheduler: Arc<GainScheduler>,
    db: SqlitePool,
    events_tx: broadcast::Sender<ReplayState>,
}

/// What a drive starts from, once `ReplayService::prepare` has resolved and checked it.
struct Prepared {
    session: RoastSession,
    profile: ProfileWithPoints,
    profile_id: String,
    plan: Plan,
    cfg: RunConfig,
    caps: DeviceCaps,
    loop_reason: String,
    hysteresis_c: f64,
    marks: BTreeMap<AlignOn, Mark>,
    /// Session seconds now, when the session has started.
    elapsed_now: Option<f64>,
    /// The log line about auto DROP, when the profile has one.
    auto_drop_note: Option<String>,
}

impl ReplayService {
    pub fn new(
        transports: Arc<TransportRegistry>,
        telemetry: TelemetryService,
        caps: Arc<CapsRegistry>,
        scheduler: Arc<GainScheduler>,
        db: SqlitePool,
    ) -> Arc<Self> {
        let (events_tx, _) = broadcast::channel(64);
        Arc::new(Self {
            runs: Mutex::new(HashMap::new()),
            stopped: Mutex::new(HashMap::new()),
            transports,
            telemetry,
            caps,
            scheduler,
            db,
            events_tx,
        })
    }

    /// Runs a previous process left open (a crash, a power cut, a restart by hand): each is
    /// closed ("server restarted"); a live one whose roast is still on (not dropped) is rebuilt
    /// as a stopped run, at the profile time where it stopped, so the Roast page and `roast
    /// live` offer to resume it (`docs/replay.md`, "Resume after a restart"). Call on boot,
    /// before telemetry flows.
    pub async fn recover_cut_runs(&self) {
        let rows = match sqlx::query(
            "SELECT id, session_id, ghost, config FROM replay_runs WHERE ended_at IS NULL ORDER BY started_at",
        )
        .fetch_all(&self.db)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "could not read replay runs left open");
                return;
            }
        };
        if rows.is_empty() {
            return;
        }
        if let Err(e) = sqlx::query(
            "UPDATE replay_runs SET ended_at = ?, end_reason = 'server restarted' WHERE ended_at IS NULL",
        )
        .bind(Utc::now())
        .execute(&self.db)
        .await
        {
            tracing::warn!(error = %e, "could not close replay runs left open");
        }
        tracing::warn!(
            runs = rows.len(),
            "closed replay runs left open by a restart"
        );
        for r in rows {
            let run_id: String = r.get("id");
            let session_id: String = r.get("session_id");
            if r.get::<bool, _>("ghost") {
                continue;
            }
            let Ok(cfg) = serde_json::from_str::<RunConfig>(&r.get::<String, _>("config")) else {
                continue;
            };
            match self.rebuild_cut_run(&run_id, &session_id, cfg).await {
                Ok(Some(st)) => {
                    tracing::warn!(
                        session_id,
                        device_id = %st.device_id,
                        profile_t = st.view.clock.profile_t,
                        "a restart cut Profile drive mid-roast: kept for a resume"
                    );
                    self.stopped.lock().await.insert(session_id, st);
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(session_id, error = %e, "could not rebuild a cut drive"),
            }
        }
    }

    /// The stopped state of a run a restart cut, or None when its roast is over.
    async fn rebuild_cut_run(
        &self,
        run_id: &str,
        session_id: &str,
        cfg: RunConfig,
    ) -> anyhow::Result<Option<ReplayState>> {
        let Some(session) = crate::roasts::sessions::get_session(&self.db, session_id).await?
        else {
            return Ok(None);
        };
        if !matches!(
            session.status,
            SessionStatus::Active | SessionStatus::Paused
        ) {
            return Ok(None);
        }
        let events = crate::roasts::events::get_roast_events(&self.db, session_id).await?;
        if events.iter().any(|e| e.event_type == RoastEventType::Drop) {
            return Ok(None);
        }
        let Some(profile_id) = session.profile_id.clone() else {
            return Ok(None);
        };
        let Some(profile) =
            crate::profiles::store::get_profile_with_points(&self.db, &profile_id).await?
        else {
            return Ok(None);
        };
        // Where it stopped: the roast's last sample before the restart (nothing new is
        // recorded yet on boot), else the run's last log entry.
        let stopped_at: Option<f64> = sqlx::query_scalar(
            "SELECT MAX(t) FROM (SELECT MAX(elapsed_seconds) AS t FROM session_telemetry WHERE session_id = ?1
             UNION ALL SELECT MAX(elapsed) FROM replay_log WHERE run_id = ?2)",
        )
        .bind(session_id)
        .bind(run_id)
        .fetch_one(&self.db)
        .await?;
        let Some(stopped_at) = stopped_at else {
            return Ok(None);
        };
        let marks = marks_from(&events);
        let caps = self.device_caps(&session.device_id);
        let loop_reason = format!("{} (as before the restart)", cfg.loop_at.as_str());
        let hysteresis = cfg.hysteresis_c;
        let mut engine = Engine::new(Plan::from_profile(&profile), cfg);
        engine.place(stopped_at, &marks);
        let reason = "the server restarted";
        let entry = engine.entry(
            LogKind::Stop,
            None,
            None,
            Some(format!(
                "{reason}; the roast is still on, a resume is offered"
            )),
        );
        self.persist(run_id, session_id, false, &[entry]).await;
        let inner = RunInner {
            engine,
            meta: Meta {
                run_id: run_id.to_string(),
                session_id: session_id.to_string(),
                device_id: session.device_id.clone(),
                profile_id,
                profile_name: profile.profile.name.clone(),
                hysteresis_c: hysteresis,
                last_sent_at: None,
                sends: 0,
                last_error: None,
                stopped_reason: Some(reason.into()),
                stopped_by_device: false,
                stopped_by_restart: true,
                telemetry_stale: false,
                active: false,
                caps,
                loop_reason,
                lease_held: false,
                lease_sent_at: None,
                heater_enabled_seen: false,
                stop_seen: None,
                stale_since: None,
            },
        };
        Ok(Some(inner.snapshot()))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ReplayState> {
        self.events_tx.subscribe()
    }

    async fn run_inner(&self, session_id: &str) -> Option<Arc<Mutex<RunInner>>> {
        self.runs
            .lock()
            .await
            .get(session_id)
            .map(|r| r.inner.clone())
    }

    pub async fn state(&self, session_id: &str) -> Option<ReplayState> {
        let inner = self.run_inner(session_id).await?;
        let s = inner.lock().await.snapshot();
        Some(s)
    }

    /// The session's last run after it stopped (none while one runs).
    pub async fn last_stopped(&self, session_id: &str) -> Option<ReplayState> {
        self.stopped.lock().await.get(session_id).cloned()
    }

    pub async fn events(&self, session_id: &str) -> Option<Vec<EventView>> {
        let inner = self.run_inner(session_id).await?;
        let v = inner.lock().await.engine.events_view();
        Some(v)
    }

    pub async fn list(&self) -> Vec<ReplayState> {
        let inners: Vec<_> = self
            .runs
            .lock()
            .await
            .values()
            .map(|r| r.inner.clone())
            .collect();
        let mut out = Vec::with_capacity(inners.len());
        for i in inners {
            out.push(i.lock().await.snapshot());
        }
        out
    }

    async fn setting_f64(&self, key: &str, default: f64) -> f64 {
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    /// Start replaying the session's profile. Replaces any run for the same
    /// session or device.
    pub async fn start(
        self: &Arc<Self>,
        session_id: &str,
        req: StartReplay,
    ) -> Result<ReplayState, ReplayError> {
        let Prepared {
            session,
            profile,
            profile_id,
            plan,
            cfg,
            caps,
            loop_reason,
            hysteresis_c: hysteresis,
            marks,
            elapsed_now,
            auto_drop_note,
        } = self.prepare(session_id, req).await?;
        // One run per session and per device.
        self.stop(session_id, "restarted").await;
        self.stop_for_device(&session.device_id, "another session started a replay")
            .await;

        if !cfg.ghost {
            self.send_mode(&session.device_id, &cfg).await?;
        }

        let run_id = uuid::Uuid::new_v4().to_string();
        let started = Utc::now();
        self.record_run(
            &run_id,
            session_id,
            &profile_id,
            &profile.profile.name,
            &cfg,
            started,
        )
        .await?;

        let mut engine = Engine::new(plan, cfg.clone());
        // The clock where it is now, so the state reads right before the first tick
        if let Some(elapsed) = elapsed_now {
            engine.place(elapsed, &marks);
        }
        let start_entry = engine.entry(
            LogKind::Start,
            None,
            None,
            Some(format!(
                "{} · loop {} · trigger {} · align {}{}{}",
                cfg.mode.as_str(),
                cfg.loop_at.as_str(),
                cfg.trigger.as_str(),
                cfg.align.as_str(),
                if cfg.ghost { " · ghost" } else { "" },
                cfg.hold
                    .map(|h| format!(" · resumed, profile {:+.0} s later", h.seconds))
                    .unwrap_or_default()
            )),
        );
        let auto_drop_entry =
            auto_drop_note.map(|n| engine.entry(LogKind::AutoDrop, None, None, Some(n)));
        let inner = Arc::new(Mutex::new(RunInner {
            engine,
            meta: Meta {
                run_id: run_id.clone(),
                session_id: session_id.to_string(),
                device_id: session.device_id.clone(),
                profile_id,
                profile_name: profile.profile.name.clone(),
                hysteresis_c: hysteresis,
                last_sent_at: None,
                sends: 0,
                last_error: None,
                stopped_reason: None,
                stopped_by_device: false,
                stopped_by_restart: false,
                telemetry_stale: false,
                active: true,
                caps,
                loop_reason,
                lease_held: false,
                lease_sent_at: None,
                heater_enabled_seen: false,
                stop_seen: None,
                stale_since: None,
            },
        }));
        let entries: Vec<LogEntry> = std::iter::once(start_entry)
            .chain(auto_drop_entry)
            .collect();
        self.persist(&run_id, session_id, cfg.ghost, &entries).await;

        let task = tokio::spawn(replay_loop(
            self.clone(),
            session_id.to_string(),
            inner.clone(),
        ));
        let snapshot = inner.lock().await.snapshot();
        self.runs.lock().await.insert(
            session_id.to_string(),
            Run {
                task,
                inner,
                device_id: session.device_id.clone(),
            },
        );
        tracing::info!(
            session_id,
            device_id = %session.device_id,
            mode = cfg.mode.as_str(),
            ghost = cfg.ghost,
            "replay started"
        );
        Ok(snapshot)
    }

    /// The run a start asks for, resolved against the session, its profile, the roaster's limits
    /// and capabilities and the settings, and checked; a resume's lost time goes on the profile
    /// offset. Nothing is sent or stored yet.
    async fn prepare(&self, session_id: &str, req: StartReplay) -> Result<Prepared, ReplayError> {
        let session = crate::roasts::sessions::get_session(&self.db, session_id)
            .await
            .map_err(|e| ReplayError::Db(e.to_string()))?
            .ok_or(ReplayError::SessionNotFound)?;
        if session.status != SessionStatus::Active {
            return Err(ReplayError::SessionNotActive(session.status));
        }
        // A drive sends setpoint, mode and heater: only with full control. A ghost sends nothing.
        let limits = self.transports.limits(&session.device_id).await;
        let control = limits.control;
        if !req.ghost && control != Control::Full {
            return Err(ReplayError::ControlOff(TransportError::ControlOff {
                device_id: session.device_id.clone(),
                control,
                command: "profile drive",
            }));
        }
        // A drive acts on the heater (setpoint, mode, heater %): a roaster without one has
        // nothing to drive.
        if !req.ghost && limits.outputs.heater.is_none() {
            return Err(ReplayError::ControlOff(TransportError::NotDeclared {
                device_id: session.device_id.clone(),
                output: "heater",
            }));
        }
        let profile_id = session.profile_id.clone().ok_or(ReplayError::NoProfile)?;
        let profile = crate::profiles::store::get_profile_with_points(&self.db, &profile_id)
            .await
            .map_err(|e| ReplayError::Db(e.to_string()))?
            .ok_or(ReplayError::NoProfile)?;
        let events = crate::roasts::events::get_roast_events(&self.db, session_id)
            .await
            .map_err(|e| ReplayError::Db(e.to_string()))?;
        if events.iter().any(|e| e.event_type == RoastEventType::Drop) {
            return Err(ReplayError::Invalid(
                "DROP is already marked for this session".into(),
            ));
        }
        self.stopped.lock().await.remove(session_id);
        let plan = Plan::from_profile(&profile);
        let mode = req.mode.unwrap_or_default();
        if plan.bt.is_empty() {
            return Err(ReplayError::EmptyProfile);
        }
        if mode != ReplayMode::Setpoint && !plan.has_data_for(Channel::Heater) {
            return Err(ReplayError::NoHeaterData);
        }

        let caps = self.device_caps(&session.device_id);
        let (loop_at, loop_reason) =
            resolve_loop(mode, req.loop_pref, caps).map_err(ReplayError::Invalid)?;

        // Lookahead anticipates the loop's lag in setpoint mode; with
        // feedforward the profile heater already anticipates.
        let lookahead = match req.lookahead_seconds {
            Some(v) if v.is_finite() && (0.0..=300.0).contains(&v) => v,
            Some(_) => {
                return Err(ReplayError::Invalid(
                    "lookahead_seconds must be 0..300".into(),
                ));
            }
            None if mode == ReplayMode::FeedforwardPid => 0.0,
            None => {
                self.setting_f64("profile_lookahead_seconds", DEFAULT_LOOKAHEAD_SECS)
                    .await
            }
        };
        let hysteresis = self
            .setting_f64("follow_hysteresis_c", DEFAULT_HYSTERESIS_C)
            .await
            .max(0.0);
        let adapt_lookahead = match req.adapt_lookahead {
            Some(v) => v,
            None => crate::auth::settings::flag(&self.db, "profile_lookahead_adapt", true).await,
        };
        let server_pid = self.server_pid_gains(&session.device_id).await;
        let mut assist = crate::control::fan_assist::load(&self.db, &session.device_id).await;
        if let Some(on) = req.fan_assist {
            assist.enabled = on;
        }
        let resume_at = match req.resume_at_profile_seconds {
            Some(t) if t.is_finite() && t >= 0.0 => Some(t),
            Some(_) => {
                return Err(ReplayError::Invalid(
                    "resume_at_profile_seconds must be 0 or more".into(),
                ));
            }
            None => None,
        };
        let mut cfg = RunConfig {
            mode,
            trigger: req.trigger.unwrap_or_default(),
            align: req.align.unwrap_or_default(),
            // The profile's fan only reaches a roaster that has a fan output.
            fan: limits.outputs.fan.is_some()
                && req.fan.unwrap_or_else(|| plan.has_data_for(Channel::Fan)),
            ghost: req.ghost,
            lookahead_seconds: lookahead,
            adapt_lookahead,
            hysteresis_c: hysteresis,
            loop_at,
            server_pid,
            fan_assist: (assist.enabled && mode.closed_loop() && limits.has_heater_and_fan())
                .then_some(assist),
            hold: None,
            auto_drop: None,
            fan_floor_pct: limits.fan_floor_pct,
        };
        let auto_drop_note = self
            .auto_drop_for(
                &profile.profile,
                &session.device_id,
                req.auto_drop,
                &mut cfg,
            )
            .await;
        let marks = marks_from(&events);
        let elapsed_now = session
            .start_time
            .map(|start| (Utc::now() - start).num_milliseconds() as f64 / 1000.0);
        // Resuming a cut drive: the profile picks up where it stopped, the time since added
        // to its offset (none when the profile time now is not past that point).
        if let (Some(t), Some(elapsed)) = (resume_at, elapsed_now) {
            let now_t = Engine::new(plan.clone(), cfg.clone()).profile_time_at(elapsed, &marks);
            let lost = now_t - t;
            if lost > 0.5 {
                cfg.hold = Some(Hold {
                    seconds: lost,
                    since_elapsed: elapsed,
                });
            }
        }

        Ok(Prepared {
            session,
            profile,
            profile_id,
            plan,
            cfg,
            caps,
            loop_reason,
            hysteresis_c: hysteresis,
            marks,
            elapsed_now,
            auto_drop_note,
        })
    }

    /// The run's row in `replay_runs`, open until it ends.
    async fn record_run(
        &self,
        run_id: &str,
        session_id: &str,
        profile_id: &str,
        profile_name: &str,
        cfg: &RunConfig,
        started: DateTime<Utc>,
    ) -> Result<(), ReplayError> {
        sqlx::query(
            "INSERT INTO replay_runs (id, session_id, profile_id, profile_name, mode, trigger_policy, align_on, ghost, config, started_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(session_id)
        .bind(profile_id)
        .bind(profile_name)
        .bind(cfg.mode.as_str())
        .bind(cfg.trigger.as_str())
        .bind(cfg.align.as_str())
        .bind(cfg.ghost)
        .bind(serde_json::to_string(cfg).unwrap_or_default())
        .bind(started)
        .execute(&self.db)
        .await
        .map_err(|e| ReplayError::Db(e.to_string()))?;
        Ok(())
    }

    /// Put the profile's auto DROP in the run configuration; returns a line for the log
    /// saying what it will do, or why it is off.
    async fn auto_drop_for(
        &self,
        profile: &crate::profiles::types::RoastProfile,
        device_id: &str,
        wanted: Option<bool>,
        cfg: &mut RunConfig,
    ) -> Option<String> {
        profile.auto_drop.as_ref()?;
        let (lo, hi) = auto_drop::COUNTDOWN_RANGE;
        let countdown = self
            .setting_f64(
                "auto_drop_countdown_seconds",
                auto_drop::DEFAULT_COUNTDOWN_SECS,
            )
            .await
            .clamp(lo, hi);
        let mut a = match auto_drop::AutoDrop::from_profile(profile, countdown) {
            Ok(Some(a)) => a,
            Ok(None) => return None,
            Err(e) => return Some(format!("auto DROP off: {e}")),
        };
        // DROP's step switches the heater off: without it a drive that marks DROP would stop
        // and leave the heater on.
        let seq = match crate::control::sequence::load_saved(&self.db, device_id).await {
            Some(c) => c,
            None => crate::control::sequence::SequenceConfig::for_kind(
                self.transports.limits(device_id).await.kind,
            ),
        };
        if !seq.drop_enabled {
            return Some(
                "auto DROP off: the roaster's DROP step is off (Roasters → Control → Roast sequence), so nothing would switch the heater off"
                    .into(),
            );
        }
        a.cancelled = wanted == Some(false);
        let line = format!(
            "auto DROP {} ({countdown:.0} s countdown){}",
            a.describe(),
            if a.cancelled {
                ", cancelled for this roast"
            } else {
                ""
            }
        );
        cfg.auto_drop = Some(a);
        Some(line)
    }

    async fn send_mode(&self, device_id: &str, cfg: &RunConfig) -> Result<(), ReplayError> {
        self.transports
            .send(
                device_id,
                &DeviceCommand::Mode {
                    mode: cfg.device_mode(),
                },
                SendOptions::default(),
            )
            .await
            .map(|_| ())
            .map_err(ReplayError::Transport)
    }

    /// What the roaster said it can do, if it said (its status message).
    pub(crate) fn reported_caps(&self, device_id: &str) -> Option<DeviceCaps> {
        self.caps.reported(device_id)
    }

    /// Reported capabilities, else inferred from the device's telemetry.
    pub(crate) fn device_caps(&self, device_id: &str) -> DeviceCaps {
        self.caps.reported(device_id).unwrap_or_else(|| {
            let reports_sp = self
                .telemetry
                .latest_sample(device_id)
                .is_some_and(|s| s.setpoint.is_some());
            DeviceCaps::inferred(reports_sp)
        })
    }

    /// Server PID gains: `server_pid:<device_id>`, else `server_pid`, else defaults.
    async fn server_pid_gains(&self, device_id: &str) -> PidGains {
        for key in [format!("server_pid:{device_id}"), "server_pid".to_string()] {
            let raw = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
                .bind(&key)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten();
            if let Some(g) = raw.and_then(|r| serde_json::from_str::<PidGains>(&r).ok())
                && [g.kp, g.ki, g.kd]
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0)
            {
                return g;
            }
        }
        PidGains::default()
    }

    /// Gain schedule: gains follow the fan the device reports. With the loop
    /// on the device the scheduler sends them (`persist: false`, deduped);
    /// with the loop here the server PID takes them (device units / 2.55,
    /// since the server PID works in heater %).
    async fn follow_gain_schedule(
        &self,
        inner: &Arc<Mutex<RunInner>>,
        device_id: &str,
        sample: Option<&roastwarden_core::Sample>,
    ) {
        let (loop_at, ghost, closed) = {
            let g = inner.lock().await;
            let c = g.engine.config();
            (c.loop_at, c.ghost, c.mode.closed_loop())
        };
        if ghost || !closed {
            return;
        }
        // The commanded fan (replay or manual); the reported one only when
        // nothing was commanded through the server yet.
        let reported = sample.and_then(|s| s.fan);
        let Some(pct) = self.scheduler.commanded_fan(device_id).await.or(reported) else {
            return;
        };
        match loop_at {
            LoopAt::Device => {
                self.scheduler.fan_changed(device_id, pct).await;
            }
            LoopAt::Server => {
                if let Some(g) = self.scheduler.gains_for(device_id, pct).await {
                    let server = PidGains {
                        kp: g.kp / 2.55,
                        ki: g.ki / 2.55,
                        kd: g.kd / 2.55,
                    };
                    if inner.lock().await.engine.set_server_gains(server) {
                        tracing::info!(
                            device_id,
                            fan_pct = pct,
                            kp = server.kp,
                            ki = server.ki,
                            "server PID follows the gain schedule"
                        );
                    }
                }
            }
            LoopAt::Open => {}
        }
    }

    /// Send a lease refresh when due; returns whether one was sent.
    async fn refresh_lease(&self, inner: &Arc<Mutex<RunInner>>, force: bool) {
        let (device_id, due) = {
            let g = inner.lock().await;
            // Stale telemetry: let the lease run out, so the roaster goes to its
            // safe state by itself rather than heat with nobody watching.
            let watched = g.meta.stale_since.is_none_or(|t| t.elapsed() < LEASE_STALE);
            let live = g.meta.active && !g.engine.config().ghost && g.meta.caps.lease && watched;
            let due = live
                && (force
                    || g.meta
                        .lease_sent_at
                        .is_none_or(|t| t.elapsed() >= LEASE_REFRESH));
            (g.meta.device_id.clone(), due)
        };
        if !due {
            return;
        }
        let res = self
            .transports
            .send(
                &device_id,
                &DeviceCommand::Lease { ms: LEASE_MS },
                SendOptions::default(),
            )
            .await;
        let mut g = inner.lock().await;
        match res {
            Ok(_) => {
                g.meta.lease_held = true;
                g.meta.lease_sent_at = Some(Instant::now());
            }
            Err(e) => {
                tracing::warn!(device_id, error = %e, "replay lease refresh failed");
                g.meta.last_error = Some(format!("lease: {e}"));
            }
        }
    }

    /// Hand the device back: release the lease and clear the feedforward.
    /// Outputs are otherwise left where they are.
    async fn release_device(&self, inner: &Arc<Mutex<RunInner>>) {
        let (device_id, lease, ff) = {
            let mut g = inner.lock().await;
            let lease = g.meta.lease_held;
            g.meta.lease_held = false;
            g.meta.lease_sent_at = None;
            let cfg = g.engine.config();
            let ff = !cfg.ghost
                && cfg.mode == ReplayMode::FeedforwardPid
                && cfg.loop_at == LoopAt::Device;
            (g.meta.device_id.clone(), lease, ff)
        };
        if ff {
            let _ = self
                .transports
                .send(
                    &device_id,
                    &DeviceCommand::Feedforward { value: None },
                    SendOptions::default(),
                )
                .await;
        }
        if lease {
            let _ = self
                .transports
                .send(
                    &device_id,
                    &DeviceCommand::Lease { ms: 0 },
                    SendOptions::default(),
                )
                .await;
        }
    }

    /// The process died: runs vanish with nothing written, as on a crash.
    #[cfg(test)]
    pub(crate) async fn die_for_test(&self) {
        for (_, r) in self.runs.lock().await.drain() {
            r.task.abort();
        }
        self.stopped.lock().await.clear();
    }

    /// Stop a run. Outputs are left as they are. Returns the final state.
    pub async fn stop(&self, session_id: &str, reason: &str) -> Option<ReplayState> {
        let mut run = self.runs.lock().await.remove(session_id)?;
        run.task.abort();
        // Wait for the task to end, so a tick caught mid-send cannot deliver a
        // command after the caller's (DROP's heater off and cooling fan). Never
        // from the run's own task, which would wait on itself.
        if tokio::task::try_id() != Some(run.task.id()) {
            let _ = (&mut run.task).await;
        }
        Some(self.close(run.inner, reason).await)
    }

    pub async fn stop_for_device(&self, device_id: &str, reason: &str) {
        let ids: Vec<String> = {
            let runs = self.runs.lock().await;
            runs.iter()
                .filter(|(_, r)| r.device_id == device_id)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            self.stop(&id, reason).await;
        }
    }

    /// Called by the loop itself when the session ends.
    async fn finish(&self, session_id: &str, reason: &str) {
        let run = self.runs.lock().await.remove(session_id);
        if let Some(run) = run {
            self.close(run.inner, reason).await;
        }
    }

    async fn close(&self, inner: Arc<Mutex<RunInner>>, reason: &str) -> ReplayState {
        self.release_device(&inner).await;
        let (snapshot, entry, ghost) = {
            let mut g = inner.lock().await;
            g.meta.active = false;
            g.meta.stopped_reason = Some(reason.to_string());
            let entry = g
                .engine
                .entry(LogKind::Stop, None, None, Some(reason.to_string()));
            (g.snapshot(), entry, g.engine.config().ghost)
        };
        self.persist(&snapshot.run_id, &snapshot.session_id, ghost, &[entry])
            .await;
        if let Err(e) =
            sqlx::query("UPDATE replay_runs SET ended_at = ?, end_reason = ? WHERE id = ?")
                .bind(Utc::now())
                .bind(reason)
                .bind(&snapshot.run_id)
                .execute(&self.db)
                .await
        {
            tracing::warn!(error = %e, "could not close replay run");
        }
        self.stopped
            .lock()
            .await
            .insert(snapshot.session_id.clone(), snapshot.clone());
        let _ = self.events_tx.send(snapshot.clone());
        tracing::info!(session_id = %snapshot.session_id, reason, "replay stopped");
        snapshot
    }

    /// Change ghost/align/trigger/lookahead while running. Engaging a ghost
    /// run switches the device to the run's mode first.
    pub async fn update(
        &self,
        session_id: &str,
        req: UpdateReplay,
    ) -> Result<ReplayState, ReplayError> {
        let inner = self
            .run_inner(session_id)
            .await
            .ok_or(ReplayError::NotRunning)?;
        if let Some(l) = req.lookahead_seconds
            && !(l.is_finite() && (0.0..=300.0).contains(&l))
        {
            return Err(ReplayError::Invalid(
                "lookahead_seconds must be 0..300".into(),
            ));
        }
        let (device_id, cfg) = {
            let g = inner.lock().await;
            (g.meta.device_id.clone(), g.engine.config().clone())
        };
        let engaging = req.ghost == Some(false) && cfg.ghost;
        if engaging {
            self.send_mode(&device_id, &cfg).await?;
        }
        if req.ghost == Some(true) && !cfg.ghost {
            // Back to a dry run: stop holding the device.
            self.release_device(&inner).await;
        }
        let mut entries = Vec::new();
        let (snapshot, ghost) = {
            let mut g = inner.lock().await;
            if let Some(ghost) = req.ghost.filter(|v| *v != g.engine.config().ghost) {
                entries.push(g.engine.set_ghost(ghost));
            }
            if let Some(e) = g
                .engine
                .reconfigure(req.align, req.trigger, req.lookahead_seconds)
            {
                entries.push(e);
            }
            if let Some(on) = req.auto_drop
                && let Some(e) = g.engine.set_auto_drop(on).map_err(ReplayError::Invalid)?
            {
                entries.push(e);
            }
            (g.snapshot(), g.engine.config().ghost)
        };
        let snapshot = if engaging {
            self.refresh_lease(&inner, true).await;
            inner.lock().await.snapshot()
        } else {
            snapshot
        };
        if req.ghost.is_some() {
            let _ = sqlx::query("UPDATE replay_runs SET ghost = ? WHERE id = ?")
                .bind(ghost)
                .bind(&snapshot.run_id)
                .execute(&self.db)
                .await;
        }
        self.persist(&snapshot.run_id, session_id, ghost, &entries)
            .await;
        let _ = self.events_tx.send(snapshot.clone());
        Ok(snapshot)
    }

    /// Pause (`paused = true`) or resume channels; `None` means all three.
    pub async fn set_paused(
        &self,
        session_id: &str,
        channel: Option<Channel>,
        paused: bool,
        reason: &str,
        value: Option<f64>,
    ) -> Result<ReplayState, ReplayError> {
        let inner = self
            .run_inner(session_id)
            .await
            .ok_or(ReplayError::NotRunning)?;
        let channels: Vec<Channel> = channel.map_or(Channel::ALL.to_vec(), |c| vec![c]);
        // Resuming the setpoint or the heater hands the BT loop back to the
        // drive: put the roaster back in the drive's mode first (a Manual or
        // Auto pressed by hand paused them and left it in the other).
        if !paused && channels.iter().any(|c| *c != Channel::Fan) {
            let (device_id, cfg, was_paused) = {
                let g = inner.lock().await;
                let was = channels
                    .iter()
                    .any(|c| *c != Channel::Fan && g.engine.paused(*c));
                (g.meta.device_id.clone(), g.engine.config().clone(), was)
            };
            if was_paused && !cfg.ghost {
                self.send_mode(&device_id, &cfg).await?;
            }
        }
        let (snapshot, entries, ghost) = {
            let mut g = inner.lock().await;
            let entries: Vec<LogEntry> = channels
                .iter()
                .filter_map(|c| {
                    if paused {
                        g.engine.pause(*c, reason, value)
                    } else {
                        g.engine.resume(*c)
                    }
                })
                .collect();
            (g.snapshot(), entries, g.engine.config().ghost)
        };
        self.persist(&snapshot.run_id, session_id, ghost, &entries)
            .await;
        let _ = self.events_tx.send(snapshot.clone());
        Ok(snapshot)
    }

    /// A manual control command reached the device: pause the matching
    /// channels of its live (non-ghost) run. `value` is the operator's value
    /// (a manual setpoint keeps steering a server-loop run).
    pub async fn manual_override(
        &self,
        device_id: &str,
        channels: &[Channel],
        what: &str,
        value: Option<f64>,
    ) {
        let target = {
            let runs = self.runs.lock().await;
            runs.iter()
                .find(|(_, r)| r.device_id == device_id)
                .map(|(id, r)| (id.clone(), r.inner.clone()))
        };
        let Some((session_id, inner)) = target else {
            return;
        };
        if inner.lock().await.engine.config().ghost {
            return;
        }
        for ch in channels {
            // A run whose fan channel is there only for the fan assist takes
            // the operator's fan as its new ceiling instead of pausing.
            if let (Channel::Fan, Some(v)) = (ch, value) {
                let entry = {
                    let mut g = inner.lock().await;
                    g.engine.set_fan_ceiling(v).then(|| {
                        g.engine.entry(
                            LogKind::Assist,
                            Some(Channel::Fan),
                            Some(v),
                            Some(format!("manual fan {v:.0} %: new fan assist ceiling")),
                        )
                    })
                };
                if let Some(e) = entry {
                    let (run_id, ghost) = {
                        let g = inner.lock().await;
                        (g.meta.run_id.clone(), g.engine.config().ghost)
                    };
                    self.persist(&run_id, &session_id, ghost, &[e]).await;
                    continue;
                }
            }
            let _ = self
                .set_paused(
                    &session_id,
                    Some(*ch),
                    true,
                    &format!("manual {what} command"),
                    value,
                )
                .await;
        }
    }

    /// The live (non-ghost) run driving a device, if any.
    async fn live_run_for(&self, device_id: &str) -> Option<Arc<Mutex<RunInner>>> {
        let inners: Vec<_> = self
            .runs
            .lock()
            .await
            .values()
            .filter(|r| r.device_id == device_id)
            .map(|r| r.inner.clone())
            .collect();
        for i in inners {
            let g = i.lock().await;
            if g.meta.active && !g.engine.config().ghost {
                drop(g);
                return Some(i);
            }
        }
        None
    }

    /// The live drive on a device as the prediction plans with it, and its profile's name.
    pub async fn plan_ahead(
        &self,
        device_id: &str,
        n: usize,
    ) -> Option<(engine::PlanAhead, String)> {
        let inner = self.live_run_for(device_id).await?;
        let g = inner.lock().await;
        Some((g.engine.plan_ahead(n), g.meta.profile_name.clone()))
    }

    /// A live replay is driving the device (the standalone fan assist
    /// stands aside).
    pub async fn drives_device(&self, device_id: &str) -> bool {
        self.live_run_for(device_id).await.is_some()
    }

    /// `Some(view)` while a live replay drives the device; the inner value
    /// is the run's fan assist, when it runs one.
    pub async fn fan_assist_view(&self, device_id: &str) -> Option<Option<AssistView>> {
        let inner = self.live_run_for(device_id).await?;
        let v = inner.lock().await.engine.view().fan_assist;
        Some(v)
    }

    /// Fan assist settings changed: every run on the device follows.
    pub async fn set_fan_assist(&self, device_id: &str, cfg: FanAssistConfig) {
        let inners: Vec<_> = self
            .runs
            .lock()
            .await
            .values()
            .filter(|r| r.device_id == device_id)
            .map(|r| r.inner.clone())
            .collect();
        for i in inners {
            let (entry, run_id, session_id, ghost) = {
                let mut g = i.lock().await;
                if !g.engine.config().mode.closed_loop() {
                    continue;
                }
                let e = g.engine.set_fan_assist(cfg.clone());
                (
                    e,
                    g.meta.run_id.clone(),
                    g.meta.session_id.clone(),
                    g.engine.config().ghost,
                )
            };
            if let Some(e) = entry {
                self.persist(&run_id, &session_id, ghost, &[e]).await;
            }
        }
    }

    pub async fn event_op(
        &self,
        session_id: &str,
        event_id: &str,
        op: EventOp,
        seconds: Option<f64>,
    ) -> Result<ReplayState, ReplayError> {
        let inner = self
            .run_inner(session_id)
            .await
            .ok_or(ReplayError::NotRunning)?;
        let (snapshot, entry, ghost) = {
            let mut g = inner.lock().await;
            let entry = match op {
                EventOp::Skip => Some(g.engine.skip(event_id)),
                EventOp::Delay => Some(g.engine.delay(event_id, seconds.unwrap_or(15.0))),
                EventOp::Fire => g.engine.fire(event_id).err().map(Err),
            }
            .transpose()
            .map_err(ReplayError::Invalid)?;
            (g.snapshot(), entry, g.engine.config().ghost)
        };
        if let Some(e) = entry {
            self.persist(&snapshot.run_id, session_id, ghost, &[e])
                .await;
        }
        let _ = self.events_tx.send(snapshot.clone());
        Ok(snapshot)
    }

    async fn persist(&self, run_id: &str, session_id: &str, ghost: bool, entries: &[LogEntry]) {
        for e in entries {
            let res = sqlx::query(
                "INSERT INTO replay_log (run_id, session_id, at, elapsed, live_t, profile_t, kind, channel, event_id, action, value, label, detail, ghost)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(run_id)
            .bind(session_id)
            .bind(Utc::now())
            .bind(e.elapsed)
            .bind(e.live_t)
            .bind(e.profile_t)
            .bind(e.kind.as_str())
            .bind(e.channel.map(engine::Channel::as_str))
            .bind(&e.event_id)
            .bind(e.action.and_then(|a| {
                serde_json::to_value(a)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
            }))
            .bind(e.value)
            .bind(&e.label)
            .bind(&e.detail)
            .bind(ghost)
            .execute(&self.db)
            .await;
            if let Err(err) = res {
                tracing::warn!(error = %err, session_id, "could not write replay log");
            }
        }
    }

    /// Runs and log entries for a session, oldest first.
    pub async fn log(&self, session_id: &str) -> Result<serde_json::Value, ReplayError> {
        let runs = sqlx::query(
            "SELECT id, profile_id, profile_name, mode, trigger_policy, align_on, ghost, config, started_at, ended_at, end_reason
             FROM replay_runs WHERE session_id = ? ORDER BY started_at",
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| ReplayError::Db(e.to_string()))?;
        let runs: Vec<serde_json::Value> = runs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<String, _>("id"),
                    "profile_id": r.get::<Option<String>, _>("profile_id"),
                    "profile_name": r.get::<Option<String>, _>("profile_name"),
                    "mode": r.get::<String, _>("mode"),
                    "trigger": r.get::<String, _>("trigger_policy"),
                    "align": r.get::<String, _>("align_on"),
                    "ghost": r.get::<bool, _>("ghost"),
                    "config": serde_json::from_str::<serde_json::Value>(&r.get::<String, _>("config")).unwrap_or_default(),
                    "started_at": r.get::<DateTime<Utc>, _>("started_at"),
                    "ended_at": r.get::<Option<DateTime<Utc>>, _>("ended_at"),
                    "end_reason": r.get::<Option<String>, _>("end_reason"),
                })
            })
            .collect();
        let entries = sqlx::query(
            "SELECT run_id, at, elapsed, live_t, profile_t, kind, channel, event_id, action, value, label, detail, ghost
             FROM replay_log WHERE session_id = ? ORDER BY id",
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| ReplayError::Db(e.to_string()))?;
        let entries: Vec<serde_json::Value> = entries
            .iter()
            .map(|r| {
                serde_json::json!({
                    "run_id": r.get::<String, _>("run_id"),
                    "at": r.get::<DateTime<Utc>, _>("at"),
                    "elapsed": r.get::<f64, _>("elapsed"),
                    "live_t": r.get::<f64, _>("live_t"),
                    "profile_t": r.get::<f64, _>("profile_t"),
                    "kind": r.get::<String, _>("kind"),
                    "channel": r.get::<Option<String>, _>("channel"),
                    "event_id": r.get::<Option<String>, _>("event_id"),
                    "action": r.get::<Option<String>, _>("action"),
                    "value": r.get::<Option<f64>, _>("value"),
                    "label": r.get::<Option<String>, _>("label"),
                    "detail": r.get::<Option<String>, _>("detail"),
                    "ghost": r.get::<bool, _>("ghost"),
                })
            })
            .collect();
        Ok(serde_json::json!({ "session_id": session_id, "runs": runs, "entries": entries }))
    }
}

/// The landmarks marked on a roast, the first of each kind.
fn marks_from(events: &[crate::roasts::types::RoastEvent]) -> BTreeMap<AlignOn, Mark> {
    let mut marks = BTreeMap::new();
    for e in events {
        if let Some(on) = AlignOn::from_event_type(&e.event_type) {
            marks.entry(on).or_insert(Mark {
                elapsed: f64::from(e.elapsed_seconds),
                bt: e.temperature.map(f64::from),
            });
        }
    }
    marks
}
