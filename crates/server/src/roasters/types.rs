//! A roaster's stored and API types: the device, its profile, its connections and status,
//! Modbus register maps, and the requests that create and change them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, FromRow, Type};

/// Device status enum matching the `devices.status` column constraint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Pending,
    Active,
    Disabled,
    Error,
}

impl Type<sqlx::Sqlite> for DeviceStatus {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for DeviceStatus {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl<'q> Encode<'q, sqlx::Sqlite> for DeviceStatus {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DeviceStatus::Pending => "pending",
            DeviceStatus::Active => "active",
            DeviceStatus::Disabled => "disabled",
            DeviceStatus::Error => "error",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for DeviceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(DeviceStatus::Pending),
            "active" => Ok(DeviceStatus::Active),
            "disabled" => Ok(DeviceStatus::Disabled),
            "error" => Ok(DeviceStatus::Error),
            _ => Err(format!("Invalid device status: {s}")),
        }
    }
}

/// Connection protocol enum matching the `device_connections.protocol` column.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Mqtt,
    #[serde(rename = "websocket")]
    WebSocket,
    ModbusTcp,
    /// Modbus RTU over a serial port (RS-485 or RS-232): PID controllers, commercial drums.
    ModbusRtu,
    /// TC4 / aArtisan line protocol over a serial port (USB CDC, UART, RS-485).
    SerialTc4,
    /// A Phidgets temperature board (1048, TMP1101, …) read through the Phidget22 network
    /// server (docs/phidgets.md).
    Phidget22,
}

impl Type<sqlx::Sqlite> for Protocol {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for Protocol {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl<'q> Encode<'q, sqlx::Sqlite> for Protocol {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Protocol::Mqtt => "mqtt",
            Protocol::WebSocket => "websocket",
            Protocol::ModbusTcp => "modbus_tcp",
            Protocol::ModbusRtu => "modbus_rtu",
            Protocol::SerialTc4 => "serial_tc4",
            Protocol::Phidget22 => "phidget22",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for Protocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mqtt" => Ok(Protocol::Mqtt),
            "websocket" => Ok(Protocol::WebSocket),
            "modbus_tcp" => Ok(Protocol::ModbusTcp),
            "modbus_rtu" => Ok(Protocol::ModbusRtu),
            "serial_tc4" => Ok(Protocol::SerialTc4),
            "phidget22" => Ok(Protocol::Phidget22),
            _ => Err(format!("Invalid protocol: {s}")),
        }
    }
}

/// Modbus register type enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModbusRegisterType {
    Input,
    Holding,
    Coil,
    Discrete,
}

impl Type<sqlx::Sqlite> for ModbusRegisterType {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for ModbusRegisterType {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl<'q> Encode<'q, sqlx::Sqlite> for ModbusRegisterType {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for ModbusRegisterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ModbusRegisterType::Input => "input",
            ModbusRegisterType::Holding => "holding",
            ModbusRegisterType::Coil => "coil",
            ModbusRegisterType::Discrete => "discrete",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for ModbusRegisterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "input" => Ok(ModbusRegisterType::Input),
            "holding" => Ok(ModbusRegisterType::Holding),
            "coil" => Ok(ModbusRegisterType::Coil),
            "discrete" => Ok(ModbusRegisterType::Discrete),
            _ => Err(format!("Invalid modbus register type: {s}")),
        }
    }
}

/// Modbus data type enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModbusDataType {
    Uint16,
    Int16,
    Float32,
    Uint32,
    Int32,
    Bool,
    /// Binary-coded decimal in one register: four digits, 0 to 9999 (Artisan's `BCDsAsInt`)
    Bcd16,
    /// Binary-coded decimal in two registers: eight digits, in the map's word order (Artisan's
    /// `bcd`)
    Bcd32,
}

impl ModbusDataType {
    /// Registers a value takes: two for the 32-bit types, one otherwise.
    pub fn words(&self) -> usize {
        match self {
            ModbusDataType::Float32
            | ModbusDataType::Uint32
            | ModbusDataType::Int32
            | ModbusDataType::Bcd32 => 2,
            _ => 1,
        }
    }
}

impl Type<sqlx::Sqlite> for ModbusDataType {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for ModbusDataType {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl<'q> Encode<'q, sqlx::Sqlite> for ModbusDataType {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for ModbusDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ModbusDataType::Uint16 => "uint16",
            ModbusDataType::Int16 => "int16",
            ModbusDataType::Float32 => "float32",
            ModbusDataType::Uint32 => "uint32",
            ModbusDataType::Int32 => "int32",
            ModbusDataType::Bool => "bool",
            ModbusDataType::Bcd16 => "bcd16",
            ModbusDataType::Bcd32 => "bcd32",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for ModbusDataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "uint16" => Ok(ModbusDataType::Uint16),
            "int16" => Ok(ModbusDataType::Int16),
            "float32" => Ok(ModbusDataType::Float32),
            "uint32" => Ok(ModbusDataType::Uint32),
            "int32" => Ok(ModbusDataType::Int32),
            "bool" => Ok(ModbusDataType::Bool),
            "bcd16" => Ok(ModbusDataType::Bcd16),
            "bcd32" => Ok(ModbusDataType::Bcd32),
            _ => Err(format!("Invalid modbus data type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_id: String,
    pub profile_id: Option<String>,
    pub status: DeviceStatus,
    pub description: Option<String>,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct DeviceProfile {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub default_control_mode: Option<String>,
    pub default_setpoint: Option<f64>,
    /// Fan, %.
    pub default_fan: Option<f64>,
    pub default_kp: Option<f64>,
    pub default_ki: Option<f64>,
    pub default_kd: Option<f64>,
    pub max_temp: Option<f64>,
    /// Lowest fan %, for the device's own interlock.
    pub min_fan: Option<f64>,
    pub telemetry_interval_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct DeviceConnection {
    pub id: String,
    pub device_id: String,
    pub protocol: Protocol,
    pub enabled: bool,
    pub priority: i32,
    /// The protocol's settings; a `password` shows as `********` (docs/phidgets.md).
    #[sqlx(json)]
    #[serde(serialize_with = "masked")]
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What an answer shows for a saved password; sent back, it keeps the saved one.
pub const PASSWORD_MASK: &str = "********";

/// A connection's config as answers show it: a password masked.
fn masked<S: serde::Serializer>(config: &serde_json::Value, s: S) -> Result<S::Ok, S::Error> {
    match config.get("password").and_then(|p| p.as_str()) {
        Some(p) if !p.is_empty() => {
            let mut c = config.clone();
            c["password"] = serde_json::Value::String(PASSWORD_MASK.into());
            c.serialize(s)
        }
        _ => config.serialize(s),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct ModbusRegisterMap {
    pub id: String,
    pub device_id: String,
    pub register_type: ModbusRegisterType,
    pub address: i32,
    pub name: String,
    pub data_type: ModbusDataType,
    pub byte_order: Option<String>,
    pub scale_factor: Option<f64>,
    pub offset: Option<f64>,
    pub unit: Option<String>,
    pub description: Option<String>,
    pub writable: bool,
    /// What the register means (docs/ingest-mapping.md, "Modbus TCP and RTU"): `bt`, `et`,
    /// `ambient`, `ror`, `heater`, `heater_applied`, `fan`, `drum`, `damper`, `setpoint`,
    /// `mode`, `heater_enable`, or `emergency_stop` (write only). Null: read and written by its
    /// name, as before.
    #[sqlx(default)]
    pub role: Option<String>,
    /// The Modbus slave this register is on, 1 to 247 (a bus of several controllers, as most
    /// of Artisan's machine setups have; docs/artisan-machines.md). Null: the connection's
    /// `unit_id`.
    #[sqlx(default)]
    pub unit_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DeviceWithConnections {
    #[serde(flatten)]
    pub device: Device,
    pub connections: Vec<DeviceConnection>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateDeviceRequest {
    pub name: String,
    pub device_id: String,
    pub profile_id: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub profile_id: Option<String>,
    pub status: Option<DeviceStatus>,
    pub description: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateDeviceProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub default_control_mode: Option<String>,
    pub default_setpoint: Option<f64>,
    /// Fan, %.
    pub default_fan: Option<f64>,
    pub default_kp: Option<f64>,
    pub default_ki: Option<f64>,
    pub default_kd: Option<f64>,
    pub max_temp: Option<f64>,
    /// Lowest fan %, for the device's own interlock.
    pub min_fan: Option<f64>,
    pub telemetry_interval_ms: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[allow(dead_code)] // Read by the device profile edit form
pub struct UpdateDeviceProfileRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub default_control_mode: Option<String>,
    pub default_setpoint: Option<f64>,
    /// Fan, %.
    pub default_fan: Option<f64>,
    pub default_kp: Option<f64>,
    pub default_ki: Option<f64>,
    pub default_kd: Option<f64>,
    pub max_temp: Option<f64>,
    /// Lowest fan %, for the device's own interlock.
    pub min_fan: Option<f64>,
    pub telemetry_interval_ms: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateConnectionRequest {
    pub protocol: Protocol,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateConnectionRequest {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRegisterMapEntry {
    pub register_type: ModbusRegisterType,
    pub address: i32,
    pub name: String,
    pub data_type: ModbusDataType,
    pub byte_order: Option<String>,
    pub scale_factor: Option<f64>,
    pub offset: Option<f64>,
    pub unit: Option<String>,
    pub description: Option<String>,
    pub writable: Option<bool>,
    /// The register's role (see [`ModbusRegisterMap::role`]); null or left out: none.
    #[serde(default)]
    pub role: Option<String>,
    /// The register's own Modbus slave, 1 to 247 (see [`ModbusRegisterMap::unit_id`]); null
    /// or left out: the connection's.
    #[serde(default)]
    pub unit_id: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TestConnectionRequest {
    pub protocol: Protocol,
    pub config: serde_json::Value,
    pub device_id: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TestConnectionResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}
