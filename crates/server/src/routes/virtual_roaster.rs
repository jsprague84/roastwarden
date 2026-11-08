//! Virtual roaster control (`docs/virtual-roaster.md`).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::sim::virtual_roaster::VirtualState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_state, put_state))
        .routes(routes!(charge))
        .routes(routes!(dump))
}

/// What to switch; a key left out keeps its simulator as it is.
#[derive(Deserialize, ToSchema)]
struct Enable {
    /// The fluid bed (`virtual_roaster`)
    #[serde(default)]
    enabled: Option<bool>,
    /// The drum (`virtual_drum`); first switched on, it writes its roaster, connection and setup
    #[serde(default)]
    drum: Option<bool>,
}

/// Which simulator's beans.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct Which {
    /// `virtual_roaster` (the default), `virtual_drum`, or a demo visitor's own simulated
    /// roaster (`virtual_` and 8 hex digits)
    roaster: Option<String>,
}

enum Sim {
    Shared,
    Drum,
    /// A demo visitor's own (demo.rs)
    Visitor(String),
}

/// Which simulator `which` names; `Err` for a roaster that is no simulator.
#[allow(clippy::result_large_err)]
fn sim(which: &Which) -> Result<Sim, Response> {
    match which.roaster.as_deref() {
        None | Some(crate::sim::virtual_roaster::DEVICE_ID) => Ok(Sim::Shared),
        Some(crate::sim::virtual_drum::DEVICE_ID) => Ok(Sim::Drum),
        Some(v) if crate::ops::demo::is_visitor_roaster(v) => Ok(Sim::Visitor(v.to_string())),
        Some(other) => Err((
            StatusCode::BAD_REQUEST,
            format!("{other} is not a simulator: virtual_roaster or virtual_drum"),
        )
            .into_response()),
    }
}

fn not_running(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        format!(
            "{id} is not running: the demo gives a visitor's roaster again on their next visit"
        ),
    )
        .into_response()
}

/// The state to answer: a visitor's roaster's own, else the shared one's (with the drum).
async fn answer(state: &AppState, sim: &Sim) -> Response {
    match sim {
        Sim::Visitor(id) => match state.virtual_roaster.state_of(id).await {
            Some(s) => Json(s).into_response(),
            None => not_running(id),
        },
        _ => Json(state.virtual_roaster.state().await).into_response(),
    }
}

/// The simulated roaster (`virtual_roaster`, or `roaster=` a demo visitor's own): on or off,
/// beans in, temperatures; the drum beside it.
#[utoipa::path(get, path = "/api/virtual-roaster", tag = "virtual roaster",
    operation_id = "get_virtual_roaster", params(Which),
    responses((status = 200, body = VirtualState),
        (status = 404, description = "A visitor's roaster that is not running", body = crate::http::errors::ErrorBody)))]
async fn get_state(State(state): State<AppState>, Query(which): Query<Which>) -> Response {
    match sim(&which) {
        Ok(s) => answer(&state, &s).await,
        Err(r) => r,
    }
}

/// Switch the simulated fluid bed (`enabled`) or drum (`drum`) on or off; each publishes
/// telemetry while on. The drum, first switched on, writes its roaster, MQTT connection and
/// setup (docs/virtual-roaster.md).
#[utoipa::path(put, path = "/api/virtual-roaster", tag = "virtual roaster",
    operation_id = "set_virtual_roaster", request_body = Enable,
    responses((status = 200, body = VirtualState)))]
async fn put_state(State(state): State<AppState>, Json(body): Json<Enable>) -> Response {
    let vr = &state.virtual_roaster;
    let saved = async {
        if let Some(on) = body.enabled {
            vr.set_enabled(on).await?;
        }
        if let Some(on) = body.drum {
            vr.drum.set_enabled(on).await?;
        }
        Ok::<(), sqlx::Error>(())
    };
    if let Err(e) = saved.await {
        tracing::warn!(error = %e, "could not save the virtual roaster setting");
        return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }
    Json(vr.state().await).into_response()
}

/// Cold beans into the virtual chamber (or, with `roaster=virtual_drum`, the drum).
#[utoipa::path(post, path = "/api/virtual-roaster/charge", tag = "virtual roaster",
    operation_id = "charge_virtual_roaster", params(Which),
    responses((status = 200, body = VirtualState),
        (status = 400, description = "Not a simulator", body = crate::http::errors::ErrorBody),
        (status = 404, description = "A visitor's roaster that is not running", body = crate::http::errors::ErrorBody)))]
async fn charge(State(state): State<AppState>, Query(which): Query<Which>) -> Response {
    let s = match sim(&which) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match &s {
        Sim::Drum => state.virtual_roaster.drum.charge().await,
        Sim::Shared => state.virtual_roaster.charge().await,
        Sim::Visitor(id) => {
            if !state.virtual_roaster.charge_visitor(id).await {
                return not_running(id);
            }
        }
    }
    answer(&state, &s).await
}

/// Beans out of the virtual chamber (or, with `roaster=virtual_drum`, the drum).
#[utoipa::path(post, path = "/api/virtual-roaster/dump", tag = "virtual roaster",
    operation_id = "dump_virtual_roaster", params(Which),
    responses((status = 200, body = VirtualState),
        (status = 400, description = "Not a simulator", body = crate::http::errors::ErrorBody),
        (status = 404, description = "A visitor's roaster that is not running", body = crate::http::errors::ErrorBody)))]
async fn dump(State(state): State<AppState>, Query(which): Query<Which>) -> Response {
    let s = match sim(&which) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match &s {
        Sim::Drum => state.virtual_roaster.drum.dump().await,
        Sim::Shared => state.virtual_roaster.dump().await,
        Sim::Visitor(id) => {
            if !state.virtual_roaster.dump_visitor(id).await {
                return not_running(id);
            }
        }
    }
    answer(&state, &s).await
}
