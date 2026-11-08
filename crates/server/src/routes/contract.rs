//! One roaster against the device contract (`docs/control.md`, "Device contract"): what it
//! reads, what it says it can do, its firmware, and whether its PID gains are the ones the
//! server holds (with the last check after a boot). Only reads. Beside it, for adding a roaster
//! (`docs/add-roaster.md`): the newest raw message, the ingest preview, the conformance check
//! and the write check (the owner's, on a cold roaster).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::control::schedule::{self, BootCheck};
use crate::control::tuning::Gains;
use crate::http::errors::ErrorBody;
use crate::roasters::caps::DeviceCaps;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_contract))
        .routes(routes!(get_raw))
        .routes(routes!(preview_ingest))
        .routes(routes!(get_check))
        .routes(routes!(write_check))
}

/// The readings the contract requires, as seen in the roaster's latest sample.
#[derive(Serialize, ToSchema)]
pub(crate) struct ContractReads {
    /// Bean temperature
    bt: bool,
    /// Environment (inlet or exhaust) temperature
    et: bool,
    heater: bool,
    fan: bool,
}

/// The roaster's PID gains against the ones the server holds.
#[derive(Serialize, ToSchema)]
pub(crate) struct ContractGains {
    /// What its onboard PID reports now
    reported: Option<Gains>,
    /// What the server holds for it at its fan now: the gain schedule's, else the newest
    /// stored change. It sends these after a boot.
    held: Option<Gains>,
    /// Reported and held agree; null when either is missing
    matches: Option<bool>,
    /// The last check after a boot, since the server started
    last_boot: Option<BootCheck>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ContractView {
    device_id: String,
    /// The firmware version in its status message
    firmware: Option<String>,
    /// Seconds since its latest sample; null when none since the server started
    last_sample_s_ago: Option<u64>,
    reads: ContractReads,
    /// What it can do (the contract's optional parts): `reported` is true when its status
    /// message listed them, false when the server inferred them from its telemetry
    caps: DeviceCaps,
    gains: ContractGains,
}

/// The roaster against the device contract: readings, caps, firmware and whether its gains
/// are the ones the server holds (docs/control.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/contract", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = ContractView),
        (status = 404, description = "Not heard since the server started", body = ErrorBody)))]
async fn get_contract(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    let sample = state.telemetry_service.latest_sample(&device_id);
    let firmware = state
        .device_registry
        .read()
        .await
        .get(&device_id)
        .map(|d| d.version.clone());
    if sample.is_none() && firmware.is_none() {
        return (
            StatusCode::NOT_FOUND,
            "this roaster has not been heard since the server started",
        )
            .into_response();
    }
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let reported = sample.as_ref().and_then(schedule::reported_gains);
    let held = state
        .gain_scheduler
        .held_gains(&device_id, sample.as_ref().and_then(|s| s.fan))
        .await;
    let matches = match (reported, held) {
        (Some(r), Some(h)) => Some(!schedule::differs(r, h)),
        _ => None,
    };
    Json(ContractView {
        firmware: firmware.flatten(),
        last_sample_s_ago: sample
            .as_ref()
            .map(|s| now_ms.saturating_sub(s.ts_ms) / 1000),
        reads: ContractReads {
            bt: sample.as_ref().is_some_and(|s| s.bt.is_some()),
            et: sample.as_ref().is_some_and(|s| s.et.is_some()),
            heater: sample.as_ref().is_some_and(|s| s.heater.is_some()),
            fan: sample.as_ref().is_some_and(|s| s.fan.is_some()),
        },
        caps: state.replay.device_caps(&device_id),
        gains: ContractGains {
            reported,
            held,
            matches,
            last_boot: state.gain_scheduler.last_boot(&device_id).await,
        },
        device_id,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Adding a roaster (docs/add-roaster.md)
// ---------------------------------------------------------------------------

/// The newest raw message of each of the roaster's connections (and MQTT topics), as received
/// before any mapping: what `roast roaster map --try` maps.
#[utoipa::path(get, path = "/api/roaster/{device_id}/raw", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = Vec<crate::roasters::raw::RawMessage>)))]
async fn get_raw(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    Json(state.telemetry_service.raw.list(&device_id)).into_response()
}

/// A mapping to try on a connection's newest message; nothing is saved.
#[derive(serde::Deserialize, ToSchema)]
pub(crate) struct PreviewRequest {
    connection_id: String,
    /// The `ingest` to try (docs/ingest-mapping.md); null: the saved one
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    ingest: Option<serde_json::Value>,
}

/// What a mapping makes of the newest message.
#[derive(Serialize, ToSchema)]
pub(crate) struct IngestPreview {
    connection_id: String,
    protocol: String,
    /// The messages it was tried on (newest per topic)
    messages: Vec<crate::roasters::raw::RawMessage>,
    /// The sample the mapping makes (°C); null when it makes none
    #[schema(value_type = Option<Object>)]
    sample: Option<serde_json::Value>,
    /// Each field: where it is read, what is there, what it becomes or why not
    fields: Vec<crate::transports::ingest::PreviewField>,
    note: Option<String>,
}

fn bad(msg: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, msg.into()).into_response()
}

/// Try a mapping on the connection's newest raw message: each field's value in °C or why it
/// has none, and the sample it would make. Writes nothing; a bad mapping is the save's 400.
#[utoipa::path(post, path = "/api/roaster/{device_id}/ingest/preview", tag = "roasters",
    params(("device_id" = String, Path)), request_body = PreviewRequest,
    responses((status = 200, body = IngestPreview),
        (status = 400, description = "A bad mapping, or a Modbus connection", body = ErrorBody),
        (status = 404, description = "No such connection on this roaster", body = ErrorBody)))]
async fn preview_ingest(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<PreviewRequest>,
) -> Response {
    use crate::roasters::types::Protocol;
    use crate::transports::ingest::{self, Kind};
    let not_found = || {
        (
            StatusCode::NOT_FOUND,
            format!("{device_id} has no connection {}", body.connection_id),
        )
            .into_response()
    };
    let dev = match crate::roasters::registry::get_device_by_device_id(&state.db, &device_id).await
    {
        Ok(Some(d)) => d,
        Ok(None) => return not_found(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let Some(conn) = dev
        .connections
        .iter()
        .find(|c| c.id == body.connection_id)
        .cloned()
    else {
        return not_found();
    };
    let kind = match conn.protocol {
        Protocol::Mqtt => Kind::Mqtt,
        Protocol::WebSocket => Kind::WebSocket,
        Protocol::SerialTc4 => Kind::Tc4,
        Protocol::Phidget22 => Kind::Phidget,
        Protocol::ModbusTcp | Protocol::ModbusRtu => {
            return bad("a Modbus connection is mapped by role in its register map \
                 (roast roaster map --register kind:address:type:role)")
        }
    };
    let mut config = conn.config.clone();
    if let Some(i) = &body.ingest {
        config["ingest"] = i.clone();
    }
    let parsed = match config.get("ingest").filter(|v| !v.is_null()) {
        Some(v) => match ingest::parse_ingest(v, kind) {
            Ok(i) => Some(i),
            Err(e) => return bad(e),
        },
        // A Phidget with no mapping reads BT ch0, ET ch1 (docs/phidgets.md).
        None if kind == Kind::Phidget => {
            ingest::parse_ingest(&crate::transports::phidget::default_ingest(), kind).ok()
        }
        None => None,
    };
    let messages: Vec<crate::roasters::raw::RawMessage> = state
        .telemetry_service
        .raw
        .list(&device_id)
        .into_iter()
        .filter(|m| m.connection_id.as_deref() == Some(conn.id.as_str()))
        .collect();
    let mut out = IngestPreview {
        connection_id: conn.id.clone(),
        protocol: conn.protocol.to_string(),
        messages: messages.clone(),
        sample: None,
        fields: Vec::new(),
        note: None,
    };
    if messages.is_empty() {
        out.note = Some(
            "no message from this connection since the server started: is the roaster on and \
             the connection right?"
                .into(),
        );
    }
    match kind {
        Kind::Mqtt | Kind::WebSocket | Kind::Phidget => {
            let telemetry_topic = if kind == Kind::Mqtt {
                match ingest::MqttRoute::from_config(&device_id, &config) {
                    Ok(Some(r)) => Some(r.telemetry_topic),
                    Ok(None) => Some(format!(
                        "{}/telemetry",
                        ingest::MqttRoute::default_prefix(&device_id)
                    )),
                    Err(e) => return bad(e),
                }
            } else {
                None
            };
            let parsed_msgs: Vec<(Option<String>, serde_json::Value)> = messages
                .iter()
                .filter_map(|m| {
                    ingest::parse_payload(m.payload.as_bytes()).map(|v| (m.topic.clone(), v))
                })
                .collect();
            match &parsed {
                Some(i) => {
                    let (fields, sample) =
                        ingest::preview_json(i, telemetry_topic.as_deref(), &parsed_msgs);
                    out.fields = fields;
                    if sample.is_none() && !messages.is_empty() {
                        out.note = Some(
                            "neither BT nor ET has a value: this message would be no sample".into(),
                        );
                    }
                    out.sample = sample.map(serde_json::Value::Object);
                }
                None => {
                    if !messages.is_empty() {
                        out.note = Some(
                            "no mapping: messages pass as they are, in Roastwarden's own field \
                             names (beanTemp, envTemp, … in °C)"
                                .into(),
                        );
                    }
                    out.sample = parsed_msgs
                        .into_iter()
                        .find(|(t, _)| telemetry_topic.is_none() || *t == telemetry_topic)
                        .map(|(_, v)| v)
                        .filter(serde_json::Value::is_object);
                }
            }
        }
        Kind::Tc4 => {
            let cfg: crate::transports::types::SerialTc4ConnectionConfig =
                match serde_json::from_value(config.clone()) {
                    Ok(c) => c,
                    Err(e) => return bad(format!("the connection's config: {e}")),
                };
            let mut setup = crate::transports::tc4::Tc4Setup::from_config(&cfg);
            // A column naming a declared °C channel follows the unit, as the poller reads it
            setup.temp_channels = state.transports.setups().temp_channels(&device_id).await;
            if let Some(line) = messages.first().map(|m| m.payload.clone()) {
                let sample = crate::transports::tc4::parse_read_line(&line, &setup);
                out.fields = match &setup.columns {
                    Some(cols) => {
                        ingest::preview_columns(&line, cols, setup.units, &setup.temp_channels)
                    }
                    None => sample
                        .iter()
                        .flatten()
                        .map(|(k, v)| ingest::PreviewField {
                            key: k.clone(),
                            field: k.clone(),
                            source: format!("{:?} reply layout", setup.reply).to_lowercase(),
                            raw: None,
                            value: v.as_f64(),
                            problem: None,
                        })
                        .collect(),
                };
                if sample.is_none() {
                    out.note = Some(
                        "the line gives neither BT nor ET with this layout: try --column".into(),
                    );
                }
                out.sample = sample.map(serde_json::Value::Object);
            }
        }
    }
    Json(out).into_response()
}

/// The roaster against the interface standard: rate, BT and ET, units, outputs and channels
/// echoed, caps, control. Each line ok, warn or fail; the verdict is the worst. Only reads.
#[utoipa::path(get, path = "/api/roaster/{device_id}/check", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = crate::roasters::check::RoasterCheck)))]
async fn get_check(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    Json(crate::roasters::check::check_now(&state, &device_id).await).into_response()
}

/// The write check, the owner's on a cold roaster: heater 10 % (through the gate), its echo,
/// then emergency stop, always (on a timeout or an error too), and the heater reading 0.
#[utoipa::path(post, path = "/api/roaster/{device_id}/check/write", tag = "roasters",
    params(("device_id" = String, Path)),
    responses((status = 200, body = crate::roasters::check::WriteCheck),
        (status = 409, description = "Control off, no heater, hot, in auto, a roast open or busy", body = ErrorBody)))]
async fn write_check(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    match crate::roasters::check::write_check(
        &state,
        &device_id,
        crate::roasters::check::Waits::default(),
    )
    .await
    {
        Ok(r) => Json(r).into_response(),
        Err(r) => {
            let mut res = (StatusCode::CONFLICT, r.message).into_response();
            res.extensions_mut()
                .insert(crate::http::errors::ErrorCode(r.code));
            res
        }
    }
}
