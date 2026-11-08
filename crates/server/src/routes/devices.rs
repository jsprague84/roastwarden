use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use tracing::{error, info, warn};
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::roasters::types::{
    CreateConnectionRequest, CreateDeviceProfileRequest, CreateDeviceRequest,
    CreateRegisterMapEntry, Device, DeviceConnection, DeviceProfile, DeviceStatus,
    DeviceWithConnections, ModbusRegisterMap, Protocol, TestConnectionRequest,
    TestConnectionResponse, UpdateConnectionRequest, UpdateDeviceProfileRequest,
    UpdateDeviceRequest, PASSWORD_MASK,
};
use crate::transports::types::WebSocketConnectionConfig;

// ============================================================================
// AppError: a status and a message, which `errors::json_errors` turns into the one error
// body like every other handler's
// ============================================================================

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn not_found(entity: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: format!("{entity} not found"),
        }
    }

    fn bad_request(msg: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.to_string(),
        }
    }

    fn conflict(msg: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: msg.to_string(),
        }
    }

    fn internal(msg: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        error!(?err, "Internal error");
        Self::internal(err)
    }
}

// ============================================================================
// Query parameters
// ============================================================================

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeviceListQuery {
    /// `active`, `inactive`, `pending` or `error`
    pub status: Option<String>,
}

// ============================================================================
// Route builder
// ============================================================================

/// The configured devices (Roasters → the roaster → Connection), their device profiles,
/// connections and Modbus register maps.
pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Discovered devices (auto-created with status 'pending')
        .routes(routes!(list_discovered_devices))
        .routes(routes!(list_devices, create_device))
        .routes(routes!(get_device, update_device, delete_device))
        .routes(routes!(list_device_profiles, create_device_profile))
        .routes(routes!(
            get_device_profile,
            update_device_profile,
            delete_device_profile
        ))
        .routes(routes!(add_connection))
        .routes(routes!(update_connection, remove_connection))
        .routes(routes!(get_register_map, set_register_map))
        .routes(routes!(test_connection))
        .routes(routes!(get_setup, put_setup))
        .routes(routes!(list_artisan_machines))
        .routes(routes!(plan_artisan_machine))
        .routes(routes!(artisan_screen_aset))
}

// ============================================================================
// Artisan machine setups (docs/artisan-machines.md)
// ============================================================================

/// Artisan's machine catalogue (its `.aset` files, embedded): each machine, the Artisan device
/// it is read through, and whether Roastwarden can set it up from it (and why not).
#[utoipa::path(get, path = "/api/artisan/machines", tag = "devices",
    responses((status = 200, body = Vec<crate::roasters::aset::Machine>)))]
async fn list_artisan_machines() -> Json<Vec<crate::roasters::aset::Machine>> {
    Json(crate::roasters::aset::machines())
}

/// Where Artisan finds the Modbus slave.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ScreenAsetQuery {
    /// The server's address as Artisan reaches it (name or IP); default: the host this
    /// request was sent to
    pub host: Option<String>,
    /// Artisan's temperature unit, `C` (default) or `F`: loading the file sets it
    pub unit: Option<String>,
}

/// An Artisan setup (`.aset`) that reads this server's Modbus slave: BT, ET, heater and fan
/// of the roaster it serves, at the slave's port; with the Air and Burner sliders when writes
/// are on. Load it in Artisan with Help → Load Settings (docs/artisan-screen.md). 404 while
/// the slave is off (`ROASTWARDEN_MODBUS_ADDR`).
#[utoipa::path(get, path = "/api/artisan/screen.aset", tag = "devices", params(ScreenAsetQuery),
    responses((status = 200, content_type = "text/plain", body = String),
        (status = 400, description = "A host that is not a name or an address, or a unit not C or F",
            body = ErrorBody),
        (status = 404, description = "The Modbus slave is off", body = ErrorBody)))]
async fn artisan_screen_aset(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<ScreenAsetQuery>,
) -> Result<Response, AppError> {
    let screen = state.modbus.as_ref().ok_or_else(|| AppError {
        status: StatusCode::NOT_FOUND,
        message: "the Modbus slave is off: set ROASTWARDEN_MODBUS_ADDR (docs/artisan-screen.md)"
            .into(),
    })?;
    let host = match q.host.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
        Some(h) => h.to_string(),
        None => headers
            .get(axum::http::header::HOST)
            .and_then(|h| h.to_str().ok())
            .map_or_else(|| "127.0.0.1".into(), crate::transports::modbus::host_of),
    };
    if !crate::transports::modbus::plain_host(&host) {
        return Err(AppError::bad_request(
            "host: a name or an IP address, without a port or a scheme",
        ));
    }
    let fahrenheit = match q.unit.as_deref().map(str::trim) {
        None | Some("" | "C" | "c") => false,
        Some("F" | "f") => true,
        Some(_) => return Err(AppError::bad_request("unit: C or F")),
    };
    let body =
        crate::transports::modbus::aset(&host, screen.addr.port(), screen.writes, fahrenheit);
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"Roastwarden.aset\"",
            ),
        ],
        body,
    )
        .into_response())
}

/// Which machine to plan: one from the catalogue, or a setup file's text.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PlanMachineRequest {
    /// A catalogue machine, `Brand/Model` (`GET /api/artisan/machines`'s `key`)
    #[serde(default)]
    pub machine: Option<String>,
    /// An `.aset` file's text (Artisan → Help → Save Settings, or the maker's file)
    #[serde(default)]
    pub aset: Option<String>,
    /// The file's name, for the plan's machine key when `aset` is given
    #[serde(default)]
    pub file_name: Option<String>,
}

/// Plan a roaster from an Artisan machine setup: its connection, register map (roles, each
/// register's slave), outputs from the sliders and channels from the extra curves, at control
/// off, with notes on what was guessed or left out. Nothing is saved: `roast roaster add
/// --artisan` and the dashboard's Add roaster apply the plan through the normal calls.
#[utoipa::path(post, path = "/api/artisan/machines/plan", tag = "devices",
    request_body = PlanMachineRequest,
    responses((status = 200, body = crate::roasters::aset::Plan),
        (status = 400, description = "Neither or both of machine and aset, or too big",
            body = ErrorBody),
        (status = 404, description = "No such machine in the catalogue", body = ErrorBody)))]
async fn plan_artisan_machine(
    Json(req): Json<PlanMachineRequest>,
) -> Result<Json<crate::roasters::aset::Plan>, AppError> {
    match (req.machine.as_deref(), req.aset.as_deref()) {
        (Some(m), None) => {
            let plan = crate::roasters::aset::plan_machine(m).ok_or_else(|| AppError {
                status: StatusCode::NOT_FOUND,
                message: format!(
                    "no machine '{m}' in Artisan's catalogue (GET /api/artisan/machines lists \
                     them as Brand/Model)"
                ),
            })?;
            Ok(Json(plan))
        }
        (None, Some(text)) => {
            if text.len() > 1024 * 1024 {
                return Err(AppError::bad_request(
                    "the setup is over 1 MB; an Artisan .aset is a few hundred KB at most",
                ));
            }
            if !text.contains("[Device]") {
                return Err(AppError::bad_request(
                    "this is not an Artisan setup: it has no [Device] section",
                ));
            }
            let name = req
                .file_name
                .as_deref()
                .map(|f| f.trim().trim_end_matches(".aset"))
                .filter(|f| !f.is_empty())
                .unwrap_or("uploaded setup");
            Ok(Json(crate::roasters::aset::plan(text, name)))
        }
        _ => Err(AppError::bad_request(
            "give machine (Brand/Model from the catalogue) or aset (a file's text), one of them",
        )),
    }
}

// ============================================================================
// Roaster setup: kind and control level (docs/roaster-setup.md)
// ============================================================================

/// The roaster's setup: its kind and what Roastwarden may send it (`off` logs only, `manual`,
/// `full`). A roaster with no setup is `off`.
#[utoipa::path(get, path = "/api/roaster/{device_id}/setup", tag = "devices",
    params(("device_id" = String, Path)),
    responses((status = 200, body = crate::roasters::setup::RoasterSetup)))]
async fn get_setup(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    Json(state.transports.setups().get(&device_id).await).into_response()
}

/// Set the roaster's kind, control level, outputs, fan floor and channels. A token (admin)
/// whose `PUT` leaves control `manual` or `full` waits for the owner's approval (202, docs/
/// control.md "Approvals"); one that leaves control off runs at once. Changing the kind writes
/// its defaults: outputs and floor (unless the body gives them), the roast sequence and fan
/// assist (docs/roaster-setup.md).
#[utoipa::path(put, path = "/api/roaster/{device_id}/setup", tag = "devices",
    params(("device_id" = String, Path)), request_body = crate::roasters::setup::SetupChange,
    responses((status = 200, body = crate::roasters::setup::RoasterSetup),
        (status = 202, description = "A token switching control on: held for the owner's approval", body = crate::auth::approvals::Held),
        (status = 400, description = "The virtual roaster's setup is fixed, or a bad output range", body = ErrorBody),
        (status = 403, description = "A token leaving control on without the owner's approval", body = ErrorBody)))]
async fn put_setup(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    actor: Option<axum::Extension<crate::auth::Actor>>,
    approved: Option<axum::Extension<crate::auth::approvals::ApprovedCall>>,
    Json(b): Json<crate::roasters::setup::SetupChange>,
) -> Response {
    use crate::roasters::setup::{Control, SetupError};
    // A token may set up a roaster at control off at once: that sends nothing. Leaving
    // control on is the owner's yes: the auth layer holds it for approval and makes the call
    // again as the token once approved (docs/roaster-setup.md, "Tokens and the setup"). This
    // refuses the call should it come here unapproved.
    if let Some(axum::Extension(crate::auth::Actor::Token(_))) = &actor {
        if b.control != Control::Off && approved.is_none() {
            let mut r = (
                StatusCode::FORBIDDEN,
                format!(
                    "a token switching {device_id}'s control on needs the owner's approval; \
                     the owner sets control under Roasters → {device_id} → Setup"
                ),
            )
                .into_response();
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode("OWNER_ONLY"));
            return r;
        }
    }
    let (setup, kind_changed) = match state.transports.setups().set(&device_id, b).await {
        Ok(r) => r,
        Err(SetupError::Invalid(m)) => return (StatusCode::BAD_REQUEST, m).into_response(),
        Err(SetupError::Db(e)) => return AppError::from(anyhow::Error::from(e)).into_response(),
    };
    if kind_changed {
        // A new kind brings its roast sequence and fan assist (a drum's records only).
        let seq = crate::control::sequence::SequenceConfig::for_kind(setup.kind);
        if let Err(e) = state.sequence.set_config(&device_id, &seq).await {
            tracing::warn!(device_id, error = %e, "kind's sequence defaults not saved");
        }
        let fa = crate::control::fan_assist::FanAssistConfig::default();
        if let Err(e) = state.fan_assist.set_config(&device_id, fa).await {
            tracing::warn!(device_id, error = %e, "kind's fan assist defaults not saved");
        }
    }
    info!(
        device_id,
        control = setup.control.as_str(),
        kind = setup.kind.as_str(),
        kind_changed,
        "roaster setup set"
    );
    Json(setup).into_response()
}

// ============================================================================
// Device CRUD handlers
// ============================================================================

/// The configured devices.
#[utoipa::path(get, path = "/api/devices", tag = "devices", params(DeviceListQuery),
    responses((status = 200, body = Vec<Device>)))]
async fn list_devices(
    State(state): State<AppState>,
    Query(q): Query<DeviceListQuery>,
    actor: Option<axum::Extension<crate::auth::Actor>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<Device>>, AppError> {
    let status_filter = q.status.and_then(|s| s.parse::<DeviceStatus>().ok());
    let mut devices = crate::roasters::registry::list_devices(&state.db, status_filter).await?;
    // A demo visitor sees their own simulated roaster and the shared one, not the others'
    // (docs/DEPLOY.md, "A simulated roaster per visitor").
    if matches!(actor.map(|a| a.0), Some(crate::auth::Actor::Demo)) {
        let own = crate::ops::demo::own_roaster(&headers);
        devices.retain(|d| {
            !crate::ops::demo::is_visitor_roaster(&d.device_id)
                || own.as_deref() == Some(d.device_id.as_str())
        });
    }
    Ok(Json(devices))
}

/// Devices that announced themselves and wait to be set up (status `pending`).
#[utoipa::path(get, path = "/api/devices/discovered", tag = "devices",
    responses((status = 200, body = Vec<Device>)))]
async fn list_discovered_devices(
    State(state): State<AppState>,
) -> Result<Json<Vec<Device>>, AppError> {
    let devices =
        crate::roasters::registry::list_devices(&state.db, Some(DeviceStatus::Pending)).await?;
    Ok(Json(devices))
}

/// A device with its connections, by registry id or by roaster `device_id`.
#[utoipa::path(get, path = "/api/devices/{id}", tag = "devices", params(("id" = String, Path)),
    responses((status = 200, body = DeviceWithConnections), (status = 404, body = ErrorBody)))]
async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeviceWithConnections>, AppError> {
    // The registry id, or the roaster's device_id (so /roasters/virtual_roaster/… works).
    let device = match crate::roasters::registry::get_device(&state.db, &id).await? {
        Some(d) => Some(d),
        None => crate::roasters::registry::get_device_by_device_id(&state.db, &id).await?,
    }
    .ok_or_else(|| AppError::not_found("Device"))?;
    Ok(Json(device))
}

/// Add a device.
#[utoipa::path(post, path = "/api/devices", tag = "devices", request_body = CreateDeviceRequest,
    responses((status = 201, body = Device),
        (status = 409, description = "A roaster with this device id exists", body = ErrorBody)))]
async fn create_device(
    State(state): State<AppState>,
    Json(req): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<Device>), AppError> {
    // A device that streamed before it was added (MQTT auto-discovery) already has its row.
    if let Some(d) =
        crate::roasters::registry::get_device_by_device_id(&state.db, &req.device_id).await?
    {
        return Err(AppError::conflict(format!(
            "{} exists already ({}): set it up with PUT /api/devices/{} rather than add it again",
            req.device_id,
            if d.device.status == DeviceStatus::Pending {
                "found, waiting to be set up"
            } else {
                "added"
            },
            d.device.id
        )));
    }
    let device = crate::roasters::registry::create_device(&state.db, req).await?;
    Ok((StatusCode::CREATED, Json(device)))
}

/// Edit a device.
#[utoipa::path(put, path = "/api/devices/{id}", tag = "devices", params(("id" = String, Path)),
    request_body = UpdateDeviceRequest,
    responses((status = 200, body = Device), (status = 404, body = ErrorBody)))]
async fn update_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeviceRequest>,
) -> Result<Json<Device>, AppError> {
    let device = crate::roasters::registry::update_device(&state.db, &id, req)
        .await?
        .ok_or_else(|| AppError::not_found("Device"))?;
    // Active or not decides whether its connections are polled.
    state.pollers.sync_device(&device.id).await;
    Ok(Json(device))
}

/// Remove a device and its connections.
#[utoipa::path(delete, path = "/api/devices/{id}", tag = "devices", params(("id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn delete_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let deleted = crate::roasters::registry::delete_device(&state.db, &id).await?;
    if deleted {
        state.pollers.sync_device(&id).await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Device"))
    }
}

// ============================================================================
// Device Profile CRUD handlers
// ============================================================================

/// The device profiles (defaults a device starts from).
#[utoipa::path(get, path = "/api/device-profiles", tag = "devices",
    responses((status = 200, body = Vec<DeviceProfile>)))]
async fn list_device_profiles(
    State(state): State<AppState>,
) -> Result<Json<Vec<DeviceProfile>>, AppError> {
    let profiles = crate::roasters::registry::list_device_profiles(&state.db).await?;
    Ok(Json(profiles))
}

/// One device profile.
#[utoipa::path(get, path = "/api/device-profiles/{id}", tag = "devices",
    params(("id" = String, Path)),
    responses((status = 200, body = DeviceProfile), (status = 404, body = ErrorBody)))]
async fn get_device_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeviceProfile>, AppError> {
    let profile = crate::roasters::registry::get_device_profile(&state.db, &id)
        .await?
        .ok_or_else(|| AppError::not_found("Device profile"))?;
    Ok(Json(profile))
}

/// Add a device profile.
#[utoipa::path(post, path = "/api/device-profiles", tag = "devices",
    request_body = CreateDeviceProfileRequest, responses((status = 201, body = DeviceProfile)))]
async fn create_device_profile(
    State(state): State<AppState>,
    Json(req): Json<CreateDeviceProfileRequest>,
) -> Result<(StatusCode, Json<DeviceProfile>), AppError> {
    let profile = crate::roasters::registry::create_device_profile(&state.db, req).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}

/// Edit a device profile.
#[utoipa::path(put, path = "/api/device-profiles/{id}", tag = "devices",
    params(("id" = String, Path)), request_body = UpdateDeviceProfileRequest,
    responses((status = 200, body = DeviceProfile), (status = 404, body = ErrorBody)))]
async fn update_device_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDeviceProfileRequest>,
) -> Result<Json<DeviceProfile>, AppError> {
    let profile = crate::roasters::registry::update_device_profile(&state.db, &id, req)
        .await?
        .ok_or_else(|| AppError::not_found("Device profile"))?;
    Ok(Json(profile))
}

/// Remove a device profile.
#[utoipa::path(delete, path = "/api/device-profiles/{id}", tag = "devices",
    params(("id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn delete_device_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let deleted = crate::roasters::registry::delete_device_profile(&state.db, &id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Device profile"))
    }
}

// ============================================================================
// Device Connection handlers
// ============================================================================

/// Add a transport (MQTT, WebSocket, Modbus TCP or RTU, serial TC4) to a device. Its config is
/// checked for the protocol (a 400 says what is wrong); an enabled connection of an active
/// device starts polling at once.
#[utoipa::path(post, path = "/api/devices/{id}/connections", tag = "devices",
    params(("id" = String, Path)), request_body = CreateConnectionRequest,
    responses((status = 201, body = DeviceConnection),
        (status = 400, description = "The config does not fit the protocol", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn add_connection(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(req): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<DeviceConnection>), AppError> {
    if req.config.get("password").and_then(|p| p.as_str()) == Some(PASSWORD_MASK) {
        return Err(AppError::bad_request(format!(
            "password: '{PASSWORD_MASK}' stands for a saved password, and this connection has \
             none yet: type the password"
        )));
    }
    // Verify device exists
    crate::roasters::registry::get_device(&state.db, &device_id)
        .await?
        .ok_or_else(|| AppError::not_found("Device"))?;
    crate::transports::pollers::check_connection_config(&req.protocol, &req.config)
        .map_err(AppError::bad_request)?;
    check_mqtt_claims(&state, &device_id, &req.protocol, &req.config).await?;

    let connection = crate::roasters::registry::add_connection(&state.db, &device_id, req).await?;
    state.pollers.sync_device(&device_id).await;
    Ok((StatusCode::CREATED, Json(connection)))
}

/// Edit a connection: enable or disable it, its priority, its config (checked as when it was
/// added). Its poller restarts, starts or stops at once.
#[utoipa::path(put, path = "/api/devices/{id}/connections/{conn_id}", tag = "devices",
    params(("id" = String, Path), ("conn_id" = String, Path)),
    request_body = UpdateConnectionRequest,
    responses((status = 200, body = DeviceConnection),
        (status = 400, description = "The config does not fit the protocol", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn update_connection(
    State(state): State<AppState>,
    Path((_device_id, conn_id)): Path<(String, String)>,
    Json(mut req): Json<UpdateConnectionRequest>,
) -> Result<Json<DeviceConnection>, AppError> {
    let existing = crate::roasters::registry::get_connection(&state.db, &conn_id)
        .await?
        .ok_or_else(|| AppError::not_found("Connection"))?;
    // The mask an answer showed keeps the saved password.
    if let Some(config) = req.config.as_mut() {
        if config.get("password").and_then(|p| p.as_str()) == Some(PASSWORD_MASK) {
            match existing.config.get("password").filter(|p| p.is_string()) {
                Some(saved) => config["password"] = saved.clone(),
                None => {
                    return Err(AppError::bad_request(format!(
                        "password: '{PASSWORD_MASK}' stands for a saved password, and this \
                         connection has none: type the password"
                    )))
                }
            }
        }
    }
    if let Some(config) = &req.config {
        crate::transports::pollers::check_connection_config(&existing.protocol, config)
            .map_err(AppError::bad_request)?;
        check_mqtt_claims(&state, &existing.device_id, &existing.protocol, config).await?;
    }
    let connection = crate::roasters::registry::update_connection(&state.db, &conn_id, req)
        .await?
        .ok_or_else(|| AppError::not_found("Connection"))?;
    state.pollers.sync_device(&connection.device_id).await;
    Ok(Json(connection))
}

/// An MQTT connection's topics must be its own (docs/ingest-mapping.md, "MQTT"): a topic another
/// device's connection already reads, or another roaster's `roaster/<id>/…`, is refused naming
/// that roaster.
async fn check_mqtt_claims(
    state: &AppState,
    registry_id: &str,
    protocol: &Protocol,
    config: &serde_json::Value,
) -> Result<(), AppError> {
    if *protocol != Protocol::Mqtt {
        return Ok(());
    }
    let dev = crate::roasters::registry::get_device(&state.db, registry_id)
        .await?
        .ok_or_else(|| AppError::not_found("Device"))?;
    let own = &dev.device.device_id;
    let Some(route) = crate::transports::ingest::MqttRoute::from_config(own, config)
        .map_err(AppError::bad_request)?
    else {
        return Ok(());
    };
    for topic in route.topics() {
        let other = match state.mqtt_routes.claimed_by_other(&topic, registry_id) {
            Some(other) => Some(other),
            None => match crate::app::consumer::parse_roaster_topic(&topic) {
                Some((owner, _)) if &owner != own => {
                    crate::roasters::registry::get_device_by_device_id(&state.db, &owner)
                        .await?
                        .map(|d| d.device.device_id)
                }
                _ => None,
            },
        };
        if let Some(other) = other {
            return Err(AppError::bad_request(format!(
                "topic '{topic}' is already {other}'s (Roasters → {other} → Connection): \
                 each topic belongs to one roaster"
            )));
        }
    }
    Ok(())
}

/// Remove a connection.
#[utoipa::path(delete, path = "/api/devices/{id}/connections/{conn_id}", tag = "devices",
    params(("id" = String, Path), ("conn_id" = String, Path)),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)))]
async fn remove_connection(
    State(state): State<AppState>,
    Path((_device_id, conn_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let existing = crate::roasters::registry::get_connection(&state.db, &conn_id).await?;
    let deleted = crate::roasters::registry::remove_connection(&state.db, &conn_id).await?;
    if deleted {
        if let Some(conn) = existing {
            state.pollers.sync_device(&conn.device_id).await;
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Connection"))
    }
}

// ============================================================================
// Register Map handlers
// ============================================================================

/// A Modbus device's registers.
#[utoipa::path(get, path = "/api/devices/{id}/register-map", tag = "devices",
    params(("id" = String, Path)),
    responses((status = 200, body = Vec<ModbusRegisterMap>), (status = 404, body = ErrorBody)))]
async fn get_register_map(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<Vec<ModbusRegisterMap>>, AppError> {
    // Verify device exists
    crate::roasters::registry::get_device(&state.db, &device_id)
        .await?
        .ok_or_else(|| AppError::not_found("Device"))?;

    let registers = crate::roasters::registry::get_register_map(&state.db, &device_id).await?;
    Ok(Json(registers))
}

/// Replace a Modbus device's registers. The map is checked first (names, kinds, data types,
/// addresses, word order, scale; a 400 names the register and what is wrong) and the device's
/// Modbus pollers restart with it at once.
#[utoipa::path(put, path = "/api/devices/{id}/register-map", tag = "devices",
    params(("id" = String, Path)), request_body = Vec<CreateRegisterMapEntry>,
    responses((status = 200, body = Vec<ModbusRegisterMap>),
        (status = 400, description = "A register is not valid", body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn set_register_map(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(registers): Json<Vec<CreateRegisterMapEntry>>,
) -> Result<Json<Vec<ModbusRegisterMap>>, AppError> {
    // Verify device exists
    crate::roasters::registry::get_device(&state.db, &device_id)
        .await?
        .ok_or_else(|| AppError::not_found("Device"))?;
    crate::transports::pollers::check_register_map(&registers).map_err(AppError::bad_request)?;

    crate::roasters::registry::set_register_map(&state.db, &device_id, registers).await?;
    state.pollers.sync_device(&device_id).await;

    let updated = crate::roasters::registry::get_register_map(&state.db, &device_id).await?;
    Ok(Json(updated))
}

// ============================================================================
// Connection test handler
// ============================================================================

/// Try a connection's settings before saving them; the answer says whether it worked.
#[utoipa::path(post, path = "/api/devices/test-connection", tag = "devices",
    request_body = TestConnectionRequest, responses((status = 200, body = TestConnectionResponse)))]
async fn test_connection(
    State(state): State<AppState>,
    Json(req): Json<TestConnectionRequest>,
) -> Json<TestConnectionResponse> {
    match req.protocol {
        Protocol::Mqtt => test_mqtt_connection(&state, &req).await,
        Protocol::ModbusTcp | Protocol::ModbusRtu => test_modbus_connection(&state, &req).await,
        Protocol::WebSocket => test_websocket_connection(&req).await,
        Protocol::SerialTc4 => test_serial_connection(&req).await,
        Protocol::Phidget22 => test_phidget_connection(&state, req).await,
    }
}

/// Phidget22 test: log in to the network server, list its temperature boards, open the
/// mapped inputs and wait for a reading of each (docs/phidgets.md).
async fn test_phidget_connection(
    state: &AppState,
    mut req: TestConnectionRequest,
) -> Json<TestConnectionResponse> {
    // A saved connection's form sends its password masked: the saved one is tried.
    if req.config.get("password").and_then(|p| p.as_str()) == Some(PASSWORD_MASK) {
        let saved = match &req.device_id {
            Some(id) => saved_phidget_password(state, id, &req.config).await,
            None => None,
        };
        match saved {
            Some(p) => req.config["password"] = serde_json::Value::String(p),
            None => {
                return Json(TestConnectionResponse {
                    success: false,
                    message: format!(
                        "password: '{PASSWORD_MASK}' stands for a saved password, and no saved \
                         Phidget connection of this roaster has one: type the password"
                    ),
                    latency_ms: None,
                })
            }
        }
    }
    let req = &req;
    let server = format!(
        "{}:{}",
        req.config["host"].as_str().unwrap_or(""),
        req.config["port"].as_u64().unwrap_or(5661)
    );
    if let Err(e) = crate::transports::pollers::check_connection_config(&req.protocol, &req.config)
    {
        return Json(TestConnectionResponse {
            success: false,
            message: e,
            latency_ms: None,
        });
    }
    let start = Instant::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        crate::transports::phidget::probe(&req.config),
    )
    .await;
    let latency_ms = Some(start.elapsed().as_millis() as u64);
    let (success, message) = match result {
        Ok(Ok(p)) => {
            let read = p.chosen.is_some() && !p.readings.is_empty();
            (read, p.summary(&server))
        }
        Ok(Err(e)) => (false, format!("Phidget22 test failed: {e}")),
        Err(_) => (
            false,
            format!("Phidget22 test of {server} timed out after 15 seconds"),
        ),
    };
    Json(TestConnectionResponse {
        success,
        message,
        latency_ms,
    })
}

/// The saved password of a roaster's Phidget connection, the one to the same host first.
async fn saved_phidget_password(
    state: &AppState,
    device_id: &str,
    config: &serde_json::Value,
) -> Option<String> {
    let dev = crate::roasters::registry::get_device_by_device_id(&state.db, device_id)
        .await
        .ok()??;
    let mut saved: Vec<&DeviceConnection> = dev
        .connections
        .iter()
        .filter(|c| c.protocol == Protocol::Phidget22)
        .filter(|c| c.config["password"].as_str().is_some_and(|p| !p.is_empty()))
        .collect();
    saved.sort_by_key(|c| c.config["host"] != config["host"]);
    saved
        .first()
        .and_then(|c| c.config["password"].as_str())
        .map(str::to_string)
}

/// Serial TC4 test: open the port, send the init lines and one READ, wait for a sample.
async fn test_serial_connection(req: &TestConnectionRequest) -> Json<TestConnectionResponse> {
    use tokio_serial::SerialPortBuilderExt;

    let config: crate::transports::types::SerialTc4ConnectionConfig =
        match serde_json::from_value(req.config.clone()) {
            Ok(c) => c,
            Err(e) => {
                return Json(TestConnectionResponse {
                    success: false,
                    message: format!("Invalid serial config: {e}"),
                    latency_ms: None,
                });
            }
        };

    let setup = crate::transports::tc4::Tc4Setup::from_config(&config);
    let start = Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let port = tokio_serial::new(&config.port, config.baud).open_native_async()?;
        let (reader, mut writer) = tokio::io::split(port);
        let mut reader = tokio::io::BufReader::new(reader);
        for line in setup.init_lines() {
            crate::transports::tc4::write_line(&mut writer, &line).await?;
        }
        crate::transports::tc4::write_line(&mut writer, "READ").await?;
        let sample = crate::transports::tc4::read_sample(
            &mut reader,
            &setup,
            std::time::Duration::from_secs(3),
        )
        .await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(sample)
    })
    .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        // The raw line next to how it was read, so a wrong reply format shows at once.
        Ok(Ok(Some((line, sample)))) => Json(TestConnectionResponse {
            success: true,
            message: format!(
                "TC4 device on {} answered READ with `{}`: {}",
                config.port,
                line,
                crate::transports::tc4::summary(&sample),
            ),
            latency_ms: Some(latency_ms),
        }),
        Ok(Ok(None)) => Json(TestConnectionResponse {
            success: false,
            message: format!(
                "Opened {} but got no sample in response to READ (check baud rate and firmware)",
                config.port
            ),
            latency_ms: Some(latency_ms),
        }),
        Ok(Err(e)) => Json(TestConnectionResponse {
            success: false,
            message: format!("Serial test failed on {}: {}", config.port, e),
            latency_ms: Some(latency_ms),
        }),
        Err(_) => Json(TestConnectionResponse {
            success: false,
            message: format!("Serial test on {} timed out after 5 seconds", config.port),
            latency_ms: Some(5000),
        }),
    }
}

/// MQTT test: check the in-memory telemetry cache for recent data from the device.
async fn test_mqtt_connection(
    state: &AppState,
    req: &TestConnectionRequest,
) -> Json<TestConnectionResponse> {
    let device_id = match &req.device_id {
        Some(id) => id.clone(),
        None => {
            // Try to extract device_id from the topic_prefix config
            if let Some(prefix) = req.config.get("topic_prefix").and_then(|v| v.as_str()) {
                // topic_prefix is typically "roaster/{device_id}"
                prefix
                    .strip_prefix("roaster/")
                    .unwrap_or(prefix)
                    .to_string()
            } else {
                return Json(TestConnectionResponse {
                    success: false,
                    message: "No device_id provided and could not extract from topic_prefix".into(),
                    latency_ms: None,
                });
            }
        }
    };

    let cache = state.telemetry_cache.read().await;
    if let Some((_val, ts)) = cache.get(&device_id) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age_secs = now.saturating_sub(*ts);
        if age_secs <= 30 {
            Json(TestConnectionResponse {
                success: true,
                message: format!(
                    "Receiving telemetry from device '{device_id}' (last seen {age_secs}s ago)"
                ),
                latency_ms: Some(age_secs * 1000),
            })
        } else {
            Json(TestConnectionResponse {
                success: false,
                message: format!(
                    "Device '{device_id}' last seen {age_secs}s ago (stale, threshold is 30s)"
                ),
                latency_ms: None,
            })
        }
    } else {
        Json(TestConnectionResponse {
            success: false,
            message: format!(
                "No telemetry found for device '{device_id}'. Ensure the device is powered on and publishing MQTT telemetry."
            ),
            latency_ms: None,
        })
    }
}

/// Modbus test (TCP or RTU): open the link and read the device's register map once (the
/// Roastwarden Standard layout when the device has none saved yet).
async fn test_modbus_connection(
    state: &AppState,
    req: &TestConnectionRequest,
) -> Json<TestConnectionResponse> {
    let mut map = Vec::new();
    if let Some(device_id) = &req.device_id {
        if let Ok(Some(dev)) =
            crate::roasters::registry::get_device_by_device_id(&state.db, device_id).await
        {
            map = crate::roasters::registry::get_register_map(&state.db, &dev.device.id)
                .await
                .unwrap_or_default();
        }
    }
    let start = Instant::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        crate::transports::pollers::probe_modbus(&req.protocol, &req.config, map),
    )
    .await;
    let latency_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(message)) => {
            info!(protocol = %req.protocol, %message, "Modbus connection test succeeded");
            Json(TestConnectionResponse {
                success: true,
                message,
                latency_ms: Some(latency_ms),
            })
        }
        Ok(Err(message)) => {
            warn!(protocol = %req.protocol, %message, "Modbus connection test failed");
            Json(TestConnectionResponse {
                success: false,
                message,
                latency_ms: Some(latency_ms),
            })
        }
        Err(_) => Json(TestConnectionResponse {
            success: false,
            message: "No answer from the device in 8 seconds".into(),
            latency_ms: Some(latency_ms),
        }),
    }
}

/// WebSocket test: attempt a handshake and wait for a message.
async fn test_websocket_connection(req: &TestConnectionRequest) -> Json<TestConnectionResponse> {
    let config: WebSocketConnectionConfig = match serde_json::from_value(req.config.clone()) {
        Ok(c) => c,
        Err(e) => {
            return Json(TestConnectionResponse {
                success: false,
                message: format!("Invalid WebSocket config: {e}"),
                latency_ms: None,
            });
        }
    };

    let start = Instant::now();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        use futures_util::StreamExt;
        let (ws_stream, _) = tokio_tungstenite::connect_async(&config.url).await?;
        let (_write, mut read) = ws_stream.split();
        // Wait for one message to confirm the connection is alive
        if let Some(msg) = read.next().await {
            msg?;
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok(())) => {
            info!(url = %config.url, "WebSocket connection test succeeded");
            Json(TestConnectionResponse {
                success: true,
                message: format!("Connected to {} and received a message", config.url),
                latency_ms: Some(latency_ms),
            })
        }
        Ok(Err(e)) => {
            warn!(url = %config.url, error = %e, "WebSocket connection test failed");
            Json(TestConnectionResponse {
                success: false,
                message: format!("Failed to connect to {}: {}", config.url, e),
                latency_ms: Some(latency_ms),
            })
        }
        Err(_) => Json(TestConnectionResponse {
            success: false,
            message: format!("Connection to {} timed out after 5 seconds", config.url),
            latency_ms: Some(5000),
        }),
    }
}
