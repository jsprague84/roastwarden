//! One JSON error body for the whole API:
//!
//! ```json
//! { "success": false, "error": { "code": "NOT_FOUND", "message": "No such setting" } }
//! ```
//!
//! Handlers still answer `(StatusCode, "text")`; [`json_errors`] turns every error under
//! `/api/` into this body on the way out, so the CLI, agents and the dashboard read one shape.
//! A response may name a more specific code with an [`ErrorCode`] extension (the auth layer
//! does: `TOKEN_EXPIRED`, `TOKEN_SCOPE`, …). An `/api/` path no route knows is a JSON 404
//! instead of the dashboard's page.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// A specific error code for [`json_errors`] to use instead of the status's.
#[derive(Debug, Clone, Copy)]
pub struct ErrorCode(pub &'static str);

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Always false.
    pub success: bool,
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorDetails {
    /// `BAD_REQUEST`, `UNAUTHORIZED`, `FORBIDDEN`, `NOT_FOUND`, `CONFLICT`, `UNPROCESSABLE`,
    /// `INTERNAL_ERROR`, …; from auth `TOKEN_INVALID`, `TOKEN_EXPIRED`, `TOKEN_REVOKED`,
    /// `TOKEN_SCOPE`, `REAL_HARDWARE`; from the control gate `CONTROL_OFF`, `OUTPUT_NOT_DECLARED` (docs/control.md, "Errors");
    /// from a profile import by link `LINK_NOT_ACTIVE`; a second CHARGE or DROP `ALREADY_MARKED`
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// The code for a status without a specific one.
pub fn code_for(status: StatusCode) -> String {
    match status {
        StatusCode::BAD_REQUEST => "BAD_REQUEST".into(),
        StatusCode::UNAUTHORIZED => "UNAUTHORIZED".into(),
        StatusCode::FORBIDDEN => "FORBIDDEN".into(),
        StatusCode::NOT_FOUND => "NOT_FOUND".into(),
        StatusCode::METHOD_NOT_ALLOWED => "METHOD_NOT_ALLOWED".into(),
        StatusCode::CONFLICT => "CONFLICT".into(),
        StatusCode::PAYLOAD_TOO_LARGE => "PAYLOAD_TOO_LARGE".into(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "UNSUPPORTED_MEDIA_TYPE".into(),
        StatusCode::UNPROCESSABLE_ENTITY => "UNPROCESSABLE".into(),
        StatusCode::TOO_MANY_REQUESTS => "TOO_MANY_REQUESTS".into(),
        StatusCode::INTERNAL_SERVER_ERROR => "INTERNAL_ERROR".into(),
        StatusCode::BAD_GATEWAY => "BAD_GATEWAY".into(),
        StatusCode::SERVICE_UNAVAILABLE => "UNAVAILABLE".into(),
        StatusCode::GATEWAY_TIMEOUT => "TIMEOUT".into(),
        s => format!("HTTP_{}", s.as_u16()),
    }
}

/// Error bodies above this are cut (they are messages, not documents).
const MAX_MESSAGE: usize = 16 * 1024;

pub async fn json_errors(req: Request, next: Next) -> Response {
    if !req.uri().path().starts_with("/api/") {
        return next.run(req).await;
    }
    let res = next.run(req).await;
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    let is_json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if is_json {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_MESSAGE)
        .await
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    let message = if text.is_empty() {
        status.canonical_reason().unwrap_or("Error").to_string()
    } else {
        text
    };
    let code = parts
        .extensions
        .get::<ErrorCode>()
        .map_or_else(|| code_for(status), |c| c.0.to_string());
    let body = ErrorBody {
        success: false,
        error: ErrorDetails {
            code,
            message,
            details: None,
        },
    };
    let json = serde_json::to_vec(&body).unwrap_or_default();
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, Body::from(json))
}

/// For `/api/{*rest}`: a path no route knows.
pub async fn api_not_found() -> Response {
    (StatusCode::NOT_FOUND, "No such API endpoint").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route(
                "/api/text",
                get(|| async { (StatusCode::BAD_REQUEST, "Keep 0 to 365") }),
            )
            .route(
                "/api/empty",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .route(
                "/api/coded",
                get(|| async {
                    let mut r = (StatusCode::UNAUTHORIZED, "expired").into_response();
                    r.extensions_mut().insert(ErrorCode("TOKEN_EXPIRED"));
                    r
                }),
            )
            .route(
                "/api/json",
                get(|| async {
                    (
                        StatusCode::CONFLICT,
                        axum::Json(serde_json::json!({"own": 1})),
                    )
                }),
            )
            .route("/api/ok", get(|| async { "fine" }))
            .route("/api/{*rest}", axum::routing::any(api_not_found))
            .route(
                "/page",
                get(|| async { (StatusCode::NOT_FOUND, "html 404") }),
            )
            .layer(axum::middleware::from_fn(json_errors))
    }

    async fn call(path: &str) -> (StatusCode, String, String) {
        let res = app()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let status = res.status();
        let b = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, ct, String::from_utf8(b.to_vec()).unwrap())
    }

    fn error(body: &str) -> (String, String) {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["success"], false);
        (
            v["error"]["code"].as_str().unwrap().into(),
            v["error"]["message"].as_str().unwrap().into(),
        )
    }

    #[tokio::test]
    async fn text_errors_become_the_json_body() {
        let (s, ct, b) = call("/api/text").await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(ct, "application/json");
        assert_eq!(error(&b), ("BAD_REQUEST".into(), "Keep 0 to 365".into()));

        let (_, _, b) = call("/api/empty").await;
        assert_eq!(
            error(&b),
            ("INTERNAL_ERROR".into(), "Internal Server Error".into())
        );

        let (_, _, b) = call("/api/coded").await;
        assert_eq!(error(&b).0, "TOKEN_EXPIRED");

        let (s, _, b) = call("/api/nope/at/all").await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(error(&b).0, "NOT_FOUND");
    }

    #[tokio::test]
    async fn json_answers_successes_and_pages_are_left_alone() {
        assert_eq!(call("/api/json").await.2, r#"{"own":1}"#);
        assert_eq!(call("/api/ok").await.2, "fine");
        let (s, _, b) = call("/page").await;
        assert_eq!((s, b.as_str()), (StatusCode::NOT_FOUND, "html 404"));
    }
}
