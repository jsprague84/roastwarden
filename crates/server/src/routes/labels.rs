//! Bag labels (docs/labels.md): the sheets, designs and settings to choose from; a roast's
//! labels as a PDF (to print at actual size) or the first page as SVG (the preview); the
//! calibration page; and for designing, a check of one label (Typst's errors, a layout
//! report, a small PNG) and a render of a design's source that is not saved.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::label_runs::{build, keep_run, send_run, PrintAsk, Printed, RunAsk, RunItemAsk};
use crate::app::state::AppState;
use crate::auth::Actor;
use crate::http::errors::ErrorBody;
use crate::labels::{self, store, Diag, LabelError, Offset, Output, Report, RestDays, Sheet};

/// The largest design source a request may carry
pub(crate) const MAX_SOURCE: usize = 256 << 10;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(label_options))
        .routes(routes!(calibration))
        .routes(routes!(labels_pdf))
        .routes(routes!(labels_svg))
        .routes(routes!(check_label))
        .routes(routes!(render_labels))
        .routes(routes!(label_printer))
        .routes(routes!(print_labels))
}

#[derive(Serialize, ToSchema)]
pub struct SheetView {
    #[serde(flatten)]
    sheet: Sheet,
    /// The owner's print offset for this sheet (`label_offsets`)
    offset: Offset,
    /// The next free label on the part-used sheet (1: a new sheet), kept as runs print
    next_label: i64,
}

#[derive(Serialize, ToSchema)]
pub struct LabelOptions {
    /// Label stock: the built-in sheets and the owner's (`label_sheets`)
    sheets: Vec<SheetView>,
    /// Every design: saved (with versions), the owner's folder, built in
    designs: Vec<super::label_studio::DesignItem>,
    /// The fields a label can leave off (`hide`)
    fields: Vec<String>,
    /// The rest before "best from", days (`rest_days_*`)
    rest_days: RestDays,
    /// Font families a design can name
    fonts: Vec<String>,
    /// Where the owner's designs and fonts go
    folder: String,
    /// Whether renders run in a sandboxed worker process (time and memory limits)
    sandboxed: bool,
    /// Bag types for the retail check: the built-in ones and the owner's (`label_bags`)
    bags: Vec<labels::retail::Bag>,
    /// The bag a retail check assumes unless told (`label_bag`)
    bag: String,
    /// What a label's QR code opens unless told (`label_qr`): `app`, `share`, `supplier` or
    /// `none`
    qr: String,
}

/// What can go into labels: sheets with their offsets, designs, fields, rest days, fonts.
#[utoipa::path(get, path = "/api/labels", tag = "labels",
    responses((status = 200, body = LabelOptions)))]
async fn label_options(State(state): State<AppState>) -> Json<LabelOptions> {
    let offsets = labels::offsets(&state.db).await;
    let used = labels::runs::sheet_use(&state.db).await.unwrap_or_default();
    let sheets = labels::sheets(&state.db)
        .await
        .into_iter()
        .map(|s| SheetView {
            offset: offsets.get(&s.id).copied().unwrap_or_default(),
            next_label: used
                .iter()
                .find(|u| u.sheet == s.id)
                .map_or(1, |u| u.next_label),
            sheet: s,
        })
        .collect();
    let designs = super::label_studio::all_designs(&state)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "listing label designs");
            vec![]
        });
    let mut extra = store::fonts(&state.db).await.unwrap_or_default();
    let folder = state.labels.clone();
    let fonts = tokio::task::spawn_blocking(move || {
        extra.extend(folder.own_fonts());
        labels::font_families(&extra)
    })
    .await
    .unwrap_or_default();
    Json(LabelOptions {
        sheets,
        designs,
        fields: labels::FIELDS
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        rest_days: labels::rest_days(&state.db).await,
        fonts,
        folder: state.labels.0.display().to_string(),
        sandboxed: state.label_renderer.sandboxed(),
        bags: labels::retail::bags(&state.db).await,
        bag: crate::auth::settings::get(&state.db, "label_bag")
            .await
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "pouch-12oz".into()),
        qr: crate::auth::settings::get(&state.db, "label_qr")
            .await
            .map(|q| q.trim().to_string())
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| "app".into()),
    })
}

#[derive(Deserialize, IntoParams)]
pub struct SheetQuery {
    /// The sheet's id (default 5163)
    sheet: Option<String>,
    /// The printer whose calibration for the sheet applies (default the default printer)
    printer: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct LabelQuery {
    /// The sheet's id (default 5163)
    sheet: Option<String>,
    /// The design's id (default plain)
    design: Option<String>,
    /// The design's parameters as a JSON object, e.g. `{"art":"cherry"}`
    params: Option<String>,
    /// A brand kit's id (default: the bean's, then the `label_brand` setting)
    brand: Option<String>,
    /// How many labels (default 1; at most 200)
    count: Option<usize>,
    /// The first label to print on, from 1 (a part-used sheet)
    start: Option<usize>,
    /// Net weight on the label, g (default the roast's roasted weight)
    weight_g: Option<f64>,
    /// `whole` (default) or `ground`
    form: Option<String>,
    /// Ground: the grind ("espresso", "filter", …); implies ground
    grind: Option<String>,
    /// A line for a gift, e.g. "for Sam"
    message: Option<String>,
    /// Tasting notes instead of the roast's cupping notes or the bean's
    notes: Option<String>,
    /// Fields to leave off, comma-separated (`GET /api/labels` lists them)
    hide: Option<String>,
    /// What the QR code opens: `app`, `share`, `supplier`, `none` or an address (default:
    /// the design's own, `share` for gift, else the `label_qr` setting, else `app`)
    qr: Option<String>,
    /// The offset from UTC in minutes east, for the dates (default: the `time_zone`
    /// setting, else UTC)
    tz: Option<i32>,
}

/// Labels to lay out: a design (saved, or its source), the sheet, the roast and what to
/// change on the label.
#[derive(Deserialize, ToSchema, Default)]
pub struct LabelAsk {
    /// A design's id (default plain)
    pub(crate) design: Option<String>,
    /// A design's Typst source to use instead: a draft, or a one-off that is not saved
    pub(crate) source: Option<String>,
    /// The sheet's id (default 5163)
    pub(crate) sheet: Option<String>,
    /// The roast to label (its id); none: sample data
    pub(crate) roast: Option<String>,
    /// With no roast: a coffee (bean id) on sample roast data, its own label too (a preview)
    pub(crate) bean: Option<String>,
    /// The design's parameters (`design.params`); text values take the default's type
    #[serde(default)]
    pub(crate) params: Map<String, Value>,
    /// A brand kit's id, for `l.brand` (default: the bean's, then the `label_brand` setting)
    pub(crate) brand: Option<String>,
    /// A brand kit given whole instead, not saved: to try one out (the Brands page's preview)
    pub(crate) brand_kit: Option<store::Brand>,
    /// The printer the labels are laid out for (its calibration for the sheet) and printed
    /// on; default the default printer
    pub(crate) printer: Option<String>,
    /// print: a tray (`media-source`) instead of the printer's
    pub(crate) tray: Option<String>,
    /// print: `monochrome` or `color` instead of the printer's
    pub(crate) color: Option<String>,
    /// For coffee that is sold: a bag type's id (`GET /api/labels` lists them; `label` for a
    /// label that is the whole front; empty: the `label_bag` setting, else the 12 oz pouch).
    /// The label's data gets `retail` (the heights the bag needs) and a check answers the
    /// retail check (21 CFR 101.3, 101.5, 101.7)
    pub(crate) retail: Option<String>,
    /// How many labels (default 1; at most 200)
    pub(crate) count: Option<usize>,
    /// The first label to print on, from 1 (a part-used sheet)
    pub(crate) start: Option<usize>,
    /// Net weight on the label, g (default the roast's roasted weight)
    pub(crate) weight_g: Option<f64>,
    /// `whole` (default) or `ground`: the label's form line ("Whole bean", "Ground ·
    /// espresso"); a ground bag's best-until is the earlier of the lot's and today plus
    /// `best_until_days_ground`
    pub(crate) form: Option<String>,
    /// Ground: the grind ("espresso", "filter", "moka", "French press", …); implies ground
    pub(crate) grind: Option<String>,
    /// A line for a gift, e.g. "for Sam"
    pub(crate) message: Option<String>,
    /// Tasting notes instead of the roast's cupping notes or the bean's
    pub(crate) notes: Option<String>,
    /// Fields to leave off (`GET /api/labels` lists them)
    #[serde(default)]
    pub(crate) hide: Vec<String>,
    /// What the QR code opens: `app` (the roast in the dashboard), `share` (its public share
    /// page, made when the roast has none and the caller may share), `supplier` (the coffee's
    /// offer, the bean's link), `none`, or an address (`https://…`). Default: the design's own (gift: `share`), the `label_qr`
    /// setting, else `app`
    pub(crate) qr: Option<String>,
    /// The offset from UTC in minutes east, for the dates (default: the `time_zone`
    /// setting, else UTC)
    pub(crate) tz: Option<i32>,
    /// check: also a PNG of the label at this many dots per inch (36 to 200; 100 is plenty
    /// to look at)
    pub(crate) png: Option<u32>,
    /// render: `pdf` (default) or `svg` (the first page with outlines)
    pub(crate) format: Option<String>,
}

fn bad(message: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, message.into()).into_response()
}

pub(crate) async fn sheet_for(
    state: &AppState,
    id: Option<&str>,
) -> Result<(Sheet, Offset), String> {
    let id = id.unwrap_or("5163");
    let sheets = labels::sheets(&state.db).await;
    let Some(sheet) = sheets.iter().find(|s| s.id == id).cloned() else {
        let known: Vec<&str> = sheets.iter().map(|s| s.id.as_str()).collect();
        return Err(format!(
            "no sheet {id:?}: {} (GET /api/labels)",
            known.join(", ")
        ));
    };
    let offset = labels::offsets(&state.db)
        .await
        .get(id)
        .copied()
        .unwrap_or_default();
    Ok((sheet, offset))
}

fn not_found(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

/// A design by reference (`id`, `id@N`): saved, then the owner's folder, then built in.
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn design_for(state: &AppState, reference: &str) -> Result<labels::DesignSource, Response> {
    let Some((id, version)) = store::parse_ref(reference) else {
        return Err(bad("a design is its id, or id@version"));
    };
    match store::design(&state.db, &id, version).await {
        Ok(Some(d)) => {
            return Ok(labels::DesignSource {
                id: d.id,
                version: Some(d.shown),
                source: d.source,
                files: std::collections::BTreeMap::new(),
            })
        }
        Ok(None) if version.is_some() => {
            return Err(not_found(format!(
                "no version {} of {id:?} (GET /api/label-designs/{id})",
                version.unwrap_or_default()
            )))
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "reading a label design");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the design",
            )
                .into_response());
        }
    }
    let folder = state.labels.clone();
    let wanted = id.clone();
    let found = tokio::task::spawn_blocking(move || folder.design(&wanted))
        .await
        .ok()
        .flatten();
    found.ok_or_else(|| {
        not_found(format!(
            "no design {id:?} (GET /api/label-designs lists them)"
        ))
    })
}

/// A brand kit by id, as `l.brand`
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn brand_for(state: &AppState, id: Option<&str>) -> Result<Value, Response> {
    let Some(id) = id.filter(|i| !i.trim().is_empty()) else {
        return Ok(store::Brand::value(None));
    };
    match store::brand(&state.db, id).await {
        Ok(Some(b)) => Ok(store::Brand::value(Some(&b))),
        Ok(None) => Err(bad(format!("no brand {id:?} (GET /api/label-brands)"))),
        Err(e) => {
            tracing::error!(error = %e, "reading a brand");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the brand",
            )
                .into_response())
        }
    }
}

/// What labels are laid out for: to be made (a PDF, a print, a print run) or only looked at
/// (a preview or a check). Only labels that are made make a roast's share link: a preview
/// of a roast with none shows a placeholder code (the same size of QR code).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Purpose {
    Preview,
    Make,
}

/// Everything but the output: the job, ready to render. A roast's bean gives the design,
/// its parameters and the brand when the request does not (`PUT /api/beans/{id}/label`);
/// then the `label_brand` setting gives the brand.
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub(crate) async fn prepare(
    state: &AppState,
    headers: &HeaderMap,
    who: Option<&Actor>,
    mut ask: LabelAsk,
    outline: bool,
    purpose: Purpose,
) -> Result<labels::Job, Response> {
    let (sheet, offset) = sheet_for(state, ask.sheet.as_deref()).await.map_err(bad)?;
    // The printer's calibration for this sheet, else the sheet's own offset
    let offset =
        labels::printers::offset_for(&state.db, ask.printer.as_deref(), &sheet.id, offset).await;
    let count = ask.count.unwrap_or(1);
    let start = ask.start.unwrap_or(1);
    if !(1..=200).contains(&count) {
        return Err(bad("count is 1 to 200"));
    }
    if !(1..=sheet.per_sheet()).contains(&start) {
        return Err(bad(format!(
            "start is a label from 1 to {} on this sheet",
            sheet.per_sheet()
        )));
    }
    if ask
        .weight_g
        .is_some_and(|g| !(g.is_finite() && g > 0.0 && g < 100_000.0))
    {
        return Err(bad("weight_g is grams above 0"));
    }
    let hide: Vec<String> = ask
        .hide
        .iter()
        .map(|f| f.trim().replace('-', "_"))
        .filter(|f| !f.is_empty())
        .collect();
    if let Some(f) = hide.iter().find(|f| !labels::FIELDS.contains(&f.as_str())) {
        return Err(bad(format!(
            "no field {f:?}: {}",
            labels::FIELDS.join(", ")
        )));
    }
    let form = crate::coffee::roasted::Form::parse(ask.form.as_deref(), ask.grind.as_deref())
        .map_err(bad)?;
    let zone = crate::ops::zone::resolve(&state.db, ask.tz).await;
    let ask_bits = labels::Ask {
        weight_g: ask.weight_g,
        message: ask.message.clone(),
        notes: ask.notes.clone(),
        hide,
        zone,
        form,
        best: crate::coffee::roasted::BestDays::load(&state.db).await,
        packed_on: Some(zone.date(chrono::Utc::now())),
    };
    let rest = labels::rest_days(&state.db).await;
    let (data, url, bean_id) = match &ask.roast {
        Some(id) => {
            let base = crate::auth::approvals::link_base(&state.db, headers).await;
            let facts = match labels::facts(&state.db, id, &base).await {
                Ok(Some(f)) => f,
                Ok(None) => return Err(not_found(format!("no roast {id:?}"))),
                Err(e) => {
                    tracing::error!(error = %e, "reading a roast for labels");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "could not read the roast",
                    )
                        .into_response());
                }
            };
            (
                labels::label_data(&facts, &ask_bits, &rest),
                facts.url.clone(),
                facts.bean_id.clone(),
            )
        }
        None => match &ask.bean {
            Some(b) => match labels::bean_sample(&state.db, b).await {
                Ok(Some(base)) => {
                    let (d, u) = labels::with_ask(base, &ask_bits);
                    (d, u, Some(b.clone()))
                }
                Ok(None) => return Err(not_found(format!("no bean {b:?}"))),
                Err(e) => {
                    tracing::error!(error = %e, "reading a bean for labels");
                    return Err(
                        (StatusCode::INTERNAL_SERVER_ERROR, "could not read the bean")
                            .into_response(),
                    );
                }
            },
            None => {
                let (d, u) = labels::sample_data(&ask_bits);
                (d, u, None)
            }
        },
    };
    let mut data = data;
    let app_url = url;
    // Coffee that is sold: the heights its bag's front needs, for the design to meet
    if let Some(asked) = &ask.retail {
        let bag = labels::retail::bag_for(&state.db, asked, sheet.w, sheet.h)
            .await
            .map_err(bad)?;
        data["retail"] = json!({
            "bag": bag.name,
            "area_sq_in": (bag.area() * 10.0).round() / 10.0,
            "min_net_mm": (labels::retail::min_letter_in(bag.area()) * 25.4 * 10.0).round() / 10.0,
            "min_mm": 1.6,
        });
    }
    // The bean's own label fills what the request leaves out
    if let Some(bean) = &bean_id
        && let Ok(Some(own)) = store::bean_label(&state.db, bean).await {
            let own_design = ask.source.is_none() && ask.design.is_none() && own.design.is_some();
            if own_design {
                ask.design.clone_from(&own.design);
                let mut params = own.params.clone();
                params.extend(std::mem::take(&mut ask.params));
                ask.params = params;
            }
            if ask.brand.is_none() {
                ask.brand.clone_from(&own.brand);
            }
        }
    if ask.brand.is_none() {
        ask.brand = crate::auth::settings::get(&state.db, "label_brand").await;
    }
    let mut design = match (&ask.source, &ask.design) {
        (Some(src), _) => {
            if src.len() > MAX_SOURCE {
                return Err(bad(format!(
                    "a design's source is at most {} KB",
                    MAX_SOURCE >> 10
                )));
            }
            labels::DesignSource {
                id: "source".into(),
                version: None,
                source: src.clone(),
                files: std::collections::BTreeMap::new(),
            }
        }
        (None, id) => design_for(state, id.as_deref().unwrap_or("plain")).await?,
    };
    // What the QR code opens: as asked, else what the design says (a gift's `share`), else
    // the setting
    let from_design = if ask.source.is_some() {
        None
    } else {
        design_qr(state, &design).await
    };
    let url = qr_url(
        state,
        headers,
        who,
        &ask,
        app_url.clone(),
        purpose,
        from_design,
    )
    .await?;
    let hidden = ask.hide.iter().any(|h| h.trim() == "qr");
    data["qr"] = json!(url.is_some() && !hidden);
    data["url"] = json!(url.as_ref().map(|u| u.shown.clone()));
    // Where it goes, for a caption: `share` (the share page), `app`, or `link` (the supplier
    // or any address)
    data["qr_to"] = json!(url.as_ref().map(|u| {
        if u.shown.contains("/R/") {
            "share"
        } else if Some(&u.shown) == app_url.as_ref() {
            "app"
        } else {
            "link"
        }
    }));
    let url = url.map(|u| u.encoded);
    let brand = match &ask.brand_kit {
        Some(kit) => {
            kit.check().map_err(bad)?;
            store::Brand::value(Some(kit))
        }
        None => brand_for(state, ask.brand.as_deref()).await?,
    };
    // The images it names (in its source, its parameters or the brand's logo), and every font
    let params_text = Value::Object(ask.params.clone()).to_string();
    let logo = brand["logo"].as_str().unwrap_or_default().to_string();
    match store::images_named(&state.db, &[&design.source, &params_text, &logo]).await {
        Ok(images) => design.files.extend(images),
        Err(e) => tracing::warn!(error = %e, "reading label images"),
    }
    let mut fonts = {
        let folder = state.labels.clone();
        tokio::task::spawn_blocking(move || folder.own_fonts())
            .await
            .unwrap_or_default()
    };
    fonts.extend(store::fonts(&state.db).await.unwrap_or_default());
    let mut job = labels::Job::new(sheet, offset, Some(design));
    job.slots = labels::slots(data, count, start);
    job.url = url;
    job.outline = outline;
    job.params = ask.params;
    job.brand = brand;
    job.fonts = fonts;
    Ok(job)
}

/// A QR code's address: as shown on the label, and as encoded (the share page's in capitals)
pub(crate) struct QrUrl {
    pub shown: String,
    pub encoded: String,
}

/// Whether `who` may make a share link (the owner, or a token that may print)
pub(crate) fn may_share(who: Option<&Actor>) -> bool {
    match who {
        Some(a) if a.is_owner() => true,
        Some(Actor::Token(t)) => t.preset.allows("POST", "/api/sessions/x/share"),
        _ => false,
    }
}

/// What a design's manifest says its QR code opens unless asked (`design.qr`: `app`,
/// `share`, `supplier` or `none`; the gift designs say `share`)
async fn design_qr(state: &AppState, design: &labels::DesignSource) -> Option<String> {
    let manifest = match store::design(&state.db, &design.id, design.version).await {
        Ok(Some(d)) => d.manifest,
        _ => labels::builtin_manifests(&state.label_renderer)
            .await
            .get(&design.id)
            .cloned(),
    }?;
    manifest["qr"]
        .as_str()
        .map(str::trim)
        .filter(|q| QR_CHOICES.contains(q))
        .map(str::to_string)
}

/// What a design's manifest may name as its QR code's default
const QR_CHOICES: [&str; 4] = ["app", "share", "supplier", "none"];

/// What the label's QR code opens: `ask.qr`, else the design's own default (`from_design`),
/// else the `label_qr` setting, else the app's page for the roast (`app`, given). A roast's
/// share page needs a link: its newest one, else a new one when the labels are made and `who`
/// may make one; a preview of a roast with no link shows a placeholder code and makes nothing.
/// When only the design asked and it cannot be (no link and no right to make one, no supplier
/// link), the app's page.
#[allow(clippy::result_large_err)] // the error is the answer, made once
#[allow(clippy::too_many_arguments)]
async fn qr_url(
    state: &AppState,
    headers: &HeaderMap,
    who: Option<&Actor>,
    ask: &LabelAsk,
    app: Option<String>,
    purpose: Purpose,
    from_design: Option<String>,
) -> Result<Option<QrUrl>, Response> {
    match ask.qr.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => qr_for(state, headers, who, ask, app, purpose, q).await,
        None => match from_design {
            Some(d) => match qr_for(state, headers, who, ask, app.clone(), purpose, &d).await {
                Ok(u) => Ok(u),
                Err(_) => Ok(app.map(|u| QrUrl {
                    shown: u.clone(),
                    encoded: u,
                })),
            },
            None => {
                let setting = crate::auth::settings::get(&state.db, "label_qr")
                    .await
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "app".into());
                qr_for(state, headers, who, ask, app, purpose, &setting).await
            }
        },
    }
}

/// The QR code's address for one choice (`app`, `share`, `supplier`, `none` or an address)
#[allow(clippy::result_large_err)] // the error is the answer, made once
async fn qr_for(
    state: &AppState,
    headers: &HeaderMap,
    who: Option<&Actor>,
    ask: &LabelAsk,
    app: Option<String>,
    purpose: Purpose,
    asked: &str,
) -> Result<Option<QrUrl>, Response> {
    let plain = |u: String| QrUrl {
        shown: u.clone(),
        encoded: u,
    };
    match asked {
        "app" => Ok(app.map(plain)),
        "none" => Ok(None),
        // The coffee's offer at its supplier (the bean's link)
        "supplier" => {
            let bean = match (&ask.roast, &ask.bean) {
                (Some(id), _) => crate::roasts::sessions::get_session(&state.db, id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|s| s.bean_id),
                (None, b) => b.clone(),
            };
            let Some(bean) = bean else {
                // Sample data: the sample's own address
                return Ok(app.map(plain));
            };
            let link = crate::coffee::inventory::bean_link(&state.db, &bean).await;
            match link.filter(|u| u.starts_with("https://") || u.starts_with("http://")) {
                Some(u) => Ok(Some(plain(u))),
                None => Err(bad(
                    "this coffee has no supplier link: add it (roast bean edit <bean> --link <offer>)",
                )),
            }
        }
        "share" => {
            let base = crate::auth::approvals::link_base(&state.db, headers).await;
            let code = match &ask.roast {
                None => labels::share::SAMPLE_CODE.to_string(),
                Some(id) => match labels::share::active(&state.db, id).await {
                    Ok(Some(link)) => link.code,
                    Ok(None) if purpose == Purpose::Preview => {
                        labels::share::SAMPLE_CODE.to_string()
                    }
                    Ok(None) if may_share(who) => {
                        let by = super::label_studio::by(who);
                        match labels::share::create(&state.db, id, None, ask.tz, &by).await {
                            Ok(link) => link.code,
                            Err(e) => {
                                tracing::error!(error = %e, "making a share link for a label");
                                return Err((
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    "could not make the share link",
                                )
                                    .into_response());
                            }
                        }
                    }
                    Ok(None) => {
                        return Err((
                            StatusCode::FORBIDDEN,
                            format!(
                                "roast {id:?} has no share link and this token may not make one: \
                                 ask the owner to share it (roast share {id})"
                            ),
                        )
                            .into_response())
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "reading share links");
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "could not read the share links",
                        )
                            .into_response());
                    }
                },
            };
            Ok(Some(QrUrl {
                shown: labels::share::url(&base, &code),
                encoded: labels::share::qr_text(&base, &code),
            }))
        }
        u if u.starts_with("https://") || u.starts_with("http://") => {
            if u.len() > 512 || u.chars().any(char::is_whitespace) {
                return Err(bad(
                    "a QR address is at most 512 characters, without spaces",
                ));
            }
            Ok(Some(plain(u.to_string())))
        }
        other => Err(bad(format!(
            "qr is app, share, supplier, none or an address (https://…), not {other:?}"
        ))),
    }
}

/// A job for checking a design's source on sample data (a save checks first).
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub(crate) async fn job_for_check(
    state: &AppState,
    source: &str,
    sheet: Option<&str>,
) -> Result<labels::Job, Response> {
    let ask = LabelAsk {
        source: Some(source.to_string()),
        sheet: sheet.map(str::to_string),
        ..LabelAsk::default()
    };
    prepare(state, &HeaderMap::new(), None, ask, false, Purpose::Preview).await
}

fn file_response(bytes: Vec<u8>, output: Output) -> Response {
    let (ty, name) = match output {
        Output::Pdf => ("application/pdf", "labels.pdf"),
        Output::Svg => ("image/svg+xml", "labels.svg"),
    };
    (
        [
            (header::CONTENT_TYPE, ty.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    )
        .into_response()
}

pub(crate) fn label_error(e: LabelError) -> Response {
    match e {
        LabelError::Bad(m) => bad(m),
        LabelError::Design(m) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("the design does not compile:\n{m}"),
        )
            .into_response(),
    }
}

async fn rendered(state: &AppState, job: labels::Job, output: Output) -> Response {
    match labels::render(&state.label_renderer, job, output).await {
        Ok(bytes) => file_response(bytes, output),
        Err(e) => label_error(e),
    }
}

impl LabelQuery {
    #[allow(clippy::result_large_err)] // the error is the answer, made once
    fn into_ask(self, roast: String) -> Result<LabelAsk, Response> {
        let params = match self.params.as_deref().map(str::trim) {
            None | Some("") => Map::new(),
            Some(text) => serde_json::from_str(text).map_err(|e| {
                bad(format!(
                    "params is a JSON object, e.g. {{\"art\":\"cherry\"}}: {e}"
                ))
            })?,
        };
        Ok(LabelAsk {
            design: self.design,
            brand: self.brand,
            sheet: self.sheet,
            roast: Some(roast),
            params,
            count: self.count,
            start: self.start,
            weight_g: self.weight_g,
            form: self.form,
            grind: self.grind,
            message: self.message,
            notes: self.notes,
            hide: self
                .hide
                .unwrap_or_default()
                .split(',')
                .map(str::to_string)
                .collect(),
            qr: self.qr,
            tz: self.tz,
            ..LabelAsk::default()
        })
    }
}

async fn roast_labels(
    state: AppState,
    headers: HeaderMap,
    who: Option<Actor>,
    id: String,
    q: LabelQuery,
    output: Output,
) -> Response {
    let ask = match q.into_ask(id) {
        Ok(a) => a,
        Err(r) => return r,
    };
    // The PDF is the labels; the SVG is the preview
    let purpose = match output {
        Output::Pdf => Purpose::Make,
        Output::Svg => Purpose::Preview,
    };
    match prepare(
        &state,
        &headers,
        who.as_ref(),
        ask,
        output == Output::Svg,
        purpose,
    )
    .await
    {
        Ok(job) => rendered(&state, job, output).await,
        Err(r) => r,
    }
}

/// A roast's labels as a PDF: print it at "Actual size" on the sheet (docs/labels.md).
#[utoipa::path(get, path = "/api/sessions/{id}/labels.pdf", tag = "labels",
    params(("id" = String, Path), LabelQuery),
    responses((status = 200, content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Unknown sheet, design or field, or numbers out of range", body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 422, description = "The design does not compile: what Typst said", body = ErrorBody)))]
async fn labels_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    Query(q): Query<LabelQuery>,
) -> Response {
    roast_labels(state, headers, actor.map(|a| a.0), id, q, Output::Pdf).await
}

/// The first page of the same labels as SVG, with each label's outline: the preview.
#[utoipa::path(get, path = "/api/sessions/{id}/labels.svg", tag = "labels",
    params(("id" = String, Path), LabelQuery),
    responses((status = 200, content_type = "image/svg+xml", body = String),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody)))]
async fn labels_svg(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Path(id): Path<String>,
    Query(q): Query<LabelQuery>,
) -> Response {
    roast_labels(state, headers, actor.map(|a| a.0), id, q, Output::Svg).await
}

/// A calibration page for a sheet: marks at the label corners after the saved offset, and
/// a 100 mm rule. Print on plain paper and hold it over a sheet against the light.
#[utoipa::path(get, path = "/api/labels/calibration.pdf", tag = "labels",
    params(SheetQuery),
    responses((status = 200, content_type = "application/pdf", body = Vec<u8>),
        (status = 400, body = ErrorBody)))]
async fn calibration(State(state): State<AppState>, Query(q): Query<SheetQuery>) -> Response {
    let (sheet, offset) = match sheet_for(&state, q.sheet.as_deref()).await {
        Ok(s) => s,
        Err(m) => return bad(m),
    };
    let offset =
        labels::printers::offset_for(&state.db, q.printer.as_deref(), &sheet.id, offset).await;
    rendered(&state, labels::Job::new(sheet, offset, None), Output::Pdf).await
}

/// What a check found.
#[derive(Serialize, ToSchema)]
pub struct CheckReply {
    /// It compiles and the report found nothing to fix
    ok: bool,
    /// Typst's errors with file and line: fix these first (there is no report until it
    /// compiles)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<Diag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<Report>,
    /// The label as a PNG, base64, when asked (`png`)
    #[serde(skip_serializing_if = "Option::is_none")]
    png: Option<String>,
    /// What was laid out: the design (`source` for a draft), the sheet, and `sample` or
    /// the roast's id
    design: String,
    sheet: String,
    data: String,
    /// With `retail`: what a bag that is sold needs, found and not (its issues are in the
    /// report's too, as `retail_*`)
    #[serde(skip_serializing_if = "Option::is_none")]
    retail: Option<labels::retail::Retail>,
}

/// Check one label of a design before saving or printing it: Typst's errors with lines, or
/// a layout report (each line of text with its font, size and box in mm; what runs past
/// the edge, overlaps, is too small, was shrunk or cut; the QR code's size; which fields
/// printed) and, when asked, a small PNG. With no roast, sample data.
#[utoipa::path(post, path = "/api/labels/check", tag = "labels",
    request_body = LabelAsk,
    responses((status = 200, body = CheckReply),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
async fn check_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Json(ask): Json<LabelAsk>,
) -> Response {
    let dpi = ask.png;
    let data = ask.roast.clone().unwrap_or_else(|| "sample".into());
    let asked_retail = ask.retail.clone();
    let job = match prepare(
        &state,
        &headers,
        actor.as_deref(),
        ask,
        false,
        Purpose::Preview,
    )
    .await
    {
        Ok(j) => j,
        Err(r) => return r,
    };
    let brand = job.brand.clone();
    let bag = match &asked_retail {
        Some(a) => match labels::retail::bag_for(&state.db, a, job.sheet.w, job.sheet.h).await {
            Ok(b) => Some(b),
            Err(m) => return bad(m),
        },
        None => None,
    };
    let design = job
        .design
        .as_ref()
        .map(labels::DesignSource::label)
        .unwrap_or_default();
    let sheet = job.sheet.id.clone();
    match labels::check(&state.label_renderer, job, dpi).await {
        Ok(mut c) => {
            let retail = match (&bag, c.report.as_mut()) {
                (Some(bag), Some(report)) => {
                    let r = labels::retail::check(report, bag, &brand);
                    report.issues.extend(r.issues.iter().cloned());
                    report.ok = report
                        .issues
                        .iter()
                        .all(|i| labels::engine::SOFT.contains(&i.kind.as_str()));
                    Some(r)
                }
                _ => None,
            };
            Json(CheckReply {
                ok: c.errors.is_empty() && c.report.as_ref().is_some_and(|r| r.ok),
                errors: c.errors,
                report: c.report,
                png: c
                    .png
                    .map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
                design,
                sheet,
                data,
                retail,
            })
            .into_response()
        }
        Err(e) => label_error(e),
    }
}

/// Labels from a design's source or a saved design, as a PDF (or the first page as SVG
/// with `format: svg`): a one-off that is not saved. With no roast, sample data.
#[utoipa::path(post, path = "/api/labels/render", tag = "labels",
    request_body = LabelAsk,
    responses((status = 200, content_type = "application/pdf", body = Vec<u8>),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 422, description = "The design does not compile: what Typst said", body = ErrorBody)))]
async fn render_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    actor: Option<Extension<Actor>>,
    Json(ask): Json<LabelAsk>,
) -> Response {
    let output = match ask.format.as_deref() {
        None | Some("pdf") => Output::Pdf,
        Some("svg") => Output::Svg,
        Some(other) => return bad(format!("format is pdf or svg, not {other:?}")),
    };
    let purpose = match output {
        Output::Pdf => Purpose::Make,
        Output::Svg => Purpose::Preview,
    };
    match prepare(
        &state,
        &headers,
        actor.as_deref(),
        ask,
        output == Output::Svg,
        purpose,
    )
    .await
    {
        Ok(job) => rendered(&state, job, output).await,
        Err(r) => r,
    }
}

/// The label printer: its address (`label_printer_url`) and what it says about itself.
#[derive(Serialize, ToSchema)]
pub struct PrinterView {
    /// A printer is set (Settings → Roasting → Bag labels, `label_printer_url`)
    configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    printer: Option<labels::print::PrinterInfo>,
    /// Why it cannot print now (stopped, a door open, no PDF), or could not be reached
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct PrinterQuery {
    /// A printer's id (default the default printer)
    printer: Option<String>,
}

/// A label printer and how it is: make and model, state (and why it is stopped), whether
/// it takes PDF (or raster), offers the "labels" paper type and actual size. Only asks;
/// prints nothing. `?printer=` for another than the default.
#[utoipa::path(get, path = "/api/labels/printer", tag = "labels",
    params(PrinterQuery),
    responses((status = 200, body = PrinterView)))]
async fn label_printer(
    State(state): State<AppState>,
    Query(q): Query<PrinterQuery>,
) -> Json<PrinterView> {
    let Ok(r) = super::label_printers::resolve(&state, q.printer.as_deref()).await else {
        return Json(PrinterView {
            configured: false,
            printer: None,
            problem: Some(
                "no printer is set: add one in Label Studio → Printers (or the label_printer_url setting)".into(),
            ),
        });
    };
    match super::label_printers::ask(&state, &r).await {
        Ok(info) => Json(PrinterView {
            configured: true,
            problem: info.problem(),
            printer: Some(info),
        }),
        Err(e) => Json(PrinterView {
            configured: true,
            printer: None,
            problem: Some(format!("{e:#}")),
        }),
    }
}

/// Print a roast's labels on the label printer (`label_printer_url`) at actual size, paper
/// type "labels". A token's print is always held for the owner's approval first, with what
/// to load ("a 5163 sheet, labels 1–3 used"); the owner key prints at once.
#[utoipa::path(post, path = "/api/sessions/{id}/labels/print", tag = "labels",
    params(("id" = String, Path)),
    request_body = LabelAsk,
    responses((status = 200, body = Printed),
        (status = 202, description = "Held for the owner's approval", body = crate::auth::approvals::Approval),
        (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
        (status = 409, description = "No printer is set, or it cannot print now (its state says why)", body = ErrorBody),
        (status = 422, body = ErrorBody), (status = 502, description = "The printer refused or could not be reached", body = ErrorBody)))]
async fn print_labels(
    State(state): State<AppState>,
    actor: Option<Extension<Actor>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(ask): Json<LabelAsk>,
) -> Response {
    if let Err(r) = super::label_printers::resolve(&state, ask.printer.as_deref()).await {
        return r;
    }
    // A run of one roast's labels, so the print is in the history; from label 1 unless told
    // (what the approval said)
    let how = PrintAsk {
        printer: ask.printer.clone(),
        tray: ask.tray.clone(),
        color: ask.color.clone(),
    };
    let run = RunAsk {
        sheet: ask.sheet,
        start: Some(ask.start.unwrap_or(1)),
        tz: ask.tz,
        note: None,
        printer: ask.printer,
        new: true,
        items: vec![RunItemAsk {
            roast: id,
            count: ask.count,
            design: ask.design,
            source: ask.source,
            params: ask.params,
            brand: ask.brand,
            message: ask.message,
            notes: ask.notes,
            weight_g: ask.weight_g,
            form: ask.form,
            grind: ask.grind,
            pack: false,
            hide: ask.hide,
            retail: ask.retail,
            qr: ask.qr,
        }],
    };
    let built = match build(&state, &headers, actor.as_deref(), &run, Purpose::Make).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let who = super::label_studio::by(actor.as_deref());
    let made = match keep_run(&state, built, &who, None, None).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    match send_run(&state, &made.run.id, &how).await {
        Ok(p) => Json(p).into_response(),
        Err(r) => r,
    }
}
