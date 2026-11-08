//! Share links (docs/labels.md, "The share page"): a public page
//! per roast at `/R/<code>`, what a bag's QR code opens. Making and revoking links is for the
//! owner and the tokens that may print (the page makes a roast public); reading them is for
//! every token. The page itself (`page`) is routed in `lib.rs`, outside `/api/`.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::Actor;
use crate::http::errors::ErrorBody;
use crate::labels::share::{self, ShareLink};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(roast_share_links, share_roast))
        .routes(routes!(share_links))
        .routes(routes!(revoke_share_link))
}

fn status(code: StatusCode, message: impl Into<String>) -> Response {
    (code, message.into()).into_response()
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "share links");
    status(StatusCode::INTERNAL_SERVER_ERROR, "the share links failed")
}

async fn with_urls(
    state: &AppState,
    headers: &HeaderMap,
    mut links: Vec<ShareLink>,
) -> Vec<ShareLink> {
    let base = crate::auth::approvals::link_base(&state.db, headers).await;
    for l in &mut links {
        l.url = share::url(&base, &l.code);
    }
    links
}

/// A roast's share links, newest first, the revoked ones too
#[utoipa::path(get, path = "/api/sessions/{id}/share", tag = "labels",
    params(("id" = String, Path, description = "The roast")),
    responses((status = 200, body = Vec<ShareLink>), (status = 404, body = ErrorBody)))]
async fn roast_share_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    match crate::roasts::sessions::get_session(&state.db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no roast {id:?}")),
        Err(e) => return failed(e),
    }
    match share::list(&state.db, Some(&id), true).await {
        Ok(links) => Json(with_urls(&state, &headers, links).await).into_response(),
        Err(e) => failed(e),
    }
}

/// What to share
#[derive(Deserialize, ToSchema, Default)]
pub struct ShareAsk {
    /// A line for the person it is for, on the page ("Happy birthday, Sam"); empty clears it
    message: Option<String>,
    /// A new link even when the roast has one (for someone else, with their own message)
    #[serde(default)]
    new: bool,
    /// The owner's offset from UTC in minutes east, for the page's dates (default: the
    /// `time_zone` setting, else UTC)
    tz: Option<i32>,
}

/// The roast's share link: its newest one that is not revoked (a message given replaces that
/// link's), else a new one (201). `new` always makes another. The page is public: anyone
/// with the link can open it.
#[utoipa::path(post, path = "/api/sessions/{id}/share", tag = "labels",
    params(("id" = String, Path, description = "The roast")),
    request_body = ShareAsk,
    responses((status = 200, description = "The roast's link", body = ShareLink),
        (status = 201, description = "A new link", body = ShareLink),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn share_roast(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    Json(ask): Json<ShareAsk>,
) -> Response {
    let message = match share::check_message(ask.message.as_deref()) {
        Ok(m) => m,
        Err(e) => return status(StatusCode::BAD_REQUEST, e),
    };
    match crate::roasts::sessions::get_session(&state.db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no roast {id:?}")),
        Err(e) => return failed(e),
    }
    let base = crate::auth::approvals::link_base(&state.db, &headers).await;
    if !ask.new {
        match share::active(&state.db, &id).await {
            Ok(Some(mut link)) => {
                if ask.message.is_some() && message != link.message {
                    if let Err(e) =
                        share::set_message(&state.db, &link.code, message.as_deref()).await
                    {
                        return failed(e);
                    }
                    link.message = message;
                }
                link.url = share::url(&base, &link.code);
                return Json(link).into_response();
            }
            Ok(None) => {}
            Err(e) => return failed(e),
        }
    }
    let by = super::label_studio::by(actor.as_deref());
    match share::create(&state.db, &id, message.as_deref(), ask.tz, &by).await {
        Ok(mut link) => {
            link.url = share::url(&base, &link.code);
            (StatusCode::CREATED, Json(link)).into_response()
        }
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, IntoParams)]
pub struct ListQuery {
    /// The revoked links too
    #[serde(default)]
    all: bool,
}

/// Every roast's share links, newest first (the revoked ones with `all`)
#[utoipa::path(get, path = "/api/share", tag = "labels",
    params(ListQuery),
    responses((status = 200, body = Vec<ShareLink>)))]
async fn share_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    match share::list(&state.db, None, q.all).await {
        Ok(links) => Json(with_urls(&state, &headers, links).await).into_response(),
        Err(e) => failed(e),
    }
}

/// Revoke a share link: its page is then a 404, as for a code that never was. Labels already
/// printed with it open nothing; share the roast again for a new link.
#[utoipa::path(delete, path = "/api/share/{code}", tag = "labels",
    params(("code" = String, Path, description = "The link's code")),
    responses((status = 200, body = ShareLink), (status = 404, body = ErrorBody)))]
async fn revoke_share_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    let Some(code) = share::normal(&code) else {
        return status(StatusCode::NOT_FOUND, "no such share link");
    };
    match share::revoke(&state.db, &code).await {
        Ok(true) => {}
        Ok(false) => return status(StatusCode::NOT_FOUND, "no such share link"),
        Err(e) => return failed(e),
    }
    match share::get(&state.db, &code).await {
        Ok(Some(link)) => {
            let mut v = with_urls(&state, &headers, vec![link]).await;
            Json(v.remove(0)).into_response()
        }
        Ok(None) => status(StatusCode::NOT_FOUND, "no such share link"),
        Err(e) => failed(e),
    }
}

/// The page's own security headers: nothing but its inline style, no referrer, not indexed,
/// not cached (a revoked link stops at once)
pub const PAGE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; \
base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

pub(crate) fn page_response(code: StatusCode, body: String) -> Response {
    let mut res = (code, body).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(PAGE_CSP),
    );
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// `GET /R/{code}` (and `/r/`): the public page, counted; a 404 page for a code that is
/// unknown or revoked, the same for both
pub async fn page(State(state): State<AppState>, Path(code): Path<String>) -> Response {
    let gone = || page_response(StatusCode::NOT_FOUND, share::gone_html());
    let Some(code) = share::normal(&code) else {
        return gone();
    };
    let link = match share::scanned(&state.db, &code).await {
        Ok(Some(l)) => l,
        Ok(None) => return gone(),
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "a share page");
            return page_response(StatusCode::INTERNAL_SERVER_ERROR, share::gone_html());
        }
    };
    match share::page(&state.db, &link, chrono::Utc::now()).await {
        Ok(Some(p)) => page_response(StatusCode::OK, share::html(&p)),
        Ok(None) => gone(),
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "a share page");
            page_response(StatusCode::INTERNAL_SERVER_ERROR, share::gone_html())
        }
    }
}
