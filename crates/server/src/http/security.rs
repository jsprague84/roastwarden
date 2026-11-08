//! Security headers on every response.
//!
//! The dashboard is a static SPA served from this origin: its scripts, styles, fonts and
//! WebSockets are all same-origin. SvelteKit's start-up script and the theme snippet in
//! `index.html` are inline, and Svelte sets inline styles, hence `'unsafe-inline'` for both.
//! `/docs` loads Swagger UI from unpkg. Nothing may frame the app (`frame-ancestors 'none'`).
//! A public demo that counts its visits (`ROASTWARDEN_DEMO_STATS_URL`) also allows its Umami
//! server's script and calls; no other install does.

use axum::http::{header, HeaderName, HeaderValue};
use axum::Router;
use tower_http::set_header::SetResponseHeaderLayer;

pub const CSP: &str = "default-src 'self'; \
script-src 'self' 'unsafe-inline' https://unpkg.com; \
style-src 'self' 'unsafe-inline' https://unpkg.com; \
img-src 'self' data: blob:; \
font-src 'self' data:; \
connect-src 'self' ws: wss:; \
object-src 'none'; \
base-uri 'self'; \
form-action 'self'; \
frame-ancestors 'none'";

/// The policy, with a demo's Umami origin when it counts its visits (already checked to be a
/// plain `https://` origin, `auth.rs` `demo_stats_from`).
pub fn csp(stats_origin: Option<&str>) -> String {
    match stats_origin {
        None => CSP.to_string(),
        Some(o) => CSP
            .replace(
                "script-src 'self' 'unsafe-inline' https://unpkg.com;",
                &format!("script-src 'self' 'unsafe-inline' https://unpkg.com {o};"),
            )
            .replace(
                "connect-src 'self' ws: wss:;",
                &format!("connect-src 'self' ws: wss: {o};"),
            ),
    }
}

pub fn with_security_headers<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    stats_origin: Option<&str>,
) -> Router<S> {
    let csp =
        HeaderValue::from_str(&csp(stats_origin)).unwrap_or_else(|_| HeaderValue::from_static(CSP));
    router
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        // A response that sets its own (the share page: stricter) keeps it
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            csp,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[tokio::test]
    async fn every_response_carries_the_headers() {
        let app = with_security_headers(Router::new().route("/x", get(|| async { "ok" })), None);
        for path in ["/x", "/missing"] {
            let res = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let h = res.headers();
            assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{path}");
            assert_eq!(h[header::X_FRAME_OPTIONS], "DENY", "{path}");
            assert!(h[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"));
        }
    }

    #[test]
    fn only_a_counting_demo_allows_its_stats_server() {
        assert_eq!(csp(None), CSP);
        assert!(!CSP.contains("stats"));
        let c = csp(Some("https://stats.example.com"));
        assert!(c.contains(
            "script-src 'self' 'unsafe-inline' https://unpkg.com https://stats.example.com;"
        ));
        assert!(c.contains("connect-src 'self' ws: wss: https://stats.example.com;"));
        assert!(c.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn a_response_with_its_own_csp_keeps_it() {
        // Label assets carry a sandboxed CSP (routes/label_studio.rs); the app's must not
        // replace it.
        use crate::routes::label_studio::{asset_response, ASSET_CSP};
        let app = with_security_headers(
            Router::new().route(
                "/asset",
                get(|| async { asset_response("a.svg", "image/svg+xml", b"<svg/>".to_vec()) }),
            ),
            None,
        );
        let res = app
            .oneshot(Request::get("/asset").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.headers()[header::CONTENT_SECURITY_POLICY], ASSET_CSP);
    }
}
