//! Home Assistant discovery (`docs/home-assistant.md`): what is published, where, and why not.
//! Switching it on and off, the broker and its password go through `PUT /api/settings`
//! (`ha_*`; the password is a secret, never read back).

use axum::{extract::State, response::IntoResponse, Json};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::notify::home_assistant::Status;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

/// Whether discovery is on, the broker, the roasters announced and the last publish.
#[utoipa::path(get, path = "/api/home-assistant", tag = "settings",
    operation_id = "home_assistant_status",
    responses((status = 200, body = Status)))]
async fn status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.home_assistant.status())
}
