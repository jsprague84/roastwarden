//! Shared profiles (docs/profiles.md, "Sharing a profile"): a profile's
//! public link and the import from a link. Making and revoking links is for the owner and
//! admin tokens (a link makes a profile public), reading them for every token; the import
//! makes a profile, so it is admin's too. The public page and its files (`page`, `file`) are
//! routed in `lib.rs`, outside `/api/`.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::Actor;
use crate::http::errors::ErrorBody;
use crate::labels::share as rshare;
use crate::profiles::share::{self as pshare, FetchError, ProfileShareLink, SharedProfile, Source};
use crate::profiles::types::{ImportArtisanProfileRequest, ProfileWithPoints};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(profile_share_links, share_profile))
        .routes(routes!(all_profile_share_links))
        .routes(routes!(revoke_profile_share_link))
        .routes(routes!(import_profile_link))
}

fn status(code: StatusCode, message: impl Into<String>) -> Response {
    (code, message.into()).into_response()
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "profile share links");
    status(
        StatusCode::INTERNAL_SERVER_ERROR,
        "the profile share links failed",
    )
}

async fn with_urls(
    state: &AppState,
    headers: &HeaderMap,
    mut links: Vec<ProfileShareLink>,
) -> Vec<ProfileShareLink> {
    let base = crate::auth::approvals::link_base(&state.db, headers).await;
    for l in &mut links {
        l.url = pshare::url(&base, &l.code);
    }
    links
}

/// A profile's share links, newest first, the revoked ones too
#[utoipa::path(get, path = "/api/profiles/{id}/share", tag = "profiles",
    params(("id" = String, Path, description = "The profile")),
    responses((status = 200, body = Vec<ProfileShareLink>), (status = 404, body = ErrorBody)))]
async fn profile_share_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    match crate::profiles::store::get_profile_with_points(&state.db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no profile {id:?}")),
        Err(e) => return failed(e),
    }
    match pshare::list(&state.db, Some(&id), true).await {
        Ok(links) => Json(with_urls(&state, &headers, links).await).into_response(),
        Err(e) => failed(e),
    }
}

/// What to share
#[derive(Deserialize, ToSchema, Default)]
pub struct ProfileShareAsk {
    /// A new link even when the profile has one (to revoke one person's later)
    #[serde(default)]
    new: bool,
}

/// The profile's share link: its newest one that is not revoked, else a new one (201); `new`
/// always makes another. The page is public: anyone with the link can see the profile and
/// import it. It shows the profile as it is now, so later edits show there too.
#[utoipa::path(post, path = "/api/profiles/{id}/share", tag = "profiles",
    params(("id" = String, Path, description = "The profile")),
    request_body = ProfileShareAsk,
    responses((status = 200, description = "The profile's link", body = ProfileShareLink),
        (status = 201, description = "A new link", body = ProfileShareLink),
        (status = 404, body = ErrorBody)))]
async fn share_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    ask: Option<Json<ProfileShareAsk>>,
) -> Response {
    let ask = ask.map(|Json(a)| a).unwrap_or_default();
    match crate::profiles::store::get_profile_with_points(&state.db, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no profile {id:?}")),
        Err(e) => return failed(e),
    }
    let base = crate::auth::approvals::link_base(&state.db, &headers).await;
    if !ask.new {
        match pshare::active(&state.db, &id).await {
            Ok(Some(mut link)) => {
                link.url = pshare::url(&base, &link.code);
                return Json(link).into_response();
            }
            Ok(None) => {}
            Err(e) => return failed(e),
        }
    }
    let by = super::label_studio::by(actor.as_deref());
    match pshare::create(&state.db, &id, &by).await {
        Ok(mut link) => {
            link.url = pshare::url(&base, &link.code);
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

/// Every profile's share links, newest first (the revoked ones with `all`)
#[utoipa::path(get, path = "/api/profile-shares", tag = "profiles",
    params(ListQuery),
    responses((status = 200, body = Vec<ProfileShareLink>)))]
async fn all_profile_share_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    match pshare::list(&state.db, None, q.all).await {
        Ok(links) => Json(with_urls(&state, &headers, links).await).into_response(),
        Err(e) => failed(e),
    }
}

/// Revoke a profile's share link: its page and files are then a 404, as for a code that
/// never was. A profile already imported elsewhere stays there.
#[utoipa::path(delete, path = "/api/profile-shares/{code}", tag = "profiles",
    params(("code" = String, Path, description = "The link's code")),
    responses((status = 200, body = ProfileShareLink), (status = 404, body = ErrorBody)))]
async fn revoke_profile_share_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    let Some(code) = rshare::normal(&code) else {
        return status(StatusCode::NOT_FOUND, "no such share link");
    };
    match pshare::revoke(&state.db, &code).await {
        Ok(true) => {}
        Ok(false) => return status(StatusCode::NOT_FOUND, "no such share link"),
        Err(e) => return failed(e),
    }
    match pshare::get(&state.db, &code).await {
        Ok(Some(link)) => {
            let mut v = with_urls(&state, &headers, vec![link]).await;
            Json(v.remove(0)).into_response()
        }
        Ok(None) => status(StatusCode::NOT_FOUND, "no such share link"),
        Err(e) => failed(e),
    }
}

/// What to import
#[derive(Deserialize, ToSchema)]
pub struct ImportLinkAsk {
    /// A shared profile's page (`https://host/P/CODE`), one of its files, or a code of this
    /// server's
    url: String,
    /// Its name here (default: the shared name)
    #[serde(default)]
    name: Option<String>,
    /// Only read it: nothing is saved
    #[serde(default)]
    preview: bool,
}

/// A profile read from a link
#[derive(Serialize, ToSchema)]
pub struct LinkImport {
    /// The shared page it came from
    pub source: String,
    /// The profile as shared
    pub profile: SharedProfile,
    /// The profile made from it (none with `preview`)
    pub saved: Option<ProfileWithPoints>,
}

/// A database error behind the import: logged, and a 500 without its detail
fn internal(e: anyhow::Error) -> (StatusCode, String) {
    tracing::error!(error = %format!("{e:#}"), "profile import from a link");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "the profile share links failed".into(),
    )
}

/// The shared profile behind a link, as a file: this server's from the database, another's
/// fetched
async fn shared(
    state: &AppState,
    headers: &HeaderMap,
    url: &str,
) -> Result<(String, SharedProfile), (StatusCode, String)> {
    let base = crate::auth::approvals::link_base(&state.db, headers).await;
    match pshare::source(url, &base).map_err(|e| (StatusCode::BAD_REQUEST, e))? {
        Source::Here(code) => {
            let gone = || {
                (
                    StatusCode::NOT_FOUND,
                    "that link is not active here".to_string(),
                )
            };
            let link = pshare::get(&state.db, &code)
                .await
                .map_err(internal)?
                .filter(|l| l.revoked_at.is_none())
                .ok_or_else(gone)?;
            let p = crate::profiles::store::get_profile_with_points(&state.db, &link.profile_id)
                .await
                .map_err(internal)?
                .ok_or_else(gone)?;
            Ok((pshare::url(&base, &code), pshare::document(&p, None)))
        }
        Source::There(link) => {
            let text = pshare::fetch(&link).await.map_err(|e| match e {
                FetchError::NotFound(m) => (StatusCode::NOT_FOUND, m),
                FetchError::Failed(m) => (StatusCode::BAD_GATEWAY, m),
            })?;
            let doc = pshare::read_document(&text)
                .map_err(|m| (StatusCode::BAD_GATEWAY, format!("{}: {m}", link.page())))?;
            Ok((link.page(), doc))
        }
    }
}

/// Import a profile from its share link (another server's `https://host/P/CODE`, or one of
/// this server's): the server reads the shared file (public addresses only, no redirects,
/// 10 s, 2 MB) and makes a profile from it, its description ending with where it came from.
/// `preview` only reads it.
#[utoipa::path(post, path = "/api/profiles/import/link", tag = "profiles",
    request_body = ImportLinkAsk,
    responses((status = 200, body = LinkImport),
        (status = 400, description = "Not a shared profile's link, or a profile it cannot make", body = ErrorBody),
        (status = 404, description = "The link is not active (`LINK_NOT_ACTIVE`)", body = ErrorBody),
        (status = 502, description = "The other server could not be read", body = ErrorBody)))]
async fn import_profile_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(ask): Json<ImportLinkAsk>,
) -> Response {
    let (source, doc) = match shared(&state, &headers, &ask.url).await {
        Ok(v) => v,
        // A link that is not active (here or there) has its own code: the hint is to ask for
        // a new link, not to look for the profile
        Err((StatusCode::NOT_FOUND, message)) => {
            let mut r = status(StatusCode::NOT_FOUND, message);
            r.extensions_mut()
                .insert(crate::http::errors::ErrorCode("LINK_NOT_ACTIVE"));
            return r;
        }
        Err((code, message)) => return status(code, message),
    };
    if ask.preview {
        return Json(LinkImport {
            source,
            profile: doc,
            saved: None,
        })
        .into_response();
    }
    let mut keep = doc.clone();
    let line = pshare::imported_line(&source);
    keep.description = Some(match keep.description.as_deref().map(str::trim) {
        Some(d) if !d.is_empty() => format!("{d}\n\n{line}"),
        _ => line,
    });
    let req = ImportArtisanProfileRequest {
        alog_content: serde_json::to_string(&keep).unwrap_or_default(),
        name: ask.name.filter(|n| !n.trim().is_empty()),
    };
    match crate::profiles::import::import_artisan_profile(&state.db, req).await {
        Ok(saved) => Json(LinkImport {
            source,
            profile: doc,
            saved: Some(saved),
        })
        .into_response(),
        Err(e) => status(
            StatusCode::BAD_REQUEST,
            format!("the shared profile could not be made here: {e:#}"),
        ),
    }
}

// ---- the public page and its files ----

fn gone() -> Response {
    super::share::page_response(StatusCode::NOT_FOUND, rshare::gone_html())
}

fn broken(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "a shared profile's page");
    super::share::page_response(StatusCode::INTERNAL_SERVER_ERROR, rshare::gone_html())
}

/// `GET /P/{code}` (and `/p/`): the public page, counted; a 404 page for a code that is
/// unknown or revoked, the same for both
pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    let Some(code) = rshare::normal(&code) else {
        return gone();
    };
    let link = match pshare::used(&state.db, &code, pshare::Use::View).await {
        Ok(Some(l)) => l,
        Ok(None) => return gone(),
        Err(e) => return broken(e),
    };
    let p = match crate::profiles::store::get_profile_with_points(&state.db, &link.profile_id).await
    {
        Ok(Some(p)) => p,
        Ok(None) => return gone(),
        Err(e) => return broken(e),
    };
    let base = crate::auth::approvals::link_base(&state.db, &headers).await;
    let brand = rshare::brand_for(&state.db, None).await;
    let html = pshare::html(&p, &code, &pshare::url(&base, &code), brand.as_ref());
    super::share::page_response(StatusCode::OK, html)
}

/// `GET /P/{code}/{file}`: `profile.json` (Roastwarden) or `profile.alog` (Artisan), counted
/// as a download; readable from any site (it is public), not cached (a revoke is at once)
pub async fn file(
    State(state): State<AppState>,
    Path((code, file)): Path<(String, String)>,
) -> Response {
    let not_found = || status(StatusCode::NOT_FOUND, "no such shared profile");
    let Some(code) = rshare::normal(&code) else {
        return not_found();
    };
    if file != "profile.json" && file != "profile.alog" {
        return not_found();
    }
    let link = match pshare::used(&state.db, &code, pshare::Use::Download).await {
        Ok(Some(l)) => l,
        Ok(None) => return not_found(),
        Err(e) => return failed(e),
    };
    let p = match crate::profiles::store::get_profile_with_points(&state.db, &link.profile_id).await
    {
        Ok(Some(p)) => p,
        Ok(None) => return not_found(),
        Err(e) => return failed(e),
    };
    let stem = safe_stem(&p.profile.name);
    let (kind, name, body) = if file == "profile.json" {
        let by = rshare::brand_for(&state.db, None).await.map(|b| b.name);
        let doc = pshare::document(&p, by);
        (
            "application/json; charset=utf-8",
            format!("{stem}.roastwarden-profile.json"),
            serde_json::to_string_pretty(&doc).unwrap_or_default(),
        )
    } else {
        // The profile's own id would be the file's roastUUID: a shared copy gets its own
        let mut shared = p;
        shared.profile.id = String::new();
        match crate::profiles::artisan::build_profile_alog(&shared) {
            Ok(alog) => (
                "text/plain; charset=utf-8",
                format!("{stem}.alog"),
                crate::profiles::artisan::render_alog(&alog),
            ),
            Err(e) => return status(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
        }
    };
    let mut res = (StatusCode::OK, body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    res
}

/// An ASCII file name from a profile's name
fn safe_stem(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.trim_matches('_').is_empty() {
        "profile".into()
    } else {
        s
    }
}
