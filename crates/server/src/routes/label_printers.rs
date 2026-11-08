//! Label printers (docs/labels.md, "Printers"): more than one, each
//! asked what it can do and kept with that answer, its tray, colour and how to send (PDF, or
//! PWG Raster or URF for a printer without PDF), the certificate trusted for `ipps`, and its
//! own calibration per sheet. Finding printers on the network (mDNS) works only where the
//! server shares the printer's network; typing the address is the main way.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::app::state::AppState;
use crate::http::errors::ErrorBody;
use crate::labels::print::{self, PrinterInfo, Target};
use crate::labels::printers::{self, Calibration, Printer};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_printers, add_printer))
        .routes(routes!(find_printers))
        .routes(routes!(printer_certificate))
        .routes(routes!(get_printer, edit_printer, delete_printer))
        .routes(routes!(default_printer))
        .routes(routes!(set_calibration, delete_calibration))
}

fn status(code: StatusCode, message: impl Into<String>) -> Response {
    (code, message.into()).into_response()
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!(error = %format!("{e:#}"), "label printers");
    status(
        StatusCode::INTERNAL_SERVER_ERROR,
        "the label printers failed",
    )
}

/// The printer a print goes to, and how to reach it
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Its id (none: the old `label_printer_url` setting)
    pub id: Option<String>,
    pub target: Target,
    /// Its tray, colour and how to send, as set
    pub tray: Option<String>,
    pub color: Option<String>,
    pub send: String,
}

/// `id`, else the default printer, else the `label_printer_url` setting
#[allow(clippy::result_large_err)] // the error is the answer, made once
pub async fn resolve(state: &AppState, id: Option<&str>) -> Result<Resolved, Response> {
    let found = match id.filter(|i| !i.trim().is_empty()) {
        Some(id) => match printers::get(&state.db, id).await {
            Ok(Some(p)) => Some(p),
            Ok(None) => {
                return Err(status(
                    StatusCode::NOT_FOUND,
                    format!("no printer {id:?} (GET /api/label-printers)"),
                ))
            }
            Err(e) => return Err(failed(e)),
        },
        None => printers::default(&state.db).await.map_err(failed)?,
    };
    if let Some(p) = found {
        return Ok(Resolved {
            target: Target::new(&p.url, p.fingerprint.as_deref()),
            id: Some(p.id),
            tray: p.tray,
            color: p.color,
            send: p.send,
        });
    }
    match crate::auth::settings::get(&state.db, "label_printer_url")
        .await
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
    {
        Some(url) => Ok(Resolved {
            id: None,
            target: Target::new(&url, None),
            tray: None,
            color: None,
            send: "auto".into(),
        }),
        None => Err(status(
            StatusCode::CONFLICT,
            "no printer is set: add one in Label Studio → Printers (POST /api/label-printers)",
        )),
    }
}

/// Ask a printer what it can do, and keep its answer
pub async fn ask(state: &AppState, r: &Resolved) -> anyhow::Result<PrinterInfo> {
    let info = print::info(&r.target).await?;
    if let Some(id) = &r.id {
        if let Ok(v) = serde_json::to_value(&info) {
            let _ = printers::set_info(&state.db, id, &v).await;
        }
    }
    Ok(info)
}

/// A printer as listed: as set, its calibrations, and what it said when last asked
#[derive(Serialize, ToSchema)]
pub struct PrinterEntry {
    #[serde(flatten)]
    pub printer: Printer,
    pub calibrations: Vec<Calibration>,
    /// Why it cannot print now, as it said when last asked (a stopped printer, no format
    /// it can be sent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// How labels go to it: pdf, pwg or urf (with the resolution), as it said
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sends: Option<String>,
}

async fn view(state: &AppState, p: Printer, problem: Option<String>) -> PrinterEntry {
    let calibrations = printers::calibrations(&state.db, &p.id)
        .await
        .unwrap_or_default();
    let info: Option<PrinterInfo> = p.info.clone().and_then(|v| serde_json::from_value(v).ok());
    let sends = info.as_ref().and_then(|i| {
        print::choose(i, &p.send).ok().map(|d| match d {
            print::Doc::Pdf => "pdf".to_string(),
            print::Doc::Raster { format, dpi, scale } => format!(
                "{} at {} dpi",
                match format {
                    crate::labels::raster::Format::Pwg => "pwg",
                    crate::labels::raster::Format::Urf => "urf",
                },
                dpi * scale
            ),
        })
    });
    let problem = problem.or_else(|| info.as_ref().and_then(PrinterInfo::problem));
    PrinterEntry {
        printer: p,
        calibrations,
        problem,
        sends,
    }
}

/// The label printers, the default first, each with its calibrations and what it said when
/// last asked (nothing is asked now: `GET /api/label-printers/{id}` asks).
#[utoipa::path(get, path = "/api/label-printers", tag = "labels",
    responses((status = 200, body = Vec<PrinterEntry>)))]
async fn list_printers(State(state): State<AppState>) -> Response {
    match printers::list(&state.db).await {
        Ok(list) => {
            let mut out = Vec::with_capacity(list.len());
            for p in list {
                out.push(view(&state, p, None).await);
            }
            Json(out).into_response()
        }
        Err(e) => failed(e),
    }
}

/// One printer, asked now how it is (state, trays, formats); the answer is kept.
#[utoipa::path(get, path = "/api/label-printers/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, body = PrinterEntry), (status = 404, body = ErrorBody)))]
async fn get_printer(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let r = match resolve(&state, Some(&id)).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let problem = ask(&state, &r).await.err().map(|e| format!("{e:#}"));
    match printers::get(&state.db, &id).await {
        Ok(Some(p)) => Json(view(&state, p, problem).await).into_response(),
        Ok(None) => status(StatusCode::NOT_FOUND, format!("no printer {id:?}")),
        Err(e) => failed(e),
    }
}

/// Save a printer and ask it what it can do (it is saved even when it does not answer: it
/// may be off). An `ipps://` printer needs `fingerprint`, from
/// `POST /api/label-printers/certificate`.
async fn save(state: &AppState, mut p: Printer, id: String) -> Response {
    if !crate::labels::valid_id(&id) {
        return status(
            StatusCode::BAD_REQUEST,
            "a printer's id is letters, digits, - and _ (up to 64)",
        );
    }
    p.id = id.clone();
    if let Err(m) = p.check() {
        return status(StatusCode::BAD_REQUEST, m);
    }
    let created = match printers::put(&state.db, &p).await {
        Ok(c) => c,
        Err(e) => return failed(e),
    };
    let r = Resolved {
        id: Some(id.clone()),
        target: Target::new(&p.url, p.fingerprint.as_deref()),
        tray: None,
        color: None,
        send: p.send.clone(),
    };
    let problem = ask(state, &r).await.err().map(|e| format!("{e:#}"));
    match printers::get(&state.db, &id).await {
        Ok(Some(saved)) => (
            if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(view(state, saved, problem).await),
        )
            .into_response(),
        Ok(None) => status(StatusCode::INTERNAL_SERVER_ERROR, "the printer vanished"),
        Err(e) => failed(e),
    }
}

/// Add a label printer: its id, name and address (`ipp://host:631/ipp/print`), and if
/// wanted its tray, colour, how to send (auto, pdf, pwg, urf) and whether it is the default.
#[utoipa::path(post, path = "/api/label-printers", tag = "labels",
    request_body = Printer,
    responses((status = 201, body = PrinterEntry), (status = 200, body = PrinterEntry),
        (status = 400, body = ErrorBody)))]
async fn add_printer(State(state): State<AppState>, Json(p): Json<Printer>) -> Response {
    let id = if p.id.trim().is_empty() {
        crate::labels::slug(&p.name)
    } else {
        p.id.clone()
    };
    save(&state, p, id).await
}

/// Change a printer (all its settings, as with add).
#[utoipa::path(put, path = "/api/label-printers/{id}", tag = "labels",
    params(("id" = String, Path)),
    request_body = Printer,
    responses((status = 200, body = PrinterEntry), (status = 201, body = PrinterEntry),
        (status = 400, body = ErrorBody)))]
async fn edit_printer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<Printer>,
) -> Response {
    save(&state, p, id).await
}

/// Delete a printer and its calibrations (may wait for the owner's approval).
#[utoipa::path(delete, path = "/api/label-printers/{id}", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_printer(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match printers::delete(&state.db, &id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no printer {id:?}")),
        Err(e) => failed(e),
    }
}

/// Make a printer the default: labels go to it unless another is named.
#[utoipa::path(post, path = "/api/label-printers/{id}/default", tag = "labels",
    params(("id" = String, Path)),
    responses((status = 200, body = PrinterEntry), (status = 404, body = ErrorBody)))]
async fn default_printer(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match printers::make_default(&state.db, &id).await {
        Ok(true) => match printers::get(&state.db, &id).await {
            Ok(Some(p)) => Json(view(&state, p, None).await).into_response(),
            Ok(None) => status(StatusCode::NOT_FOUND, format!("no printer {id:?}")),
            Err(e) => failed(e),
        },
        Ok(false) => status(StatusCode::NOT_FOUND, format!("no printer {id:?}")),
        Err(e) => failed(e),
    }
}

/// A calibration, given as an offset or as two corners measured on the calibration page
#[derive(Deserialize, ToSchema)]
pub struct CalibrationAsk {
    /// Right, mm
    pub x_mm: Option<f64>,
    /// Down, mm
    pub y_mm: Option<f64>,
    /// Or: how far the first label's marks must move (right, down mm) …
    pub first: Option<(f64, f64)>,
    /// … and the last label's: they must agree within 0.5 mm, else the page scales
    pub last: Option<(f64, f64)>,
    /// The tray it was measured from
    pub tray: Option<String>,
}

/// A printer's offset for a sheet: where its labels land. Given as `x_mm`/`y_mm`, or as the
/// first and last labels' corners from the calibration page (they must agree within 0.5 mm:
/// more means the printer scales the page or the feed skews, which no offset fixes).
#[utoipa::path(put, path = "/api/label-printers/{id}/calibration/{sheet}", tag = "labels",
    params(("id" = String, Path), ("sheet" = String, Path)),
    request_body = CalibrationAsk,
    responses((status = 200, body = PrinterEntry), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)))]
async fn set_calibration(
    State(state): State<AppState>,
    Path((id, sheet)): Path<(String, String)>,
    Json(c): Json<CalibrationAsk>,
) -> Response {
    let p = match printers::get(&state.db, &id).await {
        Ok(Some(p)) => p,
        Ok(None) => return status(StatusCode::NOT_FOUND, format!("no printer {id:?}")),
        Err(e) => return failed(e),
    };
    if let Err(m) = super::labels::sheet_for(&state, Some(&sheet)).await {
        return status(StatusCode::BAD_REQUEST, m);
    }
    let (x, y) = match (c.first, c.last, c.x_mm, c.y_mm) {
        (Some(f), Some(l), _, _) => match printers::from_corners(f, l) {
            Ok(xy) => xy,
            Err(m) => return status(StatusCode::BAD_REQUEST, m),
        },
        (Some(f), None, _, _) => f,
        (None, _, Some(x), Some(y)) => (x, y),
        _ => {
            return status(
                StatusCode::BAD_REQUEST,
                "give x_mm and y_mm, or first (and last) as [right, down]",
            )
        }
    };
    if !(x.abs() <= 20.0 && y.abs() <= 20.0) {
        return status(
            StatusCode::BAD_REQUEST,
            "an offset is at most 20 mm either way",
        );
    }
    let cal = Calibration {
        sheet,
        x_mm: x,
        y_mm: y,
        tray: c.tray.filter(|t| !t.trim().is_empty()),
        updated_at: String::new(),
    };
    if let Err(e) = printers::set_calibration(&state.db, &id, &cal).await {
        return failed(e);
    }
    Json(view(&state, p, None).await).into_response()
}

/// Forget a printer's calibration for a sheet (the sheet's own offset applies again).
#[utoipa::path(delete, path = "/api/label-printers/{id}/calibration/{sheet}", tag = "labels",
    params(("id" = String, Path), ("sheet" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
async fn delete_calibration(
    State(state): State<AppState>,
    Path((id, sheet)): Path<(String, String)>,
) -> Response {
    match printers::delete_calibration(&state.db, &id, &sheet).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => status(
            StatusCode::NOT_FOUND,
            format!("{id} has no calibration for {sheet}"),
        ),
        Err(e) => failed(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CertificateAsk {
    /// The printer's `ipps://` address
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Certificate {
    pub url: String,
    /// The SHA-256 of the certificate the printer shows, hex: compare it with the printer's
    /// own page (its web page or a configuration report) before trusting it
    pub fingerprint: String,
}

/// The certificate an `ipps://` printer shows, for the owner to trust (its SHA-256 goes into
/// the printer's `fingerprint`). Nothing is sent to it.
#[utoipa::path(post, path = "/api/label-printers/certificate", tag = "labels",
    request_body = CertificateAsk,
    responses((status = 200, body = Certificate), (status = 400, body = ErrorBody),
        (status = 502, description = "No TLS with the printer", body = ErrorBody)))]
async fn printer_certificate(Json(c): Json<CertificateAsk>) -> Response {
    if let Err(m) = print::check_url(&c.url) {
        return status(StatusCode::BAD_REQUEST, m);
    }
    match print::tls::fingerprint(c.url.trim()).await {
        Ok(f) => Json(Certificate {
            url: c.url.trim().to_string(),
            fingerprint: f,
        })
        .into_response(),
        Err(e) => status(StatusCode::BAD_GATEWAY, format!("{e:#}")),
    }
}

/// A printer found on the network
#[derive(Serialize, ToSchema, Clone, Debug)]
pub struct Found {
    /// What it calls itself
    pub name: String,
    /// The address to add it with
    pub url: String,
    /// `ty`: its make and model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// It offers ipps too
    pub tls: bool,
    /// It prints colour (`Color=T`)
    pub color: bool,
}

/// What was found and how long it looked
#[derive(Serialize, ToSchema)]
pub struct FoundPrinters {
    pub printers: Vec<Found>,
    /// Why there may be none: mDNS reaches only the server's own network segment
    pub note: String,
}

/// Look for IPP printers on the network for 4 s (mDNS `_ipp._tcp`, `_ipps._tcp`). It finds
/// only printers on the server's own network segment: in Docker that needs host networking
/// (docs/DEPLOY.md); typing the address always works.
#[utoipa::path(get, path = "/api/label-printers/find", tag = "labels",
    responses((status = 200, body = FoundPrinters)))]
async fn find_printers() -> Response {
    let found = tokio::task::spawn_blocking(|| browse(Duration::from_secs(4)))
        .await
        .unwrap_or_default();
    Json(FoundPrinters {
        note: "mDNS finds printers on the server's own network segment only; in Docker that needs host networking. Type the address (ipp://host:631/ipp/print) otherwise.".into(),
        printers: found,
    })
    .into_response()
}

fn browse(how_long: Duration) -> Vec<Found> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let Ok(daemon) = ServiceDaemon::new() else {
        return vec![];
    };
    let mut by_uuid: BTreeMap<String, Found> = BTreeMap::new();
    let deadline = std::time::Instant::now() + how_long;
    let receivers: Vec<_> = ["_ipp._tcp.local.", "_ipps._tcp.local."]
        .iter()
        .filter_map(|t| daemon.browse(t).ok().map(|rx| (*t, rx)))
        .collect();
    while std::time::Instant::now() < deadline {
        for (ty, rx) in &receivers {
            while let Ok(ev) = rx.try_recv() {
                if let ServiceEvent::ServiceResolved(info) = ev {
                    let prop = |k: &str| info.get_property_val_str(k).map(str::to_string);
                    // An IPv4 address: the container may not resolve `.local` names
                    let Some(addr) = info.get_addresses_v4().into_iter().next() else {
                        continue;
                    };
                    let host = addr.to_string();
                    let rp = prop("rp").unwrap_or_else(|| "ipp/print".into());
                    let tls = ty.starts_with("_ipps");
                    let key = prop("UUID").unwrap_or_else(|| info.get_fullname().to_string());
                    let name = info
                        .get_fullname()
                        .split("._ipp")
                        .next()
                        .unwrap_or(info.get_fullname())
                        .replace("\\032", " ");
                    let e = by_uuid.entry(key.clone()).or_insert_with(|| Found {
                        name,
                        url: format!("ipp://{host}:{}/{rp}", info.get_port()),
                        model: prop("ty"),
                        uuid: prop("UUID"),
                        tls,
                        color: prop("Color").as_deref() == Some("T"),
                    });
                    if tls {
                        e.tls = true;
                    } else {
                        e.url = format!("ipp://{host}:{}/{rp}", info.get_port());
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = daemon.shutdown();
    by_uuid.into_values().collect()
}
