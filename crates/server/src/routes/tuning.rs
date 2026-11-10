//! Autotune runs, applying their gains, and the stored-gain history with
//! undo (`docs/autotune-2.md`, "Stored gains and undo").

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::control::tuning::{self, AutotuneRunView, Gains, NewPidChange, PidChange};
use crate::http::errors::ErrorBody;
use crate::{app::dispatch::PublishOpts, app::dispatch::dispatch, app::state::AppState};
use roastwarden_core::DeviceCommand;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_runs))
        .routes(routes!(get_run))
        .routes(routes!(apply_run))
        .routes(routes!(run_samples))
        .routes(routes!(run_design))
        .routes(routes!(analyse_run))
        .routes(routes!(pid_history))
        .routes(routes!(pid_undo))
}

#[derive(Serialize, ToSchema)]
struct RunList {
    device_id: String,
    /// Newest first
    runs: Vec<AutotuneRunView>,
}

#[derive(Serialize, ToSchema)]
struct PidHistory {
    device_id: String,
    /// The gains in the roaster's latest telemetry
    current: Option<Gains>,
    /// Stored changes, newest first
    changes: Vec<PidChange>,
}

#[derive(Serialize, ToSchema)]
struct Undone {
    restored: Gains,
    /// The change undone
    undid: i64,
}

fn db_error(e: sqlx::Error) -> Response {
    tracing::warn!(error = %e, "tuning query failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response()
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct LimitQuery {
    /// 1 to 500, default 50
    limit: Option<i64>,
}

/// The roaster's autotune runs (docs/autotune-2.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/autotune/runs", tag = "autotune",
    params(("device_id" = String, Path), LimitQuery), responses((status = 200, body = RunList)))]
async fn list_runs(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(q): Query<LimitQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    match tuning::list_runs_with_analysis(&state.db, &device_id, limit).await {
        Ok(runs) => Json(RunList {
            device_id,
            runs: runs.into_iter().map(|(r, a)| r.view(a)).collect(),
        })
        .into_response(),
        Err(e) => db_error(e),
    }
}

/// One autotune run with the gains each rule gives.
#[utoipa::path(get, path = "/api/roaster/{device_id}/autotune/runs/{run_id}", tag = "autotune",
    params(("device_id" = String, Path), ("run_id" = String, Path)),
    responses((status = 200, body = AutotuneRunView), (status = 404, body = ErrorBody)))]
async fn get_run(
    State(state): State<AppState>,
    Path((device_id, run_id)): Path<(String, String)>,
) -> Response {
    match tuning::get_run_with_analysis(&state.db, &device_id, &run_id).await {
        Ok(Some((run, a))) => Json(run.view(a)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "autotune run not found").into_response(),
        Err(e) => db_error(e),
    }
}

/// A server run's record: time from the start (s), the heater in force over each interval
/// (%), BT (°C) and fan (%) (docs/autotune-2.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/autotune/runs/{run_id}/samples", tag = "autotune",
    params(("device_id" = String, Path), ("run_id" = String, Path)),
    responses((status = 200, body = crate::control::tune::Samples), (status = 404, body = ErrorBody)))]
async fn run_samples(
    State(state): State<AppState>,
    Path((device_id, run_id)): Path<(String, String)>,
) -> Response {
    let run = tuning::get_run_with_analysis(&state.db, &device_id, &run_id)
        .await
        .ok()
        .flatten();
    match tuning::get_samples(&state.db, &device_id, &run_id).await {
        Ok(Some(s)) => {
            let summary = run.as_ref().and_then(|(r, _)| {
                serde_json::from_value::<roastwarden_autotune::relay::RelaySummary>(
                    r.payload.clone(),
                )
                .ok()
            });
            let s = match (summary, run.and_then(|(_, a)| a)) {
                (Some(sm), Some(a)) => tokio::task::spawn_blocking(move || s.with_model(&sm, &a))
                    .await
                    .unwrap_or_default(),
                _ => s,
            };
            Json(s).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            "no record for this run (a firmware run keeps none)",
        )
            .into_response(),
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct DesignQuery {
    /// simc_pi (default), amigo_pi, zn_pi, tyreus_luyben_pi
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    rule: Option<String>,
    /// SIMC's knob: τc as a multiple of the dead time, 0.5–5 (default 1.5): 1 tight, 3 smooth
    tau_c: Option<f64>,
}

/// A design and the gains it sends.
#[derive(Serialize, ToSchema)]
struct DesignView {
    design: roastwarden_autotune::design::Design,
    /// In the roaster's units (0–255 per °C), D off: what Apply sends
    gains: Gains,
}

fn rule_and_tau(
    rule: Option<&str>,
    tau_c: Option<f64>,
) -> Result<(roastwarden_autotune::design::Rule, f64), String> {
    use roastwarden_autotune::design::{Rule, TAU_C_DEFAULT};
    let rule = match rule {
        None => Rule::Simc,
        Some(id) => Rule::from_id(id).ok_or_else(|| {
            let names: Vec<&str> = Rule::ALL
                .iter()
                .map(roastwarden_autotune::design::Rule::id)
                .collect();
            format!("rule must be one of: {}", names.join(", "))
        })?,
    };
    let tau = tau_c.unwrap_or(TAU_C_DEFAULT);
    if !(tau.is_finite() && (0.5..=5.0).contains(&tau)) {
        return Err("tau_c must be 0.5..5".into());
    }
    Ok((rule, tau))
}

/// Preview a design on a server run's model: the PI gains, Ms, margins, jitter and ramp lag
/// (read-only; Apply sends it).
#[utoipa::path(get, path = "/api/roaster/{device_id}/autotune/runs/{run_id}/design", tag = "autotune",
    params(("device_id" = String, Path), ("run_id" = String, Path), DesignQuery),
    responses((status = 200, body = DesignView), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 409, description = "Not a server run, or no model", body = ErrorBody)))]
async fn run_design(
    State(state): State<AppState>,
    Path((device_id, run_id)): Path<(String, String)>,
    Query(q): Query<DesignQuery>,
) -> Response {
    let (rule, tau) = match rule_and_tau(q.rule.as_deref(), q.tau_c) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let analysis = match tuning::get_run_with_analysis(&state.db, &device_id, &run_id).await {
        Ok(Some((_, Some(a)))) => a,
        Ok(Some((_, None))) => {
            return (
                StatusCode::CONFLICT,
                "the run has no analysis (a firmware run?)",
            )
                .into_response();
        }
        Ok(None) => return (StatusCode::NOT_FOUND, "autotune run not found").into_response(),
        Err(e) => return db_error(e),
    };
    match roastwarden_autotune::analysis::design_at(&analysis, rule, tau) {
        Some(design) => Json(DesignView {
            gains: tuning::device_gains(&design.pid),
            design,
        })
        .into_response(),
        None => (
            StatusCode::CONFLICT,
            "the run's model gives no gains for this rule",
        )
            .into_response(),
    }
}

/// Analyse a server run's record again (after the analysis changed), and store the result.
#[utoipa::path(post, path = "/api/roaster/{device_id}/autotune/runs/{run_id}/analyse", tag = "autotune",
    params(("device_id" = String, Path), ("run_id" = String, Path)),
    responses((status = 200, body = AutotuneRunView), (status = 404, body = ErrorBody),
        (status = 409, description = "Not a server run", body = ErrorBody)))]
async fn analyse_run(
    State(state): State<AppState>,
    Path((device_id, run_id)): Path<(String, String)>,
) -> Response {
    let run = match tuning::get_run(&state.db, &device_id, &run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "autotune run not found").into_response(),
        Err(e) => return db_error(e),
    };
    let samples = match tuning::get_samples(&state.db, &device_id, &run_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::CONFLICT,
                "the run keeps no record (a firmware run)",
            )
                .into_response();
        }
        Err(e) => return db_error(e),
    };
    let Ok(summary) =
        serde_json::from_value::<roastwarden_autotune::relay::RelaySummary>(run.payload.clone())
    else {
        return (
            StatusCode::CONFLICT,
            "the run's experiment summary cannot be read",
        )
            .into_response();
    };
    let Ok(analysis) = tokio::task::spawn_blocking(move || {
        crate::control::tune::analyse_samples(&samples, &summary)
    })
    .await
    else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "the analysis failed").into_response();
    };
    let run = crate::control::tune::with_analysis(run, &analysis);
    if let Err(e) = tuning::set_analysis(&state.db, &run, &analysis).await {
        return db_error(e);
    }
    Json(run.view(Some(analysis))).into_response()
}

/// The gains the device runs with now, from its latest telemetry.
pub(crate) fn current_gains(state: &AppState, device_id: &str) -> Option<Gains> {
    let s = state.telemetry_service.latest_sample(device_id)?;
    let g = Gains {
        kp: *s.extras.get("Kp")?,
        ki: *s.extras.get("Ki")?,
        kd: *s.extras.get("Kd")?,
    };
    g.is_valid().then_some(g)
}

/// Send gains to be stored in the device and record the change.
pub(crate) async fn store_gains(
    state: &AppState,
    device_id: &str,
    gains: Gains,
    source: &str,
    run_id: Option<&str>,
    rule: Option<&str>,
    opts: &PublishOpts,
) -> Response {
    // What undo goes back to is the set stored before: the last recorded change.
    // Telemetry is only the fallback, since it shows the gains in RAM, which are
    // the gain schedule's while it runs (sent with `persist: false`).
    let stored = tuning::list_changes(&state.db, device_id, 1)
        .await
        .ok()
        .and_then(|mut v| v.pop())
        .map(|c| c.gains);
    let previous = stored.or_else(|| current_gains(state, device_id));
    let res = dispatch(
        state,
        device_id,
        DeviceCommand::Pid {
            kp: gains.kp,
            ki: gains.ki,
            kd: gains.kd,
            persist: true,
        },
        opts,
    )
    .await;
    if res.status().is_success() {
        state.gain_scheduler.forget(device_id).await;
        if let Err(e) = tuning::record_change(
            &state.db,
            NewPidChange {
                device_id,
                gains,
                previous,
                source,
                run_id,
                rule,
            },
        )
        .await
        {
            tracing::warn!(error = %e, device_id, "could not record the PID change");
        }
    }
    res
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApplyTo {
    /// Store the gains in the device.
    #[default]
    Device,
    /// Put them in the device's gain schedule at the run's fan %.
    Schedule,
}

#[derive(Deserialize, ToSchema)]
struct ApplyRequest {
    /// A rule id from the run's `rules`; for a server run simc_pi, amigo_pi, zn_pi or
    /// tyreus_luyben_pi
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    rule: String,
    #[serde(default)]
    to: ApplyTo,
    /// A server run's SIMC knob (τc as a multiple of the dead time, 0.5–5, default 1.5)
    #[serde(default)]
    tau_c: Option<f64>,
}

/// Store a rule's gains in the roaster, or add them to its gain schedule at the run's fan %.
#[utoipa::path(post, path = "/api/roaster/{device_id}/autotune/runs/{run_id}/apply",
    tag = "autotune",
    params(("device_id" = String, Path), ("run_id" = String, Path), PublishOpts),
    request_body = ApplyRequest,
    responses((status = 204, description = "Sent to the roaster"),
        (status = 200, description = "Added to the gain schedule", body = crate::routes::schedule::GainScheduleView),
        (status = 400, description = "No such rule", body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, description = "The run cannot be applied", body = ErrorBody)))]
async fn apply_run(
    State(state): State<AppState>,
    Path((device_id, run_id)): Path<(String, String)>,
    Query(opts): Query<PublishOpts>,
    Json(req): Json<ApplyRequest>,
) -> Response {
    let (run, analysis) = match tuning::get_run_with_analysis(&state.db, &device_id, &run_id).await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "autotune run not found").into_response(),
        Err(e) => return db_error(e),
    };
    if run.is_server() {
        return apply_server_run(&state, &device_id, &run, analysis.as_ref(), &req, &opts).await;
    }
    let (ku, tu) = match run.usable() {
        Ok(v) => v,
        Err(why) => return (StatusCode::CONFLICT, format!("cannot apply: {why}")).into_response(),
    };
    let Some(rule) = tuning::rule(&req.rule) else {
        let names: Vec<&str> = tuning::RULES.iter().map(|r| r.id).collect();
        return (
            StatusCode::BAD_REQUEST,
            format!("rule must be one of: {}", names.join(", ")),
        )
            .into_response();
    };
    let Some(g) = tuning::gains_for(rule, ku, tu) else {
        return (StatusCode::CONFLICT, "the run's Ku/Tu give no gains").into_response();
    };
    match req.to {
        ApplyTo::Device => {
            store_gains(
                &state,
                &device_id,
                g.gains(),
                "autotune",
                Some(&run.id),
                Some(rule.id),
                &opts,
            )
            .await
        }
        ApplyTo::Schedule => {
            crate::routes::schedule::add_run_to_schedule(
                &state,
                &device_id,
                &run,
                rule.id,
                g.gains(),
                None,
            )
            .await
        }
    }
}

/// Apply a server run's design: refused for a poor analysis or a design with too little
/// margin (Ms above 2).
async fn apply_server_run(
    state: &AppState,
    device_id: &str,
    run: &tuning::AutotuneRun,
    analysis: Option<&roastwarden_autotune::analysis::Analysis>,
    req: &ApplyRequest,
    opts: &PublishOpts,
) -> Response {
    use roastwarden_autotune::design::Verdict;
    if let Err(why) = tuning::server_usable(run, analysis) {
        return (StatusCode::CONFLICT, format!("cannot apply: {why}")).into_response();
    }
    let (rule, tau) = match rule_and_tau(Some(&req.rule), req.tau_c) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let Some(a) = analysis else {
        return (
            StatusCode::CONFLICT,
            "cannot apply: the run has no analysis",
        )
            .into_response();
    };
    let Some(design) = roastwarden_autotune::analysis::design_at(a, rule, tau) else {
        return (
            StatusCode::CONFLICT,
            "the run's model gives no gains for this rule",
        )
            .into_response();
    };
    if design.verdict == Verdict::Refuse {
        return (
            StatusCode::CONFLICT,
            format!(
                "cannot apply {}: Ms {:.2} leaves too little margin (above 2.0)",
                design.label, design.robustness.ms
            ),
        )
            .into_response();
    }
    let gains = tuning::device_gains(&design.pid);
    match req.to {
        ApplyTo::Device => {
            store_gains(
                state,
                device_id,
                gains,
                "autotune",
                Some(&run.id),
                Some(rule.id()),
                opts,
            )
            .await
        }
        ApplyTo::Schedule => {
            crate::routes::schedule::add_run_to_schedule(
                state,
                device_id,
                run,
                rule.id(),
                gains,
                Some(crate::routes::schedule::RowModel {
                    model: a
                        .design_model
                        .unwrap_or(roastwarden_autotune::Model::itd(1.0, 0.0)),
                    tau_c: (rule == roastwarden_autotune::design::Rule::Simc).then_some(tau),
                }),
            )
            .await
        }
    }
}

/// The gains stored in the roaster over time, with the ones in use now.
#[utoipa::path(get, path = "/api/roaster/{device_id}/control/pid/history", tag = "autotune",
    params(("device_id" = String, Path), LimitQuery), responses((status = 200, body = PidHistory)))]
async fn pid_history(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(q): Query<LimitQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    match tuning::list_changes(&state.db, &device_id, limit).await {
        Ok(changes) => Json(PidHistory {
            current: current_gains(&state, &device_id),
            device_id,
            changes,
        })
        .into_response(),
        Err(e) => db_error(e),
    }
}

/// Restore the gains in use before the last stored change.
#[utoipa::path(post, path = "/api/roaster/{device_id}/control/pid/undo", tag = "autotune",
    params(("device_id" = String, Path), PublishOpts),
    responses((status = 200, body = Undone),
        (status = 409, description = "Nothing to undo", body = ErrorBody)))]
async fn pid_undo(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(opts): Query<PublishOpts>,
) -> Response {
    let change = match tuning::last_undoable(&state.db, &device_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return (StatusCode::CONFLICT, "nothing to undo").into_response(),
        Err(e) => return db_error(e),
    };
    let Some(prev) = change.previous else {
        return (
            StatusCode::CONFLICT,
            "the gains before the last change are not known (no telemetry at the time)",
        )
            .into_response();
    };
    let res = store_gains(&state, &device_id, prev, "undo", None, None, &opts).await;
    if res.status().is_success() {
        if let Err(e) = tuning::mark_undone(&state.db, change.id).await {
            return db_error(e);
        }
        return Json(Undone {
            restored: prev,
            undid: change.id,
        })
        .into_response();
    }
    res
}
