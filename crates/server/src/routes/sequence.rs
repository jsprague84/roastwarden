//! Roast sequence settings per roaster (`docs/roast-sequence.md`).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::control::sequence::SequenceConfig;
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_sequence, put_sequence))
}

#[derive(Serialize, ToSchema)]
struct SequenceView {
    device_id: String,
    config: SequenceConfig,
    /// The roaster is cooling after DROP (the fan stops at `cool_end_bt_c`)
    cooling: bool,
    /// The roast being cooled: its samples are stored until COOL END, Complete or not
    cooling_roast_id: Option<String>,
}

async fn view(state: &AppState, device_id: &str) -> Response {
    Json(SequenceView {
        device_id: device_id.to_string(),
        config: state.sequence.config(device_id).await,
        cooling: state.sequence.cooling(device_id).await,
        cooling_roast_id: state.sequence.cooling_roast(device_id).await,
    })
    .into_response()
}

/// What CHARGE and DROP do to this roaster (docs/roast-sequence.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/sequence", tag = "roasters",
    params(("device_id" = String, Path)), responses((status = 200, body = SequenceView)))]
async fn get_sequence(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    view(&state, &device_id).await
}

/// Save what CHARGE and DROP do to this roaster.
#[utoipa::path(put, path = "/api/roaster/{device_id}/sequence", tag = "roasters",
    params(("device_id" = String, Path)), request_body = SequenceConfig,
    responses((status = 200, body = SequenceView), (status = 400, body = ErrorBody)))]
async fn put_sequence(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(c): Json<SequenceConfig>,
) -> Response {
    if let Err(e) = state.sequence.set_config(&device_id, &c).await {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    view(&state, &device_id).await
}
