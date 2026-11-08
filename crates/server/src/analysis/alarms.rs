//! Server-side roast alarms, alarm actions and auto events (`docs/alarms.md`).
//!
//! Rules live in the `roast_alarms` setting (the JSON the Settings page
//! edits). Every telemetry sample for a device with an active session is
//! checked here, and a 1 Hz watchdog checks the rules that fire on silence
//! (`no_telemetry`). A rule that fires produces an [`AlarmEvent`]: streamed
//! to dashboards over the telemetry WebSocket, sent to phones and the
//! notification channels (`notify.rs`), and the rule's actions run (set setpoint/fan/heater/mode, heater
//! off, emergency stop, mark a landmark, pause or stop the replay).
//!
//! The same loop marks landmarks automatically when `auto_event_detection`
//! is on: the turning point, DRY end at the profile's dry-end BT (else `auto_dry_temp`) and DROP (a sustained
//! BT fall after development, `auto_drop`). First crack stays a prompt in the
//! dashboard: an RoR inflection is too unreliable to act on.
//!
//! Rule evaluation ([`condition_hit`], [`RuleState::should_fire`]) and auto
//! event detection ([`AutoEvents`]) are pure and unit-tested.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

use crate::analysis::telemetry::{TelemetryEvent, TelemetryService};
use crate::control::fan_assist::FanAssistService;
use crate::control::schedule::GainScheduler;
use crate::predict::ror_watch::{RorWarningKind, RorWatch, AHEAD_S};
use crate::predict::{PredictService, WARN_HOLD_S};
use crate::replay::engine::{interpolate, Channel, Plan};
use crate::replay::ReplayService;
use crate::roasts::types::{
    CreateRoastEventRequest, RoastEvent, RoastEventType, RoastSession, SessionStatus,
};
use crate::transports::transport::{SendOptions, TransportRegistry};
use roastwarden_core::{ControlMode, DeviceCommand};

const RULES_TTL: Duration = Duration::from_secs(10);
const SESSION_TTL: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_secs(1);
/// Seconds between repeats of a `repeat` rule when it sets no cooldown.
const DEFAULT_COOLDOWN_S: f64 = 60.0;

fn default_true() -> bool {
    true
}

// ------------------------------------------------------------------ rules --

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlarmRule {
    pub name: String,
    /// `temp_above`, `temp_below`, `et_above`, `et_below`, `ror_above`,
    /// `ror_below`, `delta_above`, `delta_below`, `channel_above`,
    /// `channel_below`, `time_after_event`, `no_telemetry`, `crash_ahead`, `flick_ahead`,
    /// `stall_ahead` (the RoR watch's warnings, docs/prediction.md "RoR warnings").
    pub condition_type: String,
    /// In °C, °C/min, s, or the channel's unit for `channel_*` (°C for a `C` channel); for
    /// `*_ahead` the seconds ahead it warns (0 to 120: 0 only once it is seen).
    pub threshold: f64,
    /// For `channel_above` / `channel_below`: the declared channel's key (docs/channels.md).
    /// A roaster that has not declared it never fires the rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// For `time_after_event`: the landmark the time counts from. For any
    /// other condition (as the dashboard always did for `ror_below`): the
    /// rule is armed only once this landmark is marked.
    #[serde(default)]
    pub reference_event: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Armed only once this landmark is marked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armed_after: Option<String>,
    /// Armed only once the alarm with this name has fired in this roast.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    /// Fire again each time the condition comes back (after the cooldown);
    /// otherwise once per roast.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeat: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AlarmAction>,
}

/// What a fired alarm does besides announcing itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlarmAction {
    /// Setpoint, °C.
    Setpoint {
        value: f64,
    },
    /// Fan, %.
    Fan {
        value: f64,
    },
    /// Manual heater power, %.
    Heater {
        value: f64,
    },
    /// `auto` or `manual`.
    Mode {
        mode: String,
    },
    HeaterOff,
    EmergencyStop,
    /// Mark a landmark (event code, e.g. `drop`) unless it is marked already.
    Mark {
        event: String,
    },
    ReplayPause,
    ReplayStop,
}

impl AlarmAction {
    pub fn describe(&self) -> String {
        match self {
            AlarmAction::Setpoint { value } => format!("setpoint {value:.0} °C"),
            AlarmAction::Fan { value } => format!("fan {value:.0} %"),
            AlarmAction::Heater { value } => format!("heater {value:.0} %"),
            AlarmAction::Mode { mode } => format!("mode {mode}"),
            AlarmAction::HeaterOff => "heater off".into(),
            AlarmAction::EmergencyStop => "emergency stop".into(),
            AlarmAction::Mark { event } => format!("mark {event}"),
            AlarmAction::ReplayPause => "pause replay".into(),
            AlarmAction::ReplayStop => "stop replay".into(),
        }
    }

    /// Value ranges, checked when rules load.
    fn validate(&self) -> Result<(), String> {
        let pct = |v: f64| v.is_finite() && (0.0..=100.0).contains(&v);
        match self {
            AlarmAction::Setpoint { value }
                if !(value.is_finite() && (0.0..=300.0).contains(value)) =>
            {
                Err("setpoint must be 0..300 °C".into())
            }
            AlarmAction::Fan { value } | AlarmAction::Heater { value } if !pct(*value) => {
                Err("fan and heater must be 0..100 %".into())
            }
            AlarmAction::Mode { mode } if mode.parse::<ControlMode>().is_err() => {
                Err("mode must be auto or manual".into())
            }
            AlarmAction::Mark { event } if event.parse::<RoastEventType>().is_err() => {
                Err(format!("unknown landmark '{event}'"))
            }
            _ => Ok(()),
        }
    }
}

const CONDITIONS: &[&str] = &[
    "temp_above",
    "temp_below",
    "et_above",
    "et_below",
    "ror_above",
    "ror_below",
    "delta_above",
    "delta_below",
    "channel_above",
    "channel_below",
    "time_after_event",
    "no_telemetry",
    "crash_ahead",
    "flick_ahead",
    "stall_ahead",
];

/// The RoR watch's warning a `*_ahead` condition reads.
fn ahead_kind(condition: &str) -> Option<RorWarningKind> {
    RorWarningKind::parse(condition.strip_suffix("_ahead")?)
}

impl AlarmRule {
    pub fn validate(&self) -> Result<(), String> {
        if !CONDITIONS.contains(&self.condition_type.as_str()) {
            return Err(format!("unknown condition '{}'", self.condition_type));
        }
        if !self.threshold.is_finite() {
            return Err("threshold must be a number".into());
        }
        if ahead_kind(&self.condition_type).is_some()
            && !(0.0..=AHEAD_S as f64).contains(&self.threshold)
        {
            return Err(format!(
                "{} warns 0 to {AHEAD_S} s ahead",
                self.condition_type
            ));
        }
        if self.condition_type == "time_after_event" && self.reference_event.is_none() {
            return Err("time_after_event needs reference_event".into());
        }
        if self.condition_type.starts_with("channel_")
            && self.channel.as_deref().is_none_or(|c| c.trim().is_empty())
        {
            return Err(format!("{} needs channel", self.condition_type));
        }
        for a in &self.actions {
            a.validate()?;
        }
        Ok(())
    }

    /// The landmark that arms the rule, if any.
    fn arming_event(&self) -> Option<&str> {
        self.armed_after
            .as_deref()
            .or(if self.condition_type == "time_after_event" {
                None
            } else {
                self.reference_event.as_deref()
            })
    }
}

/// Parse the `roast_alarms` setting rule by rule: one bad rule is skipped
/// (and logged), not the whole list.
pub fn parse_rules(json: &str) -> Vec<AlarmRule> {
    let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        tracing::warn!("roast_alarms is not a JSON list; no alarms");
        return Vec::new();
    };
    values
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<AlarmRule>(v) {
            Ok(r) => match r.validate() {
                Ok(()) => Some(r),
                Err(e) => {
                    tracing::warn!(alarm = %r.name, error = %e, "alarm rule skipped");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "alarm rule skipped");
                None
            }
        })
        .collect()
}

/// What a rule is evaluated against.
#[derive(Debug, Clone, Default)]
pub struct Ctx<'a> {
    /// Seconds since the session started.
    pub elapsed: f64,
    pub bt: Option<f64>,
    pub et: Option<f64>,
    /// °C/min.
    pub ror: Option<f64>,
    /// BT minus the profile BT now, °C.
    pub delta: Option<f64>,
    /// Seconds since the device's last sample.
    pub since_sample: f64,
    /// The roaster's declared channels in this sample: key → value (docs/channels.md).
    /// An undeclared channel is not here, so a rule naming it never fires.
    pub channels: Option<&'a HashMap<String, ChannelNow>>,
    /// Landmark code → elapsed seconds.
    pub marks: Option<&'a HashMap<String, f64>>,
    /// The live prediction's RoR watch (docs/prediction.md), only when a rule reads it.
    pub ror_watch: Option<&'a RorWatch>,
}

/// A declared channel's value now, with what the message calls it.
#[derive(Debug, Clone)]
pub struct ChannelNow {
    pub label: String,
    /// ` °C`, ` %`, ` kPa`
    pub unit: String,
    pub value: f64,
}

/// The declared channels' values in a sample.
pub fn channels_now(
    declared: &[crate::roasters::channels::Channel],
    extras: &std::collections::BTreeMap<String, f64>,
) -> HashMap<String, ChannelNow> {
    declared
        .iter()
        .filter_map(|c| {
            let v = extras.get(&c.key).copied().filter(|v| v.is_finite())?;
            Some((
                c.key.clone(),
                ChannelNow {
                    label: c.label.clone(),
                    unit: c.unit_suffix(),
                    value: v,
                },
            ))
        })
        .collect()
}

impl Ctx<'_> {
    fn mark(&self, code: &str) -> Option<f64> {
        self.marks.and_then(|m| m.get(code).copied())
    }
}

/// Whether the condition holds now: a message and the value that tripped it.
pub fn condition_hit(rule: &AlarmRule, c: &Ctx) -> Option<(String, Option<f64>)> {
    let t = rule.threshold;
    let above = |v: Option<f64>, what: &str, unit: &str| {
        v.filter(|v| *v > t)
            .map(|v| (format!("{what} {v:.1}{unit} > {t}{unit}"), Some(v)))
    };
    let below = |v: Option<f64>, what: &str, unit: &str| {
        v.filter(|v| *v < t)
            .map(|v| (format!("{what} {v:.1}{unit} < {t}{unit}"), Some(v)))
    };
    match rule.condition_type.as_str() {
        "temp_above" => above(c.bt, "BT", " °C"),
        "temp_below" => below(c.bt, "BT", " °C"),
        "et_above" => above(c.et, "ET", " °C"),
        "et_below" => below(c.et, "ET", " °C"),
        "ror_above" => above(c.ror, "RoR", " °C/min"),
        "ror_below" => below(c.ror, "RoR", " °C/min"),
        "delta_above" => above(c.delta, "BT − profile", " °C"),
        "delta_below" => below(c.delta, "BT − profile", " °C"),
        "channel_above" | "channel_below" => {
            let ch = c.channels?.get(rule.channel.as_deref()?)?;
            if rule.condition_type == "channel_above" {
                above(Some(ch.value), &ch.label, &ch.unit)
            } else {
                below(Some(ch.value), &ch.label, &ch.unit)
            }
        }
        "time_after_event" => {
            let code = rule.reference_event.as_deref()?;
            let dt = c.elapsed - c.mark(code)?;
            (dt > t).then(|| (format!("{}s after {code}", dt.round()), Some(dt)))
        }
        "no_telemetry" => (c.since_sample > t).then(|| {
            (
                format!("no telemetry for {:.0} s", c.since_sample),
                Some(c.since_sample),
            )
        }),
        cond => {
            let kind = ahead_kind(cond)?;
            // The last prediction before DROP may still be there for a few seconds.
            if c.mark("drop").is_some() {
                return None;
            }
            let w = c.ror_watch?;
            let x = w
                .warning(kind)
                .filter(|x| w.t - x.since >= WARN_HOLD_S && x.t - w.t <= t)?;
            let mut msg = x.reason.clone();
            if let Some(f) = msg.get(..1) {
                msg = f.to_uppercase() + &msg[1..];
            }
            Some((msg, Some(((x.t - w.t).max(0.0) * 10.0).round() / 10.0)))
        }
    }
}

/// Per-rule state within one roast.
#[derive(Debug, Clone, Default)]
pub struct RuleState {
    pub count: u32,
    last_fired: Option<f64>,
    was_true: bool,
}

impl RuleState {
    /// Feed the condition's value; returns whether the rule fires now. Once
    /// per roast, or with `repeat` each time the condition comes back after
    /// the cooldown.
    pub fn should_fire(&mut self, rule: &AlarmRule, hit: bool, elapsed: f64) -> bool {
        if !hit {
            self.was_true = false;
            return false;
        }
        let edge = !self.was_true;
        self.was_true = true;
        let fire = if self.count == 0 {
            true
        } else {
            let cooldown = rule.cooldown_seconds.unwrap_or(DEFAULT_COOLDOWN_S).max(0.0);
            rule.repeat && edge && self.last_fired.is_none_or(|l| elapsed - l >= cooldown)
        };
        if fire {
            self.count += 1;
            self.last_fired = Some(elapsed);
        }
        fire
    }
}

/// Whether a rule may fire: enabled, its landmark marked, its required alarm
/// fired.
pub fn armed(rule: &AlarmRule, c: &Ctx, states: &HashMap<String, RuleState>) -> bool {
    rule.enabled
        && rule.arming_event().is_none_or(|e| c.mark(e).is_some())
        && rule
            .requires
            .as_deref()
            .is_none_or(|r| states.get(r).is_some_and(|s| s.count > 0))
}

// ------------------------------------------------------------ auto events --

/// Turning point: BT must climb this far above its post-charge minimum...
const TP_RISE_C: f64 = 2.0;
/// A turning point needs BT to have fallen this far below its CHARGE reading. A cold charge
/// (the fluid bed) never dips, and its "minimum" was the first sample: TP at 0:00.
const TP_DIP_C: f64 = 3.0;
/// ...no earlier than this after charge, so the charge transient cannot be
/// mistaken for it.
const TP_MIN_SECS: f64 = 20.0;
/// DRY end is not marked earlier than this after charge.
const DRY_MIN_SECS: f64 = 60.0;
/// BT − profile for the `delta_*` rules, none once DROP is marked: the profile holds its
/// last BT past its end, so cooling beans read as "Behind profile" (it fired 7 s after
/// DROP on batches #6 and #8).
fn profile_delta(
    bt: Option<f64>,
    profile_bt: Option<f64>,
    marks: &HashMap<String, f64>,
) -> Option<f64> {
    if marks.contains_key(&RoastEventType::Drop.to_string()) {
        return None;
    }
    Some(bt? - profile_bt?)
}

/// DROP is armed once FC is marked or BT has reached this.
const DROP_ARM_C: f64 = 190.0;
/// DROP: BT this far below its peak...
const DROP_FALL_C: f64 = 15.0;
/// ...falling at least this fast (°C/min)...
const DROP_ROR: f64 = -20.0;
/// ...for this many samples in a row.
const DROP_SAMPLES: u32 = 5;
/// The DROP time is the last moment BT was within this of its peak.
const DROP_NEAR_PEAK_C: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoConfig {
    pub enabled: bool,
    pub dry_temp: f64,
    pub drop: bool,
}

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dry_temp: 150.0,
            drop: true,
        }
    }
}

/// Landmark detection within one roast.
#[derive(Debug, Clone, Default)]
pub struct AutoEvents {
    /// The CHARGE time the trackers below were collected against.
    charge_ref: Option<f64>,
    /// BT at the first sample from CHARGE.
    bt_at_charge: Option<f64>,
    bt_min: Option<(f64, f64)>,
    /// Settled: a TP was marked, or BT rose without dipping (no TP this roast).
    tp_done: bool,
    /// When BT was seen rising without a dip (a cold charge): rules armed after the
    /// turning point count from here, since no TP is ever marked.
    tp_settled_at: Option<f64>,
    dry_done: bool,
    peak: Option<f64>,
    near_peak_at: Option<f64>,
    falling: u32,
    drop_done: bool,
}

impl AutoEvents {
    /// The marks alarm rules see: with no turning point marked on a roast whose BT rose
    /// without a dip, the moment that was seen stands in for it, so rules armed after the
    /// turning point ("Behind profile") arm on a cold charge too.
    pub fn alarm_marks(&self, marks: &HashMap<String, f64>) -> HashMap<String, f64> {
        let mut m = marks.clone();
        if let Some(t) = self.tp_settled_at {
            m.entry("turning_point".into()).or_insert(t);
        }
        m
    }

    /// One sample: the landmarks to mark now.
    pub fn step(
        &mut self,
        cfg: &AutoConfig,
        elapsed: f64,
        bt: Option<f64>,
        ror: Option<f64>,
        marks: &HashMap<String, f64>,
    ) -> Vec<CreateRoastEventRequest> {
        let mut out = Vec::new();
        if !cfg.enabled {
            return out;
        }
        let Some(bt) = bt else { return out };
        let charge = marks.get("charge").copied().unwrap_or(0.0);
        if elapsed < charge {
            return out;
        }
        // CHARGE marked (or moved) after samples were seen: what came before
        // it is the empty chamber or preheat, not this roast.
        if self.charge_ref != Some(charge) {
            self.charge_ref = Some(charge);
            self.bt_at_charge = None;
            self.bt_min = None;
            // Re-armed: a "no dip" verdict on pre-charge samples says nothing about the roast.
            self.tp_done = false;
            self.tp_settled_at = None;
            self.peak = None;
            self.near_peak_at = None;
            self.falling = 0;
        }
        let auto = |t: RoastEventType, at: f64, temp: f64| CreateRoastEventRequest {
            event_type: t,
            elapsed_seconds: at as f32,
            temperature: Some(temp as f32),
            notes: Some("auto-detected".into()),
        };

        // Turning point: the BT minimum after charge, if BT dipped at all.
        let at_charge = *self.bt_at_charge.get_or_insert(bt);
        if !self.tp_done {
            if marks.contains_key("turning_point") {
                self.tp_done = true;
            } else {
                match self.bt_min {
                    Some((min_t, min_bt)) if bt >= min_bt => {
                        if bt - min_bt >= TP_RISE_C && elapsed - charge >= TP_MIN_SECS {
                            self.tp_done = true;
                            if at_charge - min_bt >= TP_DIP_C {
                                out.push(auto(RoastEventType::TurningPoint, min_t, min_bt));
                            } else {
                                self.tp_settled_at = Some(elapsed);
                            }
                        }
                    }
                    _ => self.bt_min = Some((elapsed, bt)),
                }
            }
        }

        // DRY end at the configured BT, after the turning point (or once BT is rising
        // without one).
        let tp = self.tp_done || marks.contains_key("turning_point");
        if !self.dry_done {
            if marks.contains_key("drying_end") {
                self.dry_done = true;
            } else if tp && bt >= cfg.dry_temp && elapsed - charge >= DRY_MIN_SECS {
                self.dry_done = true;
                out.push(auto(RoastEventType::DryingEnd, elapsed, bt));
            }
        }

        // DROP: a sustained fall from the peak once development is under way.
        if cfg.drop && !self.drop_done {
            if marks.contains_key("drop") {
                self.drop_done = true;
                return out;
            }
            if self.peak.is_none_or(|p| bt > p) {
                self.peak = Some(bt);
            }
            let peak = self.peak.unwrap_or(bt);
            if bt >= peak - DROP_NEAR_PEAK_C {
                self.near_peak_at = Some(elapsed);
            }
            let armed = marks.contains_key("first_crack_start") || peak >= DROP_ARM_C;
            let falling = bt <= peak - DROP_FALL_C && ror.is_some_and(|r| r <= DROP_ROR);
            self.falling = if armed && falling {
                self.falling + 1
            } else {
                0
            };
            if self.falling >= DROP_SAMPLES {
                self.drop_done = true;
                out.push(auto(
                    RoastEventType::Drop,
                    self.near_peak_at.unwrap_or(elapsed),
                    peak,
                ));
            }
        }
        out
    }
}

// ---------------------------------------------------------------- service --

#[derive(Debug, Clone, Serialize)]
pub struct AlarmEvent {
    pub device_id: String,
    pub session_id: String,
    pub session_name: String,
    pub name: String,
    pub condition_type: String,
    pub threshold: f64,
    pub message: String,
    pub value: Option<f64>,
    pub elapsed_seconds: f64,
    pub fired_at: DateTime<Utc>,
    /// What the alarm did, one line per action ("fan 80 %", "heater off:
    /// failed: …").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
}

/// A session's landmarks changed (auto event, alarm action, API): dashboards
/// reload them.
#[derive(Debug, Clone, Serialize)]
pub struct SessionEventsChanged {
    pub session_id: String,
}

struct SessionWatch {
    session: RoastSession,
    session_loaded: Instant,
    events: Vec<RoastEvent>,
    events_loaded: Instant,
    states: HashMap<String, RuleState>,
    auto: AutoEvents,
    last_sample: Instant,
    last_bt: Option<f64>,
    /// The session profile's BT curve (seconds from CHARGE, °C), loaded once.
    profile_bt: Option<Vec<(f64, f64)>>,
    /// The profile's own dry-end BT (its `drying_end` landmark), loaded with the curve.
    profile_dry_bt: Option<f64>,
    profile_loaded: bool,
}

impl SessionWatch {
    fn marks(&self) -> HashMap<String, f64> {
        let mut m = HashMap::new();
        for e in &self.events {
            m.entry(e.event_type.to_string())
                .or_insert(f64::from(e.elapsed_seconds));
        }
        m
    }
}

struct Inner {
    rules: Vec<AlarmRule>,
    /// The `roast_alarms` JSON the rules came from: parsed (and bad rules
    /// logged) only when it changes.
    rules_json: Option<String>,
    auto: AutoConfig,
    rules_loaded: Option<Instant>,
    /// `device_id` -> watched active session
    watches: HashMap<String, SessionWatch>,
    /// `device_id` -> last time we looked for an active session and found none
    idle_checked: HashMap<String, Instant>,
}

/// What alarm actions (and the roast sequence) act through.
#[derive(Clone)]
pub struct ActionDeps {
    pub transports: Arc<TransportRegistry>,
    pub replay: Arc<ReplayService>,
    pub scheduler: Arc<GainScheduler>,
    pub fan_assist: Arc<FanAssistService>,
}

struct Fired {
    event: AlarmEvent,
    actions: Vec<AlarmAction>,
    bt: Option<f64>,
}

pub struct AlarmService {
    db: SqlitePool,
    deps: ActionDeps,
    /// The live prediction, for the RoR watch's conditions
    predict: Arc<PredictService>,
    events_tx: broadcast::Sender<AlarmEvent>,
    marks_tx: broadcast::Sender<SessionEventsChanged>,
    inner: Mutex<Inner>,
}

impl AlarmService {
    pub fn new(db: SqlitePool, deps: ActionDeps, predict: Arc<PredictService>) -> Arc<Self> {
        let (events_tx, _) = broadcast::channel(64);
        let (marks_tx, _) = broadcast::channel(64);
        Arc::new(Self {
            db,
            deps,
            predict,
            events_tx,
            marks_tx,
            inner: Mutex::new(Inner {
                rules: Vec::new(),
                rules_json: None,
                auto: AutoConfig::default(),
                rules_loaded: None,
                watches: HashMap::new(),
                idle_checked: HashMap::new(),
            }),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AlarmEvent> {
        self.events_tx.subscribe()
    }

    pub fn subscribe_marks(&self) -> broadcast::Receiver<SessionEventsChanged> {
        self.marks_tx.subscribe()
    }

    /// Send a notice that no rule fired (auto DROP's countdown): dashboards, phones and the
    /// notification channels get it as they get an alarm.
    pub fn announce(&self, e: AlarmEvent) {
        tracing::info!(device_id = %e.device_id, alarm = %e.name, message = %e.message, "notice");
        let _ = self.events_tx.send(e);
    }

    /// Force the rules to be re-read on the next sample.
    pub async fn reload(&self) {
        self.inner.lock().await.rules_loaded = None;
    }

    /// Landmarks changed for a session: refresh them before the next
    /// evaluation and tell dashboards.
    pub async fn invalidate_events(&self, session_id: &str) {
        {
            let mut inner = self.inner.lock().await;
            for w in inner.watches.values_mut() {
                if w.session.id == session_id {
                    w.events_loaded = Instant::now()
                        .checked_sub(SESSION_TTL)
                        .expect("the machine has been up longer than SESSION_TTL (3 s)");
                }
            }
        }
        let _ = self.marks_tx.send(SessionEventsChanged {
            session_id: session_id.to_string(),
        });
    }

    /// Run forever: every telemetry sample, plus a 1 Hz watchdog for the
    /// rules that fire on silence.
    pub async fn run(self: Arc<Self>, telemetry: TelemetryService) {
        let mut rx = telemetry.subscribe();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                r = rx.recv() => match r {
                    Ok(evt) => self.handle(evt).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "alarm engine lagged behind telemetry");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = tick.tick() => self.watchdog().await,
            }
        }
    }

    async fn load_settings(&self, inner: &mut Inner) {
        let get = |key: &'static str| async move {
            sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten()
        };
        let json = get("roast_alarms").await;
        if json != inner.rules_json {
            inner.rules = json.as_deref().map(parse_rules).unwrap_or_default();
            inner.rules_json = json;
        }
        inner.auto = AutoConfig {
            enabled: get("auto_event_detection")
                .await
                .is_none_or(|v| v != "false"),
            dry_temp: get("auto_dry_temp")
                .await
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && (50.0..=250.0).contains(v))
                .unwrap_or(150.0),
            drop: get("auto_drop").await.is_none_or(|v| v != "false"),
        };
        inner.rules_loaded = Some(Instant::now());
        tracing::debug!(
            rules = inner.rules.len(),
            auto = ?inner.auto,
            "alarm rules loaded"
        );
    }

    /// Load the session profile's curve and dry-end BT once.
    async fn load_profile(&self, watch: &mut SessionWatch) {
        if watch.profile_loaded {
            return;
        }
        watch.profile_loaded = true;
        let Some(pid) = watch.session.profile_id.clone() else {
            return;
        };
        if let Ok(Some(p)) = crate::profiles::store::get_profile_with_points(&self.db, &pid).await {
            watch.profile_dry_bt = p
                .events
                .iter()
                .find(|e| {
                    e.action == crate::profiles::types::ProfileAction::Landmark
                        && e.label.as_deref() == Some("drying_end")
                })
                .and_then(|e| e.at_bt)
                .filter(|b| b.is_finite() && (50.0..=250.0).contains(b));
            let plan = Plan::from_profile(&p);
            watch.profile_bt = (!plan.bt.is_empty()).then_some(plan.bt);
        }
    }

    /// The session profile's BT now: the running replay's aligned target,
    /// else the profile curve at the time since CHARGE.
    async fn profile_bt_now(&self, watch: &mut SessionWatch, elapsed: f64) -> Option<f64> {
        if let Some(st) = self.deps.replay.state(&watch.session.id).await {
            if st.active {
                return st.view.target_now;
            }
        }
        self.load_profile(watch).await;
        let curve = watch.profile_bt.as_ref()?;
        let charge = watch
            .events
            .iter()
            .find(|e| e.event_type == RoastEventType::Charge)
            .map_or(0.0, |e| f64::from(e.elapsed_seconds));
        interpolate(curve, elapsed - charge)
    }

    async fn handle(&self, evt: TelemetryEvent) {
        let now = Instant::now();
        let mut inner = self.inner.lock().await;

        if inner
            .rules_loaded
            .is_none_or(|t| now.duration_since(t) > RULES_TTL)
        {
            self.load_settings(&mut inner).await;
        }
        if inner.rules.iter().all(|r| !r.enabled) && !inner.auto.enabled {
            return;
        }

        // Which session, if any, is this device roasting?
        let needs_lookup = match inner.watches.get(&evt.device_id) {
            Some(w) => now.duration_since(w.session_loaded) > SESSION_TTL,
            None => inner
                .idle_checked
                .get(&evt.device_id)
                .is_none_or(|t| now.duration_since(*t) > SESSION_TTL),
        };
        if needs_lookup {
            match crate::roasts::sessions::get_active_session(&self.db, &evt.device_id).await {
                Ok(Some(session)) if session.status == SessionStatus::Active => {
                    let same = inner
                        .watches
                        .get(&evt.device_id)
                        .is_some_and(|w| w.session.id == session.id);
                    if same {
                        let w = inner
                            .watches
                            .get_mut(&evt.device_id)
                            .expect("a watch for this roaster: checked just above");
                        w.session = session;
                        w.session_loaded = now;
                    } else {
                        let events = crate::roasts::events::get_roast_events(&self.db, &session.id)
                            .await
                            .unwrap_or_default();
                        inner.watches.insert(
                            evt.device_id.clone(),
                            SessionWatch {
                                session,
                                session_loaded: now,
                                events,
                                events_loaded: now,
                                states: HashMap::new(),
                                auto: AutoEvents::default(),
                                last_sample: now,
                                last_bt: None,
                                profile_bt: None,
                                profile_dry_bt: None,
                                profile_loaded: false,
                            },
                        );
                    }
                }
                _ => {
                    inner.watches.remove(&evt.device_id);
                    inner.idle_checked.insert(evt.device_id.clone(), now);
                    return;
                }
            }
        }
        let Inner {
            rules,
            auto,
            watches,
            ..
        } = &mut *inner;
        let Some(watch) = watches.get_mut(&evt.device_id) else {
            return;
        };
        if now.duration_since(watch.events_loaded) > SESSION_TTL {
            if let Ok(events) =
                crate::roasts::events::get_roast_events(&self.db, &watch.session.id).await
            {
                watch.events = events;
            }
            watch.events_loaded = now;
        }
        watch.last_sample = now;
        let Some(start) = watch.session.start_time else {
            return;
        };
        let elapsed = (Utc::now() - start).num_milliseconds() as f64 / 1000.0;
        let s = &evt.sample;
        watch.last_bt = s.bt;
        let marks = watch.marks();

        // DRY end at the profile's own dry-end BT when it has one, so the roast's phases
        // line up with the profile's (and an align on DRY compares like with like).
        self.load_profile(watch).await;
        let auto_cfg = AutoConfig {
            dry_temp: watch.profile_dry_bt.unwrap_or(auto.dry_temp),
            ..*auto
        };
        let auto_marks = watch.auto.step(&auto_cfg, elapsed, s.bt, s.ror_bt, &marks);

        let wants_delta = rules
            .iter()
            .any(|r| r.enabled && r.condition_type.starts_with("delta_"));
        // The roaster's declared channels, only when a rule reads one (in memory, no SQL).
        let wants_channels = rules
            .iter()
            .any(|r| r.enabled && r.condition_type.starts_with("channel_"));
        let channels = if wants_channels {
            let declared = self.deps.transports.setups().channels(&evt.device_id).await;
            channels_now(&declared, &s.extras)
        } else {
            HashMap::new()
        };
        let delta = if wants_delta {
            profile_delta(s.bt, self.profile_bt_now(watch, elapsed).await, &marks)
        } else {
            None
        };
        // The RoR watch, only when a rule reads it (in memory: the last prediction).
        let wants_watch = rules
            .iter()
            .any(|r| r.enabled && ahead_kind(&r.condition_type).is_some());
        let ror_watch = if wants_watch {
            self.predict.live(&evt.device_id).await.map(|p| p.ror)
        } else {
            None
        };
        let alarm_marks = watch.auto.alarm_marks(&marks);
        let ctx = Ctx {
            elapsed,
            bt: s.bt,
            et: s.et,
            ror: s.ror_bt,
            delta,
            since_sample: 0.0,
            channels: Some(&channels),
            marks: Some(&alarm_marks),
            ror_watch: ror_watch.as_ref(),
        };
        let fired = evaluate(rules, watch, &evt.device_id, &ctx, |r| {
            r.condition_type != "no_telemetry"
        });
        let session_id = watch.session.id.clone();
        drop(inner);

        for req in auto_marks {
            let kind = req.event_type.to_string();
            match crate::roasts::events::create_roast_event(&self.db, &session_id, req).await {
                Ok(ev) => {
                    tracing::info!(%session_id, event = %kind, elapsed = ev.elapsed_seconds, temp = ?ev.temperature, "landmark auto-marked");
                    self.invalidate_events(&session_id).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, event = %kind, "could not record an auto landmark");
                }
            }
        }
        self.dispatch(fired).await;
    }

    /// Rules that fire on silence, and dropping watches whose session ended.
    async fn watchdog(&self) {
        let now = Instant::now();
        let mut inner = self.inner.lock().await;
        let has_silence_rules = inner
            .rules
            .iter()
            .any(|r| r.enabled && r.condition_type == "no_telemetry");
        if !has_silence_rules || inner.watches.is_empty() {
            return;
        }
        // A silent device sends no samples to refresh its session: check here.
        let stale: Vec<(String, String)> = inner
            .watches
            .iter()
            .filter(|(_, w)| now.duration_since(w.session_loaded) > Duration::from_secs(10))
            .map(|(d, w)| (d.clone(), w.session.id.clone()))
            .collect();
        for (device_id, session_id) in stale {
            match crate::roasts::sessions::get_session(&self.db, &session_id).await {
                Ok(Some(s)) if s.status == SessionStatus::Active => {
                    if let Some(w) = inner.watches.get_mut(&device_id) {
                        w.session = s;
                        w.session_loaded = now;
                    }
                }
                Ok(_) => {
                    inner.watches.remove(&device_id);
                }
                Err(_) => {}
            }
        }
        let Inner { rules, watches, .. } = &mut *inner;
        let mut all = Vec::new();
        for (device_id, watch) in watches.iter_mut() {
            let Some(start) = watch.session.start_time else {
                continue;
            };
            let elapsed = (Utc::now() - start).num_milliseconds() as f64 / 1000.0;
            let marks = watch.auto.alarm_marks(&watch.marks());
            let ctx = Ctx {
                elapsed,
                since_sample: now.duration_since(watch.last_sample).as_secs_f64(),
                marks: Some(&marks),
                ..Ctx::default()
            };
            all.extend(evaluate(rules, watch, device_id, &ctx, |r| {
                r.condition_type == "no_telemetry"
            }));
        }
        drop(inner);
        self.dispatch(all).await;
    }

    /// Run fired alarms' actions and announce them (dashboards, `notify.rs`).
    async fn dispatch(&self, fired: Vec<Fired>) {
        for mut f in fired {
            if !f.actions.is_empty() {
                f.event.actions = self.run_actions(&f).await;
            }
            let alarm = f.event;
            tracing::info!(device_id = %alarm.device_id, alarm = %alarm.name, message = %alarm.message, actions = ?alarm.actions, "roast alarm fired");
            let _ = self.events_tx.send(alarm);
        }
    }

    /// Run a fired alarm's actions in order; one line per action.
    async fn run_actions(&self, f: &Fired) -> Vec<String> {
        let d = &self.deps;
        let dev = f.event.device_id.as_str();
        let sid = f.event.session_id.as_str();
        let why = format!("alarm {}", f.event.name);
        let send = |cmd: DeviceCommand| async move {
            d.transports
                .send(dev, &cmd, SendOptions::default())
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        let mut out = Vec::new();
        for a in &f.actions {
            let res: Result<(), String> = match a {
                AlarmAction::Setpoint { value } => {
                    d.replay
                        .manual_override(dev, &[Channel::Setpoint], "setpoint", Some(*value))
                        .await;
                    send(DeviceCommand::Setpoint { value: *value }).await
                }
                AlarmAction::Fan { value } => {
                    d.replay
                        .manual_override(dev, &[Channel::Fan], "fan", Some(*value))
                        .await;
                    let r = send(DeviceCommand::Fan { percent: *value }).await;
                    if r.is_ok() {
                        d.fan_assist.operator_fan(dev, *value).await;
                        d.scheduler.fan_changed(dev, *value).await;
                    }
                    r
                }
                AlarmAction::Heater { value } => {
                    d.replay
                        .manual_override(dev, &[Channel::Heater], "heater", None)
                        .await;
                    send(DeviceCommand::Heater { percent: *value }).await
                }
                AlarmAction::Mode { mode } => match mode.parse::<ControlMode>() {
                    Ok(m) => {
                        d.replay
                            .manual_override(
                                dev,
                                &[Channel::Setpoint, Channel::Heater],
                                "mode",
                                None,
                            )
                            .await;
                        send(DeviceCommand::Mode { mode: m }).await
                    }
                    Err(e) => Err(e),
                },
                AlarmAction::HeaterOff => {
                    send(DeviceCommand::HeaterEnable { enabled: false }).await
                }
                AlarmAction::EmergencyStop => {
                    let r = send(DeviceCommand::EmergencyStop).await;
                    d.replay.stop_for_device(dev, &why).await;
                    r
                }
                AlarmAction::Mark { event } => {
                    self.mark(sid, event, f.event.elapsed_seconds, f.bt).await
                }
                AlarmAction::ReplayPause => d
                    .replay
                    .set_paused(sid, None, true, &why, None)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                AlarmAction::ReplayStop => {
                    d.replay.stop_for_device(dev, &why).await;
                    Ok(())
                }
            };
            let line = match res {
                Ok(()) => a.describe(),
                Err(e) => {
                    tracing::warn!(device_id = dev, action = %a.describe(), error = %e, "alarm action failed");
                    format!("{}: failed: {e}", a.describe())
                }
            };
            out.push(line);
        }
        out
    }

    async fn mark(
        &self,
        session_id: &str,
        event: &str,
        elapsed: f64,
        bt: Option<f64>,
    ) -> Result<(), String> {
        let kind: RoastEventType = event.parse()?;
        let events = crate::roasts::events::get_roast_events(&self.db, session_id)
            .await
            .map_err(|e| e.to_string())?;
        if kind != RoastEventType::Custom && events.iter().any(|e| e.event_type == kind) {
            return Ok(());
        }
        crate::roasts::events::create_roast_event(
            &self.db,
            session_id,
            CreateRoastEventRequest {
                event_type: kind,
                elapsed_seconds: elapsed as f32,
                temperature: bt.map(|b| b as f32),
                notes: Some("marked by an alarm".into()),
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        self.invalidate_events(session_id).await;
        Ok(())
    }
}

/// Evaluate the rules `which` selects against `ctx`; returns the ones that fire.
fn evaluate(
    rules: &[AlarmRule],
    watch: &mut SessionWatch,
    device_id: &str,
    ctx: &Ctx,
    which: impl Fn(&AlarmRule) -> bool,
) -> Vec<Fired> {
    let mut fired = Vec::new();
    for rule in rules.iter().filter(|r| which(r)) {
        if !armed(rule, ctx, &watch.states) {
            continue;
        }
        let hit = condition_hit(rule, ctx);
        let state = watch.states.entry(rule.name.clone()).or_default();
        if !state.should_fire(rule, hit.is_some(), ctx.elapsed) {
            continue;
        }
        let (message, value) = hit.unwrap_or_default();
        fired.push(Fired {
            event: AlarmEvent {
                device_id: device_id.to_string(),
                session_id: watch.session.id.clone(),
                session_name: watch.session.name.clone(),
                name: rule.name.clone(),
                condition_type: rule.condition_type.clone(),
                threshold: rule.threshold,
                message,
                value,
                elapsed_seconds: ctx.elapsed,
                fired_at: Utc::now(),
                actions: Vec::new(),
            },
            actions: rule.actions.clone(),
            bt: ctx.bt.or(watch.last_bt),
        });
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(cond: &str, t: f64) -> AlarmRule {
        AlarmRule {
            name: cond.into(),
            condition_type: cond.into(),
            threshold: t,
            channel: None,
            reference_event: None,
            enabled: true,
            armed_after: None,
            requires: None,
            repeat: false,
            cooldown_seconds: None,
            actions: vec![],
        }
    }

    #[test]
    fn rules_parse_the_dashboard_json_and_skip_bad_ones() {
        let json = r#"[{"name":"High Temp Warning","condition_type":"temp_above","threshold":230,"enabled":true},
            {"name":"Low RoR Warning","condition_type":"ror_below","threshold":5.0,"reference_event":"first_crack_start","enabled":true},
            {"name":"legacy","condition_type":"temp_below","threshold":50},
            {"name":"cut","condition_type":"temp_above","threshold":235,"actions":[{"type":"heater_off"},{"type":"fan","value":100},{"type":"mark","event":"drop"}],"repeat":true,"cooldown_seconds":30},
            {"name":"bad action","condition_type":"temp_above","threshold":1,"actions":[{"type":"fan","value":150}]},
            {"name":"bad condition","condition_type":"moon_phase","threshold":1},
            "not a rule"]"#;
        let rules = parse_rules(json);
        assert_eq!(rules.len(), 4);
        assert_eq!(rules[1].arming_event(), Some("first_crack_start"));
        assert!(rules[2].enabled, "enabled defaults to true");
        assert_eq!(rules[3].actions.len(), 3);
        assert_eq!(rules[3].actions[1], AlarmAction::Fan { value: 100.0 });
        // Round trip keeps the old shape for old rules.
        let v = serde_json::to_value(&rules[0]).unwrap();
        assert!(v.get("actions").is_none() && v.get("repeat").is_none());
        assert_eq!(
            parse_rules("{}"),
            [] as [crate::analysis::alarms::AlarmRule; 0]
        );
    }

    #[test]
    fn no_profile_delta_after_drop() {
        let mut marks = HashMap::new();
        marks.insert(RoastEventType::FirstCrackStart.to_string(), 468.0);
        assert_eq!(profile_delta(Some(200.0), Some(205.0), &marks), Some(-5.0));
        assert_eq!(profile_delta(None, Some(205.0), &marks), None);
        marks.insert(RoastEventType::Drop.to_string(), 609.0);
        assert_eq!(profile_delta(Some(180.0), Some(210.0), &marks), None);
    }

    #[test]
    fn conditions_cover_bt_et_ror_delta_time_and_silence() {
        let mut marks = HashMap::new();
        marks.insert("first_crack_start".to_string(), 500.0);
        let c = Ctx {
            elapsed: 600.0,
            bt: Some(212.0),
            et: Some(240.0),
            ror: Some(4.0),
            delta: Some(-6.0),
            since_sample: 12.0,
            channels: None,
            marks: Some(&marks),
            ror_watch: None,
        };
        assert!(condition_hit(&rule("temp_above", 210.0), &c).is_some());
        assert!(condition_hit(&rule("temp_below", 210.0), &c).is_none());
        assert!(condition_hit(&rule("et_above", 235.0), &c).is_some());
        assert!(condition_hit(&rule("ror_above", 3.0), &c).is_some());
        assert!(condition_hit(&rule("ror_below", 5.0), &c).is_some());
        assert!(condition_hit(&rule("delta_below", -5.0), &c).is_some());
        assert!(condition_hit(&rule("delta_above", 3.0), &c).is_none());
        assert!(condition_hit(&rule("no_telemetry", 10.0), &c).is_some());
        let mut t = rule("time_after_event", 90.0);
        t.reference_event = Some("first_crack_start".into());
        let (msg, v) = condition_hit(&t, &c).unwrap();
        assert_eq!(v, Some(100.0));
        assert!(msg.contains("first_crack_start"));
        t.reference_event = Some("drop".into());
        assert!(condition_hit(&t, &c).is_none());
        // Missing data never fires.
        assert!(condition_hit(&rule("delta_below", -5.0), &Ctx::default()).is_none());
    }

    /// `channel_above` / `channel_below` on a declared channel, in its unit; a channel the
    /// roaster has not declared (or a sample without it) never fires; the rule needs `channel`.
    #[test]
    fn ror_watch_conditions_fire_on_a_warning_that_held_and_is_near_enough() {
        use crate::predict::ror_watch::RorWarning;
        let watch = |since: f64, t: f64| RorWatch {
            t: 500.0,
            now: Some(3.0),
            trend: Some(-1.0),
            at_60: Some(0.8),
            at_120: Some(0.2),
            armed: true,
            warnings: vec![RorWarning {
                kind: RorWarningKind::Stall,
                t,
                seen: false,
                since,
                reason: "the model expects RoR under 1 °C/min from 9:20".into(),
            }],
        };
        let marks = HashMap::new();
        let w = watch(490.0, 560.0);
        let c = Ctx {
            ror_watch: Some(&w),
            marks: Some(&marks),
            ..Ctx::default()
        };
        let (msg, v) = condition_hit(&rule("stall_ahead", 90.0), &c).unwrap();
        assert!(msg.starts_with("The model expects"), "{msg}");
        assert_eq!(v, Some(60.0));
        // Too far ahead for this rule, or another kind.
        assert!(condition_hit(&rule("stall_ahead", 30.0), &c).is_none());
        assert!(condition_hit(&rule("crash_ahead", 120.0), &c).is_none());
        // Given by one prediction only: not yet.
        let w1 = watch(500.0, 560.0);
        let c1 = Ctx {
            ror_watch: Some(&w1),
            ..Ctx::default()
        };
        assert!(condition_hit(&rule("stall_ahead", 90.0), &c1).is_none());
        // After DROP: never.
        let mut dropped = HashMap::new();
        dropped.insert("drop".to_string(), 495.0);
        let c2 = Ctx {
            ror_watch: Some(&w),
            marks: Some(&dropped),
            ..Ctx::default()
        };
        assert!(condition_hit(&rule("stall_ahead", 90.0), &c2).is_none());
        // Seconds ahead are 0..120.
        assert!(rule("flick_ahead", 121.0).validate().is_err());
        assert!(rule("flick_ahead", 0.0).validate().is_ok());
    }

    #[test]
    fn channel_conditions_hit_miss_and_need_a_declared_channel() {
        use crate::roasters::channels::Channel;
        let declared = vec![
            Channel {
                key: "t3".into(),
                label: "Exhaust".into(),
                unit: "C".into(),
                max: None,
            },
            Channel {
                key: "burner_kpa".into(),
                label: "Gas".into(),
                unit: "kPa".into(),
                max: Some(4.0),
            },
        ];
        let extras = std::collections::BTreeMap::from([
            ("t3".to_string(), 251.0),
            ("burner_kpa".to_string(), 0.8),
            ("t4".to_string(), 999.0),
        ]);
        let now = channels_now(&declared, &extras);
        let c = Ctx {
            channels: Some(&now),
            ..Ctx::default()
        };
        let on = |cond: &str, key: &str, t: f64| AlarmRule {
            channel: Some(key.into()),
            ..rule(cond, t)
        };
        let (msg, v) = condition_hit(&on("channel_above", "t3", 250.0), &c).unwrap();
        assert_eq!(v, Some(251.0));
        assert_eq!(msg, "Exhaust 251.0 °C > 250 °C");
        assert!(condition_hit(&on("channel_above", "t3", 260.0), &c).is_none());
        let (msg, _) = condition_hit(&on("channel_below", "burner_kpa", 1.0), &c).unwrap();
        assert_eq!(msg, "Gas 0.8 kPa < 1 kPa");
        assert!(condition_hit(&on("channel_below", "burner_kpa", 0.5), &c).is_none());
        // t4 is in the sample but not declared: never.
        assert!(condition_hit(&on("channel_above", "t4", 0.0), &c).is_none());
        assert!(condition_hit(&on("channel_above", "t3", 0.0), &Ctx::default()).is_none());
        // The rule must name its channel.
        assert!(rule("channel_above", 1.0).validate().is_err());
        assert!(on("channel_below", "t3", 1.0).validate().is_ok());
        let parsed = parse_rules(
            r#"[{"name":"Exhaust hot","condition_type":"channel_above","channel":"t3","threshold":250},
                {"name":"no channel","condition_type":"channel_above","threshold":250}]"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].channel.as_deref(), Some("t3"));
    }

    #[test]
    fn arming_by_landmark_and_by_another_alarm() {
        let marks: HashMap<String, f64> = HashMap::new();
        let c = Ctx {
            marks: Some(&marks),
            ..Ctx::default()
        };
        let mut states = HashMap::new();
        let mut r = rule("ror_below", 5.0);
        r.reference_event = Some("first_crack_start".into());
        assert!(!armed(&r, &c, &states));
        let mut marked = HashMap::new();
        marked.insert("first_crack_start".to_string(), 1.0);
        let c2 = Ctx {
            marks: Some(&marked),
            ..Ctx::default()
        };
        assert!(armed(&r, &c2, &states));

        let mut chained = rule("temp_above", 1.0);
        chained.requires = Some("first".into());
        assert!(!armed(&chained, &c, &states));
        states.insert(
            "first".into(),
            RuleState {
                count: 1,
                ..RuleState::default()
            },
        );
        assert!(armed(&chained, &c, &states));
        chained.enabled = false;
        assert!(!armed(&chained, &c, &states));
    }

    #[test]
    fn once_per_roast_unless_repeat_with_cooldown() {
        let once = rule("temp_above", 1.0);
        let mut s = RuleState::default();
        assert!(s.should_fire(&once, true, 0.0));
        assert!(!s.should_fire(&once, true, 1.0));
        assert!(!s.should_fire(&once, false, 2.0));
        assert!(!s.should_fire(&once, true, 300.0));

        let mut rep = rule("temp_above", 1.0);
        rep.repeat = true;
        rep.cooldown_seconds = Some(30.0);
        let mut s = RuleState::default();
        assert!(s.should_fire(&rep, true, 0.0));
        // Still true: no refire while the condition holds.
        assert!(!s.should_fire(&rep, true, 100.0));
        // Comes back inside the cooldown: no.
        assert!(!s.should_fire(&rep, false, 101.0));
        let mut s2 = RuleState::default();
        assert!(s2.should_fire(&rep, true, 0.0));
        assert!(!s2.should_fire(&rep, false, 5.0));
        assert!(!s2.should_fire(&rep, true, 10.0));
        assert!(!s2.should_fire(&rep, false, 20.0));
        assert!(s2.should_fire(&rep, true, 40.0));
        assert_eq!(s2.count, 2);
    }

    /// Feed (elapsed, bt, ror) samples; collect marked landmark codes with time.
    fn run_auto(cfg: AutoConfig, samples: &[(f64, f64, f64)]) -> Vec<(String, f64, f64)> {
        let mut a = AutoEvents::default();
        let mut marks: HashMap<String, f64> = HashMap::new();
        let mut out = Vec::new();
        for &(t, bt, ror) in samples {
            for e in a.step(&cfg, t, Some(bt), Some(ror), &marks) {
                let code = e.event_type.to_string();
                marks.insert(code.clone(), f64::from(e.elapsed_seconds));
                out.push((
                    code,
                    f64::from(e.elapsed_seconds),
                    f64::from(e.temperature.unwrap()),
                ));
            }
        }
        out
    }

    /// A cold-charge fluid-bed roast: 25 °C rising to a 215 °C peak at 660 s,
    /// then dumped (BT falls at 60 °C/min).
    fn roast() -> Vec<(f64, f64, f64)> {
        let mut v = Vec::new();
        for t in 0..=660 {
            let t = t as f64;
            let bt = 25.0 + 190.0 * (1.0 - (-t / 220.0).exp()) / (1.0 - (-3.0f64).exp());
            v.push((t, bt.min(215.0), 10.0));
        }
        for k in 1..=60 {
            v.push((660.0 + k as f64, 215.0 - k as f64, -60.0));
        }
        v
    }

    #[test]
    fn auto_events_mark_tp_dry_and_drop_once() {
        let marks = run_auto(AutoConfig::default(), &roast());
        let codes: Vec<&str> = marks.iter().map(|m| m.0.as_str()).collect();
        // A cold charge: BT never dips, so no turning point.
        assert_eq!(codes, vec!["drying_end", "drop"]);
        let dry = &marks[0];
        assert!(dry.2 >= 150.0 && dry.2 < 152.0, "{dry:?}");
        let drop = &marks[1];
        // DROP at the last moment near the peak, at the peak temperature.
        assert!((drop.1 - 660.0).abs() <= 3.0, "{drop:?}");
        assert!((drop.2 - 215.0).abs() < 0.01);
    }

    #[test]
    fn a_cold_charge_arms_turning_point_rules_without_a_mark() {
        let mut a = AutoEvents::default();
        let mut marks = HashMap::new();
        marks.insert("charge".to_string(), 0.0);
        let mut settled = None;
        for (t, bt, ror) in roast().into_iter().take(120) {
            a.step(&AutoConfig::default(), t, Some(bt), Some(ror), &marks);
            if settled.is_none() {
                settled = a.alarm_marks(&marks).get("turning_point").copied();
            }
        }
        // BT rose 2 °C without a dip after 20 s: rules armed after TP count from there.
        let at = settled.expect("armed");
        assert!((20.0..30.0).contains(&at), "{at}");
        assert!(!marks.contains_key("turning_point"), "nothing is marked");
        // A marked TP wins.
        marks.insert("turning_point".to_string(), 50.0);
        assert_eq!(a.alarm_marks(&marks)["turning_point"], 50.0);
    }

    #[test]
    fn hot_charge_marks_the_turning_point_at_the_dip() {
        // A drum charged at 200 °C: BT falls to 90 °C at 60 s, then climbs.
        let v: Vec<(f64, f64, f64)> = (0..400)
            .map(|t| {
                let t = t as f64;
                let bt = if t < 60.0 {
                    200.0 - 110.0 * t / 60.0
                } else {
                    90.0 + (t - 60.0) * 0.4
                };
                (t, bt, 0.0)
            })
            .collect();
        let marks = run_auto(AutoConfig::default(), &v);
        assert_eq!(marks[0].0, "turning_point");
        assert!((marks[0].1 - 60.0).abs() < 0.5, "{:?}", marks[0]);
        assert!((marks[0].2 - 90.0).abs() < 0.01);
        assert_eq!(marks[1].0, "drying_end");
    }

    #[test]
    fn turning_point_ignores_samples_before_a_late_charge() {
        // A recorded roast (id 1622b15b): idle at 25 °C, CHARGE marked at 39 s, then BT
        // rises. TP was reported at 1.6 s from the pre-charge minimum. Here the
        // pre-charge chamber is hot (60 °C) and the beans dip to 40 °C after CHARGE.
        let cfg = AutoConfig::default();
        let mut a = AutoEvents::default();
        let mut marks: HashMap<String, f64> = HashMap::new();
        let mut tp = None;
        for k in 0..200 {
            let t = k as f64;
            if k == 39 {
                marks.insert("charge".into(), 39.0);
            }
            let bt = if t < 39.0 {
                25.0 + t
            } else if t < 50.0 {
                64.0 - (t - 39.0) * 2.0
            } else {
                42.0 + (t - 50.0) * 0.5
            };
            for e in a.step(&cfg, t, Some(bt), Some(10.0), &marks) {
                if e.event_type.to_string() == "turning_point" {
                    tp = Some((
                        f64::from(e.elapsed_seconds),
                        f64::from(e.temperature.unwrap()),
                    ));
                }
            }
        }
        let (at, temp) = tp.expect("turning point marked");
        assert!(at >= 39.0, "TP at {at} s, before CHARGE");
        assert!((temp - 42.0).abs() < 0.01, "TP BT {temp}");
    }

    #[test]
    fn drop_needs_development_and_a_sustained_fall() {
        // A dip at 170 °C (fan change) is not a DROP: not armed below 190 °C.
        let mut v: Vec<(f64, f64, f64)> = (0..300)
            .map(|t| (t as f64, 100.0 + t as f64 * 0.25, 15.0))
            .collect();
        for k in 1..=20 {
            v.push((300.0 + k as f64, 175.0 - k as f64, -60.0));
        }
        assert!(!run_auto(AutoConfig::default(), &v)
            .iter()
            .any(|m| m.0 == "drop"));
        // Above 190 °C, a single glitch sample is not either.
        let mut g: Vec<(f64, f64, f64)> = (0..200)
            .map(|t| (t as f64, 150.0 + t as f64 * 0.25, 15.0))
            .collect();
        g.push((200.0, 170.0, -80.0));
        g.extend((201..260).map(|t| (t as f64, 200.0, 5.0)));
        assert!(!run_auto(AutoConfig::default(), &g)
            .iter()
            .any(|m| m.0 == "drop"));
        // Switched off.
        let off = AutoConfig {
            drop: false,
            ..AutoConfig::default()
        };
        assert!(!run_auto(off, &roast()).iter().any(|m| m.0 == "drop"));
        let none = AutoConfig {
            enabled: false,
            ..AutoConfig::default()
        };
        assert_eq!(
            run_auto(none, &roast()),
            [] as [(std::string::String, f64, f64); 0]
        );
    }
}
