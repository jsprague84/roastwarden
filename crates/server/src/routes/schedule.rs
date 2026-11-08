//! The device's PID gain schedule by fan % (`docs/autotune-2.md`, "Gain schedule").

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::control::schedule::{self, GainSchedule, ScheduleRow};
use crate::control::tuning::{AutotuneRun, Gains};
use crate::http::errors::ErrorBody;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_schedule, put_schedule))
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GainScheduleView {
    device_id: String,
    schedule: GainSchedule,
    /// The roaster's fan now, %
    fan_pct: Option<f64>,
    /// The gains the schedule gives at that fan
    gains_now: Option<Gains>,
    /// The gains just sent, when the save sent any
    applied: Option<Gains>,
    /// What to look at in the schedule, in words (D terms on this sensor, rows that disagree)
    advice: Vec<String>,
}

/// The device's fan % now, from its latest telemetry.
fn current_fan_pct(state: &AppState, device_id: &str) -> Option<f64> {
    state.telemetry_service.latest_sample(device_id)?.fan
}

fn view(state: &AppState, device_id: &str, s: &GainSchedule, applied: Option<Gains>) -> Response {
    let fan_now = current_fan_pct(state, device_id);
    Json(GainScheduleView {
        device_id: device_id.to_string(),
        schedule: s.clone(),
        fan_pct: fan_now,
        gains_now: fan_now.and_then(|f| s.gains_at(f)),
        applied,
        advice: schedule::advice(s),
    })
    .into_response()
}

/// The roaster's PID gains by fan % (docs/autotune-2.md, "Gain schedule").
#[utoipa::path(get, path = "/api/roaster/{device_id}/gain_schedule", tag = "roasters",
    params(("device_id" = String, Path)), responses((status = 200, body = GainScheduleView)))]
async fn get_schedule(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    let s = state.gain_scheduler.schedule(&device_id).await;
    view(&state, &device_id, &s, None)
}

/// Save a schedule and, when enabled, send the gains for the current fan.
async fn save_and_apply(state: &AppState, device_id: &str, mut s: GainSchedule) -> Response {
    if let Err(e) = s.normalize() {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    if let Err(e) = schedule::save(&state.db, device_id, &s).await {
        tracing::warn!(error = %e, device_id, "could not save the gain schedule");
        return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response();
    }
    state.gain_scheduler.forget(device_id).await;
    let applied = match current_fan_pct(state, device_id) {
        Some(f) if s.enabled => state.gain_scheduler.fan_changed(device_id, f).await,
        _ => None,
    };
    view(state, device_id, &s, applied)
}

/// Save the schedule; when it is on, the gains for the fan now go to the roaster.
#[utoipa::path(put, path = "/api/roaster/{device_id}/gain_schedule", tag = "roasters",
    params(("device_id" = String, Path)), request_body = GainSchedule,
    responses((status = 200, body = GainScheduleView), (status = 400, body = ErrorBody)))]
async fn put_schedule(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(mut s): Json<GainSchedule>,
) -> Response {
    // Rows still naming an autotune rule: is that what their gains are?
    let mut rule_gains = std::collections::HashMap::new();
    for r in &s.rows {
        let (Some(run_id), Some(rule)) = (r.run_id.as_deref(), r.rule.as_deref()) else {
            continue;
        };
        let run = crate::control::tuning::get_run_with_analysis(&state.db, &device_id, run_id)
            .await
            .ok()
            .flatten();
        let tau_c = r.tau_c;
        let g = run.and_then(|(run, analysis)| {
            if run.is_server() {
                let rule = roastwarden_autotune::design::Rule::from_id(rule)?;
                let d = roastwarden_autotune::analysis::design_at(
                    analysis.as_ref()?,
                    rule,
                    tau_c.unwrap_or(roastwarden_autotune::design::TAU_C_DEFAULT),
                )?;
                let g = crate::control::tuning::device_gains(&d.pid);
                return Some(Gains {
                    kp: round(g.kp, 3),
                    ki: round(g.ki, 5),
                    kd: 0.0,
                });
            }
            let rule = crate::control::tuning::rule(rule)?;
            let g = crate::control::tuning::gains_for(rule, run.ku?, run.tu?)?.gains();
            Some(Gains {
                kp: round(g.kp, 3),
                ki: round(g.ki, 5),
                kd: round(g.kd, 3),
            })
        });
        rule_gains.insert((run_id.to_string(), rule.to_string()), g);
    }
    s.mark_hand_edits(|run, rule| {
        rule_gains
            .get(&(run.to_string(), rule.to_string()))
            .copied()
            .flatten()
    });
    save_and_apply(&state, &device_id, s).await
}

/// What a server run adds to its row: the model the gains were designed on and SIMC's knob.
pub(crate) struct RowModel {
    pub model: roastwarden_autotune::Model,
    pub tau_c: Option<f64>,
}

/// Add (or replace) the row at the run's fan %. The first row turns the
/// schedule on.
pub(crate) async fn add_run_to_schedule(
    state: &AppState,
    device_id: &str,
    run: &AutotuneRun,
    rule: &str,
    gains: Gains,
    model: Option<RowModel>,
) -> Response {
    let Some(fan_pct) = run.fan_pct else {
        return (
            StatusCode::CONFLICT,
            "the run did not report its fan speed (firmware < 3.2)",
        )
            .into_response();
    };
    let mut s = state.gain_scheduler.schedule(device_id).await;
    if s.rows.is_empty() {
        s.enabled = true;
    }
    s.upsert(ScheduleRow {
        fan_pct: (fan_pct * 10.0).round() / 10.0,
        kp: round(gains.kp, 3),
        ki: round(gains.ki, 5),
        kd: round(gains.kd, 3),
        ff: run.bias,
        run_id: Some(run.id.clone()),
        rule: Some(rule.to_string()),
        note: None,
        tau_c: model.as_ref().and_then(|m| m.tau_c),
        model: model.map(|m| m.model),
    });
    save_and_apply(state, device_id, s).await
}

fn round(v: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (v * f).round() / f
}
