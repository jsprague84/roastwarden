//! The OpenAPI document, generated from the handlers.
//!
//! A documented route lives in an `OpenApiRouter` and is registered with `routes!(…)`, so the
//! same line routes it and describes it: the document cannot miss a route or describe one
//! that is not there. `GET /api-docs/openapi.json` serves it; `docs/openapi.json` is the
//! committed copy, and `openapi_document_is_current` fails when the two differ. After an API
//! change: `UPDATE_OPENAPI=1 cargo test -p roastwarden-server openapi_document`.
//!
//! The WebSockets (`/ws/v2/telemetry`, `/ws/device/{id}/telemetry`, …) are described in
//! `docs/websocket-v2.md`, not here.

use std::sync::OnceLock;

use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::app::state::AppState;

/// The document's fixed parts: title, how to authenticate, the error body.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Roastwarden API",
        description = "A server for coffee roasters: roasts, profiles, profile drive, beans and \
                       stock, alarms and notifications. Temperatures are °C, heater and fan are \
                       0–100 %, weights grams. Authenticate with the owner key or an `rr_` token \
                       (`Authorization: Bearer …` or `X-API-Key`, Settings → Tokens); reads \
                       need no credential unless the server says otherwise \
                       (`GET /api/auth/status`). Every error is an `ErrorBody` \
                       (docs/control.md, \"Errors\"). WebSockets: docs/websocket-v2.md."
    ),
    servers((url = "/", description = "This server")),
    modifiers(&Security),
    security(("bearer" = []), ("api_key" = [])),
    components(schemas(crate::http::errors::ErrorBody))
)]
struct ApiDoc;

struct Security;

impl utoipa::Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{
            ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme,
        };
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("owner key or rr_ token")
                    .build(),
            ),
        );
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
    }
}

/// What a control command can answer (docs/control.md): `responses(CommandResponses)`.
#[derive(utoipa::IntoResponses)]
#[allow(dead_code)] // only describes responses
pub(crate) enum CommandResponses {
    #[response(status = 204, description = "Sent to the roaster")]
    Sent,
    #[response(status = 400, description = "A value out of range")]
    Invalid(crate::http::errors::ErrorBody),
    #[response(
        status = 403,
        description = "A token may not control this roaster (`REAL_HARDWARE`, `TOKEN_SCOPE`)"
    )]
    Refused(crate::http::errors::ErrorBody),
    #[response(
        status = 501,
        description = "The roaster's transport cannot carry the command"
    )]
    Unsupported(crate::http::errors::ErrorBody),
    #[response(status = 502, description = "Sending failed")]
    SendFailed(crate::http::errors::ErrorBody),
    #[response(status = 503, description = "No transport reaches the roaster")]
    NoTransport(crate::http::errors::ErrorBody),
    #[response(status = 504, description = "No acknowledgement in time (`wait_ack`)")]
    Timeout(crate::http::errors::ErrorBody),
}

/// Every documented route.
pub(crate) fn routes() -> OpenApiRouter<AppState> {
    use crate::routes;
    OpenApiRouter::new()
        .merge(crate::auth::audit::routes())
        .merge(crate::analysis::ws2::routes())
        .merge(routes::system::routes())
        .merge(routes::telemetry::routes())
        .merge(routes::replay::routes())
        .merge(routes::profiles::routes())
        .merge(routes::profile_share::routes())
        .merge(routes::roast_events::routes())
        .merge(routes::sessions::routes())
        .merge(routes::control::routes())
        .merge(routes::devices::routes())
        .merge(routes::inventory::routes())
        .merge(routes::orders::routes())
        .merge(routes::roasted::routes())
        .merge(routes::trace::routes())
        .merge(routes::labels::routes())
        .merge(routes::label_studio::routes())
        .merge(routes::label_runs::routes())
        .merge(routes::label_printers::routes())
        .merge(routes::share::routes())
        .merge(routes::reports::routes())
        .merge(routes::tuning::routes())
        .merge(routes::tune::routes())
        .merge(routes::schedule::routes())
        .merge(routes::contract::routes())
        .merge(routes::predict::routes())
        .merge(routes::analysis::routes())
        .merge(routes::fan_assist::routes())
        .merge(routes::sequence::routes())
        .merge(routes::virtual_roaster::routes())
        .merge(routes::push::routes())
        .merge(routes::tokens::routes())
        .merge(routes::approvals::routes())
        .merge(routes::about::routes())
        .merge(routes::backups::routes())
        .merge(routes::notifications::routes())
        .merge(routes::home_assistant::routes())
        .merge(routes::support::routes())
        .merge(routes::firmware::routes())
        .merge(routes::mqtt_passwords::routes())
        .merge(routes::settings::routes())
}

/// The OpenAPI document of every documented route.
pub fn document() -> utoipa::openapi::OpenApi {
    let mut doc = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(routes())
        .into_openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_string();
    doc.info.license = None;
    describe_responses(&mut doc);
    plain_operation_ids(&mut doc);
    doc
}

/// Operation ids are the handlers' names; the older ones start with `api_`, which says
/// nothing in an API document.
fn plain_operation_ids(doc: &mut utoipa::openapi::OpenApi) {
    for item in doc.paths.paths.values_mut() {
        let ops = [
            &mut item.get,
            &mut item.put,
            &mut item.post,
            &mut item.delete,
            &mut item.patch,
        ];
        for op in ops.into_iter().flatten() {
            if let Some(id) = op.operation_id.as_mut()
                && let Some(rest) = id.strip_prefix("api_") {
                    *id = rest.to_string();
                }
        }
    }
}

/// OpenAPI requires a description on every response; an annotation that gives none gets the
/// status's reason phrase ("OK", "Not Found").
fn describe_responses(doc: &mut utoipa::openapi::OpenApi) {
    use utoipa::openapi::RefOr;
    for item in doc.paths.paths.values_mut() {
        let ops = [
            &mut item.get,
            &mut item.put,
            &mut item.post,
            &mut item.delete,
            &mut item.patch,
        ];
        for op in ops.into_iter().flatten() {
            for (status, response) in &mut op.responses.responses {
                if let RefOr::T(r) = response
                    && r.description.is_empty() {
                        r.description = status
                            .parse::<u16>()
                            .ok()
                            .and_then(|s| axum::http::StatusCode::from_u16(s).ok())
                            .and_then(|s| s.canonical_reason())
                            .unwrap_or("Response")
                            .to_string();
                    }
            }
        }
    }
}

/// `GET /api-docs/openapi.json`
pub(crate) async fn serve() -> Response {
    static JSON: OnceLock<String> = OnceLock::new();
    let body = JSON.get_or_init(|| serde_json::to_string(&document()).unwrap_or_default());
    ([(CONTENT_TYPE, "application/json")], body.clone()).into_response()
}

#[cfg(test)]
mod tests {
    /// `docs/openapi.json` is the document the code generates. After an API change:
    /// `UPDATE_OPENAPI=1 cargo test -p roastwarden-server openapi_document`.
    #[test]
    fn openapi_document_is_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/openapi.json");
        let mut generated = serde_json::to_string_pretty(&super::document()).unwrap();
        generated.push('\n');
        if std::env::var("UPDATE_OPENAPI").is_ok() {
            std::fs::write(path, &generated).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            committed == generated,
            "docs/openapi.json is stale; regenerate with \
             UPDATE_OPENAPI=1 cargo test -p roastwarden-server openapi_document"
        );
    }

    /// An /api/ route outside `routes!` would be served but missing from the document: the
    /// only `.route("/api/…` left is the JSON 404 for unknown paths.
    #[test]
    fn every_api_route_is_documented() {
        let src = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        // Every module of the library, in its domain folders (the `roast` CLI is a client)
        let mut files = Vec::new();
        let mut dirs = vec![src.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() && path != src.join("bin") {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            // Tests build routers of their own
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for (n, line) in code.lines().enumerate() {
                let line = line.trim();
                if line.contains("\"/api/") && !line.starts_with("//") {
                    let routed = line.starts_with(".route(\"/api/")
                        || line.starts_with(".route_service(\"/api/")
                        || line.starts_with(".nest(\"/api");
                    assert!(
                        !routed || line.contains("/api/{*rest}"),
                        "{}:{}: an /api/ route outside routes!(): {line}",
                        path.display(),
                        n + 1
                    );
                }
            }
        }
    }

    /// A client generated from the document names a call by its operationId; two handlers
    /// called `list` need an `operation_id` in their annotation.
    #[test]
    fn operation_ids_are_unique() {
        let doc = super::document();
        let mut seen = std::collections::BTreeMap::new();
        for (path, item) in &doc.paths.paths {
            let ops = [&item.get, &item.put, &item.post, &item.delete, &item.patch];
            for op in ops.into_iter().flatten() {
                let id = op.operation_id.clone().unwrap_or_default();
                if let Some(other) = seen.insert(id.clone(), path.clone()) {
                    panic!("operationId {id} is on {other} and {path}");
                }
            }
        }
    }
}
