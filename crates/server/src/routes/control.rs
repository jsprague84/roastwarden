//! Direct control of a roaster (docs/control.md): setpoint, fan, heater, drum, damper, mode,
//! heater power, PID gains, the RoR limiter and the emergency stop. Each command goes through the
//! transport registry; a manual change takes its channel from a running profile drive.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::openapi::CommandResponses;
use crate::replay::engine::Channel;
use crate::{
    app::dispatch::dispatch, app::dispatch::PublishOpts, app::state::AppState, control::tuning,
};
use roastwarden_core::{ControlMode, DeviceCommand};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_set_setpoint))
        .routes(routes!(api_set_fan))
        .routes(routes!(api_set_heater))
        .routes(routes!(api_set_drum))
        .routes(routes!(api_set_damper))
        .routes(routes!(api_set_mode))
        .routes(routes!(api_set_heater_enable))
        .routes(routes!(api_set_pid))
        .routes(routes!(api_set_ror_limit))
        .routes(routes!(api_emergency_stop))
}

#[derive(Deserialize, Serialize, ToSchema)]
struct SetpointPayload {
    value: f64,
}

/// Heater and fan commands: 0..=100 %.
#[derive(Deserialize, Serialize, ToSchema)]
struct PercentPayload {
    percent: f64,
}

impl PercentPayload {
    fn valid(&self) -> bool {
        self.percent.is_finite() && (0.0..=100.0).contains(&self.percent)
    }
}

fn bad_percent() -> Response {
    (StatusCode::BAD_REQUEST, "percent must be 0..100").into_response()
}

#[derive(Deserialize, Serialize, ToSchema)]
struct ModePayload {
    mode: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
struct EnablePayload {
    enabled: bool,
}

#[derive(Deserialize, Serialize, ToSchema)]
struct PidPayload {
    kp: f64,
    ki: f64,
    kd: f64,
    /// `false`: switch gains without writing the device's NVS.
    #[serde(default)]
    persist: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
struct RorLimitPayload {
    /// °C/min; 0 turns the limiter off.
    max: f64,
    /// BT at which the limit starts to apply.
    #[serde(default = "default_ror_min_temp")]
    min_temp: f64,
}

fn default_ror_min_temp() -> f64 {
    150.0
}

/// The BT setpoint, °C (0–300). Takes the setpoint channel from a running profile drive.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/setpoint", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = SetpointPayload, responses(CommandResponses))]
async fn api_set_setpoint(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<SetpointPayload>,
) -> Response {
    // Basic validation range 0..300 C
    if !(0.0..=300.0).contains(&body.value) {
        return (
            StatusCode::BAD_REQUEST,
            "setpoint must be between 0 and 300 C",
        )
            .into_response();
    }
    state
        .replay
        .manual_override(
            &device_id,
            &[Channel::Setpoint],
            "setpoint",
            Some(body.value),
        )
        .await;
    dispatch(
        &state,
        &device_id,
        DeviceCommand::Setpoint { value: body.value },
        &opts,
    )
    .await
}

/// Fan, 0–100 %. Takes the fan channel from a running drive (or sets fan assist's ceiling);
/// the gain schedule follows.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/fan", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = PercentPayload, responses(CommandResponses))]
async fn api_set_fan(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<PercentPayload>,
) -> Response {
    if !body.valid() {
        return bad_percent();
    }
    set_fan(&state, &device_id, body.percent, &opts).await
}

/// A manual fan change: overrides a replay's fan channel (or becomes the
/// fan assist's new ceiling), sends it, then switches to the scheduled gains
/// for the new fan.
pub(crate) async fn set_fan(
    state: &AppState,
    device_id: &str,
    percent: f64,
    opts: &PublishOpts,
) -> Response {
    state
        .replay
        .manual_override(device_id, &[Channel::Fan], "fan", Some(percent))
        .await;
    let res = dispatch(state, device_id, DeviceCommand::Fan { percent }, opts).await;
    if res.status().is_success() {
        state.fan_assist.operator_fan(device_id, percent).await;
        state.gain_scheduler.fan_changed(device_id, percent).await;
    }
    res
}

/// Manual heater power, 0–100 %.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/heater", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = PercentPayload, responses(CommandResponses))]
async fn api_set_heater(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<PercentPayload>,
) -> Response {
    if !body.valid() {
        return bad_percent();
    }
    state
        .replay
        .manual_override(&device_id, &[Channel::Heater], "heater", None)
        .await;
    dispatch(
        &state,
        &device_id,
        DeviceCommand::Heater {
            percent: body.percent,
        },
        &opts,
    )
    .await
}

/// Drum speed, 0–100 % (a drum roaster's; docs/ingest-mapping.md, "Drum and damper"). Sent
/// only when the roaster's setup declares a drum output (409 `OUTPUT_NOT_DECLARED` otherwise),
/// shaped to its range; manual or full control.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/drum", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = PercentPayload, responses(CommandResponses))]
async fn api_set_drum(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<PercentPayload>,
) -> Response {
    if !body.valid() {
        return bad_percent();
    }
    dispatch(
        &state,
        &device_id,
        DeviceCommand::Drum {
            percent: body.percent,
        },
        &opts,
    )
    .await
}

/// Damper position, 0–100 %, as the drum: only when the setup declares a damper output.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/damper", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = PercentPayload, responses(CommandResponses))]
async fn api_set_damper(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<PercentPayload>,
) -> Response {
    if !body.valid() {
        return bad_percent();
    }
    dispatch(
        &state,
        &device_id,
        DeviceCommand::Damper {
            percent: body.percent,
        },
        &opts,
    )
    .await
}

/// `auto` (the roaster's PID holds the setpoint) or `manual` (heater %).
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/mode", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = ModePayload, responses(CommandResponses))]
async fn api_set_mode(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<ModePayload>,
) -> Response {
    let Ok(mode) = body.mode.parse::<ControlMode>() else {
        return (StatusCode::BAD_REQUEST, "mode must be 'auto' or 'manual'").into_response();
    };
    state
        .replay
        .manual_override(
            &device_id,
            &[Channel::Setpoint, Channel::Heater],
            "mode",
            None,
        )
        .await;
    dispatch(&state, &device_id, DeviceCommand::Mode { mode }, &opts).await
}

/// Heater power on or off (the roaster's own interlock still applies).
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/heater_enable", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = EnablePayload, responses(CommandResponses))]
async fn api_set_heater_enable(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<EnablePayload>,
) -> Response {
    dispatch(
        &state,
        &device_id,
        DeviceCommand::HeaterEnable {
            enabled: body.enabled,
        },
        &opts,
    )
    .await
}

/// PID gains. Stored ones (the default) are recorded and can be undone.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/pid", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = PidPayload, responses(CommandResponses))]
async fn api_set_pid(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<PidPayload>,
) -> Response {
    let gains = tuning::Gains {
        kp: body.kp,
        ki: body.ki,
        kd: body.kd,
    };
    if !gains.is_valid() {
        return (StatusCode::BAD_REQUEST, "gains must be finite and >= 0").into_response();
    }
    if body.persist.unwrap_or(true) {
        // Stored changes are recorded so they can be undone.
        return crate::routes::tuning::store_gains(
            &state, &device_id, gains, "manual", None, None, &opts,
        )
        .await;
    }
    dispatch(
        &state,
        &device_id,
        DeviceCommand::Pid {
            kp: body.kp,
            ki: body.ki,
            kd: body.kd,
            persist: false,
        },
        &opts,
    )
    .await
}

/// The roaster's RoR limiter: at most `max` °C/min once BT passes `min_temp`. Only for a
/// roaster with one (cap `ror`); a roaster that lists its caps without it gets a 409.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/ror_limit", tag = "control",
    params(("device_id" = String, Path), PublishOpts), request_body = RorLimitPayload,
    responses(CommandResponses, (status = 409, description = "The roaster has no RoR limiter", body = crate::http::errors::ErrorBody)))]
async fn api_set_ror_limit(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
    Json(body): Json<RorLimitPayload>,
) -> Response {
    if !(0.0..=200.0).contains(&body.max) || !(0.0..=250.0).contains(&body.min_temp) {
        return (
            StatusCode::BAD_REQUEST,
            "max must be 0-200 C/min (0 = off), min_temp 0-250 C",
        )
            .into_response();
    }
    if state
        .replay
        .reported_caps(&device_id)
        .is_some_and(|c| !c.ror)
    {
        return (
            StatusCode::CONFLICT,
            "this roaster has no RoR limiter (it does not list cap `ror`; firmware 3.4.1 dropped it): Profile drive shapes the RoR",
        )
            .into_response();
    }
    dispatch(
        &state,
        &device_id,
        DeviceCommand::RorLimit {
            max: body.max,
            min_temp: body.min_temp,
        },
        &opts,
    )
    .await
}

/// Heater off at once, then the drive stops.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/emergency_stop", tag = "control",
    params(("device_id" = String, Path), PublishOpts), responses(CommandResponses))]
async fn api_emergency_stop(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(opts): Query<PublishOpts>,
) -> Response {
    // Stop the device first; stopping the replay then releases its lease.
    let res = dispatch(&state, &device_id, DeviceCommand::EmergencyStop, &opts).await;
    state
        .replay
        .stop_for_device(&device_id, "emergency stop")
        .await;
    res
}
