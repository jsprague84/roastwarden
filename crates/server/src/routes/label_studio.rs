//! The label studio (docs/labels.md, "Label studio"): saved designs with their versions,
//! assets (images and fonts), brand kits, and a coffee's own label. A design is saved only
//! when it compiles: the save runs the same check as `POST /api/labels/check` and answers its
//! report. Writes for inventory and admin tokens; deletes may wait for the owner's approval.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::auth::Actor;
use crate::http::errors::ErrorBody;
use crate::labels::{self, store, Report};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_designs, create_design))
        .routes(routes!(get_design, edit_design, delete_design))
        .routes(routes!(rollback_design))
        .routes(routes!(list_assets))
        .routes(routes!(get_asset, put_asset, delete_asset))
        .routes(routes!(list_brands))
        .routes(routes!(get_brand, put_brand, delete_brand))
        .routes(routes!(set_bean_label))
}

/// Who made a change: `owner`, the token's name, or `demo visitor`
pub fn by(actor: Option<&Actor>) -> String {
    match actor {
        Some(Actor::Token(t)) => t.name.clone(),
        Some(Actor::Demo) => "demo visitor".into(),
        _ => "owner".into(),
    }
}

fn status(code: StatusCode, message: impl Into<String>) -> Response {
    (code, message.into()).into_response()
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "label studio");
    status(StatusCode::INTERNAL_SERVER_ERROR, "the label store failed")
}

// ---- designs ----

/// A design to choose from: saved (with versions), the owner's folder, or built in.
#[derive(Serialize, ToSchema)]
pub struct DesignItem {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// saved, folder or builtin
    pub kind: String,
    /// A saved design's current version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// Its parameters' names (`design.params`), when saved
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// The coffees (beans) whose own label is this design
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub used_by: Vec<String>,
}

fn param_names(manifest: Option<&serde_json::Value>) -> Vec<String> {
    manifest
        .and_then(|m| m["params"].as_object())
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every design: saved ones (with versions), the owner's folder, the built-in ones, each
/// with the coffees that use it.
pub async fn all_designs(state: &AppState) -> anyhow::Result<Vec<DesignItem>> {
    let mut users = store::label_users(&state.db).await?;
    let mut out: Vec<DesignItem> = store::designs(&state.db)
        .await?
        .into_iter()
        .map(|(d, manifest)| DesignItem {
            params: param_names(manifest.as_ref()),
            used_by: users.remove(&d.id).unwrap_or_default(),
            id: d.id,
            name: d.name,
            description: d.description,
            kind: "saved".into(),
            version: Some(d.version),
            updated_at: Some(d.updated_at),
        })
        .collect();
    let folder = state.labels.clone();
    let others = tokio::task::spawn_blocking(move || folder.list())
        .await
        .unwrap_or_default();
    let builtin = labels::builtin_manifests(&state.label_renderer).await;
    for d in others {
        if out.iter().all(|o| o.id != d.id) {
            let manifest = d.builtin.then(|| builtin.get(&d.id)).flatten();
            out.push(DesignItem {
                description: manifest
                    .and_then(|m| m["description"].as_str())
                    .map(str::to_string),
                params: param_names(manifest),
                used_by: users.remove(&d.id).unwrap_or_default(),
                // The manifest's short name ("Shop") over the list's longer one
                name: manifest
                    .and_then(|m| m["name"].as_str())
                    .map_or(d.name, str::to_string),
                id: d.id,
                kind: if d.builtin { "builtin" } else { "folder" }.into(),
                version: None,
                updated_at: None,
            });
        }
    }
    Ok(out)
}

/// Every label design: saved (with their current version and parameters), the owner's
/// folder, built in.
#[utoipa::path(get, path = "/api/label-designs", tag = "labels",
    responses((status = 200, body = Vec<DesignItem>)))]
async fn list_designs(State(state): State<AppState>) -> Response {
    match all_designs(&state).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, IntoParams)]
pub struct VersionQuery {
    /// Another version than the current one
    version: Option<i64>,
}

/// A design with its source: saved (`sam-gift`, or `sam-gift@2` for a version, with every
/// version listed), or the owner's folder or a built-in one (`kind`), to start from.
#[utoipa::path(get, path = "/api/label-designs/{id}", tag = "labels",
    params(("id" = String, Path, description = "the design, `id@N` for a version"), VersionQuery),
    responses((status = 200, body = store::SavedDesign), (status = 404, body = ErrorBody)))]
async fn get_design(
    State(state): State<AppState>,
    Path(reference): Path<String>,
    Query(q): Query<VersionQuery>,
) -> Response {
    let Some((id, at)) = store::parse_ref(&reference) else {
        return status(StatusCode::BAD_REQUEST, "a design is its id, or id@version");
    };
    match store::design(&state.db, &id, at.or(q.version)).await {
        Ok(Some(d)) => return Json(d).into_response(),
        Ok(None) => {}
        Err(e) => return failed(e),
    }
    // Not saved: the folder's or a built-in one, as version 0
    let folder = state.labels.clone();
    let wanted = id.clone();
    let other = tokio::task::spawn_blocking(move || folder.design(&wanted))
        .await
        .ok()
        .flatten();
    match other {
        Some(src) => {
            let builtin = labels::builtin_design(&id).is_some_and(|b| b.source == src.source);
            let manifest = if builtin {
                labels::builtin_manifests(&state.label_renderer)
                    .await
                    .get(&id)
                    .cloned()
            } else {
                None
            };
            let field = |k: &str| {
                manifest
                    .as_ref()
                    .and_then(|m| m[k].as_str())
                    .map(str::to_string)
            };
            Json(json!({
                "id": id,
                "name": field("name").unwrap_or_else(|| id.clone()),
                "description": field("description"),
                "kind": if builtin { "builtin" } else { "folder" },
                "version": 0,
                "shown": 0,
                "source": src.source,
                "manifest": manifest,
                "versions": [],
            }))
            .into_response()
        }
        None => status(
            StatusCode::NOT_FOUND,
            format!("no design {id:?} (GET /api/label-designs)"),
        ),
    }
}

/// A design to save.
#[derive(Deserialize, ToSchema)]
pub struct NewDesign {
    /// Letters, digits, `-` and `_`; not a built-in's or a folder design's
    id: String,
    /// Default: the manifest's `design.name`, else the id
    name: Option<String>,
    description: Option<String>,
    /// The Typst source
    source: String,
    /// What changed (kept with the version)
    note: Option<String>,
    /// The sheet the save's check uses (default 5163)
    sheet: Option<String>,
}

/// What a save answers: the design and the check of the version saved.
#[derive(Serialize, ToSchema)]
pub struct Saved {
    design: store::SavedDesign,
    /// The check on sample data (`POST /api/labels/check`): issues do not stop a save,
    /// errors do
    report: Option<Report>,
}

/// Check a source on sample data: the report, or 422 with Typst's errors.
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn checked(state: &AppState, source: &str, sheet: Option<&str>) -> Result<Report, Response> {
    if source.len() > super::labels::MAX_SOURCE {
        return Err(status(
            StatusCode::BAD_REQUEST,
            "a design's source is at most 256 KB",
        ));
    }
    let job = super::labels::job_for_check(state, source, sheet).await?;
    match labels::check(&state.label_renderer, job, None).await {
        Ok(c) if c.errors.is_empty() => Ok(c.report.unwrap_or_else(unreachable_report)),
        Ok(c) => Err(status(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "the design does not compile, nothing was saved (`roast labels design check` shows where):\n{}",
                c.errors
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        )),
        Err(e) => Err(status(StatusCode::BAD_REQUEST, e.to_string())),
    }
}

fn unreachable_report() -> Report {
    serde_json::from_value(json!({"ok": false, "w": 0.0, "h": 0.0, "issues": [], "lines": [], "fields": {"shown": [], "missing": []}}))
        .expect("a report")
}

/// Save a new design (version 1). It must compile: the answer has the check's report.
#[utoipa::path(post, path = "/api/label-designs", tag = "labels",
    request_body = NewDesign,
    responses((status = 201, body = Saved), (status = 400, body = ErrorBody),
        (status = 409, description = "The id is taken (saved, built in or in the folder)", body = ErrorBody),
        (status = 422, description = "It does not compile: Typst's errors with lines", body = ErrorBody)))]
async fn create_design(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    Json(d): Json<NewDesign>,
) -> Response {
    if !labels::valid_id(&d.id) {
        return status(
            StatusCode::BAD_REQUEST,
            "a design's id is letters, digits, - and _ (up to 64)",
        );
    }
    let folder = state.labels.clone();
    let id = d.id.clone();
    let taken = tokio::task::spawn_blocking(move || folder.design(&id).is_some())
        .await
        .unwrap_or(false);
    if taken {
        return status(
            StatusCode::CONFLICT,
            format!("{} is a built-in or folder design: choose another id", d.id),
        );
    }
    let report = match checked(&state, &d.source, d.sheet.as_deref()).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let name = d
        .name
        .clone()
        .or_else(|| {
            report
                .manifest
                .as_ref()
                .and_then(|m| m["name"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| d.id.clone());
    let description = d.description.clone().or_else(|| {
        report
            .manifest
            .as_ref()
            .and_then(|m| m["description"].as_str())
            .map(str::to_string)
    });
    let who = by(actor.as_deref());
    match store::create(
        &state.db,
        &d.id,
        &name,
        description.as_deref(),
        &d.source,
        report.manifest.as_ref(),
        d.note.as_deref(),
        &who,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            return status(
                StatusCode::CONFLICT,
                format!(
                    "there is a design {:?}: edit it (PUT) for a new version",
                    d.id
                ),
            )
        }
        Err(e) => return failed(e),
    }
    match store::design(&state.db, &d.id, None).await {
        Ok(Some(design)) => (
            StatusCode::CREATED,
            Json(Saved {
                design,
                report: Some(report),
            }),
        )
            .into_response(),
        Ok(None) => status(StatusCode::INTERNAL_SERVER_ERROR, "the design vanished"),
        Err(e) => failed(e),
    }
}

/// A change to a saved design: a new source is a new version.
#[derive(Deserialize, ToSchema)]
pub struct DesignEdit {
    source: Option<String>,
    name: Option<String>,
    description: Option<String>,
    note: Option<String>,
    sheet: Option<String>,
}

/// Change a saved design: a new source becomes the next version (checked first, as a save
/// is); name and description change in place.
#[utoipa::path(put, path = "/api/label-designs/{id}", tag = "labels",
    params(("id" = String, Path)),
    request_body = DesignEdit,
    responses((status = 200, body = Saved), (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody)))]
async fn edit_design(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    Json(e): Json<DesignEdit>,
) -> Response {
    let report = match &e.source {
        Some(src) => match checked(&state, src, e.sheet.as_deref()).await {
            Ok(r) => Some(r),
            Err(r) => return r,
        },
        None => None,
    };
    let who = by(actor.as_deref());
    let source = e
        .source
        .as_deref()
        .map(|s| (s, report.as_ref().and_then(|r| r.manifest.as_ref())));
    let done = store::edit(
        &state.db,
        &id,
        store::Edit {
            source,
            name: e.name.as_deref(),
            description: e.description.as_deref(),
            note: e.note.as_deref(),
            by: &who,
        },
    )
    .await;
    match done {
        Ok(Some(_)) => {}
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no saved design {id:?}")),
        Err(e) => return failed(e),
    }
    match store::design(&state.db, &id, None).await {
        Ok(Some(design)) => Json(Saved { design, report }).into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no saved design {id:?}")),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct Rollback {
    /// The version to go back to: it becomes the next version, so nothing is lost
    version: i64,
    note: Option<String>,
}

/// Go back to an earlier version: its source becomes the next version.
#[utoipa::path(post, path = "/api/label-designs/{id}/rollback", tag = "labels",
    params(("id" = String, Path)),
    request_body = Rollback,
    responses((status = 200, body = Saved), (status = 404, body = ErrorBody)))]
async fn rollback_design(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    Json(r): Json<Rollback>,
) -> Response {
    let old = match store::design(&state.db, &id, Some(r.version)).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return status(
                StatusCode::NOT_FOUND,
                format!("no version {} of {id:?}", r.version),
            )
        }
        Err(e) => return failed(e),
    };
    let who = by(actor.as_deref());
    let note = r
        .note
        .unwrap_or_else(|| format!("back to version {}", r.version));
    let done = store::edit(
        &state.db,
        &id,
        store::Edit {
            source: Some((&old.source, old.manifest.as_ref())),
            name: None,
            description: None,
            note: Some(&note),
            by: &who,
        },
    )
    .await;
    if let Err(e) = done {
        return failed(e);
    }
    match store::design(&state.db, &id, None).await {
        Ok(Some(design)) => Json(Saved {
            design,
            report: None,
        })
        .into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no saved design {id:?}")),
        Err(e) => failed(e),
    }
}

/// Delete a saved design and its versions (may wait for the owner's approval). Refused
/// while a coffee uses it.
#[utoipa::path(delete, path = "/api/label-designs/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody),
        (status = 409, description = "A bean's label uses it", body = ErrorBody)))]
async fn delete_design(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match store::beans_using(&state.db, &id).await {
        Ok(beans) if !beans.is_empty() => {
            return status(
                StatusCode::CONFLICT,
                format!(
                    "{} uses it as its label: give it another first",
                    beans.join(", ")
                ),
            )
        }
        Ok(_) => {}
        Err(e) => return failed(e),
    }
    match store::delete_design(&state.db, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no saved design {id:?}")),
        Err(e) => failed(e),
    }
}

// ---- assets ----

/// The images and fonts designs can use.
#[utoipa::path(get, path = "/api/label-assets", tag = "labels",
    responses((status = 200, body = Vec<store::Asset>)))]
async fn list_assets(State(state): State<AppState>) -> Response {
    match store::assets(&state.db).await {
        Ok(a) => Json(a).into_response(),
        Err(e) => failed(e),
    }
}

/// One asset's file.
#[utoipa::path(get, path = "/api/label-assets/{name}", tag = "labels",
    params(("name" = String, Path)),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, body = ErrorBody)))]
async fn get_asset(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    match store::asset_data(&state.db, &name).await {
        Ok(Some((ct, data))) => asset_response(&name, ct, data),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no asset {name:?}")),
        Err(e) => failed(e),
    }
}

/// An asset is uploaded content served from the dashboard's own origin: an SVG opened on its
/// own must not run script there. A sandboxed CSP with no sources does that and changes
/// nothing for `<img>`, a `FontFace` or a download, which is how the dashboard uses assets.
pub(crate) const ASSET_CSP: &str =
    "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; sandbox";

pub(crate) fn asset_response(name: &str, ct: impl Into<String>, data: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, ct.into()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{name}\""),
            ),
            (header::CONTENT_SECURITY_POLICY, ASSET_CSP.to_string()),
        ],
        data,
    )
        .into_response()
}

#[cfg(test)]
mod asset_tests {
    use super::*;

    #[test]
    fn an_asset_is_served_sandboxed_with_no_script() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let res = asset_response("x.svg", "image/svg+xml", svg.to_vec());
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("sandbox"), "{csp}");
        assert!(csp.starts_with("default-src 'none'"), "{csp}");
        assert!(!csp.contains("script-src"), "{csp}");
    }
}

#[derive(Deserialize, IntoParams)]
pub struct AssetQuery {
    /// Where it came from, its licence
    note: Option<String>,
}

/// Add or replace an asset: the request's body is the file (SVG, PNG or JPEG up to 2 MB; a
/// TTF or OTF font up to 8 MB). A design names it: `image("logo.svg")`, or the font's family.
#[utoipa::path(put, path = "/api/label-assets/{name}", tag = "labels",
    params(("name" = String, Path, description = "with its type's ending: logo.svg, Caveat.ttf"), AssetQuery),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 200, body = store::Asset), (status = 201, body = store::Asset),
        (status = 400, body = ErrorBody)))]
async fn put_asset(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    Path(name): Path<String>,
    Query(q): Query<AssetQuery>,
    body: Bytes,
) -> Response {
    let (kind, ct, family) = match store::check_asset(&name, &body) {
        Ok(k) => k,
        Err(m) => return status(StatusCode::BAD_REQUEST, m),
    };
    let who = by(actor.as_deref());
    let created = match store::put_asset(
        &state.db,
        &name,
        kind,
        ct,
        &body,
        family.as_deref(),
        q.note.as_deref(),
        &who,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => return failed(e),
    };
    match store::asset(&state.db, &name).await {
        Ok(Some(a)) => (
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(a),
        )
            .into_response(),
        Ok(None) => status(StatusCode::INTERNAL_SERVER_ERROR, "the asset vanished"),
        Err(e) => failed(e),
    }
}

/// Delete an asset (may wait for the owner's approval).
#[utoipa::path(delete, path = "/api/label-assets/{name}", tag = "labels",
    params(("name" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_asset(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    match store::delete_asset(&state.db, &name).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no asset {name:?}")),
        Err(e) => failed(e),
    }
}

// ---- brands ----

/// The brand kits.
#[utoipa::path(get, path = "/api/label-brands", tag = "labels",
    responses((status = 200, body = Vec<store::Brand>)))]
async fn list_brands(State(state): State<AppState>) -> Response {
    match store::brands(&state.db).await {
        Ok(b) => Json(b).into_response(),
        Err(e) => failed(e),
    }
}

/// One brand kit.
#[utoipa::path(get, path = "/api/label-brands/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, body = store::Brand), (status = 404, body = ErrorBody)))]
async fn get_brand(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match store::brand(&state.db, &id).await {
        Ok(Some(b)) => Json(b).into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no brand {id:?}")),
        Err(e) => failed(e),
    }
}

/// Add or replace a brand kit: name, tagline, place, website, a logo (an image asset's name),
/// colours (`#rrggbb`) and fonts. A design reads it as `l.brand`.
#[utoipa::path(put, path = "/api/label-brands/{id}", tag = "labels",
    params(("id" = String, Path)),
    request_body = store::Brand,
    responses((status = 200, body = store::Brand), (status = 201, body = store::Brand),
        (status = 400, body = ErrorBody)))]
async fn put_brand(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut b): Json<store::Brand>,
) -> Response {
    if !labels::valid_id(&id) {
        return status(
            StatusCode::BAD_REQUEST,
            "a brand's id is letters, digits, - and _ (up to 64)",
        );
    }
    b.id = id.clone();
    if let Err(m) = b.check() {
        return status(StatusCode::BAD_REQUEST, m);
    }
    if let Some(logo) = &b.logo {
        match store::asset(&state.db, logo).await {
            Ok(Some(a)) if a.kind == "image" => {}
            Ok(_) => {
                return status(
                    StatusCode::BAD_REQUEST,
                    format!(
                    "the logo {logo:?} is not an image asset (PUT /api/label-assets/{logo} first)"
                ),
                )
            }
            Err(e) => return failed(e),
        }
    }
    let created = match store::put_brand(&state.db, &b).await {
        Ok(c) => c,
        Err(e) => return failed(e),
    };
    match store::brand(&state.db, &id).await {
        Ok(Some(b)) => (
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(b),
        )
            .into_response(),
        Ok(None) => status(StatusCode::INTERNAL_SERVER_ERROR, "the brand vanished"),
        Err(e) => failed(e),
    }
}

/// Delete a brand kit (may wait for the owner's approval).
#[utoipa::path(delete, path = "/api/label-brands/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_brand(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match store::delete_brand(&state.db, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no brand {id:?}")),
        Err(e) => failed(e),
    }
}

// ---- a coffee's label ----

/// Give a coffee (bean) its own label: the design (`id` or `id@N`), its parameters and a
/// brand, used for its roasts' labels unless a request says otherwise. All empty clears it.
#[utoipa::path(put, path = "/api/beans/{id}/label", tag = "labels",
    params(("id" = String, Path, description = "the bean's id")),
    request_body = store::BeanLabel,
    responses((status = 200, body = store::BeanLabel), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn set_bean_label(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(l): Json<store::BeanLabel>,
) -> Response {
    if let Some(d) = &l.design {
        let Some((design_id, version)) = store::parse_ref(d) else {
            return status(StatusCode::BAD_REQUEST, "a design is its id, or id@version");
        };
        let saved = match store::design(&state.db, &design_id, version).await {
            Ok(s) => s.is_some(),
            Err(e) => return failed(e),
        };
        let other = version.is_none() && {
            let folder = state.labels.clone();
            tokio::task::spawn_blocking(move || folder.design(&design_id).is_some())
                .await
                .unwrap_or(false)
        };
        if !saved && !other {
            return status(
                StatusCode::BAD_REQUEST,
                format!("no design {d:?} (GET /api/label-designs)"),
            );
        }
    }
    if let Some(b) = &l.brand {
        match store::brand(&state.db, b).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return status(
                    StatusCode::BAD_REQUEST,
                    format!("no brand {b:?} (GET /api/label-brands)"),
                )
            }
            Err(e) => return failed(e),
        }
    }
    match store::set_bean_label(&state.db, &id, &l).await {
        Ok(true) => Json(l).into_response(),
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no bean {id:?}")),
        Err(e) => failed(e),
    }
}
