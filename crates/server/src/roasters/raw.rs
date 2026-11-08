//! The newest raw message from each roaster's connection, as received before any mapping
//! (`docs/add-roaster.md`, "The newest raw message"): the JSON text of an MQTT message or a
//! WebSocket frame, a TC4 reply line, or a Modbus poll's registers decoded by type with their
//! addresses. `roast roaster map --try` and the dashboard's Mapping editor show it, and the
//! ingest preview maps it. In memory only, at most 4 KB a message.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use utoipa::ToSchema;

/// The most of a message kept, bytes.
pub const MAX_BYTES: usize = 4096;
/// The most messages kept per roaster (one per connection and topic).
const MAX_PER_ROASTER: usize = 16;

/// One message as the roaster sent it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RawMessage {
    /// The connection it came on; null for the inbound device socket or a roaster heard on
    /// MQTT that has no connection saved
    pub connection_id: Option<String>,
    /// `mqtt`, `websocket`, `serial_tc4`, `modbus_tcp` or `modbus_rtu`
    pub protocol: String,
    /// The MQTT topic it came on
    pub topic: Option<String>,
    /// When it arrived (RFC 3339)
    pub received_at: String,
    /// The message as text: JSON, the TC4 line, or (Modbus) a JSON list of
    /// `{kind, address, name, type, value}`; cut at 4096 bytes
    pub payload: String,
    /// The message was longer than 4096 bytes
    pub truncated: bool,
}

/// The newest message per roaster, connection and topic.
#[derive(Default)]
pub struct RawStore {
    by_roaster: Mutex<HashMap<String, Vec<RawMessage>>>,
}

/// `s` cut to at most `max` bytes at a character boundary.
fn cut(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], true)
}

impl RawStore {
    /// Keep `payload` as the newest message of the roaster's connection (and topic).
    pub fn record(
        &self,
        device_id: &str,
        connection_id: Option<&str>,
        protocol: &str,
        topic: Option<&str>,
        payload: &[u8],
    ) {
        let text = String::from_utf8_lossy(payload);
        let (text, truncated) = cut(text.trim_end(), MAX_BYTES);
        let msg = RawMessage {
            connection_id: connection_id.map(str::to_string),
            protocol: protocol.to_string(),
            topic: topic.map(str::to_string),
            received_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            payload: text.to_string(),
            truncated,
        };
        let mut all = self
            .by_roaster
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let list = all.entry(device_id.to_string()).or_default();
        list.retain(|m| !(m.connection_id == msg.connection_id && m.topic == msg.topic));
        list.insert(0, msg);
        list.truncate(MAX_PER_ROASTER);
    }

    /// The roaster's newest messages, newest first.
    pub fn list(&self, device_id: &str) -> Vec<RawMessage> {
        self.by_roaster
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(device_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_per_connection_and_topic_is_kept_and_cut_at_4_kb() {
        let s = RawStore::default();
        s.record("r", Some("c1"), "mqtt", Some("a"), b"{\"x\":1}");
        s.record("r", Some("c1"), "mqtt", Some("b"), b"2");
        s.record("r", Some("c1"), "mqtt", Some("a"), b"{\"x\":3}\n");
        let l = s.list("r");
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].topic.as_deref(), Some("a"));
        assert_eq!(l[0].payload, "{\"x\":3}");
        assert!(s.list("other").is_empty());

        let long = "é".repeat(3000); // 6000 bytes
        s.record("r", None, "websocket", None, long.as_bytes());
        let m = &s.list("r")[0];
        assert!(m.truncated && m.payload.len() <= MAX_BYTES && m.payload.len() > 4090);
        for i in 0..40 {
            s.record("r", Some("c1"), "mqtt", Some(&format!("t{i}")), b"1");
        }
        assert_eq!(s.list("r").len(), MAX_PER_ROASTER);
    }
}
