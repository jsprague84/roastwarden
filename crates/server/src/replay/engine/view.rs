//! What a run shows: the clock, each channel, the events with their status and ETA, the align,
//! the plan ahead for the prediction; and the log entries a run writes.

use serde::{Deserialize, Serialize};

use super::config::{AlignOn, Channel, LoopAt};
use super::pid::PidGains;
use super::plan::{interpolate, PlanEvent};
use super::tick::TickOutput;
use super::{Engine, EventRt, RECENT_LEN, UPCOMING_LEN};
use crate::control::fan_assist::AssistView;
use crate::profiles::types::{ProfileAction, TriggerKind};
use crate::replay::auto_drop::AutoDropView;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    Start,
    Stop,
    Engage,
    Ghost,
    Anchor,
    Align,
    Fired,
    Held,
    Cue,
    Superseded,
    Skipped,
    Delayed,
    Paused,
    Resumed,
    Config,
    Assist,
    AutoDrop,
}

impl LogKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LogKind::Start => "start",
            LogKind::Stop => "stop",
            LogKind::Engage => "engage",
            LogKind::Ghost => "ghost",
            LogKind::Anchor => "anchor",
            LogKind::Align => "align",
            LogKind::Fired => "fired",
            LogKind::Held => "held",
            LogKind::Cue => "cue",
            LogKind::Superseded => "superseded",
            LogKind::Skipped => "skipped",
            LogKind::Delayed => "delayed",
            LogKind::Paused => "paused",
            LogKind::Resumed => "resumed",
            LogKind::Config => "config",
            LogKind::Assist => "assist",
            LogKind::AutoDrop => "auto_drop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct LogEntry {
    pub kind: LogKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ProfileAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub elapsed: f64,
    pub live_t: f64,
    pub profile_t: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EventStatus {
    Pending,
    /// Trigger met, waiting out a delay until `fire_at` (session elapsed).
    Waiting {
        fire_at: f64,
    },
    Fired,
    /// Fired while its channel was paused: the profile value moved on but
    /// nothing was sent.
    Held,
    Superseded,
    Skipped {
        reason: String,
    },
}

impl EventStatus {
    pub(super) fn is_open(&self) -> bool {
        matches!(self, EventStatus::Pending | EventStatus::Waiting { .. })
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, utoipa::ToSchema)]
pub struct Clock {
    pub elapsed: f64,
    pub live_t: f64,
    pub profile_t: f64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ChannelView {
    pub channel: Channel,
    pub driven: bool,
    /// `events` or `curve` when driven.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    pub paused: bool,
    pub target: Option<f64>,
    pub last_sent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct EventView {
    #[serde(flatten)]
    pub event: PlanEvent,
    /// The trigger this run uses for it.
    pub trigger: TriggerKind,
    #[serde(flatten)]
    pub status: EventStatus,
    pub delay_seconds: f64,
    /// Seconds until it fires, when that can be estimated.
    pub eta_seconds: Option<f64>,
    /// °C still to go for a BT trigger.
    pub to_go_c: Option<f64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AlignView {
    pub on: AlignOn,
    /// The live landmark is marked and the profile has its time.
    pub applied: bool,
    /// Live time minus profile time at the landmark.
    pub offset_seconds: f64,
    /// Live BT minus profile BT at the landmark (shown, not applied).
    pub offset_c: Option<f64>,
    pub profile_landmark_t: Option<f64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct EngineView {
    #[serde(flatten)]
    pub clock: Clock,
    pub charge_marked: bool,
    pub past_turning_point: bool,
    pub align: AlignView,
    /// Profile BT at the current profile time (no lookahead).
    pub target_now: Option<f64>,
    /// The lookahead the setpoint uses now (s): the configured one, or less
    /// when it adapts.
    pub lookahead_now: f64,
    /// Where the BT loop runs.
    #[serde(rename = "loop")]
    pub loop_at: LoopAt,
    /// Server PID output this tick, heater % (server loop only).
    pub pid_output: Option<f64>,
    /// Server PID integral term, heater %.
    pub pid_integral: Option<f64>,
    /// Fan assist, while it runs.
    pub fan_assist: Option<AssistView>,
    pub channels: Vec<ChannelView>,
    pub upcoming: Vec<EventView>,
    pub recent: Vec<LogEntry>,
    /// Auto DROP, when the drive has one.
    pub auto_drop: Option<AutoDropView>,
}

/// The drive's next seconds, one value a second from now (`Engine::plan_ahead`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanAhead {
    /// The setpoints it will send; None when the drive runs no BT loop
    pub setpoints: Option<Vec<f64>>,
    /// The profile's BT now and ahead, without the lookahead
    pub targets: Vec<f64>,
    /// The heater curve, when it runs open loop
    pub heater: Option<Vec<f64>>,
    /// The fan, when the profile drives it
    pub fan: Option<Vec<f64>>,
    pub lookahead_s: f64,
    pub loop_at: LoopAt,
    pub server_gains: PidGains,
    /// Profile time now and live minus profile time, s
    pub profile_t: f64,
    pub offset_s: f64,
}

impl Engine {
    pub fn events_view(&self) -> Vec<EventView> {
        self.plan
            .events
            .iter()
            .zip(&self.events)
            .map(|(e, rt)| self.event_view(e, rt))
            .collect()
    }

    fn event_view(&self, e: &PlanEvent, rt: &EventRt) -> EventView {
        let trigger = self.effective_kind(e);
        let (mut eta, mut to_go) = (None, None);
        match rt.status {
            EventStatus::Waiting { fire_at } => eta = Some((fire_at - self.clock.elapsed).max(0.0)),
            EventStatus::Pending => match trigger {
                TriggerKind::Time => {
                    eta = e
                        .at_seconds
                        .map(|a| (a - self.clock.profile_t).max(0.0) + rt.delay_s);
                }
                TriggerKind::Bt => {
                    if let (Some(a), Some(b)) = (e.at_bt, self.last_bt) {
                        let go = (a - b).max(0.0);
                        to_go = Some(go);
                        eta = self
                            .last_ror
                            .filter(|r| *r > 0.5)
                            .map(|r| go / r * 60.0 + rt.delay_s);
                    }
                    if eta.is_none() {
                        // Fall back to where the profile expects it.
                        eta = e
                            .expected_t
                            .map(|t| (t - self.clock.profile_t).max(0.0) + rt.delay_s);
                    }
                }
            },
            _ => {}
        }
        EventView {
            event: e.clone(),
            trigger,
            status: rt.status.clone(),
            delay_seconds: rt.delay_s,
            eta_seconds: eta,
            to_go_c: to_go,
        }
    }

    /// The next `n` seconds of the drive as the prediction reads them (docs/prediction.md):
    /// the setpoint it sends each second (with the lookahead in use now), the profile's BT
    /// without it, the heater curve when it runs open loop, and the fan when the profile
    /// drives it (fan assist's fan is not planned: it holds).
    pub fn plan_ahead(&self, n: usize) -> PlanAhead {
        let t0 = self.clock.profile_t;
        let series = |ch: Channel, lead: f64| -> Option<Vec<f64>> {
            let c = self.channels.get(&ch)?;
            if !c.driven {
                return None;
            }
            if c.paused {
                return c.held.or(c.target).map(|v| vec![v]);
            }
            (0..=n)
                .map(|k| self.base_value(ch, t0 + lead + k as f64))
                .collect()
        };
        let setpoints = series(Channel::Setpoint, self.lead);
        let heater = if setpoints.is_none() {
            series(Channel::Heater, 0.0)
        } else {
            None
        };
        PlanAhead {
            targets: (0..=n)
                .filter_map(|k| interpolate(&self.plan.bt, t0 + k as f64))
                .collect(),
            setpoints,
            heater,
            fan: if self.assist_channel {
                None
            } else {
                series(Channel::Fan, 0.0)
            },
            lookahead_s: self.lead,
            loop_at: self.cfg.loop_at,
            server_gains: self.cfg.server_pid,
            profile_t: t0,
            offset_s: self.align.offset_seconds,
        }
    }

    pub fn view(&self) -> EngineView {
        let mut upcoming: Vec<EventView> = self
            .plan
            .events
            .iter()
            .zip(&self.events)
            .filter(|(_, rt)| rt.status.is_open())
            .map(|(e, rt)| self.event_view(e, rt))
            .collect();
        upcoming.sort_by(|a, b| {
            a.eta_seconds
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b.eta_seconds.unwrap_or(f64::INFINITY))
        });
        upcoming.truncate(UPCOMING_LEN);
        EngineView {
            clock: self.clock,
            charge_marked: self.charge_marked,
            past_turning_point: self.past_tp,
            align: self.align.clone(),
            target_now: interpolate(&self.plan.bt, self.clock.profile_t),
            lookahead_now: (self.lead * 10.0).round() / 10.0,
            loop_at: self.cfg.loop_at,
            pid_output: self.pid_out,
            pid_integral: self.pid.started().then_some(self.pid.integ),
            fan_assist: self.assist_view,
            channels: self
                .channels
                .iter()
                .map(|(ch, c)| ChannelView {
                    channel: *ch,
                    driven: c.driven,
                    source: c
                        .driven
                        .then_some(if *ch == Channel::Fan && self.assist_channel {
                            "assist"
                        } else if c.event_driven {
                            "events"
                        } else {
                            "curve"
                        }),
                    paused: c.paused,
                    target: c.target,
                    last_sent: c.last_sent,
                })
                .collect(),
            upcoming,
            recent: self.recent.iter().cloned().collect(),
            auto_drop: self
                .cfg
                .auto_drop
                .as_ref()
                .map(|a| self.drop_rt.view(a, self.cfg.ghost)),
        }
    }

    pub fn last_sent(&self, ch: Channel) -> Option<f64> {
        self.channels.get(&ch).and_then(|c| c.last_sent)
    }

    pub fn entry(
        &mut self,
        kind: LogKind,
        channel: Option<Channel>,
        value: Option<f64>,
        detail: Option<String>,
    ) -> LogEntry {
        let e = LogEntry {
            kind,
            channel,
            event_id: None,
            action: None,
            value,
            label: None,
            detail,
            elapsed: self.clock.elapsed,
            live_t: self.clock.live_t,
            profile_t: self.clock.profile_t,
        };
        self.remember(e.clone());
        e
    }

    pub(super) fn event_entry(
        &mut self,
        kind: LogKind,
        ev: &PlanEvent,
        detail: Option<String>,
    ) -> LogEntry {
        let e = LogEntry {
            kind,
            channel: ev.channel(),
            event_id: Some(ev.id.clone()),
            action: Some(ev.action),
            value: ev.value,
            label: ev.label.clone(),
            detail,
            elapsed: self.clock.elapsed,
            live_t: self.clock.live_t,
            profile_t: self.clock.profile_t,
        };
        self.remember(e.clone());
        e
    }

    fn remember(&mut self, e: LogEntry) {
        self.recent.push_front(e);
        self.recent.truncate(RECENT_LEN);
    }

    pub(super) fn log(
        &mut self,
        out: &mut TickOutput,
        kind: LogKind,
        channel: Option<Channel>,
        value: Option<f64>,
        detail: Option<String>,
    ) {
        let e = self.entry(kind, channel, value, detail);
        out.log.push(e);
    }

    pub(super) fn log_event(
        &mut self,
        out: &mut TickOutput,
        kind: LogKind,
        ev: &PlanEvent,
        detail: Option<String>,
    ) {
        let e = self.event_entry(kind, ev, detail);
        out.log.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_kind_names_itself_as_the_wire_does() {
        for k in [
            LogKind::Start,
            LogKind::Stop,
            LogKind::Engage,
            LogKind::Ghost,
            LogKind::Anchor,
            LogKind::Align,
            LogKind::Fired,
            LogKind::Held,
            LogKind::Cue,
            LogKind::Superseded,
            LogKind::Skipped,
            LogKind::Delayed,
            LogKind::Paused,
            LogKind::Resumed,
            LogKind::Config,
            LogKind::Assist,
            LogKind::AutoDrop,
        ] {
            assert_eq!(
                serde_json::to_value(k).unwrap(),
                serde_json::Value::String(k.as_str().into())
            );
        }
    }
}
