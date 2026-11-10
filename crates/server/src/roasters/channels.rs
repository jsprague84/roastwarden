//! Declared channels (`docs/channels.md`): the extra values a roaster sends
//! (a TC4's `t3`, an exhaust probe, burner pressure) that the owner wants kept with a roast,
//! drawn on the scope, usable in alarms and exported to Artisan. Each roaster's list lives in
//! its setup (`roaster_setup.channels`, migration 048); this module is the channel's shape,
//! its checks and the helpers that read a channel's values out of a payload or a stored roast.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

/// At most this many channels per roaster.
pub const MAX_CHANNELS: usize = 8;

/// The sample fields that are not channels: the typed fields (`roastwarden_core::Sample`), the
/// server's own and the drum and damper outputs (stored with the heater and fan when
/// declared).
const RESERVED: &[&str] = &[
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
    "drum",
    "damper",
];

/// One declared channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Channel {
    /// The sample's field name (`t3`, `exhaust`, `burner_kpa`)
    pub key: String,
    /// What the dashboard, alarms and Artisan call it, 1–24 characters
    pub label: String,
    /// `C` (a temperature: °C, drawn on the temperature axis), `%` (the output axis), or
    /// another short unit (≤ 8 characters: its own scale)
    pub unit: String,
    /// Top of the scale for a channel in another unit; null: the highest value seen
    #[serde(default)]
    pub max: Option<f64>,
}

impl Channel {
    /// A temperature, °C.
    pub fn is_temp(&self) -> bool {
        self.unit == "C"
    }

    pub fn is_pct(&self) -> bool {
        self.unit == "%"
    }

    /// The unit as text follows a value: ` °C`, ` %`, ` kPa`.
    pub fn unit_suffix(&self) -> String {
        if self.is_temp() {
            " °C".into()
        } else {
            format!(" {}", self.unit)
        }
    }
}

fn key_ok(k: &str) -> bool {
    let mut c = k.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic())
        && k.len() <= 32
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Check a roaster's channels and tidy them (labels trimmed, `°C` read as `C`, a `max` only
/// where a scale of its own uses it): the setup `PUT`'s 400.
/// Whether `key` is a sample field of its own, not a channel.
pub(crate) fn is_reserved(key: &str) -> bool {
    RESERVED.contains(&key)
}

pub fn validate(channels: &mut [Channel]) -> Result<(), String> {
    if channels.len() > MAX_CHANNELS {
        return Err(format!("at most {MAX_CHANNELS} channels"));
    }
    for (i, ch) in channels.iter_mut().enumerate() {
        let n = i + 1;
        ch.key = ch.key.trim().to_string();
        if !key_ok(&ch.key) {
            return Err(format!(
                "channel {n}: key '{}' must be a letter then letters, digits or _ (32 at most)",
                ch.key
            ));
        }
        if RESERVED.contains(&ch.key.as_str()) {
            return Err(format!(
                "channel {n}: '{}' is a field of its own, not a channel",
                ch.key
            ));
        }
        ch.label = ch.label.trim().to_string();
        if !(1..=24).contains(&ch.label.chars().count()) {
            return Err(format!("channel {n}: label must be 1..24 characters"));
        }
        let unit = ch.unit.trim();
        ch.unit = match unit {
            "C" | "c" | "°C" => "C".into(),
            "F" | "f" | "°F" => {
                return Err(format!(
                "channel {n}: temperatures are kept in °C; use C (an ingest mapping converts °F)"
            ))
            }
            u => u.to_string(),
        };
        if !(1..=8).contains(&ch.unit.chars().count()) {
            return Err(format!("channel {n}: unit must be C, % or 1..8 characters"));
        }
        if ch.is_temp() || ch.is_pct() {
            ch.max = None;
        }
        if let Some(m) = ch.max
            && !(m.is_finite() && m > 0.0) {
                return Err(format!("channel {n}: max must be a number above 0"));
            }
    }
    for (i, a) in channels.iter().enumerate() {
        if channels[..i].iter().any(|b| b.key == a.key) {
            return Err(format!("channel key '{}' is declared twice", a.key));
        }
    }
    Ok(())
}

/// A channel's value in a telemetry payload (the JSON `process_telemetry` gets).
pub fn value_in(payload: &serde_json::Value, key: &str) -> Option<f64> {
    payload.get(key)?.as_f64().filter(|v| v.is_finite())
}

/// A channel's value in a stored sample's `extras`.
pub fn stored(extras: Option<&serde_json::Value>, key: &str) -> Option<f64> {
    extras.and_then(|e| value_in(e, key))
}

/// A roaster's declared channels, from the table (reports and exports; the hot path uses the
/// in-memory setups). None for a roaster with no row, NULL or an unreadable list.
pub async fn for_device(db: &SqlitePool, device_id: &str) -> Vec<Channel> {
    let raw: Option<Option<String>> =
        sqlx::query_scalar("SELECT channels FROM roaster_setup WHERE device_id = ?")
            .bind(device_id)
            .fetch_optional(db)
            .await
            .unwrap_or(None);
    parse(raw.flatten().as_deref())
}

/// A roast's channels: the list it kept when it started (migration 049), else (a roast from
/// before, or one not started yet) its roaster's list now.
pub async fn for_roast(db: &SqlitePool, session_id: &str, device_id: &str) -> Vec<Channel> {
    let own: Option<Option<String>> =
        sqlx::query_scalar("SELECT channels FROM roast_sessions WHERE id = ?")
            .bind(session_id)
            .fetch_optional(db)
            .await
            .unwrap_or(None);
    match own.flatten() {
        Some(list) => parse(Some(&list)),
        None => for_device(db, device_id).await,
    }
}

/// The stored list: NULL, empty or unreadable is none.
pub fn parse(raw: Option<&str>) -> Vec<Channel> {
    raw.and_then(|s| serde_json::from_str::<Vec<Channel>>(s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(key: &str, label: &str, unit: &str) -> Channel {
        Channel {
            key: key.into(),
            label: label.into(),
            unit: unit.into(),
            max: None,
        }
    }

    #[test]
    fn channels_validate() {
        let mut ok = vec![
            ch("t3", " Exhaust ", "°C"),
            Channel {
                max: Some(4.0),
                ..ch("burner_kpa", "Gas", "kPa")
            },
            Channel {
                max: Some(50.0),
                ..ch("heaterApplied", "Applied", "%")
            },
        ];
        validate(&mut ok).unwrap();
        assert_eq!(
            (ok[0].label.as_str(), ok[0].unit.as_str()),
            ("Exhaust", "C")
        );
        assert_eq!(ok[1].max, Some(4.0));
        assert_eq!(ok[2].max, None, "a % channel is on the output axis");

        for bad in [
            vec![ch("3t", "x", "C")],
            vec![ch("t-3", "x", "C")],
            vec![ch(&"a".repeat(33), "x", "C")],
            vec![ch("beanTemp", "BT", "C")],
            vec![ch("fan", "Fan", "%")],
            vec![ch("drum", "Drum", "%")],
            vec![ch("t3", "", "C")],
            vec![ch("t3", &"x".repeat(25), "C")],
            vec![ch("t3", "x", "")],
            vec![ch("t3", "x", "F")],
            vec![ch("t3", "x", "ninechars")],
            vec![ch("t3", "a", "C"), ch("t3", "b", "C")],
            vec![Channel {
                max: Some(0.0),
                ..ch("p", "Gas", "kPa")
            }],
            (0..9).map(|i| ch(&format!("t{i}"), "x", "C")).collect(),
        ] {
            let mut b = bad.clone();
            assert!(validate(&mut b).is_err(), "{bad:?} passed");
        }
    }

    #[test]
    fn values_are_read_from_a_payload_and_a_stored_sample() {
        let p = serde_json::json!({ "t3": 151.5, "note": "x", "bad": null });
        assert_eq!(value_in(&p, "t3"), Some(151.5));
        assert_eq!(value_in(&p, "note"), None);
        assert_eq!(value_in(&p, "missing"), None);
        assert_eq!(stored(Some(&p), "t3"), Some(151.5));
        assert_eq!(stored(None, "t3"), None);
        assert_eq!(parse(None), [] as [crate::roasters::channels::Channel; 0]);
        assert_eq!(
            parse(Some("not json")),
            [] as [crate::roasters::channels::Channel; 0]
        );
        assert_eq!(
            parse(Some(r#"[{"key":"t3","label":"Exhaust","unit":"C"}]"#)),
            vec![ch("t3", "Exhaust", "C")]
        );
    }

    /// Each roast keeps its channel list (migration 049): a started roast reads the list of
    /// its start, also after the roaster's changes; one declared with none keeps none; one
    /// from before (NULL) and one not started read the roaster's list now. The roast's API,
    /// its report data and the Artisan export all read it.
    #[tokio::test]
    async fn a_roast_keeps_the_channels_it_started_with() {
        use crate::test_support::Harness;
        use serde_json::json;

        let h = Harness::new().await;
        let setup = |channels: serde_json::Value| {
            let h = &h;
            async move {
                let r = h
                    .owner(
                        "PUT",
                        "/api/roaster/tc4/setup",
                        Some(json!({ "kind": "logger", "control": "off", "channels": channels })),
                    )
                    .await;
                assert!(r.status.is_success(), "{:?}", r.json());
            }
        };
        let db = &h.state.db;
        for id in ["none", "old", "planned", "kept"] {
            sqlx::query(
                "INSERT INTO roast_sessions (id, name, device_id, status, created_at)
                 VALUES (?, ?, 'tc4', 'planning', CURRENT_TIMESTAMP)",
            )
            .bind(id)
            .bind(id)
            .execute(db)
            .await
            .unwrap();
        }
        let db = &h.state.db;
        // Started with none declared: it keeps none
        crate::roasts::sessions::start_session(db, "none")
            .await
            .unwrap()
            .unwrap();
        setup(json!([{ "key": "t3", "label": "Exhaust", "unit": "C" }])).await;
        crate::roasts::sessions::start_session(db, "kept")
            .await
            .unwrap()
            .unwrap();
        crate::roasts::sessions::start_session(db, "old")
            .await
            .unwrap()
            .unwrap();
        // A roast from before migration 049
        sqlx::query("UPDATE roast_sessions SET channels = NULL WHERE id = 'old'")
            .execute(db)
            .await
            .unwrap();
        // The roaster's list changes afterwards
        setup(json!([{ "key": "t4", "label": "Inlet", "unit": "C" }])).await;

        let h = &h;
        let keys = |id: &'static str| async move {
            let r = h.owner("GET", &format!("/api/sessions/{id}"), None).await;
            r.json()["channels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["key"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys("kept").await, ["t3"]);
        assert_eq!(keys("none").await, [] as [std::string::String; 0]);
        assert_eq!(
            keys("old").await,
            ["t4"],
            "from before: the roaster's list now"
        );
        assert_eq!(
            keys("planned").await,
            ["t4"],
            "not started: the roaster's list now"
        );
        // The report's (and `roast review`'s) data and the Artisan export read the same list
        let input = crate::reports::roast::gather(db, "kept").await.unwrap();
        assert_eq!(input.channels[0].key, "t3");
        let (alog, _) = crate::roasts::export::export_alog(db, "kept")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(alog["extraname1"], json!(["Exhaust"]));
    }
}
