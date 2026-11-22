//! A touch panel at the roaster (`docs/panel.md`): what its screen shows, in one small answer,
//! and its buttons. The server works out the roast's stage and which buttons do something
//! now, so a panel's firmware draws what it is given and sends a button's name.
//!
//! The buttons do what the Roast page's do, through the same handlers and the control gate.
//! They are the owner's and a "panel" token's (`auth/tokens.rs`): one made for this roaster.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use roastwarden_core::DeviceCommand;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::dispatch::{PublishOpts, dispatch};
use crate::app::state::AppState;
use crate::coffee::inventory::InventoryService;
use crate::http::errors::{ErrorBody, ErrorCode};
use crate::http::opt_json::OptJson;
use crate::notify::home_assistant::{Phase, phase};
use crate::roasters::setup::{Control, Limits};
use crate::roasts::types::{
    CreateRoastEventRequest, CreateSessionRequest, RoastEvent, RoastEventType, RoastSession,
};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_panel))
        .routes(routes!(press))
}

/// A roaster that has not reported for this long reads as offline on a panel.
const ONLINE_WITHIN_MS: u64 = 10_000;
/// How many roasts a panel offers to start.
const QUEUE_LEN: usize = 6;

/// Where the roast on this roaster is, as a panel shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum Stage {
    /// No roast open
    Idle,
    /// A roast is recording; the beans are not in yet
    Ready,
    /// CHARGE is marked
    Roasting,
    /// DROP is marked; the roast waits to be completed
    Dropped,
}

/// A panel's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum Action {
    /// Start a roast from the queue and record
    Start,
    Charge,
    DryEnd,
    FirstCrack,
    Drop,
    /// Finish the roast
    Complete,
    /// Discard the roast
    Cancel,
    /// Heater off, fan to full
    Stop,
    /// Fan, `percent`
    Fan,
    /// Heater, `percent`
    Heater,
}

impl Action {
    const ALL: [Action; 10] = [
        Action::Start,
        Action::Charge,
        Action::DryEnd,
        Action::FirstCrack,
        Action::Drop,
        Action::Complete,
        Action::Cancel,
        Action::Stop,
        Action::Fan,
        Action::Heater,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Action::Start => "start",
            Action::Charge => "charge",
            Action::DryEnd => "dry_end",
            Action::FirstCrack => "first_crack",
            Action::Drop => "drop",
            Action::Complete => "complete",
            Action::Cancel => "cancel",
            Action::Stop => "stop",
            Action::Fan => "fan",
            Action::Heater => "heater",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }

    /// The landmark a marking button marks.
    fn landmark(self) -> Option<RoastEventType> {
        match self {
            Action::Charge => Some(RoastEventType::Charge),
            Action::DryEnd => Some(RoastEventType::DryingEnd),
            Action::FirstCrack => Some(RoastEventType::FirstCrackStart),
            Action::Drop => Some(RoastEventType::Drop),
            _ => None,
        }
    }
}

/// The roast open on the roaster.
#[derive(Debug, Serialize, ToSchema)]
struct PanelRoast {
    id: String,
    name: String,
    batch: Option<i32>,
    /// Its profile's name
    profile: Option<String>,
    /// Seconds since CHARGE (since the roast started, before CHARGE); stands still at DROP
    time_s: i64,
    charged: bool,
    dry_end: bool,
    first_crack: bool,
    dropped: bool,
    /// Profile drive runs it
    drive: bool,
}

/// A roast a panel can start: a planned one, or one roasted before, again.
#[derive(Debug, Serialize, ToSchema)]
struct Recipe {
    /// `plan`: a planned roast; `again`: a finished roast's coffee, profile and weight
    kind: &'static str,
    /// The planned roast's id, or the finished roast's: what Start takes
    id: String,
    /// The line to show
    name: String,
    bean: Option<String>,
    profile: Option<String>,
    green_g: Option<f32>,
}

/// What a touch panel shows.
#[derive(Debug, Serialize, ToSchema)]
struct PanelState {
    device_id: String,
    /// The roaster's name (Roasters)
    name: String,
    /// It reported in the last ten seconds
    online: bool,
    /// Seconds since its newest reading; null before the first
    age_s: Option<u64>,
    /// °C
    bt: Option<f64>,
    /// °C
    et: Option<f64>,
    /// °C/min
    ror: Option<f64>,
    /// %
    heater: Option<f64>,
    /// %
    fan: Option<f64>,
    /// °C
    setpoint: Option<f64>,
    /// Heater power
    heater_on: Option<bool>,
    /// What the server may send this roaster (docs/roaster-setup.md)
    control: Control,
    stage: Stage,
    phase: Phase,
    /// The fan runs until the chamber is cold (docs/roast-sequence.md)
    cooling: bool,
    roast: Option<PanelRoast>,
    /// The buttons that do something now
    actions: Vec<Action>,
    /// What Start can start, the first by default: the planned roasts, then roasts to repeat.
    /// Empty while a roast is open.
    queue: Vec<Recipe>,
}

/// A button's body; every field optional.
#[derive(Debug, Default, Deserialize, ToSchema)]
struct Press {
    /// `start`: the planned roast to start
    plan: Option<String>,
    /// `start`: the finished roast to repeat
    again: Option<String>,
    /// `fan`, `heater`: 0–100 %
    percent: Option<f64>,
}

fn coded(status: StatusCode, code: &'static str, msg: impl Into<String>) -> Response {
    let mut res = (status, msg.into()).into_response();
    res.extensions_mut().insert(ErrorCode(code));
    res
}

fn failed(what: &str, e: impl std::fmt::Display) -> Response {
    tracing::error!(error = %e, "panel: {what}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Could not {what}"),
    )
        .into_response()
}

fn round(v: Option<f64>, places: i32) -> Option<f64> {
    let m = 10f64.powi(places);
    v.filter(|v| v.is_finite()).map(|v| (v * m).round() / m)
}

fn has(events: &[RoastEvent], t: &RoastEventType) -> bool {
    events.iter().any(|e| &e.event_type == t)
}

fn at(events: &[RoastEvent], t: &RoastEventType) -> Option<f64> {
    events
        .iter()
        .find(|e| &e.event_type == t)
        .map(|e| f64::from(e.elapsed_seconds))
}

/// Seconds since the roast started.
fn since_start(session: &RoastSession) -> f64 {
    session.start_time.map_or(0.0, |t| {
        ((Utc::now() - t).num_milliseconds() as f64 / 1000.0).max(0.0)
    })
}

/// The roast clock: from CHARGE (from the start before it), standing still at DROP.
fn roast_time(elapsed: f64, events: &[RoastEvent]) -> i64 {
    let from = at(events, &RoastEventType::Charge).unwrap_or(0.0);
    let to = at(events, &RoastEventType::Drop).unwrap_or(elapsed);
    (to - from).max(0.0).round() as i64
}

fn stage_of(open: Option<&[RoastEvent]>) -> Stage {
    match open {
        None => Stage::Idle,
        Some(ev) if has(ev, &RoastEventType::Drop) => Stage::Dropped,
        Some(ev) if has(ev, &RoastEventType::Charge) => Stage::Roasting,
        Some(_) => Stage::Ready,
    }
}

/// The buttons that do something at this stage, on a roaster with these limits.
fn actions_at(stage: Stage, events: &[RoastEvent], limits: &Limits) -> Vec<Action> {
    let mut out = match stage {
        Stage::Idle => vec![Action::Start],
        Stage::Ready => vec![Action::Charge, Action::Cancel],
        Stage::Roasting => {
            let mut v = Vec::new();
            if !has(events, &RoastEventType::DryingEnd)
                && !has(events, &RoastEventType::FirstCrackStart)
            {
                v.push(Action::DryEnd);
            }
            if !has(events, &RoastEventType::FirstCrackStart) {
                v.push(Action::FirstCrack);
            }
            v.push(Action::Drop);
            v.push(Action::Cancel);
            v
        }
        Stage::Dropped => vec![Action::Complete],
    };
    if limits.control.allows(&DeviceCommand::EmergencyStop) {
        out.push(Action::Stop);
    }
    if limits.outputs.fan.is_some() && limits.control.allows(&DeviceCommand::Fan { percent: 0.0 }) {
        out.push(Action::Fan);
    }
    if limits.outputs.heater.is_some()
        && limits
            .control
            .allows(&DeviceCommand::Heater { percent: 0.0 })
    {
        out.push(Action::Heater);
    }
    out
}

fn recipe_line(name: Option<&str>, bean: Option<&str>, profile: Option<&str>) -> String {
    let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    match (clean(name), clean(bean), clean(profile)) {
        (Some(n), _, _) => n,
        (None, Some(b), Some(p)) => format!("{b} · {p}"),
        (None, Some(one), None) | (None, None, Some(one)) => one,
        (None, None, None) => "Roast".to_string(),
    }
}

type Again = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f32>,
);

/// What Start can start on this roaster: its planned roasts in order, then the coffee and
/// profile pairs it roasted last, newest first.
async fn queue(state: &AppState, device_id: &str) -> Vec<Recipe> {
    let mut out: Vec<Recipe> = InventoryService::new(state.db.clone())
        .list_plans(Some(device_id), false)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| Recipe {
            kind: "plan",
            name: recipe_line(
                p.name.as_deref(),
                p.bean_name.as_deref(),
                p.profile_name.as_deref(),
            ),
            id: p.id,
            bean: p.bean_name,
            profile: p.profile_name,
            green_g: p.green_weight_g,
        })
        .collect();
    let recent: Vec<Again> = sqlx::query_as(
        "SELECT s.id, s.bean_id, b.name, s.profile_id, p.name, s.green_weight \
         FROM roast_sessions s \
         LEFT JOIN beans b ON b.id = s.bean_id \
         LEFT JOIN roast_profiles p ON p.id = s.profile_id \
         WHERE s.device_id = ? AND s.status = 'completed' \
           AND (b.id IS NOT NULL OR p.id IS NOT NULL) \
         ORDER BY s.end_time DESC LIMIT 40",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let mut seen: Vec<(Option<String>, Option<String>)> = Vec::new();
    for (id, bean_id, bean, profile_id, profile, green_g) in recent {
        let pair = (bean.as_ref().and(bean_id), profile.as_ref().and(profile_id));
        if seen.contains(&pair) {
            continue;
        }
        seen.push(pair);
        out.push(Recipe {
            kind: "again",
            id,
            name: recipe_line(None, bean.as_deref(), profile.as_deref()),
            bean,
            profile,
            green_g,
        });
    }
    out.truncate(QUEUE_LEN);
    out
}

async fn roaster_name(state: &AppState, device_id: &str) -> String {
    crate::roasters::registry::get_device_by_device_id(&state.db, device_id)
        .await
        .ok()
        .flatten()
        .map(|d| d.device.name)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| device_id.to_string())
}

async fn open_roast(state: &AppState, device_id: &str) -> Option<RoastSession> {
    crate::roasts::sessions::get_active_session(&state.db, device_id)
        .await
        .ok()
        .flatten()
}

async fn panel_state(state: &AppState, device_id: &str) -> PanelState {
    let sample = state.telemetry_service.latest_sample(device_id);
    let now_ms = u64::try_from(Utc::now().timestamp_millis()).unwrap_or(0);
    let age_ms = sample.as_ref().map(|s| now_ms.saturating_sub(s.ts_ms));
    let limits = state.transports.limits(device_id).await;
    let cooling = state.sequence.cooling(device_id).await;
    let session = open_roast(state, device_id).await;
    let events = match &session {
        Some(s) => crate::roasts::events::get_roast_events(&state.db, &s.id)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let open = session.as_ref().map(|_| events.as_slice());
    let stage = stage_of(open);
    let roast = match &session {
        Some(s) => {
            let profile: Option<String> = match &s.profile_id {
                Some(id) => sqlx::query_scalar("SELECT name FROM roast_profiles WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten(),
                None => None,
            };
            Some(PanelRoast {
                id: s.id.clone(),
                name: s.name.clone(),
                batch: s.batch_number,
                profile,
                time_s: roast_time(since_start(s), &events),
                charged: has(&events, &RoastEventType::Charge),
                dry_end: has(&events, &RoastEventType::DryingEnd),
                first_crack: has(&events, &RoastEventType::FirstCrackStart),
                dropped: has(&events, &RoastEventType::Drop),
                drive: state.replay.state(&s.id).await.is_some_and(|r| r.active),
            })
        }
        None => None,
    };
    let s = sample.as_ref();
    PanelState {
        device_id: device_id.to_string(),
        name: roaster_name(state, device_id).await,
        online: age_ms.is_some_and(|a| a < ONLINE_WITHIN_MS),
        age_s: age_ms.map(|a| a / 1000),
        bt: round(s.and_then(|s| s.bt), 1),
        et: round(s.and_then(|s| s.et), 1),
        ror: round(s.and_then(|s| s.ror_bt), 1),
        heater: round(s.and_then(|s| s.heater), 0),
        fan: round(s.and_then(|s| s.fan), 0),
        setpoint: round(s.and_then(|s| s.setpoint), 1),
        heater_on: s.and_then(|s| s.heater_enable),
        control: limits.control,
        stage,
        phase: phase(open, cooling),
        cooling,
        actions: actions_at(stage, &events, &limits),
        queue: if session.is_none() {
            queue(state, device_id).await
        } else {
            Vec::new()
        },
        roast,
    }
}

/// What a touch panel at this roaster shows: its readings, the roast's stage and clock, the
/// buttons that do something now, and what Start can start (docs/panel.md).
#[utoipa::path(get, path = "/api/roaster/{device_id}/panel", tag = "panel",
    params(("device_id" = String, Path)), responses((status = 200, body = PanelState)))]
async fn get_panel(State(state): State<AppState>, Path(device_id): Path<String>) -> Response {
    Json(panel_state(&state, &device_id).await).into_response()
}

/// The simulated roasters follow the buttons: CHARGE loads their beans and DROP dumps them,
/// as on the Roast page.
async fn simulated_beans(state: &AppState, device_id: &str, load: bool) {
    let vr = &state.virtual_roaster;
    if device_id == crate::sim::virtual_roaster::DEVICE_ID {
        match (load, vr.state().await.loaded) {
            (true, false) => vr.charge().await,
            (false, true) => vr.dump().await,
            _ => {}
        }
    } else if device_id == crate::sim::virtual_drum::DEVICE_ID {
        match (load, vr.drum.state().await.loaded) {
            (true, false) => vr.drum.charge().await,
            (false, true) => vr.drum.dump().await,
            _ => {}
        }
    }
}

/// Heater power off, when the roaster has a heater the server may switch: what the Roast
/// page sends with DROP and Complete.
async fn heater_power_off(state: &AppState, device_id: &str) {
    let limits = state.transports.limits(device_id).await;
    let off = DeviceCommand::HeaterEnable { enabled: false };
    if limits.outputs.heater.is_some() && limits.control.allows(&off) {
        let _ = dispatch(state, device_id, off, &opts()).await;
    }
}

fn opts() -> PublishOpts {
    PublishOpts {
        wait_ack: None,
        timeout_ms: None,
    }
}

fn no_roast() -> Response {
    coded(
        StatusCode::CONFLICT,
        "NO_ROAST",
        "No roast is open on this roaster: Start one first",
    )
}

/// Start a roast from the queue: the recipe the body names, else the first, else a plain
/// roast. It records at once; CHARGE is the next button.
async fn start(state: &AppState, device_id: &str, body: &Press) -> Response {
    if let Some(open) = open_roast(state, device_id).await {
        return coded(
            StatusCode::CONFLICT,
            "ROAST_OPEN",
            format!("\"{}\" is open on this roaster", open.name),
        );
    }
    let zone = crate::ops::zone::resolve(&state.db, None).await;
    let dated = format!("Roast {}", zone.local(Utc::now()).format("%Y-%m-%d %H:%M"));
    let mut req = CreateSessionRequest {
        name: dated.clone(),
        device_id: device_id.to_string(),
        profile_id: None,
        bean_origin: None,
        bean_variety: None,
        green_weight: None,
        target_roast_level: None,
        notes: None,
        ambient_temp: None,
        humidity: None,
        bean_id: None,
        planned_roast_id: None,
        roast_purpose: None,
    };
    let picked = match (&body.plan, &body.again) {
        (Some(id), _) => Some(("plan", id.clone())),
        (None, Some(id)) => Some(("again", id.clone())),
        (None, None) => queue(state, device_id)
            .await
            .into_iter()
            .next()
            .map(|r| (r.kind, r.id)),
    };
    match picked {
        Some(("plan", id)) => {
            let plan = match InventoryService::new(state.db.clone()).plan(&id).await {
                Ok(p) => p.filter(|p| p.session_id.is_none()),
                Err(e) => return failed("read the planned roast", e),
            };
            let Some(plan) = plan else {
                return coded(
                    StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "No such planned roast, or it already ran",
                );
            };
            if plan.device_id.as_deref().is_some_and(|d| d != device_id) {
                return coded(
                    StatusCode::CONFLICT,
                    "OTHER_ROASTER",
                    "That roast is planned for another roaster",
                );
            }
            // A plan's own name wins (roasts::sessions); else the roast is named after its bean
            if let Some(bean) = plan.bean_name.filter(|b| !b.trim().is_empty()) {
                req.name = bean;
            }
            if let Some(n) = plan.name.filter(|n| !n.trim().is_empty()) {
                req.name = n;
            }
            req.planned_roast_id = Some(id);
        }
        Some((_, id)) => {
            type Past = (Option<String>, Option<String>, Option<f32>, Option<String>);
            let past: Result<Option<Past>, sqlx::Error> = sqlx::query_as(
                "SELECT s.bean_id, s.profile_id, s.green_weight, b.name FROM roast_sessions s \
                 LEFT JOIN beans b ON b.id = s.bean_id \
                 WHERE s.id = ? AND s.device_id = ? AND s.status = 'completed'",
            )
            .bind(&id)
            .bind(device_id)
            .fetch_optional(&state.db)
            .await;
            match past {
                Ok(Some((bean_id, profile_id, green, bean))) => {
                    req.bean_id = bean.as_ref().and(bean_id);
                    req.profile_id = profile_id;
                    req.green_weight = green;
                    if let Some(bean) = bean.filter(|b| !b.trim().is_empty()) {
                        req.name = bean;
                    }
                }
                Ok(None) => {
                    return coded(
                        StatusCode::NOT_FOUND,
                        "NOT_FOUND",
                        "No such finished roast on this roaster",
                    );
                }
                Err(e) => return failed("read the roast to repeat", e),
            }
        }
        None => {}
    }
    let created = match crate::roasts::sessions::create_session(&state.db, req).await {
        Ok(s) => s,
        Err(e) => return failed("start the roast", e),
    };
    match crate::roasts::sessions::start_session(&state.db, &created.id).await {
        Ok(Some(_)) => Json(panel_state(state, device_id).await).into_response(),
        Ok(None) => failed("start the roast", "the new roast is not in planning"),
        Err(e) => failed("start the roast", e),
    }
}

/// Mark a landmark on the open roast, now, at the newest BT. CHARGE and DROP act on the
/// roaster (docs/roast-sequence.md).
async fn mark(state: &AppState, device_id: &str, event_type: RoastEventType) -> Response {
    let Some(session) = open_roast(state, device_id).await else {
        return no_roast();
    };
    // A second tap marks nothing: one of each on a roast from a panel
    let marked = crate::roasts::events::get_roast_events(&state.db, &session.id)
        .await
        .unwrap_or_default();
    if has(&marked, &event_type) {
        let what = match event_type {
            RoastEventType::Charge => "CHARGE",
            RoastEventType::DryingEnd => "Dry end",
            RoastEventType::FirstCrackStart => "First crack",
            RoastEventType::Drop => "DROP",
            _ => "That",
        };
        return coded(
            StatusCode::CONFLICT,
            "ALREADY_MARKED",
            format!("{what} is already marked on this roast"),
        );
    }
    let elapsed = (since_start(&session) * 10.0).round() / 10.0;
    let bt = state
        .telemetry_service
        .latest_sample(device_id)
        .and_then(|s| s.bt);
    let req = CreateRoastEventRequest {
        event_type: event_type.clone(),
        elapsed_seconds: elapsed as f32,
        temperature: bt.map(|v| v as f32),
        notes: None,
    };
    let res = crate::routes::roast_events::mark(state, &session.id, req).await;
    if !res.status().is_success() {
        return res;
    }
    match event_type {
        RoastEventType::Charge => simulated_beans(state, device_id, true).await,
        RoastEventType::Drop => {
            heater_power_off(state, device_id).await;
            simulated_beans(state, device_id, false).await;
        }
        _ => {}
    }
    Json(panel_state(state, device_id).await).into_response()
}

fn percent_of(body: &Press) -> Option<f64> {
    body.percent
        .filter(|p| p.is_finite() && (0.0..=100.0).contains(p))
}

fn bad_percent() -> Response {
    (StatusCode::BAD_REQUEST, "percent must be 0..100").into_response()
}

/// Press a button of this roaster's panel: `start`, `charge`, `dry_end`, `first_crack`,
/// `drop`, `complete`, `cancel`, `stop`, `fan` or `heater` (docs/panel.md). Each does what
/// the Roast page's button does; the answer is what the panel shows next. The owner's and a
/// panel token's alone.
#[utoipa::path(post, path = "/api/roaster/{device_id}/panel/{action}", tag = "panel",
    params(("device_id" = String, Path), ("action" = Action, Path)),
    request_body(content = Option<Press>, description = "`start`: `plan` or `again`; `fan`, `heater`: `percent`"),
    responses((status = 200, body = PanelState),
        (status = 400, description = "A percent that is none", body = ErrorBody),
        (status = 404, description = "No such button, planned roast or roast to repeat", body = ErrorBody),
        (status = 409, description = "`NO_ROAST`, `ROAST_OPEN`, `ALREADY_MARKED`, `OTHER_ROASTER`, or the control gate's `CONTROL_OFF`", body = ErrorBody)))]
async fn press(
    State(state): State<AppState>,
    Path((device_id, action)): Path<(String, String)>,
    OptJson(body): OptJson<Press>,
) -> Response {
    let Some(action) = Action::parse(&action) else {
        return (
            StatusCode::NOT_FOUND,
            format!("a panel has no \"{action}\" button"),
        )
            .into_response();
    };
    let body = body.unwrap_or_default();
    let state = &state;
    let device_id = device_id.as_str();
    if let Some(landmark) = action.landmark() {
        return mark(state, device_id, landmark).await;
    }
    let res = match action {
        Action::Start => return start(state, device_id, &body).await,
        Action::Complete => {
            let Some(session) = open_roast(state, device_id).await else {
                return no_roast();
            };
            heater_power_off(state, device_id).await;
            crate::routes::sessions::complete(state, &session.id).await
        }
        Action::Cancel => {
            let Some(session) = open_roast(state, device_id).await else {
                return no_roast();
            };
            match state.sequence.cancel(&session.id).await {
                Ok(_) => {
                    simulated_beans(state, device_id, false).await;
                    StatusCode::NO_CONTENT.into_response()
                }
                Err(e) => failed("cancel the roast", e),
            }
        }
        Action::Stop => {
            // As the dashboard's STOP: heater power off and the heater at 0 %, so nothing
            // heats when power comes back, then the roaster's own emergency stop.
            heater_power_off(state, device_id).await;
            let _ = dispatch(
                state,
                device_id,
                DeviceCommand::Heater { percent: 0.0 },
                &opts(),
            )
            .await;
            crate::routes::control::emergency_stop(state, device_id, &opts()).await
        }
        Action::Fan => match percent_of(&body) {
            Some(p) => crate::routes::control::set_fan(state, device_id, p, &opts()).await,
            None => return bad_percent(),
        },
        Action::Heater => match percent_of(&body) {
            Some(p) => crate::routes::control::set_heater(state, device_id, p, &opts()).await,
            None => return bad_percent(),
        },
        Action::Charge | Action::DryEnd | Action::FirstCrack | Action::Drop => {
            unreachable!("a marking button returned above")
        }
    };
    if !res.status().is_success() {
        return res;
    }
    Json(panel_state(state, device_id).await).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Harness, Reply, call, eventually};
    use serde_json::{Value, json};

    const VR: &str = crate::sim::virtual_roaster::DEVICE_ID;

    /// A panel token for `roaster`: its secret.
    async fn panel_token(h: &Harness, name: &str, roaster: &str) -> String {
        let r = h
            .owner(
                "POST",
                "/api/tokens",
                Some(json!({ "name": name, "preset": "panel", "roaster": roaster })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json());
        assert_eq!(r.json()["token"]["roaster"], roaster);
        assert_eq!(r.json()["token"]["require_approval"], false);
        r.json()["secret"].as_str().unwrap().to_string()
    }

    async fn press(h: &Harness, token: &str, roaster: &str, button: &str, body: Value) -> Reply {
        let url = format!("/api/roaster/{roaster}/panel/{button}");
        call(&h.app, "POST", &url, Some(token), Some(body)).await
    }

    fn names(v: &Value) -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A panel's token runs a whole roast on its roaster with the buttons alone: Start
    /// records, CHARGE and DROP act as on the Roast page, and each answer is the next screen.
    #[tokio::test]
    async fn a_panel_runs_a_roast_on_its_roaster() {
        let h = Harness::new().await;
        let on = h
            .owner(
                "PUT",
                "/api/virtual-roaster",
                Some(json!({ "enabled": true })),
            )
            .await;
        assert!(on.status.is_success());
        let token = panel_token(&h, "roast room", VR).await;
        let url = format!("/api/roaster/{VR}/panel");
        let state = || async { call(&h.app, "GET", &url, Some(&token), None).await.json() };
        assert!(
            eventually(|| async { state().await["online"] == true }).await,
            "the simulated roaster reports"
        );
        let idle = state().await;
        assert_eq!(idle["stage"], "idle");
        assert_eq!(idle["phase"], "idle");
        assert_eq!(idle["control"], "full");
        assert!(idle["roast"].is_null());
        assert!(idle["bt"].is_number());
        assert_eq!(names(&idle["actions"]), ["start", "stop", "fan", "heater"]);

        // Before Start a button of the roast says there is none
        let r = press(&h, &token, VR, "charge", json!({})).await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["error"]["code"], "NO_ROAST");

        let r = press(&h, &token, VR, "start", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let ready = r.json();
        assert_eq!(ready["stage"], "ready");
        assert_eq!(ready["roast"]["charged"], false);
        assert!(
            ready["roast"]["name"]
                .as_str()
                .unwrap()
                .starts_with("Roast ")
        );
        assert_eq!(
            names(&ready["actions"]),
            ["charge", "cancel", "stop", "fan", "heater"]
        );
        assert_eq!(ready["queue"], json!([]));
        let again = press(&h, &token, VR, "start", json!({})).await;
        assert_eq!(again.json()["error"]["code"], "ROAST_OPEN");

        let r = press(&h, &token, VR, "charge", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "roasting");
        assert_eq!(r.json()["phase"], "drying");
        assert_eq!(
            names(&r.json()["actions"]),
            [
                "dry_end",
                "first_crack",
                "drop",
                "cancel",
                "stop",
                "fan",
                "heater"
            ]
        );
        assert!(
            h.state.virtual_roaster.state().await.loaded,
            "CHARGE loads the beans"
        );
        let twice = press(&h, &token, VR, "charge", json!({})).await;
        assert_eq!(twice.json()["error"]["code"], "ALREADY_MARKED");

        let r = press(&h, &token, VR, "fan", json!({ "percent": 85 })).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let r = press(&h, &token, VR, "fan", json!({ "percent": 140 })).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);

        let r = press(&h, &token, VR, "first_crack", json!({})).await;
        assert_eq!(r.json()["phase"], "development");
        assert_eq!(r.json()["roast"]["first_crack"], true);
        assert_eq!(
            names(&r.json()["actions"]),
            ["drop", "cancel", "stop", "fan", "heater"]
        );
        let twice = press(&h, &token, VR, "first_crack", json!({})).await;
        assert_eq!(twice.status, StatusCode::CONFLICT);
        assert_eq!(twice.json()["error"]["code"], "ALREADY_MARKED");

        let r = press(&h, &token, VR, "drop", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "dropped");
        assert_eq!(r.json()["roast"]["dropped"], true);
        assert_eq!(
            names(&r.json()["actions"]),
            ["complete", "stop", "fan", "heater"]
        );
        assert!(
            !h.state.virtual_roaster.state().await.loaded,
            "DROP dumps the beans"
        );
        let sid = r.json()["roast"]["id"].as_str().unwrap().to_string();

        let r = press(&h, &token, VR, "complete", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "idle");
        assert!(r.json()["roast"].is_null());
        let roast = h
            .owner("GET", &format!("/api/sessions/{sid}"), None)
            .await
            .json();
        assert_eq!(roast["status"], "completed", "{roast}");
        let marks: Vec<String> = h
            .owner("GET", &format!("/api/sessions/{sid}/events"), None)
            .await
            .json()
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["event_type"].as_str().unwrap().to_string())
            .collect();
        for want in ["charge", "first_crack_start", "drop"] {
            assert!(marks.iter().any(|m| m == want), "{want} in {marks:?}");
        }

        let r = press(&h, &token, VR, "stop", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let r = press(&h, &token, VR, "espresso", json!({})).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);

        // Every press is in the audit log under the panel's name
        let logged = || async {
            h.owner("GET", "/api/audit?kind=control&limit=100", None)
                .await
                .json()
        };
        assert!(
            eventually(|| async {
                logged().await.as_array().is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["path"] == format!("/api/roaster/{VR}/panel/drop")
                            && r["token_name"] == "roast room"
                    })
                })
            })
            .await,
            "{}",
            logged().await
        );
    }

    /// Start takes a planned roast or a finished one to repeat; with neither named, the
    /// first of the queue.
    #[tokio::test]
    async fn start_takes_the_plan_or_a_roast_to_repeat() {
        let h = Harness::new().await;
        let token = panel_token(&h, "roast room", VR).await;
        let bean = h
            .owner("POST", "/api/beans", Some(json!({ "name": "Guji Uraga" })))
            .await
            .json();
        let bean_id = bean["id"].as_str().expect("a bean").to_string();
        let plan = h
            .owner(
                "POST",
                "/api/plan",
                Some(json!({ "bean_id": bean_id, "green_weight_g": 120.0, "device_id": VR })),
            )
            .await
            .json();
        let plan_id = plan["id"].as_str().expect("a planned roast").to_string();
        let other = h
            .owner(
                "POST",
                "/api/plan",
                Some(json!({ "name": "For the drum", "device_id": "drum_1" })),
            )
            .await
            .json();
        let url = format!("/api/roaster/{VR}/panel");
        let st = call(&h.app, "GET", &url, Some(&token), None).await.json();
        assert_eq!(st["queue"].as_array().unwrap().len(), 1, "{st}");
        assert_eq!(st["queue"][0]["kind"], "plan");
        assert_eq!(st["queue"][0]["id"], plan_id.as_str());
        assert_eq!(st["queue"][0]["name"], "Guji Uraga");
        assert_eq!(st["queue"][0]["green_g"], 120.0);

        // Another roaster's plan is not this panel's to start
        let r = press(&h, &token, VR, "start", json!({ "plan": other["id"] })).await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json());
        assert_eq!(r.json()["error"]["code"], "OTHER_ROASTER");
        let r = press(&h, &token, VR, "start", json!({ "plan": "nope" })).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);

        // No recipe named: the first of the queue, the plan
        let r = press(&h, &token, VR, "start", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["roast"]["name"], "Guji Uraga");
        let first = r.json()["roast"]["id"].as_str().unwrap().to_string();
        let roast = h
            .owner("GET", &format!("/api/sessions/{first}"), None)
            .await
            .json();
        assert_eq!(roast["planned_roast_id"], plan_id.as_str());
        assert_eq!(roast["bean_id"], bean_id.as_str());
        assert_eq!(roast["green_weight"], 120.0);
        press(&h, &token, VR, "charge", json!({})).await;
        press(&h, &token, VR, "drop", json!({})).await;
        let r = press(&h, &token, VR, "complete", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());

        // The plan ran; the roast is now one to repeat
        let queue = r.json()["queue"].clone();
        assert_eq!(queue.as_array().unwrap().len(), 1, "{queue}");
        assert_eq!(queue[0]["kind"], "again");
        assert_eq!(queue[0]["id"], first.as_str());
        assert_eq!(queue[0]["bean"], "Guji Uraga");
        let r = press(&h, &token, VR, "start", json!({ "again": first })).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        let second = r.json()["roast"]["id"].as_str().unwrap().to_string();
        assert_ne!(second, first);
        let roast = h
            .owner("GET", &format!("/api/sessions/{second}"), None)
            .await
            .json();
        assert_eq!(roast["bean_id"], bean_id.as_str());
        assert_eq!(roast["green_weight"], 120.0);
        assert!(roast["planned_roast_id"].is_null());
        // Cancel discards it
        let r = press(&h, &token, VR, "cancel", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "idle");
        let gone = h
            .owner("GET", &format!("/api/sessions/{second}"), None)
            .await;
        assert_eq!(gone.status, StatusCode::NOT_FOUND);
    }

    /// A panel's token reaches its roaster's panel and nothing else; the buttons are no
    /// other token's; and on real hardware it needs no "tokens may control" switch, while
    /// the roaster's control level still decides what is sent.
    #[tokio::test]
    async fn a_panel_token_is_held_to_its_roaster_and_its_panel() {
        let h = Harness::new().await;
        let made = h
            .owner(
                "POST",
                "/api/devices",
                Some(json!({ "name": "Drum", "device_id": "drum_1" })),
            )
            .await;
        assert!(made.status.is_success(), "{:?}", made.json());
        let panel = panel_token(&h, "drum panel", "drum_1").await;
        let get = |token: String, url: &'static str| {
            let app = h.app.clone();
            async move { call(&app, "GET", url, Some(&token), None).await }
        };

        // Its own panel, the server's health and who it is; no other read
        let own = get(panel.clone(), "/api/roaster/drum_1/panel").await;
        assert_eq!(own.status, StatusCode::OK, "{:?}", own.json());
        assert_eq!(own.json()["name"], "Drum");
        assert_eq!(own.json()["online"], false);
        assert_eq!(own.json()["control"], "off");
        assert_eq!(names(&own.json()["actions"]), ["start"]);
        let me = get(panel.clone(), "/api/auth/status").await;
        assert_eq!(me.status, StatusCode::OK);
        for url in [
            "/api/sessions",
            "/api/beans",
            "/api/settings",
            "/api/plan",
            "/api/roaster/drum_1/samples",
            "/api/roaster/drum_1/setup",
            "/api/tokens",
        ] {
            let r = get(panel.clone(), url).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN, "{url}");
        }
        // Another roaster's panel is not its own
        let r = get(panel.clone(), "/api/roaster/virtual_roaster/panel").await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json()["error"]["code"], "TOKEN_SCOPE");
        assert!(
            r.json()["error"]["message"]
                .as_str()
                .unwrap()
                .contains("is for drum_1 alone"),
            "{:?}",
            r.json()
        );
        let r = press(&h, &panel, VR, "start", json!({})).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json()["error"]["code"], "TOKEN_SCOPE");
        // The Roast page's own routes are not a panel's
        let fan = "/api/roaster/drum_1/control/fan";
        let r = call(
            &h.app,
            "POST",
            fan,
            Some(&panel),
            Some(json!({ "percent": 50 })),
        )
        .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json()["error"]["code"], "TOKEN_SCOPE");

        // Real hardware with the tokens' switch off: the panel is let through to the
        // control gate, which refuses while the roaster's control is off
        let access = h
            .owner("GET", "/api/roaster/drum_1/token-access", None)
            .await;
        assert_eq!(access.json()["tokens_may_control"], false);
        let r = press(&h, &panel, "drum_1", "stop", json!({})).await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json());
        assert_eq!(r.json()["error"]["code"], "CONTROL_OFF");
        // Start only records: it needs no control
        let r = press(&h, &panel, "drum_1", "start", json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "ready");

        // The buttons are no other token's, whatever its preset, on any roaster
        for preset in ["read_only", "roast", "inventory", "admin"] {
            let t = h.token(preset).await;
            let r = press(&h, &t, VR, "start", json!({})).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN, "{preset}");
            assert_eq!(r.json()["error"]["code"], "TOKEN_SCOPE", "{preset}");
            // what a panel shows is any token's to read
            let r = get(t, "/api/roaster/virtual_roaster/panel").await;
            assert_eq!(r.status, StatusCode::OK, "{preset}");
        }
        // The owner may press them
        let r = h
            .owner("POST", "/api/roaster/drum_1/panel/cancel", None)
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json());
        assert_eq!(r.json()["stage"], "idle");
    }

    /// A panel token needs a roaster the server knows, never asks first, and no other
    /// preset takes a roaster.
    #[tokio::test]
    async fn a_panel_token_needs_a_known_roaster() {
        let h = Harness::new().await;
        let make = |body: Value| {
            let app = h.app.clone();
            async move {
                call(
                    &app,
                    "POST",
                    "/api/tokens",
                    Some(crate::test_support::OWNER_KEY),
                    Some(body),
                )
                .await
            }
        };
        let r = make(json!({ "name": "p1", "preset": "panel" })).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        let r = make(json!({ "name": "p1", "preset": "panel", "roaster": " " })).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        let r = make(json!({ "name": "p1", "preset": "panel", "roaster": "nope" })).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert!(
            r.json()["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no roaster")
        );
        let r = make(json!({ "name": "a1", "preset": "roast", "roaster": VR })).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        // An id a URL writes differently could name another roaster in a path: no panel for it
        let odd = h
            .owner(
                "POST",
                "/api/devices",
                Some(json!({ "name": "Odd", "device_id": "%64rum_1" })),
            )
            .await;
        if odd.status.is_success() {
            let r = make(json!({ "name": "p0", "preset": "panel", "roaster": "%64rum_1" })).await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        }
        // Asked to ask first, a panel still does not
        let r = make(json!({ "name": "p1", "preset": "panel", "roaster": VR,
            "require_approval": true }))
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json());
        assert_eq!(r.json()["token"]["require_approval"], false);
        assert_eq!(r.json()["token"]["preset"], "panel");
        let id = r.json()["token"]["id"].as_i64().unwrap();
        let r = h
            .owner(
                "PUT",
                &format!("/api/tokens/{id}"),
                Some(json!({ "require_approval": true })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json());
        // A revoked panel stops at once
        let secret = panel_token(&h, "p2", VR).await;
        let list = h.owner("GET", "/api/tokens", None).await.json();
        let p2 = list
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "p2")
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        h.owner("POST", &format!("/api/tokens/{p2}/revoke"), None)
            .await;
        let url = format!("/api/roaster/{VR}/panel");
        let r = call(&h.app, "GET", &url, Some(&secret), None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.json()["error"]["code"], "TOKEN_REVOKED");
    }
}
