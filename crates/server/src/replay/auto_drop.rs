//! Auto DROP (`docs/replay.md`, "Auto DROP"): a drive marks DROP by itself
//! at the profile's drop, by time, by BT or by development time after first crack, after a
//! countdown the operator can cancel. Pure: the engine feeds it one [`DropInput`] per tick;
//! the roast sequence marks DROP when the state reads [`DropState::Due`] (so DROP's step runs:
//! heater off, fan to cool) and sends the notice when the countdown starts.

use serde::{Deserialize, Serialize};

use crate::profiles::types::RoastProfile;

/// The countdown when the setting `auto_drop_countdown_seconds` is not set.
pub const DEFAULT_COUNTDOWN_SECS: f64 = 10.0;
/// The setting's range.
pub const COUNTDOWN_RANGE: (f64, f64) = (3.0, 60.0);
/// A countdown that would be shorter than the full one by more than this (a drive started
/// past the drop, first crack marked late) gets the full one: the operator always has the
/// time to cancel. Three seconds covers a tick (1 s), the BT trigger's second tick and
/// rounding.
const LATE_SLACK_SECS: f64 = 3.0;
/// A BT trigger counts only from this share of the profile's drop time when the profile has
/// no first crack time (with one, from its first crack).
const BT_GUARD_SHARE: f64 = 0.8;
/// Below this RoR (°C/min) a BT trigger gives no estimate.
const MIN_ROR: f64 = 0.5;
/// A BT trigger starts its countdown on this many ticks in a row within reach.
const BT_TICKS: u8 = 2;

/// What a profile's auto DROP goes by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropBy {
    /// The profile's drop time from CHARGE (`target_total_time`), in profile time: an align
    /// or a resumed drive moves it as it moves the rest of the profile.
    Time,
    /// The profile's drop temperature (`target_end_temp`), once first crack is marked or the
    /// profile is past its first crack time.
    Bt,
    /// The profile's development time (drop minus first crack) after first crack is marked.
    Development,
}

impl DropBy {
    pub fn as_str(self) -> &'static str {
        match self {
            DropBy::Time => "time",
            DropBy::Bt => "bt",
            DropBy::Development => "development",
        }
    }
}

impl std::str::FromStr for DropBy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "time" => Ok(DropBy::Time),
            "bt" => Ok(DropBy::Bt),
            "development" => Ok(DropBy::Development),
            other => Err(format!(
                "unknown auto DROP {other:?} (one of time, bt, development; or off)"
            )),
        }
    }
}

/// A drive's auto DROP (in its run configuration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AutoDrop {
    pub by: DropBy,
    /// Profile seconds from CHARGE (`time`), °C (`bt`) or seconds after first crack
    /// (`development`).
    pub at: f64,
    pub countdown_seconds: f64,
    /// A BT trigger counts from this profile time when first crack is not marked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bt_from_profile_t: Option<f64>,
    /// Cancelled for this roast (the Roast page's Cancel); a resumed drive keeps it.
    #[serde(default)]
    pub cancelled: bool,
}

impl AutoDrop {
    /// A profile's auto DROP, or `None` when it is off. An error names what the profile
    /// lacks for the trigger it chose.
    pub fn from_profile(p: &RoastProfile, countdown: f64) -> Result<Option<AutoDrop>, String> {
        let Some(by) = p.auto_drop.as_deref() else {
            return Ok(None);
        };
        let by: DropBy = by.parse()?;
        let total = p.target_total_time.map(f64::from);
        let fc = p.target_first_crack.map(f64::from);
        let at = trigger_value(by, total, fc, p.target_end_temp.map(f64::from))?;
        Ok(Some(AutoDrop {
            by,
            at,
            countdown_seconds: countdown,
            bt_from_profile_t: (by == DropBy::Bt)
                .then(|| fc.or(total.map(|t| t * BT_GUARD_SHARE)))
                .flatten(),
            cancelled: false,
        }))
    }

    /// One line: "at 10:05", "at 210 °C", "2:15 after first crack".
    pub fn describe(&self) -> String {
        match self.by {
            DropBy::Time => format!("at the profile's drop, {}", clock(self.at)),
            DropBy::Bt => format!("at the profile's drop BT, {:.1} °C", self.at),
            DropBy::Development => format!("{} after first crack", clock(self.at)),
        }
    }
}

/// What a trigger fires at, from a profile's targets; an error says what is missing.
pub fn trigger_value(
    by: DropBy,
    total: Option<f64>,
    fc: Option<f64>,
    end_temp: Option<f64>,
) -> Result<f64, String> {
    match by {
        DropBy::Time => total
            .filter(|t| *t > 0.0)
            .ok_or_else(|| "auto DROP by time needs the profile's drop time".to_string()),
        DropBy::Bt => end_temp
            .filter(|t| t.is_finite() && (100.0..=300.0).contains(t))
            .ok_or_else(|| {
                "auto DROP by BT needs the profile's drop temperature (100–300 °C)".to_string()
            }),
        DropBy::Development => match (total, fc) {
            (Some(t), Some(f)) if t > f && f > 0.0 => Ok(t - f),
            _ => Err(
                "auto DROP by development needs the profile's first crack and drop times, drop after first crack"
                    .to_string(),
            ),
        },
    }
}

fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Where an auto DROP is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropState {
    /// Watching for the trigger (`waiting_for` says what it still needs).
    Armed,
    /// The countdown runs; DROP at `drop_at_elapsed`.
    Counting,
    /// The countdown is over: DROP is being marked.
    Due,
    /// Cancelled for this roast.
    Cancelled,
}

/// The state's `auto_drop`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AutoDropView {
    #[serde(flatten)]
    pub config: AutoDrop,
    pub state: DropState,
    /// Seconds to DROP: the countdown's while it runs, else an estimate when one can be made.
    pub eta_seconds: Option<f64>,
    /// Session seconds DROP is marked at, while counting or due.
    pub drop_at_elapsed: Option<f64>,
    /// What it waits for before it can count down: CHARGE, first crack, a fresh BT.
    pub waiting_for: Option<&'static str>,
    /// A ghost run shows when it would drop and marks nothing.
    pub ghost: bool,
    pub describe: String,
}

/// One tick's inputs.
#[derive(Debug, Clone, Copy, Default)]
pub struct DropInput {
    /// Session seconds.
    pub elapsed: f64,
    /// Seconds since CHARGE.
    pub live_t: f64,
    /// Profile seconds (align and a resume's hold applied).
    pub profile_t: f64,
    pub charge_marked: bool,
    /// Seconds since CHARGE first crack was marked at.
    pub fc_live_t: Option<f64>,
    /// BT, `None` when stale.
    pub bt: Option<f64>,
    pub ror: Option<f64>,
    pub ghost: bool,
}

/// An auto DROP's runtime.
#[derive(Debug, Clone, Default)]
pub struct DropRt {
    /// Session seconds DROP is marked at, once the countdown started.
    drop_at: Option<f64>,
    due: bool,
    eta: Option<f64>,
    waiting: Option<&'static str>,
    /// BT trigger: ticks in a row within reach.
    near: u8,
}

impl DropRt {
    /// Back to watching (a cancel, a re-arm).
    pub fn reset(&mut self) {
        *self = DropRt::default();
    }

    /// Advance one tick; returns a line for the log when the countdown starts or ends.
    pub fn step(&mut self, cfg: &AutoDrop, i: &DropInput) -> Option<String> {
        if cfg.cancelled {
            self.reset();
            return None;
        }
        if let Some(at) = self.drop_at {
            self.eta = Some((at - i.elapsed).max(0.0));
            if i.elapsed >= at && !self.due {
                self.due = true;
                return Some(format!(
                    "auto DROP: countdown over, DROP ({})",
                    cfg.describe()
                ));
            }
            return None;
        }
        let (eta, waiting) = Self::estimate(cfg, i);
        self.eta = eta;
        self.waiting = waiting;
        let within = eta.is_some_and(|e| e <= cfg.countdown_seconds);
        if cfg.by == DropBy::Bt {
            self.near = if within {
                self.near.saturating_add(1)
            } else {
                0
            };
        }
        // A ghost run shows when it would drop and never counts down.
        if !within || i.ghost || (cfg.by == DropBy::Bt && self.near < BT_TICKS) {
            return None;
        }
        let eta = eta.unwrap_or(0.0);
        let lead = if eta < cfg.countdown_seconds - LATE_SLACK_SECS {
            cfg.countdown_seconds
        } else {
            eta.max(0.0)
        };
        self.drop_at = Some(i.elapsed + lead);
        self.eta = Some(lead);
        Some(format!(
            "auto DROP in {lead:.0} s ({}); cancel to roast on",
            cfg.describe()
        ))
    }

    fn estimate(cfg: &AutoDrop, i: &DropInput) -> (Option<f64>, Option<&'static str>) {
        if !i.charge_marked {
            return (None, Some("CHARGE"));
        }
        match cfg.by {
            DropBy::Time => (Some(cfg.at - i.profile_t), None),
            DropBy::Development => match i.fc_live_t {
                Some(fc) => (Some(cfg.at - (i.live_t - fc)), None),
                None => (None, Some("first crack")),
            },
            DropBy::Bt => {
                // A profile with neither time waits for first crack to be marked.
                let late = i.fc_live_t.is_some()
                    || cfg.bt_from_profile_t.is_some_and(|t| i.profile_t >= t);
                if !late {
                    return (
                        None,
                        Some("first crack, or the profile past its first crack"),
                    );
                }
                match (i.bt, i.ror) {
                    (None, _) => (None, Some("a fresh BT")),
                    (Some(b), _) if b >= cfg.at => (Some(0.0), None),
                    (Some(b), Some(r)) if r >= MIN_ROR => (Some((cfg.at - b) / r * 60.0), None),
                    // Not rising: no estimate, and no countdown until it gets there.
                    _ => (None, None),
                }
            }
        }
    }

    pub fn state(&self, cfg: &AutoDrop) -> DropState {
        if cfg.cancelled {
            DropState::Cancelled
        } else if self.due {
            DropState::Due
        } else if self.drop_at.is_some() {
            DropState::Counting
        } else {
            DropState::Armed
        }
    }

    pub fn view(&self, cfg: &AutoDrop, ghost: bool) -> AutoDropView {
        let state = self.state(cfg);
        AutoDropView {
            config: cfg.clone(),
            state,
            eta_seconds: (state != DropState::Cancelled)
                .then_some(self.eta)
                .flatten()
                .map(|e| (e * 10.0).round() / 10.0),
            drop_at_elapsed: self.drop_at,
            waiting_for: (state == DropState::Armed)
                .then_some(self.waiting)
                .flatten(),
            ghost,
            describe: cfg.describe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(by: DropBy, at: f64) -> AutoDrop {
        AutoDrop {
            by,
            at,
            countdown_seconds: 10.0,
            bt_from_profile_t: (by == DropBy::Bt).then_some(470.0),
            cancelled: false,
        }
    }

    /// Charged at session second 30, profile time = live time.
    fn input(live_t: f64) -> DropInput {
        DropInput {
            elapsed: live_t + 30.0,
            live_t,
            profile_t: live_t,
            charge_marked: true,
            ..DropInput::default()
        }
    }

    /// Ticks once a second from `from` to `to` (live time); returns the live time of each
    /// log line.
    fn run(
        rt: &mut DropRt,
        c: &AutoDrop,
        from: f64,
        to: f64,
        f: impl Fn(f64) -> DropInput,
    ) -> Vec<(f64, String)> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            if let Some(n) = rt.step(c, &f(t)) {
                out.push((t, n));
            }
            t += 1.0;
        }
        out
    }

    #[test]
    fn by_time_the_countdown_ends_on_the_profiles_drop() {
        let c = cfg(DropBy::Time, 605.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 0.0, 620.0, input);
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(log[0].0, 595.0, "the countdown starts 10 s before");
        assert!(log[0].1.contains("in 10 s"), "{}", log[0].1);
        assert_eq!(log[1].0, 605.0, "DROP on the profile's drop");
        assert_eq!(rt.state(&c), DropState::Due);
        let v = rt.view(&c, false);
        assert_eq!(v.drop_at_elapsed, Some(635.0));
        assert_eq!(v.eta_seconds, Some(0.0));
    }

    #[test]
    fn by_time_follows_the_profile_clock() {
        // Aligned 20 s later (profile_t = live_t − 20): DROP at live 625.
        let c = cfg(DropBy::Time, 605.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 0.0, 640.0, |t| DropInput {
            profile_t: t - 20.0,
            ..input(t)
        });
        assert_eq!(log.last().map(|l| l.0), Some(625.0));
    }

    #[test]
    fn nothing_before_charge_and_nothing_in_a_ghost() {
        let c = cfg(DropBy::Time, 605.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 0.0, 700.0, |t| DropInput {
            charge_marked: false,
            ..input(t)
        });
        assert_eq!(log, [] as [(f64, std::string::String); 0]);
        assert_eq!(rt.view(&c, false).waiting_for, Some("CHARGE"));
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 0.0, 700.0, |t| DropInput {
            ghost: true,
            ..input(t)
        });
        assert!(log.is_empty(), "a ghost shows, never counts: {log:?}");
        assert_eq!(rt.state(&c), DropState::Armed);
        assert!(rt.view(&c, true).eta_seconds.is_some_and(|e| e < 0.0));
    }

    #[test]
    fn a_drive_started_past_the_drop_gets_the_full_countdown() {
        let c = cfg(DropBy::Time, 605.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 640.0, 660.0, input);
        assert_eq!(log[0].0, 640.0);
        assert_eq!(log[1].0, 650.0, "10 s to cancel: {log:?}");
    }

    #[test]
    fn by_development_waits_for_first_crack() {
        let c = cfg(DropBy::Development, 135.0);
        let mut rt = DropRt::default();
        let fc = |from: f64| {
            move |t: f64| DropInput {
                fc_live_t: (t >= from).then_some(from),
                ..input(t)
            }
        };
        let log = run(&mut rt, &c, 0.0, 460.0, fc(470.0));
        assert_eq!(log, [] as [(f64, std::string::String); 0]);
        assert_eq!(rt.view(&c, false).waiting_for, Some("first crack"));
        let log = run(&mut rt, &c, 461.0, 700.0, fc(470.0));
        assert_eq!(log[0].0, 595.0);
        assert_eq!(log[1].0, 605.0, "135 s after first crack at 7:50");
    }

    #[test]
    fn first_crack_marked_late_still_gives_the_full_countdown() {
        // FC marked at 9:00 for a mark the operator dated back to 7:50: already past.
        let c = cfg(DropBy::Development, 60.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 540.0, 560.0, |t| DropInput {
            fc_live_t: Some(470.0),
            ..input(t)
        });
        assert_eq!(log[0].0, 540.0);
        assert_eq!(log[1].0, 550.0);
    }

    /// BT rising 5 °C/min from 200 °C at 7:50.
    fn late_roast(t: f64) -> DropInput {
        DropInput {
            bt: Some(200.0 + (t - 470.0) * 5.0 / 60.0),
            ror: Some(5.0),
            ..input(t)
        }
    }

    #[test]
    fn by_bt_drops_when_the_profiles_drop_bt_is_reached() {
        let c = cfg(DropBy::Bt, 210.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 470.0, 620.0, late_roast);
        // 210 °C at 470 + 120 = 590 s; within 10 s just after 580, two ticks in a row: 582.
        assert_eq!(log[0].0, 582.0, "{log:?}");
        assert_eq!(log[1].0, 590.0, "DROP as BT reaches 210 °C");
    }

    #[test]
    fn by_bt_needs_the_late_roast() {
        // 210 °C at 3:00 (a spike, a hot probe): not before the profile's first crack.
        let c = cfg(DropBy::Bt, 210.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 170.0, 200.0, |t| DropInput {
            bt: Some(211.0),
            ror: Some(30.0),
            ..input(t)
        });
        assert!(log.is_empty(), "{log:?}");
        assert!(rt
            .view(&c, false)
            .waiting_for
            .is_some_and(|w| w.contains("first crack")));
        // With no first crack or drop time in the profile, only a marked first crack counts.
        let bare = AutoDrop {
            bt_from_profile_t: None,
            ..c.clone()
        };
        let mut rt2 = DropRt::default();
        let log = run(&mut rt2, &bare, 600.0, 620.0, |t| DropInput {
            bt: Some(211.0),
            ror: Some(5.0),
            ..input(t)
        });
        assert!(log.is_empty(), "{log:?}");
        // First crack marked: it counts.
        let log = run(&mut rt, &c, 201.0, 215.0, |t| DropInput {
            bt: Some(211.0),
            ror: Some(30.0),
            fc_live_t: Some(195.0),
            ..input(t)
        });
        assert_eq!(log.len(), 2, "{log:?}");
    }

    #[test]
    fn by_bt_one_noisy_reading_starts_nothing() {
        let c = cfg(DropBy::Bt, 210.0);
        let mut rt = DropRt::default();
        let log = run(&mut rt, &c, 500.0, 530.0, |t| DropInput {
            bt: Some(if t == 510.0 { 210.5 } else { 203.0 }),
            ror: Some(5.0),
            ..input(t)
        });
        assert!(log.is_empty(), "{log:?}");
        // Stale BT: waits.
        rt.step(
            &c,
            &DropInput {
                bt: None,
                ..input(531.0)
            },
        );
        assert_eq!(rt.view(&c, false).waiting_for, Some("a fresh BT"));
    }

    #[test]
    fn once_counting_a_dip_does_not_stop_it_but_a_cancel_does() {
        let c = cfg(DropBy::Bt, 210.0);
        let mut rt = DropRt::default();
        run(&mut rt, &c, 470.0, 583.0, late_roast);
        assert_eq!(rt.state(&c), DropState::Counting);
        let log = run(&mut rt, &c, 584.0, 600.0, |t| DropInput {
            bt: Some(205.0),
            ..late_roast(t)
        });
        assert_eq!(log.len(), 1, "due despite the dip");
        let mut rt = DropRt::default();
        run(&mut rt, &c, 470.0, 583.0, late_roast);
        let cancelled = AutoDrop {
            cancelled: true,
            ..c.clone()
        };
        let log = run(&mut rt, &cancelled, 584.0, 700.0, late_roast);
        assert_eq!(log, [] as [(f64, std::string::String); 0]);
        assert_eq!(rt.state(&cancelled), DropState::Cancelled);
        assert_eq!(rt.view(&cancelled, false).eta_seconds, None);
    }

    #[test]
    fn a_profile_names_what_it_lacks() {
        assert_eq!(
            trigger_value(DropBy::Time, Some(605.0), None, None),
            Ok(605.0)
        );
        assert_eq!(
            trigger_value(DropBy::Development, Some(605.0), Some(470.0), None),
            Ok(135.0)
        );
        assert!(trigger_value(DropBy::Development, Some(605.0), None, None)
            .unwrap_err()
            .contains("first crack"));
        assert!(trigger_value(DropBy::Bt, None, None, None).is_err());
        assert!(trigger_value(DropBy::Bt, None, None, Some(50.0)).is_err());
        assert!("later".parse::<DropBy>().is_err());
    }
}
