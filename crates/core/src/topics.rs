// Topic layout helpers and constants matching ESP32 firmware

pub const ROOT: &str = "roaster";

// Device-scoped topics
pub fn telemetry_topic(device_id: &str) -> String {
    format!("{ROOT}/{device_id}/telemetry")
}

pub fn status_topic(device_id: &str) -> String {
    format!("{ROOT}/{device_id}/status")
}

pub fn control_root(device_id: &str) -> String {
    format!("{ROOT}/{device_id}/control")
}

pub fn control_setpoint(device_id: &str) -> String {
    format!("{}/setpoint", control_root(device_id))
}
pub fn control_fan(device_id: &str) -> String {
    format!("{}/fan", control_root(device_id))
}
pub fn control_heater(device_id: &str) -> String {
    format!("{}/heater", control_root(device_id))
}
pub fn control_drum(device_id: &str) -> String {
    format!("{}/drum", control_root(device_id))
}
pub fn control_damper(device_id: &str) -> String {
    format!("{}/damper", control_root(device_id))
}
pub fn control_mode(device_id: &str) -> String {
    format!("{}/mode", control_root(device_id))
}
pub fn control_heater_enable(device_id: &str) -> String {
    format!("{}/heater_enable", control_root(device_id))
}
pub fn control_pid(device_id: &str) -> String {
    format!("{}/pid", control_root(device_id))
}
pub fn control_emergency_stop(device_id: &str) -> String {
    format!("{}/emergency_stop", control_root(device_id))
}
pub fn control_feedforward(device_id: &str) -> String {
    format!("{}/feedforward", control_root(device_id))
}
pub fn control_lease(device_id: &str) -> String {
    format!("{}/lease", control_root(device_id))
}

pub fn control_ror_limit(device_id: &str) -> String {
    format!("{}/ror_limit", control_root(device_id))
}

// Wildcards
pub fn telemetry_wildcard_all() -> &'static str {
    "roaster/+/telemetry"
}
pub fn status_wildcard_all() -> &'static str {
    "roaster/+/status"
}
pub fn control_wildcard(device_id: &str) -> String {
    format!("{}/#", control_root(device_id))
}
pub fn autotune_wildcard_all() -> &'static str {
    "roaster/+/autotune/#"
}
