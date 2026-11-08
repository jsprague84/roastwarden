//! What each roaster can do, so the replay engine can place the BT loop
//! (`docs/control.md`, "Device contract": these are its optional capabilities).
//!
//! Firmware 3.1+ lists its capabilities in the retained status message
//! (`"caps": ["pid", "ff", "lease", "tpc"]`). Devices that do not (older
//! firmware, TC4, Modbus) are inferred from their telemetry: a device that
//! reports a setpoint has an onboard PID, and nothing else is assumed.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct DeviceCaps {
    /// Onboard PID follows `setpoint` in auto mode.
    pub pid: bool,
    /// Onboard PID accepts a feedforward heater %.
    pub ff: bool,
    /// Control lease with a fail-safe on expiry.
    pub lease: bool,
    /// Heater % is time-proportioned power.
    pub tpc: bool,
    /// An onboard RoR limiter (`control/ror_limit`; firmware 3.2 to 3.4.0).
    pub ror: bool,
    /// `true` when the device said so; `false` when inferred.
    pub reported: bool,
}

impl DeviceCaps {
    /// Capabilities from a status message, if it carries a `caps` list.
    pub fn from_status(status: &serde_json::Value) -> Option<DeviceCaps> {
        let list = status.get("caps")?.as_array()?;
        let has = |name: &str| list.iter().any(|v| v.as_str() == Some(name));
        Some(DeviceCaps {
            pid: has("pid"),
            ff: has("ff"),
            lease: has("lease"),
            tpc: has("tpc"),
            ror: has("ror"),
            reported: true,
        })
    }

    /// Best guess for a device that does not report capabilities.
    pub fn inferred(reports_setpoint: bool) -> DeviceCaps {
        DeviceCaps {
            pid: reports_setpoint,
            ..DeviceCaps::default()
        }
    }
}

#[derive(Default)]
pub struct CapsRegistry {
    inner: RwLock<HashMap<String, DeviceCaps>>,
}

impl CapsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or clear) a device's capabilities from its status message.
    pub fn update_from_status(&self, device_id: &str, status: &serde_json::Value) {
        let mut map = self
            .inner
            .write()
            .expect("the capabilities lock is poisoned");
        match DeviceCaps::from_status(status) {
            Some(c) => {
                map.insert(device_id.to_string(), c);
            }
            // An "online" status without caps means older firmware took over.
            None if status.get("status").and_then(|s| s.as_str()) == Some("online") => {
                map.remove(device_id);
            }
            None => {}
        }
    }

    pub fn reported(&self, device_id: &str) -> Option<DeviceCaps> {
        self.inner
            .read()
            .expect("the capabilities lock is poisoned")
            .get(device_id)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn caps_come_from_status_or_are_inferred() {
        let c = DeviceCaps::from_status(&json!({"status":"online","caps":["pid","ff","lease"]}))
            .unwrap();
        assert!(c.pid && c.ff && c.lease && !c.tpc && !c.ror && c.reported);
        assert!(DeviceCaps::from_status(&json!({"status":"online"})).is_none());
        assert_eq!(
            DeviceCaps::inferred(true),
            DeviceCaps {
                pid: true,
                ..Default::default()
            }
        );

        let reg = CapsRegistry::new();
        reg.update_from_status("r1", &json!({"status":"online","caps":["pid","ff"]}));
        assert!(reg.reported("r1").unwrap().ff);
        // An offline last-will keeps what we knew; an online status without caps clears it.
        reg.update_from_status("r1", &json!({"status":"offline"}));
        assert!(reg.reported("r1").is_some());
        reg.update_from_status("r1", &json!({"status":"online","version":"3.0.0"}));
        assert!(reg.reported("r1").is_none());
    }
}
