//! Replay engine core: a pure state machine, no I/O.
//!
//! The service feeds it one [`TickInput`] per second (session time, the
//! device's latest BT/RoR, the landmarks marked so far) and gets back the
//! device commands to send and the log entries to persist. Everything the
//! replay decides — the CHARGE time base, align offsets, which events fire,
//! catch-up coalescing, paused channels, the server PID — lives here so
//! it can be tested without a database, a broker or a clock. See
//! `docs/replay.md` for the behaviour this implements.
//!
//! `config.rs` is the run's vocabulary and configuration, `pid.rs` the server PID, `plan.rs` the
//! profile compiled for replay, `tick.rs` the second's step (time base, targets, sends),
//! `events.rs` the profile's events, `fan.rs` fan assist inside the drive, `controls.rs` what the
//! operator changes mid-run, `view.rs` the state snapshot and the log.

mod config;
mod controls;
mod events;
mod fan;
mod pid;
mod plan;
#[cfg(test)]
mod tests;
mod tick;
mod view;

use std::collections::{BTreeMap, VecDeque};

pub use config::{AlignOn, Channel, Hold, LoopAt, ReplayMode, RunConfig, TriggerPolicy};
pub use pid::{PidGains, ServerPid};
pub use plan::{interpolate, Plan};
pub use tick::{Mark, TickInput};
pub use view::{
    AlignView, Clock, EngineView, EventStatus, EventView, LogEntry, LogKind, PlanAhead,
};

use super::auto_drop::DropRt;
use crate::control::fan_assist::{AssistView, FanAssist};

/// Driven channels are re-sent this often even when unchanged, so a lost
/// command does not stick.
pub const REFRESH_SECS: f64 = 15.0;
/// Turning point (same rule as the auto-TP detector): BT climbs this far
/// above its post-charge minimum...
const TP_RISE_C: f64 = 2.0;
/// ...no earlier than this after CHARGE.
const TP_MIN_SECS: f64 = 20.0;
/// A cue already this far past when the run starts is skipped, not fired.
const CUE_GRACE_SECS: f64 = 2.0;
/// Log entries kept in memory for the state snapshot.
const RECENT_LEN: usize = 10;
/// Upcoming events in the state snapshot.
const UPCOMING_LEN: usize = 5;
/// Adaptive lookahead (`docs/replay.md`, "Lookahead"): it starts adapting this
/// long after CHARGE (the loop lags most while the beans first heat)...
const ADAPT_FROM_SECS: f64 = 60.0;
/// ...closes the gap between BT and the profile with this time constant...
const ADAPT_TAU_SECS: f64 = 45.0;
/// ...and only where the profile still climbs this fast (°C/s, 3 °C/min):
/// on a flat stretch a degree is many seconds.
const ADAPT_MIN_SLOPE: f64 = 0.05;
/// A tick's reading counts as at most this many seconds ahead or behind.
const ADAPT_MAX_AHEAD_SECS: f64 = 30.0;
/// The fluid bed's fan floor, the default for a run with none saved (a run stored before runs kept
/// their roaster's floor): above the firmware's 40 % heater interlock. A run takes its roaster's
/// floor.
pub const MIN_DRIVE_FAN_PCT: f64 = crate::roasters::setup::FLUID_BED_FAN_FLOOR_PCT;

#[derive(Debug, Clone)]
struct EventRt {
    status: EventStatus,
    delay_s: f64,
    force: bool,
    /// Session elapsed when it fired (or was held / superseded).
    done_elapsed: Option<f64>,
    /// Order of application, for recomputing a channel's value after re-arm.
    seq: u64,
}

#[derive(Debug, Clone, Default)]
struct ChannelRt {
    driven: bool,
    /// The value goes to the device (a server-loop setpoint stays internal).
    send: bool,
    event_driven: bool,
    paused: bool,
    /// Operator value while paused (a manual setpoint in a server-loop run).
    held: Option<f64>,
    event_value: Option<f64>,
    target: Option<f64>,
    last_sent: Option<f64>,
    last_sent_elapsed: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct TpTracker {
    min: Option<f64>,
    turned: bool,
}

impl TpTracker {
    fn update(&mut self, live_t: f64, bt: Option<f64>) {
        if self.turned || live_t < 0.0 {
            return;
        }
        let Some(b) = bt else { return };
        match self.min {
            Some(m) if b - m >= TP_RISE_C && live_t >= TP_MIN_SECS => self.turned = true,
            Some(m) if b >= m => {}
            _ => self.min = Some(b),
        }
    }
}

pub struct Engine {
    plan: Plan,
    cfg: RunConfig,
    events: Vec<EventRt>,
    channels: BTreeMap<Channel, ChannelRt>,
    charge: Option<f64>,
    charge_marked: bool,
    tp: TpTracker,
    past_tp: bool,
    align: AlignView,
    pid: ServerPid,
    pid_out: Option<f64>,
    started: bool,
    seq: u64,
    clock: Clock,
    prev_elapsed: Option<f64>,
    last_bt: Option<f64>,
    last_ror: Option<f64>,
    recent: VecDeque<LogEntry>,
    assist: FanAssist,
    /// The fan channel exists for the assist alone (the profile does not
    /// drive the fan): its ceiling is `fan_ceiling`.
    assist_channel: bool,
    fan_ceiling: Option<f64>,
    assist_view: Option<AssistView>,
    /// The lookahead in use (s) and the value last written to the log.
    lead: f64,
    lead_logged: f64,
    /// A profile fan below the interlock was raised (logged once).
    fan_raised: bool,
    drop_rt: DropRt,
}

impl Engine {
    pub fn new(plan: Plan, cfg: RunConfig) -> Engine {
        let events = plan
            .events
            .iter()
            .map(|_| EventRt {
                status: EventStatus::Pending,
                delay_s: 0.0,
                force: false,
                done_elapsed: None,
                seq: 0,
            })
            .collect();
        let pid = ServerPid::new(cfg.server_pid);
        let lead = cfg.lookahead_seconds;
        let assist_channel =
            Self::assist_wanted(&cfg) && !(cfg.fan && plan.has_data_for(Channel::Fan));
        let mut e = Engine {
            plan,
            cfg,
            events,
            channels: BTreeMap::new(),
            charge: None,
            charge_marked: false,
            tp: TpTracker::default(),
            past_tp: false,
            align: AlignView {
                on: AlignOn::Charge,
                applied: true,
                offset_seconds: 0.0,
                offset_c: None,
                profile_landmark_t: Some(0.0),
            },
            pid,
            pid_out: None,
            started: false,
            seq: 0,
            clock: Clock::default(),
            prev_elapsed: None,
            last_bt: None,
            last_ror: None,
            recent: VecDeque::new(),
            assist: FanAssist::default(),
            assist_channel,
            fan_ceiling: None,
            assist_view: None,
            lead,
            lead_logged: lead,
            fan_raised: false,
            drop_rt: DropRt::default(),
        };
        e.configure_channels();
        e
    }

    pub fn config(&self) -> &RunConfig {
        &self.cfg
    }

    /// Switch the server PID gains mid-run (gain schedule). The integral
    /// takes up the change in the P term, so the output does not jump.
    /// Returns whether they changed.
    pub fn set_server_gains(&mut self, g: PidGains) -> bool {
        if self.cfg.server_pid == g {
            return false;
        }
        self.cfg.server_pid = g;
        self.pid.set_gains(g);
        true
    }

    #[cfg(test)]
    pub fn clock(&self) -> Clock {
        self.clock
    }

    fn configure_channels(&mut self) {
        let mode = self.cfg.mode;
        let loop_at = self.cfg.loop_at;
        for ch in Channel::ALL {
            let has = self.plan.has_data_for(ch);
            let (driven, send) = match ch {
                Channel::Setpoint => (mode.closed_loop() && has, loop_at == LoopAt::Device),
                Channel::Heater => (
                    match (mode, loop_at) {
                        (ReplayMode::Feedforward, _)
                        | (ReplayMode::FeedforwardPid, LoopAt::Device) => has,
                        (_, LoopAt::Server) => self.plan.has_data_for(Channel::Setpoint),
                        _ => false,
                    },
                    true,
                ),
                Channel::Fan => ((self.cfg.fan && has) || self.assist_channel, true),
            };
            let event_driven =
                self.plan.has_events_for(ch) && !(ch == Channel::Fan && self.assist_channel);
            let rt = self.channels.entry(ch).or_default();
            rt.driven = driven;
            rt.send = send;
            rt.event_driven = event_driven;
        }
    }

    /// The heater channel carries feedforward for the device PID rather than
    /// a heater output.
    fn heater_is_feedforward(&self) -> bool {
        self.cfg.mode == ReplayMode::FeedforwardPid && self.cfg.loop_at == LoopAt::Device
    }
}
