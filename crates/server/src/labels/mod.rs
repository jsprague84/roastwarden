//! Bag labels (docs/labels.md): a sheet of labels for a roast, laid out by Typst and written
//! as a PDF to print at actual size, or as an SVG of the first page for the preview.
//!
//! Three parts, each data:
//! - a **sheet** is the stock's geometry (Avery 5163, 6874, 5294, or the owner's own in the
//!   `label_sheets` setting), with the owner's print offsets (`label_offsets`);
//! - a **design** is a Typst file that draws one label, `#let label(l, w, h, round)`: the
//!   built-in `plain` and `gift`, or the owner's in `<data>/labels/designs/<name>.typ`, with
//!   their own fonts in `<data>/labels/fonts/`;
//! - the **driver** (here) places the labels on the sheet: start at label N, the offsets,
//!   as many pages as the copies need.
//!
//! The data reaches Typst as JSON in `sys.inputs.data`; a design never sees the database.

pub mod engine;
pub mod print;
pub mod printers;
pub mod raster;
pub mod retail;
pub mod runs;
pub mod sandbox;
pub mod share;
pub mod store;

use std::collections::HashMap;
use std::path::Path;

use serde_bytes::ByteBuf;
use serde_json::{Value, json};
use sqlx::SqlitePool;

mod designs;
mod facts;
mod notes;
mod sheets;
#[cfg(test)]
mod tests;

pub use designs::{DesignSource, Folder, builtin_design, font_families, slug, valid_id};
use facts::hide_fields;
use sheets::builtin_sheets;

pub use engine::{Diag, Report};
pub use facts::{
    Ask, BlendPart, FIELDS, LotFacts, RestDays, facts, label_data, net_weight, rest_days,
    weight_text,
};
pub use notes::{notes_for, suggest_notes};
pub use sandbox::Renderer;
pub use sheets::{Offset, Sheet, check_offsets, check_sheets, offsets, sheets};

/// The designs that ship: (id, name, source)
const BUILTIN_DESIGNS: [(&str, &str, &str); 6] = [
    (
        "plain",
        "Plain: everything at a glance",
        include_str!("../../assets/labels/plain.typ"),
    ),
    (
        "gift",
        "Gift: serif and script, a frame, a message",
        include_str!("../../assets/labels/gift.typ"),
    ),
    (
        "script",
        "Script: calligraphy and a flourish",
        include_str!("../../assets/labels/script.typ"),
    ),
    (
        "ascii",
        "ASCII: a typewriter ticket with a drawing in characters",
        include_str!("../../assets/labels/ascii.typ"),
    ),
    (
        "cherry",
        "Cherry: a drawn coffee branch",
        include_str!("../../assets/labels/cherry.typ"),
    ),
    (
        "shop",
        "Shop: your brand, and what a bag that is sold needs",
        include_str!("../../assets/labels/shop.typ"),
    ),
];

/// Why labels could not be made.
#[derive(Debug)]
pub enum LabelError {
    /// Unknown sheet or design, or bad numbers: 400
    Bad(String),
    /// The design does not compile (or was stopped by the sandbox): 422, with what Typst said
    Design(String),
}

impl std::fmt::Display for LabelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LabelError::Bad(m) | LabelError::Design(m) => f.write_str(m),
        }
    }
}

/// What to make: a PDF of every page, or an SVG of the first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Output {
    Pdf,
    Svg,
}

/// Another design on the same sheet, with its own parameters, brand and QR code: a print
/// run of several coffees. A label's `group` (1, 2, …) picks it.
#[derive(Debug, Clone)]
pub struct Group {
    pub design: DesignSource,
    pub params: serde_json::Map<String, Value>,
    pub brand: Value,
    pub url: Option<String>,
}

pub struct Job {
    pub sheet: Sheet,
    pub offset: Offset,
    /// The design (none for the calibration page)
    pub design: Option<DesignSource>,
    /// From the sheet's first label: a label, or none for one left blank. A label's `group`
    /// (an index into `more`, from 1) gives it another design; none or 0 is `design`.
    pub slots: Vec<Option<Value>>,
    /// The QR code's target for `design`'s labels (each group has its own)
    pub url: Option<String>,
    /// More designs on the sheet (group 1, 2, …)
    pub more: Vec<Group>,
    /// Draw each label's outline (the preview)
    pub outline: bool,
    /// The design's parameters as given (`art=cherry`); its defaults fill the rest
    pub params: serde_json::Map<String, Value>,
    /// The brand the design reads as `l.brand`
    pub brand: Value,
    /// Fonts beyond the built-in ones
    pub fonts: Vec<Vec<u8>>,
}

impl Job {
    /// A job with no parameters, brand or extra fonts
    pub fn new(sheet: Sheet, offset: Offset, design: Option<DesignSource>) -> Self {
        Self {
            sheet,
            offset,
            design,
            slots: vec![],
            url: None,
            more: vec![],
            outline: false,
            params: serde_json::Map::new(),
            // Every key there, none set: what a design gets without a brand
            brand: store::Brand::value(None),
            fonts: vec![],
        }
    }

    fn request(
        &self,
        kind: engine::Kind,
        output: engine::Output,
    ) -> Result<engine::Request, LabelError> {
        self.sheet.check().map_err(LabelError::Bad)?;
        let mut files = self
            .design
            .as_ref()
            .map(|d| d.files.clone())
            .unwrap_or_default();
        let mut modules = None;
        if let Some(url) = &self.url {
            let (svg, width) = qr_svg(url)?;
            modules = Some(width);
            files.insert("qr.svg".into(), svg);
        }
        let mut more = Vec::with_capacity(self.more.len());
        let mut groups = Vec::with_capacity(self.more.len());
        // Each group's QR code width in modules: a design sizes the code by it
        // (`l.qr_modules`: a short share link needs fewer, smaller squares than a long address)
        let mut widths = vec![modules];
        for (i, g) in self.more.iter().enumerate() {
            let dir = format!("g{}", i + 1);
            for (name, bytes) in &g.design.files {
                files.insert(format!("{dir}/{name}"), bytes.clone());
            }
            let mut width = None;
            if let Some(url) = &g.url {
                let (svg, w) = qr_svg(url)?;
                width = Some(w);
                files.insert(format!("{dir}/qr.svg"), svg);
            }
            widths.push(width);
            more.push(g.design.source.clone());
            groups.push(json!({ "params": g.params, "brand": g.brand }));
        }
        let with_modules = |s: &Option<Value>| {
            s.clone().map(|mut l| {
                let g = l["group"].as_u64().unwrap_or(0) as usize;
                if l.is_object() {
                    l["qr_modules"] = json!(widths.get(g).copied().flatten());
                }
                l
            })
        };
        let slots: Vec<Option<Value>> = match kind {
            engine::Kind::Check => self
                .slots
                .iter()
                .filter(|s| s.is_some())
                .take(1)
                .map(with_modules)
                .collect(),
            _ => self.slots.iter().map(with_modules).collect(),
        };
        Ok(engine::Request {
            kind,
            design: self.design.as_ref().map(|d| d.source.clone()),
            more: if kind == engine::Kind::Sheet {
                more
            } else {
                vec![]
            },
            data: json!({
                "sheet": self.sheet,
                "dx": self.offset.x_mm,
                "dy": self.offset.y_mm,
                "outline": self.outline,
                "slots": slots,
                "params": self.params,
                "brand": self.brand,
                "groups": groups,
                "qr_modules": modules,
            }),
            files,
            fonts: self.fonts.iter().cloned().map(ByteBuf::from).collect(),
            output,
        })
    }
}

/// A QR code to `url` as SVG (no quiet zone: a design keeps its own margin), and its width
/// in modules
fn qr_svg(url: &str) -> Result<(ByteBuf, usize), LabelError> {
    let code = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::M)
        .map_err(|e| LabelError::Bad(format!("the QR code: {e}")))?;
    let width = code.width();
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .quiet_zone(false)
        .build();
    Ok((ByteBuf::from(svg.into_bytes()), width))
}

fn lines(errors: &[Diag]) -> String {
    errors
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lay the job out on its sheet (or the calibration page, with no design) and write it.
pub async fn render(r: &Renderer, job: Job, output: Output) -> Result<Vec<u8>, LabelError> {
    let kind = if job.design.is_some() {
        engine::Kind::Sheet
    } else {
        engine::Kind::Calibration
    };
    let out = match output {
        Output::Pdf => engine::Output::Pdf,
        Output::Svg => engine::Output::Svg,
    };
    match r.render(job.request(kind, out)?).await {
        engine::Reply::Done { bytes, .. } => Ok(bytes.into_vec()),
        engine::Reply::Failed { errors, .. } => Err(LabelError::Design(lines(&errors))),
        engine::Reply::Bad(m) => Err(LabelError::Bad(m)),
    }
}

/// The job's sheets as raster for a printer without PDF (PWG Raster or URF), at `dpi`, sent
/// at `dpi × scale`; grey unless `colour` (then grey anyway for a page without colour).
pub async fn render_raster(
    r: &Renderer,
    job: Job,
    format: raster::Format,
    (dpi, scale): (u32, u32),
    colour: bool,
    media_type: &str,
) -> Result<Vec<u8>, LabelError> {
    let (w, h) = (job.sheet.page_w, job.sheet.page_h);
    let setup = raster::Setup {
        colour: raster::Colour::Grey,
        dpi,
        scale,
        page_pt: ((w * 72.0).round() as u32, (h * 72.0).round() as u32),
        size_name: size_name(w, h),
        media_type: media_type.to_string(),
        quality: 4,
    };
    let out = engine::Output::Raster {
        format,
        setup,
        auto_colour: colour,
    };
    match r.render(job.request(engine::Kind::Sheet, out)?).await {
        engine::Reply::Done { bytes, .. } => Ok(bytes.into_vec()),
        engine::Reply::Failed { errors, .. } => Err(LabelError::Design(lines(&errors))),
        engine::Reply::Bad(m) => Err(LabelError::Bad(m)),
    }
}

/// PWG 5101.1's name for a page `w` × `h` inches
fn size_name(w: f64, h: f64) -> String {
    let near = |a: f64, b: f64| (a - b).abs() < 0.02;
    if near(w, 8.5) && near(h, 11.0) {
        "na_letter_8.5x11in".into()
    } else if near(w, 8.27) && near(h, 11.69) {
        "iso_a4_210x297mm".into()
    } else {
        let t = |v: f64| {
            let s = format!("{v:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        format!("custom_{}x{}in_{}x{}in", t(w), t(h), t(w), t(h))
    }
}

/// What a check found.
pub struct Checked {
    /// Typst's errors: the design does not compile (and there is no report)
    pub errors: Vec<Diag>,
    pub report: Option<Report>,
    /// One label at the asked resolution, its edge dashed
    pub png: Option<Vec<u8>>,
}

/// Lay one label of the job out on its own and report what landed where; a PNG too when
/// `dpi` is given.
pub async fn check(r: &Renderer, job: Job, dpi: Option<u32>) -> Result<Checked, LabelError> {
    if job.design.is_none() {
        return Err(LabelError::Bad("a check needs a design".into()));
    }
    let out = dpi.map_or(engine::Output::Report, |dpi| engine::Output::Png { dpi });
    match r.render(job.request(engine::Kind::Check, out)?).await {
        engine::Reply::Done { bytes, report, .. } => Ok(Checked {
            errors: vec![],
            report: report.map(|b| *b),
            png: dpi.map(|_| bytes.into_vec()),
        }),
        engine::Reply::Failed { errors, .. } => Ok(Checked {
            errors,
            report: None,
            png: None,
        }),
        engine::Reply::Bad(m) => Err(LabelError::Bad(m)),
    }
}

/// Sample data for a label (a check or a test print without a roast), with what was asked.
pub fn sample_data(ask: &Ask) -> (Value, Option<String>) {
    with_ask(engine::sample_label(), ask)
}

/// Sample roast data for a coffee (bean): its name, origin, variety, process and the
/// supplier's notes on the sample's roast, date and weight. None when there is no such bean.
pub async fn bean_sample(db: &SqlitePool, bean_id: &str) -> anyhow::Result<Option<Value>> {
    #[derive(sqlx::FromRow)]
    struct B {
        name: String,
        origin: Option<String>,
        region: Option<String>,
        producer: Option<String>,
        variety: Option<String>,
        process: Option<String>,
        cupping_notes: Option<String>,
        label_notes: Option<String>,
        short_name: Option<String>,
    }
    let Some(b) = sqlx::query_as::<_, B>(
        "SELECT name, origin, region, producer, variety, process, cupping_notes, label_notes, \
         short_name FROM beans WHERE id = ?",
    )
    .bind(bean_id)
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let mut l = engine::sample_label();
    let place = [b.region.as_deref(), b.origin.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    let origin = [place, b.producer.unwrap_or_default()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    l["name"] = json!(
        b.short_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| b.name.clone())
    );
    l["full_name"] = json!(b.name);
    l["origin"] = json!((!origin.is_empty()).then_some(origin));
    l["variety"] = json!(b.variety);
    l["process"] = json!(b.process);
    l["notes"] = json!(notes_for(
        b.label_notes.as_deref(),
        None,
        b.cupping_notes.as_deref()
    ));
    Ok(Some(l))
}

/// A label's data with what the owner asked: the weight, notes, message, hidden fields.
pub fn with_ask(mut l: Value, ask: &Ask) -> (Value, Option<String>) {
    if let Some(g) = ask.weight_g {
        l["weight"] = json!(weight_text(g));
        l["weight_g"] = json!((g * 10.0).round() / 10.0);
        l["net"] = json!(net_weight(g));
    }
    if let Some(n) = ask.notes.as_ref().filter(|n| !n.trim().is_empty()) {
        l["notes"] = json!(n);
    }
    if let Some(m) = ask.message.as_ref().filter(|m| !m.trim().is_empty()) {
        l["message"] = json!(m);
    }
    l["form"] = json!(ask.form.text());
    l["ground"] = json!(ask.form.ground);
    l["grind"] = json!(ask.form.grind);
    hide_fields(&mut l, &ask.hide);
    let url = (l["qr"] == true).then(|| l["url"].as_str().unwrap_or_default().to_string());
    (l, url)
}

/// The built-in designs' manifests (their `design` dictionary: name, description,
/// parameters), from a check of each on sample data, made once per process.
pub async fn builtin_manifests(r: &Renderer) -> &'static HashMap<String, Value> {
    static CACHE: std::sync::OnceLock<HashMap<String, Value>> = std::sync::OnceLock::new();
    if let Some(m) = CACHE.get() {
        return m;
    }
    let mut out = HashMap::new();
    if let Some(sheet) = builtin_sheets().into_iter().find(|s| s.id == "5163") {
        for (id, _, _) in BUILTIN_DESIGNS {
            let mut job = Job::new(sheet.clone(), Offset::default(), builtin_design(id));
            let (data, url) = sample_data(&Ask::default());
            job.slots = slots(data, 1, 1);
            job.url = url;
            if let Ok(Checked {
                report: Some(report),
                ..
            }) = check(r, job, None).await
                && let Some(m) = report.manifest
            {
                out.insert(id.to_string(), m);
            }
        }
    }
    // Two first calls at once both make it; the first one kept wins
    let _ = CACHE.set(out);
    CACHE.get().expect("just set")
}

/// `count` labels from `data`, after `start - 1` left blank (a part-used sheet).
pub fn slots(data: Value, count: usize, start: usize) -> Vec<Option<Value>> {
    let mut v: Vec<Option<Value>> = vec![None; start.saturating_sub(1)];
    v.extend(std::iter::repeat_n(Some(data), count));
    v
}

/// The labels folder for a database path: `<dir of the database>/labels`.
pub fn folder_for(db_path: &Path) -> Folder {
    Folder(
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("labels"),
    )
}
