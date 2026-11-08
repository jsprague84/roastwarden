//! One id per request, for correlating a client-visible failure with the server log.
//!
//! The id is minted here, never taken from the client: a caller-chosen value would let anyone
//! write arbitrary text into the log's `request_id` field and collide with someone else's id.
//! It travels in the `x-request-id` response header and in the `http` tracing span, so
//! "what happened to request X" is one grep of the log.

use axum::{extract::Request, middleware::Next, response::Response, Router};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::Instrument;

pub const HEADER: &str = "x-request-id";

/// Wrap a router so every request gets a fresh id (outermost layers: they must run before the
/// trace span is created so the span can carry the id).
pub fn with_request_ids<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        // Innermost: the span every log line of the request carries.
        .layer(axum::middleware::from_fn(span))
        // Copies the request's id into the response.
        .layer(PropagateRequestIdLayer::x_request_id())
        // Mints an id when none is present — which is always, thanks to the layer outside.
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(axum::middleware::from_fn(drop_client_request_id))
}

/// Discard any id the client sent so the one we mint is the one in the log.
async fn drop_client_request_id(mut request: Request, next: Next) -> Response {
    request.headers_mut().remove(HEADER);
    next.run(request).await
}

async fn span(request: Request, next: Next) -> Response {
    let span = tracing::info_span!(
        "http",
        request_id = %from_headers(request.headers()),
        method = %request.method(),
        path = %request.uri().path(),
    );
    next.run(request).instrument(span).await
}

/// The request's id for log spans; `-` before the layer ran (never, in the real stack).
pub fn from_headers(headers: &axum::http::HeaderMap) -> &str {
    headers
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, routing::get};
    use tower::ServiceExt;

    fn app() -> Router {
        with_request_ids(Router::new().route(
            "/",
            get(|headers: axum::http::HeaderMap| async move { from_headers(&headers).to_string() }),
        ))
    }

    fn is_uuid(s: &str) -> bool {
        s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
    }

    async fn body(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn every_response_carries_the_id_the_handler_saw() {
        let resp = app()
            .oneshot(HttpRequest::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let header = resp.headers()[HEADER].to_str().unwrap().to_string();
        assert!(is_uuid(&header), "{header}");
        assert_eq!(body(resp).await, header);
    }

    #[tokio::test]
    async fn ids_are_unique_and_client_supplied_ones_are_replaced() {
        let a = app()
            .oneshot(HttpRequest::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let b = app()
            .oneshot(
                HttpRequest::get("/")
                    .header(HEADER, "attacker-chosen")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let a_id = a.headers()[HEADER].to_str().unwrap().to_string();
        let b_id = b.headers()[HEADER].to_str().unwrap().to_string();
        assert_ne!(a_id, b_id);
        assert!(is_uuid(&b_id), "client value must not survive: {b_id}");
        assert_eq!(body(b).await, b_id);
    }

    #[test]
    fn missing_header_reads_as_dash() {
        assert_eq!(from_headers(&axum::http::HeaderMap::new()), "-");
    }
}
