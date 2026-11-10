//! The profile's events: when each fires (its own trigger or the run's policy, the catch-up
//! when a run starts mid-roast), re-arming after CHARGE moves, and the operator's skip, delay
//! and fire.

use std::collections::BTreeMap;

use roastwarden_core::{ControlMode, DeviceCommand};

use super::config::{Channel, TriggerPolicy};
use super::plan::PlanEvent;
use super::tick::{TickInput, TickOutput};
use super::view::{EventStatus, LogEntry, LogKind};
use super::{CUE_GRACE_SECS, Engine};
use crate::profiles::types::{ProfileAction, TriggerKind};

impl Engine {
    pub(super) fn effective_kind(&self, ev: &PlanEvent) -> TriggerKind {
        let want = match self.cfg.trigger {
            TriggerPolicy::Profile => ev.trigger_kind,
            TriggerPolicy::Time => TriggerKind::Time,
            TriggerPolicy::Bt => TriggerKind::Bt,
        };
        match want {
            TriggerKind::Time if ev.at_seconds.is_none() => TriggerKind::Bt,
            TriggerKind::Bt if ev.at_bt.is_none() => TriggerKind::Time,
            k => k,
        }
    }

    fn trigger_met(&self, ev: &PlanEvent, bt: Option<f64>) -> bool {
        match self.effective_kind(ev) {
            TriggerKind::Time => ev.at_seconds.is_some_and(|a| self.clock.profile_t >= a),
            TriggerKind::Bt => {
                self.past_tp
                    && self.clock.live_t >= 0.0
                    && matches!((bt, ev.at_bt), (Some(b), Some(a)) if b >= a)
            }
        }
    }

    pub(super) fn run_events(&mut self, inp: &TickInput, out: &mut TickOutput) {
        let catchup = !self.started;
        let mut ready: Vec<usize> = Vec::new();
        for i in 0..self.events.len() {
            let met = self.events[i].status == EventStatus::Pending
                && (self.events[i].force || self.trigger_met(&self.plan.events[i], inp.bt));
            let rt = &mut self.events[i];
            match rt.status {
                EventStatus::Pending if met => {
                    if rt.delay_s > 0.0 && !rt.force {
                        rt.status = EventStatus::Waiting {
                            fire_at: inp.elapsed + rt.delay_s,
                        };
                    } else {
                        ready.push(i);
                    }
                    rt.force = false;
                }
                EventStatus::Waiting { fire_at } if inp.elapsed >= fire_at || rt.force => {
                    rt.force = false;
                    ready.push(i);
                }
                _ => {}
            }
        }
        if ready.is_empty() {
            return;
        }

        // Several value events for one channel in the same tick (a run that
        // starts mid-roast, a re-align jump): only the last one applies.
        let mut last_for: BTreeMap<(Channel, bool), usize> = BTreeMap::new();
        for &i in &ready {
            let ev = &self.plan.events[i];
            if let Some(ch) = ev.channel() {
                last_for.insert((ch, ev.action == ProfileAction::Mode), i);
            }
        }

        for i in ready {
            let ev = self.plan.events[i].clone();
            self.seq += 1;
            self.events[i].seq = self.seq;
            self.events[i].done_elapsed = Some(inp.elapsed);
            let Some(ch) = ev.channel() else {
                // Cue.
                let past = match self.effective_kind(&ev) {
                    TriggerKind::Time => ev
                        .at_seconds
                        .is_some_and(|a| self.clock.profile_t - a > CUE_GRACE_SECS),
                    TriggerKind::Bt => true,
                };
                if catchup && past && !self.events[i].force {
                    self.skip_event(i, "already past when the replay started", out);
                } else {
                    self.events[i].status = EventStatus::Fired;
                    self.log_event(out, LogKind::Cue, &ev, None);
                }
                continue;
            };
            if last_for.get(&(ch, ev.action == ProfileAction::Mode)) != Some(&i) {
                self.events[i].status = EventStatus::Superseded;
                self.log_event(
                    out,
                    LogKind::Superseded,
                    &ev,
                    Some("a later event on the same channel applies".into()),
                );
                continue;
            }
            let rt = self.channels.get(&ch).cloned().unwrap_or_default();
            if !rt.driven {
                let why = if ch == Channel::Fan && !self.cfg.fan {
                    "fan replay is off".to_string()
                } else {
                    format!(
                        "{} is not driven in {} mode",
                        ch.as_str(),
                        self.cfg.mode.as_str()
                    )
                };
                self.skip_event(i, &why, out);
                continue;
            }
            if ev.action == ProfileAction::Mode {
                let mode = ev
                    .label
                    .as_deref()
                    .and_then(|l| l.parse::<ControlMode>().ok());
                match mode {
                    Some(_) if rt.paused => self.skip_event(i, "heater is paused", out),
                    Some(m) => {
                        self.events[i].status = EventStatus::Fired;
                        out.commands.push((None, DeviceCommand::Mode { mode: m }));
                        self.log_event(out, LogKind::Fired, &ev, None);
                    }
                    None => self.skip_event(i, "mode event without auto/manual label", out),
                }
                continue;
            }
            let Some(v) = ev.value.filter(|v| v.is_finite()) else {
                self.skip_event(i, "event has no value", out);
                continue;
            };
            let v = match ch {
                Channel::Setpoint => v.clamp(0.0, 300.0),
                Channel::Heater | Channel::Fan => v.clamp(0.0, 100.0),
            };
            if let Some(c) = self.channels.get_mut(&ch) {
                c.event_value = Some(v);
            }
            if rt.paused {
                self.events[i].status = EventStatus::Held;
                self.log_event(
                    out,
                    LogKind::Held,
                    &ev,
                    Some(format!("{} is paused", ch.as_str())),
                );
            } else {
                self.events[i].status = EventStatus::Fired;
                self.log_event(out, LogKind::Fired, &ev, None);
            }
        }
    }

    fn skip_event(&mut self, i: usize, reason: &str, out: &mut TickOutput) {
        self.events[i].status = EventStatus::Skipped {
            reason: reason.to_string(),
        };
        let ev = self.plan.events[i].clone();
        self.log_event(out, LogKind::Skipped, &ev, Some(reason.to_string()));
    }

    /// Re-arm events that were done before `charge` (session elapsed) and
    /// recompute the event-driven channel values. Returns how many re-armed.
    pub(super) fn rearm_before(&mut self, charge: f64) -> usize {
        let mut n = 0;
        for rt in &mut self.events {
            let before = rt.done_elapsed.is_some_and(|d| d < charge - 0.5);
            let rearm = match rt.status {
                EventStatus::Waiting { .. } => true,
                EventStatus::Fired | EventStatus::Held | EventStatus::Superseded => before,
                EventStatus::Skipped { ref reason } => before && reason.starts_with("already past"),
                EventStatus::Pending => false,
            };
            if rearm {
                rt.status = EventStatus::Pending;
                rt.done_elapsed = None;
                n += 1;
            }
        }
        for ch in Channel::ALL {
            let latest = self
                .plan
                .events
                .iter()
                .zip(&self.events)
                .filter(|(e, rt)| {
                    e.channel() == Some(ch)
                        && e.action != ProfileAction::Mode
                        && matches!(rt.status, EventStatus::Fired | EventStatus::Held)
                })
                .max_by_key(|(_, rt)| rt.seq)
                .and_then(|(e, _)| e.value);
            if let Some(c) = self.channels.get_mut(&ch) {
                c.event_value = latest;
            }
        }
        n
    }

    fn index_of(&self, event_id: &str) -> Result<usize, String> {
        let i = self
            .plan
            .events
            .iter()
            .position(|e| e.id == event_id)
            .ok_or_else(|| "no such event in this replay".to_string())?;
        if !self.events[i].status.is_open() {
            return Err("event has already fired or been skipped".into());
        }
        Ok(i)
    }

    pub fn skip(&mut self, event_id: &str) -> Result<LogEntry, String> {
        let i = self.index_of(event_id)?;
        self.events[i].status = EventStatus::Skipped {
            reason: "skipped by the operator".into(),
        };
        let ev = self.plan.events[i].clone();
        Ok(self.event_entry(
            LogKind::Skipped,
            &ev,
            Some("skipped by the operator".into()),
        ))
    }

    pub fn delay(&mut self, event_id: &str, seconds: f64) -> Result<LogEntry, String> {
        if !seconds.is_finite() || seconds <= 0.0 || seconds > 600.0 {
            return Err("delay must be between 0 and 600 seconds".into());
        }
        let i = self.index_of(event_id)?;
        let rt = &mut self.events[i];
        rt.delay_s += seconds;
        if let EventStatus::Waiting { fire_at } = rt.status {
            rt.status = EventStatus::Waiting {
                fire_at: fire_at + seconds,
            };
        }
        let total = rt.delay_s;
        let ev = self.plan.events[i].clone();
        Ok(self.event_entry(
            LogKind::Delayed,
            &ev,
            Some(format!("delayed {seconds:.0} s (total {total:.0} s)")),
        ))
    }

    /// Fire on the next tick regardless of the trigger.
    pub fn fire(&mut self, event_id: &str) -> Result<(), String> {
        let i = self.index_of(event_id)?;
        self.events[i].force = true;
        Ok(())
    }
}
