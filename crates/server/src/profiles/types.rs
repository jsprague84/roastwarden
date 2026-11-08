//! A roast profile's stored and API types: its points, events, background curves, and the
//! requests that create, save and import one.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::http::patch::double_option;
use crate::roasts::types::RoastEventType;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct RoastProfile {
    pub id: String, // UUID as string
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_public: bool,

    // Profile settings
    pub target_total_time: Option<i32>,  // seconds
    pub target_first_crack: Option<i32>, // seconds from start
    pub target_end_temp: Option<f32>,
    pub preheat_temp: Option<f32>,
    pub charge_temp: Option<f32>,

    // What it was made for (Artisan's beans, weight in, roastertype).
    #[serde(default)]
    pub beans: Option<String>,
    #[serde(default)]
    pub batch_size_g: Option<f32>,
    #[serde(default)]
    pub roaster: Option<String>,

    // Roast guidance (migration 019, docs/roast-guidance.md).
    /// SCA colour tile key: `extremely_light` … `dark`.
    #[serde(default)]
    pub roast_level: Option<String>,
    /// `espresso` | `filter` | `omni` | `other`.
    #[serde(default)]
    pub roast_purpose: Option<String>,
    /// Overrides of the level's expected ranges: `{"weight_loss_pct": [14, 16], …}`.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub expectations: Option<sqlx::types::Json<serde_json::Value>>,
    /// Auto DROP (migration 041, `docs/replay.md`): `time`, `bt` or `development`; none is
    /// off. A drive marks DROP by itself at the profile's drop, after a countdown.
    #[serde(default)]
    pub auto_drop: Option<String>,
    /// The roaster kind it was written for (migration 051, `docs/profiles.md`): one of the
    /// setup kinds (`fluid_bed_electric`, `drum_electric`, `drum_gas`, `logger`, `other`);
    /// none fits any. The library lists the selected roaster's kind first.
    #[serde(default)]
    pub roaster_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct ProfilePoint {
    pub id: String,
    pub profile_id: String,
    pub time_seconds: i32,
    pub target_temp: f32,
    pub fan_speed: Option<i32>, // 0-100
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub target_env_temp: Option<f32>,
    /// Heater feedforward, 0-100 %.
    pub heater_pct: Option<i32>,
}

/// Which reference a profile event fires on by default.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Seconds from CHARGE (`at_seconds`).
    Time,
    /// Bean temperature in °C on the way up (`at_bt`).
    Bt,
}

/// What a profile event does when it fires.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ProfileAction {
    /// `value` = setpoint °C.
    Setpoint,
    /// `value` = fan %.
    Fan,
    /// `value` = heater %.
    Heater,
    /// `label` = `auto` or `manual`.
    Mode,
    /// `label` = a roast event type such as `first_crack_start`.
    Landmark,
    /// `label` = note text.
    Note,
}

/// A replayable profile event (Artisan's "replay events by time / by BT").
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct ProfileEvent {
    pub id: String,
    pub profile_id: String,
    pub trigger_kind: TriggerKind,
    pub at_seconds: Option<f64>,
    pub at_bt: Option<f64>,
    pub action: ProfileAction,
    pub value: Option<f64>,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct CreateProfileEventRequest {
    pub trigger_kind: Option<TriggerKind>,
    #[serde(default)]
    pub at_seconds: Option<f64>,
    #[serde(default)]
    pub at_bt: Option<f64>,
    pub action: Option<ProfileAction>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub label: Option<String>,
}

/// A validated event, ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidProfileEvent {
    pub trigger_kind: TriggerKind,
    pub at_seconds: Option<f64>,
    pub at_bt: Option<f64>,
    pub action: ProfileAction,
    pub value: Option<f64>,
    pub label: Option<String>,
}

impl CreateProfileEventRequest {
    /// Check ranges and the fields each action needs.
    pub fn validate(&self) -> Result<ValidProfileEvent, String> {
        let action = self.action.ok_or("event needs an action")?;
        let at_seconds = match self.at_seconds {
            Some(t) if !t.is_finite() || !(0.0..=7200.0).contains(&t) => {
                return Err(format!("at_seconds {t} is outside 0..=7200"))
            }
            other => other,
        };
        let at_bt = match self.at_bt {
            Some(t) if !t.is_finite() || !(0.0..=350.0).contains(&t) => {
                return Err(format!("at_bt {t} is outside 0..=350 °C"))
            }
            other => other,
        };
        let trigger_kind = match self.trigger_kind {
            Some(k) => k,
            None if at_seconds.is_some() => TriggerKind::Time,
            None if at_bt.is_some() => TriggerKind::Bt,
            None => return Err("event needs at_seconds or at_bt".into()),
        };
        match trigger_kind {
            TriggerKind::Time if at_seconds.is_none() => {
                return Err("a time-triggered event needs at_seconds".into())
            }
            TriggerKind::Bt if at_bt.is_none() => {
                return Err("a BT-triggered event needs at_bt".into())
            }
            _ => {}
        }
        let label = self
            .label
            .as_ref()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        let numeric = |lo: f64, hi: f64, what: &str| -> Result<Option<f64>, String> {
            match self.value {
                Some(v) if v.is_finite() && (lo..=hi).contains(&v) => Ok(Some(v)),
                Some(v) => Err(format!("{what} {v} is outside {lo}..={hi}")),
                None => Err(format!("{what} event needs a value")),
            }
        };
        let (value, label) = match action {
            ProfileAction::Setpoint => (numeric(0.0, 300.0, "setpoint")?, label),
            ProfileAction::Fan => (numeric(0.0, 100.0, "fan")?, label),
            ProfileAction::Heater => (numeric(0.0, 100.0, "heater")?, label),
            ProfileAction::Mode => match label.as_deref().map(str::to_ascii_lowercase) {
                Some(m) if m == "auto" || m == "manual" => (None, Some(m)),
                _ => return Err("mode event needs label auto or manual".into()),
            },
            ProfileAction::Landmark => {
                let l = label.ok_or("landmark event needs an event type label")?;
                match l.parse::<RoastEventType>() {
                    Ok(RoastEventType::Custom) | Err(_) => {
                        return Err(format!("landmark label {l:?} is not a roast event type"))
                    }
                    Ok(t) => (None, Some(t.to_string())),
                }
            }
            ProfileAction::Note => {
                let l = label.ok_or("note event needs text in label")?;
                (None, Some(l))
            }
        };
        Ok(ValidProfileEvent {
            trigger_kind,
            at_seconds,
            at_bt,
            action,
            value,
            label,
        })
    }
}

/// Background curves for a profile: one value per `interval_seconds` from
/// CHARGE, `null` where the source had no data. Temperatures in °C, RoR in
/// °C/min, heater and fan in %.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct ProfileCurves {
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default = "one")]
    pub interval_seconds: f64,
    pub bt: Vec<Option<f32>>,
    #[serde(default)]
    pub et: Vec<Option<f32>>,
    #[serde(default)]
    pub ror: Vec<Option<f32>>,
    #[serde(default)]
    pub heater: Vec<Option<f32>>,
    #[serde(default)]
    pub fan: Vec<Option<f32>>,
}

fn one() -> f64 {
    1.0
}

/// Longest curve accepted: two hours at 1 s.
pub const MAX_CURVE_LEN: usize = 7200;

impl ProfileCurves {
    pub fn validate(&self) -> Result<(), String> {
        if !self.interval_seconds.is_finite() || !(0.1..=60.0).contains(&self.interval_seconds) {
            return Err("curves interval_seconds must be within 0.1..=60".into());
        }
        if self.bt.is_empty() {
            return Err("curves need a bt array".into());
        }
        for (name, arr) in [
            ("bt", &self.bt),
            ("et", &self.et),
            ("ror", &self.ror),
            ("heater", &self.heater),
            ("fan", &self.fan),
        ] {
            if arr.len() > MAX_CURVE_LEN {
                return Err(format!(
                    "curve {name} is longer than {MAX_CURVE_LEN} samples"
                ));
            }
            if !arr.is_empty() && arr.len() != self.bt.len() {
                return Err(format!("curve {name} length differs from bt"));
            }
            if arr.iter().flatten().any(|v| !v.is_finite()) {
                return Err(format!("curve {name} has a non-finite value"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct CreateProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub target_total_time: Option<i32>,
    pub target_first_crack: Option<i32>,
    pub target_end_temp: Option<f32>,
    pub preheat_temp: Option<f32>,
    pub charge_temp: Option<f32>,
    #[serde(default)]
    pub beans: Option<String>,
    #[serde(default)]
    pub batch_size_g: Option<f32>,
    #[serde(default)]
    pub roaster: Option<String>,
    /// Roast guidance. On update, absent keeps the stored value, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    pub roast_level: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub roast_purpose: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub expectations: Option<Option<serde_json::Value>>,
    /// Auto DROP: `time`, `bt`, `development`, or `null` (off). On update, absent keeps it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub auto_drop: Option<Option<String>>,
    /// The roaster kind it is for (a setup kind), or `null` for any. On update, absent keeps it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub roaster_kind: Option<Option<String>>,
    pub points: Vec<CreateProfilePointRequest>,
    /// On update, absent keeps the stored events; a list replaces them.
    #[serde(default)]
    pub events: Option<Vec<CreateProfileEventRequest>>,
    /// On update, absent keeps the stored curves, `null` removes them.
    #[serde(default, deserialize_with = "double_option")]
    pub curves: Option<Option<ProfileCurves>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct CreateProfilePointRequest {
    pub time_seconds: i32,
    pub target_temp: f32,
    #[serde(default)]
    pub fan_speed: Option<i32>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub target_env_temp: Option<f32>,
    #[serde(default)]
    pub heater_pct: Option<i32>,
}

/// `POST /api/sessions/{id}/profile`: save a finished roast as a profile.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct SaveSessionAsProfileRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ImportArtisanProfileRequest {
    pub alog_content: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProfileWithPoints {
    #[serde(flatten)]
    pub profile: RoastProfile,
    pub points: Vec<ProfilePoint>,
    pub events: Vec<ProfileEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curves: Option<ProfileCurves>,
}
