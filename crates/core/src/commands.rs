//! Device control commands shared by every transport.
//!
//! A [`DeviceCommand`] is transport-agnostic. Each transport decides how to
//! carry it: MQTT and WebSocket use the topic/payload pair from
//! [`DeviceCommand::topic`] and [`DeviceCommand::payload`], Modbus maps the
//! command onto a register, and a future serial/TC4 transport will render it
//! as a line command. Keeping this enum in `roastwarden-core` lets a Rust
//! firmware depend on the same definition.

use serde::{Deserialize, Serialize};

use crate::topics;

/// Controller operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlMode {
    Manual,
    Auto,
}

impl ControlMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlMode::Manual => "manual",
            ControlMode::Auto => "auto",
        }
    }

    pub fn as_u16(&self) -> u16 {
        match self {
            ControlMode::Manual => 0,
            ControlMode::Auto => 1,
        }
    }
}

impl std::str::FromStr for ControlMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "manual" | "0" => Ok(ControlMode::Manual),
            "auto" | "1" => Ok(ControlMode::Auto),
            other => Err(format!("invalid control mode: {other}")),
        }
    }
}

/// A control command addressed to one roaster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceCommand {
    /// Target bean temperature in °C.
    Setpoint {
        value: f64,
    },
    /// Fan speed, 0..=100 %.
    Fan {
        percent: f64,
    },
    /// Manual heater power, 0..=100 %.
    Heater {
        percent: f64,
    },
    /// Drum speed, 0..=100 % (a drum roaster's; docs/ingest-mapping.md).
    Drum {
        percent: f64,
    },
    /// Damper (exhaust air) position, 0..=100 %.
    Damper {
        percent: f64,
    },
    Mode {
        mode: ControlMode,
    },
    HeaterEnable {
        enabled: bool,
    },
    Pid {
        kp: f64,
        ki: f64,
        kd: f64,
        /// Firmware 3.2 to 3.4 stored the gains in NVS unless this is `false`;
        /// 3.5 keeps gains in RAM only and ignores it (the server keeps the
        /// gains and sends them when the roaster comes online).
        #[serde(default = "default_true")]
        persist: bool,
    },
    EmergencyStop,
    /// Feedforward heater % added to the device PID in auto mode; `None`
    /// clears it. Firmware 3.1+ (`caps` contains `ff`).
    Feedforward {
        value: Option<f64>,
    },
    /// Control lease in milliseconds; the device enters its safe state when
    /// it is not refreshed in time. `0` releases it. Firmware 3.1+ (`lease`).
    Lease {
        ms: u32,
    },
    /// Rate-of-rise limiter: cut the applied heater while RoR (°C/min) is
    /// above `max` and BT is at or above `min_temp`. `max` 0 turns it off.
    /// Firmware 3.2+ (`caps` contains `ror`).
    RorLimit {
        max: f64,
        min_temp: f64,
    },
}

fn default_true() -> bool {
    true
}

/// Percent payload: clamped to 0..=100, one decimal, no trailing ".0".
fn pct(v: f64) -> String {
    let v = if v.is_finite() {
        v.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.1}")
    }
}

impl DeviceCommand {
    /// Short stable name, used in logs and error messages.
    pub fn name(&self) -> &'static str {
        match self {
            DeviceCommand::Setpoint { .. } => "setpoint",
            DeviceCommand::Fan { .. } => "fan",
            DeviceCommand::Heater { .. } => "heater",
            DeviceCommand::Drum { .. } => "drum",
            DeviceCommand::Damper { .. } => "damper",
            DeviceCommand::Mode { .. } => "mode",
            DeviceCommand::HeaterEnable { .. } => "heater_enable",
            DeviceCommand::Pid { .. } => "pid",
            DeviceCommand::EmergencyStop => "emergency_stop",
            DeviceCommand::Feedforward { .. } => "feedforward",
            DeviceCommand::Lease { .. } => "lease",
            DeviceCommand::RorLimit { .. } => "ror_limit",
        }
    }

    /// MQTT topic this command is published on for `device_id`.
    pub fn topic(&self, device_id: &str) -> String {
        match self {
            DeviceCommand::Setpoint { .. } => topics::control_setpoint(device_id),
            DeviceCommand::Fan { .. } => topics::control_fan(device_id),
            DeviceCommand::Heater { .. } => topics::control_heater(device_id),
            DeviceCommand::Drum { .. } => topics::control_drum(device_id),
            DeviceCommand::Damper { .. } => topics::control_damper(device_id),
            DeviceCommand::Mode { .. } => topics::control_mode(device_id),
            DeviceCommand::HeaterEnable { .. } => topics::control_heater_enable(device_id),
            DeviceCommand::Pid { .. } => topics::control_pid(device_id),
            DeviceCommand::EmergencyStop => topics::control_emergency_stop(device_id),
            DeviceCommand::Feedforward { .. } => topics::control_feedforward(device_id),
            DeviceCommand::Lease { .. } => topics::control_lease(device_id),
            DeviceCommand::RorLimit { .. } => topics::control_ror_limit(device_id),
        }
    }

    /// Text payload as the ESP32 firmware expects it on MQTT and WebSocket.
    pub fn payload(&self) -> String {
        match self {
            DeviceCommand::Setpoint { value } => format!("{value}"),
            DeviceCommand::Fan { percent }
            | DeviceCommand::Heater { percent }
            | DeviceCommand::Drum { percent }
            | DeviceCommand::Damper { percent } => pct(*percent),
            DeviceCommand::Mode { mode } => mode.as_str().to_string(),
            DeviceCommand::HeaterEnable { enabled } => {
                if *enabled {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            DeviceCommand::Pid {
                kp,
                ki,
                kd,
                persist,
            } => {
                let mut v = serde_json::json!({ "kp": kp, "ki": ki, "kd": kd });
                if !persist {
                    v["persist"] = false.into();
                }
                v.to_string()
            }
            DeviceCommand::EmergencyStop => "1".into(),
            DeviceCommand::Feedforward { value } => match value {
                Some(v) => format!("{:.1}", v.clamp(0.0, 100.0)),
                None => "off".into(),
            },
            DeviceCommand::Lease { ms } => ms.to_string(),
            DeviceCommand::RorLimit { max, min_temp } => {
                serde_json::json!({ "max": max, "min_temp": min_temp }).to_string()
            }
        }
    }

    /// Whether the command is a safety action that must never be dropped
    /// silently (used by transports to pick QoS and logging level).
    pub fn is_safety_critical(&self) -> bool {
        matches!(
            self,
            DeviceCommand::EmergencyStop | DeviceCommand::HeaterEnable { enabled: false }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_and_payloads_match_firmware_contract() {
        let cmd = DeviceCommand::Setpoint { value: 200.5 };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/setpoint");
        assert_eq!(cmd.payload(), "200.5");

        let cmd = DeviceCommand::Fan { percent: 74.96 };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/fan");
        assert_eq!(cmd.payload(), "75");
        let cmd = DeviceCommand::Heater { percent: 62.54 };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/heater");
        assert_eq!(cmd.payload(), "62.5");
        assert_eq!(DeviceCommand::Fan { percent: 140.0 }.payload(), "100");
        assert_eq!(DeviceCommand::Fan { percent: f64::NAN }.payload(), "0");

        let cmd = DeviceCommand::Drum { percent: 40.04 };
        assert_eq!(cmd.name(), "drum");
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/drum");
        assert_eq!(cmd.payload(), "40");
        let cmd = DeviceCommand::Damper { percent: 33.33 };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/damper");
        assert_eq!(cmd.payload(), "33.3");
        let back: DeviceCommand =
            serde_json::from_str(r#"{"type":"damper","percent":20}"#).unwrap();
        assert_eq!(back, DeviceCommand::Damper { percent: 20.0 });

        let cmd = DeviceCommand::Mode {
            mode: ControlMode::Auto,
        };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/mode");
        assert_eq!(cmd.payload(), "auto");

        let cmd = DeviceCommand::HeaterEnable { enabled: false };
        assert_eq!(cmd.payload(), "0");
        assert!(cmd.is_safety_critical());

        let cmd = DeviceCommand::Pid {
            kp: 15.0,
            ki: 1.0,
            kd: 25.0,
            persist: true,
        };
        let v: serde_json::Value = serde_json::from_str(&cmd.payload()).unwrap();
        assert_eq!(v["kp"], 15.0);
        assert!(v.get("persist").is_none());
        let cmd = DeviceCommand::Pid {
            kp: 15.0,
            ki: 1.0,
            kd: 25.0,
            persist: false,
        };
        let v: serde_json::Value = serde_json::from_str(&cmd.payload()).unwrap();
        assert_eq!(v["persist"], false);
        let back: DeviceCommand =
            serde_json::from_str(r#"{"type":"pid","kp":1,"ki":2,"kd":3}"#).unwrap();
        assert!(matches!(back, DeviceCommand::Pid { persist: true, .. }));

        let cmd = DeviceCommand::RorLimit {
            max: 30.0,
            min_temp: 150.0,
        };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/ror_limit");
        let v: serde_json::Value = serde_json::from_str(&cmd.payload()).unwrap();
        assert_eq!(v["max"], 30.0);
        assert_eq!(v["min_temp"], 150.0);

        let cmd = DeviceCommand::Feedforward { value: Some(62.04) };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/feedforward");
        assert_eq!(cmd.payload(), "62.0");
        assert_eq!(DeviceCommand::Feedforward { value: None }.payload(), "off");
        let cmd = DeviceCommand::Lease { ms: 10_000 };
        assert_eq!(cmd.topic("r1"), "roaster/r1/control/lease");
        assert_eq!(cmd.payload(), "10000");
    }

    #[test]
    fn control_mode_parses_firmware_spellings() {
        assert_eq!("auto".parse::<ControlMode>().unwrap(), ControlMode::Auto);
        assert_eq!("1".parse::<ControlMode>().unwrap(), ControlMode::Auto);
        assert_eq!(
            "MANUAL".parse::<ControlMode>().unwrap(),
            ControlMode::Manual
        );
        assert!("pid".parse::<ControlMode>().is_err());
    }
}
