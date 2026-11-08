use hostname::get as get_hostname;
use std::env;

/// Retained presence message published on connect, with a matching last will
/// so subscribers (firmware, dashboards, Home Assistant) can tell whether the
/// control plane is alive and not just the broker.
#[derive(Debug, Clone)]
pub struct StatusAnnouncement {
    pub topic: String,
    pub online: Vec<u8>,
    pub offline: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub keep_alive_secs: u16,
    pub clean_session: bool,
    /// Connect with TLS. Requires `ca_cert_path` (PEM; a system bundle works for public CAs).
    pub tls: bool,
    pub ca_cert_path: Option<String>,
    pub status: Option<StatusAnnouncement>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        let host = "localhost".to_string();
        let port = 1883;
        let client_id = default_client_id();
        let keep_alive_secs = 30;
        Self {
            host,
            port,
            client_id,
            username: None,
            password: None,
            keep_alive_secs,
            clean_session: true,
            tls: false,
            ca_cert_path: None,
            status: None,
        }
    }
}

impl MqttConfig {
    pub fn from_env() -> Self {
        let mut cfg = MqttConfig::default();

        if let Ok(v) = env::var("MQTT_BROKER_HOST") {
            if !v.is_empty() {
                cfg.host = v;
            }
        }
        if let Ok(v) = env::var("MQTT_BROKER_PORT") {
            if let Ok(p) = v.parse::<u16>() {
                cfg.port = p;
            }
        }
        if let Ok(v) = env::var("MQTT_CLIENT_ID") {
            if !v.is_empty() {
                cfg.client_id = v;
            }
        }
        if let Ok(v) = env::var("MQTT_USERNAME") {
            if !v.is_empty() {
                cfg.username = Some(v);
            }
        }
        if let Ok(v) = env::var("MQTT_PASSWORD") {
            if !v.is_empty() {
                cfg.password = Some(v);
            }
        }
        if let Ok(v) = env::var("MQTT_KEEP_ALIVE_SECS") {
            if let Ok(s) = v.parse::<u16>() {
                cfg.keep_alive_secs = s;
            }
        }
        if let Ok(v) = env::var("MQTT_TLS") {
            cfg.tls = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Ok(v) = env::var("MQTT_CA_CERT") {
            if !v.is_empty() {
                cfg.ca_cert_path = Some(v);
            }
        }

        cfg
    }
}

fn default_client_id() -> String {
    let host = get_hostname()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string());
    let pid = std::process::id();
    format!("roastwarden-{host}-{pid}")
}
