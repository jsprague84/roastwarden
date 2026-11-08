//! Prediction from the roaster's own model (docs/prediction.md).
//!
//! Each roaster gets a thermal model fitted from its completed roasts (`model`), refitted
//! when a roast completes. While a roast runs between CHARGE + 30 s and DROP the service
//! predicts BT for the next two minutes under the plan, first crack and DROP (`live`), every
//! 5 s, and keeps what it said every 15 s with the BT that came (`roast_predictions`), so
//! each roast scores the prediction against today's projection.

pub mod live;
pub mod model;
pub mod ror_watch;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tokio::sync::{Mutex, RwLock};
use utoipa::ToSchema;

use crate::analysis::telemetry::TelemetryService;
use crate::control::schedule::GainScheduler;
use crate::replay::auto_drop::{AutoDrop, DropBy};
use crate::replay::engine::{LoopAt, PidGains};
use crate::replay::ReplayService;
use crate::roasts::types::{RoastEventType, SessionStatus};
use live::{DropTarget, FcHistory, LiveInput, Plan, PlanKind, Prediction};
use model::{FitQuality, HorizonError, ThermalModel, Tick};
use ror_watch::{RorWarning, RorWarningKind};

/// How often the live prediction runs, and how often a fit is checked against the roasts.
const TICK: Duration = Duration::from_secs(5);
const REFIT_EVERY: Duration = Duration::from_secs(300);
/// A prediction is kept with the roast this often (s of roast time).
const KEEP_EVERY_S: f64 = 15.0;
/// The newest roasts a fit uses.
const FIT_ROASTS: i64 = 30;
/// Fits kept per roaster.
const KEEP_FITS: i64 = 20;
/// The device PID's gains are in 0..255 output units; the prediction's PID in heater %.
const DEVICE_UNITS: f64 = 2.55;
/// An RoR warning counts (an alarm, kept with the roast) once two predictions in a row gave
/// it, s.
pub const WARN_HOLD_S: f64 = 4.0;

/// A stored fit.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StoredModel {
    pub id: String,
    pub device_id: String,
    pub fitted_at: String,
    pub model: ThermalModel,
    pub quality: FitQuality,
    pub roasts: usize,
    pub samples: usize,
    /// The roasts it was fitted on
    pub session_ids: Vec<String>,
}

/// Where BT settles at one heater and fan, by the model.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Settle {
    pub heater_pct: f64,
    pub fan_pct: f64,
    /// °C; null when above 300 °C
    pub bt: Option<f64>,
}

/// A landmark's predicted time against when it came.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LandmarkScore {
    /// When it came, s since CHARGE
    pub actual_t: f64,
    /// What the prediction about two minutes before said, and when it said it
    pub predicted_t: f64,
    pub made_at_t: f64,
    /// Predicted minus actual, s
    pub error_s: f64,
    /// Mean absolute error of every prediction made before it, s
    pub mae_s: f64,
    pub n: usize,
}

/// How the live prediction did on one roast.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoastScore {
    pub session_id: String,
    /// Predictions kept
    pub predictions: usize,
    /// The model's BT at 60 and 120 s ahead against what came
    pub model: Vec<HorizonError>,
    /// Today's projection at the same moments
    pub projection: Vec<HorizonError>,
    /// The model's two-minute error is below the projection's
    pub beats_projection: Option<bool>,
    pub first_crack: Option<LandmarkScore>,
    pub drop: Option<LandmarkScore>,
    /// The RoR warnings given during the roast, each against what the RoR then did
    pub ror_warnings: Vec<RorWarningScore>,
    /// What the roast's RoR did, read afterwards as the watch reads it live (a crash, a flick
    /// after first crack, a stall before DROP); empty before DROP
    pub ror_events: Vec<RorWarning>,
}

/// An RoR warning given during a roast, and whether it came.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RorWarningScore {
    pub kind: RorWarningKind,
    /// When it was first given (held for two predictions), s since CHARGE
    pub warned_at: f64,
    /// When it said it would start then (or started, when seen)
    pub expected_t: f64,
    /// Seen in the RoR then; false: expected by the model
    pub seen: bool,
    pub reason: String,
    /// The last time it still stood
    pub last_t: f64,
    /// When the roast's RoR did it, read afterwards; null: it did not come
    pub came_t: Option<f64>,
    /// How long before it came the warning was given, s (negative: after)
    pub lead_s: Option<f64>,
}

/// One recent roast's score.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecentScore {
    pub name: String,
    pub batch_number: Option<i32>,
    #[serde(flatten)]
    pub score: RoastScore,
}

/// Every kept prediction of the recent roasts pooled.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PooledScore {
    pub roasts: usize,
    pub model_120: Option<HorizonError>,
    pub projection_120: Option<HorizonError>,
    pub beats_projection: Option<bool>,
}

/// A roaster's model: the fit in use (or the prior), where it settles, and how its live
/// predictions did on the recent roasts.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ModelView {
    pub device_id: String,
    /// `fit`, or `prior` (the virtual roaster's model) before the roaster has roasts
    pub source: String,
    pub model: ThermalModel,
    /// The stored fit; null for the prior
    pub fit: Option<StoredModel>,
    pub settles: Vec<Settle>,
    pub recent: Vec<RecentScore>,
    pub pooled: PooledScore,
}

pub struct PredictService {
    db: SqlitePool,
    telemetry: TelemetryService,
    replay: Arc<ReplayService>,
    scheduler: Arc<GainScheduler>,
    /// Newest fit per roaster (None: looked up, there is none)
    models: RwLock<HashMap<String, Option<StoredModel>>>,
    /// The latest live prediction per roaster
    live: RwLock<HashMap<String, Prediction>>,
    /// Roast time of the last kept prediction per roast
    kept: Mutex<HashMap<String, f64>>,
    /// The roasts each roaster's fit was last checked against
    checked: Mutex<HashMap<String, Vec<String>>>,
    fitting: Mutex<()>,
}

fn gains_of(kp: Option<f64>, ki: Option<f64>) -> Option<PidGains> {
    match (kp, ki) {
        (Some(kp), Some(ki)) if kp.is_finite() && ki.is_finite() => Some(PidGains {
            kp: kp / DEVICE_UNITS,
            ki: ki / DEVICE_UNITS,
            kd: 0.0,
        }),
        _ => None,
    }
}

fn tick_of_sample(s: &roastwarden_core::Sample, charge_ms: f64) -> Tick {
    let x = |k: &str| s.extras.get(k).copied().filter(|v| v.is_finite());
    Tick {
        t: (s.ts_ms as f64 - charge_ms) / 1000.0,
        bt: s.bt,
        heater: x("heaterApplied").or(s.heater),
        fan: s.fan,
        setpoint: s.setpoint,
        auto: s.mode == Some(1),
        ror: s.ror_bt,
        gains: gains_of(x("Kp"), x("Ki")),
    }
}

/// BT at `t` from ticks, interpolated; None outside them or across a gap over 5 s.
fn bt_at(ticks: &[Tick], t: f64) -> Option<f64> {
    let i = ticks.partition_point(|k| k.t < t);
    let b = ticks.get(i)?;
    if (b.t - t).abs() < 1e-9 {
        return b.bt;
    }
    let a = ticks.get(i.checked_sub(1)?)?;
    if b.t - a.t > 5.0 {
        return None;
    }
    let (ba, bb) = (a.bt?, b.bt?);
    Some(ba + (bb - ba) * (t - a.t) / (b.t - a.t))
}

fn settles(m: &ThermalModel) -> Vec<Settle> {
    [
        (80.0, 100.0),
        (100.0, 100.0),
        (90.0, 80.0),
        (100.0, 80.0),
        (100.0, 70.0),
    ]
    .iter()
    .map(|&(h, f)| Settle {
        heater_pct: h,
        fan_pct: f,
        bt: m.settles_at(h, f),
    })
    .collect()
}

impl PredictService {
    pub fn new(
        db: SqlitePool,
        telemetry: TelemetryService,
        replay: Arc<ReplayService>,
        scheduler: Arc<GainScheduler>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            telemetry,
            replay,
            scheduler,
            models: RwLock::new(HashMap::new()),
            live: RwLock::new(HashMap::new()),
            kept: Mutex::new(HashMap::new()),
            checked: Mutex::new(HashMap::new()),
            fitting: Mutex::new(()),
        })
    }

    pub async fn run(self: Arc<Self>) {
        let mut since_refit = REFIT_EVERY;
        loop {
            if since_refit >= REFIT_EVERY {
                since_refit = Duration::ZERO;
                if let Err(e) = self.refit_stale().await {
                    tracing::warn!(error = %e, "prediction: refit check failed");
                }
            }
            if let Err(e) = self.tick().await {
                tracing::debug!(error = %e, "prediction: tick failed");
            }
            tokio::time::sleep(TICK).await;
            since_refit += TICK;
        }
    }

    // ------------------------------------------------------------------ fits --

    /// The completed roasts a fit uses: newest first, with a DROP.
    async fn fit_roasts(&self, device_id: &str) -> Result<Vec<(String, f64, f64)>> {
        let rows = sqlx::query(
            "SELECT s.id,
                    COALESCE((SELECT MIN(elapsed_seconds) FROM roast_events
                              WHERE session_id = s.id AND event_type = 'charge'), 0.0) AS charge,
                    (SELECT MIN(elapsed_seconds) FROM roast_events
                     WHERE session_id = s.id AND event_type = 'drop') AS dropped
             FROM roast_sessions s
             WHERE s.device_id = ? AND s.status = ?
             ORDER BY s.start_time DESC LIMIT ?",
        )
        .bind(device_id)
        .bind(SessionStatus::Completed.to_string())
        .bind(FIT_ROASTS)
        .fetch_all(&self.db)
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let drop: Option<f64> = r.get("dropped");
                let charge: f64 = r.get("charge");
                drop.filter(|d| d - charge >= 120.0)
                    .map(|d| (r.get::<String, _>("id"), charge, d))
            })
            .collect())
    }

    async fn records(&self, roasts: &[(String, f64, f64)]) -> Result<Vec<model::Record>> {
        let mut out = Vec::new();
        for (id, charge, drop) in roasts {
            let rows = crate::roasts::telemetry_store::get_session_telemetry(&self.db, id).await?;
            let x = |e: &Option<serde_json::Value>, k: &str| {
                e.as_ref()
                    .and_then(|v| v.get(k))
                    .and_then(serde_json::Value::as_f64)
            };
            let ticks: Vec<Tick> = rows
                .iter()
                .filter(|r| f64::from(r.elapsed_seconds) >= *charge)
                .map(|r| Tick {
                    t: f64::from(r.elapsed_seconds) - charge,
                    bt: r.bean_temp.map(f64::from),
                    heater: x(&r.extras, "heaterApplied").or(r.heater),
                    fan: r.fan,
                    setpoint: r.setpoint.map(f64::from),
                    auto: x(&r.extras, "controlMode") == Some(1.0),
                    ror: r.rate_of_rise.map(f64::from),
                    gains: gains_of(x(&r.extras, "Kp"), x(&r.extras, "Ki")),
                })
                .collect();
            if let Some(r) = model::Record::from_ticks(id, &ticks, drop - charge) {
                out.push(r);
            }
        }
        Ok(out)
    }

    /// Fit a roaster's model now from its roasts and store it.
    pub async fn fit_now(&self, device_id: &str) -> Result<Option<StoredModel>> {
        let _one = self.fitting.lock().await;
        let roasts = self.fit_roasts(device_id).await?;
        let records = self.records(&roasts).await?;
        if records.is_empty() {
            self.models
                .write()
                .await
                .insert(device_id.to_string(), None);
            return Ok(None);
        }
        let ids: Vec<String> = records.iter().map(|r| r.session_id.clone()).collect();
        let fitted = tokio::task::spawn_blocking(move || model::fit(&records))
            .await
            .context("fit task")?;
        let stored = StoredModel {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            fitted_at: Utc::now().to_rfc3339(),
            roasts: fitted.quality.roasts,
            samples: fitted.quality.samples,
            model: fitted.model,
            quality: fitted.quality,
            session_ids: ids,
        };
        sqlx::query(
            "INSERT INTO roaster_models (id, device_id, fitted_at, model, quality, roasts, samples, session_ids)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&stored.id)
        .bind(device_id)
        .bind(&stored.fitted_at)
        .bind(serde_json::to_string(&stored.model)?)
        .bind(serde_json::to_string(&stored.quality)?)
        .bind(stored.roasts as i64)
        .bind(stored.samples as i64)
        .bind(serde_json::to_string(&stored.session_ids)?)
        .execute(&self.db)
        .await?;
        sqlx::query(
            "DELETE FROM roaster_models WHERE device_id = ? AND id NOT IN
             (SELECT id FROM roaster_models WHERE device_id = ? ORDER BY fitted_at DESC LIMIT ?)",
        )
        .bind(device_id)
        .bind(device_id)
        .bind(KEEP_FITS)
        .execute(&self.db)
        .await?;
        tracing::info!(
            device_id,
            roasts = stored.roasts,
            tau_s = stored.model.tau_s,
            "prediction: fitted the roaster's model"
        );
        self.models
            .write()
            .await
            .insert(device_id.to_string(), Some(stored.clone()));
        Ok(Some(stored))
    }

    /// Refit each roaster whose roasts changed since its fit (a roast completed or deleted).
    async fn refit_stale(&self) -> Result<()> {
        let devices: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT device_id FROM roast_sessions WHERE status = ?")
                .bind(SessionStatus::Completed.to_string())
                .fetch_all(&self.db)
                .await?;
        for d in devices {
            let mut want: Vec<String> = self
                .fit_roasts(&d)
                .await?
                .into_iter()
                .map(|r| r.0)
                .collect();
            want.sort();
            if self.checked.lock().await.get(&d) == Some(&want) {
                continue;
            }
            let mut got = self
                .model(&d)
                .await
                .map(|m| m.session_ids)
                .unwrap_or_default();
            got.sort();
            // A roast whose telemetry is too short is left out of a fit, so the fit's list
            // can differ from the candidates: refit only when the candidates change.
            if want != got {
                if let Err(e) = self.fit_now(&d).await {
                    tracing::warn!(error = %e, device_id = d, "prediction: fit failed");
                }
            }
            self.checked.lock().await.insert(d, want);
        }
        Ok(())
    }

    /// The newest fit of a roaster, if it has one.
    pub async fn model(&self, device_id: &str) -> Option<StoredModel> {
        if let Some(m) = self.models.read().await.get(device_id) {
            return m.clone();
        }
        let m = self.load_model(device_id).await.ok().flatten();
        self.models
            .write()
            .await
            .insert(device_id.to_string(), m.clone());
        m
    }

    async fn load_model(&self, device_id: &str) -> Result<Option<StoredModel>> {
        let Some(r) = sqlx::query(
            "SELECT * FROM roaster_models WHERE device_id = ? ORDER BY fitted_at DESC LIMIT 1",
        )
        .bind(device_id)
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(StoredModel {
            id: r.get("id"),
            device_id: r.get("device_id"),
            fitted_at: r.get("fitted_at"),
            model: serde_json::from_str(r.get("model"))?,
            quality: serde_json::from_str(r.get("quality"))?,
            roasts: r.get::<i64, _>("roasts") as usize,
            samples: r.get::<i64, _>("samples") as usize,
            session_ids: serde_json::from_str(r.get("session_ids"))?,
        }))
    }

    pub async fn view(&self, device_id: &str) -> Result<ModelView> {
        let fit = self.model(device_id).await;
        let model = fit
            .as_ref()
            .map_or_else(ThermalModel::prior, |m| m.model.clone());
        let recent = self.recent_scores(device_id, 10).await?;
        let pooled = pool(&self.db, &recent).await?;
        Ok(ModelView {
            device_id: device_id.to_string(),
            source: if fit.is_some() { "fit" } else { "prior" }.into(),
            settles: settles(&model),
            model,
            fit,
            recent,
            pooled,
        })
    }

    // ------------------------------------------------------------------ live --

    pub async fn live(&self, device_id: &str) -> Option<Prediction> {
        self.live.read().await.get(device_id).cloned()
    }

    async fn tick(&self) -> Result<()> {
        let active: Vec<(String, String)> =
            sqlx::query_as("SELECT id, device_id FROM roast_sessions WHERE status = ?")
                .bind(SessionStatus::Active.to_string())
                .fetch_all(&self.db)
                .await?;
        let mut seen = Vec::new();
        for (session_id, device_id) in active {
            match self.predict_roast(&session_id, &device_id).await {
                Ok(Some(mut p)) => {
                    seen.push(device_id.clone());
                    let mut live = self.live.write().await;
                    // A warning the last prediction gave too keeps its start.
                    if let Some(prev) = live.get(&device_id) {
                        for w in &mut p.ror.warnings {
                            if let Some(o) = prev.ror.warning(w.kind) {
                                w.since = o.since;
                            }
                        }
                    }
                    live.insert(device_id, p.clone());
                    drop(live);
                    if let Err(e) = self.keep_warnings(&session_id, &p).await {
                        tracing::debug!(error = %e, session_id, "keeping RoR warnings failed");
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::debug!(error = %e, session_id, "prediction failed"),
            }
        }
        self.live.write().await.retain(|d, _| seen.contains(d));
        Ok(())
    }

    async fn predict_roast(&self, session_id: &str, device_id: &str) -> Result<Option<Prediction>> {
        let Some(session) = crate::roasts::sessions::get_session(&self.db, session_id).await?
        else {
            return Ok(None);
        };
        let events = crate::roasts::events::get_roast_events(&self.db, session_id).await?;
        let mark = |ty: RoastEventType| {
            events
                .iter()
                .filter(|e| e.event_type == ty)
                .map(|e| f64::from(e.elapsed_seconds))
                .reduce(f64::min)
        };
        let (Some(start), Some(charge)) = (session.start_time, mark(RoastEventType::Charge)) else {
            return Ok(None);
        };
        if mark(RoastEventType::Drop).is_some() {
            return Ok(None);
        }
        let charge_ms = start.timestamp_millis() as f64 + charge * 1000.0;
        let samples = self
            .telemetry
            .samples_since_ms(device_id, charge_ms.max(0.0) as u64, 20_000);
        let ticks: Vec<Tick> = samples
            .iter()
            .map(|s| tick_of_sample(s, charge_ms))
            .collect();
        let Some(last) = ticks.last().copied() else {
            return Ok(None);
        };
        if last.t < model::START_AFTER_CHARGE_S as f64 {
            return Ok(None);
        }
        let fit = self.model(device_id).await;
        let (m, quality) = match &fit {
            Some(f) => (f.model.clone(), f.quality.clone()),
            None => (
                ThermalModel::prior(),
                FitQuality {
                    roasts: 0,
                    samples: 0,
                    modes: Vec::new(),
                },
            ),
        };
        let fan_now = last.fan.unwrap_or(0.0);
        let heater_now = last.heater.unwrap_or(0.0);
        let device_gains = match last.gains {
            Some(g) => Some(g),
            None => self
                .scheduler
                .held_gains(device_id, last.fan)
                .await
                .and_then(|g| gains_of(Some(g.kp), Some(g.ki))),
        };
        let fc_marked = mark(RoastEventType::FirstCrackStart).map(|t| t - charge);
        let (plan, offset) = match self.replay.plan_ahead(device_id, 900).await {
            Some((a, profile)) => {
                let label = match (&a.setpoints, &a.heater) {
                    (Some(_), _) => format!(
                        "{profile}'s setpoints ({:.0} s ahead), fan {}",
                        a.lookahead_s,
                        if a.fan.is_some() {
                            "from the profile".to_string()
                        } else {
                            format!("held at {fan_now:.0} %")
                        }
                    ),
                    (None, Some(_)) => format!("{profile}'s heater curve"),
                    _ => format!("heater {heater_now:.0} % and fan {fan_now:.0} % held"),
                };
                let gains = if a.loop_at == LoopAt::Server {
                    Some(a.server_gains)
                } else {
                    device_gains
                };
                (
                    Plan {
                        kind: PlanKind::Drive,
                        targets: Some(a.targets),
                        heater: a.heater.unwrap_or_else(|| vec![heater_now]),
                        fan: a.fan.unwrap_or_else(|| vec![fan_now]),
                        setpoints: a.setpoints.filter(|_| gains.is_some()),
                        gains,
                        label,
                    },
                    a.offset_s,
                )
            }
            None => match (last.auto, last.setpoint, device_gains) {
                (true, Some(sp), Some(g)) => (
                    Plan {
                        kind: PlanKind::Setpoint,
                        setpoints: Some(vec![sp]),
                        targets: Some(vec![sp]),
                        heater: vec![heater_now],
                        fan: vec![fan_now],
                        gains: Some(g),
                        label: format!(
                            "the roaster's PID holding {sp:.1} °C, fan held at {fan_now:.0} %"
                        ),
                    },
                    0.0,
                ),
                _ => (
                    Plan {
                        kind: PlanKind::Held,
                        setpoints: None,
                        targets: None,
                        heater: vec![heater_now],
                        fan: vec![fan_now],
                        gains: None,
                        label: format!("heater {heater_now:.0} % and fan {fan_now:.0} % held"),
                    },
                    0.0,
                ),
            },
        };
        let profile = match session.profile_id.as_deref() {
            Some(pid) => crate::profiles::store::get_profile_with_points(&self.db, pid).await?,
            None => None,
        };
        let fc_history = match fc_history(
            &self.db,
            device_id,
            Some(session_id),
            session.bean_id.as_deref(),
            session.profile_id.as_deref(),
        )
        .await?
        {
            Some(h) => Some(h),
            None => profile.as_ref().and_then(profile_fc),
        };
        let drop_target = profile.as_ref().and_then(|p| drop_target(p, offset));
        let Some(p) = live::predict(&LiveInput {
            model: &m,
            quality: &quality,
            ticks: &ticks,
            plan,
            fc_history,
            fc_marked,
            dry_marked: mark(RoastEventType::DryingEnd).is_some(),
            drop_target,
        }) else {
            return Ok(None);
        };
        self.keep(session_id, fit.as_ref().map(|f| f.id.as_str()), &p, &ticks)
            .await?;
        Ok(Some(p))
    }

    /// Keep the prediction with the roast every 15 s, and fill in BT for the kept ones whose
    /// time has come.
    async fn keep(
        &self,
        session_id: &str,
        model_id: Option<&str>,
        p: &Prediction,
        ticks: &[Tick],
    ) -> Result<()> {
        let due = {
            let mut kept = self.kept.lock().await;
            let last = kept.get(session_id).copied();
            let due = last.is_none_or(|l| p.t - l >= KEEP_EVERY_S - 0.5);
            if due {
                kept.insert(session_id.to_string(), p.t);
            }
            due
        };
        if due {
            sqlx::query(
                "INSERT OR IGNORE INTO roast_predictions
                 (session_id, model_id, t, plan, bt, pred_60, pred_120, proj_60, proj_120, fc_t, drop_t)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(session_id)
            .bind(model_id)
            .bind(p.t)
            .bind(serde_json::to_value(p.plan)?.as_str().unwrap_or("held").to_string())
            .bind(p.bt)
            .bind(p.bt_60)
            .bind(p.bt_120)
            .bind(p.projection_60)
            .bind(p.projection_120)
            .bind(p.first_crack.as_ref().map(|l| l.t))
            .bind(p.drop.as_ref().map(|l| l.t))
            .execute(&self.db)
            .await?;
        }
        let pending: Vec<(i64, f64, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT id, t, actual_60, actual_120 FROM roast_predictions
             WHERE session_id = ? AND ((actual_60 IS NULL AND t + 60 <= ?)
                                    OR (actual_120 IS NULL AND t + 120 <= ?))",
        )
        .bind(session_id)
        .bind(p.t)
        .bind(p.t)
        .fetch_all(&self.db)
        .await?;
        for (id, t, a60, a120) in pending {
            let v60 = a60.or_else(|| bt_at(ticks, t + 60.0));
            let v120 = a120.or_else(|| bt_at(ticks, t + 120.0));
            sqlx::query("UPDATE roast_predictions SET actual_60 = ?, actual_120 = ? WHERE id = ?")
                .bind(v60)
                .bind(v120)
                .bind(id)
                .execute(&self.db)
                .await?;
        }
        Ok(())
    }

    /// Keep each kind of RoR warning with the roast once it holds (two predictions in a row):
    /// when it was first given, what it said then, and the last time it still stood.
    async fn keep_warnings(&self, session_id: &str, p: &Prediction) -> Result<()> {
        for w in &p.ror.warnings {
            if p.t - w.since < WARN_HOLD_S {
                continue;
            }
            sqlx::query(
                "INSERT INTO roast_ror_warnings
                 (session_id, kind, first_t, expected_t, seen, reason, last_t)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (session_id, kind) DO UPDATE SET last_t = excluded.last_t",
            )
            .bind(session_id)
            .bind(w.kind.as_str())
            .bind(p.t)
            .bind(w.t)
            .bind(w.seen)
            .bind(&w.reason)
            .bind(p.t)
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- scores --

    /// How the live prediction did on one roast.
    pub async fn score(&self, session_id: &str) -> Result<RoastScore> {
        score(&self.db, session_id).await
    }

    async fn recent_scores(&self, device_id: &str, limit: i64) -> Result<Vec<RecentScore>> {
        let rows: Vec<(String, String, Option<i32>)> = sqlx::query_as(
            "SELECT s.id, s.name, s.batch_number FROM roast_sessions s
             WHERE s.device_id = ? AND s.status = ?
               AND EXISTS (SELECT 1 FROM roast_predictions p WHERE p.session_id = s.id)
             ORDER BY s.start_time DESC LIMIT ?",
        )
        .bind(device_id)
        .bind(SessionStatus::Completed.to_string())
        .bind(limit)
        .fetch_all(&self.db)
        .await?;
        let mut out = Vec::new();
        for (id, name, batch_number) in rows {
            out.push(RecentScore {
                score: score(&self.db, &id).await?,
                name,
                batch_number,
            });
        }
        Ok(out)
    }
}

/// First crack's BT on this roaster: the same bean on the same profile, else the
/// profile, else the bean, else every roast. `leave_out` is the roast being predicted (the
/// new bean guide, `bean_guide.rs`, leaves none out).
pub async fn fc_history(
    db: &SqlitePool,
    device_id: &str,
    leave_out: Option<&str>,
    bean_id: Option<&str>,
    profile_id: Option<&str>,
) -> Result<Option<FcHistory>> {
    let rows = sqlx::query(
        "SELECT s.bean_id, s.profile_id, b.name AS bean, p.name AS profile, e.temperature
         FROM roast_sessions s
         JOIN roast_events e ON e.session_id = s.id AND e.event_type = 'first_crack_start'
         LEFT JOIN beans b ON b.id = s.bean_id
         LEFT JOIN roast_profiles p ON p.id = s.profile_id
         WHERE s.device_id = ? AND s.status = ? AND s.id IS NOT ? AND e.temperature IS NOT NULL
         ORDER BY s.start_time DESC LIMIT 50",
    )
    .bind(device_id)
    .bind(SessionStatus::Completed.to_string())
    .bind(leave_out)
    .fetch_all(db)
    .await?;
    struct Fc {
        bean_id: Option<String>,
        profile_id: Option<String>,
        bean: Option<String>,
        profile: Option<String>,
        bt: f64,
    }
    let all: Vec<Fc> = rows
        .iter()
        .map(|r| Fc {
            bean_id: r.get("bean_id"),
            profile_id: r.get("profile_id"),
            bean: r.get("bean"),
            profile: r.get("profile"),
            bt: r.get::<f64, _>("temperature"),
        })
        .collect();
    let same = |a: &Option<String>, b: Option<&str>| b.is_some() && a.as_deref() == b;
    let tiers: [(Vec<&Fc>, usize); 4] = [
        (
            all.iter()
                .filter(|f| same(&f.bean_id, bean_id) && same(&f.profile_id, profile_id))
                .collect(),
            1,
        ),
        (
            all.iter()
                .filter(|f| same(&f.profile_id, profile_id))
                .collect(),
            2,
        ),
        (
            all.iter().filter(|f| same(&f.bean_id, bean_id)).collect(),
            2,
        ),
        (all.iter().collect(), 2),
    ];
    for (k, (fcs, min)) in tiers.iter().enumerate() {
        if fcs.len() < *min {
            continue;
        }
        let n = fcs.len() as f64;
        let mean = fcs.iter().map(|f| f.bt).sum::<f64>() / n;
        let sd = if fcs.len() > 1 {
            (fcs.iter().map(|f| (f.bt - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
        } else {
            1.5
        };
        let roasts = |n: usize| {
            if n == 1 {
                "1 roast".to_string()
            } else {
                format!("{n} roasts")
            }
        };
        let f0 = fcs[0];
        let name = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
        let basis = match k {
            0 => format!(
                "{} of {} on {}",
                roasts(fcs.len()),
                name(&f0.bean),
                name(&f0.profile)
            ),
            1 => format!("{} on {}", roasts(fcs.len()), name(&f0.profile)),
            2 => format!("{} of {}", roasts(fcs.len()), name(&f0.bean)),
            _ => format!("{} on this roaster", roasts(fcs.len())),
        };
        return Ok(Some(FcHistory {
            bt_mean: (mean * 10.0).round() / 10.0,
            bt_sd: (sd * 10.0).round() / 10.0,
            roasts: fcs.len(),
            basis,
        }));
    }
    Ok(None)
}

/// First crack from the profile when no roast on this roaster has marked one: its BT at the
/// profile's first crack time.
pub fn profile_fc(p: &crate::profiles::types::ProfileWithPoints) -> Option<FcHistory> {
    let fc = f64::from(p.profile.target_first_crack?);
    let mut pts: Vec<(f64, f64)> = p
        .points
        .iter()
        .map(|x| (f64::from(x.time_seconds), f64::from(x.target_temp)))
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let bt = crate::replay::engine::interpolate(&pts, fc)?;
    Some(FcHistory {
        bt_mean: (bt * 10.0).round() / 10.0,
        bt_sd: 2.0,
        roasts: 0,
        basis: format!(
            "{}'s first crack (none marked on this roaster yet)",
            p.profile.name
        ),
    })
}

/// The profile's drop: its auto DROP when on, else its development (drop minus first crack),
/// else its drop BT. `offset` is live minus profile time (a drive's alignment).
fn drop_target(
    p: &crate::profiles::types::ProfileWithPoints,
    offset: f64,
) -> Option<(DropTarget, String)> {
    let p = &p.profile;
    if let Ok(Some(a)) = AutoDrop::from_profile(p, 0.0) {
        let target = match a.by {
            DropBy::Time => DropTarget::Time(a.at + offset),
            DropBy::Bt => DropTarget::Bt(a.at),
            DropBy::Development => DropTarget::Development(a.at),
        };
        return Some((target, format!("auto DROP {}", a.describe())));
    }
    if let (Some(total), Some(fc)) = (p.target_total_time, p.target_first_crack) {
        if total > fc {
            let dev = f64::from(total - fc);
            return Some((
                DropTarget::Development(dev),
                format!(
                    "the profile's development, {} after first crack",
                    live::mmss(dev)
                ),
            ));
        }
    }
    p.target_end_temp.map(|c| {
        (
            DropTarget::Bt(f64::from(c)),
            format!("the profile's drop BT, {c:.1} °C"),
        )
    })
}

/// A kept prediction's model and projection BT at 60 and 120 s, and the BT that came.
type KeptRow = (f64, f64, Option<f64>, Option<f64>, Option<f64>, Option<f64>);
/// A kept prediction's time, first crack and DROP.
type LandmarkRow = (f64, Option<f64>, Option<f64>);

/// Pairs of (predicted, actual) at 60 and 120 s for the model and the projection.
async fn errors(db: &SqlitePool, session_id: &str) -> Result<[Vec<f64>; 4]> {
    let rows: Vec<KeptRow> = sqlx::query_as(
        "SELECT pred_60, pred_120, proj_60, proj_120, actual_60, actual_120
         FROM roast_predictions WHERE session_id = ? ORDER BY t",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;
    let mut e: [Vec<f64>; 4] = Default::default();
    for (p60, p120, j60, j120, a60, a120) in rows {
        if let Some(a) = a60 {
            e[0].push(p60 - a);
            if let Some(j) = j60 {
                e[2].push(j - a);
            }
        }
        if let Some(a) = a120 {
            e[1].push(p120 - a);
            if let Some(j) = j120 {
                e[3].push(j - a);
            }
        }
    }
    Ok(e)
}

fn horizons(e60: &[f64], e120: &[f64]) -> Vec<HorizonError> {
    [(60, e60), (120, e120)]
        .iter()
        .filter(|(_, e)| !e.is_empty())
        .map(|(h, e)| model::summarize(*h, e))
        .collect()
}

fn beats(model: &[HorizonError], proj: &[HorizonError]) -> Option<bool> {
    let m = model.iter().find(|h| h.horizon_s == 120)?;
    let p = proj.iter().find(|h| h.horizon_s == 120)?;
    Some(m.rmse < p.rmse)
}

pub async fn score(db: &SqlitePool, session_id: &str) -> Result<RoastScore> {
    let e = errors(db, session_id).await?;
    let rows: Vec<LandmarkRow> = sqlx::query_as(
        "SELECT t, fc_t, drop_t FROM roast_predictions WHERE session_id = ? ORDER BY t",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;
    let ev: Vec<(String, f64)> = sqlx::query_as(
        "SELECT event_type, CAST(elapsed_seconds AS REAL) FROM roast_events WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;
    let first = |ty: &str| {
        ev.iter()
            .filter(|(t, _)| t == ty)
            .map(|(_, v)| *v)
            .reduce(f64::min)
    };
    let charge = first("charge").unwrap_or(0.0);
    let landmark = |actual: Option<f64>, pick: &dyn Fn(&LandmarkRow) -> Option<f64>| {
        let actual = actual? - charge;
        let before: Vec<(f64, f64)> = rows
            .iter()
            .filter(|r| r.0 < actual)
            .filter_map(|r| pick(r).map(|p| (r.0, p)))
            .collect();
        let made = before
            .iter()
            .filter(|(t, _)| *t <= actual - 60.0)
            .min_by(|a, b| {
                ((a.0 - (actual - 120.0)).abs()).total_cmp(&(b.0 - (actual - 120.0)).abs())
            })?;
        let mae = before.iter().map(|(_, p)| (p - actual).abs()).sum::<f64>() / before.len() as f64;
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        Some(LandmarkScore {
            actual_t: r1(actual),
            predicted_t: made.1,
            made_at_t: made.0,
            error_s: r1(made.1 - actual),
            mae_s: r1(mae),
            n: before.len(),
        })
    };
    let model = horizons(&e[0], &e[1]);
    let projection = horizons(&e[2], &e[3]);
    let ror_events = match first("drop") {
        Some(drop) => {
            let tele: Vec<(f64, Option<f64>)> = sqlx::query_as(
                "SELECT CAST(elapsed_seconds AS REAL), bean_temp FROM session_telemetry
                 WHERE session_id = ? ORDER BY elapsed_seconds",
            )
            .bind(session_id)
            .fetch_all(db)
            .await?;
            let ticks: Vec<Tick> = tele
                .into_iter()
                .filter(|(t, _)| *t >= charge - 30.0)
                .map(|(t, bt)| Tick {
                    t: t - charge,
                    bt,
                    ..Tick::default()
                })
                .collect();
            ror_watch::events(
                &ticks,
                first("drying_end").map(|t| t - charge),
                first("first_crack_start").map(|t| t - charge),
                drop - charge,
                None,
            )
        }
        None => Vec::new(),
    };
    let kept: Vec<(String, f64, f64, bool, String, f64)> = sqlx::query_as(
        "SELECT kind, first_t, expected_t, seen, reason, last_t FROM roast_ror_warnings
         WHERE session_id = ? ORDER BY first_t",
    )
    .bind(session_id)
    .fetch_all(db)
    .await?;
    let ror_warnings = kept
        .into_iter()
        .filter_map(|(kind, warned_at, expected_t, seen, reason, last_t)| {
            let kind = RorWarningKind::parse(&kind)?;
            let came_t = ror_events.iter().find(|e| e.kind == kind).map(|e| e.t);
            Some(RorWarningScore {
                kind,
                warned_at,
                expected_t,
                seen,
                reason,
                last_t,
                came_t,
                lead_s: came_t.map(|c| ((c - warned_at) * 10.0).round() / 10.0),
            })
        })
        .collect();
    Ok(RoastScore {
        session_id: session_id.to_string(),
        predictions: rows.len(),
        beats_projection: beats(&model, &projection),
        first_crack: landmark(first("first_crack_start"), &|r| r.1),
        drop: landmark(first("drop"), &|r| r.2),
        model,
        projection,
        ror_warnings,
        ror_events,
    })
}

async fn pool(db: &SqlitePool, recent: &[RecentScore]) -> Result<PooledScore> {
    let (mut m, mut p) = (Vec::new(), Vec::new());
    for r in recent {
        let e = errors(db, &r.score.session_id).await?;
        m.extend(e[1].iter());
        p.extend(e[3].iter());
    }
    let model_120 = (!m.is_empty()).then(|| model::summarize(120, &m));
    let projection_120 = (!p.is_empty()).then(|| model::summarize(120, &p));
    Ok(PooledScore {
        roasts: recent.len(),
        beats_projection: match (&model_120, &projection_120) {
            (Some(a), Some(b)) => Some(a.rmse < b.rmse),
            _ => None,
        },
        model_120,
        projection_120,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{eventually, Harness};
    use serde_json::json;

    /// A completed roast stored as the server stores one: CHARGE at 5 s, DROP at 5 + len, BT
    /// from the virtual roaster's physics under a ramping heater.
    async fn stored_roast(db: &SqlitePool, device: &str, k: usize, len: usize) -> String {
        let id = format!("roast-{k}");
        let start = Utc::now() - chrono::Duration::hours(10 - k as i64);
        sqlx::query(
            "INSERT INTO roast_sessions (id, name, device_id, status, start_time) VALUES (?, ?, ?, 'completed', ?)",
        )
        .bind(&id)
        .bind(format!("Roast {k}"))
        .bind(device)
        .bind(start)
        .execute(db)
        .await
        .unwrap();
        for (ty, t, temp) in [
            ("charge", 5.0, 25.0),
            (
                "first_crack_start",
                5.0 + len as f64 * 0.8,
                190.0 + k as f64,
            ),
            ("drop", 5.0 + len as f64, 205.0),
        ] {
            sqlx::query(
                "INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&id)
            .bind(ty)
            .bind(t)
            .bind(temp)
            .bind(start)
            .execute(db)
            .await
            .unwrap();
        }
        let mut m = crate::sim::virtual_roaster::Model::default();
        m.charge();
        for i in 0..len + 10 {
            let heater = (40.0 + 5.0 * k as f64 + i as f64 / 10.0).min(95.0);
            let extras = json!({"heaterApplied": heater, "controlMode": 0});
            sqlx::query(
                "INSERT INTO session_telemetry (id, session_id, timestamp, elapsed_seconds, bean_temp, heater, fan, extras)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&id)
            .bind(start)
            .bind(5.0 + i as f64)
            .bind(m.bt)
            .bind(heater)
            .bind(100.0)
            .bind(extras.to_string())
            .execute(db)
            .await
            .unwrap();
            m.step(1.0, heater, 100.0);
        }
        id
    }

    #[tokio::test]
    async fn a_roasters_model_is_fitted_from_its_stored_roasts() {
        let h = Harness::new().await;
        let db = &h.state.db;
        let before = h
            .owner("GET", "/api/roaster/esp32_test/model", None)
            .await
            .json();
        assert_eq!(before["source"], "prior", "{before}");
        assert!(before["fit"].is_null());
        for k in 0..3 {
            stored_roast(db, "esp32_test", k, 400).await;
        }
        let v = h
            .owner("POST", "/api/roaster/esp32_test/model/fit", Some(json!({})))
            .await;
        assert!(v.status.is_success());
        let v = v.json();
        assert_eq!(v["source"], "fit", "{v}");
        assert_eq!(v["fit"]["roasts"], 3);
        let rec = v["fit"]["quality"]["modes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["mode"] == "recorded")
            .unwrap()["horizons"][3]
            .clone();
        assert_eq!(rec["horizon_s"], 120);
        assert!(rec["rmse"].as_f64().unwrap() < 1.5, "{rec}");
        // Where BT settles at full heat and fan: the virtual roaster's ~184 °C.
        let full = v["settles"][1]["bt"].as_f64().unwrap();
        assert!((full - 184.0).abs() < 6.0, "{full}");
    }

    #[tokio::test]
    async fn a_running_roast_is_predicted_kept_and_scored() {
        let h = Harness::new().await;
        let db = h.state.db.clone();
        // History: first crack at 190–192 °C on this roaster.
        for k in 0..3 {
            stored_roast(&db, "esp32_test", k, 300).await;
        }
        // Fit from the whole history now. The background refit runs when the harness starts and
        // then every 5 min: on a slow runner it could fit a half-inserted first roast, a model
        // that never reaches 191 °C at 90 % / 70 %, so no first crack (as it did in CI).
        // Fits take one lock, so this one lands last.
        let fit = h
            .owner("POST", "/api/roaster/esp32_test/model/fit", Some(json!({})))
            .await
            .json();
        assert_eq!(fit["fit"]["roasts"], 3, "{fit}");
        // A roast that started a minute ago, CHARGE at 0.
        let s = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({"name": "live", "device_id": "esp32_test"})),
            )
            .await
            .json();
        let sid = s["id"].as_str().unwrap().to_string();
        h.owner("POST", &format!("/api/sessions/{sid}/start"), None)
            .await;
        sqlx::query("UPDATE roast_sessions SET start_time = ? WHERE id = ?")
            .bind(Utc::now() - chrono::Duration::seconds(90))
            .bind(&sid)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, created_at)
             VALUES ('c', ?, 'charge', 0, 25, ?)",
        )
        .bind(&sid)
        .bind(Utc::now())
        .execute(&db)
        .await
        .unwrap();
        // Before any sample: nothing to predict.
        let none = h
            .owner("GET", "/api/roaster/esp32_test/prediction", None)
            .await;
        assert_eq!(none.status, axum::http::StatusCode::NOT_FOUND);
        let emit =
            || {
                h.owner(
                "POST",
                "/api/test/emit-telemetry/esp32_test",
                Some(json!({"beanTemp": 150.0, "envTemp": 160.0, "heater": 90, "heaterApplied": 90,
                    "fan": 70, "controlMode": 0, "rateOfRise": 12.0})),
            )
            };
        assert!(emit().await.status.is_success());
        // A prediction with its first crack: on a slow runner the first answer may come before
        // the first-crack part is filled in (as it did in CI).
        let last = std::sync::Mutex::new(serde_json::Value::Null);
        let got = eventually(|| async {
            emit().await;
            let r = h
                .owner("GET", "/api/roaster/esp32_test/prediction", None)
                .await;
            let ok = r.status.is_success() && r.json()["first_crack"]["reason"].is_string();
            *last.lock().unwrap() = json!({"status": r.status.as_u16(), "body": r.json()});
            ok
        })
        .await;
        assert!(
            got,
            "no prediction with its first crack; the last answer: {}",
            last.lock().unwrap()
        );
        let p = h
            .owner("GET", "/api/roaster/esp32_test/prediction", None)
            .await
            .json();
        assert_eq!(p["plan"], "held", "{p}");
        assert_eq!(p["points"].as_array().unwrap().len(), 24);
        assert!(p["bt_120"].as_f64().unwrap() > 150.0, "{p}");
        assert!(
            p["reason"]
                .as_str()
                .unwrap()
                .contains("heater 90 % and fan 70 % held"),
            "{p}"
        );
        // First crack from this roaster's history (no bean or profile here: every roast); at
        // 90 % heater and 70 % fan the virtual roaster's model reaches its 191 °C.
        assert!(
            p["first_crack"]["reason"]
                .as_str()
                .unwrap()
                .contains("3 roasts on this roaster"),
            "{p}"
        );
        // It was kept with the roast; with BT filled in later the roast scores it.
        let kept: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM roast_predictions WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(kept >= 1);
        sqlx::query("UPDATE roast_predictions SET actual_60 = pred_60 + 1, actual_120 = pred_120 - 2 WHERE session_id = ?")
            .bind(&sid)
            .execute(&db)
            .await
            .unwrap();
        let score = h
            .owner("GET", &format!("/api/sessions/{sid}/prediction"), None)
            .await
            .json();
        assert_eq!(score["model"][1]["horizon_s"], 120, "{score}");
        assert_eq!(score["model"][1]["bias"], 2.0, "{score}");
        assert_eq!(score["model"][0]["bias"], -1.0, "{score}");
        // First crack as said about two minutes before it came (at 295 s): 290 s, 5 s early.
        sqlx::query(
            "INSERT INTO roast_predictions (session_id, t, plan, bt, pred_60, pred_120, fc_t)
             VALUES (?, 180.5, 'held', 170, 180, 190, 290)",
        )
        .bind(&sid)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, created_at)
             VALUES ('fc', ?, 'first_crack_start', 295, 192, ?)",
        )
        .bind(&sid)
        .bind(Utc::now())
        .execute(&db)
        .await
        .unwrap();
        let score = h
            .owner("GET", &format!("/api/sessions/{sid}/prediction"), None)
            .await
            .json();
        assert_eq!(score["first_crack"]["made_at_t"], 180.5, "{score}");
        assert_eq!(score["first_crack"]["error_s"], -5.0, "{score}");
        assert_eq!(
            h.owner("GET", "/api/sessions/nope/prediction", None)
                .await
                .status,
            axum::http::StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_stall_is_warned_live_alarmed_kept_and_scored() {
        let h = Harness::new().await;
        let db = h.state.db.clone();
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('roast_alarms', ?)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(
            json!([{"name": "Stall ahead", "condition_type": "stall_ahead", "threshold": 120}])
                .to_string(),
        )
        .execute(&db)
        .await
        .unwrap();
        h.state.alarms.reload().await;
        let alarms = Mutex::new(h.state.alarms.subscribe());
        // A roast 5 min in, DRY end marked: BT 180 °C with heater and fan at 100 %, where the
        // virtual roaster's model (the prior: no roasts here) settles near 184 °C.
        let s = h
            .owner(
                "POST",
                "/api/sessions",
                Some(json!({"name": "stall", "device_id": "esp32_test"})),
            )
            .await
            .json();
        let sid = s["id"].as_str().unwrap().to_string();
        h.owner("POST", &format!("/api/sessions/{sid}/start"), None)
            .await;
        sqlx::query("UPDATE roast_sessions SET start_time = ? WHERE id = ?")
            .bind(Utc::now() - chrono::Duration::seconds(300))
            .bind(&sid)
            .execute(&db)
            .await
            .unwrap();
        for (id, ty, t) in [("c", "charge", 0.0), ("d", "drying_end", 200.0)] {
            sqlx::query(
                "INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, created_at)
                 VALUES (?, ?, ?, ?, 160, ?)",
            )
            .bind(id)
            .bind(&sid)
            .bind(ty)
            .bind(t)
            .bind(Utc::now())
            .execute(&db)
            .await
            .unwrap();
        }
        let emit = || {
            h.owner(
                "POST",
                "/api/test/emit-telemetry/esp32_test",
                Some(
                    json!({"beanTemp": 180.0, "envTemp": 190.0, "heater": 100, "heaterApplied": 100,
                    "fan": 100, "controlMode": 0}),
                ),
            )
        };
        let fired = std::sync::Mutex::new(None);
        let got = eventually(|| async {
            emit().await;
            if let Ok(a) = alarms.lock().await.try_recv() {
                *fired.lock().unwrap() = Some(a);
                return true;
            }
            false
        })
        .await;
        assert!(got, "no stall alarm");
        let a = fired.lock().unwrap().clone().unwrap();
        assert_eq!(a.name, "Stall ahead");
        assert_eq!(a.condition_type, "stall_ahead");
        assert!(
            a.message
                .starts_with("The model expects RoR under 1 °C/min"),
            "{}",
            a.message
        );
        assert!(a.message.contains("BT settles near 18"), "{}", a.message);
        let p = h
            .owner("GET", "/api/roaster/esp32_test/prediction", None)
            .await
            .json();
        let w = &p["ror"]["warnings"][0];
        assert_eq!(w["kind"], "stall", "{p}");
        assert_eq!(w["seen"], false, "{p}");
        assert_eq!(p["ror"]["armed"], true, "{p}");
        // Kept with the roast (the row is written after the alarm goes out, so wait for it);
        // after DROP its score says whether it came.
        let kept = || async {
            sqlx::query_as::<_, (String, bool)>(
                "SELECT kind, seen FROM roast_ror_warnings WHERE session_id = ?",
            )
            .bind(&sid)
            .fetch_all(&db)
            .await
            .unwrap()
        };
        assert!(
            eventually(|| async { !kept().await.is_empty() }).await,
            "the warning was not kept with the roast"
        );
        assert_eq!(kept().await, vec![("stall".to_string(), false)]);
        sqlx::query(
            "INSERT INTO roast_events (id, session_id, event_type, elapsed_seconds, temperature, created_at)
             VALUES ('x', ?, 'drop', 600, 181, ?)",
        )
        .bind(&sid)
        .bind(Utc::now())
        .execute(&db)
        .await
        .unwrap();
        let score = h
            .owner("GET", &format!("/api/sessions/{sid}/prediction"), None)
            .await
            .json();
        let ws = &score["ror_warnings"][0];
        assert_eq!(ws["kind"], "stall", "{score}");
        assert!(
            ws["reason"].as_str().unwrap().contains("settles"),
            "{score}"
        );
        // Nothing of this roast is stored (the samples went to the ring only): nothing came.
        assert!(ws["came_t"].is_null(), "{score}");
        assert_eq!(score["ror_events"], json!([]), "{score}");
    }
}
