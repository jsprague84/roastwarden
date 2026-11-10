//! `roast labels`: bag labels for a roast, and the label studio (docs/labels.md). A design is
//! Typst; `labels design check` lays one label out on the server's sandboxed renderer and
//! answers Typst's errors with lines, or a layout report (text that runs past the edge,
//! overlaps, is too small, was shrunk or cut; the QR code's size; which fields printed), and
//! a small PNG when asked. Text first, pixels only when asked: cheap per turn for an agent.
//! This file has the command's shape, the shared arguments and the sheet; the subcommands are in
//! printers.rs, runs.rs, check.rs and designs.rs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Map, Value, json};

use crate::api::{Api, Upload, enc};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::records::save_file;
use crate::commands::roasts::roast_id;
use crate::out::{Failure, Out, code};
use crate::text::parse_weight;

mod check;
mod designs;
mod printers;
mod runs;

use designs::{AssetCmd, BrandCmd, DesignCmd, asset, brand, design};
use printers::{PrinterCmd, printer};
use runs::{ReprintArgs, RunArgs, RunsCmd, reprint, run, runs, sheets};

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct LabelsCmd {
    #[command(subcommand)]
    pub sub: Option<LabelsSub>,
    #[command(flatten)]
    pub args: LabelsArgs,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)] // parsed once per run of the CLI
pub enum LabelsSub {
    /// Label designs: check a draft, save it with versions, give a coffee its label
    #[command(subcommand)]
    Design(DesignCmd),
    /// Images and fonts designs can use (`image("logo.svg")`, a font's family)
    #[command(subcommand)]
    Asset(AssetCmd),
    /// Brand kits: the roaster's name, tagline, place, logo, colours and fonts (`l.brand`)
    #[command(subcommand)]
    Brand(BrandCmd),
    /// A label printer, asked how it is: make and model, its state and why it is stopped (a
    /// door open, no paper), what it takes (PDF, PWG Raster, URF), trays. Prints nothing.
    /// `printer list|add|edit|rm|default|find|certificate|calibrate` manage them
    Printer(PrinterCmd),
    /// A sheet of labels for one roast or several at once (a gift run): each roast with its
    /// design, settings, brand and copies, from the sheet's next free label. Writes the PDF
    /// (the run is kept in the history), or --print sends it to the label printer (a token's
    /// print waits for the owner's approval: exit 3)
    Run(RunArgs),
    /// Print runs, newest first: what was printed when, on which sheet from which label, how
    /// the printer's job went. `runs show|pdf|print|done <run>` for one (`latest`: the newest)
    Runs(RunsCmd),
    /// A run's labels again, as they were (each label's data, design@version and QR code):
    /// all, or --labels 3,4; from the sheet's next free label or --start; then a PDF or --print
    Reprint(ReprintArgs),
    /// Each sheet's next free label, where a run starts (it moves on when a print completes);
    /// --set 5163=1 after loading a new sheet
    Sheets {
        /// SHEET=N: the next free label on that sheet
        #[arg(long, value_name = "SHEET=N")]
        set: Option<String>,
    },
}

/// Which printer, tray and colour a print uses
#[derive(Args, Debug, Default, Clone)]
pub struct Via {
    /// A printer's id (default the default printer): its calibration and where it prints
    #[arg(long)]
    pub printer: Option<String>,
    /// A tray instead of the printer's (`media-source`, e.g. tray-1)
    #[arg(long)]
    pub tray: Option<String>,
    /// monochrome or color instead of the printer's
    #[arg(long)]
    pub color: Option<String>,
}

impl Via {
    fn body(&self) -> Value {
        let mut b = json!({});
        for (k, v) in [
            ("printer", &self.printer),
            ("tray", &self.tray),
            ("color", &self.color),
        ] {
            if let Some(v) = v {
                b[k] = json!(v);
            }
        }
        b
    }
}

/// What changes on a label, for a check and for labels alike.
#[derive(Args, Debug, Default)]
pub struct Fill {
    /// The label sheet: 5163 (4 × 2 in, the default), 6874, 5294 (round), or the owner's
    #[arg(long)]
    pub sheet: Option<String>,
    /// A design parameter (its `design.params`), repeatable: `--set art=cherry`; `KEY=@FILE`
    /// reads the value from a file (ASCII art)
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub set: Vec<String>,
    /// Net weight on the label, e.g. 250g (default the roast's roasted weight)
    #[arg(long, value_parser = parse_weight)]
    pub weight: Option<f64>,
    /// Ground coffee with this grind (espresso, filter, moka, "French press"): the label
    /// reads "Ground · espresso" instead of "Whole bean", and its best-until is earlier
    #[arg(long, value_name = "GRIND")]
    pub ground: Option<String>,
    /// A line for a gift, e.g. "for Sam"
    #[arg(long)]
    pub message: Option<String>,
    /// Tasting notes instead of the roast's cupping notes or the bean's
    #[arg(long)]
    pub notes: Option<String>,
    /// Leave fields off, comma-separated: origin, variety, process, level, roasted, batch,
    /// best_from, form, weight, notes, qr, roaster
    #[arg(long)]
    pub hide: Option<String>,
    /// A brand kit (`l.brand`; default the bean's, then the `label_brand` setting)
    #[arg(long)]
    pub brand: Option<String>,
    /// Coffee that is sold: the bag it goes on (pouch-12oz, pouch-8oz, sample-2oz, …;
    /// `label` when the label is the whole front; alone: the `label_bag` setting). The label
    /// gets the heights the bag needs, and a check reads it against 21 CFR 101.3, 101.5, 101.7
    #[arg(long, value_name = "BAG", num_args = 0..=1, default_missing_value = "default")]
    pub retail: Option<String>,
    /// What the QR code opens: `app` (the roast in the dashboard), `share` (its public share
    /// page; made when the roast has none: `roast share`), `supplier` (the coffee's offer),
    /// `none`, or an address (https://…). Default: the `label_qr` setting, else `app`
    #[arg(long, value_name = "app|share|supplier|none|URL")]
    pub qr: Option<String>,
}

#[derive(Args, Debug)]
pub struct LabelsArgs {
    /// The roast's id, or `latest` (--roaster narrows it)
    #[arg(required_unless_present_any = ["calibration", "list"])]
    pub roast: Option<String>,
    /// The design: plain (default), gift, or the owner's (`--list` shows them)
    #[arg(long, conflicts_with = "source")]
    pub design: Option<String>,
    /// A design's .typ file to use, not saved: a one-off
    #[arg(long, value_name = "FILE")]
    pub source: Option<PathBuf>,
    #[command(flatten)]
    pub fill: Fill,
    /// How many labels
    #[arg(long, default_value_t = 1)]
    pub count: usize,
    /// The first label to print on, from 1, for a part-used sheet
    #[arg(long)]
    pub start: Option<usize>,
    /// The first page as SVG, with each label's outline (a preview), not the PDF
    #[arg(long)]
    pub svg: bool,
    /// The file (default labels-<batch or id>.pdf here); `-` for stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub roaster: Option<String>,
    /// The calibration page for --sheet instead: marks at the label corners
    #[arg(long, conflicts_with_all = ["roast", "list"])]
    pub calibration: bool,
    /// What there is to choose: sheets (with their offsets), designs, fields, rest days, fonts
    #[arg(long, conflicts_with = "roast")]
    pub list: bool,
    /// Send them to the label printer instead of writing a file. A token's print always waits
    /// for the owner's approval (exit 3), which says which sheet to load and from which label
    #[arg(long, conflicts_with_all = ["svg", "output"])]
    pub print: bool,
    #[command(flatten)]
    pub via: Via,
    #[command(flatten)]
    pub wait: Wait,
}

pub async fn labels(api: &Api, out: &Out, cmd: LabelsCmd) -> Result<u8, Failure> {
    match cmd.sub {
        Some(LabelsSub::Design(c)) => design(api, out, c).await,
        Some(LabelsSub::Asset(c)) => asset(api, out, c).await,
        Some(LabelsSub::Brand(c)) => brand(api, out, c).await,
        Some(LabelsSub::Printer(c)) => printer(api, out, c).await,
        Some(LabelsSub::Run(a)) => run(api, out, a).await,
        Some(LabelsSub::Runs(c)) => runs(api, out, c).await,
        Some(LabelsSub::Reprint(a)) => reprint(api, out, a).await,
        Some(LabelsSub::Sheets { set }) => sheets(api, out, set).await,
        None => sheet(api, out, cmd.args).await,
    }
}

/// This machine's offset from UTC, minutes, when it has one: dates on a label are the owner's
/// dates. A host on UTC (a server, a container, an agent's box) sends none, so the server's
/// `time_zone` setting decides (docs/cli.md).
pub(crate) fn tz() -> Option<i32> {
    Some(chrono::Local::now().offset().local_minus_utc() / 60).filter(|m| *m != 0)
}

/// `path` with `tz=` added when this machine has an offset ([`tz`]).
pub(crate) fn with_tz(path: &str) -> String {
    match tz() {
        Some(m) => format!("{path}{}tz={m}", if path.contains('?') { '&' } else { '?' }),
        None => path.to_string(),
    }
}

fn read_file(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path)
        .map_err(|e| Failure::new("NO_FILE", format!("could not read {}: {e}", path.display())))
}

/// `--set KEY=VALUE` as the parameters object; `KEY=@FILE` reads the file.
fn params(set: &[String]) -> Result<Map<String, Value>, Failure> {
    let mut map = Map::new();
    for s in set {
        let Some((k, v)) = s.split_once('=') else {
            return Err(Failure::new(
                "USAGE",
                format!("--set takes KEY=VALUE (or KEY=@FILE), not {s:?}"),
            ));
        };
        let value = match v.strip_prefix('@') {
            Some(file) => read_file(Path::new(file))?,
            None => v.to_string(),
        };
        map.insert(k.trim().to_string(), Value::String(value));
    }
    Ok(map)
}

/// The body both `POST /api/labels/check` and `/render` read.
fn body(fill: &Fill, roast: Option<String>) -> Result<Value, Failure> {
    let mut b = json!({ "params": params(&fill.set)?, "tz": tz() });
    let put = |b: &mut Value, k: &str, v: Value| {
        b[k] = v;
    };
    if let Some(s) = &fill.sheet {
        put(&mut b, "sheet", json!(s));
    }
    if let Some(r) = roast {
        put(&mut b, "roast", json!(r));
    }
    if let Some(w) = fill.weight {
        put(&mut b, "weight_g", json!(w));
    }
    if let Some(g) = &fill.ground {
        put(&mut b, "form", json!("ground"));
        put(&mut b, "grind", json!(g));
    }
    if let Some(m) = &fill.message {
        put(&mut b, "message", json!(m));
    }
    if let Some(n) = &fill.notes {
        put(&mut b, "notes", json!(n));
    }
    if let Some(br) = &fill.brand {
        put(&mut b, "brand", json!(br));
    }
    if let Some(h) = &fill.hide {
        put(
            &mut b,
            "hide",
            json!(h.split(',').map(str::trim).collect::<Vec<_>>()),
        );
    }
    if let Some(r) = &fill.retail {
        put(&mut b, "retail", json!(r));
    }
    if let Some(q) = &fill.qr {
        put(&mut b, "qr", json!(q));
    }
    Ok(b)
}

/// A design argument: a file when it names one (or ends in .typ), else a design's id.
fn design_arg(b: &mut Value, design: &str) -> Result<String, Failure> {
    let path = Path::new(design);
    if path.is_file() || design.ends_with(".typ") {
        b["source"] = json!(read_file(path)?);
        Ok(path.display().to_string())
    } else {
        b["design"] = json!(design);
        Ok(design.to_string())
    }
}

async fn sheet(api: &Api, out: &Out, a: LabelsArgs) -> Result<u8, Failure> {
    if a.list {
        let o = api.get("/api/labels").await?;
        out.emit(&o, |o| {
            let mut lines = vec!["sheets:".to_string()];
            for s in o["sheets"].as_array().into_iter().flatten() {
                lines.push(format!(
                    "  {:<8} {}  (offset {} mm right, {} mm down)",
                    s["id"].as_str().unwrap_or("?"),
                    s["name"].as_str().unwrap_or(""),
                    s["offset"]["x_mm"],
                    s["offset"]["y_mm"]
                ));
            }
            lines.push("designs:".into());
            for d in o["designs"].as_array().into_iter().flatten() {
                lines.push(format!(
                    "  {:<8} {}",
                    d["id"].as_str().unwrap_or("?"),
                    d["name"].as_str().unwrap_or("")
                ));
            }
            lines.join("\n")
        });
        return Ok(code::OK);
    }
    if a.calibration {
        let mut q = String::new();
        if let Some(s) = &a.fill.sheet {
            q = format!("?sheet={}", enc(s));
        }
        let path = format!("/api/labels/calibration.pdf{q}");
        let name = format!(
            "calibration-{}.pdf",
            a.fill.sheet.as_deref().unwrap_or("5163")
        );
        return save_file(api, out, &path, a.output, &name).await;
    }
    let roast = a.roast.clone().unwrap_or_default();
    let id = roast_id(api, &roast, a.roaster.as_deref()).await?;
    if a.print {
        let mut b = body(&a.fill, None)?;
        for (k, v) in a.via.body().as_object().into_iter().flatten() {
            b[k] = v.clone();
        }
        b["count"] = json!(a.count);
        if let Some(s) = a.start {
            b["start"] = json!(s);
        }
        match (&a.source, &a.design) {
            (Some(file), _) => b["source"] = json!(read_file(file)?),
            (None, Some(d)) => b["design"] = json!(d),
            (None, None) => {}
        }
        let reply = api
            .send(
                Method::POST,
                &format!("/api/sessions/{}/labels/print", enc(&id)),
                Some(&b),
                Duration::from_secs(120),
            )
            .await?;
        return held_or_done(api, out, reply, a.wait, |p| {
            format!(
                "{} ({}, job {}: {} label(s) of {} on {} from label {}; run {})",
                p["message"].as_str().unwrap_or("sent"),
                p["printer"].as_str().unwrap_or("the printer"),
                p["job_id"],
                p["count"],
                p["design"].as_str().unwrap_or("?"),
                p["sheet"].as_str().unwrap_or("?"),
                p["start"],
                p["run"].as_str().unwrap_or("?")
            )
        })
        .await;
    }
    let ext = if a.svg { "svg" } else { "pdf" };
    // A file name people can tell apart: the batch number when there is one
    let one = api.get(&format!("/api/sessions/{}", enc(&id))).await?;
    let tag = one["batch_number"]
        .as_i64()
        .map_or_else(|| id.chars().take(8).collect(), |n| format!("batch-{n}"));
    let output = a
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("labels-{tag}.{ext}")));
    let bytes = if a.source.is_none() && a.fill.set.is_empty() && a.fill.retail.is_none() {
        // A saved design: the same GET the dashboard's download uses
        let mut q = vec![format!("count={}", a.count)];
        q.extend(tz().map(|m| format!("tz={m}")));
        for (k, v) in [
            ("sheet", a.fill.sheet.clone()),
            ("design", a.design.clone()),
            ("message", a.fill.message.clone()),
            ("notes", a.fill.notes.clone()),
            ("hide", a.fill.hide.clone()),
            ("brand", a.fill.brand.clone()),
            ("qr", a.fill.qr.clone()),
        ] {
            if let Some(v) = v {
                q.push(format!("{k}={}", enc(&v)));
            }
        }
        if let Some(s) = a.start {
            q.push(format!("start={s}"));
        }
        if let Some(w) = a.fill.weight {
            q.push(format!("weight_g={w}"));
        }
        let path = format!("/api/sessions/{}/labels.{ext}?{}", enc(&id), q.join("&"));
        api.download(&path).await?.0
    } else {
        // A one-off source, or parameters: the body carries them
        let mut b = body(&a.fill, Some(id.clone()))?;
        b["count"] = json!(a.count);
        if let Some(s) = a.start {
            b["start"] = json!(s);
        }
        b["format"] = json!(ext);
        match (&a.source, &a.design) {
            (Some(file), _) => b["source"] = json!(read_file(file)?),
            (None, Some(d)) => b["design"] = json!(d),
            (None, None) => {}
        }
        api.transfer(Method::POST, "/api/labels/render", Upload::Json(&b))
            .await?
            .0
    };
    if output == Path::new("-") {
        use std::io::Write;
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| Failure::new("FAILED", e.to_string()))?;
        return Ok(code::OK);
    }
    std::fs::write(&output, &bytes).map_err(|e| {
        Failure::new(
            "FAILED",
            format!("could not write {}: {e}", output.display()),
        )
    })?;
    let answer = json!({ "file": output.display().to_string(), "bytes": bytes.len() });
    out.emit(&answer, |a| {
        format!(
            "wrote {} ({} bytes)",
            a["file"].as_str().unwrap_or("?"),
            a["bytes"]
        )
    });
    Ok(code::OK)
}
