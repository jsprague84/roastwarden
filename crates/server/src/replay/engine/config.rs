//! A run's vocabulary: the channels it drives, its mode, where the BT loop runs, how events
//! trigger, the landmark it aligns on; and the run configuration itself.

use roastwarden_core::ControlMode;
use serde::{Deserialize, Serialize};

use super::pid::PidGains;
use super::MIN_DRIVE_FAN_PCT;
use crate::control::fan_assist::FanAssistConfig;
use crate::replay::auto_drop::AutoDrop;
use crate::roasts::types::RoastEventType;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Setpoint,
    Heater,
    Fan,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Setpoint, Channel::Heater, Channel::Fan];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Setpoint => "setpoint",
            Channel::Heater => "heater",
            Channel::Fan => "fan",
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "setpoint" => Ok(Channel::Setpoint),
            "heater" => Ok(Channel::Heater),
            "fan" => Ok(Channel::Fan),
            other => Err(format!("unknown channel: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// Closed loop on the profile BT curve (the old "follow").
    #[default]
    Setpoint,
    /// Open loop: heater % straight from the profile.
    Feedforward,
    /// Closed loop on the BT curve plus the profile heater % as feedforward.
    FeedforwardPid,
}

impl ReplayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayMode::Setpoint => "setpoint",
            ReplayMode::Feedforward => "feedforward",
            ReplayMode::FeedforwardPid => "feedforward_pid",
        }
    }

    pub fn closed_loop(self) -> bool {
        self != ReplayMode::Feedforward
    }
}

/// Where the BT loop runs for a run (`docs/control.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopAt {
    /// The device PID closes the loop; the server streams setpoint (+ feedforward).
    #[default]
    Device,
    /// The server PID closes the loop and sends heater %.
    Server,
    /// No loop: feedforward mode.
    Open,
}

impl LoopAt {
    pub fn as_str(self) -> &'static str {
        match self {
            LoopAt::Device => "device",
            LoopAt::Server => "server",
            LoopAt::Open => "open",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerPolicy {
    /// Each event's own `trigger_kind`.
    #[default]
    Profile,
    /// By time from CHARGE, where the event has a time.
    Time,
    /// By bean temperature, where the event has one.
    Bt,
}

impl TriggerPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerPolicy::Profile => "profile",
            TriggerPolicy::Time => "time",
            TriggerPolicy::Bt => "bt",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AlignOn {
    #[default]
    Charge,
    TurningPoint,
    DryingEnd,
    FirstCrackStart,
}

impl AlignOn {
    pub fn as_str(self) -> &'static str {
        match self {
            AlignOn::Charge => "charge",
            AlignOn::TurningPoint => "turning_point",
            AlignOn::DryingEnd => "drying_end",
            AlignOn::FirstCrackStart => "first_crack_start",
        }
    }

    pub fn from_event_type(t: &RoastEventType) -> Option<AlignOn> {
        match t {
            RoastEventType::Charge => Some(AlignOn::Charge),
            RoastEventType::TurningPoint => Some(AlignOn::TurningPoint),
            RoastEventType::DryingEnd => Some(AlignOn::DryingEnd),
            RoastEventType::FirstCrackStart => Some(AlignOn::FirstCrackStart),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RunConfig {
    pub mode: ReplayMode,
    pub trigger: TriggerPolicy,
    pub align: AlignOn,
    /// Replay the fan channel.
    pub fan: bool,
    pub ghost: bool,
    pub lookahead_seconds: f64,
    /// Let the lookahead follow the loop's lag: `lookahead_seconds` is the
    /// most it uses (`docs/replay.md`, "Lookahead").
    #[serde(default)]
    pub adapt_lookahead: bool,
    pub hysteresis_c: f64,
    /// Where the BT loop runs (resolved from device capabilities at start).
    #[serde(rename = "loop")]
    pub loop_at: LoopAt,
    /// Gains when the loop runs in the server.
    pub server_pid: PidGains,
    /// Fan assist (`docs/fan-assist.md`): trims the fan below the profile's
    /// when the heater is pinned. Closed-loop modes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan_assist: Option<FanAssistConfig>,
    /// A run resumed after a restart cut the last one (`docs/replay.md`, "Resume after a
    /// restart"): the profile waits out the time no drive ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<Hold>,
    /// Auto DROP (`docs/replay.md`, "Auto DROP"): from the profile, off when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_drop: Option<AutoDrop>,
    /// The roaster's fan floor while heating, %: a profile fan below it is raised to it
    /// (docs/roaster-setup.md, "The interlock"). 45 on the fluid bed; 0 on a drum.
    #[serde(default = "default_fan_floor_pct")]
    pub fan_floor_pct: f64,
}

fn default_fan_floor_pct() -> f64 {
    MIN_DRIVE_FAN_PCT
}

/// The time a cut drive lost, added to the profile offset of the run that resumes it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Hold {
    /// Seconds added to the profile offset.
    pub seconds: f64,
    /// Session time the run resumed at: a landmark marked later aligns on its own.
    pub since_elapsed: f64,
}

impl RunConfig {
    /// Device control mode the run needs: auto only when the device PID
    /// closes the loop.
    pub fn device_mode(&self) -> ControlMode {
        if self.mode.closed_loop() && self.loop_at == LoopAt::Device {
            ControlMode::Auto
        } else {
            ControlMode::Manual
        }
    }
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            mode: ReplayMode::Setpoint,
            trigger: TriggerPolicy::Profile,
            align: AlignOn::Charge,
            fan: false,
            ghost: false,
            lookahead_seconds: 20.0,
            adapt_lookahead: false,
            hysteresis_c: 1.0,
            loop_at: LoopAt::Device,
            server_pid: PidGains::default(),
            fan_assist: None,
            hold: None,
            auto_drop: None,
            fan_floor_pct: MIN_DRIVE_FAN_PCT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<T: serde::Serialize + Copy>(all: &[T], as_str: fn(T) -> &'static str) {
        for v in all {
            assert_eq!(
                serde_json::to_value(v).unwrap(),
                serde_json::Value::String(as_str(*v).into())
            );
        }
    }

    #[test]
    fn as_str_is_the_wire_name_of_every_variant() {
        names(&Channel::ALL, Channel::as_str);
        names(
            &[
                ReplayMode::Setpoint,
                ReplayMode::Feedforward,
                ReplayMode::FeedforwardPid,
            ],
            ReplayMode::as_str,
        );
        names(
            &[LoopAt::Device, LoopAt::Server, LoopAt::Open],
            LoopAt::as_str,
        );
        names(
            &[
                TriggerPolicy::Profile,
                TriggerPolicy::Time,
                TriggerPolicy::Bt,
            ],
            TriggerPolicy::as_str,
        );
        names(
            &[
                AlignOn::Charge,
                AlignOn::TurningPoint,
                AlignOn::DryingEnd,
                AlignOn::FirstCrackStart,
            ],
            AlignOn::as_str,
        );
        for ch in Channel::ALL {
            assert_eq!(ch.as_str().parse::<Channel>().ok(), Some(ch));
        }
    }

    #[test]
    fn the_align_landmarks_are_charge_turning_point_dry_end_and_first_crack() {
        let of = AlignOn::from_event_type;
        assert_eq!(of(&RoastEventType::Charge), Some(AlignOn::Charge));
        assert_eq!(
            of(&RoastEventType::TurningPoint),
            Some(AlignOn::TurningPoint)
        );
        assert_eq!(of(&RoastEventType::DryingEnd), Some(AlignOn::DryingEnd));
        assert_eq!(
            of(&RoastEventType::FirstCrackStart),
            Some(AlignOn::FirstCrackStart)
        );
        assert_eq!(of(&RoastEventType::Drop), None);
    }

    #[test]
    fn a_run_stored_without_a_fan_floor_takes_the_fluid_beds() {
        let mut v = serde_json::to_value(RunConfig::default()).unwrap();
        v.as_object_mut().unwrap().remove("fan_floor_pct");
        let cfg: RunConfig = serde_json::from_value(v).unwrap();
        assert_eq!(cfg.fan_floor_pct, MIN_DRIVE_FAN_PCT);
    }
}
