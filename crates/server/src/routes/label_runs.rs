//! Print runs (docs/labels.md, "Print runs"): a sheet of labels for
//! one roast or several (a gift run), each roast with its own design, settings, brand, copies
//! and QR code. A run is made (its PDF kept), then printed, or printed by hand and marked so;
//! a reprint lays the same labels out again from what they carried. The printer's job states
//! are kept with their times, and a sheet's next free label moves on when a job completes.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::labels::{LabelAsk, Purpose, label_error, prepare, sheet_for};
use crate::app::state::AppState;
use crate::auth::Actor;
use crate::http::errors::ErrorBody;
use crate::labels::{self, Output, runs, store};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_label_runs, make_label_run))
        .routes(routes!(preview_label_run))
        .routes(routes!(get_label_run))
        .routes(routes!(label_run_pdf))
        .routes(routes!(print_label_run))
        .routes(routes!(label_run_done))
        .routes(routes!(reprint_label_run))
        .routes(routes!(list_label_sheets))
        .routes(routes!(set_label_sheet))
}

/// The most items a run holds, and labels
const MAX_ITEMS: usize = 30;
const MAX_LABELS: usize = 200;

fn status(code: StatusCode, message: impl Into<String>) -> Response {
    (code, message.into()).into_response()
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "label runs");
    status(StatusCode::INTERNAL_SERVER_ERROR, "the label runs failed")
}

// ---- making a run ----

/// One roast's labels in a run
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct RunItemAsk {
    /// The roast's id
    pub roast: String,
    /// How many labels (default 1)
    pub count: Option<usize>,
    /// A design (`id`, `id@N`); default the coffee's own, else plain
    pub design: Option<String>,
    /// A design's Typst source instead: a one-off
    pub source: Option<String>,
    /// The design's parameters
    #[serde(default)]
    pub params: Map<String, Value>,
    /// A brand kit's id (default the coffee's, then the `label_brand` setting)
    pub brand: Option<String>,
    /// A gift line ("for Sam")
    pub message: Option<String>,
    /// Tasting notes instead of the roast's or the bean's
    pub notes: Option<String>,
    /// Net weight on the label, g (default the roast's roasted weight)
    pub weight_g: Option<f64>,
    /// `whole` (default) or `ground`: the label's form line and best-until
    pub form: Option<String>,
    /// Ground: the grind ("espresso", "filter", "moka", "French press", …); implies ground
    pub grind: Option<String>,
    /// Take the bags off the roast when the run is made: a packed move of `count` bags of
    /// the label's weight, whole or ground (docs/roasted.md, "Packed bags"). Refused when
    /// that much is not left; a reused run does not take them twice
    #[serde(default)]
    pub pack: bool,
    /// Fields to leave off
    #[serde(default)]
    pub hide: Vec<String>,
    /// For coffee that is sold: its bag type (the label's data gets the heights it needs)
    pub retail: Option<String>,
    /// What the QR code opens: `app`, `share`, `supplier`, `none` or an address (default:
    /// the `label_qr` setting, else `app`)
    pub qr: Option<String>,
}

/// A sheet of labels for one roast or several
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct RunAsk {
    /// The sheet's id (default 5163)
    pub sheet: Option<String>,
    /// The first label to print on, from 1 (default the sheet's next free label)
    pub start: Option<usize>,
    /// Each roast's labels, in the order they go on the sheet (1 to 30)
    pub items: Vec<RunItemAsk>,
    /// The offset from UTC in minutes east, for the dates (default: the `time_zone`
    /// setting, else UTC)
    pub tz: Option<i32>,
    /// Kept with the run ("gift baskets")
    pub note: Option<String>,
    /// The printer it is laid out for (its calibration for the sheet), and printed on unless
    /// another is named then; default the default printer
    pub printer: Option<String>,
    /// Always a new run. By default a make with the same labels as a run made in the last
    /// 24 hours and not yet sent or printed answers that run (`reused: true`), so a second
    /// Download does not keep a second copy
    #[serde(default)]
    pub new: bool,
}

/// Which printer, tray and colour a print uses (each default the printer's own)
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct PrintAsk {
    /// A printer's id (default the run's, else the default printer)
    pub printer: Option<String>,
    /// A tray (`media-source`, e.g. `tray-1`)
    pub tray: Option<String>,
    /// `monochrome` or `color`
    pub color: Option<String>,
}

/// A run laid out: the job, what each item carried, where it starts
pub(crate) struct Built {
    job: labels::Job,
    items: Vec<runs::RunItem>,
    start: usize,
    labels: usize,
    /// The printer whose calibration it has
    printer: Option<String>,
}

/// "Guji 1 (#7)": the label's name with its batch
fn item_name(data: &Value) -> String {
    let name = data["name"].as_str().unwrap_or("labels").to_string();
    match data["batch"].as_i64() {
        Some(b) => format!("{name} (#{b})"),
        None => name,
    }
}

#[allow(clippy::result_large_err)] // the error is the answer, made once
fn check_size(items: usize, labels: usize) -> Result<(), Response> {
    if !(1..=MAX_ITEMS).contains(&items) {
        return Err(status(
            StatusCode::BAD_REQUEST,
            format!("a run has 1 to {MAX_ITEMS} items"),
        ));
    }
    if !(1..=MAX_LABELS).contains(&labels) {
        return Err(status(
            StatusCode::BAD_REQUEST,
            format!("a run has 1 to {MAX_LABELS} labels"),
        ));
    }
    Ok(())
}

/// The first label: asked, else the sheet's next free one
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn start_for(
    state: &AppState,
    sheet: &labels::Sheet,
    asked: Option<usize>,
) -> Result<usize, Response> {
    let start = match asked {
        Some(s) => s,
        None => runs::next_label(&state.db, &sheet.id)
            .await
            .map_err(failed)?
            .map_or(1, |n| n as usize),
    };
    if !(1..=sheet.per_sheet()).contains(&start) {
        return Err(status(
            StatusCode::BAD_REQUEST,
            format!(
                "start is a label from 1 to {} on this sheet",
                sheet.per_sheet()
            ),
        ));
    }
    Ok(start)
}

/// Each item laid out as a roast's labels would be, then all on one sheet: the first item's
/// design is the sheet's, each further item a group with its own design, settings, brand
/// and QR code.
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub(crate) async fn build(
    state: &AppState,
    headers: &HeaderMap,
    who: Option<&Actor>,
    ask: &RunAsk,
    purpose: Purpose,
) -> Result<Built, Response> {
    let labels: usize = ask.items.iter().map(|i| i.count.unwrap_or(1)).sum();
    check_size(ask.items.len(), labels)?;
    let (sheet, _) = sheet_for(state, ask.sheet.as_deref())
        .await
        .map_err(|m| status(StatusCode::BAD_REQUEST, m))?;
    let start = start_for(state, &sheet, ask.start).await?;
    // The printer it is laid out for: named, else the default one
    let printer = match &ask.printer {
        Some(p) => Some(
            super::label_printers::resolve(state, Some(p))
                .await?
                .id
                .unwrap_or_else(|| p.clone()),
        ),
        None => labels::printers::default(&state.db)
            .await
            .map_err(failed)?
            .map(|p| p.id),
    };
    let mut base: Option<labels::Job> = None;
    let mut items = Vec::with_capacity(ask.items.len());
    let mut slots: Vec<Option<Value>> = vec![None; start - 1];
    for (i, it) in ask.items.iter().enumerate() {
        let count = it.count.unwrap_or(1);
        if count == 0 {
            return Err(status(
                StatusCode::BAD_REQUEST,
                "an item has 1 label or more",
            ));
        }
        let job = prepare(
            state,
            headers,
            who,
            LabelAsk {
                design: it.design.clone(),
                source: it.source.clone(),
                sheet: Some(sheet.id.clone()),
                roast: Some(it.roast.clone()),
                params: it.params.clone(),
                brand: it.brand.clone(),
                count: Some(1),
                start: Some(1),
                weight_g: it.weight_g,
                form: it.form.clone(),
                grind: it.grind.clone(),
                message: it.message.clone(),
                notes: it.notes.clone(),
                hide: it.hide.clone(),
                tz: ask.tz,
                printer: printer.clone(),
                retail: it.retail.clone(),
                qr: it.qr.clone(),
                ..LabelAsk::default()
            },
            false,
            purpose,
        )
        .await?;
        let Some(design) = job.design.clone() else {
            return Err(status(StatusCode::BAD_REQUEST, "an item has no design"));
        };
        let data = job.slots.first().cloned().flatten().unwrap_or(Value::Null);
        items.push(runs::RunItem {
            roast: Some(it.roast.clone()),
            name: item_name(&data),
            design: design.label(),
            count,
            params: job.params.clone(),
            brand: job.brand["id"].as_str().map(str::to_string),
            data: data.clone(),
            brand_value: job.brand.clone(),
            source: design.source.clone(),
            url: job.url.clone(),
        });
        let mut label = data;
        if i > 0 {
            label["group"] = json!(i);
        }
        slots.extend(std::iter::repeat_n(Some(label), count));
        match base.as_mut() {
            None => base = Some(job),
            Some(b) => b.more.push(labels::Group {
                design,
                params: job.params,
                brand: job.brand,
                url: job.url,
            }),
        }
    }
    let mut job = base.ok_or_else(|| status(StatusCode::BAD_REQUEST, "a run has items"))?;
    job.slots = slots;
    Ok(Built {
        job,
        items,
        start,
        labels,
        printer,
    })
}

/// Each item again, from what it carried (a reprint): its data, design source, settings and
/// brand as they were; the sheet's offset as it is now. `pick` keeps only those labels (from
/// 1, in the run's order).
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn rebuild(
    state: &AppState,
    run: &runs::Run,
    start: usize,
    pick: Option<&[usize]>,
    printer: Option<&str>,
) -> Result<Built, Response> {
    let (sheet, offset) = sheet_for(state, Some(&run.run.sheet))
        .await
        .map_err(|m| status(StatusCode::BAD_REQUEST, m))?;
    let offset = labels::printers::offset_for(&state.db, printer, &sheet.id, offset).await;
    let mut fonts = {
        let folder = state.labels.clone();
        tokio::task::spawn_blocking(move || folder.own_fonts())
            .await
            .unwrap_or_default()
    };
    fonts.extend(store::fonts(&state.db).await.unwrap_or_default());
    let mut job = labels::Job::new(sheet, offset, None);
    job.fonts = fonts;
    let mut chosen: Vec<runs::RunItem> = Vec::new();
    let mut slots: Vec<Option<Value>> = vec![None; start - 1];
    let mut n = 0;
    for it in &run.items {
        // The labels of this item that are kept
        let keep = (0..it.count)
            .filter(|_| {
                n += 1;
                pick.is_none_or(|p| p.contains(&n))
            })
            .count();
        if keep == 0 {
            continue;
        }
        let params_text = Value::Object(it.params.clone()).to_string();
        let logo = it.brand_value["logo"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let files = store::images_named(&state.db, &[&it.source, &params_text, &logo])
            .await
            .map_err(failed)?;
        let (id, version) = store::parse_ref(&it.design).unwrap_or((it.design.clone(), None));
        let design = labels::DesignSource {
            id,
            version,
            source: it.source.clone(),
            files,
        };
        let group = chosen.len();
        let mut label = it.data.clone();
        if group > 0 {
            label["group"] = json!(group);
        }
        slots.extend(std::iter::repeat_n(Some(label), keep));
        if group == 0 {
            job.design = Some(design);
            job.params = it.params.clone();
            job.brand = it.brand_value.clone();
            job.url = it.url.clone();
        } else {
            job.more.push(labels::Group {
                design,
                params: it.params.clone(),
                brand: it.brand_value.clone(),
                url: it.url.clone(),
            });
        }
        chosen.push(runs::RunItem {
            count: keep,
            ..it.clone()
        });
    }
    let labels: usize = chosen.iter().map(|i| i.count).sum();
    check_size(chosen.len(), labels)?;
    job.slots = slots;
    Ok(Built {
        job,
        items: chosen,
        start,
        labels,
        printer: printer.map(str::to_string),
    })
}

/// Render a built run to PDF and keep it
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub(crate) async fn keep_run(
    state: &AppState,
    built: Built,
    by: &str,
    reprint_of: Option<&str>,
    note: Option<&str>,
) -> Result<runs::Run, Response> {
    let sheet = built.job.sheet.clone();
    let pages = (built.start - 1 + built.labels).div_ceil(sheet.per_sheet());
    let pdf = labels::render(&state.label_renderer, built.job, Output::Pdf)
        .await
        .map_err(label_error)?;
    let id = uuid::Uuid::new_v4().to_string();
    let printer_id = built.printer.clone();
    runs::insert(
        &state.db,
        runs::NewRun {
            id: &id,
            by,
            sheet: &sheet.id,
            sheet_name: &sheet.name,
            start: built.start,
            labels: built.labels,
            pages,
            items: &built.items,
            pdf: &pdf,
            reprint_of,
            note: note.map(str::trim).filter(|n| !n.is_empty()),
            printer_id: printer_id.as_deref(),
        },
    )
    .await
    .map_err(failed)?;
    runs::get(&state.db, &id)
        .await
        .map_err(failed)?
        .ok_or_else(|| status(StatusCode::INTERNAL_SERVER_ERROR, "the run vanished"))
}

fn by(actor: Option<&Actor>) -> String {
    super::label_studio::by(actor)
}

/// Bags a run takes off a roast (`pack`)
struct Bags {
    roast: String,
    bags: usize,
    bag_g: f64,
    form: Option<String>,
    grind: Option<String>,
}

/// The bags the items with `pack` take off their roasts, each of the label's weight (the
/// one asked, else the roast's roasted weight), checked against what is left. Roasts in
/// `done` were packed by this run already.
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn bags_for(
    state: &AppState,
    who: Option<&Actor>,
    ask: &RunAsk,
    done: &[String],
) -> Result<Vec<Bags>, Response> {
    let asked: Vec<&RunItemAsk> = ask
        .items
        .iter()
        .filter(|i| i.pack && !done.contains(&i.roast))
        .collect();
    if asked.is_empty() {
        return Ok(vec![]);
    }
    let may = match who {
        Some(Actor::Token(t)) => t.preset.allows("POST", "/api/roasted/x/moves"),
        Some(Actor::Anonymous(_)) => false,
        Some(Actor::Demo) => crate::auth::tokens::demo_allows("POST", "/api/roasted/x/moves"),
        // The owner (key or login), or authentication off
        None | Some(_) => true,
    };
    if !may {
        return Err(status(
            StatusCode::FORBIDDEN,
            "this token may not take bags off a roast (the inventory preset may): make the run without pack",
        ));
    }
    let svc = crate::coffee::roasted::RoastedService::new(state.db.clone());
    let zone = crate::ops::zone::resolve(&state.db, ask.tz).await;
    let now = chrono::Utc::now();
    let mut out: Vec<Bags> = Vec::new();
    let mut want = std::collections::BTreeMap::<String, f64>::new();
    for it in asked {
        let lot = match svc.lot(&it.roast, zone, now).await {
            Ok(Some(l)) => l,
            Ok(None) => {
                return Err(status(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "roast {:?} is not a finished roast: its bags cannot be taken off",
                        it.roast
                    ),
                ));
            }
            Err(e) => return Err(failed(e)),
        };
        // A roast in one bag with others: the bag's coffee, what they have left together
        let left_g = lot.combined.as_ref().map(|c| c.left_g).or(lot.left_g);
        let Some(left) = left_g else {
            return Err(status(
                StatusCode::BAD_REQUEST,
                format!(
                    "{} has no roasted weight yet: record it first (roast weigh)",
                    lot.name
                ),
            ));
        };
        let roasted_g = lot.combined.as_ref().map(|c| c.roasted_g).or(lot.roasted_g);
        let Some(bag_g) = it
            .weight_g
            .or(roasted_g.map(f64::from))
            .filter(|g| *g > 0.0)
        else {
            return Err(status(
                StatusCode::BAD_REQUEST,
                "pack needs the bag's weight (weight_g)",
            ));
        };
        let bags = it.count.unwrap_or(1);
        // Two items on roasts of one bag draw on the same coffee
        let key = lot
            .combined
            .as_ref()
            .map_or_else(|| it.roast.clone(), |c| c.id.clone());
        let total = want.entry(key).or_default();
        *total += bags as f64 * bag_g;
        if *total > f64::from(left) + 0.5 {
            return Err(status(
                StatusCode::BAD_REQUEST,
                format!(
                    "only {left:.1} g of {} is left: {bags} × {} g is {:.1} g. Make the run without taking the bags off, or fewer labels",
                    lot.name,
                    crate::reports::grams(bag_g),
                    *total
                ),
            ));
        }
        out.push(Bags {
            roast: it.roast.clone(),
            bags,
            bag_g,
            form: it.form.clone(),
            grind: it.grind.clone(),
        });
    }
    Ok(out)
}

/// Record the bags as packed moves that name the run
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn take_bags(
    state: &AppState,
    run: &str,
    bags: Vec<Bags>,
    tz: Option<i32>,
) -> Result<(), Response> {
    let svc = crate::coffee::roasted::RoastedService::new(state.db.clone());
    let zone = crate::ops::zone::resolve(&state.db, tz).await;
    for b in bags {
        let req = crate::coffee::roasted::RoastedMoveRequest {
            kind: "packed".into(),
            bags: Some(b.bags as f32),
            bag_g: Some(b.bag_g as f32),
            form: b.form,
            grind: b.grind,
            ..Default::default()
        };
        if let Err(e) = svc
            .add_move_for(&b.roast, req, Some(run), zone, chrono::Utc::now())
            .await
        {
            if let Some(bad) = e.downcast_ref::<crate::coffee::inventory::InventoryInputError>() {
                return Err(status(StatusCode::BAD_REQUEST, bad.to_string()));
            }
            return Err(failed(e));
        }
    }
    Ok(())
}

/// The roasts a run has taken bags off already
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn packed_by(state: &AppState, run: &str) -> Result<Vec<String>, Response> {
    crate::coffee::roasted::packed_by_run(&state.db, run)
        .await
        .map_err(|e| failed(e.into()))
}

/// Make a run: one roast's labels or several roasts' on one sheet (a gift run), each with its
/// design, settings, brand, copies and QR code, from the sheet's next free label. The PDF is
/// kept; print it with `POST /api/label-runs/{id}/print`, or download it
/// (`GET …/pdf`) and mark it printed (`POST …/done`). The same labels as a run made in the
/// last 24 hours and not yet sent or printed answer that run (200, `reused: true`) unless
/// `new` is set.
#[utoipa::path(post, path = "/api/label-runs", tag = "labels",
    request_body = RunAsk,
    responses((status = 201, body = runs::Run),
        (status = 200, description = "The same labels as a run not yet printed: that run, `reused: true`", body = runs::Run),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 422, description = "A design does not compile", body = ErrorBody)))]
async fn make_label_run(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    headers: HeaderMap,
    Json(ask): Json<RunAsk>,
) -> Response {
    let built = match build(&state, &headers, actor.as_deref(), &ask, Purpose::Make).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    if !ask.new {
        let note = ask.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
        let same = runs::unprinted_same(
            &state.db,
            &built.job.sheet.id,
            built.start,
            built.printer.as_deref(),
            note,
            &built.items,
        )
        .await;
        match same {
            Ok(Some(id)) => {
                // Bags asked for now and not yet taken off by that run
                let done = match packed_by(&state, &id).await {
                    Ok(d) => d,
                    Err(r) => return r,
                };
                let bags = match bags_for(&state, actor.as_deref(), &ask, &done).await {
                    Ok(b) => b,
                    Err(r) => return r,
                };
                if let Err(r) = take_bags(&state, &id, bags, ask.tz).await {
                    return r;
                }
                return match runs::get(&state.db, &id).await {
                    Ok(Some(mut run)) => {
                        run.reused = true;
                        Json(run).into_response()
                    }
                    Ok(None) => status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
                    Err(e) => failed(e),
                };
            }
            Ok(None) => {}
            Err(e) => return failed(e),
        }
    }
    let bags = match bags_for(&state, actor.as_deref(), &ask, &[]).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let run = match keep_run(
        &state,
        built,
        &by(actor.as_deref()),
        None,
        ask.note.as_deref(),
    )
    .await
    {
        Ok(run) => run,
        Err(r) => return r,
    };
    if bags.is_empty() {
        return (StatusCode::CREATED, Json(run)).into_response();
    }
    if let Err(r) = take_bags(&state, &run.run.id, bags, ask.tz).await {
        return r;
    }
    match runs::get(&state.db, &run.run.id).await {
        Ok(Some(run)) => (StatusCode::CREATED, Json(run)).into_response(),
        Ok(None) => status(StatusCode::INTERNAL_SERVER_ERROR, "the run vanished"),
        Err(e) => failed(e),
    }
}

/// A run's first sheet as SVG with every label outlined, before making it: nothing is kept.
#[utoipa::path(post, path = "/api/label-runs/preview", tag = "labels",
    request_body = RunAsk,
    responses((status = 200, content_type = "image/svg+xml", body = String),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody)))]
async fn preview_label_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Json(ask): Json<RunAsk>,
) -> Response {
    let mut built = match build(&state, &headers, actor.as_deref(), &ask, Purpose::Preview).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    built.job.outline = true;
    match labels::render(&state.label_renderer, built.job, Output::Svg).await {
        Ok(svg) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            svg,
        )
            .into_response(),
        Err(e) => label_error(e),
    }
}

#[derive(Deserialize, IntoParams)]
pub struct RunsQuery {
    /// How many (default 30, at most 500)
    limit: Option<i64>,
}

/// The runs, newest first: what, on which sheet from which label, by whom, and how the
/// printer's job went.
#[utoipa::path(get, path = "/api/label-runs", tag = "labels",
    params(RunsQuery),
    responses((status = 200, body = Vec<runs::RunSummary>)))]
async fn list_label_runs(State(state): State<AppState>, Query(q): Query<RunsQuery>) -> Response {
    match runs::list(&state.db, q.limit.unwrap_or(30).clamp(1, 500)).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => failed(e),
    }
}

/// One run: each item (its roast, `design@version`, settings, brand and the data each label
/// carried) and the printer's job states with their times.
#[utoipa::path(get, path = "/api/label-runs/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, body = runs::Run), (status = 404, body = ErrorBody)))]
async fn get_label_run(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match runs::get(&state.db, &id).await {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
        Err(e) => failed(e),
    }
}

/// A run's PDF, as it was made (print it at "Actual size").
#[utoipa::path(get, path = "/api/label-runs/{id}/pdf", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, content_type = "application/pdf", body = Vec<u8>),
        (status = 404, body = ErrorBody)))]
async fn label_run_pdf(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match runs::pdf(&state.db, &id).await {
        Ok(Some(pdf)) => (
            [
                (header::CONTENT_TYPE, "application/pdf".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!(
                        "inline; filename=\"labels-{}.pdf\"",
                        id.get(..8).unwrap_or(&id)
                    ),
                ),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            pdf,
        )
            .into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
        Err(e) => failed(e),
    }
}

// ---- printing a run ----

/// What a print answers.
#[derive(Serialize, ToSchema)]
pub struct Printed {
    /// In words: printed, printing, or what the printer waits for ("feed a sheet into
    /// tray-1")
    pub message: String,
    pub printer: String,
    pub job_id: Option<i32>,
    /// What the printer last said of the job: completed, processing (still going after
    /// 45 s; Roastwarden keeps following it), stopped (it waits: paper, a door), …
    pub job_state: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub job_state_reasons: Vec<String>,
    /// Sheets printed so far, as the printer counts them
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheets: Option<i32>,
    /// The tray it asked for: the one holding paper of the sheet's size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tray: Option<String>,
    /// The designs printed (`sam-gift@2`, …), the sheet, the labels
    pub design: String,
    pub sheet: String,
    pub count: usize,
    pub start: usize,
    pub pages: usize,
    /// The run (`GET /api/label-runs/{id}`)
    pub run: String,
    /// How it went to the printer: `application/pdf`, `image/pwg-raster` or `image/urf`
    pub sent_as: String,
}

/// The next free label once a run's labels are used
async fn advance_sheet(state: &AppState, run: &runs::RunSummary) {
    let per = match sheet_for(state, Some(&run.sheet)).await {
        Ok((s, _)) => s.per_sheet(),
        Err(_) => return,
    };
    let next = runs::after(run.start as usize, run.labels as usize, per) as i64;
    if let Err(e) = runs::set_next(&state.db, &run.sheet, next, Some(&run.id)).await {
        tracing::warn!(error = %e, "keeping the next free label");
    }
}

/// Keep what the printer says of a run's job; when it completes, the sheet's next free
/// label moves on
async fn record(state: &AppState, run: &runs::RunSummary, now: &labels::print::JobNow) {
    let Some(st) = now.state.as_deref() else {
        return;
    };
    match runs::state(&state.db, &run.id, st, &now.reasons, now.sheets).await {
        Ok(true) if st == "completed" => advance_sheet(state, run).await,
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, run = %run.id, "keeping a job's state"),
    }
}

/// Keep following a job the printer has not finished (waiting for paper, still printing):
/// every 5 s for up to 15 minutes
fn follow_on(state: AppState, run: runs::RunSummary, target: labels::print::Target, job: i32) {
    tokio::spawn(async move {
        for _ in 0..180 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let Ok(now) = labels::print::job_now(&target, job).await else {
                continue;
            };
            record(&state, &run, &now).await;
            if now.finished() {
                return;
            }
        }
    });
}

/// Send a run to a printer (the one asked, else the run's, else the default): its PDF as
/// made when that printer takes PDF and the run was laid out for it; else the run laid out
/// again for this printer's calibration, as PDF or as raster (PWG, URF) for a printer
/// without PDF. Answer what the printer did, and keep following the job.
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub(crate) async fn send_run(
    state: &AppState,
    id: &str,
    how: &PrintAsk,
) -> Result<Printed, Response> {
    let run = runs::get(&state.db, id)
        .await
        .map_err(failed)?
        .ok_or_else(|| status(StatusCode::NOT_FOUND, format!("no label run {id:?}")))?;
    let asked = how.printer.as_deref().or(run.run.printer_id.as_deref());
    let printer = super::label_printers::resolve(state, asked).await?;
    let (sheet, _) = sheet_for(state, Some(&run.run.sheet))
        .await
        .map_err(|m| status(StatusCode::BAD_REQUEST, m))?;
    let info = super::label_printers::ask(state, &printer)
        .await
        .map_err(|e| status(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    if let Some(p) = info.problem() {
        return Err(status(StatusCode::CONFLICT, p));
    }
    let doc =
        labels::print::choose(&info, &printer.send).map_err(|m| status(StatusCode::CONFLICT, m))?;
    let colour = how.color.clone().or(printer.color.clone());
    let same_layout = printer.id.is_some() && printer.id == run.run.printer_id;
    let (bytes, mime) = match doc {
        labels::print::Doc::Pdf if same_layout || printer.id.is_none() => (
            runs::pdf(&state.db, id)
                .await
                .map_err(failed)?
                .unwrap_or_default(),
            "application/pdf",
        ),
        labels::print::Doc::Pdf => {
            let built = rebuild(
                state,
                &run,
                run.run.start as usize,
                None,
                printer.id.as_deref(),
            )
            .await?;
            (
                labels::render(&state.label_renderer, built.job, Output::Pdf)
                    .await
                    .map_err(label_error)?,
                "application/pdf",
            )
        }
        labels::print::Doc::Raster { format, dpi, scale } => {
            let built = rebuild(
                state,
                &run,
                run.run.start as usize,
                None,
                printer.id.as_deref(),
            )
            .await?;
            let colour = match colour.as_deref() {
                Some("monochrome") => false,
                _ => labels::print::prints_colour(&info, doc),
            };
            (
                labels::render_raster(
                    &state.label_renderer,
                    built.job,
                    format,
                    (dpi, scale),
                    colour,
                    if info.labels_media { "labels" } else { "" },
                )
                .await
                .map_err(label_error)?,
                format.mime(),
            )
        }
    };
    let designs = {
        let mut d: Vec<String> = run.items.iter().map(|i| i.design.clone()).collect();
        d.dedup();
        d.join(", ")
    };
    let name = format!("Roastwarden labels: {}", run.run.summary);
    let page = (sheet.page_w, sheet.page_h);
    let shown = info
        .make_and_model
        .clone()
        .unwrap_or_else(|| printer.target.url.clone());
    let opts = labels::print::Options {
        tray: how.tray.clone().or(printer.tray.clone()),
        color_mode: colour,
    };
    let sent =
        match labels::print::print_doc(&printer.target, &info, bytes, mime, &name, page, &opts)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let why = format!("{e:#}");
                let st = if why.contains("aborted") {
                    "aborted"
                } else if why.contains("canceled") {
                    "canceled"
                } else {
                    "failed"
                };
                let _ = runs::state(&state.db, id, st, std::slice::from_ref(&why), None).await;
                return Err(status(StatusCode::BAD_GATEWAY, why));
            }
        };
    let _ = runs::sent(
        &state.db,
        id,
        &printer.target.url,
        printer.id.as_deref(),
        sent.job_id,
        sent.tray.as_deref(),
    )
    .await;
    let now = labels::print::JobNow {
        state: sent.job_state.clone().or(Some("sent".into())),
        reasons: sent.job_state_reasons.clone(),
        sheets: sent.sheets,
    };
    record(state, &run.run, &now).await;
    if let (false, Some(job)) = (now.finished(), sent.job_id) {
        follow_on(state.clone(), run.run.clone(), printer.target.clone(), job);
    }
    let pages = run.run.pages as usize;
    Ok(Printed {
        message: match sent.job_state.as_deref() {
            Some("completed") => {
                format!("printed {} sheet(s)", sent.sheets.unwrap_or(pages as i32))
            }
            Some("stopped" | "held") => format!(
                "the printer is waiting: feed the {} sheet into {} and press OK on the printer",
                sheet.id,
                sent.tray.as_deref().unwrap_or("its tray")
            ),
            _ => "sent: the printer is still working on it (the run keeps its state)".into(),
        },
        printer: shown,
        job_id: sent.job_id,
        job_state: sent.job_state,
        job_state_reasons: sent.job_state_reasons,
        sheets: sent.sheets,
        tray: sent.tray,
        design: designs,
        sheet: sheet.id,
        count: run.run.labels as usize,
        start: run.run.start as usize,
        pages,
        run: id.to_string(),
        sent_as: mime.to_string(),
    })
}

/// Print a run on the label printer at actual size on label stock. A token's print is always
/// held for the owner's approval first, with what to load ("a 5163 sheet, labels 1–3
/// used"); the owner key prints at once. Its job is followed until the printer finishes it.
#[utoipa::path(post, path = "/api/label-runs/{id}/print", tag = "labels",
    params(("id" = String, Path)),
    request_body = PrintAsk,
    responses((status = 200, body = Printed),
        (status = 202, description = "Held for the owner's approval", body = crate::auth::approvals::Approval),
        (status = 404, body = ErrorBody),
        (status = 409, description = "No printer is set, or it cannot print now (its state says why)", body = ErrorBody),
        (status = 502, description = "The printer refused, aborted the job or could not be reached", body = ErrorBody)))]
async fn print_label_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<PrintAsk>>,
) -> Response {
    let how = body.map(|b| b.0).unwrap_or_default();
    match send_run(&state, &id, &how).await {
        Ok(p) => Json(p).into_response(),
        Err(r) => r,
    }
}

/// Mark a run printed by hand (its PDF printed from a browser): the sheet's next free label
/// moves on as when a printer completes a job.
#[utoipa::path(post, path = "/api/label-runs/{id}/done", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, body = runs::Run), (status = 404, body = ErrorBody),
        (status = 409, description = "A printer already finished it", body = ErrorBody)))]
async fn label_run_done(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let run = match runs::get(&state.db, &id).await {
        Ok(Some(r)) => r,
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
        Err(e) => return failed(e),
    };
    if matches!(run.run.state.as_str(), "completed" | "printed") {
        return status(
            StatusCode::CONFLICT,
            format!("the run is {} already", run.run.state),
        );
    }
    if let Err(e) = runs::state(&state.db, &id, "printed", &[], None).await {
        return failed(e);
    }
    advance_sheet(&state, &run.run).await;
    match runs::get(&state.db, &id).await {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
        Err(e) => failed(e),
    }
}

/// What to reprint
#[derive(Debug, Deserialize, ToSchema, Default)]
pub struct Reprint {
    /// The first label (default the sheet's next free label)
    pub start: Option<usize>,
    /// Only these labels of the run, from 1 in its order (default all)
    pub labels: Option<Vec<usize>>,
    pub note: Option<String>,
}

/// A new run with a run's labels as they were (each label's data, `design@version` and
/// source, settings, brand, QR code), all or some, from another label; print it with
/// `POST /api/label-runs/{id}/print`.
#[utoipa::path(post, path = "/api/label-runs/{id}/reprint", tag = "labels",
    params(("id" = String, Path)),
    request_body = Reprint,
    responses((status = 201, body = runs::Run), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
async fn reprint_label_run(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    body: Option<Json<Reprint>>,
) -> Response {
    let ask = body.map(|b| b.0).unwrap_or_default();
    let run = match runs::get(&state.db, &id).await {
        Ok(Some(r)) => r,
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no label run {id:?}")),
        Err(e) => return failed(e),
    };
    let sheet = match sheet_for(&state, Some(&run.run.sheet)).await {
        Ok((s, _)) => s,
        Err(m) => return status(StatusCode::BAD_REQUEST, m),
    };
    let start = match start_for(&state, &sheet, ask.start).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Some(p) = &ask.labels
        && let Some(bad) = p.iter().find(|n| **n == 0 || **n > run.run.labels as usize)
    {
        return status(
            StatusCode::BAD_REQUEST,
            format!(
                "label {bad} is not in the run: its labels are 1 to {}",
                run.run.labels
            ),
        );
    }
    let printer = match labels::printers::default(&state.db).await {
        Ok(d) => run.run.printer_id.clone().or(d.map(|p| p.id)),
        Err(e) => return failed(e),
    };
    let built = match rebuild(
        &state,
        &run,
        start,
        ask.labels.as_deref(),
        printer.as_deref(),
    )
    .await
    {
        Ok(b) => b,
        Err(r) => return r,
    };
    let note = ask.note.clone().or_else(|| run.run.note.clone());
    match keep_run(
        &state,
        built,
        &by(actor.as_deref()),
        Some(&id),
        note.as_deref(),
    )
    .await
    {
        Ok(r) => (StatusCode::CREATED, Json(r)).into_response(),
        Err(r) => r,
    }
}

// ---- the sheets' next free labels ----

/// A sheet and its next free label
#[derive(Serialize, ToSchema)]
pub struct SheetNext {
    pub sheet: String,
    pub name: String,
    /// Labels on a sheet
    pub per_sheet: usize,
    /// The next free label on the part-used sheet (1: a new sheet)
    pub next_label: i64,
    /// The run that left it so (none: set by hand, or never used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

pub(crate) async fn sheets_next(state: &AppState) -> anyhow::Result<Vec<SheetNext>> {
    let used = runs::sheet_use(&state.db).await?;
    Ok(labels::sheets(&state.db)
        .await
        .into_iter()
        .map(|s| {
            let u = used.iter().find(|u| u.sheet == s.id);
            SheetNext {
                per_sheet: s.per_sheet(),
                next_label: u.map_or(1, |u| u.next_label),
                run_id: u.and_then(|u| u.run_id.clone()),
                updated_at: u.map(|u| u.updated_at.clone()),
                sheet: s.id,
                name: s.name,
            }
        })
        .collect())
}

/// Every sheet with its next free label: where a run starts unless told otherwise.
#[utoipa::path(get, path = "/api/label-sheets", tag = "labels",
    responses((status = 200, body = Vec<SheetNext>)))]
async fn list_label_sheets(State(state): State<AppState>) -> Response {
    match sheets_next(&state).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct NextLabel {
    /// The next free label, from 1 (1: a new sheet)
    pub next: i64,
}

/// Set a sheet's next free label by hand (labels used outside Roastwarden; 1 for a new sheet).
#[utoipa::path(put, path = "/api/label-sheets/{id}", tag = "labels",
    params(("id" = String, Path, description = "the sheet's id")),
    request_body = NextLabel,
    responses((status = 200, body = SheetNext), (status = 400, body = ErrorBody)))]
async fn set_label_sheet(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<NextLabel>,
) -> Response {
    let sheet = match sheet_for(&state, Some(&id)).await {
        Ok((s, _)) => s,
        Err(m) => return status(StatusCode::BAD_REQUEST, m),
    };
    if !(1..=sheet.per_sheet() as i64).contains(&b.next) {
        return status(
            StatusCode::BAD_REQUEST,
            format!("next is a label from 1 to {}", sheet.per_sheet()),
        );
    }
    if let Err(e) = runs::set_next(&state.db, &id, b.next, None).await {
        return failed(e);
    }
    match sheets_next(&state).await {
        Ok(list) => match list.into_iter().find(|s| s.sheet == id) {
            Some(s) => Json(s).into_response(),
            None => status(StatusCode::NOT_FOUND, format!("no sheet {id:?}")),
        },
        Err(e) => failed(e),
    }
}
