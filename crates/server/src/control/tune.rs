//! The server-run autotune (docs/autotune-2.md): the relay experiment of `roastwarden-autotune`
//! driven through the transports like Profile drive (manual heater %, a control lease), any
//! roaster that takes a heater % and reports BT. Every sample is kept; at the end the heater
//! goes off, the record is analysed and stored as an autotune run with `source: "server"`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use roastwarden_autotune::analysis::{Analysis, Quality, analyse};
use roastwarden_autotune::fit::Record;
use roastwarden_autotune::relay::{Phase, Relay, RelayConfig, RelaySummary};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::analysis::telemetry::TelemetryService;
use crate::control::tuning::{self, AutotuneRun};
use crate::replay::ReplayService;
use crate::transports::transport::{SendOptions, TransportRegistry};
use roastwarden_core::{ControlMode, DeviceCommand};

/// How often the loop looks for a new sample (the roaster sends one a second).
const POLL: Duration = Duration::from_millis(250);
/// Control lease the roaster enforces while the test drives it, refreshed every 3 s...
const LEASE_MS: u32 = 10_000;
const LEASE_REFRESH: Duration = Duration::from_secs(3);
/// ...but not with telemetry this old (the roaster then goes to its safe state on its own),
const LEASE_STALE: Duration = Duration::from_secs(8);
/// and the test stops at this age.
const STALE_STOP: Duration = Duration::from_secs(20);
/// The heater is sent again at least this often (a command lost on the way).
const RESEND: Duration = Duration::from_secs(5);
/// The device PID's sample time, for the designs (half a sample of delay).
const CONTROL_DT: f64 = 1.0;

/// Start a test (`POST /api/roaster/{id}/tune/start`).
#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub struct StartTune {
    /// Bean temperature to test at, °C (100–250).
    pub setpoint: f64,
    /// Fan % for the test (45–100); unset: as it is. Set before the test starts.
    #[serde(default)]
    pub fan_pct: Option<f64>,
    /// Heater % on the way up (50–100, default 100).
    #[serde(default)]
    pub approach_power: Option<f64>,
    /// Largest relay swing, % (5–50, default 30).
    #[serde(default)]
    pub max_swing: Option<f64>,
    /// Larger swing over the smaller in the asymmetric phase (1.5–4, default 2); 1 skips the
    /// phase.
    #[serde(default)]
    pub asymmetry: Option<f64>,
    /// Switching band ± around the level, °C (0.1–5); unset: from the measured noise.
    #[serde(default)]
    pub hysteresis: Option<f64>,
}

impl StartTune {
    /// The test's settings; its fan must be at or above the roaster's fan floor while heating
    /// (`fan_floor_pct`, 45 % on the fluid bed).
    pub fn config(&self, fan_floor_pct: f64) -> Result<RelayConfig, String> {
        let mut c = RelayConfig {
            setpoint: self.setpoint,
            ..RelayConfig::default()
        };
        if !(self.setpoint.is_finite() && (100.0..=250.0).contains(&self.setpoint)) {
            return Err("setpoint must be 100..250 °C".into());
        }
        if let Some(f) = self.fan_pct {
            // A fluid bed's firmware cuts the heater at 40 % fan or less (its interlock).
            let floor = fan_floor_pct.clamp(0.0, 100.0);
            if !(f.is_finite() && (floor..=100.0).contains(&f)) {
                return Err(format!("fan_pct must be {floor:.0}..100"));
            }
        }
        if let Some(p) = self.approach_power {
            if !(p.is_finite() && (50.0..=100.0).contains(&p)) {
                return Err("approach_power must be 50..100".into());
            }
            c.approach_power = p;
        }
        if let Some(s) = self.max_swing {
            if !(s.is_finite() && (5.0..=50.0).contains(&s)) {
                return Err("max_swing must be 5..50".into());
            }
            c.max_swing = s;
        }
        if let Some(a) = self.asymmetry {
            if !(a.is_finite() && (1.0..=4.0).contains(&a)) {
                return Err("asymmetry must be 1..4".into());
            }
            if a < 1.5 {
                c.asym_cycles = 0;
            } else {
                c.asymmetry = a;
            }
        }
        if let Some(h) = self.hysteresis {
            if !(h.is_finite() && (0.1..=5.0).contains(&h)) {
                return Err("hysteresis must be 0.1..5 °C".into());
            }
            c.hysteresis = Some(h);
        }
        Ok(c)
    }
}

/// The test as the dashboard shows it (`GET /api/roaster/{id}/tune`).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TuneState {
    pub device_id: String,
    pub run_id: String,
    /// Driving the roaster now.
    pub active: bool,
    /// approach, symmetric, asymmetric, analysing, done, failed, stopped
    pub phase: String,
    pub started_at: DateTime<Utc>,
    pub elapsed_s: f64,
    /// The level asked for, and the one tested (lower when the roaster could not reach it).
    pub target: f64,
    pub setpoint: f64,
    pub fan_pct: Option<f64>,
    /// The heater % now and the holding power the relay is centred on.
    pub input: f64,
    pub bias: f64,
    pub hysteresis: f64,
    /// Sensor noise measured on the approach (σ, °C).
    pub noise: Option<f64>,
    pub cycles: usize,
    pub samples: usize,
    pub bt: Option<f64>,
    /// Why it ended, when it did.
    pub message: Option<String>,
    /// The stored run, once analysed.
    pub result_run_id: Option<String>,
    pub quality: Option<String>,
}

#[derive(Debug)]
pub enum TuneError {
    Invalid(String),
    Busy(String),
    Offline,
    Transport(String),
    /// The roaster's control level is below full (docs/roaster-setup.md).
    ControlOff(String),
}

/// One recorded sample.
#[derive(Debug, Clone, Copy)]
struct Rec {
    t: f64,
    y: f64,
    /// The heater % the test asked for from this sample on.
    cmd: f64,
    /// The heater % the roaster reported applying at this sample.
    applied: Option<f64>,
    fan: Option<f64>,
}

struct Run {
    state: TuneState,
    relay: Relay,
    record: Vec<Rec>,
    t0_ms: u64,
    last_seq: u64,
    last_sample_at: Instant,
    last_sent: Option<(f64, Instant)>,
    lease: bool,
    lease_sent_at: Option<Instant>,
    heater_seen_on: bool,
    stop: Option<String>,
    started: Instant,
}

/// What one new sample asks of the loop.
#[derive(Debug, Default, PartialEq)]
struct Step {
    /// Send this heater %.
    send: Option<f64>,
    /// The test is over, for this reason.
    end: Option<String>,
}

impl Run {
    /// Feed one new sample: the relay's next input, the record, the state the dashboard sees.
    fn ingest(&mut self, s: &roastwarden_core::Sample) -> Step {
        let mut step = Step::default();
        self.last_seq = s.seq;
        self.last_sample_at = Instant::now();
        if s.heater_enable == Some(true) {
            self.heater_seen_on = true;
        } else if s.heater_enable == Some(false) && self.heater_seen_on {
            step.end = Some("the roaster cut its heater (its safe state, or Heat off)".into());
            return step;
        }
        let Some(y) = s.bt else {
            return step;
        };
        let t = s.ts_ms.saturating_sub(self.t0_ms) as f64 / 1000.0;
        let u = self.relay.step(t, y);
        self.record.push(Rec {
            t,
            y,
            cmd: u,
            applied: s.extras.get("heaterApplied").copied().or(s.heater),
            fan: s.fan,
        });
        let resend = self
            .last_sent
            .is_none_or(|(v, at)| (v - u).abs() > 0.05 || at.elapsed() >= RESEND);
        if self.relay.active() && resend {
            step.send = Some(u);
        }
        let sm = self.relay.summary();
        self.state.bt = Some(y);
        self.state.input = u;
        self.state.bias = self.relay.bias();
        self.state.setpoint = self.relay.setpoint();
        self.state.hysteresis = sm.hysteresis;
        self.state.noise = sm.noise;
        self.state.cycles = self.relay.cycles();
        self.state.samples = self.record.len();
        self.state.phase = phase_name(self.relay.phase()).into();
        if !self.relay.active() {
            step.end = Some(sm.reason.unwrap_or_else(|| "finished".into()));
        }
        step
    }
}

pub struct TuneService {
    db: SqlitePool,
    transports: Arc<TransportRegistry>,
    telemetry: TelemetryService,
    replay: Arc<ReplayService>,
    runs: Mutex<HashMap<String, Arc<Mutex<Run>>>>,
}

impl TuneService {
    pub fn new(
        db: SqlitePool,
        transports: Arc<TransportRegistry>,
        telemetry: TelemetryService,
        replay: Arc<ReplayService>,
    ) -> Arc<Self> {
        Arc::new(TuneService {
            db,
            transports,
            telemetry,
            replay,
            runs: Mutex::new(HashMap::new()),
        })
    }

    /// The test on this roaster now, or the last one since the server started.
    pub async fn state(&self, device_id: &str) -> Option<TuneState> {
        let run = self.runs.lock().await.get(device_id).cloned()?;
        let g = run.lock().await;
        Some(g.state.clone())
    }

    pub async fn active(&self, device_id: &str) -> bool {
        self.state(device_id).await.is_some_and(|s| s.active)
    }

    /// Stop the test (the operator, a roast starting): the heater goes off and what was
    /// recorded is analysed.
    pub async fn stop(&self, device_id: &str, why: &str) -> bool {
        let Some(run) = self.runs.lock().await.get(device_id).cloned() else {
            return false;
        };
        let mut g = run.lock().await;
        if !g.state.active {
            return false;
        }
        g.stop = Some(why.to_string());
        true
    }

    /// What must hold before a test touches the roaster (the fan included): no test, roast
    /// or drive on it, and telemetry in the last 5 s. Answers the newest sample.
    pub async fn precheck(&self, device_id: &str) -> Result<roastwarden_core::Sample, TuneError> {
        let limits = self.transports.limits(device_id).await;
        if limits.control != crate::roasters::setup::Control::Full {
            let e = crate::transports::transport::TransportError::ControlOff {
                device_id: device_id.to_string(),
                control: limits.control,
                command: "the autotune",
            };
            return Err(TuneError::ControlOff(e.to_string()));
        }
        // The test drives the heater: a roaster without one has nothing to tune.
        if limits.outputs.heater.is_none() {
            let e = crate::transports::transport::TransportError::NotDeclared {
                device_id: device_id.to_string(),
                output: "heater",
            };
            return Err(TuneError::ControlOff(e.to_string()));
        }
        if self.active(device_id).await {
            return Err(TuneError::Busy(
                "a test is already running on this roaster".into(),
            ));
        }
        if let Ok(Some(s)) = crate::roasts::sessions::get_active_session(&self.db, device_id).await
        {
            return Err(TuneError::Busy(format!(
                "a roast is on this roaster ({}): finish or cancel it before an autotune",
                s.name
            )));
        }
        if self
            .replay
            .list()
            .await
            .iter()
            .any(|r| r.device_id == device_id && r.active)
        {
            return Err(TuneError::Busy(
                "Profile drive is driving this roaster".into(),
            ));
        }
        let now_ms = now_ms();
        self.telemetry
            .latest_sample(device_id)
            .filter(|s| now_ms.saturating_sub(s.ts_ms) <= 5_000)
            .ok_or(TuneError::Offline)
    }

    async fn send(&self, device_id: &str, cmd: DeviceCommand) -> Result<(), TuneError> {
        self.transports
            .send(device_id, &cmd, SendOptions::default())
            .await
            .map(|_| ())
            .map_err(|e| TuneError::Transport(e.to_string()))
    }

    /// Start a test. The fan is the caller's to set first (it goes through the fan's own
    /// path: fan assist and the gain schedule hear of it).
    pub async fn start(
        self: &Arc<Self>,
        device_id: &str,
        req: &StartTune,
    ) -> Result<TuneState, TuneError> {
        let floor = self.transports.limits(device_id).await.fan_floor_pct;
        let cfg = req.config(floor).map_err(TuneError::Invalid)?;
        let sample = self.precheck(device_id).await?;
        let Some(bt) = sample.bt else {
            return Err(TuneError::Invalid(
                "the roaster reports no bean temperature".into(),
            ));
        };

        // Manual, heater power on, the lease, then the approach's heater.
        self.send(
            device_id,
            DeviceCommand::Mode {
                mode: ControlMode::Manual,
            },
        )
        .await?;
        self.send(device_id, DeviceCommand::Heater { percent: 0.0 })
            .await?;
        self.send(device_id, DeviceCommand::HeaterEnable { enabled: true })
            .await?;
        let lease = self
            .send(device_id, DeviceCommand::Lease { ms: LEASE_MS })
            .await
            .is_ok();
        let relay = Relay::start(cfg.clone(), 0.0, bt);
        let input = relay.input();
        self.send(device_id, DeviceCommand::Heater { percent: input })
            .await?;

        let state = TuneState {
            device_id: device_id.to_string(),
            run_id: uuid::Uuid::new_v4().to_string(),
            active: true,
            phase: "approach".into(),
            started_at: Utc::now(),
            elapsed_s: 0.0,
            target: req.setpoint,
            setpoint: relay.setpoint(),
            fan_pct: req.fan_pct.or(sample.fan),
            input,
            bias: relay.bias(),
            hysteresis: cfg.hysteresis.unwrap_or(cfg.min_hysteresis),
            noise: None,
            cycles: 0,
            samples: 0,
            bt: Some(bt),
            message: None,
            result_run_id: None,
            quality: None,
        };
        let run = Arc::new(Mutex::new(Run {
            state: state.clone(),
            relay,
            record: Vec::new(),
            t0_ms: sample.ts_ms,
            last_seq: sample.seq,
            last_sample_at: Instant::now(),
            last_sent: Some((input, Instant::now())),
            lease,
            lease_sent_at: lease.then(Instant::now),
            heater_seen_on: false,
            stop: None,
            started: Instant::now(),
        }));
        self.runs
            .lock()
            .await
            .insert(device_id.to_string(), run.clone());
        tokio::spawn(self.clone().drive(device_id.to_string(), run));
        tracing::info!(
            device_id,
            setpoint = req.setpoint,
            "server autotune started"
        );
        Ok(state)
    }

    async fn drive(self: Arc<Self>, device_id: String, run: Arc<Mutex<Run>>) {
        let mut ticker = tokio::time::interval(POLL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let reason = loop {
            ticker.tick().await;
            if let Some(why) = run.lock().await.stop.clone() {
                break why;
            }
            if let Ok(Some(s)) =
                crate::roasts::sessions::get_active_session(&self.db, &device_id).await
            {
                break format!("a roast started on the roaster ({})", s.name);
            }
            let sample = self.telemetry.latest_sample(&device_id);
            let mut send: Option<f64> = None;
            let mut end: Option<String> = None;
            {
                let mut g = run.lock().await;
                g.state.elapsed_s = g.started.elapsed().as_secs_f64();
                if let Some(s) = sample.filter(|s| s.seq > g.last_seq) {
                    let step = g.ingest(&s);
                    send = step.send;
                    end = step.end;
                } else if g.last_sample_at.elapsed() >= STALE_STOP {
                    g.relay.abort("no telemetry from the roaster for 20 s");
                    end = Some("no telemetry from the roaster for 20 s".into());
                }
            }
            if let Some(why) = end {
                break why;
            }
            if let Some(u) = send
                && self
                    .send(&device_id, DeviceCommand::Heater { percent: u })
                    .await
                    .is_ok()
            {
                run.lock().await.last_sent = Some((u, Instant::now()));
            }
            self.refresh_lease(&device_id, &run).await;
        };
        self.finish(&device_id, &run, &reason).await;
    }

    async fn refresh_lease(&self, device_id: &str, run: &Arc<Mutex<Run>>) {
        let due = {
            let g = run.lock().await;
            g.lease
                && g.last_sample_at.elapsed() < LEASE_STALE
                && g.lease_sent_at.is_none_or(|t| t.elapsed() >= LEASE_REFRESH)
        };
        if due
            && self
                .send(device_id, DeviceCommand::Lease { ms: LEASE_MS })
                .await
                .is_ok()
        {
            run.lock().await.lease_sent_at = Some(Instant::now());
        }
    }

    /// Heater off and its power off, the lease released; analyse what was recorded and store
    /// it as a run.
    async fn finish(&self, device_id: &str, run: &Arc<Mutex<Run>>, reason: &str) {
        let lease = {
            let mut g = run.lock().await;
            g.relay.abort(reason);
            g.state.active = false;
            g.state.phase = "analysing".into();
            g.state.message = Some(reason.to_string());
            g.lease
        };
        let _ = self
            .send(device_id, DeviceCommand::Heater { percent: 0.0 })
            .await;
        let _ = self
            .send(device_id, DeviceCommand::HeaterEnable { enabled: false })
            .await;
        if lease {
            let _ = self.send(device_id, DeviceCommand::Lease { ms: 0 }).await;
        }
        tracing::info!(device_id, reason, "server autotune ended");

        let (record, summary, state) = {
            let g = run.lock().await;
            (g.record.clone(), g.relay.summary(), g.state.clone())
        };
        let samples = Samples::from_record(&record);
        let analysis = {
            let (s, sm) = (samples.clone(), summary.clone());
            tokio::task::spawn_blocking(move || analyse_samples(&s, &sm))
                .await
                .ok()
        };
        let stored = to_run(device_id, &state, &summary, analysis.as_ref(), &samples);
        let ok = match tuning::insert_server_run(&self.db, &stored).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, device_id, "could not store the autotune run");
                false
            }
        };
        let mut g = run.lock().await;
        g.state.phase = match summary.phase {
            Phase::Done => "done",
            _ if g.stop.is_some() => "stopped",
            _ => "failed",
        }
        .into();
        if ok {
            g.state.result_run_id = Some(stored.run.id.clone());
            g.state.quality.clone_from(&stored.run.quality);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Approach => "approach",
        Phase::Symmetric => "symmetric",
        Phase::Asymmetric => "asymmetric",
        Phase::Done => "done",
        Phase::Failed => "failed",
    }
}

/// A run's record as stored (`GET …/runs/{id}/samples`): time from the start (s), the heater
/// in force over each interval (%), BT (°C), fan (%).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Samples {
    pub t: Vec<f64>,
    pub u: Vec<f64>,
    pub y: Vec<f64>,
    pub fan: Vec<Option<f64>>,
    /// The chosen model's output over the relay part, for plotting against `y` (filled when
    /// served, not stored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelTrace>,
}

/// A model's output at times `t`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModelTrace {
    pub t: Vec<f64>,
    pub y: Vec<f64>,
}

impl Samples {
    /// Add the chosen model's output (rounded like the record).
    pub fn with_model(mut self, summary: &RelaySummary, a: &Analysis) -> Samples {
        let p = roastwarden_autotune::analysis::prediction(
            Record {
                t: &self.t,
                u: &self.u,
                y: &self.y,
            },
            summary,
            a,
        );
        self.model = p.map(|(t, y)| ModelTrace {
            t,
            y: y.into_iter().map(|v| (v * 100.0).round() / 100.0).collect(),
        });
        self
    }
}

impl Samples {
    /// The heater in force from sample k to k+1 is what the roaster reported applying at
    /// k+1 (the command sent at k lands before the next sample); the commanded value when it
    /// does not report one.
    fn from_record(r: &[Rec]) -> Samples {
        let round = |v: f64, d: f64| (v * d).round() / d;
        Samples {
            t: r.iter().map(|x| round(x.t, 1000.0)).collect(),
            u: (0..r.len())
                .map(|i| {
                    let v = r.get(i + 1).and_then(|n| n.applied).unwrap_or(r[i].cmd);
                    round(v, 100.0)
                })
                .collect(),
            y: r.iter().map(|x| round(x.y, 100.0)).collect(),
            fan: r.iter().map(|x| x.fan.map(|f| round(f, 10.0))).collect(),
            model: None,
        }
    }
}

/// Analyse a stored record with its experiment's summary.
pub fn analyse_samples(s: &Samples, summary: &RelaySummary) -> Analysis {
    analyse(
        Record {
            t: &s.t,
            u: &s.u,
            y: &s.y,
        },
        summary,
        CONTROL_DT,
    )
}

/// A run's summary fields from its analysis: quality, warnings, Ku (the firmware's units,
/// 0–255 per °C, as the first autotune reported it), Tu, the holding power.
pub fn with_analysis(mut run: AutotuneRun, a: &Analysis) -> AutotuneRun {
    run.quality = Some(
        match a.quality {
            Quality::Good => "good",
            Quality::Acceptable => "acceptable",
            Quality::Poor => "poor",
        }
        .to_string(),
    );
    run.warnings = a.warnings.iter().map(|w| w.code.clone()).collect();
    run.ku = a.ku.map(|k| k * 2.55);
    run.tu = a.tu;
    if let Some(b) = a.fitted_holding_power.or(a.holding_power) {
        run.bias = Some(b);
    }
    run
}

pub struct StoredRun {
    pub run: AutotuneRun,
    pub analysis: Option<Analysis>,
    pub samples: Samples,
}

fn to_run(
    device_id: &str,
    state: &TuneState,
    summary: &RelaySummary,
    analysis: Option<&Analysis>,
    samples: &Samples,
) -> StoredRun {
    let run = AutotuneRun {
        id: state.run_id.clone(),
        device_id: device_id.to_string(),
        created_at: state.started_at,
        state: match summary.phase {
            Phase::Done => "complete".into(),
            _ => "failed".into(),
        },
        quality: None,
        message: state.message.clone(),
        fan_pct: state.fan_pct,
        target_temperature: Some(state.target),
        test_temperature: Some(summary.setpoint),
        ku: None,
        tu: None,
        bias: summary.biases.last().copied(),
        tuning_method: Some("simc_pi".into()),
        warnings: Vec::new(),
        payload: serde_json::to_value(summary).unwrap_or_default(),
        source: "server".into(),
    };
    StoredRun {
        run: match analysis {
            Some(a) => with_analysis(run, a),
            None => run,
        },
        analysis: analysis.cloned(),
        samples: samples.clone(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use roastwarden_autotune::Model;
    use roastwarden_autotune::bench::{Bench, BenchConfig};
    use std::collections::BTreeMap;

    fn sample(
        seq: u64,
        ts_ms: u64,
        bt: f64,
        applied: f64,
        heater_on: bool,
    ) -> roastwarden_core::Sample {
        roastwarden_core::Sample {
            seq,
            ts_ms,
            bt: Some(bt),
            et: None,
            ror_bt: None,
            ror_et: None,
            ror_device: None,
            heater: Some(applied),
            fan: Some(72.0),
            setpoint: None,
            mode: Some(0),
            heater_enable: Some(heater_on),
            extras: BTreeMap::from([("heaterApplied".to_string(), applied)]),
        }
    }

    fn new_run(cfg: RelayConfig, bt: f64) -> Run {
        let relay = Relay::start(cfg, 0.0, bt);
        Run {
            state: TuneState {
                device_id: "r".into(),
                run_id: "run".into(),
                active: true,
                phase: "approach".into(),
                started_at: Utc::now(),
                elapsed_s: 0.0,
                target: relay.setpoint(),
                setpoint: relay.setpoint(),
                fan_pct: Some(72.0),
                input: relay.input(),
                bias: relay.bias(),
                hysteresis: 0.2,
                noise: None,
                cycles: 0,
                samples: 0,
                bt: Some(bt),
                message: None,
                result_run_id: None,
                quality: None,
            },
            relay,
            record: Vec::new(),
            t0_ms: 1_000_000,
            last_seq: 0,
            last_sample_at: Instant::now(),
            last_sent: None,
            lease: true,
            lease_sent_at: None,
            heater_seen_on: false,
            stop: None,
            started: Instant::now(),
        }
    }

    /// The loop against a simulated roaster like this one: each command lands before the
    /// next sample, which reports it as applied, as the real one does. Returns the run and
    /// how it ended.
    fn simulate(device_id: &str) -> (Run, Option<String>) {
        let truth = Model::sotd(2.3, 60.0, 8.0, 2.0);
        let mut bench = Bench::new(
            BenchConfig {
                oversample: 4,
                ..BenchConfig::roaster_like(truth)
            },
            0.0,
        );
        let mut run = new_run(
            RelayConfig {
                setpoint: 190.0,
                ..RelayConfig::default()
            },
            bench.step(0.0),
        );
        run.state.device_id = device_id.to_string();
        let mut heater = run.relay.input();
        for k in 1..3000u64 {
            let y = bench.step(heater);
            let step = run.ingest(&sample(k, 1_000_000 + k * 1000, y, heater, true));
            if let Some(u) = step.send {
                heater = u;
                run.last_sent = Some((u, Instant::now()));
            }
            if step.end.is_some() {
                run.state.message = step.end.clone();
                return (run, step.end);
            }
        }
        (run, None)
    }

    /// A finished, analysed server run on a simulated roaster, for tests elsewhere.
    pub(crate) fn simulated_run(device_id: &str) -> StoredRun {
        let (run, _) = simulate(device_id);
        let samples = Samples::from_record(&run.record);
        let summary = run.relay.summary();
        let a = analyse_samples(&samples, &summary);
        to_run(device_id, &run.state, &summary, Some(&a), &samples)
    }

    #[test]
    fn a_test_against_a_simulated_roaster_runs_to_an_analysis() {
        let truth = Model::sotd(2.3, 60.0, 8.0, 2.0);
        let (run, end) = simulate("r");
        assert_eq!(end.as_deref(), Some("finished"), "{:?}", run.state);
        assert_eq!(run.relay.phase(), Phase::Done);
        let samples = Samples::from_record(&run.record);
        // The heater over each interval is what the next sample reported applied.
        assert_eq!(samples.u[0], run.record[1].applied.unwrap());
        let a = analyse_samples(&samples, &run.relay.summary());
        assert_ne!(a.quality, Quality::Poor, "{:#?}", a.warnings);
        let rec = &a.designs[a.recommended.unwrap()];
        let on_truth = roastwarden_autotune::evaluate(&truth, &rec.pid, None, 1.0).unwrap();
        assert!(on_truth.ms < 1.8, "{}", on_truth.ms);
        let stored = to_run("r", &run.state, &run.relay.summary(), Some(&a), &samples);
        assert_eq!(stored.run.source, "server");
        assert_eq!(stored.run.state, "complete");
        assert!(stored.run.ku.unwrap() > 0.0 && stored.run.bias.unwrap() > 50.0);
    }

    #[test]
    fn the_roaster_cutting_its_heater_ends_the_test() {
        let mut run = new_run(RelayConfig::default(), 25.0);
        // Before the first sample shows heater power on, an off is the command not landed yet.
        assert_eq!(
            run.ingest(&sample(1, 1_001_000, 25.0, 0.0, false)).end,
            None
        );
        assert_eq!(
            run.ingest(&sample(2, 1_002_000, 26.0, 100.0, true)).end,
            None
        );
        let step = run.ingest(&sample(3, 1_003_000, 27.0, 0.0, false));
        assert!(step.end.unwrap().contains("cut its heater"));
    }

    #[test]
    fn start_options_are_checked() {
        let ok = StartTune {
            setpoint: 190.0,
            fan_pct: Some(72.0),
            approach_power: None,
            max_swing: Some(25.0),
            asymmetry: Some(1.0),
            hysteresis: None,
        };
        let c = ok.config(45.0).unwrap();
        assert_eq!(c.max_swing, 25.0);
        assert_eq!(c.asym_cycles, 0, "asymmetry 1 skips the asymmetric phase");
        for bad in [
            StartTune {
                setpoint: 300.0,
                ..ok.clone()
            },
            StartTune {
                fan_pct: Some(30.0),
                ..ok.clone()
            },
            StartTune {
                hysteresis: Some(9.0),
                ..ok.clone()
            },
        ] {
            assert!(bad.config(45.0).is_err());
        }
    }
}
