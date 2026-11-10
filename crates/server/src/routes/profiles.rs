//! Roast profiles (docs/profiles.md): seed profiles, the editor's points, events and curves,
//! saving a roast as a profile, a profile's roasts, and Artisan import and export.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::http::errors::ErrorBody;
use crate::http::opt_json::OptJson;
use crate::profiles::history::ProfileHistoryRoast;
use crate::profiles::types::{
    CreateProfileRequest, ImportArtisanProfileRequest, ProfileWithPoints, RoastProfile,
    SaveSessionAsProfileRequest,
};
use crate::profiles::validate::ProfileInputError;
use crate::{app::state::AppState, profiles::artisan};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_list_profiles, api_create_profile))
        .routes(routes!(
            api_get_profile,
            api_update_profile,
            api_delete_profile
        ))
        .routes(routes!(api_profile_history))
        .routes(routes!(api_export_profile))
        .routes(routes!(api_export_profile_artisan))
        .routes(routes!(api_import_artisan_profile))
        .routes(routes!(api_save_session_as_profile))
}

/// Make a profile: BT points, fan, events, curves and the roast level it aims at
/// (docs/profiles.md).
#[utoipa::path(post, path = "/api/profiles", tag = "profiles", request_body = CreateProfileRequest,
    responses((status = 200, body = ProfileWithPoints), (status = 400, description = "Not a valid profile: the reason", body = ErrorBody)))]
async fn api_create_profile(
    State(state): State<AppState>,
    Json(req): Json<CreateProfileRequest>,
) -> Response {
    match crate::profiles::store::create_profile(&state.db, req).await {
        Ok(profile) => Json(profile).into_response(),
        Err(e) => profile_error(e, "Failed to create profile"),
    }
}

/// Bad input is a 400 with the reason; anything else is a logged 500.
fn profile_error(e: anyhow::Error, what: &str) -> Response {
    if let Some(bad) = e.downcast_ref::<ProfileInputError>() {
        return (StatusCode::BAD_REQUEST, bad.to_string()).into_response();
    }
    tracing::error!(?e, "{what}");
    (StatusCode::INTERNAL_SERVER_ERROR, what.to_string()).into_response()
}

/// Save a recorded roast as a new profile (points, events, curves).
#[utoipa::path(post, path = "/api/sessions/{id}/profile", tag = "profiles", params(("id" = String, Path)),
    request_body(content = Option<SaveSessionAsProfileRequest>, description = "Optional"),
    responses((status = 201, body = ProfileWithPoints), (status = 400, description = "Not a valid profile: the reason", body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn api_save_session_as_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
    OptJson(body): OptJson<SaveSessionAsProfileRequest>,
) -> Response {
    let req = body.unwrap_or_default();
    match crate::profiles::extract::save_session_as_profile(&state.db, &id, req).await {
        Ok(Some(profile)) => (StatusCode::CREATED, Json(profile)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Session not found").into_response(),
        Err(e) => profile_error(e, "Failed to save session as profile"),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ProfileListQuery {
    /// Include profiles marked private
    include_private: Option<bool>,
}

/// The profiles, seed profiles included.
#[utoipa::path(get, path = "/api/profiles", tag = "profiles", params(ProfileListQuery),
    responses((status = 200, body = Vec<RoastProfile>)))]
async fn api_list_profiles(
    State(state): State<AppState>,
    Query(q): Query<ProfileListQuery>,
) -> Response {
    match crate::profiles::store::list_profiles(&state.db, q.include_private.unwrap_or(false)).await
    {
        Ok(profiles) => Json(profiles).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to list profiles");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list profiles").into_response()
        }
    }
}

/// A profile with its points, events and curves.
#[utoipa::path(get, path = "/api/profiles/{id}", tag = "profiles", params(("id" = String, Path)),
    responses((status = 200, body = ProfileWithPoints), (status = 404, body = ErrorBody)))]
async fn api_get_profile(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::profiles::store::get_profile_with_points(&state.db, &id).await {
        Ok(Some(profile)) => Json(profile).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Profile not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to get profile");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get profile").into_response()
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ProfileHistoryQuery {
    /// Only roasts on this roaster
    device_id: Option<String>,
}

/// The completed roasts of a profile, newest first, with their landmarks: what roast
/// guidance judges temperatures and times against (docs/roast-guidance.md).
#[utoipa::path(get, path = "/api/profiles/{id}/history", tag = "profiles",
    params(("id" = String, Path), ProfileHistoryQuery),
    responses((status = 200, body = Vec<ProfileHistoryRoast>)))]
async fn api_profile_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ProfileHistoryQuery>,
) -> Response {
    match crate::profiles::history::profile_history(
        &state.db,
        &id,
        q.device_id.as_deref().filter(|d| !d.is_empty()),
    )
    .await
    {
        Ok(roasts) => Json(roasts).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to read profile history");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read profile history",
            )
                .into_response()
        }
    }
}

/// Replace a profile's points; events and curves are kept when absent.
#[utoipa::path(put, path = "/api/profiles/{id}", tag = "profiles", params(("id" = String, Path)),
    request_body = CreateProfileRequest,
    responses((status = 200, body = ProfileWithPoints), (status = 400, description = "Not a valid profile: the reason", body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn api_update_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateProfileRequest>,
) -> Response {
    match crate::profiles::store::update_profile(&state.db, &id, req).await {
        Ok(Some(profile)) => Json(profile).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Profile not found").into_response(),
        Err(e) => profile_error(e, "Failed to update profile"),
    }
}

/// Delete a profile.
#[utoipa::path(delete, path = "/api/profiles/{id}", tag = "profiles", params(("id" = String, Path)),
    responses((status = 204, description = "Deleted"), (status = 404, body = ErrorBody)))]
async fn api_delete_profile(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::profiles::store::delete_profile(&state.db, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Profile not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to delete profile");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete profile",
            )
                .into_response()
        }
    }
}

/// Download a profile as JSON that `POST /api/profiles/import/artisan` accepts back.
#[utoipa::path(get, path = "/api/profiles/{id}/export", tag = "profiles", params(("id" = String, Path)),
    responses((status = 200, body = ProfileWithPoints), (status = 404, body = ErrorBody)))]
async fn api_export_profile(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match crate::profiles::store::get_profile_with_points(&state.db, &id).await {
        Ok(Some(profile)) => {
            let filename = format!(
                "{}.roastwarden-profile.json",
                profile.profile.name.replace(' ', "_")
            );
            let body = serde_json::to_string_pretty(&profile).unwrap_or_default();
            let headers = [
                (CONTENT_TYPE, "application/json; charset=utf-8"),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ];
            (headers, body).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Profile not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to export profile");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export profile",
            )
                .into_response()
        }
    }
}

/// Download a profile as an Artisan `.alog`, loadable in Artisan as a roast or as a
/// background profile (docs/profiles.md, "Compatibility notes").
#[utoipa::path(get, path = "/api/profiles/{id}/export/artisan", tag = "profiles", params(("id" = String, Path)),
    responses((status = 200, content_type = "text/plain", body = String), (status = 404, body = ErrorBody),
        (status = 422, description = "The profile cannot be written as an Artisan file", body = ErrorBody)))]
async fn api_export_profile_artisan(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match crate::profiles::store::get_profile_with_points(&state.db, &id).await {
        Ok(Some(profile)) => match artisan::build_profile_alog(&profile) {
            Ok(alog) => {
                let filename = format!("{}.alog", safe_filename(&profile.profile.name));
                let headers = [
                    (CONTENT_TYPE, "text/plain; charset=utf-8".to_string()),
                    (
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{filename}\""),
                    ),
                ];
                (headers, artisan::render_alog(&alog)).into_response()
            }
            Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response(),
        },
        Ok(None) => (StatusCode::NOT_FOUND, "Profile not found").into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to export profile as Artisan");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export profile",
            )
                .into_response()
        }
    }
}

/// An ASCII file name from a title (header-safe): letters, digits, `-`, `_`, `.`; anything else becomes `_`.
fn safe_filename(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
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

/// Import an Artisan `.alog` (or a Roastwarden profile export) as a profile.
#[utoipa::path(post, path = "/api/profiles/import/artisan", tag = "profiles",
    request_body = ImportArtisanProfileRequest,
    responses((status = 200, body = ProfileWithPoints),
        (status = 400, description = "Not a profile it can read", body = ErrorBody)))]
async fn api_import_artisan_profile(
    State(state): State<AppState>,
    Json(req): Json<ImportArtisanProfileRequest>,
) -> Response {
    match crate::profiles::import::import_artisan_profile(&state.db, req).await {
        Ok(profile) => Json(profile).into_response(),
        Err(e) => {
            tracing::error!(?e, "Failed to import Artisan profile");
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to import Artisan profile: {e}"),
            )
                .into_response()
        }
    }
}
