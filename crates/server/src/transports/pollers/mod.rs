//! Background tasks that connect to configured devices (Modbus TCP and RTU, WebSocket, serial
//! TC4, a Phidgets board through the Phidget22 network server) and poll or stream their telemetry into the one `TelemetryService` pipeline.
//!
//! MQTT devices are handled by the MQTT consumer loop in lib.rs; this module covers the rest.
//! While a connection is up its task also registers a command sink with the
//! [`TransportRegistry`], so control commands reach these devices instead of falling through
//! to MQTT.
//!
//! [`Pollers`] keeps one task per enabled connection of an active device, in step with the
//! database: the device routes call [`Pollers::sync_device`] after every change to a device, a
//! connection or a register map, which starts, restarts or stops that device's tasks at once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio::task::JoinHandle;

use sqlx::SqlitePool;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::types::{
    DeviceConnection, DeviceStatus, DeviceWithConnections, ModbusRegisterMap, Protocol,
};
use crate::transports::ingest::{MqttRoute, MqttRouter};
use crate::transports::transport::{CommandSink, SinkHandle, TransportRegistry};
use roastwarden_mqtt::MqttService;

mod modbus;
mod phidget;
mod tc4;
#[cfg(test)]
mod tests;
mod validate;
mod websocket;

use modbus::modbus_poller_loop;
use phidget::phidget_loop;
use tc4::serial_tc4_loop;
use websocket::websocket_client_loop;

pub(crate) use modbus::probe_modbus;
pub(crate) use validate::{check_connection_config, check_register_map};

/// The shortest poll interval a connection may ask for.
const MIN_POLL_MS: u64 = 100;

/// The running device tasks, by connection id.
pub struct Pollers {
    db: SqlitePool,
    telemetry: TelemetryService,
    transports: Arc<TransportRegistry>,
    running: tokio::sync::Mutex<HashMap<String, Running>>,
    /// The MQTT route table (docs/ingest-mapping.md, "MQTT"): kept in step by the same calls.
    mqtt_routes: Arc<MqttRouter>,
    mqtt: MqttService,
}

struct Running {
    /// The device's registry id (`devices.id`)
    device: String,
    /// What the task was started with; a change restarts it.
    fingerprint: String,
    task: JoinHandle<()>,
    sink: Tracked,
}

/// A connection that should have a task, with what it is started from.
struct Wanted {
    roaster: String,
    conn: DeviceConnection,
    map: Vec<ModbusRegisterMap>,
    fingerprint: String,
}

impl Pollers {
    pub fn new(
        db: SqlitePool,
        telemetry: TelemetryService,
        transports: Arc<TransportRegistry>,
        mqtt_routes: Arc<MqttRouter>,
        mqtt: MqttService,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            telemetry,
            transports,
            running: tokio::sync::Mutex::new(HashMap::new()),
            mqtt_routes,
            mqtt,
        })
    }

    /// Bring every device's tasks in step with the database (at boot).
    pub async fn sync_all(&self) {
        let mut running = self.running.lock().await;
        let devices = match crate::roasters::registry::list_devices(&self.db, None).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "Failed to list devices for the pollers");
                return;
            }
        };
        let mut wanted = Vec::new();
        for device in devices {
            self.sync_mqtt(&device.id).await;
            match self.wanted(&device.id).await {
                Ok(w) => wanted.extend(w),
                Err(e) => tracing::error!(device = %device.device_id, error = %e,
                    "Failed to read a device's connections for its poller"),
            }
        }
        self.reconcile(&mut running, None, wanted).await;
    }

    /// Put a device's MQTT route in the table (or take it out), and subscribe to the topics
    /// it adds. A topic under `roaster/` is already covered by the server's `roaster/#`; a
    /// second, overlapping subscription would make some brokers deliver twice (app.rs).
    async fn sync_mqtt(&self, registry_id: &str) {
        let route = match crate::roasters::registry::get_device(&self.db, registry_id).await {
            Ok(Some(dev)) => mqtt_route_of(&dev),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(registry_id, error = %e,
                    "Failed to read a device's connections; its MQTT route is unchanged");
                return;
            }
        };
        for topic in self.mqtt_routes.set(registry_id, route) {
            if topic.starts_with(&format!("{}/", roastwarden_core::ROOT)) {
                continue;
            }
            match self.mqtt.subscribe(&topic, rumqttc::QoS::AtMostOnce).await {
                Ok(()) => tracing::info!(topic, "subscribed to a mapped MQTT topic"),
                Err(e) => tracing::warn!(topic, error = ?e, "MQTT subscribe failed"),
            }
        }
    }

    /// Bring one device's tasks in step with the database: after its connections, register
    /// map or status changed, or it was deleted. `registry_id` is `devices.id`.
    pub async fn sync_device(&self, registry_id: &str) {
        // Held from the read to the last start, so two changes in quick succession apply in
        // order and the later one wins.
        let mut running = self.running.lock().await;
        self.sync_mqtt(registry_id).await;
        match self.wanted(registry_id).await {
            Ok(wanted) => {
                self.reconcile(&mut running, Some(registry_id), wanted)
                    .await;
            }
            Err(e) => tracing::error!(registry_id, error = %e,
                "Failed to read a device's connections; its pollers are unchanged"),
        }
    }

    /// The connection ids with a task, for tests and diagnostics.
    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn running(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.running.lock().await.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The connections of a device that should have a task: every enabled one but MQTT, when
    /// the device is active.
    async fn wanted(&self, registry_id: &str) -> anyhow::Result<Vec<Wanted>> {
        let Some(dev) = crate::roasters::registry::get_device(&self.db, registry_id).await? else {
            return Ok(Vec::new());
        };
        if dev.device.status != DeviceStatus::Active {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for conn in dev.connections {
            if !conn.enabled || conn.protocol == Protocol::Mqtt {
                continue;
            }
            let map = if matches!(conn.protocol, Protocol::ModbusTcp | Protocol::ModbusRtu) {
                crate::roasters::registry::get_register_map(&self.db, registry_id).await?
            } else {
                Vec::new()
            };
            let fingerprint = serde_json::to_string(&json!({
                "roaster": dev.device.device_id,
                "protocol": conn.protocol,
                "priority": conn.priority,
                "config": conn.config,
                "map": map,
            }))?;
            out.push(Wanted {
                roaster: dev.device.device_id.clone(),
                conn,
                map,
                fingerprint,
            });
        }
        Ok(out)
    }

    /// Stop the tasks (of `scope`'s device, or of every device) that are no longer wanted or
    /// were started from something else, then start what is wanted and not running.
    async fn reconcile(
        &self,
        running: &mut HashMap<String, Running>,
        scope: Option<&str>,
        wanted: Vec<Wanted>,
    ) {
        let stale: Vec<String> = running
            .iter()
            .filter(|(id, r)| {
                scope.is_none_or(|d| d == r.device)
                    && !wanted
                        .iter()
                        .any(|w| &w.conn.id == *id && w.fingerprint == r.fingerprint)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            if let Some(r) = running.remove(&id) {
                r.task.abort();
                // Wait for the task to be dropped, so its port or socket is closed before a
                // new task opens it, then take its command sink out of the registry.
                let _ = r.task.await;
                r.sink.release().await;
                tracing::info!(connection = %id, "device poller stopped");
            }
        }
        for w in wanted {
            if running.contains_key(&w.conn.id) {
                continue;
            }
            let sink = Tracked::new(self.transports.clone());
            let id = w.conn.id.clone();
            tracing::info!(roaster = %w.roaster, protocol = %w.conn.protocol, connection = %id,
                "device poller starting");
            let device = w.conn.device_id.clone();
            let fingerprint = w.fingerprint.clone();
            let task = self.spawn(w, sink.clone());
            running.insert(
                id,
                Running {
                    device,
                    fingerprint,
                    task,
                    sink,
                },
            );
        }
    }

    fn spawn(&self, w: Wanted, sink: Tracked) -> JoinHandle<()> {
        let ts = self.telemetry.clone();
        let Wanted {
            roaster, conn, map, ..
        } = w;
        match conn.protocol {
            Protocol::ModbusTcp | Protocol::ModbusRtu => {
                tokio::spawn(modbus_poller_loop(roaster, conn, map, ts, sink))
            }
            Protocol::WebSocket => tokio::spawn(websocket_client_loop(roaster, conn, ts, sink)),
            Protocol::SerialTc4 => tokio::spawn(serial_tc4_loop(roaster, conn, ts, sink)),
            // Reads only: no command sink (docs/phidgets.md).
            Protocol::Phidget22 => tokio::spawn(phidget_loop(roaster, conn, ts)),
            // Handled by the MQTT consumer loop; never wanted.
            Protocol::Mqtt => tokio::spawn(async {}),
        }
    }
}

/// A device's MQTT route: its enabled MQTT connection (the highest priority when there are
/// several) when that differs from the default contract; `None` otherwise.
pub(crate) fn mqtt_route_of(dev: &DeviceWithConnections) -> Option<MqttRoute> {
    let conn = dev
        .connections
        .iter()
        .filter(|c| c.enabled && c.protocol == Protocol::Mqtt)
        .max_by_key(|c| c.priority)?;
    match MqttRoute::from_config(&dev.device.device_id, &conn.config) {
        Ok(r) => r.map(|r| MqttRoute {
            connection_id: Some(conn.id.clone()),
            ..r
        }),
        Err(e) => {
            // Refused when saved; a config from before the checks is left out.
            tracing::warn!(device = %dev.device.device_id, error = %e,
                "MQTT connection config not read; the device uses the default topics");
            None
        }
    }
}

/// The [`TransportRegistry`] as a device task sees it: the same `register` and `unregister`,
/// remembering the live handle so that [`Pollers`] can take the sink out when it stops the
/// task from outside.
#[derive(Clone)]
struct Tracked {
    registry: Arc<TransportRegistry>,
    live: Arc<Mutex<Option<SinkHandle>>>,
}

impl Tracked {
    fn new(registry: Arc<TransportRegistry>) -> Self {
        Self {
            registry,
            live: Arc::new(Mutex::new(None)),
        }
    }

    async fn register(
        &self,
        device_id: &str,
        priority: i32,
        sink: Arc<dyn CommandSink>,
    ) -> SinkHandle {
        let handle = self.registry.register(device_id, priority, sink).await;
        *self
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle.clone());
        handle
    }

    async fn unregister(&self, handle: &SinkHandle) {
        // Out of the registry first: a task stopped half way leaves the handle for release.
        self.registry.unregister(handle).await;
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    /// Unregister whatever the task left registered (it was stopped mid-connection).
    async fn release(&self) {
        let handle = self
            .live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            self.registry.unregister(&handle).await;
        }
    }
}
