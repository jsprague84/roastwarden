//! Typst for labels: one [`Request`] in, one [`Reply`] out, and nothing else. No database, no
//! files, no network, no clock: everything a render needs comes in the request, so it can
//! run in the worker process (`sandbox.rs`) as well as in the server.
//!
//! Four drivers place a design: on a sheet ([`Kind::Sheet`], the PDF and the preview), the
//! calibration page, one label on a page of its own size ([`Kind::Check`]), which is laid
//! out without clipping so the [`Report`] can see what runs past the edge, and a report
//! ([`Kind::Report`], `docs/reports.md`): a whole document from a report design and its data.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{json, Value};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Dict, IntoValue, Label};
use typst::introspection::{Introspector, MetadataElem};
use typst::layout::{Frame, FrameItem, Point, Rect, Transform};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::{LazyHash, PicoStr};
use typst::visualize::ImageKind;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_layout::PagedDocument;
use utoipa::ToSchema;

/// The fonts every design can use. Each is under the SIL Open Font License (assets/fonts/OFL-*).
const BUILTIN_FONTS: [&[u8]; 11] = [
    include_bytes!("../../assets/fonts/AtkinsonHyperlegibleNext-Regular.ttf"),
    include_bytes!("../../assets/fonts/AtkinsonHyperlegibleNext-Bold.ttf"),
    include_bytes!("../../assets/fonts/AtkinsonHyperlegibleNext-Italic.ttf"),
    include_bytes!("../../assets/fonts/DMSerifDisplay-Regular.ttf"),
    include_bytes!("../../assets/fonts/DMSerifDisplay-Italic.ttf"),
    include_bytes!("../../assets/fonts/CormorantGaramond-Regular.ttf"),
    include_bytes!("../../assets/fonts/CormorantGaramond-Italic.ttf"),
    include_bytes!("../../assets/fonts/CormorantGaramond-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/PinyonScript-Regular.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMonoNL-Regular.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMonoNL-Bold.ttf"),
];

/// Helpers a design may import: `#import "roastwarden.typ": fit, clamp`
pub const HELPERS: &str = include_str!("../../assets/labels/roastwarden.typ");

/// Helpers a report design imports: `#import "report.typ": page-frame, rtable, roast-chart`
pub const REPORT_HELPERS: &str = include_str!("../../assets/reports/report.typ");

pub fn builtin_fonts() -> &'static [Font] {
    static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
    FONTS.get_or_init(|| {
        BUILTIN_FONTS
            .iter()
            .flat_map(|b| Font::iter(Bytes::new(*b)))
            .collect()
    })
}

/// What to lay out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    /// Labels on a sheet: `data` has `sheet`, `dx`, `dy`, `outline`, `slots`
    Sheet,
    /// The calibration page: `data` has `sheet`, `dx`, `dy`
    Calibration,
    /// One label on a page of its size, unclipped: `data` has `sheet` and one `slots` entry
    Check,
    /// A report: the design's `report(data)` makes the whole document; `data.meta` has the
    /// title, subject and the time it was made (`made_iso`), as the sandbox has no clock
    Report,
}

/// What to make of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Output {
    /// Every page
    Pdf,
    /// The first page
    Svg,
    /// The first page as PNG, at this many dots per inch
    Png { dpi: u32 },
    /// Nothing but the report (a check without a picture)
    Report,
    /// Every page in one SVG, one under the other (a report's preview)
    SvgPages,
    /// One page (from 1) as PNG at this many dots per inch, without a label's outline
    PagePng { page: usize, dpi: u32 },
    /// Every page as raster for a printer without PDF: PWG Raster or Apple URF, rendered at
    /// `setup.dpi`; with `auto_colour` a page is grey unless it has colour on it
    Raster {
        format: super::raster::Format,
        setup: super::raster::Setup,
        auto_colour: bool,
    },
}

/// Everything one render needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub kind: Kind,
    /// The design's Typst source (none for the calibration page)
    pub design: Option<String>,
    /// More designs on the same sheet (a print run of several coffees): group 1, 2, … each
    /// in its own folder (`g1/design.typ`, its `g1/qr.svg`); a label's `group` picks one
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<String>,
    /// `sys.inputs.data`, as JSON
    pub data: Value,
    /// Files a design reads by name: `qr.svg`, images, its assets
    pub files: BTreeMap<String, ByteBuf>,
    /// Fonts beyond the built-in ones
    pub fonts: Vec<ByteBuf>,
    pub output: Output,
}

/// One error or warning from Typst, with where it is.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Diag {
    /// `design.typ`, `roastwarden.typ`, or the driver's `main.typ`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// Where in the file, as offsets in UTF-16 code units (what a browser editor counts)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<usize>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let (Some(file), Some(line)) = (&self.file, self.line) {
            write!(f, "{file}:{line}: ")?;
        }
        f.write_str(&self.message)?;
        for h in &self.hints {
            write!(f, " (hint: {h})")?;
        }
        Ok(())
    }
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Reply {
    /// The PDF, SVG or PNG (empty for [`Output::Report`]), and for a check the report
    Done {
        bytes: ByteBuf,
        report: Option<Box<Report>>,
        /// Pages laid out
        #[serde(default)]
        pages: usize,
    },
    /// Typst could not compile it
    Failed {
        errors: Vec<Diag>,
        warnings: Vec<Diag>,
    },
    /// Not something to compile (bad numbers, no design)
    Bad(String),
}

// ---- the world: the driver, the design, the helpers and the request's files ----

struct Sandbox {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    sources: HashMap<String, Source>,
    files: BTreeMap<String, ByteBuf>,
}

fn file_id(path: &str) -> FileId {
    FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new(path).expect("a path of ours"),
    ))
}

impl World for Sandbox {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            return Ok(self.main.clone());
        }
        let path = id.vpath().get_without_slash();
        if let Some(s) = self.sources.get(path) {
            return Ok(s.clone());
        }
        // `g1/roastwarden.typ`: the helpers, as at the root (with this file's id)
        if let Some(s) = group_rest(path).and_then(|rest| self.sources.get(rest)) {
            return Ok(Source::new(id, s.text().to_string()));
        }
        let text =
            String::from_utf8(self.file(id)?.to_vec()).map_err(|_| FileError::InvalidUtf8)?;
        Ok(Source::new(id, text))
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = id.vpath().get_without_slash();
        if id.root() != &VirtualRoot::Project {
            // A package (`@preview/…`): none are reachable, there is no network
            return Err(FileError::Other(Some(
                "packages are not available to label designs (no network); draw it with Typst's own shapes or an asset".into(),
            )));
        }
        for p in [Some(path), group_rest(path)].into_iter().flatten() {
            if let Some(b) = self.files.get(p) {
                return Ok(Bytes::new(b.to_vec()));
            }
            if let Some(s) = self.sources.get(p) {
                return Ok(Bytes::from_string(s.text().to_string()));
            }
        }
        Err(FileError::NotFound(path.into()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

/// `g3/logo.svg` → `logo.svg`: what a group's folder does not hold comes from the root.
fn group_rest(path: &str) -> Option<&str> {
    let (dir, rest) = path.split_once('/')?;
    let n = dir.strip_prefix('g')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(rest)
}

/// What every driver starts with: the data, and the design's manifest and parameters. A
/// design may define `#let design = (name: …, params: (art: (default: "cherry", …), …))`;
/// each parameter is its default, or a dictionary with a `default`. Values given as text
/// (`--set size=12`) become the default's type.
const PRELUDE: &str = r#"
#import "design.typ" as d
#let data = json(bytes(sys.inputs.data))
#let manifest = dictionary(d).at("design", default: none)
#let specs = if type(manifest) == dictionary { manifest.at("params", default: (:)) } else { (:) }
#let default-of(spec) = if type(spec) == dictionary { spec.at("default", default: none) } else { spec }
#let typed(def, v) = if type(v) != str { v } else if type(def) == int { int(v) } else if type(def) == float { float(v) } else if type(def) == bool { v in ("true", "yes", "1", "on") } else { v }
#let params = {
  let p = (:)
  for (k, spec) in specs { p.insert(k, default-of(spec)) }
  for (k, v) in data.at("params", default: (:)) { p.insert(k, typed(default-of(specs.at(k, default: none)), v)) }
  p
}
#let with-extras(l) = if l == none { none } else { l + (params: params, brand: data.at("brand", default: (:))) }
"#;

/// The sheet driver's groups: group 0 is `design.typ` with `data.params` and `data.brand`;
/// group N (a print run of several coffees) is `gN/design.typ` with `data.groups.at(N - 1)`.
fn groups_prelude(more: usize) -> String {
    let mut out = String::from(
        "#let params-for(m, given) = {\n  let man = dictionary(m).at(\"design\", default: none)\n  let sp = if type(man) == dictionary { man.at(\"params\", default: (:)) } else { (:) }\n  let p = (:)\n  for (k, spec) in sp { p.insert(k, default-of(spec)) }\n  for (k, v) in given { p.insert(k, typed(default-of(sp.at(k, default: none)), v)) }\n  p\n}\n#let groups = ((label: d.label, params: params, brand: data.at(\"brand\", default: (:))),)\n",
    );
    for n in 1..=more {
        let _ = write!(
            out,
            "#import \"g{n}/design.typ\" as d{n}\n#let groups = groups + ((label: d{n}.label, params: params-for(d{n}, data.groups.at({i}).at(\"params\", default: (:))), brand: data.groups.at({i}).at(\"brand\", default: (:))),)\n",
            i = n - 1
        );
    }
    out
}

/// Places `label()` from design.typ on the sheet. `data.slots` has one entry a label position
/// from the first page's first label: a label's data, or none (left blank). A label's
/// `group` picks its design (a print run of several coffees; 0, design.typ, when absent).
const SHEET: &str = r#"
#let s = data.sheet
#set page(width: s.page_w * 1in, height: s.page_h * 1in, margin: 0pt)
#let per = s.cols * s.rows
#let at(i) = (
  (s.left + calc.rem(i, s.cols) * s.pitch_x) * 1in + data.dx * 1mm,
  (s.top + calc.quo(i, s.cols) * s.pitch_y) * 1in + data.dy * 1mm,
)
// Every position of every page, so the preview outlines the whole sheet
#for p in range(calc.max(1, calc.ceil(data.slots.len() / per))) {
  if p > 0 { pagebreak() }
  for i in range(per) {
    let l = data.slots.at(p * per + i, default: none)
    let (x, y) = at(i)
    if data.outline {
      place(top + left, dx: x, dy: y,
        box(width: s.w * 1in, height: s.h * 1in, radius: if s.round { 50% } else { 0.06in },
          stroke: 0.8pt + luma(175)))
    }
    if l != none {
      let g = groups.at(l.at("group", default: 0))
      place(top + left, dx: x, dy: y,
        box(width: s.w * 1in, height: s.h * 1in, clip: true,
          radius: if s.round { 50% } else { 0pt },
          (g.label)(l + (params: g.params, brand: g.brand), s.w * 1in, s.h * 1in, s.round)))
    }
  }
}
"#;

/// One label on a page of its own size, not clipped, with the manifest and the parameters
/// as metadata for the report.
const CHECK: &str = r"
#let s = data.sheet
#let (w, h) = (s.w * 1in, s.h * 1in)
#set page(width: w, height: h, margin: 0pt)
#metadata(manifest) <rr-manifest>
#metadata(params) <rr-params>
#place(top + left, box(width: w, height: h, (d.label)(with-extras(data.slots.at(0)), w, h, s.round)))
";

/// A report: the document's metadata from the data, then the design's `report(data)`.
const REPORT: &str = r#"
#import "design.typ" as d
#let data = json(bytes(sys.inputs.data))
#let made = data.meta.made_iso
#set document(
  title: data.meta.title + " · " + data.meta.subject,
  author: "Roastwarden",
  date: datetime(year: made.at(0), month: made.at(1), day: made.at(2), hour: made.at(3), minute: made.at(4), second: made.at(5)),
)
#(d.report)(data)
"#;

/// Marks a few mm inside each label (an L in each corner; for round labels a circle and its
/// centre) and a mm scale across each edge to read how far the print is off, after the
/// offsets, and a 100 mm rule to check the printer did not scale. Nothing prints within
/// 4.5 mm of the page's edge (a laser's margin is about 4.2 mm): a scale that would is left
/// out, and the label's other scales read the same offset.
const CALIBRATION: &str = r#"
#let data = json(bytes(sys.inputs.data))
#let s = data.sheet
#let (pw, ph) = (s.page_w * 1in, s.page_h * 1in)
#set page(width: pw, height: ph, margin: 0pt)
#set text(font: "Atkinson Hyperlegible Next", size: 7.5pt)
#let safe = 4.5mm
#let inset = 3mm
#let arm = 5mm
#let reach = 3mm
#let stroke = 0.4pt
#let line-at(x, y, dx, dy, width: stroke) = place(top + left, dx: x, dy: y, line(end: (dx, dy), stroke: width))
#let inside(x, y) = x >= safe and x <= pw - safe and y >= safe and y <= ph - safe
// A scale across an edge at (x, y): `reach` either side, a tick each mm, the long one where
// the edge should be; `across` is true for a vertical edge (the scale runs left to right)
#let scale(x, y, across) = {
  let (x0, y0, x1, y1) = if across { (x - reach, y, x + reach, y) } else { (x, y - reach, x, y + reach) }
  if inside(x0, y0) and inside(x1, y1) {
    line-at(x0, y0, x1 - x0, y1 - y0)
    for i in range(-3, 4) {
      let t = if i == 0 { 2.4mm } else { 1mm }
      let w = if i == 0 { 0.6pt } else { stroke }
      if across { line-at(x + i * 1mm, y - t / 2, 0pt, t, width: w) }
      else { line-at(x - t / 2, y + i * 1mm, t, 0pt, width: w) }
    }
  }
}
#let (gx, gy) = ((s.pitch_x - s.w) * 1in, (s.pitch_y - s.h) * 1in)
#for i in range(s.cols * s.rows) {
  let (col, row) = (calc.rem(i, s.cols), calc.quo(i, s.cols))
  let x = (s.left + col * s.pitch_x) * 1in + data.dx * 1mm
  let y = (s.top + row * s.pitch_y) * 1in + data.dy * 1mm
  let (w, h) = (s.w * 1in, s.h * 1in)
  if s.round {
    let (cx, cy) = (x + w / 2, y + h / 2)
    place(top + left, dx: x + inset, dy: y + inset, circle(radius: w / 2 - inset, stroke: stroke))
    // The centre (not under the first label's words)
    if i > 0 {
      line-at(cx - 3mm, cy, 6mm, 0pt)
      line-at(cx, cy - 3mm, 0pt, 6mm)
    }
    // A scale where the circle meets its centre lines; between labels closer than two
    // scales, only the outer rows' and columns'
    if col == 0 or gx >= 2 * reach { scale(x, cy, true) }
    if col == s.cols - 1 or gx >= 2 * reach { scale(x + w, cy, true) }
    if row == 0 or gy >= 2 * reach { scale(cx, y, false) }
    if row == s.rows - 1 or gy >= 2 * reach { scale(cx, y + h, false) }
  } else {
    for (cx, cy, sx, sy) in ((x, y, 1, 1), (x + w, y, -1, 1), (x, y + h, 1, -1), (x + w, y + h, -1, -1)) {
      let (ix, iy) = (cx + sx * inset, cy + sy * inset)
      line-at(ix, iy, sx * arm, 0pt)
      line-at(ix, iy, 0pt, sy * arm)
    }
    // One scale on each edge, each at its own distance from the corner so a neighbour's
    // never lands on it
    scale(x, y + 12mm, true)
    scale(x + w, y + 18mm, true)
    scale(x + 12mm, y, false)
    scale(x + 18mm, y + h, false)
  }
  place(top + left, dx: x + 0.25in, dy: y + h / 2 - 0.3in, text(fill: luma(120))[#(i + 1)])
}
// What to do, inside the first label (plain paper: it prints anywhere inside)
#let (x1, y1) = (s.left * 1in + data.dx * 1mm, s.top * 1in + data.dy * 1mm)
#let tw = if s.round { s.w * 1in * 0.62 } else { s.w * 1in - 2 * (inset + arm) - 0.4in }
#place(top + left, dx: x1 + (s.w * 1in - tw) / 2, dy: y1, box(width: tw, height: s.h * 1in,
  align(center + horizon, text(size: if s.round { 6pt } else { 6.5pt })[
  *Calibration: #s.name.* Print at *Actual size* (not "Fit to page") on plain paper and
  hold it over a sheet of labels against the light. The marks are #(inset / 1mm) mm inside
  each label; where a label's edge crosses a scale (a tick each mm, the long tick where the
  edge should be) is how far to move the marks, right and down plus (now #data.dx mm right,
  #data.dy mm down).
])))
#place(bottom + left, dx: 0.5in, dy: -5mm, stack(dir: ltr, spacing: 3mm,
  move(dy: -1mm, line(length: 100mm, stroke: 0.6pt)),
  text(fill: luma(100))[100 mm: if this line is not 100 mm, the printer scaled the page]))
"#;

/// Lay the request out and make its output. Blocking: run it off the async runtime.
pub fn execute(req: &Request) -> Reply {
    let world = match sandbox(req) {
        Ok(w) => w,
        Err(reply) => return reply,
    };
    let compiled = typst::compile::<PagedDocument>(&world);
    let warnings = diags(&world, &compiled.warnings);
    let reply = match compiled.output {
        Err(errors) => Reply::Failed {
            errors: diags(&world, &errors),
            warnings,
        },
        Ok(doc) => made(&world, req, &doc, warnings),
    };
    // Typst keeps every memoised result; keep only what the last renders used
    comemo::evict(10);
    reply
}

/// One page of a document as text: its size and each run of text as Typst set it, with its
/// size, baseline and ink box, in pt from the page's top left (the report check,
/// `reports::check`).
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct PageText {
    pub w: f64,
    pub h: f64,
    /// (text, size pt, baseline y, [x0, y0, x1, y1]), in the order set
    pub runs: Vec<(String, f64, f64, [f64; 4])>,
}

/// Lay a request out and read every page's text runs (no output made). In process: for
/// tests and checks of the built-in designs, not for untrusted ones.
#[cfg(test)]
pub fn page_text(req: &Request) -> Result<Vec<PageText>, String> {
    let world = sandbox(req).map_err(|r| format!("{r:?}"))?;
    let compiled = typst::compile::<PagedDocument>(&world);
    let doc = compiled.output.map_err(|e| {
        diags(&world, &e)
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(doc
        .pages()
        .iter()
        .map(|p| {
            let mut seen = Vec::new();
            walk(&p.frame, Transform::identity(), None, None, &mut seen);
            PageText {
                w: p.frame.width().to_pt(),
                h: p.frame.height().to_pt(),
                runs: seen
                    .iter()
                    .filter_map(|s| match s {
                        Seen::Text {
                            text,
                            pt,
                            base,
                            ink,
                            ..
                        } => Some((text.clone(), *pt, base.1, [ink.x0, ink.y0, ink.x1, ink.y1])),
                        _ => None,
                    })
                    .collect(),
            }
        })
        .collect())
}

/// The sandboxed world a request is laid out in: its designs, the helpers, the data and fonts.
fn sandbox(req: &Request) -> Result<Sandbox, Reply> {
    let main_text = match (req.kind, &req.design) {
        (Kind::Calibration, _) => CALIBRATION.to_string(),
        (Kind::Sheet, Some(_)) => format!("{PRELUDE}{}{SHEET}", groups_prelude(req.more.len())),
        (Kind::Check, Some(_)) => format!("{PRELUDE}{CHECK}"),
        (Kind::Report, Some(_)) => REPORT.to_string(),
        (_, None) => return Err(Reply::Bad("no design to lay out".into())),
    };
    let mut sources = HashMap::new();
    if let Some(src) = &req.design {
        sources.insert(
            "design.typ".to_string(),
            Source::new(file_id("design.typ"), src.clone()),
        );
    }
    for (i, src) in req.more.iter().enumerate() {
        let path = format!("g{}/design.typ", i + 1);
        sources.insert(path.clone(), Source::new(file_id(&path), src.clone()));
    }
    sources.insert(
        "roastwarden.typ".to_string(),
        Source::new(file_id("roastwarden.typ"), HELPERS.to_string()),
    );
    if req.kind == Kind::Report {
        sources.insert(
            "report.typ".to_string(),
            Source::new(file_id("report.typ"), REPORT_HELPERS.to_string()),
        );
    }
    let mut inputs = Dict::new();
    inputs.insert("data".into(), req.data.to_string().into_value());
    let mut fonts: Vec<Font> = builtin_fonts().to_vec();
    fonts.extend(
        req.fonts
            .iter()
            .flat_map(|b| Font::iter(Bytes::new(b.to_vec())).collect::<Vec<_>>()),
    );
    Ok(Sandbox {
        library: LazyHash::new(Library::builder().with_inputs(inputs).build()),
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
        main: Source::new(file_id("main.typ"), main_text),
        sources,
        files: req.files.clone(),
    })
}

fn made(world: &Sandbox, req: &Request, doc: &PagedDocument, warnings: Vec<Diag>) -> Reply {
    let Some(page) = doc.pages().first() else {
        return Reply::Bad("the design made no page".into());
    };
    let pdf_options = typst_pdf::PdfOptions {
        creator: typst::foundations::Smart::Custom(Some(format!(
            "Roastwarden {} (Typst)",
            env!("CARGO_PKG_VERSION")
        ))),
        ..typst_pdf::PdfOptions::default()
    };
    let bytes = match &req.output {
        Output::Pdf => match typst_pdf::pdf(doc, &pdf_options) {
            Ok(b) => b,
            Err(e) => {
                return Reply::Failed {
                    errors: diags(world, &e),
                    warnings,
                }
            }
        },
        Output::Svg => typst_svg::svg(page, &typst_svg::SvgOptions::default()).into_bytes(),
        Output::Png { dpi } => match png(page, &req.data, *dpi) {
            Ok(b) => b,
            Err(m) => return Reply::Bad(m),
        },
        Output::Report => Vec::new(),
        Output::SvgPages => typst_svg::svg_merged(
            doc,
            &typst_svg::SvgOptions::default(),
            typst::layout::Abs::pt(12.0),
        )
        .into_bytes(),
        Output::PagePng { page, dpi } => {
            let Some(p) = doc.pages().get(page.saturating_sub(1)) else {
                return Reply::Bad(format!(
                    "page {page}: the document has {} page(s)",
                    doc.pages().len()
                ));
            };
            let opts = typst_render::RenderOptions {
                pixel_per_pt: typst::utils::Scalar::new(f64::from((*dpi).clamp(36, 200)) / 72.0),
                render_bleed: false,
            };
            match typst_render::render(p, &opts).encode_png() {
                Ok(b) => b,
                Err(e) => return Reply::Bad(format!("the PNG: {e}")),
            }
        }
        Output::Raster {
            format,
            setup,
            auto_colour,
        } => raster_doc(doc, *format, setup, *auto_colour),
    };
    // A check's report; a sheet's (its first page) when only a report is asked for
    let report = (req.kind == Kind::Check || req.output == Output::Report)
        .then(|| Box::new(report(doc, req, warnings)));
    Reply::Done {
        bytes: ByteBuf::from(bytes),
        report,
        pages: doc.pages().len(),
    }
}

/// Typst's diagnostics with file and line.
fn diags(world: &Sandbox, list: &[SourceDiagnostic]) -> Vec<Diag> {
    list.iter()
        .map(|d| {
            let place = world
                .range(d.span)
                .zip(d.span.id())
                .and_then(|(range, id)| {
                    let src = world.source(id).ok()?;
                    let lines = src.lines();
                    let line = lines.byte_to_line(range.start)?;
                    let column = lines.byte_to_column(range.start)?;
                    let utf16 = (
                        lines.byte_to_utf16(range.start),
                        lines.byte_to_utf16(range.end),
                    );
                    Some((
                        id.vpath().get_without_slash().to_string(),
                        line,
                        column,
                        utf16,
                    ))
                });
            Diag {
                file: place.as_ref().map(|p| p.0.clone()),
                line: place.as_ref().map(|p| p.1 + 1),
                column: place.as_ref().map(|p| p.2 + 1),
                from: place.as_ref().and_then(|p| p.3 .0),
                to: place.as_ref().and_then(|p| p.3 .1),
                message: d.message.to_string(),
                hints: d.hints.iter().map(|h| h.v.to_string()).collect(),
            }
        })
        .collect()
}

// ---- the PNG ----

/// The page at `dpi` with the label's outline (its edge, a circle for a round one) drawn in.
fn png(page: &typst_layout::Page, data: &Value, dpi: u32) -> Result<Vec<u8>, String> {
    let dpi = dpi.clamp(36, 200);
    let opts = typst_render::RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(f64::from(dpi) / 72.0),
        render_bleed: false,
    };
    let mut pix = typst_render::render(page, &opts);
    let round = data["sheet"]["round"].as_bool().unwrap_or(false);
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    let path = if round {
        tiny_skia::PathBuilder::from_circle(w / 2.0, h / 2.0, w / 2.0 - 0.5)
    } else {
        tiny_skia::Rect::from_xywh(0.5, 0.5, w - 1.0, h - 1.0)
            .map(tiny_skia::PathBuilder::from_rect)
    };
    if let Some(path) = path {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(170, 170, 170, 255);
        let stroke = tiny_skia::Stroke {
            width: 1.0,
            dash: tiny_skia::StrokeDash::new(vec![4.0, 3.0], 0.0),
            ..tiny_skia::Stroke::default()
        };
        pix.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    pix.encode_png().map_err(|e| format!("the PNG: {e}"))
}

/// Every page rendered at `setup.dpi`, flattened onto white paper, and coded as raster one
/// page at a time.
fn raster_doc(
    doc: &PagedDocument,
    format: super::raster::Format,
    setup: &super::raster::Setup,
    auto_colour: bool,
) -> Vec<u8> {
    use super::raster::{add_page, begin, Colour, Page};
    let n = doc.pages().len() as u32;
    let mut out = begin(format, n);
    let opts = typst_render::RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(f64::from(setup.dpi) / 72.0),
        render_bleed: false,
    };
    for page in doc.pages() {
        let pix = typst_render::render(page, &opts);
        let (w, h) = (pix.width(), pix.height());
        // Premultiplied RGBA on white: c + (255 − a)
        let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
        let mut coloured = false;
        for px in pix.data().as_chunks::<4>().0 {
            let white = 255 - px[3];
            let (r, g, b) = (
                px[0].saturating_add(white),
                px[1].saturating_add(white),
                px[2].saturating_add(white),
            );
            coloured |= r.abs_diff(g) > 8 || g.abs_diff(b) > 8 || r.abs_diff(b) > 8;
            rgb.extend_from_slice(&[r, g, b]);
        }
        drop(pix);
        let colour = if auto_colour && coloured {
            Colour::Rgb
        } else if auto_colour {
            Colour::Grey
        } else {
            setup.colour
        };
        let pixels = match colour {
            Colour::Rgb => rgb,
            Colour::Grey => rgb
                .as_chunks::<3>()
                .0
                .iter()
                .map(|c| {
                    let y = 0.2126 * f32::from(c[0])
                        + 0.7152 * f32::from(c[1])
                        + 0.0722 * f32::from(c[2]);
                    y.round().clamp(0.0, 255.0) as u8
                })
                .collect(),
        };
        let mut s = setup.clone();
        s.colour = colour;
        add_page(
            format,
            &mut out,
            &Page {
                width: w,
                height: h,
                pixels: &pixels,
            },
            &s,
            n,
        );
    }
    out
}

// ---- the layout report ----

/// What a check found: the label's lines of text and pictures where they landed, and what
/// to fix. Lengths are mm from the label's top left corner, sizes pt.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Report {
    /// True when `issues` has nothing to fix: none, or only the soft ones (`dropped`, `hint`)
    pub ok: bool,
    /// The label's width and height, mm
    pub w: f64,
    pub h: f64,
    /// The height nothing printed in, above the first line or image and below the last
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free: Option<Free>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub round: bool,
    /// What to fix, worst first
    pub issues: Vec<Issue>,
    /// Each line of text: what it says, its font and size, and its ink box
    pub lines: Vec<TextLine>,
    /// Images (the QR code, artwork) and their boxes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageBox>,
    /// The label's fields that have a value: printed (found in the text) or not
    pub fields: FieldsSeen,
    /// The design's `design` dictionary (name, description, params), if it has one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Value>,
    /// The parameters the design got: its defaults with what was given
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// What `fit`, `wrap` and `clamp` did: text they shrank, wrapped or cut
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Value>,
}

/// Issues that are worth a look but leave the label `ok`
pub const SOFT: [&str; 2] = ["dropped", "hint"];

/// The label's height that nothing printed in, mm
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Free {
    pub top_mm: f64,
    pub bottom_mm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Issue {
    /// `overflow`, `near_edge`, `overlap`, `too_long` (a `fit` or `wrap` at its minimum
    /// size and still too long), `tiny_text`, `qr_small`, `qr_crowded`, `cut` (a `clamp`
    /// dropped words; with the label's free height when there was room), `typst` (a
    /// warning: e.g. an unknown font, which falls back to another), `empty`, `param` (a
    /// parameter the design does not declare); soft, the label still `ok`: `dropped` (the
    /// name, roast date or best-from date not printed) and `hint` (a design's own note,
    /// `hint()` in `roastwarden.typ`)
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub font: String,
    pub pt: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Set in a bold (or heavier) face
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// Its lowercase letters' height (the "o"), mm: what US labelling rules measure in mixed
    /// case
    #[serde(default)]
    pub x_mm: f64,
    /// Its capitals' height, mm
    #[serde(default)]
    pub cap_mm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ImageBox {
    /// `qr`, `svg` or `raster`
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// A QR code's module (one square) size, mm: 0.33 (13 mil) or more scans from a phone
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct FieldsSeen {
    pub shown: Vec<String>,
    pub missing: Vec<String>,
}

/// A box in label coordinates (pt), from a frame item.
#[derive(Debug, Clone, Copy)]
struct Bx {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Bx {
    fn of(r: Rect, ts: Transform) -> Option<Self> {
        let pts = [
            r.min,
            r.max,
            Point::new(r.min.x, r.max.y),
            Point::new(r.max.x, r.min.y),
        ]
        .map(|p| p.transform(ts));
        let xs = pts.map(|p| p.x.to_pt());
        let ys = pts.map(|p| p.y.to_pt());
        let b = Bx {
            x0: xs.iter().copied().fold(f64::INFINITY, f64::min),
            y0: ys.iter().copied().fold(f64::INFINITY, f64::min),
            x1: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            y1: ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        };
        b.is_finite().then_some(b)
    }
    fn is_finite(&self) -> bool {
        [self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|v| v.is_finite())
    }
    fn clip(self, c: Option<Bx>) -> Option<Self> {
        let Some(c) = c else { return Some(self) };
        let b = Bx {
            x0: self.x0.max(c.x0),
            y0: self.y0.max(c.y0),
            x1: self.x1.min(c.x1),
            y1: self.y1.min(c.y1),
        };
        (b.x0 < b.x1 && b.y0 < b.y1).then_some(b)
    }
    fn union(self, o: Bx) -> Bx {
        Bx {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
    fn grow(self, by: f64) -> Bx {
        Bx {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }
    /// How far the boxes overlap on each axis (negative: apart)
    fn overlap(&self, o: &Bx) -> (f64, f64) {
        (
            self.x1.min(o.x1) - self.x0.max(o.x0),
            self.y1.min(o.y1) - self.y0.max(o.y0),
        )
    }
}

enum Seen {
    Text {
        text: String,
        font: String,
        pt: f64,
        /// The baseline's start, label coordinates
        base: (f64, f64),
        ink: Bx,
        style: Style,
    },
    Image {
        qr: bool,
        svg: bool,
        b: Bx,
    },
    Shape(Bx),
}

fn walk(frame: &Frame, ts: Transform, clip: Option<Bx>, qr: Option<&[u8]>, out: &mut Vec<Seen>) {
    for (pos, item) in frame.items() {
        let here = ts.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(g) => {
                let inner = here.pre_concat(g.transform);
                let c = match &g.clip {
                    Some(curve) => match Bx::of(curve.bbox(None), inner) {
                        Some(b) => Some(b.clip(clip).unwrap_or(Bx {
                            x0: 0.0,
                            y0: 0.0,
                            x1: 0.0,
                            y1: 0.0,
                        })),
                        None => clip,
                    },
                    None => clip,
                };
                walk(&g.frame, inner, c, qr, out);
            }
            FrameItem::Text(t) => {
                let r = t.bbox();
                // Text runs are y-up: the corners come back with y flipped
                let r = Rect::new(
                    Point::new(r.min.x.min(r.max.x), r.min.y.min(r.max.y)),
                    Point::new(r.min.x.max(r.max.x), r.min.y.max(r.max.y)),
                );
                let Some(ink) = Bx::of(r, here).and_then(|b| b.clip(clip)) else {
                    continue;
                };
                let base = Point::zero().transform(here);
                let face = t.font.font();
                let metrics = t.font.metrics();
                out.push(Seen::Text {
                    text: t.text.to_string(),
                    font: face.info().family.clone(),
                    pt: t.size.to_pt(),
                    base: (base.x.to_pt(), base.y.to_pt()),
                    ink,
                    style: Style {
                        bold: face.info().variant.weight.to_number() >= 600,
                        x_em: metrics.x_height.get(),
                        cap_em: metrics.cap_height.get(),
                    },
                });
            }
            FrameItem::Image(img, size, _) => {
                let r = Rect::new(Point::zero(), Point::new(size.x, size.y));
                let Some(b) = Bx::of(r, here).and_then(|b| b.clip(clip)) else {
                    continue;
                };
                let (is_svg, is_qr) = match img.kind() {
                    ImageKind::Svg(s) => (true, qr.is_some_and(|q| s.data().as_slice() == q)),
                    _ => (false, false),
                };
                out.push(Seen::Image {
                    qr: is_qr,
                    svg: is_svg,
                    b,
                });
            }
            FrameItem::Shape(s, _) => {
                if let Some(b) = Bx::of(s.bbox(true), here).and_then(|b| b.clip(clip)) {
                    out.push(Seen::Shape(b));
                }
            }
            FrameItem::Link(..) | FrameItem::Tag(_) => {}
        }
    }
}

fn mm(pt: f64) -> f64 {
    (pt * 25.4 / 72.0 * 10.0).round() / 10.0
}

/// A face's weight and letter heights (em)
#[derive(Debug, Clone, Copy, Default)]
struct Style {
    bold: bool,
    x_em: f64,
    cap_em: f64,
}

/// A line of text: what it says, its font, its size (pt), its ink box, and its largest
/// piece's face
type Line = (String, String, f64, Bx, Style);

/// Pieces of text on one baseline, close together, as one line.
fn lines_of(seen: &[Seen]) -> Vec<Line> {
    // Each with its baseline's start
    let mut lines: Vec<(Line, (f64, f64))> = Vec::new();
    for s in seen {
        let Seen::Text {
            text,
            font,
            pt,
            base,
            ink,
            style,
        } = s
        else {
            continue;
        };
        if let Some((last, start)) = lines.last_mut() {
            let same_line = (start.1 - base.1).abs() < 0.3 * pt.max(last.2);
            let gap = base.0 - last.3.x1;
            if same_line && gap > -0.5 * pt && gap < 1.2 * pt {
                if gap > 0.12 * pt && !last.0.ends_with(' ') && !text.starts_with(' ') {
                    last.0.push(' ');
                }
                last.0.push_str(text);
                if *pt > last.2 {
                    last.1.clone_from(font);
                    last.2 = *pt;
                    last.4 = *style;
                }
                last.3 = last.3.union(*ink);
                continue;
            }
        }
        lines.push(((text.clone(), font.clone(), *pt, *ink, *style), *base));
    }
    lines
        .into_iter()
        .map(|((t, f, p, b, st), _)| (t.trim().to_string(), f, p, b, st))
        .filter(|l| !l.0.is_empty())
        .collect()
}

fn short(t: &str) -> String {
    let mut s: String = t.chars().take(28).collect();
    if t.chars().count() > 28 {
        s.push('…');
    }
    format!("\"{s}\"")
}

/// The fields a check looks for in the printed text.
const TEXT_FIELDS: [&str; 13] = [
    "name",
    "origin",
    "variety",
    "process",
    "level",
    "purpose",
    "roasted",
    "batch",
    "best_from",
    "weight",
    "notes",
    "message",
    "roaster",
];

/// A label's printed text, for finding its fields: as words (lowercase letters and digits,
/// everything else a space) and as one run of letters and digits.
struct Printed {
    words: String,
    run: String,
}

impl Printed {
    fn new(text: &str) -> Self {
        Printed {
            words: format!(" {} ", words(text)),
            run: run(text),
        }
    }

    /// `value`'s words in order, each whole ("1 nov" is not in "21 nov")
    fn has_words(&self, value: &str) -> bool {
        let w = words(value);
        !w.is_empty() && self.words.contains(&format!(" {w} "))
    }

    /// `value` printed: its first words (about 14 letters: a `clamp` may cut the rest), or
    /// its first letters and digits run together when they are at least `least` long (a word
    /// broken across lines, "`KenyaNyeri`")
    fn has(&self, value: &str, least: usize) -> bool {
        let all = words(value);
        let mut probe = String::new();
        for w in all.split(' ') {
            if !probe.is_empty() && probe.len() + w.len() > 14 {
                break;
            }
            if !probe.is_empty() {
                probe.push(' ');
            }
            probe.push_str(w);
        }
        if !probe.is_empty() && self.words.contains(&format!(" {probe} ")) {
            return true;
        }
        let r: String = run(value).chars().take(12).collect();
        r.chars().count() >= least && self.run.contains(&r)
    }
}

/// A gift message without a leading "for" or "to" ("For Sam" → "Sam")
fn recipient(message: &str) -> &str {
    let m = message.trim();
    for lead in ["for", "to"] {
        let Some(head) = m.get(..lead.len()) else {
            continue;
        };
        let rest = m[lead.len()..].trim_start_matches([':', ' ']);
        // "for Sam", not "Forest"
        if head.eq_ignore_ascii_case(lead) && rest.len() < m.len() - lead.len() && !rest.is_empty()
        {
            return rest;
        }
    }
    m
}

/// Lowercase words of letters and digits, one space between
fn words(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase letters and digits only
fn run(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The ways a design may print a date: as given ("29 Oct 2026"), the day and month either
/// way round, short or long ("29 Oct", "October 29"), ISO, and day and month in figures.
fn date_forms(text: &str, iso: Option<&str>) -> Vec<String> {
    use chrono::Datelike;
    let day = iso
        .and_then(|i| chrono::NaiveDate::parse_from_str(i, "%Y-%m-%d").ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(text.trim(), "%d %b %Y").ok());
    let mut forms = vec![text.to_string()];
    if let Some(d) = day {
        let (n, m) = (d.day(), d.month());
        for month in [d.format("%b").to_string(), d.format("%B").to_string()] {
            forms.push(format!("{n} {month}"));
            forms.push(format!("{month} {n}"));
        }
        forms.push(d.format("%Y-%m-%d").to_string());
        forms.push(format!("{m}/{n}"));
        forms.push(format!("{n}/{m}"));
        forms.push(format!("{m:02}/{n:02}"));
        forms.push(format!("{n:02}/{m:02}"));
    }
    forms
}

fn report(doc: &PagedDocument, req: &Request, warnings: Vec<Diag>) -> Report {
    let sheet = &req.data["sheet"];
    let inch = 72.0;
    let w = sheet["w"].as_f64().unwrap_or(4.0) * inch;
    let h = sheet["h"].as_f64().unwrap_or(2.0) * inch;
    let round = sheet["round"].as_bool().unwrap_or(false);
    let qr = req.files.get("qr.svg").map(|b| b.as_slice());
    let mut seen = Vec::new();
    if let Some(page) = doc.pages().first() {
        walk(&page.frame, Transform::identity(), None, qr, &mut seen);
    }
    let lines = lines_of(&seen);
    let mut issues: Vec<Issue> = Vec::new();
    let issue = |kind: &str, message: String| Issue {
        kind: kind.into(),
        message,
    };

    // Past the edge, or close to it. On a round label text is measured at its corners;
    // shapes and images by their box (a round picture fills a square box).
    let (cx, cy, r) = (w / 2.0, h / 2.0, w.min(h) / 2.0);
    let past_round = |b: &Bx| {
        [(b.x0, b.y0), (b.x1, b.y0), (b.x0, b.y1), (b.x1, b.y1)]
            .iter()
            .map(|(x, y)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let past_rect = |b: &Bx| {
        [-b.x0, -b.y0, b.x1 - w, b.y1 - h]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let side = |b: &Bx| {
        let d = [
            (-b.x0, "left"),
            (-b.y0, "top"),
            (b.x1 - w, "right"),
            (b.y1 - h, "bottom"),
        ];
        d.iter()
            .copied()
            .fold(
                (f64::NEG_INFINITY, ""),
                |a, x| if x.0 > a.0 { x } else { a },
            )
            .1
    };
    const EDGE: f64 = 0.2 * 72.0 / 25.4; // 0.2 mm
    const SAFE: f64 = 1.5 * 72.0 / 25.4; // 1.5 mm: a printer may land a millimetre off
    for (text, _, _, b, _) in &lines {
        let past = if round { past_round(b) } else { past_rect(b) };
        let where_ = if round {
            "the circle".to_string()
        } else {
            format!("the {} edge", side(b))
        };
        if past > EDGE {
            issues.push(issue(
                "overflow",
                format!("{} runs {} mm past {where_}", short(text), mm(past)),
            ));
        } else if past > -SAFE {
            issues.push(issue(
                "near_edge",
                format!(
                    "{} is {} mm from {where_}: keep 1.5 mm clear, printers land up to 1 mm off",
                    short(text),
                    mm(-past.min(0.0))
                ),
            ));
        }
    }
    for s in &seen {
        let (b, what) = match s {
            Seen::Image { b, qr: true, .. } => (b, "the QR code"),
            Seen::Image { b, .. } => (b, "an image"),
            Seen::Shape(b) => (b, "a shape"),
            Seen::Text { .. } => continue,
        };
        let past = past_rect(b);
        if past > 3.0 * EDGE {
            issues.push(issue(
                "overflow",
                format!(
                    "{what} at {}, {} mm runs {} mm past the {} edge",
                    mm(b.x0),
                    mm(b.y0),
                    mm(past),
                    side(b)
                ),
            ));
        }
    }
    // Text on text
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            let (ox, oy) = a.3.overlap(&b.3);
            if ox > EDGE && oy > EDGE {
                issues.push(issue(
                    "overlap",
                    format!(
                        "{} and {} overlap by {} mm",
                        short(&a.0),
                        short(&b.0),
                        mm(oy)
                    ),
                ));
            }
        }
    }
    if let Some(small) = lines
        .iter()
        .filter(|l| l.2 < 5.5)
        .min_by(|a, b| a.2.total_cmp(&b.2))
    {
        issues.push(issue(
            "tiny_text",
            format!(
                "{} is {:.1} pt: 5.5 pt is about the smallest that reads on a bag",
                short(&small.0),
                small.2
            ),
        ));
    }
    // The QR code: big enough to scan, with a clear margin
    let modules = req.data["qr_modules"].as_f64();
    let mut images = Vec::new();
    for s in &seen {
        let Seen::Image { qr: is_qr, svg, b } = s else {
            continue;
        };
        let module = (*is_qr)
            .then(|| modules.map(|m| (b.x1 - b.x0) / m))
            .flatten();
        if let Some(m) = module {
            // 0.33 mm (13 mil) is the usual smallest module for print a phone reads
            if m * 25.4 / 72.0 < 0.33 {
                issues.push(issue(
                    "qr_small",
                    format!(
                        "the QR code's squares are {} mm: make it at least {} mm wide (0.33 mm squares) or phones may not read it",
                        (m * 25.4 / 72.0 * 100.0).round() / 100.0,
                        (0.33 * modules.unwrap_or(25.0)).ceil()
                    ),
                ));
            }
            let quiet = b.grow(2.0 * m);
            if let Some(near) = lines.iter().find(|l| {
                let (ox, oy) = l.3.overlap(&quiet);
                ox > 0.0 && oy > 0.0
            }) {
                issues.push(issue(
                    "qr_crowded",
                    format!(
                        "{} is within two squares ({} mm) of the QR code: leave that margin clear",
                        short(&near.0),
                        mm(2.0 * m)
                    ),
                ));
            }
        }
        images.push(ImageBox {
            kind: if *is_qr {
                "qr"
            } else if *svg {
                "svg"
            } else {
                "raster"
            }
            .into(),
            x: mm(b.x0),
            y: mm(b.y0),
            w: mm(b.x1 - b.x0),
            h: mm(b.y1 - b.y0),
            module: module.map(|m| (m * 25.4 / 72.0 * 100.0).round() / 100.0),
        });
    }
    if lines.is_empty() && images.is_empty() {
        issues.push(issue("empty", "the label has no text or picture".into()));
    }

    // What fit and clamp did (metadata the helpers leave), and the manifest
    let intro = doc.introspector();
    let labelled = |name: &str| -> Vec<Value> {
        let Some(label) = Label::new(PicoStr::intern(name)) else {
            return vec![];
        };
        intro
            .query(&typst::foundations::Selector::Label(label))
            .iter()
            .filter_map(|c| c.to_packed::<MetadataElem>())
            .filter_map(|m| serde_json::to_value(&m.value).ok())
            .collect()
    };
    let notes = labelled("rr-note");
    // The label's height that nothing printed in: above the first line or image and below
    // the last (frames and drawn shapes do not count)
    let (top, bottom) = lines
        .iter()
        .map(|l| (l.3.y0, l.3.y1))
        .chain(images.iter().map(|i| {
            let (y, hh) = (i.y * 72.0 / 25.4, i.h * 72.0 / 25.4);
            (y, y + hh)
        }))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), (y0, y1)| {
            (a.min(y0), b.max(y1))
        });
    let free = (top.is_finite() && bottom.is_finite()).then(|| Free {
        top_mm: mm(top.max(0.0)),
        bottom_mm: mm((h - bottom).max(0.0)),
    });
    for n in &notes {
        if n["kind"] == "hint" {
            issues.push(issue(
                "hint",
                n["text"].as_str().unwrap_or_default().to_string(),
            ));
        }
        if n["kind"] == "too_long" {
            // `fit`: one line; `wrap`: its lines
            let lines = match n["lines"].as_u64() {
                Some(k) if k > 1 => format!("on {k} lines"),
                _ => "on one line".into(),
            };
            issues.push(issue(
                "too_long",
                format!(
                    "{} does not fit {lines} even at {} pt ({} mm too long): shorten it, lower `min`, or give it more room",
                    short(n["text"].as_str().unwrap_or("")),
                    n["pt"],
                    n["over_mm"]
                ),
            ));
        }
        if n["kind"] == "cut" {
            // Room the cut text could have had: the free height past the usual margins
            // (about 4 mm at the top and the bottom)
            let spare = free.as_ref().map_or(0.0, |f| f.top_mm + f.bottom_mm - 8.0);
            let room = if spare >= 4.0 {
                format!(
                    " while {spare:.0} mm of the label's height is free (clamp with `height:` the room left, or centre the text)"
                )
            } else {
                String::new()
            };
            issues.push(issue(
                "cut",
                format!(
                    "{} was cut to {} lines{room}: shorten it, give it more room, or allow more lines",
                    short(n["text"].as_str().unwrap_or("")),
                    n["lines"]
                ),
            ));
        }
    }
    let manifest = labelled("rr-manifest")
        .into_iter()
        .next()
        .filter(|v| !v.is_null());
    let params = labelled("rr-params").into_iter().next();
    if let Some(given) = req.data["params"].as_object() {
        let declared = manifest
            .as_ref()
            .and_then(|m| m["params"].as_object())
            .map(|p| p.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for k in given.keys().filter(|k| !declared.contains(k)) {
            issues.push(issue(
                "param",
                format!("the design does not declare a parameter {k:?} (its `design.params`)"),
            ));
        }
    }
    for wn in &warnings {
        issues.push(issue("typst", wn.to_string()));
    }

    // Which fields made it onto the label: by words, whatever the case, punctuation or
    // slugging ("kenya-nyeri-gatugi-aa"), and a date in any of its usual forms ("29 Oct")
    let printed_text = lines
        .iter()
        .map(|l| l.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let printed = Printed::new(&printed_text);
    let slot = &req.data["slots"][0];
    let mut fields = FieldsSeen::default();
    for f in TEXT_FIELDS {
        let shown = match &slot[f] {
            Value::String(s) if !s.trim().is_empty() => match f {
                "roasted" | "best_from" => {
                    let iso = slot[format!("{f}_iso").as_str()].as_str();
                    date_forms(s, iso).iter().any(|d| printed.has_words(d))
                }
                // "250 g · 8.8 oz": either part ("Net wt 8.8 oz (250 g)", "250g")
                "weight" => s.split('·').any(|part| printed.has(part, 4)),
                // A design with its own "To" drops the message's "for" ("for Sam" → "To Sam")
                "message" => printed.has(s, 5) || printed.has(recipient(s), 3),
                _ => printed.has(s, 5),
            },
            // A number is printed when it stands alone ("#5", "batch 5"), not inside "25"
            Value::Number(n) => printed.has_words(&n.to_string()),
            _ => continue,
        };
        if shown {
            fields.shown.push(f.into());
        } else {
            fields.missing.push(f.into());
        }
    }
    if slot["qr"] == true {
        if images.iter().any(|i| i.kind == "qr") {
            fields.shown.push("qr".into());
        } else {
            fields.missing.push("qr".into());
        }
    }

    // What a bag's label is for: the coffee, when it was roasted and when it is at its best.
    // A design may leave one off on purpose (a jar label without the name), so a soft issue
    for (f, what) in [
        ("name", "the coffee's name"),
        ("roasted", "the roast date"),
        ("best_from", "the best-from date"),
    ] {
        if fields.missing.iter().any(|m| m == f) {
            issues.push(issue(
                "dropped",
                format!(
                    "the label does not print {what} ({f}) though the roast has it: print it, or hide it on purpose (hide={f})"
                ),
            ));
        }
    }

    let rank = |k: &str| match k {
        "overflow" => 0,
        "overlap" | "too_long" => 1,
        "qr_small" | "qr_crowded" => 2,
        "cut" => 3,
        "typst" | "param" | "empty" => 4,
        "near_edge" => 5,
        "dropped" | "hint" => 7,
        _ => 6,
    };
    issues.sort_by_key(|i| rank(&i.kind));
    Report {
        ok: issues.iter().all(|i| SOFT.contains(&i.kind.as_str())),
        free,
        w: mm(w),
        h: mm(h),
        round,
        issues,
        lines: lines
            .into_iter()
            .map(|(text, font, pt, b, st)| TextLine {
                text,
                font,
                pt: (pt * 10.0).round() / 10.0,
                x: mm(b.x0),
                y: mm(b.y0),
                w: mm(b.x1 - b.x0),
                h: mm(b.y1 - b.y0),
                bold: st.bold,
                x_mm: mm(st.x_em * pt),
                cap_mm: mm(st.cap_em * pt),
            })
            .collect(),
        images,
        fields,
        manifest,
        params,
        notes,
    }
}

/// The sample label a check uses when no roast is named: every field a roast's has.
pub fn sample_label() -> Value {
    json!({
        "name": "Costa Rica Palmichal",
        "full_name": "Costa Rica Palmichal",
        "origin": "Tarrazú, Costa Rica",
        "variety": "Caturra, Catuaí",
        "process": "washed",
        "level": "Light",
        "purpose": "Espresso",
        "roasted": "25 Oct 2026",
        "roasted_iso": "2026-10-25",
        "batch": 5,
        "best_from": "4 Nov 2026",
        "best_from_iso": "2026-11-04",
        "rest_days": 10,
        "form": "Whole bean",
        "ground": false,
        "grind": null,
        "best_until": "29 Nov 2026",
        "best_until_iso": "2026-11-29",
        "weight": "250 g · 8.8 oz",
        "weight_g": 250.0,
        "net": "Net wt 8.8 oz (250 g)",
        "notes": "Red apple, panela, milk chocolate",
        "message": null,
        "roaster": null,
        "brew": super::share::brew_for(Some("espresso"), Some("light"), 18.0, 15.0),
        "qr": true,
        "qr_to": "app",
        "url": "https://roastwarden.example/history/0b6f3c1e-5d2a-4c8e-9f7a-2e4d6b8a1c3f",
    })
}

#[cfg(test)]
mod tests {
    use super::recipient;

    #[test]
    fn a_message_loses_its_own_for_or_to() {
        assert_eq!(recipient("for Sam"), "Sam");
        assert_eq!(recipient(" For Sam and Alex "), "Sam and Alex");
        assert_eq!(recipient("To: Sam"), "Sam");
        assert_eq!(recipient("Forest"), "Forest");
        assert_eq!(recipient("for"), "for");
        assert_eq!(recipient("Merry Christmas"), "Merry Christmas");
        assert_eq!(recipient("für Oma"), "für Oma");
    }
}
