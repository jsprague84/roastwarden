//! The support bundle (`docs/support.md`): one JSON file for a bug report, secrets removed.

use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(bundle))
}

/// What runs, the roasters, the newest roast, settings, problems and the recent log, with every
/// secret removed and no green coffee, orders or receipts; as a file to attach to an issue.
#[utoipa::path(get, path = "/api/support-bundle", tag = "system",
    operation_id = "support_bundle",
    responses((status = 200, description = "The bundle (JSON, as an attachment)", content_type = "application/json", body = Object)))]
async fn bundle(State(state): State<AppState>) -> Response {
    let text = crate::ops::support::bundle(&state).await;
    let name = format!(
        "roastwarden-support-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%MZ")
    );
    (
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        text,
    )
        .into_response()
}
