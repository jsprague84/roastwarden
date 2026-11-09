//! A roast session's stored and API types: the session, its telemetry and status, its
//! events, its cupping, and the requests that create and change them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, FromRow, Type};

use crate::http::patch::double_option;
use crate::profiles::types::ProfileWithPoints;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct RoastSession {
    pub id: String, // UUID as string
    pub name: String,
    pub device_id: String,
    pub profile_id: Option<String>, // Optional linked profile
    pub status: SessionStatus,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Metadata fields
    pub bean_origin: Option<String>,
    pub bean_variety: Option<String>,
    pub green_weight: Option<f32>,
    pub roasted_weight: Option<f32>,
    pub target_roast_level: Option<String>,
    pub notes: Option<String>,
    pub ambient_temp: Option<f32>,
    pub humidity: Option<f32>,

    // Session summary data
    pub max_temp: Option<f32>,
    pub total_time_seconds: Option<i32>,
    pub first_crack_time: Option<i32>,
    pub development_time_ratio: Option<f32>,

    // Enhanced statistics (AP-001)
    pub weight_loss_pct: Option<f32>,
    pub max_ror: Option<f32>,
    pub avg_ror_drying: Option<f32>,
    pub avg_ror_maillard: Option<f32>,
    pub avg_ror_development: Option<f32>,
    pub drying_end_time: Option<i32>,
    pub drying_end_temp: Option<f32>,

    // AUC (AP-002)
    pub auc_value: Option<f32>,

    // Roast properties (migration 016)
    pub roast_purpose: Option<String>,
    pub color_whole: Option<f32>,
    pub color_ground: Option<f32>,
    pub color_scale: Option<String>,
    pub moisture_green: Option<f32>,
    pub moisture_roasted: Option<f32>,
    pub density_green: Option<f32>,
    pub roast_flags: Option<String>,
    pub rating: Option<i32>,
    // Tracking against the session profile, TP..DROP (°C)
    pub profile_rmse: Option<f32>,
    pub profile_max_dev: Option<f32>,
    pub profile_bias: Option<f32>,
    // Batches and beans (migration 018, docs/batches.md)
    pub bean_id: Option<String>,
    pub batch_number: Option<i32>,
    pub planned_roast_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct SessionTelemetry {
    pub id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub elapsed_seconds: f32,
    pub bean_temp: Option<f32>,
    pub env_temp: Option<f32>,
    pub rate_of_rise: Option<f32>,
    /// %
    pub heater: Option<f64>,
    /// %
    pub fan: Option<f64>,
    pub setpoint: Option<f32>,
    /// What the roaster said about its control with this sample, only the keys it sent
    /// (`heaterApplied`, `rorLimited`, `Kp`, `Ki`, `Kd`, `controlMode`, `heaterEnable`,
    /// `stopCode`, `ff`; migration 037). None before that.
    #[sqlx(json(nullable), default)]
    #[schema(value_type = Option<Object>)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Planning,  // Created but not started
    Active,    // Currently roasting
    Paused,    // Temporarily paused
    Completed, // Successfully finished
    Failed,    // Ended due to error
    Cancelled, // Manually cancelled
}

// SQLx implementations for SessionStatus
impl Type<sqlx::Sqlite> for SessionStatus {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for SessionStatus {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl Encode<'_, sqlx::Sqlite> for SessionStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::sqlite::SqliteArgumentsBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SessionStatus::Planning => "planning",
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
            SessionStatus::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for SessionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "planning" => Ok(SessionStatus::Planning),
            "active" => Ok(SessionStatus::Active),
            "paused" => Ok(SessionStatus::Paused),
            "completed" => Ok(SessionStatus::Completed),
            "failed" => Ok(SessionStatus::Failed),
            "cancelled" => Ok(SessionStatus::Cancelled),
            _ => Err(format!("Invalid session status: {s}")),
        }
    }
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    pub name: String,
    pub device_id: String,
    pub profile_id: Option<String>,
    pub bean_origin: Option<String>,
    pub bean_variety: Option<String>,
    pub green_weight: Option<f32>,
    pub target_roast_level: Option<String>,
    pub notes: Option<String>,
    pub ambient_temp: Option<f32>,
    pub humidity: Option<f32>,
    /// A bean from stock (docs/batches.md): the roast draws its green weight from it.
    #[serde(default)]
    pub bean_id: Option<String>,
    /// The planned roast this starts; fills what the request leaves out.
    #[serde(default)]
    pub planned_roast_id: Option<String>,
    /// espresso | filter | omni | other; filled from the profile when absent.
    #[serde(default)]
    pub roast_purpose: Option<String>,
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateSessionRequest {
    pub name: Option<String>,
    // Roast properties: absent = unchanged, null = clear.
    #[serde(default, deserialize_with = "double_option")]
    pub roast_purpose: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color_whole: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color_ground: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color_scale: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub moisture_green: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub moisture_roasted: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub density_green: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub roast_flags: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub rating: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub bean_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub batch_number: Option<Option<i32>>,
    pub bean_origin: Option<String>,
    pub bean_variety: Option<String>,
    pub green_weight: Option<f32>,
    pub roasted_weight: Option<f32>,
    pub target_roast_level: Option<String>,
    pub ambient_temp: Option<f32>,
    pub humidity: Option<f32>,
    pub notes: Option<String>,
    pub first_crack_time: Option<i32>,
    pub development_time_ratio: Option<f32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionWithTelemetry {
    #[serde(flatten)]
    pub session: RoastSession,
    pub telemetry: Vec<SessionTelemetry>,
    pub profile: Option<ProfileWithPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cupping: Option<CuppingWithAttributes>,
    /// The roast's declared channels (docs/channels.md): the roaster's list when it started;
    /// for a roast from before migration 049, the roaster's list now
    pub channels: Vec<crate::roasters::channels::Channel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct RoastEvent {
    pub id: String, // UUID as string
    pub session_id: String,
    pub event_type: RoastEventType,
    pub elapsed_seconds: f32,
    pub temperature: Option<f32>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoastEventType {
    /// Beans loaded; roast time zero. Optional: a session's `start_time` is the
    /// charge when no explicit event is recorded.
    Charge,
    TurningPoint,
    DryingEnd,
    FirstCrackStart,
    FirstCrackEnd,
    SecondCrackStart,
    SecondCrackEnd,
    DevelopmentStart,
    Drop,
    CoolEnd,
    Custom,
}

// SQLx implementations for RoastEventType
impl Type<sqlx::Sqlite> for RoastEventType {
    fn type_info() -> SqliteTypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, sqlx::Sqlite> for RoastEventType {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl Encode<'_, sqlx::Sqlite> for RoastEventType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::sqlite::SqliteArgumentsBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as Encode<sqlx::Sqlite>>::encode_by_ref(&self.to_string(), buf)
    }
}

impl std::fmt::Display for RoastEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RoastEventType::Charge => "charge",
            RoastEventType::TurningPoint => "turning_point",
            RoastEventType::DryingEnd => "drying_end",
            RoastEventType::FirstCrackStart => "first_crack_start",
            RoastEventType::FirstCrackEnd => "first_crack_end",
            RoastEventType::SecondCrackStart => "second_crack_start",
            RoastEventType::SecondCrackEnd => "second_crack_end",
            RoastEventType::DevelopmentStart => "development_start",
            RoastEventType::Drop => "drop",
            RoastEventType::CoolEnd => "cool_end",
            RoastEventType::Custom => "custom",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for RoastEventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "charge" => Ok(RoastEventType::Charge),
            "turning_point" => Ok(RoastEventType::TurningPoint),
            "drying_end" => Ok(RoastEventType::DryingEnd),
            "first_crack_start" => Ok(RoastEventType::FirstCrackStart),
            "first_crack_end" => Ok(RoastEventType::FirstCrackEnd),
            "second_crack_start" => Ok(RoastEventType::SecondCrackStart),
            "second_crack_end" => Ok(RoastEventType::SecondCrackEnd),
            "development_start" => Ok(RoastEventType::DevelopmentStart),
            // "drop_out" was a duplicate of "drop" in early schemas.
            "drop" | "drop_out" => Ok(RoastEventType::Drop),
            "cool_end" => Ok(RoastEventType::CoolEnd),
            "custom" => Ok(RoastEventType::Custom),
            _ => Err(format!("Invalid roast event type: {s}")),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateRoastEventRequest {
    pub event_type: RoastEventType,
    pub elapsed_seconds: f32,
    pub temperature: Option<f32>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateRoastEventRequest {
    pub elapsed_seconds: Option<f32>,
    pub temperature: Option<f32>,
    pub notes: Option<String>,
}

/// A roast's cupping on the classic SCA form (`docs/cupping.md`).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct CuppingScore {
    pub id: String,
    pub session_id: String,
    /// `sca`: the classic SCA form (2004 protocol)
    pub scoring_framework: String,
    /// The ten attributes' total less defects; null until all ten are scored
    #[sqlx(rename = "overall_score")]
    pub final_score: Option<f32>,
    pub notes: Option<String>,
    /// Cups with a taint: 2 points each off the total
    pub taint_cups: i64,
    /// Cups with a fault: 4 points each off the total
    pub fault_cups: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct CuppingAttribute {
    pub id: String,
    pub cupping_id: String,
    /// `fragrance_aroma`, `flavor`, `aftertaste`, `acidity`, `body`, `balance`, `uniformity`,
    /// `clean_cup`, `sweetness` or `overall`
    pub attribute_name: String,
    pub score: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CuppingWithAttributes {
    #[serde(flatten)]
    pub cupping: CuppingScore,
    /// The ten attributes' sum; null until all ten are scored
    pub total_score: Option<f32>,
    /// Points off for defects: 2 per tainted cup, 4 per faulty cup
    pub defects: i64,
    /// The final score's band: `outstanding` (90+), `excellent` (85+), `very good` (80+),
    /// `below specialty`; null until all ten are scored
    pub band: Option<String>,
    /// In the form's order
    pub attributes: Vec<CuppingAttribute>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateCuppingAttributeRequest {
    /// An attribute's key or another name for it (`aroma`, `flavour`, `clean`)
    pub name: String,
    /// Quality attributes 0–10 in quarter points (the form's scale is 6–9.75); uniformity,
    /// clean cup and sweetness 2 a cup over five cups: 0, 2, 4, 6, 8 or 10
    pub score: f32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateCuppingRequest {
    /// `sca`, the only form recorded (the default)
    pub scoring_framework: Option<String>,
    pub notes: Option<String>,
    /// Some or all of the ten; the total and final score come once all ten are scored
    pub attributes: Vec<CreateCuppingAttributeRequest>,
    /// Cups with a taint (0–5, default 0)
    #[serde(default)]
    pub taint_cups: i64,
    /// Cups with a fault (0–5, default 0)
    #[serde(default)]
    pub fault_cups: i64,
}
