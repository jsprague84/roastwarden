//! mDNS / DNS-SD discovery of roasters on the LAN.
//!
//! MQTT devices announce themselves by publishing; everything else (serial,
//! WebSocket, Modbus) had to be typed in by hand. The server now browses two
//! service types and keeps a table the device wizard can offer with one click:
//!
//! - `_roastwarden._tcp` — advertised by Roastwarden-aware firmware. TXT keys:
//!   `device_id`, `proto` (`mqtt`, `websocket`, `modbus_tcp`, `serial_tc4`),
//!   `version`. (Before 0.26.0 the server also browsed `_rustroast._tcp`, the name before
//!   the rename; firmware older than 3.7.0 needs updating to be found.)
//! - `_modbus._tcp` — generic Modbus TCP devices and gateways.
//! - `_phidget22server._tcp` — a Phidget22 network server, a Phidgets temperature board
//!   plugged into it (docs/phidgets.md).
//!
//! Multicast does not cross Docker's default bridge network; run the container
//! with `network_mode: host` (or on the roaster's LAN) for discovery to work.
//! Discovery is advisory only: nothing is created without the user confirming
//! it in the wizard.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::Serialize;
use tokio::sync::RwLock;

const SERVICE_TYPES: &[&str] = &[
    "_roastwarden._tcp.local.",
    "_modbus._tcp.local.",
    "_phidget22server._tcp.local.",
];
/// Forget a service this long after its last announcement without a goodbye.
const STALE_AFTER: Duration = Duration::from_mins(10);

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DiscoveredService {
    /// Instance name without the service suffix, e.g. `esp32_roaster_01`.
    pub name: String,
    pub service_type: String,
    pub host: String,
    pub addresses: Vec<String>,
    pub port: u16,
    pub txt: HashMap<String, String>,
    /// Best guess at the Roastwarden device id (TXT `device_id`, else the instance name).
    pub device_id: String,
    /// Suggested Roastwarden protocol for this service.
    pub protocol: String,
    pub last_seen: DateTime<Utc>,
    #[serde(skip)]
    seen_at: Instant,
}

#[derive(Default)]
pub struct DiscoveryService {
    services: RwLock<HashMap<String, DiscoveredService>>,
    enabled: std::sync::atomic::AtomicBool,
}

impl DiscoveryService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn list(&self) -> Vec<DiscoveredService> {
        let now = Instant::now();
        let mut out: Vec<DiscoveredService> = self
            .services
            .read()
            .await
            .values()
            .filter(|s| now.duration_since(s.seen_at) < STALE_AFTER)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Browse forever. Errors (no multicast interface, container without host
    /// networking) are logged once and discovery stays disabled.
    #[expect(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "spawns a blocking task per service type: async so callers are inside the Tokio runtime"
    )]
    pub async fn run(self: Arc<Self>) {
        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "mDNS discovery unavailable");
                return;
            }
        };
        let mut receivers = Vec::new();
        for ty in SERVICE_TYPES {
            match daemon.browse(ty) {
                Ok(rx) => receivers.push((ty.to_string(), rx)),
                Err(e) => tracing::warn!(service = ty, error = %e, "mDNS browse failed"),
            }
        }
        if receivers.is_empty() {
            return;
        }
        self.enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(types = ?SERVICE_TYPES, "mDNS discovery started");

        // mdns-sd receivers are flume channels; poll them from a blocking thread per type.
        for (ty, rx) in receivers {
            let me = self.clone();
            tokio::task::spawn_blocking(move || {
                while let Ok(event) = rx.recv() {
                    let me = me.clone();
                    let ty = ty.clone();
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let txt: HashMap<String, String> = info
                                .get_properties()
                                .iter()
                                .map(|p| (p.key().to_string(), p.val_str().to_string()))
                                .collect();
                            let name = info
                                .get_fullname()
                                .strip_suffix(&format!(".{ty}"))
                                .unwrap_or(info.get_fullname())
                                .to_string();
                            let device_id = txt.get("device_id").cloned().unwrap_or_else(|| {
                                // A Phidget22 server's name is the computer's
                                // ("Phidget22 Server on sbc"): an id from it.
                                if ty.starts_with("_phidget22server") {
                                    slug(&name)
                                } else {
                                    name.clone()
                                }
                            });
                            let protocol = txt.get("proto").cloned().unwrap_or_else(|| {
                                if ty.starts_with("_modbus") {
                                    "modbus_tcp".into()
                                } else if ty.starts_with("_phidget22server") {
                                    "phidget22".into()
                                } else {
                                    "mqtt".into()
                                }
                            });
                            let svc = DiscoveredService {
                                name: name.clone(),
                                service_type: ty.trim_end_matches(".local.").to_string(),
                                host: info.get_hostname().trim_end_matches('.').to_string(),
                                addresses: info
                                    .get_addresses()
                                    .iter()
                                    .map(std::string::ToString::to_string)
                                    .collect(),
                                port: info.get_port(),
                                txt,
                                device_id,
                                protocol,
                                last_seen: Utc::now(),
                                seen_at: Instant::now(),
                            };
                            tracing::info!(name = %svc.name, service = %svc.service_type, addrs = ?svc.addresses, port = svc.port, "mDNS service resolved");
                            let key = info.get_fullname().to_string();
                            tokio::runtime::Handle::current().block_on(async move {
                                me.services.write().await.insert(key, svc);
                            });
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            tracing::info!(name = %fullname, "mDNS service removed");
                            tokio::runtime::Handle::current().block_on(async move {
                                me.services.write().await.remove(&fullname);
                            });
                        }
                        _ => {}
                    }
                }
            });
        }
    }
}

/// A roaster id from a service's name: lowercase letters, digits and `_`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_');
    if out.is_empty() {
        "phidget".into()
    } else {
        out.chars().take(48).collect()
    }
}
