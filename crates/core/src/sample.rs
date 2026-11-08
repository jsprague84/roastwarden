//! Typed telemetry sample: the unit of the v2 telemetry stream.
//!
//! Every transport (MQTT, WebSocket, Modbus, serial) ends up producing one
//! [`Sample`] per reading. The server assigns a per-device, monotonically
//! increasing `seq`, keeps the recent history in memory, and streams
//! samples to dashboards as CBOR batches. A client that reconnects sends
//! the last `seq` it saw and receives exactly the samples it missed.
//!
//! Field names are short on purpose: at 1 Hz per device the overhead is
//! irrelevant, but a 2 h catch-up is 7200 samples and the names dominate the
//! frame size. All temperatures are Celsius; rates are °C/min.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One telemetry sample in the server's compact form (docs/websocket-v2.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Sample {
    /// Per-device sequence number (1-based, monotonically increasing).
    #[serde(rename = "q")]
    pub seq: u64,
    /// Server receive time, Unix milliseconds.
    #[serde(rename = "t")]
    pub ts_ms: u64,
    /// Bean temperature, °C.
    #[serde(rename = "bt", skip_serializing_if = "Option::is_none", default)]
    pub bt: Option<f64>,
    /// Environment / exhaust temperature, °C.
    #[serde(rename = "et", skip_serializing_if = "Option::is_none", default)]
    pub et: Option<f64>,
    /// Canonical server rate of rise of BT, °C/min.
    #[serde(rename = "rb", skip_serializing_if = "Option::is_none", default)]
    pub ror_bt: Option<f64>,
    /// Canonical server rate of rise of ET, °C/min.
    #[serde(rename = "re", skip_serializing_if = "Option::is_none", default)]
    pub ror_et: Option<f64>,
    /// The device's own RoR estimate, if it sent one.
    #[serde(rename = "rd", skip_serializing_if = "Option::is_none", default)]
    pub ror_device: Option<f64>,
    /// Heater command, 0..=100 %.
    #[serde(rename = "h", skip_serializing_if = "Option::is_none", default)]
    pub heater: Option<f64>,
    /// Fan, 0..=100 %.
    #[serde(rename = "f", skip_serializing_if = "Option::is_none", default)]
    pub fan: Option<f64>,
    /// PID setpoint, °C.
    #[serde(rename = "sp", skip_serializing_if = "Option::is_none", default)]
    pub setpoint: Option<f64>,
    /// 0 = manual, 1 = auto (PID).
    #[serde(rename = "m", skip_serializing_if = "Option::is_none", default)]
    pub mode: Option<u8>,
    /// Heater enable flag.
    #[serde(rename = "he", skip_serializing_if = "Option::is_none", default)]
    pub heater_enable: Option<bool>,
    /// Anything else numeric the device reported (ambientTemp, rssi, freeHeap…),
    /// keyed by the original field name.
    #[serde(rename = "x", skip_serializing_if = "BTreeMap::is_empty", default)]
    pub extras: BTreeMap<String, f64>,
}

/// JSON keys that map to typed fields; everything else numeric goes to `extras`.
const TYPED_KEYS: &[&str] = &[
    "beanTemp",
    "envTemp",
    "rateOfRise",
    "rateOfRiseEt",
    "deviceRateOfRise",
    "heater",
    "fan",
    "setpoint",
    "controlMode",
    "heaterEnable",
    "timestamp",
    "uptime",
    "uptimeMs",
];

fn finite(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

impl Sample {
    /// Build a sample from the enriched camelCase telemetry JSON the server
    /// already produces for v1 clients.
    pub fn from_telemetry_json(seq: u64, ts_ms: u64, v: &serde_json::Value) -> Sample {
        let obj = v.as_object();
        let num = |k: &str| {
            obj.and_then(|o| o.get(k))
                .and_then(serde_json::Value::as_f64)
        };
        let mut extras = BTreeMap::new();
        if let Some(o) = obj {
            for (k, val) in o {
                if TYPED_KEYS.contains(&k.as_str()) {
                    continue;
                }
                if let Some(n) = val.as_f64().filter(|n| n.is_finite()) {
                    extras.insert(k.clone(), n);
                }
            }
        }
        Sample {
            seq,
            ts_ms,
            bt: finite(num("beanTemp")),
            et: finite(num("envTemp")),
            ror_bt: finite(num("rateOfRise")),
            ror_et: finite(num("rateOfRiseEt")),
            ror_device: finite(num("deviceRateOfRise")),
            heater: finite(num("heater")).map(|h| h.clamp(0.0, 100.0)),
            fan: finite(num("fan")).map(|f| f.clamp(0.0, 100.0)),
            setpoint: finite(num("setpoint")),
            mode: num("controlMode").map(|m| u8::from(m != 0.0)),
            heater_enable: num("heaterEnable").map(|h| h != 0.0),
            extras,
        }
    }

    /// Convert back to the v1 camelCase telemetry object (what the dashboard
    /// `Telemetry` type expects). Extras are spread back as top-level keys.
    pub fn to_telemetry_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        let mut put = |k: &str, v: Option<serde_json::Value>| {
            if let Some(v) = v {
                m.insert(k.to_string(), v);
            }
        };
        put("beanTemp", self.bt.map(serde_json::Value::from));
        put("envTemp", self.et.map(serde_json::Value::from));
        put("rateOfRise", self.ror_bt.map(serde_json::Value::from));
        put("rateOfRiseEt", self.ror_et.map(serde_json::Value::from));
        put(
            "deviceRateOfRise",
            self.ror_device.map(serde_json::Value::from),
        );
        put("heater", self.heater.map(serde_json::Value::from));
        put("fan", self.fan.map(serde_json::Value::from));
        put("setpoint", self.setpoint.map(serde_json::Value::from));
        put("controlMode", self.mode.map(serde_json::Value::from));
        put(
            "heaterEnable",
            self.heater_enable
                .map(|b| serde_json::Value::from(u8::from(b))),
        );
        for (k, v) in &self.extras {
            m.insert(k.clone(), serde_json::Value::from(*v));
        }
        m.insert(
            "timestamp".into(),
            serde_json::Value::from(self.ts_ms / 1000),
        );
        serde_json::Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_v1_json_both_ways() {
        let v = json!({
            "beanTemp": 185.25, "envTemp": 210.0, "rateOfRise": 8.5, "deviceRateOfRise": 9.0,
            "heater": 55.5, "fan": 78.4, "setpoint": 190.0, "controlMode": 1, "heaterEnable": 1,
            "rssi": -70, "freeHeap": 190_000, "uptime": 1234, "name": "ignored"
        });
        let s = Sample::from_telemetry_json(7, 1_700_000_000_123, &v);
        assert_eq!(s.seq, 7);
        assert_eq!(s.bt, Some(185.25));
        assert_eq!(s.et, Some(210.0));
        assert_eq!(s.ror_bt, Some(8.5));
        assert_eq!(s.ror_device, Some(9.0));
        assert_eq!(s.heater, Some(55.5));
        assert_eq!(s.fan, Some(78.4));
        assert_eq!(s.mode, Some(1));
        assert_eq!(s.heater_enable, Some(true));
        assert_eq!(s.extras.get("rssi"), Some(&-70.0));
        assert!(!s.extras.contains_key("uptime"));
        assert!(!s.extras.contains_key("name"));

        let back = s.to_telemetry_json();
        assert_eq!(back["beanTemp"], 185.25);
        assert_eq!(back["heater"], 55.5);
        assert_eq!(back["fan"], 78.4);
        assert_eq!(back["heaterEnable"], 1);
        assert_eq!(back["rssi"], -70.0);
        assert_eq!(back["timestamp"], 1_700_000_000u64);
    }

    #[test]
    fn missing_and_nonfinite_fields_are_omitted() {
        let v = json!({"beanTemp": f64::NAN, "envTemp": 20.0});
        let s = Sample::from_telemetry_json(1, 0, &v);
        assert_eq!(s.bt, None);
        assert_eq!(s.et, Some(20.0));
        let text = serde_json::to_string(&s).unwrap();
        assert!(!text.contains("\"bt\""));
        assert!(text.contains("\"et\":20.0"));
    }

    #[test]
    fn compact_serde_names_round_trip() {
        let s = Sample {
            seq: 3,
            ts_ms: 42,
            bt: Some(100.0),
            heater: Some(10.5),
            ..Default::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(text, r#"{"q":3,"t":42,"bt":100.0,"h":10.5}"#);
        let back: Sample = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);
    }
}
