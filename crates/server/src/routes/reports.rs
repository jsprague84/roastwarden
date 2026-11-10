//! Reports made with Typst (docs/reports.md): PDFs, previews and the data behind them. Every
//! report is a read: any token may make one.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::reports::{self, Format, Made, Options, ReportError};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(report_kinds))
        .routes(routes!(roast_report))
        .routes(routes!(compare_report))
        .routes(routes!(cupping_report))
        .routes(routes!(inventory_report))
        .routes(routes!(shopping_report))
        .routes(routes!(coffee_report))
        .routes(routes!(profile_report))
        .routes(routes!(period_report))
        .routes(routes!(purchases_report))
        .routes(routes!(plan_report))
        .routes(routes!(trace_report))
}

#[derive(Serialize, ToSchema)]
pub struct ReportKindInfo {
    /// `roast`
    id: &'static str,
    title: &'static str,
    description: &'static str,
    /// The schema of its data (`format=json`): `roastwarden.report.<kind>/<version>`
    schema: &'static str,
    /// Its address: `/api/reports/roast/{id}`
    path: &'static str,
}

/// The kinds of report the server makes, and where each is.
#[utoipa::path(get, path = "/api/reports", tag = "reports",
    responses((status = 200, body = Vec<ReportKindInfo>)))]
async fn report_kinds() -> Json<Vec<ReportKindInfo>> {
    Json(
        reports::KINDS
            .iter()
            .map(|k| ReportKindInfo {
                id: k.id,
                title: k.title,
                description: k.description,
                schema: k.schema,
                path: match k.id {
                    "roast" => "/api/reports/roast/{id}",
                    "compare" => "/api/reports/compare",
                    "cupping" => "/api/reports/cupping",
                    "inventory" => "/api/reports/inventory",
                    "shopping" => "/api/reports/shopping",
                    "coffee" => "/api/reports/coffee/{id}",
                    "profile" => "/api/reports/profile/{id}",
                    "period" => "/api/reports/period",
                    "purchases" => "/api/reports/purchases",
                    "plan" => "/api/reports/plan",
                    "trace" => "/api/reports/trace",
                    _ => "",
                },
            })
            .collect(),
    )
}

/// How to make a report: its format, paper and units, and the reader's time zone.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportQuery {
    /// `pdf` (the default), `svg` (every page in one SVG: a preview), `png` (one page), or
    /// `json` (the data the design gets)
    format: Option<Format>,
    /// With `png`: the page, from 1 (default 1)
    page: Option<usize>,
    /// With `png`: dots per inch, 36 to 200 (default 96)
    dpi: Option<u32>,
    /// `letter` or `a4` (default: the `report_paper` setting, else letter)
    paper: Option<String>,
    /// `C` or `F` (default: the `report_units` setting, else C)
    units: Option<String>,
    /// The offset from UTC in minutes east, for dates and times (default: the `time_zone`
    /// setting, else UTC)
    tz: Option<i32>,
}

impl ReportQuery {
    async fn options(&self, state: &AppState) -> Result<Options, ReportError> {
        Options::from_settings(
            &state.db,
            self.paper.as_deref(),
            self.units.as_deref(),
            self.tz,
        )
        .await
    }
}

/// A roast's report (docs/reports.md, "Roast report"): the coffee, the numbers against what
/// was expected, the curve with its phases and events, notes and cupping.
#[utoipa::path(get, path = "/api/reports/roast/{id}", tag = "reports",
    params(("id" = String, Path, description = "The roast's id"), ReportQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 404, description = "No such roast", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn roast_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let input = reports::roast::gather(&state.db, &id).await?;
        let file = format!(
            "roast-report-{}",
            input
                .session
                .batch_number
                .map_or_else(|| id.chars().take(8).collect(), |n| format!("batch-{n}"))
        );
        let data = reports::roast::build(&input, &o);
        let kind = reports::kind("roast").expect("the roast kind");
        let made = reports::render(
            &state.label_renderer,
            kind,
            data,
            q.format.unwrap_or_default(),
            q.page.unwrap_or(1),
            q.dpi.unwrap_or(96),
        )
        .await?;
        Ok::<_, ReportError>((file, made))
    }
    .await;
    match made {
        Ok((file, made)) => respond(&file, made),
        Err(e) => report_error(e),
    }
}

/// Which two roasts to compare.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompareQuery {
    /// The roasts by id, comma-separated: A (the one B is measured against), then B. One id:
    /// it is B, against the finished roast before it on its roaster. None: the newest two
    /// finished roasts (on `roaster`), the older as A
    roasts: Option<String>,
    /// With no roast named: only this roaster's
    roaster: Option<String>,
    /// How close BT counts as together, °C (default 0.5)
    within: Option<f64>,
}

/// Two roasts compared (docs/reports.md, "Compare two roasts"; docs/analysis.md): BT and
/// RoR of both on one chart with heater and fan, the phases on one time scale, every number
/// side by side with B − A, and how closely B's BT followed A's.
#[utoipa::path(get, path = "/api/reports/compare", tag = "reports",
    params(ReportQuery, CompareQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter, more than two roasts, or the same roast twice", body = ErrorBody),
        (status = 404, description = "No such roast, or none before it to compare it with", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn compare_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(c): Query<CompareQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let ids: Vec<String> = c
            .roasts
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let input = reports::compare::gather(
            &state.db,
            &ids,
            c.roaster.as_deref().filter(|r| !r.is_empty()),
            c.within.unwrap_or(0.5),
        )
        .await?;
        let name = |s: &crate::roasts::types::RoastSession| {
            s.batch_number
                .map_or_else(|| s.id.chars().take(8).collect(), |n| n.to_string())
        };
        let file = format!(
            "compare-{}-{}",
            name(&input.a.session),
            name(&input.b.session)
        );
        Ok::<_, ReportError>((file, reports::compare::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "compare", made).await
}

/// What the cupping sheet is of.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CuppingQuery {
    /// The roasts, by id, comma-separated (default: the latest cupping session; none for a
    /// blank form of samples only)
    roasts: Option<String>,
    /// A blank form to score on, with blind codes and the key on its own page (default: the
    /// roasts' scores)
    blank: Option<bool>,
    /// On a blank form, this many samples of the owner's own as well (default 0; 2 when no
    /// roast is named)
    samples: Option<usize>,
    /// On a blank form: 2 samples a page with scales to mark (the default), or 6 with boxes
    per_page: Option<u8>,
    /// The scores of the cuppings on this day, `YYYY-MM-DD` (the reader's day; default the
    /// latest)
    date: Option<String>,
}

/// The cupping sheet (docs/reports.md, "Cupping sheet"; docs/cupping.md): a blank form
/// after the classic SCA protocol with blind codes, or a session's scores as a table.
#[utoipa::path(get, path = "/api/reports/cupping", tag = "reports",
    params(ReportQuery, CuppingQuery),
    responses((status = 200, description = "The sheet: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter, too many samples, or no roast cupped yet", body = ErrorBody),
        (status = 404, description = "No such roast, or no cupping that day", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn cupping_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(c): Query<CuppingQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let date = match c.date.as_deref() {
            Some(d) => Some(
                chrono::NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d")
                    .map_err(|_| ReportError::Bad(format!("date '{d}': YYYY-MM-DD")))?,
            ),
            None => None,
        };
        let ask = reports::cupping::Ask {
            roasts: c
                .roasts
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            blank: c.blank.unwrap_or(false),
            samples: c.samples.unwrap_or(0),
            per_page: c.per_page.unwrap_or(2),
            date,
        };
        let roasts = reports::cupping::gather(&state.db, &ask, &o).await?;
        let data = reports::cupping::build(&roasts, &ask, &o)?;
        let file = format!(
            "cupping-{}-{}",
            if ask.blank { "sheet" } else { "scores" },
            o.local(o.now).format("%Y-%m-%d")
        );
        let kind = reports::kind("cupping").expect("the cupping kind");
        let made = reports::render(
            &state.label_renderer,
            kind,
            data,
            q.format.unwrap_or_default(),
            q.page.unwrap_or(1),
            q.dpi.unwrap_or(96),
        )
        .await?;
        Ok::<_, ReportError>((file, made))
    }
    .await;
    match made {
        Ok((file, made)) => respond(&file, made),
        Err(e) => report_error(e),
    }
}

/// Green inventory: what is in stock, its cost and value, its age, and when each coffee runs
/// out at the recent pace; with `count` a stock-take sheet to weigh against instead.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InventoryQuery {
    /// The stock-take sheet: each coffee's grams with boxes to write the count in
    count: Option<bool>,
}

/// Green inventory (docs/reports.md, "Green inventory"): what is in stock, what it cost and
/// is worth, how old it is and when each coffee runs out; or a stock-take sheet (`count`).
#[utoipa::path(get, path = "/api/reports/inventory", tag = "reports",
    params(ReportQuery, InventoryQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn inventory_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(i): Query<InventoryQuery>,
) -> Response {
    let count = i.count.unwrap_or(false);
    let made = async {
        let o = q.options(&state).await?;
        let input = reports::inventory::gather(&state.db).await?;
        let data = reports::inventory::build(&input, count, &o);
        let file = format!(
            "{}-{}",
            if count {
                "stock-take"
            } else {
                "green-inventory"
            },
            o.local(o.now).format("%Y-%m-%d")
        );
        Ok::<_, ReportError>((file, data))
    }
    .await;
    finish(&state, &q, "inventory", made).await
}

/// The shopping list (docs/reports.md, "Shopping list"): one short order per supplier with
/// prices as seen and the usual shipping, then the coffees running low.
#[utoipa::path(get, path = "/api/reports/shopping", tag = "reports",
    params(ReportQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn shopping_report(State(state): State<AppState>, Query(q): Query<ReportQuery>) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let input = reports::shopping::gather(&state.db).await?;
        let data = reports::shopping::build(&input, &o);
        let file = format!("shopping-list-{}", o.local(o.now).format("%Y-%m-%d"));
        Ok::<_, ReportError>((file, data))
    }
    .await;
    finish(&state, &q, "shopping", made).await
}

/// A green coffee's report (docs/reports.md, "Coffee report"): what is known of it, stock
/// and cost, its roasts as a table and small multiples, its cuppings, its ledger.
#[utoipa::path(get, path = "/api/reports/coffee/{id}", tag = "reports",
    params(("id" = String, Path, description = "The coffee's (bean's) id"), ReportQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 404, description = "No such coffee", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn coffee_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let input = reports::coffee::gather(&state.db, &id).await?;
        let file = format!("coffee-{}", crate::labels::slug(&input.detail.bean.name));
        Ok::<_, ReportError>((file, reports::coffee::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "coffee", made).await
}

/// Which roasts of a profile.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProfileReportQuery {
    /// Only this roaster's roasts (default: every roaster's)
    roaster: Option<String>,
}

/// A profile's report (docs/reports.md, "Profile report"): its targets, its roasts ranked
/// with an average row, their BT overlaid on the profile's, how the numbers spread.
#[utoipa::path(get, path = "/api/reports/profile/{id}", tag = "reports",
    params(("id" = String, Path, description = "The profile's id"), ReportQuery, ProfileReportQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 404, description = "No such profile", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn profile_report(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ReportQuery>,
    Query(p): Query<ProfileReportQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let input = reports::profile::gather(
            &state.db,
            &id,
            p.roaster.as_deref().filter(|r| !r.is_empty()),
        )
        .await?;
        let file = format!(
            "profile-{}",
            crate::labels::slug(&input.profile.profile.name)
        );
        Ok::<_, ReportError>((file, reports::profile::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "profile", made).await
}

/// The days a period report or purchases cover.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PeriodQuery {
    /// A whole year, `2026`: the year in review
    year: Option<String>,
    /// A whole month, `2026-10`
    month: Option<String>,
    /// The first day, `YYYY-MM-DD` (default: the first of this month)
    from: Option<String>,
    /// The last day, `YYYY-MM-DD` (default: today)
    to: Option<String>,
}

impl PeriodQuery {
    fn range(&self, o: &Options) -> Result<reports::period::Range, ReportError> {
        reports::period::Range::parse(
            self.year.as_deref(),
            self.month.as_deref(),
            self.from.as_deref(),
            self.to.as_deref(),
            o.local(o.now).date_naive(),
        )
    }
}

/// A period's report (docs/reports.md, "Period report"): a month, a range or a year in
/// review: roasts with a sum row, green roasted and bought, spend, each coffee, stock at the end.
#[utoipa::path(get, path = "/api/reports/period", tag = "reports",
    params(ReportQuery, PeriodQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter or range", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn period_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(p): Query<PeriodQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let range = p.range(&o)?;
        let input = reports::period::gather(&state.db, range, &o).await?;
        let file = format!("period-{}-{}", range.from, range.to);
        Ok::<_, ReportError>((file, reports::period::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "period", made).await
}

/// Purchases (docs/reports.md, "Purchases"): the green bought over a range, order by order
/// with shipping and fees, each supplier's share, landed cost per kg over time.
#[utoipa::path(get, path = "/api/reports/purchases", tag = "reports",
    params(ReportQuery, PeriodQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter or range", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn purchases_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(p): Query<PeriodQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let range = p.range(&o)?;
        let input = reports::purchases::gather(&state.db, range).await?;
        let file = format!("purchases-{}-{}", range.from, range.to);
        Ok::<_, ReportError>((file, reports::purchases::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "purchases", made).await
}

/// Which roaster's plan.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlanQuery {
    /// Only what is planned on this roaster (and on no roaster in particular)
    roaster: Option<String>,
}

/// The roast-day plan sheet (docs/reports.md, "Roast-day plan"): the planned roasts in
/// order with the green to weigh, the profile's targets and boxes to write in.
#[utoipa::path(get, path = "/api/reports/plan", tag = "reports",
    params(ReportQuery, PlanQuery),
    responses((status = 200, description = "The sheet: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "A bad parameter", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn plan_report(
    State(state): State<AppState>,
    Query(q): Query<ReportQuery>,
    Query(p): Query<PlanQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let input =
            reports::plan::gather(&state.db, p.roaster.as_deref().filter(|r| !r.is_empty()))
                .await?;
        let file = format!("roast-day-{}", o.local(o.now).format("%Y-%m-%d"));
        Ok::<_, ReportError>((file, reports::plan::build(&input, &o)))
    }
    .await;
    finish(&state, &q, "plan", made).await
}

/// What a trace is of: one roast or one bean.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TraceQuery {
    /// A roast's id: back to its green lot, and where its coffee went
    roast: Option<String>,
    /// A bean's id: its every roast and where each went
    bean: Option<String>,
}

/// Traceability (docs/reports.md, "Traceability"): the green coffee (supplier, lot, origin,
/// process, purchases), each roast (batch, date, roaster, profile, green → roasted, loss)
/// and where it went (packed bags, label runs, gifts, use, discards, what is left). No prices.
#[utoipa::path(get, path = "/api/reports/trace", tag = "reports",
    params(TraceQuery, ReportQuery),
    responses((status = 200, description = "The report: PDF, SVG, PNG or its data as JSON (`format`); `X-Report-Pages` says how many pages", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Neither or both of roast and bean", body = ErrorBody),
        (status = 404, description = "No such roast or bean", body = ErrorBody),
        (status = 422, description = "The report design does not compile: what Typst said", body = ErrorBody)))]
async fn trace_report(
    State(state): State<AppState>,
    Query(t): Query<TraceQuery>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let made = async {
        let o = q.options(&state).await?;
        let trace =
            reports::trace::gather(&state.db, t.roast.as_deref(), t.bean.as_deref(), &o).await?;
        let file = match (trace.subject.as_str(), trace.roasts.first(), &trace.coffee) {
            ("roast", Some(r), _) => format!(
                "trace-{}",
                r.batch_number
                    .map_or_else(|| r.id.chars().take(8).collect(), |n| format!("batch-{n}"))
            ),
            (_, _, Some(c)) => format!("trace-{}", crate::labels::slug(&c.name)),
            _ => "trace".into(),
        };
        Ok::<_, ReportError>((file, reports::trace::build(&trace, &o)))
    }
    .await;
    finish(&state, &q, "trace", made).await
}

/// Render a kind's data in its format and answer it, or the error
async fn finish(
    state: &AppState,
    q: &ReportQuery,
    kind: &str,
    made: Result<(String, serde_json::Value), ReportError>,
) -> Response {
    let (file, data) = match made {
        Ok(m) => m,
        Err(e) => return report_error(e),
    };
    let kind = reports::kind(kind).expect("a kind of ours");
    match reports::render(
        &state.label_renderer,
        kind,
        data,
        q.format.unwrap_or_default(),
        q.page.unwrap_or(1),
        q.dpi.unwrap_or(96),
    )
    .await
    {
        Ok(made) => respond(&file, made),
        Err(e) => report_error(e),
    }
}

fn respond(file: &str, made: Made) -> Response {
    let ext = match made.content_type {
        "application/pdf" => "pdf",
        "image/svg+xml" => "svg",
        "image/png" => "png",
        _ => "json",
    };
    let mut res = (
        [
            (header::CONTENT_TYPE, made.content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{file}.{ext}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        made.bytes,
    )
        .into_response();
    if let Some(n) = made.pages {
        res.headers_mut()
            .insert("x-report-pages", header::HeaderValue::from(n));
    }
    res
}

pub(crate) fn report_error(e: ReportError) -> Response {
    match e {
        ReportError::NotFound(m) => (StatusCode::NOT_FOUND, m).into_response(),
        ReportError::Bad(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        ReportError::Design(m) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the report design does not compile:\n{m}"),
        )
            .into_response(),
        ReportError::Internal(e) => {
            tracing::error!(error = ?e, "report failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "the report failed").into_response()
        }
    }
}
