//! The MQTT consumer: each message to its device's mapping or the roaster/ topics, telemetry
//! processed, a roaster's status kept, unknown roasters added, the metrics counted.

use std::collections::HashMap;
use std::sync::Arc;

use roastwarden_mqtt::MqttService;
use tokio::sync::{broadcast, RwLock};

use sqlx::SqlitePool;

use crate::analysis::telemetry::TelemetryService;
use crate::app::state::{DeviceInfo, Metrics};
use crate::roasters::caps;
use crate::roasters::types::{
    CreateConnectionRequest, CreateDeviceRequest, DeviceStatus, Protocol,
};
use crate::transports::ingest;

pub(crate) fn parse_roaster_topic(topic: &str) -> Option<(String, String)> {
    // Expect: roaster/{device_id}/<kind>
    let mut parts = topic.split('/');
    let root = parts.next()?;
    if root != roastwarden_core::ROOT {
        return None;
    }
    let device = parts.next()?.to_string();
    let kind = parts.next()?.to_string();
    Some((device, kind))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn mqtt_consumer_loop(
    mqtt: MqttService,
    telemetry_service: TelemetryService,
    device_registry: Arc<RwLock<HashMap<String, DeviceInfo>>>,
    metrics: Arc<Metrics>,
    db: SqlitePool,
    caps: Arc<caps::CapsRegistry>,
    mqtt_routes: Arc<ingest::MqttRouter>,
) {
    let mut rx = mqtt.events();
    // The client connects before this loop subscribes, so the first Connected is never seen
    // here: start from its state (/metrics read 0 while connected).
    metrics.mqtt_connected.set(i64::from(mqtt.is_ready()));

    loop {
        match rx.recv().await {
            Ok(roastwarden_mqtt::MqttEvent::Connected) => metrics.mqtt_connected.set(1),
            Ok(roastwarden_mqtt::MqttEvent::Disconnected) => metrics.mqtt_connected.set(0),
            Ok(roastwarden_mqtt::MqttEvent::Publish { topic, payload }) => {
                metrics.mqtt_rx_total.inc();
                // A topic in the route table is handled by its device's mapping alone
                // (docs/ingest-mapping.md, "MQTT"); everything else goes the way it always has.
                let now_ms = epoch_secs_ms();
                // The newest raw message, before any mapping (docs/add-roaster.md)
                if let Some(route) = mqtt_routes.route_of_topic(&topic) {
                    telemetry_service.raw.record(
                        &route.device_id,
                        route.connection_id.as_deref(),
                        "mqtt",
                        Some(&topic),
                        &payload,
                    );
                }
                if let ingest::Routed::Sample { device_id, sample } =
                    mqtt_routes.on_message(&topic, &payload, now_ms)
                {
                    if let Some(sample) = sample {
                        let status =
                            crate::roasters::registry::get_device_by_device_id(&db, &device_id)
                                .await
                                .ok()
                                .flatten()
                                .map(|d| d.device.status);
                        telemetry_service
                            .process_telemetry(&device_id, &sample, status.as_ref())
                            .await;
                    }
                    continue;
                }
                if let Some((device_id, kind)) = parse_roaster_topic(&topic) {
                    let now = epoch_secs();
                    if kind == "telemetry" {
                        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&payload) {
                            // Auto-discover: if device_id is not in the devices table, create it with status 'pending'
                            let device_status =
                                match crate::roasters::registry::get_device_by_device_id(
                                    &db, &device_id,
                                )
                                .await
                                {
                                    Ok(Some(dev)) => {
                                        let conn = dev
                                            .connections
                                            .iter()
                                            .filter(|c| c.enabled && c.protocol == Protocol::Mqtt)
                                            .max_by_key(|c| c.priority);
                                        telemetry_service.raw.record(
                                            &device_id,
                                            conn.map(|c| c.id.as_str()),
                                            "mqtt",
                                            Some(&topic),
                                            &payload,
                                        );
                                        Some(dev.device.status)
                                    }
                                    Ok(None) => {
                                        // Auto-create the device
                                        let req = CreateDeviceRequest {
                                            name: device_id.clone(),
                                            device_id: device_id.clone(),
                                            profile_id: None,
                                            description: Some(
                                                "Auto-discovered via MQTT".to_string(),
                                            ),
                                            location: None,
                                        };
                                        match crate::roasters::registry::create_device(&db, req)
                                            .await
                                        {
                                            Ok(dev) => {
                                                // Add a default MQTT connection config derived from the topic
                                                let mqtt_config = serde_json::json!({
                                                    "topic_prefix": format!("roaster/{}", device_id),
                                                    "qos": 0
                                                });
                                                let conn_req = CreateConnectionRequest {
                                                    protocol: Protocol::Mqtt,
                                                    enabled: Some(true),
                                                    priority: Some(0),
                                                    config: mqtt_config,
                                                };
                                                if let Err(e) =
                                                    crate::roasters::registry::add_connection(
                                                        &db, &dev.id, conn_req,
                                                    )
                                                    .await
                                                {
                                                    tracing::warn!(%device_id, error = %e, "Failed to add default MQTT connection for auto-discovered device");
                                                }
                                                telemetry_service.raw.record(
                                                    &device_id,
                                                    None,
                                                    "mqtt",
                                                    Some(&topic),
                                                    &payload,
                                                );
                                                tracing::info!(%device_id, "Auto-discovered new device via MQTT");
                                                Some(DeviceStatus::Pending)
                                            }
                                            Err(e) => {
                                                tracing::warn!(%device_id, error = %e, "Failed to auto-create device");
                                                None
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(%device_id, error = %e, "Failed to look up device");
                                        None
                                    }
                                };

                            // Shared telemetry processing (cache, persist, session recording, last-seen)
                            telemetry_service
                                .process_telemetry(&device_id, &val, device_status.as_ref())
                                .await;
                        }
                    } else if kind == "status"
                        && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&payload) {
                            metrics
                                .status_last_seen
                                .with_label_values(&[&device_id])
                                .set(now as i64);
                            let mut reg = device_registry.write().await;
                            let entry = reg.entry(device_id.clone()).or_insert(DeviceInfo {
                                device_id: device_id.clone(),
                                last_seen: now,
                                id: None,
                                ip: None,
                                version: None,
                                rssi: None,
                                status_raw: None,
                            });
                            entry.last_seen = now;
                            caps.update_from_status(&device_id, &val);
                            entry.status_raw = Some(val.clone());
                            entry.id = val
                                .get("id")
                                .and_then(|v| v.as_str())
                                .map(std::string::ToString::to_string);
                            entry.ip = val
                                .get("ip")
                                .and_then(|v| v.as_str())
                                .map(std::string::ToString::to_string);
                            entry.version = val
                                .get("version")
                                .and_then(|v| v.as_str())
                                .map(std::string::ToString::to_string);
                            entry.rssi = val.get("rssi").and_then(serde_json::Value::as_i64);
                        }
                }
            }
            Ok(roastwarden_mqtt::MqttEvent::PubAck(_)) => { /* ack observed */ }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "MQTT consumer fell behind; messages skipped");
            }
            Err(broadcast::error::RecvError::Closed) => {
                tracing::error!("MQTT event channel closed; the consumer stops");
                return;
            }
        }
    }
}

fn epoch_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn epoch_secs_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
