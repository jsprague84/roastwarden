//! The state every handler and loop shares (`AppState`), the status the roasters last sent and
//! the server's Prometheus metrics.

use std::collections::HashMap;
use std::sync::Arc;

use prometheus::{IntCounter, IntGauge, IntGaugeVec};
use roastwarden_mqtt::MqttService;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::analysis::alarms::AlarmService;
use crate::analysis::telemetry::TelemetryService;
use crate::auth::AuthConfig;
use crate::replay::ReplayService;
use crate::roasters::discovery::DiscoveryService;
use crate::transports::transport::TransportRegistry;
use crate::{auth, coffee, control, labels, notify, ops, predict, roasters, sim, transports};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) mqtt: MqttService,
    pub(crate) telemetry_cache: Arc<RwLock<HashMap<String, (serde_json::Value, u64)>>>,
    pub(crate) device_registry: Arc<RwLock<HashMap<String, DeviceInfo>>>,
    pub(crate) metrics: Arc<Metrics>,
    pub(crate) db: SqlitePool,
    pub(crate) telemetry_service: TelemetryService,
    /// Outbound command transports (MQTT, device WebSockets, Modbus pollers).
    pub(crate) transports: Arc<TransportRegistry>,
    /// One task per enabled Modbus, WebSocket or serial connection (`device_poller.rs`)
    pub(crate) pollers: Arc<transports::pollers::Pollers>,
    /// MQTT devices with their own topics, field names and command syntax (ingest.rs)
    pub(crate) mqtt_routes: Arc<transports::ingest::MqttRouter>,
    pub(crate) auth: AuthConfig,
    pub(crate) replay: Arc<ReplayService>,
    pub(crate) alarms: Arc<AlarmService>,
    pub(crate) discovery: Arc<DiscoveryService>,
    pub(crate) gain_scheduler: Arc<control::schedule::GainScheduler>,
    pub(crate) fan_assist: Arc<control::fan_assist::FanAssistService>,
    pub(crate) virtual_roaster: Arc<sim::virtual_roaster::VirtualRoaster>,
    /// The server-run autotune (docs/autotune-2.md)
    pub(crate) tune: Arc<control::tune::TuneService>,
    /// Prediction from the roaster's own model (docs/prediction.md)
    pub(crate) predict: Arc<predict::PredictService>,
    pub(crate) sequence: Arc<control::sequence::SequenceService>,
    pub(crate) backups: Arc<ops::backups::Backups>,
    pub(crate) push: notify::push::PushService,
    pub(crate) notifier: notify::Notifier,
    pub(crate) approvals: auth::approvals::Approvals,
    pub(crate) importer: coffee::import::Importer,
    /// Owner label designs and fonts, beside the database (labels/mod.rs)
    pub(crate) labels: labels::Folder,
    /// Where label renders run: a sandboxed child process in production (labels/sandbox.rs)
    pub(crate) label_renderer: labels::Renderer,
    /// The Modbus slave's settings (`ROASTWARDEN_MODBUS_ADDR`; modbus.rs); none: off
    pub(crate) modbus: Option<Arc<transports::modbus::Screen>>,
    /// Home Assistant discovery (`home_assistant.rs`), off unless switched on
    pub(crate) home_assistant: Arc<notify::home_assistant::HomeAssistant>,
    /// The roasters' MQTT passwords: `MQTT_DEVICES` and the dashboard's (`mqtt_passwords.rs`)
    pub(crate) mqtt_passwords: roasters::mqtt_passwords::MqttPasswords,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct DeviceInfo {
    pub(crate) device_id: String,
    pub(crate) last_seen: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rssi: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status_raw: Option<serde_json::Value>,
}

pub(crate) struct Metrics {
    pub(crate) mqtt_connected: IntGauge,
    pub(crate) mqtt_rx_total: IntCounter,
    pub(crate) mqtt_tx_total: IntCounter,
    pub(crate) ws_clients: IntGauge,
    pub(crate) telemetry_last_seen: IntGaugeVec, // label: device_id
    pub(crate) status_last_seen: IntGaugeVec,    // label: device_id
}

impl Metrics {
    pub(crate) fn new() -> Arc<Self> {
        let mqtt_connected = IntGauge::new(
            "roastwarden_mqtt_connected",
            "MQTT connection status (1 connected, 0 otherwise)",
        )
        .expect("a valid metric name and help");
        let mqtt_rx_total = IntCounter::new(
            "roastwarden_mqtt_messages_received_total",
            "Total MQTT messages received",
        )
        .expect("a valid metric name and help");
        let mqtt_tx_total = IntCounter::new(
            "roastwarden_mqtt_messages_published_total",
            "Total MQTT messages published",
        )
        .expect("a valid metric name and help");
        let ws_clients = IntGauge::new(
            "roastwarden_ws_clients",
            "Number of connected WebSocket clients",
        )
        .expect("a valid metric name and help");
        let telemetry_last_seen = IntGaugeVec::new(
            prometheus::Opts::new(
                "roastwarden_device_telemetry_last_seen",
                "Last seen telemetry epoch seconds",
            ),
            &["device_id"],
        )
        .expect("a valid metric name and help");
        let status_last_seen = IntGaugeVec::new(
            prometheus::Opts::new(
                "roastwarden_device_status_last_seen",
                "Last seen status epoch seconds",
            ),
            &["device_id"],
        )
        .expect("a valid metric name and help");

        let registry = prometheus::default_registry();
        let _ = registry.register(Box::new(mqtt_connected.clone()));
        let _ = registry.register(Box::new(mqtt_rx_total.clone()));
        let _ = registry.register(Box::new(mqtt_tx_total.clone()));
        let _ = registry.register(Box::new(ws_clients.clone()));
        let _ = registry.register(Box::new(telemetry_last_seen.clone()));
        let _ = registry.register(Box::new(status_last_seen.clone()));

        Arc::new(Self {
            mqtt_connected,
            mqtt_rx_total,
            mqtt_tx_total,
            ws_clients,
            telemetry_last_seen,
            status_last_seen,
        })
    }
}
