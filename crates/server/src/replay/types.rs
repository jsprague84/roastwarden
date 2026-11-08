//! What a drive is started and changed with (`StartReplay`, `UpdateReplay`, `EventOp`), where
//! the BT loop goes, the state streamed once a tick (`ReplayState`) and the errors.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::engine::{AlignOn, EngineView, LoopAt, ReplayMode, RunConfig, TriggerPolicy};
use crate::roasters::caps::DeviceCaps;
use crate::roasts::types::SessionStatus;
use crate::transports::transport::TransportError;

/// How a run is started (`POST /api/sessions/{id}/replay`).
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct StartReplay {
    #[serde(default)]
    pub mode: Option<ReplayMode>,
    #[serde(default)]
    pub trigger: Option<TriggerPolicy>,
    #[serde(default)]
    pub align: Option<AlignOn>,
    /// Defaults to "the profile has fan data".
    #[serde(default)]
    pub fan: Option<bool>,
    #[serde(default)]
    pub ghost: bool,
    #[serde(default)]
    pub lookahead_seconds: Option<f64>,
    /// Let the lookahead follow the loop's lag, up to `lookahead_seconds`
    /// (`docs/replay.md`, "Lookahead"); defaults to the setting
    /// `profile_lookahead_adapt` (on).
    #[serde(default)]
    pub adapt_lookahead: Option<bool>,
    /// Where the BT loop runs: `auto` (default), `device` or `server`.
    #[serde(default, rename = "loop")]
    pub loop_pref: LoopPref,
    /// Fan assist for this run; defaults to the roaster's saved setting.
    #[serde(default)]
    pub fan_assist: Option<bool>,
    /// Start at this profile time instead of the profile time now (a resume after a
    /// restart): the time between goes on the profile offset (`docs/replay.md`).
    #[serde(default)]
    pub resume_at_profile_seconds: Option<f64>,
    /// Auto DROP for this run: absent goes by the profile; `false` starts it cancelled (a
    /// resume of a drive whose auto DROP was cancelled), `true` as the profile has it.
    #[serde(default)]
    pub auto_drop: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopPref {
    #[default]
    Auto,
    Device,
    Server,
}

/// Place the BT loop for a mode on a device with these capabilities.
pub fn resolve_loop(
    mode: ReplayMode,
    pref: LoopPref,
    caps: DeviceCaps,
) -> Result<(LoopAt, String), String> {
    if !mode.closed_loop() {
        return Ok((LoopAt::Open, "open loop: heater % from the profile".into()));
    }
    let needs_ff = mode == ReplayMode::FeedforwardPid;
    let device_ok = caps.pid && (!needs_ff || caps.ff);
    let missing = if caps.pid {
        "the device PID takes no feedforward (firmware < 3.1)"
    } else {
        "the device has no onboard PID"
    };
    match pref {
        LoopPref::Server => Ok((LoopAt::Server, "server PID (requested)".into())),
        LoopPref::Device if device_ok => Ok((LoopAt::Device, "device PID (requested)".into())),
        LoopPref::Device => Err(format!("cannot run the loop on the device: {missing}")),
        LoopPref::Auto if device_ok => Ok((
            LoopAt::Device,
            if needs_ff {
                "device PID with feedforward".into()
            } else {
                "device PID".into()
            },
        )),
        LoopPref::Auto => Ok((LoopAt::Server, format!("server PID: {missing}"))),
    }
}

/// Mid-run changes (`PATCH /api/sessions/{id}/replay`).
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateReplay {
    #[serde(default)]
    pub ghost: Option<bool>,
    #[serde(default)]
    pub align: Option<AlignOn>,
    #[serde(default)]
    pub trigger: Option<TriggerPolicy>,
    #[serde(default)]
    pub lookahead_seconds: Option<f64>,
    /// `false` cancels auto DROP for this roast, `true` puts it back (`docs/replay.md`).
    #[serde(default)]
    pub auto_drop: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventOp {
    Skip,
    Delay,
    Fire,
}

/// Snapshot streamed once per tick. The top-level `lookahead_seconds`,
/// `hysteresis_c`, `elapsed_seconds`, `target_now`, `last_sent_setpoint`,
/// `last_sent_at` and `sends` keep the old follow state's shape.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ReplayState {
    pub run_id: String,
    pub session_id: String,
    pub device_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub active: bool,
    pub config: RunConfig,
    pub lookahead_seconds: f64,
    pub hysteresis_c: f64,
    pub elapsed_seconds: f64,
    pub last_sent_setpoint: Option<f64>,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub sends: u64,
    pub last_error: Option<String>,
    pub stopped_reason: Option<String>,
    /// The run ended because the roaster cut its heater (safe state, e-stop, an
    /// interlock): the Roast page offers to resume (`docs/roast-sequence.md`).
    pub stopped_by_device: bool,
    /// A restart of the server cut the run with the roast still on: rebuilt on boot, stopped
    /// at `profile_t`, and a resume picks up there (`docs/replay.md`).
    pub stopped_by_restart: bool,
    pub telemetry_stale: bool,
    pub updated_at: DateTime<Utc>,
    /// What the device can do, and why the loop runs where it does.
    pub caps: DeviceCaps,
    pub loop_reason: String,
    /// The device enforces a control lease for this run.
    pub lease: bool,
    #[serde(flatten)]
    pub view: EngineView,
}

#[derive(Debug)]
pub enum ReplayError {
    SessionNotFound,
    SessionNotActive(SessionStatus),
    NoProfile,
    EmptyProfile,
    NoHeaterData,
    NotRunning,
    Invalid(String),
    Db(String),
    Transport(TransportError),
    /// The roaster's control level does not allow a drive (docs/roaster-setup.md).
    ControlOff(TransportError),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::SessionNotFound => write!(f, "session not found"),
            ReplayError::SessionNotActive(s) => write!(f, "session is {s}, not active"),
            ReplayError::NoProfile => write!(f, "session has no profile to replay"),
            ReplayError::EmptyProfile => write!(f, "profile has no points"),
            ReplayError::NoHeaterData => write!(
                f,
                "feedforward needs heater % in the profile points or heater events"
            ),
            ReplayError::NotRunning => write!(f, "no replay is running for this session"),
            ReplayError::Invalid(m) => write!(f, "{m}"),
            ReplayError::Db(e) => write!(f, "database error: {e}"),
            ReplayError::Transport(e) => write!(f, "could not switch the device mode: {e}"),
            ReplayError::ControlOff(e) => {
                write!(
                    f,
                    "{e}; Profile drive needs full control (ghost still works)"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(pid: bool, ff: bool) -> DeviceCaps {
        DeviceCaps {
            pid,
            ff,
            ..DeviceCaps::default()
        }
    }

    #[test]
    fn the_loop_goes_where_the_device_can_run_it() {
        use LoopPref::*;
        use ReplayMode::*;
        let fw31 = caps(true, true);
        let fw30 = caps(true, false);
        let dumb = caps(false, false);
        assert_eq!(
            resolve_loop(Setpoint, Auto, fw30).unwrap().0,
            LoopAt::Device
        );
        assert_eq!(
            resolve_loop(Setpoint, Auto, dumb).unwrap().0,
            LoopAt::Server
        );
        assert_eq!(
            resolve_loop(FeedforwardPid, Auto, fw31).unwrap().0,
            LoopAt::Device
        );
        let (at, why) = resolve_loop(FeedforwardPid, Auto, fw30).unwrap();
        assert_eq!(at, LoopAt::Server);
        assert!(why.contains("feedforward"), "{why}");
        assert_eq!(
            resolve_loop(Feedforward, Device, dumb).unwrap().0,
            LoopAt::Open
        );
        assert_eq!(
            resolve_loop(Setpoint, Server, fw31).unwrap().0,
            LoopAt::Server
        );
        assert!(resolve_loop(FeedforwardPid, Device, fw30).is_err());
        assert!(resolve_loop(Setpoint, Device, dumb).is_err());
    }
}
