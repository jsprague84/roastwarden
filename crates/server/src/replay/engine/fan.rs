//! Fan assist inside the drive: the profile's fan (or the operator's, when the profile does not
//! drive it) is the ceiling, trimmed while the heater is pinned (`docs/fan-assist.md`).

use super::Engine;
use super::config::{Channel, LoopAt, RunConfig};
use super::plan::interpolate;
use super::tick::TickInput;
use super::view::{LogEntry, LogKind};
use crate::control::fan_assist::{AssistStatus, FanAssistConfig};

impl Engine {
    pub(super) fn assist_wanted(cfg: &RunConfig) -> bool {
        cfg.mode.closed_loop() && cfg.fan_assist.as_ref().is_some_and(|a| a.enabled)
    }

    /// The fan the profile (or, for an assist-only fan channel, the
    /// operator) asks for.
    fn fan_ceiling_now(&mut self, inp_fan: Option<f64>) -> Option<f64> {
        if self.assist_channel {
            if self.fan_ceiling.is_none() {
                self.fan_ceiling = inp_fan.map(|f| f.clamp(0.0, 100.0));
            }
            self.fan_ceiling
        } else {
            self.base_value(Channel::Fan, self.clock.profile_t)
        }
    }

    /// Fan target: the ceiling, trimmed by the assist while it runs. Returns
    /// a note for the log when the assist starts, reaches the floor or lets go.
    pub(super) fn update_fan(&mut self, inp: &TickInput, dt: f64) -> Option<String> {
        let ceiling = self.fan_ceiling_now(inp.fan);
        let paused = self.channels.get(&Channel::Fan).is_none_or(|c| c.paused);
        let heater_paused = self
            .channels
            .get(&Channel::Heater)
            .is_some_and(|c| c.paused);
        // The heater output of the BT loop, wherever it runs.
        let heater = match self.cfg.loop_at {
            LoopAt::Server if !heater_paused => self.pid_out,
            LoopAt::Device if inp.auto != Some(false) => inp.heater,
            _ => None,
        };
        // Without a heater output (stale telemetry, roaster in manual) the
        // assist holds its trim.
        let run = Self::assist_wanted(&self.cfg) && !paused;
        let before = (self.assist.trim(), self.assist.status());
        let target = match (ceiling, self.cfg.fan_assist.clone()) {
            (Some(c), Some(cfg)) if run => {
                // The setpoint the BT loop is chasing (with the lookahead, or
                // the operator's while paused): against the profile *now*, BT
                // runs a lookahead's worth ahead by design and the assist
                // would hold while the heater is pinned (as a virtual
                // roast showed).
                let target = match self.channels.get(&Channel::Setpoint) {
                    Some(sp) if sp.paused => sp.held,
                    Some(sp) => sp.target,
                    None => None,
                }
                .or_else(|| interpolate(&self.plan.bt, self.clock.profile_t));
                let f = self.assist.step(&cfg, c, heater, inp.bt, target, dt);
                self.assist_view = Some(self.assist.view());
                Some(f)
            }
            _ => {
                self.assist.reset();
                self.assist_view = None;
                ceiling
            }
        };
        if let Some(c) = self.channels.get_mut(&Channel::Fan) {
            c.target = target;
        }
        let after = (self.assist.trim(), self.assist.status());
        let view = self.assist_view?;
        if before.0 < 0.5 && after.0 >= 0.5 {
            Some(format!(
                "fan assist: heater at {:.0} %, trimming the fan below {:.0} %",
                view.heater.unwrap_or(0.0),
                view.ceiling
            ))
        } else if after.1 == AssistStatus::AtFloor && before.1 != AssistStatus::AtFloor {
            Some(format!(
                "fan assist: at the {:.0} % floor; the heater is still pinned",
                view.floor
            ))
        } else if before.0 >= 0.5 && after.0 < 0.5 {
            Some("fan assist: trim given back, fan at the profile's".into())
        } else {
            None
        }
    }

    /// The operator set the fan during a run whose fan channel exists only
    /// for the assist: that fan is the new ceiling. Returns false when the
    /// profile drives the fan (the caller pauses the channel instead).
    pub fn set_fan_ceiling(&mut self, pct: f64) -> bool {
        if !self.assist_channel {
            return false;
        }
        self.fan_ceiling = Some(pct.clamp(0.0, 100.0));
        self.assist.reset();
        if let Some(c) = self.channels.get_mut(&Channel::Fan) {
            c.target = self.fan_ceiling;
            c.last_sent = self.fan_ceiling;
        }
        true
    }

    /// New assist settings mid-run. Switching it on in a run without a fan
    /// channel adds one (ceiling: the fan now); switching it off gives the
    /// trim back on the next tick.
    pub fn set_fan_assist(&mut self, a: FanAssistConfig) -> Option<LogEntry> {
        if self.cfg.fan_assist.as_ref() == Some(&a) {
            return None;
        }
        let on = a.enabled;
        self.cfg.fan_assist = Some(a);
        if !on {
            self.assist.reset();
        }
        if Self::assist_wanted(&self.cfg)
            && !self.channels.get(&Channel::Fan).is_some_and(|c| c.driven)
        {
            self.assist_channel = true;
            self.configure_channels();
        }
        Some(self.entry(
            LogKind::Config,
            Some(Channel::Fan),
            None,
            Some(format!("fan assist {}", if on { "on" } else { "off" })),
        ))
    }
}
