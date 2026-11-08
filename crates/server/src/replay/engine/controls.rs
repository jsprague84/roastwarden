//! What the operator changes mid-run: a ghost run engaged (or a live one made ghost), the align
//! landmark, trigger policy and lookahead, a channel paused by a manual override and resumed.

use super::config::{AlignOn, Channel, TriggerPolicy};
use super::pid::ServerPid;
use super::view::{LogEntry, LogKind};
use super::Engine;

impl Engine {
    pub fn set_ghost(&mut self, ghost: bool) -> LogEntry {
        self.cfg.ghost = ghost;
        if !ghost {
            self.resend_all();
            // The server PID integrated against a roaster it was not driving:
            // start it again from the heater the roaster has now.
            self.pid = ServerPid::new(self.cfg.server_pid);
        }
        self.entry(
            if ghost {
                LogKind::Ghost
            } else {
                LogKind::Engage
            },
            None,
            None,
            Some(if ghost {
                "ghost mode: nothing is sent".into()
            } else {
                "engaged: replay drives the roaster".into()
            }),
        )
    }

    /// Change the align landmark, trigger policy or lookahead mid-run.
    pub fn reconfigure(
        &mut self,
        align: Option<AlignOn>,
        trigger: Option<TriggerPolicy>,
        lookahead: Option<f64>,
    ) -> Option<LogEntry> {
        let mut parts = Vec::new();
        if let Some(a) = align.filter(|a| *a != self.cfg.align) {
            self.cfg.align = a;
            parts.push(format!("align on {}", a.as_str()));
        }
        if let Some(t) = trigger.filter(|t| *t != self.cfg.trigger) {
            self.cfg.trigger = t;
            parts.push(format!("trigger {}", t.as_str()));
        }
        if let Some(l) =
            lookahead.filter(|l| (*l - self.cfg.lookahead_seconds).abs() > f64::EPSILON)
        {
            self.cfg.lookahead_seconds = l;
            self.lead = l;
            self.lead_logged = l;
            parts.push(format!("lookahead {l:.0} s"));
        }
        (!parts.is_empty()).then(|| self.entry(LogKind::Config, None, None, Some(parts.join(", "))))
    }

    /// Pause a channel (manual override). `value` is the operator's value,
    /// held as the target while paused (a server-loop setpoint keeps steering
    /// the server PID). `None` when it was already paused and nothing changed.
    /// Whether a channel is paused (a manual override).
    pub fn paused(&self, ch: Channel) -> bool {
        self.channels.get(&ch).is_some_and(|c| c.paused)
    }

    pub fn pause(&mut self, ch: Channel, reason: &str, value: Option<f64>) -> Option<LogEntry> {
        let c = self.channels.get_mut(&ch)?;
        if value.is_some() {
            c.held = value;
        }
        if c.paused {
            return None;
        }
        c.paused = true;
        Some(self.entry(LogKind::Paused, Some(ch), value, Some(reason.to_string())))
    }

    /// Resume a channel; the profile's current value is sent on the next tick.
    pub fn resume(&mut self, ch: Channel) -> Option<LogEntry> {
        let c = self.channels.get_mut(&ch)?;
        if !c.paused {
            return None;
        }
        c.paused = false;
        c.held = None;
        c.last_sent = None;
        c.last_sent_elapsed = None;
        let target = c.target;
        if ch == Channel::Heater {
            // The operator moved the heater: restart the server PID from it.
            self.pid = ServerPid::new(self.cfg.server_pid);
        }
        let mut e = self.entry(LogKind::Resumed, Some(ch), None, None);
        e.value = target;
        Some(e)
    }
}
