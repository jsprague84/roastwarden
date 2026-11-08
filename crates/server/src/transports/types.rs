//! Each transport's connection config, as a roaster's connection stores it: WebSocket,
//! MQTT, Modbus TCP and RTU, TC4 serial and Phidget22.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WebSocketConnectionConfig {
    pub url: String,
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_ms: u64,
    /// The device's own field names and units (docs/ingest-mapping.md): `{units, fields}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingest: Option<serde_json::Value>,
    /// Command templates: `{heater: {payload, scale, decimals}, …}`; the payload is the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<serde_json::Value>,
    /// For a device that answers requests rather than pushes (Artisan's WebSocket protocol).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<WebSocketPoll>,
}

/// A request the WebSocket client sends every `interval_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WebSocketPoll {
    /// The frame sent; `{id}` is replaced by a counter (1, 2, …).
    pub message: String,
    /// At least 200.
    #[serde(default = "default_poll_interval")]
    pub interval_ms: u64,
}

/// An MQTT connection's config (docs/ingest-mapping.md, "MQTT"); every key may be left out.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MqttConnectionConfig {
    /// Commands go to `<topic_prefix>/control/<command>`; default `roaster/<device_id>`.
    #[serde(default)]
    pub topic_prefix: Option<String>,
    /// The topic whose messages are samples; default `<topic_prefix>/telemetry`.
    #[serde(default)]
    pub telemetry_topic: Option<String>,
    #[serde(default)]
    pub qos: Option<u8>,
    /// The device's own field names and units: `{units, fields}`; a field may name its own
    /// `topic`.
    #[serde(default)]
    pub ingest: Option<serde_json::Value>,
    /// Command templates: `{heater: {topic, payload, scale, decimals}, …}`.
    #[serde(default)]
    pub commands: Option<serde_json::Value>,
}

fn default_reconnect_interval() -> u64 {
    5000
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModbusTcpConnectionConfig {
    /// IP address or host name.
    pub host: String,
    #[serde(default = "default_modbus_port")]
    pub port: u16,
    #[serde(default = "default_unit_id")]
    pub unit_id: u8,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// How long one request waits for the device's answer before the link is reopened.
    #[serde(default = "default_modbus_timeout")]
    pub timeout_ms: u64,
}

/// Modbus RTU over a serial port (`docs/hardware-integration-guide.md`, "Modbus RTU"). The
/// register map is the device's, as for Modbus TCP.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ModbusRtuConnectionConfig {
    /// Serial device path, e.g. `/dev/ttyUSB0` (an RS-485 adapter).
    pub port: String,
    #[serde(default = "default_rtu_baud")]
    pub baud: u32,
    #[serde(default)]
    pub parity: SerialParity,
    /// 5 to 8; 8 for nearly every controller.
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    /// 1 or 2.
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// The controller's slave address, 1 to 247.
    #[serde(default = "default_unit_id")]
    pub unit_id: u8,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// How long one request waits for the device's answer before the port is reopened.
    #[serde(default = "default_modbus_timeout")]
    pub timeout_ms: u64,
    /// `rtu` (binary frames, the default) or `ascii` (Modbus ASCII: `:`-framed hex lines with
    /// an LRC, often at 7 data bits; Delta DTA and DTK controllers)
    #[serde(default)]
    pub framing: ModbusFraming,
}

/// How a Modbus serial line frames its messages.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModbusFraming {
    #[default]
    Rtu,
    Ascii,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SerialParity {
    #[default]
    None,
    Even,
    Odd,
}

fn default_poll_interval() -> u64 {
    1000
}

fn default_modbus_port() -> u16 {
    502
}

fn default_unit_id() -> u8 {
    1
}

fn default_modbus_timeout() -> u64 {
    1000
}

fn default_rtu_baud() -> u32 {
    9600
}

fn default_data_bits() -> u8 {
    8
}

fn default_stop_bits() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SerialTc4ConnectionConfig {
    /// Serial device path, e.g. `/dev/ttyUSB0` or `/dev/ttyACM0`.
    pub port: String,
    #[serde(default = "default_baud")]
    pub baud: u32,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// Physical TC4 input carrying the environment probe: logical channel 1 of `CHAN`
    /// (1..4, 0 = no ET probe).
    #[serde(default = "default_et_channel")]
    pub et_channel: u8,
    /// Physical TC4 input carrying the bean probe: logical channel 2 of `CHAN` (1..4, 0 = none).
    #[serde(default = "default_bt_channel")]
    pub bt_channel: u8,
    /// Physical input for logical channel 3, reported as the extra field `t3` (0 = off).
    #[serde(default)]
    pub t3_channel: u8,
    /// Physical input for logical channel 4, reported as the extra field `t4` (0 = off).
    #[serde(default)]
    pub t4_channel: u8,
    /// How the device answers `READ`: `fixed` (ambient and all four channels, the
    /// default) or `active` (stock aArtisan: only the channels `CHAN` turned on).
    #[serde(default)]
    pub reply_format: crate::transports::tc4::ReplyFormat,
    /// The output that drives the fan: `io3` (default), `ot2`, `dcfan` or `none`.
    #[serde(default)]
    pub fan_output: crate::transports::tc4::FanOutput,
    /// `{units: "F"}` sends `UNITS;F` and converts; `{columns: [...]}` reads the reply by
    /// position instead of `reply_format` (docs/ingest-mapping.md, "Serial TC4").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingest: Option<serde_json::Value>,
    /// Command templates: `{drum: {payload: "OT2;{value}"}, …}`; several lines split by `\n`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<serde_json::Value>,
}

/// A Phidgets temperature board through the Phidget22 network server (docs/phidgets.md).
/// Readings are named by the board's temperature input, `ch0` to `ch7` (Phidgets' channel
/// number); `ingest.fields` says which is BT, ET or a channel (default BT `ch0`, ET `ch1`, as
/// Artisan reads a 1048 or a TMP1101).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Phidget22ConnectionConfig {
    /// The computer running the Phidget22 network server, the board plugged into it.
    pub host: String,
    #[serde(default = "default_phidget_port")]
    pub port: u16,
    /// The network server's password, when it has one. Answers show it as `********`; sending
    /// that back keeps the saved one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// The board's serial number (a VINT board's is its hub's); none: the first temperature
    /// board the server has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<i32>,
    /// A VINT board's hub port (0 to 5); none: any port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_port: Option<i32>,
    /// How often a sample is made from the readings since the last one (their mean).
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// How often the board reports each input, in ms (a 1048 takes 32 to 60000).
    #[serde(default = "default_phidget_data_interval")]
    pub data_interval_ms: u32,
    /// The thermocouple type of every thermocouple input.
    #[serde(default)]
    pub thermocouple_type: ThermocoupleType,
    /// An input's own type, over `thermocouple_type`: `{"ch1": "J"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermocouple_types: Option<std::collections::BTreeMap<String, ThermocoupleType>>,
    /// An RTD input's (TMP1200) element.
    #[serde(default)]
    pub rtd_type: RtdType,
    /// An RTD input's wiring: 2, 3 or 4 wires.
    #[serde(default = "default_rtd_wires")]
    pub rtd_wires: u8,
    /// `{fields: {bt: "ch0", et: "ch1", exhaust: "ch2"}}`; a field may take `scale` and
    /// `offset` (docs/ingest-mapping.md, "Phidgets").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingest: Option<serde_json::Value>,
}

/// A thermocouple's type, as Phidgets name them.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub enum ThermocoupleType {
    J,
    #[default]
    K,
    E,
    T,
}

impl ThermocoupleType {
    /// Phidget22's number for it.
    pub fn code(self) -> i64 {
        match self {
            ThermocoupleType::J => 1,
            ThermocoupleType::K => 2,
            ThermocoupleType::E => 3,
            ThermocoupleType::T => 4,
        }
    }
}

/// An RTD's element and its curve (α 0.00385 or 0.00392).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RtdType {
    #[default]
    Pt100_3850,
    Pt1000_3850,
    Pt100_3920,
    Pt1000_3920,
}

impl RtdType {
    /// Phidget22's number for it.
    pub fn code(self) -> i64 {
        match self {
            RtdType::Pt100_3850 => 1,
            RtdType::Pt1000_3850 => 2,
            RtdType::Pt100_3920 => 3,
            RtdType::Pt1000_3920 => 4,
        }
    }
}

fn default_phidget_port() -> u16 {
    5661
}

fn default_phidget_data_interval() -> u32 {
    256
}

fn default_rtd_wires() -> u8 {
    2
}

fn default_baud() -> u32 {
    115_200
}

fn default_et_channel() -> u8 {
    1
}

fn default_bt_channel() -> u8 {
    2
}
