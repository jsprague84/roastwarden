//! An ESP32 roaster's firmware, filled in (`firmware.rs`; docs/esp32-starter.md,
//! docs/esp32-fluid-bed.md): what this server can fill in, and the download.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::roasters::firmware::{FirmwareInfo, FirmwareRequest};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(firmware_info, firmware_download))
}

/// The firmware kinds, the MQTT port a roaster connects to, and the roaster ids whose MQTT
/// password the server has (`MQTT_DEVICES` and Roasters → MQTT passwords; never the passwords).
#[utoipa::path(get, path = "/api/firmware", tag = "devices",
    responses((status = 200, body = FirmwareInfo)))]
async fn firmware_info(State(state): State<AppState>) -> Response {
    let added: Vec<String> = match crate::roasters::registry::list_devices(&state.db, None).await {
        // Set up: not only found streaming ("Found, not added yet")
        Ok(d) => d
            .into_iter()
            .filter(|d| d.status != crate::roasters::types::DeviceStatus::Pending)
            .map(|d| d.device_id)
            .collect(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    Json(crate::roasters::firmware::info(
        &state.mqtt_passwords,
        &added,
    ))
    .into_response()
}

/// An ESP32 sketch with the roaster's id, its MQTT password (from the request, else the
/// server's: `MQTT_DEVICES` or Roasters → MQTT passwords), the server's address and the Wi-Fi
/// filled in, as a PlatformIO project in a zip that the Arduino IDE opens too. Nothing is stored. `X-Firmware-Missing` lists what was
/// left to fill in by hand (`password`, `wifi`).
#[utoipa::path(post, path = "/api/firmware", tag = "devices",
    request_body = FirmwareRequest,
    responses((status = 200, description = "The project (zip, as an attachment)", content_type = "application/zip", body = Vec<u8>),
        (status = 400, description = "A setting that cannot go into the sketch")))]
async fn firmware_download(
    State(state): State<AppState>,
    Json(req): Json<FirmwareRequest>,
) -> Response {
    let devices: Vec<(String, String)> = state
        .mqtt_passwords
        .password(req.roaster_id.trim())
        .map(|p| vec![(req.roaster_id.trim().to_string(), p)])
        .unwrap_or_default();
    match crate::roasters::firmware::build(
        &req,
        &devices,
        crate::roasters::firmware::default_port(),
    ) {
        Ok(fw) => {
            tracing::info!(
                kind = req.kind.sketch_name(),
                roaster = req.roaster_id.trim(),
                missing = ?fw.missing,
                "firmware download"
            );
            (
                [
                    (header::CONTENT_TYPE, "application/zip".to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", fw.file_name),
                    ),
                    (header::CACHE_CONTROL, "no-store".to_string()),
                    (
                        header::HeaderName::from_static("x-firmware-missing"),
                        fw.missing.join(","),
                    ),
                ],
                fw.zip,
            )
                .into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
