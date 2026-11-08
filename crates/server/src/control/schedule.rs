//! PID gain scheduling by fan % (`docs/autotune-2.md`, "Gain schedule").
//!
//! Gains depend on airflow: a roast at 100 % fan through drying and ~75 %
//! later needs different gains in each part. A device's schedule
//! (`gain_schedule:<device_id>` in settings) holds rows of fan % → gains in
//! device PID units, one per autotune run at that fan. Between rows the gains
//! are interpolated linearly; beyond the first and last row they hold flat.
//!
//! When the fan changes through the API or a replay, the scheduler sends the
//! gains for the new fan with `"persist": false`, so firmware 3.2 to 3.4 did not
//! write them to NVS. They stay in the device's RAM until it reboots or gains
//! are stored again.
//!
//! **Gains after a boot** (`docs/control.md`, "Device contract"). The server holds a
//! roaster's gains: firmware 3.5.0 keeps none across a reboot. On the first sample from a
//! roaster (this server just started) and on a sample whose uptime went back (the roaster
//! rebooted), the scheduler sends the gains it holds when the roaster reports others: the
//! schedule's for the fan, else the newest stored change (`pid_changes`). The last such
//! check per roaster is kept ([`BootCheck`]) for `GET /api/roaster/{id}/contract`.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::analysis::telemetry::TelemetryService;
use crate::control::tuning::Gains;
use crate::transports::transport::{SendOptions, TransportRegistry};
use roastwarden_autotune::design::Rule;
use roastwarden_autotune::Model;
use roastwarden_core::DeviceCommand;
use roastwarden_core::Sample;

/// Rows closer than this (fan %) are the same fan level.
const SAME_FAN: f64 = 0.5;
const MAX_ROWS: usize = 16;
/// Relative change below which the scheduler does not resend gains...
const RESEND_REL: f64 = 0.01;
/// ...unless it last sent them this long ago: a device that rebooted since may be
/// back on other gains.
const RESEND_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ScheduleRow {
    pub fan_pct: f64,
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    /// Holding power at this fan (the autotune `bias`), % — recorded for
    /// feedforward; not sent yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ff: Option<f64>,
    /// Autotune run the row came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The model the row's gains were designed on (a server run, docs/autotune-2.md): between
    /// two such rows the schedule interpolates the model and designs the gains from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,
    /// SIMC's knob the row was designed with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tau_c: Option<f64>,
}

impl ScheduleRow {
    pub fn gains(&self) -> Gains {
        Gains {
            kp: self.kp,
            ki: self.ki,
            kd: self.kd,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GainSchedule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rows: Vec<ScheduleRow>,
}

impl GainSchedule {
    /// Check the rows and sort them by fan.
    pub fn normalize(&mut self) -> Result<(), String> {
        if self.rows.len() > MAX_ROWS {
            return Err(format!("at most {MAX_ROWS} rows"));
        }
        for r in &self.rows {
            if !r.fan_pct.is_finite() || !(0.0..=100.0).contains(&r.fan_pct) {
                return Err("fan_pct must be 0..100".into());
            }
            if !r.gains().is_valid() {
                return Err(format!(
                    "gains at fan {:.0} % must be finite and >= 0",
                    r.fan_pct
                ));
            }
            if r.ff
                .is_some_and(|f| !f.is_finite() || !(0.0..=100.0).contains(&f))
            {
                return Err("ff must be 0..100 %".into());
            }
        }
        self.rows.sort_by(|a, b| a.fan_pct.total_cmp(&b.fan_pct));
        if self
            .rows
            .windows(2)
            .any(|w| w[1].fan_pct - w[0].fan_pct < SAME_FAN)
        {
            return Err("two rows have the same fan %".into());
        }
        Ok(())
    }

    /// A row that names an autotune rule but whose gains are not that rule's (edited by hand)
    /// keeps its run and holding power and loses the rule, so the schedule does not say
    /// "Tyreus–Luyben PID" over PI gains. `rule_gains` answers the rule's gains for a row's run
    /// (rounded as `add_run_to_schedule` stores them), `None` when the run is gone.
    pub fn mark_hand_edits(&mut self, rule_gains: impl Fn(&str, &str) -> Option<Gains>) {
        for r in &mut self.rows {
            let (Some(run), Some(rule)) = (r.run_id.as_deref(), r.rule.as_deref()) else {
                continue;
            };
            let same = rule_gains(run, rule).is_some_and(|g| {
                let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 + 1e-3 * b.abs();
                close(r.kp, g.kp) && close(r.ki, g.ki) && close(r.kd, g.kd)
            });
            if !same {
                r.rule = None;
                r.note = Some("edited".into());
            }
        }
    }

    /// Replace the row at the same fan level, or add one.
    pub fn upsert(&mut self, row: ScheduleRow) {
        self.rows
            .retain(|r| (r.fan_pct - row.fan_pct).abs() >= SAME_FAN);
        self.rows.push(row);
        self.rows.sort_by(|a, b| a.fan_pct.total_cmp(&b.fan_pct));
    }

    /// Gains at a fan %: linear between rows, flat beyond the ends. `None`
    /// when the schedule is off or empty.
    pub fn gains_at(&self, fan_pct: f64) -> Option<Gains> {
        if !self.enabled || !fan_pct.is_finite() {
            return None;
        }
        let rows = &self.rows;
        let first = rows.first()?;
        let last = rows.last()?;
        if fan_pct <= first.fan_pct {
            return Some(first.gains());
        }
        if fan_pct >= last.fan_pct {
            return Some(last.gains());
        }
        let i = rows.iter().position(|r| r.fan_pct >= fan_pct)?;
        let (a, b) = (&rows[i - 1], &rows[i]);
        let f = (fan_pct - a.fan_pct) / (b.fan_pct - a.fan_pct);
        if let Some(g) = designed_between(a, b, f) {
            return Some(g);
        }
        let lerp = |x: f64, y: f64| x + (y - x) * f;
        Some(Gains {
            kp: lerp(a.kp, b.kp),
            ki: lerp(a.ki, b.ki),
            kd: lerp(a.kd, b.kd),
        })
    }
}

/// Two rows' models blended at `f` (0 = `a`): the rate `kv` and the lag log-linearly (gains go
/// as 1/kv, time constants multiply), the dead time linearly. Two first-order models stay
/// first order; otherwise both become integrators with their rate and dead time.
pub fn interpolate_model(a: &Model, b: &Model, f: f64) -> Model {
    let (a, b) = (a.for_pi(), b.for_pi());
    let log = |x: f64, y: f64| (x.ln() + (y.ln() - x.ln()) * f).exp();
    let l = a.l + (b.l - a.l) * f;
    let kv = log(a.kv().abs().max(1e-12), b.kv().abs().max(1e-12));
    if !a.integrating && !b.integrating {
        let t1 = log(a.t1, b.t1);
        Model::fotd(kv * t1, t1, l)
    } else {
        Model::itd(kv, l)
    }
}

/// The gains a row's rule gives on the model between `a` and `b`, when both rows were
/// designed from a model with the same rule and knob.
fn designed_between(a: &ScheduleRow, b: &ScheduleRow, f: f64) -> Option<Gains> {
    let (ma, mb) = (a.model.as_ref()?, b.model.as_ref()?);
    if a.rule != b.rule
        || a.tau_c.map(|t| (t * 100.0).round()) != b.tau_c.map(|t| (t * 100.0).round())
    {
        return None;
    }
    let rule = Rule::from_id(a.rule.as_deref()?)?;
    let tau = a
        .tau_c
        .unwrap_or(roastwarden_autotune::design::TAU_C_DEFAULT);
    let pid = roastwarden_autotune::design::gains(rule, &interpolate_model(ma, mb, f), tau, 1.0)?;
    Some(crate::control::tuning::device_gains(&pid))
}

/// Whether two gain sets differ enough to send.
pub fn differs(a: Gains, b: Gains) -> bool {
    let d = |x: f64, y: f64| (x - y).abs() > RESEND_REL * x.abs().max(y.abs()).max(1e-6);
    d(a.kp, b.kp) || d(a.ki, b.ki) || d(a.kd, b.kd)
}

pub fn setting_key(device_id: &str) -> String {
    format!("gain_schedule:{device_id}")
}

pub async fn load(db: &SqlitePool, device_id: &str) -> Result<GainSchedule, sqlx::Error> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(setting_key(device_id))
        .fetch_optional(db)
        .await?;
    Ok(raw
        .and_then(|r| serde_json::from_str::<GainSchedule>(&r).ok())
        .and_then(|mut s| s.normalize().ok().map(|()| s))
        .unwrap_or_default())
}

pub async fn save(db: &SqlitePool, device_id: &str, s: &GainSchedule) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(setting_key(device_id))
    .bind(serde_json::to_string(s).unwrap_or_default())
    .execute(db)
    .await
    .map(|_| ())
}

/// What to look at in a schedule, in words (`docs/autotune-2.md`, "Checking a schedule").
pub fn advice(s: &GainSchedule) -> Vec<String> {
    let mut out = Vec::new();
    let with_d: Vec<String> = s
        .rows
        .iter()
        .filter(|r| r.kd > 0.0)
        .map(|r| format!("{:.0} %", r.fan_pct))
        .collect();
    if !with_d.is_empty() {
        out.push(format!(
            "The rows at {} fan have a D term: on a thermocouple read once a second in 0.25 °C \
             steps, D turns each step into a heater jump (10 to 30 % a second on the real \
             roaster). Add those runs to the schedule again with a PI rule (Tyreus–Luyben PI).",
            with_d.join(", ")
        ));
    }
    let mut rows: Vec<&ScheduleRow> = s.rows.iter().collect();
    rows.sort_by(|a, b| a.fan_pct.total_cmp(&b.fan_pct));
    for w in rows.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Between two rows with models: the gains for the midpoint, on each neighbour's model
        // as if it were the true process.
        if let (Some(ma), Some(mb), Some(g)) = (a.model, b.model, designed_between(a, b, 0.5)) {
            let pid = roastwarden_autotune::Pid::pi(g.kp / 2.55, g.ki / 2.55);
            let worst = [ma, mb]
                .iter()
                .filter_map(|m| roastwarden_autotune::evaluate(m, &pid, None, 1.0))
                .map(|r| r.ms)
                .fold(0.0, f64::max);
            if worst > 2.0 {
                out.push(format!(
                    "Halfway between the rows at {:.0} % and {:.0} % fan the gains have little                      margin on either row's model (Ms {worst:.1}): add a test between them, or                      use a smoother setting.",
                    a.fan_pct, b.fan_pct
                ));
            }
            continue;
        }
        let gap = b.fan_pct - a.fan_pct;
        let rel = (a.kp - b.kp).abs() / a.kp.max(b.kp).max(1e-9);
        if gap <= 10.0 && rel > 0.2 {
            out.push(format!(
                "The rows at {:.0} % and {:.0} % fan are {gap:.0} % of fan apart but their Kp \
                 differs by {:.0} %: the gains jump as fan assist moves the fan between them. \
                 Keep one, or run the autotune again with beans.",
                a.fan_pct,
                b.fan_pct,
                rel * 100.0
            ));
        }
    }
    out
}

/// The gains a sample says the roaster runs, if it reports them.
pub fn reported_gains(s: &Sample) -> Option<Gains> {
    let g = |k: &str| s.extras.get(k).copied().filter(|v| v.is_finite());
    Some(Gains {
        kp: g("Kp")?,
        ki: g("Ki")?,
        kd: g("Kd")?,
    })
}

/// The roaster's uptime in seconds, if its telemetry reports one (`uptimeMs` or `uptime`;
/// the typed sample leaves both out).
fn uptime_s(payload: &serde_json::Value) -> Option<f64> {
    let x = |k: &str| {
        payload
            .get(k)
            .and_then(serde_json::Value::as_f64)
            .filter(|v| v.is_finite())
    };
    x("uptimeMs").map(|ms| ms / 1000.0).or_else(|| x("uptime"))
}

/// Whether a sample is the first from this roaster since the server started
/// (`server_start`), or the first since it rebooted (`reboot`: its uptime went back).
/// Remembers the uptime for the next call.
fn booted(
    seen: &mut HashMap<String, Option<f64>>,
    device_id: &str,
    uptime: Option<f64>,
) -> Option<&'static str> {
    match seen.insert(device_id.to_string(), uptime) {
        None => Some("server_start"),
        Some(Some(p)) if uptime.is_some_and(|u| u + 1.0 < p) => Some("reboot"),
        Some(_) => None,
    }
}

/// The server's last check of a roaster's gains after a boot.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct BootCheck {
    pub at: chrono::DateTime<chrono::Utc>,
    /// `server_start` (its first sample since the server started) or `reboot` (its uptime
    /// went back)
    pub why: String,
    /// The gains it came up with (none: it reports none)
    pub reported: Option<Gains>,
    /// The gains the server held for it then
    pub held: Option<Gains>,
    /// The gains the server sent it (none: it already had them, or the server holds none)
    pub sent: Option<Gains>,
}

/// Sends scheduled gains when the fan changes, and the held gains after a boot.
pub struct GainScheduler {
    db: SqlitePool,
    transports: Arc<TransportRegistry>,
    /// Gains the scheduler last sent per device, and when.
    last_sent: Mutex<HashMap<String, (Gains, tokio::time::Instant)>>,
    /// Fan % last commanded per device. Gains follow the command, not the
    /// reported fan, which lags it by a sample.
    fan: Mutex<HashMap<String, f64>>,
    /// Uptime of each roaster's last sample, to tell a reboot.
    uptime: Mutex<HashMap<String, Option<f64>>>,
    /// The last boot check per roaster.
    boots: Mutex<HashMap<String, BootCheck>>,
}

impl GainScheduler {
    pub fn new(db: SqlitePool, transports: Arc<TransportRegistry>) -> Arc<Self> {
        Arc::new(Self {
            db,
            transports,
            last_sent: Mutex::new(HashMap::new()),
            fan: Mutex::new(HashMap::new()),
            uptime: Mutex::new(HashMap::new()),
            boots: Mutex::new(HashMap::new()),
        })
    }

    pub async fn schedule(&self, device_id: &str) -> GainSchedule {
        load(&self.db, device_id).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, device_id, "could not load the gain schedule");
            GainSchedule::default()
        })
    }

    /// Scheduled gains for a fan %, if the device has an enabled schedule.
    pub async fn gains_for(&self, device_id: &str, fan_pct: f64) -> Option<Gains> {
        self.schedule(device_id).await.gains_at(fan_pct)
    }

    /// The fan changed: send the scheduled gains (`persist: false`) unless
    /// they match what was last sent. Returns the gains sent.
    pub async fn fan_changed(&self, device_id: &str, fan_pct: f64) -> Option<Gains> {
        self.note_fan(device_id, fan_pct).await;
        let g = self.gains_for(device_id, fan_pct).await?;
        if let Some((prev, at)) = self.last_sent.lock().await.get(device_id) {
            if !differs(*prev, g) && at.elapsed() < RESEND_AFTER {
                return None;
            }
        }
        self.send(device_id, g).await.then_some(g)
    }

    /// Send gains with `persist: false` and remember them.
    async fn send(&self, device_id: &str, g: Gains) -> bool {
        // PID gains need full control and a heater (docs/roaster-setup.md); skip quietly
        // without them.
        let limits = self.transports.limits(device_id).await;
        if limits.control != crate::roasters::setup::Control::Full
            || limits.outputs.heater.is_none()
        {
            return false;
        }
        let cmd = DeviceCommand::Pid {
            kp: g.kp,
            ki: g.ki,
            kd: g.kd,
            persist: false,
        };
        match self
            .transports
            .send(device_id, &cmd, SendOptions::default())
            .await
        {
            Ok(_) => {
                tracing::info!(device_id, kp = g.kp, ki = g.ki, kd = g.kd, "gains sent");
                self.last_sent
                    .lock()
                    .await
                    .insert(device_id.to_string(), (g, tokio::time::Instant::now()));
                true
            }
            Err(e) => {
                tracing::warn!(device_id, error = %e, "could not send gains");
                false
            }
        }
    }

    /// The gains the server holds for a roaster: the schedule's for the fan (the one last
    /// commanded, else `fan`), else the newest stored change.
    pub async fn held_gains(&self, device_id: &str, fan: Option<f64>) -> Option<Gains> {
        let fan = self.commanded_fan(device_id).await.or(fan);
        if let Some(g) = match fan {
            Some(f) => self.gains_for(device_id, f).await,
            None => None,
        } {
            return Some(g);
        }
        crate::control::tuning::list_changes(&self.db, device_id, 1)
            .await
            .ok()?
            .pop()
            .map(|c| c.gains)
    }

    /// A sample from a roaster. On the first one since the server started, or after the
    /// roaster rebooted, send the gains the server holds if the roaster reports others (or
    /// none, with a setpoint: an onboard PID that does not report its gains). Returns the
    /// gains sent.
    pub async fn on_sample(
        &self,
        device_id: &str,
        s: &Sample,
        payload: &serde_json::Value,
    ) -> Option<Gains> {
        let why = booted(&mut *self.uptime.lock().await, device_id, uptime_s(payload))?;
        let reported = reported_gains(s);
        if reported.is_none() && s.setpoint.is_none() {
            return None;
        }
        let held = self.held_gains(device_id, s.fan).await;
        let sent = match held {
            Some(want) if reported.is_none_or(|r| differs(r, want)) => {
                tracing::info!(device_id, ?reported, "the roaster booted with other gains");
                self.send(device_id, want).await.then_some(want)
            }
            _ => None,
        };
        self.boots.lock().await.insert(
            device_id.to_string(),
            BootCheck {
                at: chrono::Utc::now(),
                why: why.to_string(),
                reported,
                held,
                sent,
            },
        );
        sent
    }

    /// The last check of a roaster's gains after a boot, since the server started.
    pub async fn last_boot(&self, device_id: &str) -> Option<BootCheck> {
        self.boots.lock().await.get(device_id).cloned()
    }

    /// Watch every roaster's telemetry for boots (any transport).
    pub async fn run(self: Arc<Self>, telemetry: TelemetryService) {
        let mut rx = telemetry.subscribe();
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    self.on_sample(&ev.device_id, &ev.sample, &ev.payload).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    /// Record a fan command without sending gains.
    pub async fn note_fan(&self, device_id: &str, fan_pct: f64) {
        self.fan.lock().await.insert(device_id.to_string(), fan_pct);
    }

    /// Fan % last commanded through the server, if any.
    pub async fn commanded_fan(&self, device_id: &str) -> Option<f64> {
        self.fan.lock().await.get(device_id).copied()
    }

    /// The device's gains changed some other way (stored, schedule edited):
    /// the next fan change sends again.
    pub async fn forget(&self, device_id: &str) {
        self.last_sent.lock().await.remove(device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(fan: f64, kp: f64, ki: f64) -> ScheduleRow {
        ScheduleRow {
            fan_pct: fan,
            kp,
            ki,
            kd: 0.0,
            ff: None,
            run_id: None,
            rule: None,
            note: None,
            model: None,
            tau_c: None,
        }
    }

    #[test]
    fn gains_interpolate_between_rows_and_hold_at_the_ends() {
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![row(100.0, 10.0, 0.2), row(75.0, 6.0, 0.1)],
        };
        s.normalize().unwrap();
        assert_eq!(s.rows[0].fan_pct, 75.0);
        assert_eq!(s.gains_at(100.0).unwrap().kp, 10.0);
        assert_eq!(s.gains_at(120.0).unwrap().kp, 10.0);
        assert_eq!(s.gains_at(50.0).unwrap().kp, 6.0);
        let mid = s.gains_at(87.5).unwrap();
        assert!((mid.kp - 8.0).abs() < 1e-9 && (mid.ki - 0.15).abs() < 1e-9);

        s.enabled = false;
        assert!(s.gains_at(90.0).is_none());
        assert!(GainSchedule {
            enabled: true,
            rows: vec![]
        }
        .gains_at(90.0)
        .is_none());
    }

    #[test]
    fn rows_are_validated_and_upserted_by_fan_level() {
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![row(75.0, 6.0, 0.1), row(75.2, 7.0, 0.1)],
        };
        assert!(s.normalize().is_err());
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![row(120.0, 6.0, 0.1)],
        };
        assert!(s.normalize().is_err());
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![row(80.0, -1.0, 0.1)],
        };
        assert!(s.normalize().is_err());

        let mut s = GainSchedule::default();
        s.upsert(row(100.0, 10.0, 0.2));
        s.upsert(row(75.0, 6.0, 0.1));
        s.upsert(row(74.8, 5.0, 0.1));
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0].kp, 5.0);
        assert!(s.normalize().is_ok());
    }

    #[test]
    fn advice_names_d_terms_and_rows_that_disagree() {
        let mut a = row(71.0, 16.8, 0.263);
        a.kd = 77.0;
        let s = GainSchedule {
            enabled: true,
            rows: vec![row(100.0, 10.6, 0.133), row(75.0, 13.3, 0.161), a],
        };
        let adv = advice(&s);
        assert_eq!(adv.len(), 2, "{adv:?}");
        assert!(adv[0].contains("71 % fan have a D term"), "{}", adv[0]);
        assert!(adv[1].contains("71 % and 75 %"), "{}", adv[1]);
        assert_eq!(
            advice(&GainSchedule {
                enabled: true,
                rows: vec![row(100.0, 10.6, 0.133), row(75.0, 11.0, 0.15)],
            }),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn hand_edited_rows_lose_their_rule() {
        let from_run = |kp: f64, ki: f64, kd: f64| ScheduleRow {
            kd,
            run_id: Some("run1".into()),
            rule: Some("tyreus_luyben".into()),
            ..row(71.0, kp, ki)
        };
        let tl = Gains {
            kp: 16.797,
            ki: 0.26328,
            kd: 77.321,
        };
        let lookup =
            |run: &str, rule: &str| (run == "run1" && rule == "tyreus_luyben").then_some(tl);
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![
                from_run(16.797, 0.26328, 77.321),
                from_run(11.55, 0.181, 0.0),
            ],
        };
        s.mark_hand_edits(lookup);
        assert_eq!(s.rows[0].rule.as_deref(), Some("tyreus_luyben"));
        assert_eq!(s.rows[0].note, None);
        assert_eq!(s.rows[1].rule, None);
        assert_eq!(s.rows[1].note.as_deref(), Some("edited"));
        assert_eq!(s.rows[1].run_id.as_deref(), Some("run1"), "the run is kept");
        // A run that is gone: the rule cannot be checked, so it is not claimed.
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![from_run(16.797, 0.26328, 77.321)],
        };
        s.mark_hand_edits(|_, _| None);
        assert_eq!(s.rows[0].rule, None);
    }

    #[test]
    fn rows_with_models_design_the_gains_between_them() {
        use roastwarden_autotune::design::{gains, Rule};
        let at = |fan: f64, kv: f64, l: f64| {
            let m = Model::itd(kv, l);
            let g = crate::control::tuning::device_gains(&gains(Rule::Simc, &m, 1.5, 1.0).unwrap());
            ScheduleRow {
                kd: 0.0,
                rule: Some("simc_pi".into()),
                tau_c: Some(1.5),
                model: Some(m),
                ..row(fan, g.kp, g.ki)
            }
        };
        let s = GainSchedule {
            enabled: true,
            rows: vec![at(70.0, 0.02, 6.0), at(100.0, 0.01, 8.0)],
        };
        // At the rows: their own gains.
        assert_eq!(s.gains_at(70.0).unwrap().kp, s.rows[0].kp);
        // Halfway: designed on kv √(0.02·0.01) and L 7, not the gains' average.
        let mid = s.gains_at(85.0).unwrap();
        let want = crate::control::tuning::device_gains(
            &gains(
                Rule::Simc,
                &Model::itd((0.02f64 * 0.01).sqrt(), 7.0),
                1.5,
                1.0,
            )
            .unwrap(),
        );
        assert!((mid.kp - want.kp).abs() < 1e-9 && (mid.ki - want.ki).abs() < 1e-9);
        assert!(advice(&s).is_empty(), "{:?}", advice(&s));
        // Rows far apart in the process: the midpoint check speaks up.
        let far = GainSchedule {
            enabled: true,
            rows: vec![at(70.0, 0.2, 1.0), at(100.0, 0.002, 40.0)],
        };
        assert!(
            advice(&far).iter().any(|a| a.contains("Halfway")),
            "{:?}",
            advice(&far)
        );
    }

    #[test]
    fn small_changes_are_not_resent() {
        let a = Gains {
            kp: 10.0,
            ki: 0.2,
            kd: 0.0,
        };
        assert!(!differs(a, Gains { kp: 10.05, ..a }));
        assert!(differs(a, Gains { kp: 10.2, ..a }));
        assert!(differs(a, Gains { kd: 0.5, ..a }));
        assert!(!differs(a, a));
    }

    #[tokio::test]
    async fn schedule_round_trips_through_settings() {
        let db = crate::db::open_memory().await;
        assert_eq!(load(&db, "r1").await.unwrap(), GainSchedule::default());
        let mut s = GainSchedule {
            enabled: true,
            rows: vec![row(100.0, 10.0, 0.2)],
        };
        s.normalize().unwrap();
        save(&db, "r1", &s).await.unwrap();
        save(&db, "r1", &s).await.unwrap();
        assert_eq!(load(&db, "r1").await.unwrap(), s);
        assert_eq!(load(&db, "r2").await.unwrap(), GainSchedule::default());
    }

    #[test]
    fn a_boot_is_the_first_sample_or_uptime_going_back() {
        let mut seen = HashMap::new();
        assert_eq!(
            booted(&mut seen, "r1", Some(100.0)),
            Some("server_start"),
            "first sample since start"
        );
        assert_eq!(booted(&mut seen, "r1", Some(101.0)), None);
        assert_eq!(
            booted(&mut seen, "r1", Some(100.5)),
            None,
            "jitter is not a boot"
        );
        assert_eq!(
            booted(&mut seen, "r1", Some(3.0)),
            Some("reboot"),
            "uptime went back"
        );
        assert_eq!(booted(&mut seen, "r2", None), Some("server_start"));
        assert_eq!(
            booted(&mut seen, "r2", None),
            None,
            "no uptime: only the first"
        );
        assert_eq!(
            booted(&mut seen, "r2", Some(5.0)),
            None,
            "uptime appearing is no boot"
        );
    }

    #[test]
    fn gains_and_uptime_come_from_the_sample() {
        let v = serde_json::json!({"beanTemp": 25.0, "Kp": 5.5, "Ki": 0.085, "Kd": 0.0, "uptimeMs": 4200});
        let s = Sample::from_telemetry_json(1, 0, &v);
        assert_eq!(
            reported_gains(&s),
            Some(Gains {
                kp: 5.5,
                ki: 0.085,
                kd: 0.0
            })
        );
        assert_eq!(uptime_s(&v), Some(4.2));
        assert_eq!(uptime_s(&serde_json::json!({"uptime": 7})), Some(7.0));
        let bare = serde_json::json!({"beanTemp": 25.0});
        let s = Sample::from_telemetry_json(1, 0, &bare);
        assert_eq!((reported_gains(&s), uptime_s(&bare)), (None, None));
    }

    /// The virtual roaster boots on its own default gains, like firmware 3.5.0 without NVS:
    /// the server sends the stored ones back.
    #[tokio::test]
    async fn a_rebooted_roaster_gets_the_held_gains_back() {
        use crate::test_support::{eventually, Harness};
        use serde_json::json;
        let h = Harness::new().await;
        let vr = crate::sim::virtual_roaster::DEVICE_ID;
        let latest = format!("/api/roaster/{vr}/telemetry/latest");
        let on = |enabled: bool| {
            h.owner(
                "PUT",
                "/api/virtual-roaster",
                Some(json!({ "enabled": enabled })),
            )
        };
        let kp = || async {
            let v = h.owner("GET", &latest, None).await.json();
            (
                v["telemetry"]["Kp"].as_f64(),
                v["telemetry"]["uptime"].as_f64(),
            )
        };
        assert!(on(true).await.status.is_success());
        assert!(eventually(|| async { kp().await.0.is_some() }).await);
        let default_kp = kp().await.0.unwrap();

        let stored = h
            .owner(
                "POST",
                &format!("/api/roaster/{vr}/control/pid"),
                Some(json!({"kp": 4.0, "ki": 0.05, "kd": 0.0})),
            )
            .await;
        assert!(stored.status.is_success(), "{}", stored.json());
        assert!(eventually(|| async { kp().await.0 == Some(4.0) }).await);
        // Long enough up that a reboot shows as uptime going back.
        assert!(eventually(|| async { kp().await.1.is_some_and(|u| u >= 3.0) }).await);
        let up_before = kp().await.1.unwrap();

        assert!(on(false).await.status.is_success());
        assert!(on(true).await.status.is_success());
        // It came back on its defaults (a sample from after the reboot: uptime below the last
        // one before it), and the server put the stored gains back.
        let after = |want: f64| async move {
            let (k, up) = kp().await;
            k == Some(want) && up.is_some_and(|u| u < up_before)
        };
        assert!(
            eventually(|| after(4.0)).await,
            "stored gains back after the reboot (defaults were {default_kp})"
        );

        // The contract call shows the reboot check: what it came up with, what was sent,
        // and that its gains now match the held ones.
        let c = h
            .owner("GET", &format!("/api/roaster/{vr}/contract"), None)
            .await
            .json();
        let boot = &c["gains"]["last_boot"];
        assert_eq!(boot["why"], "reboot", "{c}");
        assert_eq!(boot["reported"]["kp"], default_kp, "{c}");
        assert_eq!(boot["sent"]["kp"], 4.0, "{c}");
        assert_eq!(c["gains"]["held"]["kp"], 4.0, "{c}");
        assert_eq!(c["gains"]["matches"], true, "{c}");
        assert_eq!(c["firmware"], "virtual-1", "{c}");
        assert_eq!(c["caps"]["reported"], true, "{c}");
    }
}
