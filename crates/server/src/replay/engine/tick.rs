//! One tick a second: the time base from CHARGE, the alignment on a landmark, the lookahead,
//! each channel's target, auto DROP's step, and the commands that go out.

use std::collections::BTreeMap;

use roastwarden_core::DeviceCommand;

use super::config::{AlignOn, Channel, LoopAt, ReplayMode};
use super::plan::interpolate;
use super::view::{AlignView, Clock, LogEntry, LogKind};
use super::{
    Engine, TpTracker, ADAPT_FROM_SECS, ADAPT_MAX_AHEAD_SECS, ADAPT_MIN_SLOPE, ADAPT_TAU_SECS,
    REFRESH_SECS,
};
use crate::replay::auto_drop::DropInput;

/// A landmark marked in the live session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    /// Seconds since the session start.
    pub elapsed: f64,
    pub bt: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct TickInput {
    /// Seconds since the session start.
    pub elapsed: f64,
    /// Latest BT, `None` when telemetry is stale.
    pub bt: Option<f64>,
    /// Latest RoR (°C/min), `None` when stale.
    pub ror: Option<f64>,
    /// Heater % the device reports, for a bumpless server-PID start (and,
    /// with the loop on the device, the output fan assist works from).
    pub heater: Option<f64>,
    /// Fan % now (commanded, else reported): the assist's ceiling when the
    /// profile does not drive the fan.
    pub fan: Option<f64>,
    /// The device reports its PID in auto.
    pub auto: Option<bool>,
    /// Landmarks marked so far.
    pub marks: BTreeMap<AlignOn, Mark>,
}

#[derive(Debug, Default)]
pub struct TickOutput {
    /// Commands to send, with the channel they belong to (none for mode
    /// switches fired by events).
    pub commands: Vec<(Option<Channel>, DeviceCommand)>,
    pub log: Vec<LogEntry>,
}

impl Engine {
    /// Advance one tick.
    pub fn tick(&mut self, inp: &TickInput) -> TickOutput {
        let mut out = TickOutput::default();
        let charge_mark = inp.marks.get(&AlignOn::Charge);
        let charge = charge_mark.map_or(0.0, |m| m.elapsed);
        self.charge_marked = charge_mark.is_some();
        let live_t = inp.elapsed - charge;
        self.clock.elapsed = inp.elapsed;
        self.clock.live_t = live_t;

        // CHARGE marked (or moved) after the run started: the time base
        // re-anchors and whatever fired before the new CHARGE fires again.
        if let Some(prev) = self.charge {
            if (prev - charge).abs() > 0.5 {
                self.tp = TpTracker::default();
                self.drop_rt.reset();
                let rearmed = self.rearm_before(charge);
                self.clock.profile_t = live_t - self.align.offset_seconds;
                self.log(
                    &mut out,
                    LogKind::Anchor,
                    None,
                    None,
                    Some(format!(
                        "time base re-anchored on CHARGE at {charge:.0} s; {rearmed} event(s) re-armed"
                    )),
                );
            }
        }
        self.charge = Some(charge);

        self.tp.update(live_t, inp.bt);
        self.past_tp = inp.marks.contains_key(&AlignOn::TurningPoint) || self.tp.turned;

        let prev_offset = self.align.offset_seconds;
        self.align = self.compute_align(&inp.marks, charge);
        let profile_t = live_t - self.align.offset_seconds;
        self.clock.profile_t = profile_t;
        if self.started && (self.align.offset_seconds - prev_offset).abs() > 0.5 {
            self.log(
                &mut out,
                LogKind::Align,
                None,
                None,
                Some(format!(
                    "aligned on {}: profile offset {:+.0} s",
                    self.align.on.as_str(),
                    self.align.offset_seconds
                )),
            );
        }

        let dt = if self.started {
            (inp.elapsed - self.last_tick_elapsed()).clamp(0.0, 5.0)
        } else {
            0.0
        };
        self.last_bt = inp.bt;
        self.last_ror = inp.ror;

        self.run_events(inp, &mut out);
        self.step_auto_drop(inp, charge, &mut out);
        if let Some(note) = self.adapt_lookahead(inp.bt, dt) {
            self.log(
                &mut out,
                LogKind::Config,
                Some(Channel::Setpoint),
                None,
                Some(note),
            );
        }
        self.update_targets(inp.bt, inp.heater, dt);
        if let Some(note) = self.update_fan(inp, dt) {
            self.log(
                &mut out,
                LogKind::Assist,
                Some(Channel::Fan),
                None,
                Some(note),
            );
        }
        self.emit_sends(inp.elapsed, &mut out);
        self.started = true;
        self.prev_elapsed = Some(inp.elapsed);
        out
    }

    fn step_auto_drop(&mut self, inp: &TickInput, charge: f64, out: &mut TickOutput) {
        let Some(cfg) = self.cfg.auto_drop.clone() else {
            return;
        };
        let note = self.drop_rt.step(
            &cfg,
            &DropInput {
                elapsed: inp.elapsed,
                live_t: self.clock.live_t,
                profile_t: self.clock.profile_t,
                charge_marked: self.charge_marked,
                fc_live_t: inp
                    .marks
                    .get(&AlignOn::FirstCrackStart)
                    .map(|m| m.elapsed - charge),
                bt: inp.bt,
                ror: inp.ror,
                ghost: self.cfg.ghost,
            },
        );
        if let Some(n) = note {
            self.log(out, LogKind::AutoDrop, None, None, Some(n));
        }
    }

    /// Cancel auto DROP for this roast (`on = false`) or put it back. An error when the
    /// drive has none.
    pub fn set_auto_drop(&mut self, on: bool) -> Result<Option<LogEntry>, String> {
        let Some(a) = self.cfg.auto_drop.as_mut() else {
            return Err("this drive has no auto DROP (the profile's is off)".into());
        };
        if a.cancelled != on {
            return Ok(None);
        }
        a.cancelled = !on;
        self.drop_rt.reset();
        Ok(Some(self.entry(
            LogKind::AutoDrop,
            None,
            None,
            Some(if on {
                "auto DROP back on".into()
            } else {
                "auto DROP cancelled for this roast".into()
            }),
        )))
    }

    fn last_tick_elapsed(&self) -> f64 {
        self.prev_elapsed.unwrap_or(self.clock.elapsed)
    }

    /// Profile time at `elapsed` (session seconds) with these landmarks; runs nothing.
    pub fn profile_time_at(&self, elapsed: f64, marks: &BTreeMap<AlignOn, Mark>) -> f64 {
        let charge = marks.get(&AlignOn::Charge).map_or(0.0, |m| m.elapsed);
        elapsed - charge - self.compute_align(marks, charge).offset_seconds
    }

    /// Put the clock at `elapsed` without running events or channels: the view of a run
    /// rebuilt after a restart, stopped where it was.
    pub fn place(&mut self, elapsed: f64, marks: &BTreeMap<AlignOn, Mark>) {
        let charge_mark = marks.get(&AlignOn::Charge);
        let charge = charge_mark.map_or(0.0, |m| m.elapsed);
        self.charge_marked = charge_mark.is_some();
        self.charge = Some(charge);
        self.past_tp = marks.contains_key(&AlignOn::TurningPoint);
        self.align = self.compute_align(marks, charge);
        self.clock = Clock {
            elapsed,
            live_t: elapsed - charge,
            profile_t: elapsed - charge - self.align.offset_seconds,
        };
    }

    fn compute_align(&self, marks: &BTreeMap<AlignOn, Mark>, charge: f64) -> AlignView {
        let mut v = self.landmark_align(marks, charge);
        // A resumed run waits out the lost time, unless its landmark came after the resume
        // (that landmark then places the profile by itself).
        if let Some(h) = self.cfg.hold {
            let after = v.on != AlignOn::Charge
                && marks
                    .get(&v.on)
                    .is_some_and(|m| v.applied && m.elapsed >= h.since_elapsed);
            if !after {
                v.offset_seconds += h.seconds;
            }
        }
        v
    }

    fn landmark_align(&self, marks: &BTreeMap<AlignOn, Mark>, charge: f64) -> AlignView {
        let on = self.cfg.align;
        let profile_lm = self.plan.landmarks.get(&on).copied();
        let live = marks.get(&on);
        let bt_at = |t: f64| interpolate(&self.plan.bt_ref, t);
        match (live, profile_lm) {
            (Some(m), Some(pt)) => AlignView {
                on,
                applied: true,
                offset_seconds: if on == AlignOn::Charge {
                    0.0
                } else {
                    (m.elapsed - charge) - pt
                },
                offset_c: m.bt.zip(bt_at(pt)).map(|(a, b)| a - b),
                profile_landmark_t: Some(pt),
            },
            _ => AlignView {
                on,
                applied: on == AlignOn::Charge,
                offset_seconds: 0.0,
                offset_c: None,
                profile_landmark_t: profile_lm,
            },
        }
    }

    /// Adaptive lookahead: the setpoint leads the profile to make up for the
    /// loop's lag, but the lag shrinks as the roast slows (on the real roaster
    /// about 17 s while drying, 10 s to first crack, none after it), so a fixed
    /// 20 s lead put BT 20 s ahead of the profile from first crack to drop and
    /// first crack 20 s early. Each tick the lead moves by how many seconds BT
    /// is ahead of the profile itself, over `ADAPT_TAU_SECS`, between 0 and
    /// the configured lookahead. Returns a log note at each 5 s step.
    pub(super) fn adapt_lookahead(&mut self, bt: Option<f64>, dt: f64) -> Option<String> {
        let max = self.cfg.lookahead_seconds.max(0.0);
        if !self.cfg.adapt_lookahead {
            self.lead = max;
            return None;
        }
        let sp = self.channels.get(&Channel::Setpoint)?;
        let adapting = self.cfg.mode.closed_loop()
            && !self.cfg.ghost
            && sp.driven
            && !sp.paused
            && !sp.event_driven
            && self.charge_marked
            && self.clock.live_t >= ADAPT_FROM_SECS
            && dt > 0.0;
        let bt = bt.filter(|_| adapting)?;
        let t = self.clock.profile_t;
        let now = interpolate(&self.plan.bt, t)?;
        let slope =
            (interpolate(&self.plan.bt, t + 5.0)? - interpolate(&self.plan.bt, t - 5.0)?) / 10.0;
        if slope < ADAPT_MIN_SLOPE {
            return None;
        }
        let ahead = ((bt - now) / slope).clamp(-ADAPT_MAX_AHEAD_SECS, ADAPT_MAX_AHEAD_SECS);
        self.lead = (self.lead - ahead * dt / ADAPT_TAU_SECS).clamp(0.0, max);
        if (self.lead - self.lead_logged).abs() >= 5.0 {
            self.lead_logged = (self.lead / 5.0).round() * 5.0;
            return Some(format!(
                "lookahead {:.0} s (BT {:.0} s {} the profile)",
                self.lead,
                ahead.abs(),
                if ahead >= 0.0 { "ahead of" } else { "behind" }
            ));
        }
        None
    }

    fn update_targets(&mut self, bt: Option<f64>, heater_now: Option<f64>, dt: f64) {
        let profile_t = self.clock.profile_t;
        let cfg = self.cfg.clone();

        // Setpoint: the profile curve (with lookahead), or the operator's
        // value while the channel is paused.
        let sp_curve = self.base_value(Channel::Setpoint, profile_t + self.lead);
        if let Some(c) = self.channels.get_mut(&Channel::Setpoint) {
            if c.paused {
                if c.held.is_some() {
                    c.target = c.held;
                }
            } else {
                c.target = sp_curve.map(|t| (t * 10.0).round() / 10.0);
            }
        }
        let sp = self
            .channels
            .get(&Channel::Setpoint)
            .filter(|c| c.driven)
            .and_then(|c| c.target);

        // Feedforward from the profile (heater curve or heater events).
        let ff = if cfg.mode == ReplayMode::Setpoint {
            None
        } else {
            self.base_value(Channel::Heater, profile_t)
        };

        let heater_paused = self
            .channels
            .get(&Channel::Heater)
            .is_some_and(|c| c.paused);
        self.pid_out = None;
        let heater = match (cfg.mode, cfg.loop_at) {
            (ReplayMode::Feedforward, _) | (ReplayMode::FeedforwardPid, LoopAt::Device) => {
                ff.map(|f| f.clamp(0.0, 100.0).round())
            }
            (_, LoopAt::Server) if !heater_paused => match sp {
                Some(sp) => {
                    let f = ff.unwrap_or(0.0);
                    if !self.pid.started() {
                        if let Some(b) = bt {
                            self.pid.reset(sp, b, heater_now.unwrap_or(f), f);
                        }
                    }
                    let out = self.pid.compute(sp, bt, dt, f);
                    self.pid_out = Some(out);
                    Some(out.round())
                }
                None => None,
            },
            _ => None,
        };
        if let Some(c) = self.channels.get_mut(&Channel::Heater) {
            if !c.paused {
                c.target = heater;
            }
        }
    }

    /// A channel's profile value: the latest fired event for an
    /// event-driven channel (the curve's CHARGE value until the first event
    /// fires), else the curve at `t`.
    pub(super) fn base_value(&self, ch: Channel, t: f64) -> Option<f64> {
        let c = self.channels.get(&ch)?;
        if !c.driven {
            return None;
        }
        let curve = match ch {
            Channel::Setpoint => &self.plan.bt,
            Channel::Heater => &self.plan.heater,
            Channel::Fan => &self.plan.fan,
        };
        if c.event_driven {
            c.event_value.or_else(|| interpolate(curve, 0.0))
        } else {
            interpolate(curve, t)
        }
    }

    fn emit_sends(&mut self, elapsed: f64, out: &mut TickOutput) {
        let hysteresis = self.cfg.hysteresis_c.max(0.0);
        let heater_ff = self.heater_is_feedforward();
        let floor = self.cfg.fan_floor_pct.clamp(0.0, 100.0);
        let mut fan_note = None;
        for (ch, c) in &mut self.channels {
            if !c.driven || c.paused || !c.send {
                continue;
            }
            let Some(v) = c.target else { continue };
            let changed = match c.last_sent {
                None => true,
                Some(p) => match ch {
                    Channel::Setpoint if c.event_driven => (v - p).abs() >= 0.05,
                    Channel::Setpoint => (v - p).abs() >= hysteresis,
                    // Heater and fan go out in whole percent.
                    Channel::Heater | Channel::Fan => v.round() != p.round(),
                },
            };
            let stale = c
                .last_sent_elapsed
                .is_none_or(|t| elapsed - t >= REFRESH_SECS);
            if !(changed || stale) {
                continue;
            }
            let cmd = match ch {
                Channel::Setpoint => DeviceCommand::Setpoint { value: v },
                Channel::Heater if heater_ff => DeviceCommand::Feedforward {
                    value: Some(v.clamp(0.0, 100.0)),
                },
                Channel::Heater => DeviceCommand::Heater {
                    percent: v.round().clamp(0.0, 100.0),
                },
                Channel::Fan => DeviceCommand::Fan {
                    percent: v.round().clamp(floor, 100.0),
                },
            };
            if *ch == Channel::Fan && v.round() < floor && !self.fan_raised {
                self.fan_raised = true;
                fan_note = Some(v);
            }
            c.last_sent = Some(v);
            c.last_sent_elapsed = Some(elapsed);
            out.commands.push((Some(*ch), cmd));
        }
        if let Some(v) = fan_note {
            self.log(
                out,
                LogKind::Assist,
                Some(Channel::Fan),
                Some(floor),
                Some(format!(
                    "profile fan {v:.0} % raised to {floor:.0} %: below it the roaster's interlock gates the heater"
                )),
            );
        }
    }

    /// The service could not deliver a command: retry it next tick.
    pub fn send_failed(&mut self, ch: Channel) {
        if let Some(c) = self.channels.get_mut(&ch) {
            c.last_sent = None;
            c.last_sent_elapsed = None;
        }
    }

    /// Forget what was sent so every driven channel is sent on the next tick
    /// (used when a ghost run is engaged).
    pub fn resend_all(&mut self) {
        for c in self.channels.values_mut() {
            c.last_sent = None;
            c.last_sent_elapsed = None;
        }
    }
}
