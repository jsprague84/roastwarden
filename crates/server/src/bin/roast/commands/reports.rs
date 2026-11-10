//! `roast report <kind> …`: reports the server makes with Typst (docs/reports.md), saved as
//! files, or their data as JSON.

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use reqwest::Method;

use crate::api::{Api, enc};
use crate::commands::labels;
use crate::commands::records::save_file;
use crate::commands::roasts::roast_id;
use crate::out::{Failure, Out, code};

#[derive(Args, Debug)]
pub struct ReportCmd {
    #[command(subcommand)]
    pub kind: Kind,
}

#[derive(Subcommand, Debug)]
pub enum Kind {
    /// The kinds of report the server makes
    List,
    /// A roast's report: the coffee, its numbers against what its level, profile and earlier
    /// roasts lead to expect, the curve with phases and events, heater and fan, notes and
    /// cupping. ROAST: its id, or `latest` (--roaster narrows it)
    Roast(RoastArgs),
    /// Two roasts compared, an A/B experiment: BT and RoR of both on one chart with heater
    /// and fan, the phases on one time scale, every number side by side with B − A. A then B
    /// (ids, `#12`, `latest`); one roast: it is B, against the finished roast before it on
    /// its roaster; none: the newest two finished roasts (--roaster narrows it)
    Compare(CompareArgs),
    /// The cupping sheet: with --blank a form to score on (classic SCA protocol) for these
    /// roasts with blind codes and the key on its own page; without, their scores as a table
    /// (no roast named: the latest cupping session, or --date). ROAST: ids or `latest`
    Cupping(CuppingArgs),
    /// Green inventory: stock, planned roasts, cost and value, age, the recent pace and when
    /// each coffee runs out; --count for a stock-take sheet to weigh against
    Inventory(InventoryArgs),
    /// The shopping list as one short order per supplier (prices as seen, the usual
    /// shipping), then the coffees running low
    Shopping(How),
    /// A green coffee's report: what is known of it, stock and cost, its roasts as a table
    /// and small multiples, its cuppings, its ledger. COFFEE: its name or id
    Coffee(SubjectArgs),
    /// A profile's report: its targets, its roasts ranked with an average row, their curves
    /// overlaid on the profile's, how the numbers spread. PROFILE: its name or id
    Profile(ProfileArgs),
    /// A period's report: the roasts with a sum row, green roasted and bought, spend and
    /// landed cost, each coffee, the stock at the end. --year 2026 is the year in review;
    /// --month 2026-10; or --from and --to (default: this month so far)
    Period(PeriodArgs),
    /// Purchases over a period (as `period` takes it): each order with shipping and fees,
    /// each supplier, landed cost per kg over time
    Purchases(PeriodArgs),
    /// The roast-day plan sheet: the planned roasts in order with the green to weigh, the
    /// profile's targets and boxes to write charge, first crack, drop and the weight in
    Plan(PlanArgs),
    /// Traceability: the green coffee (supplier, lot, purchases), each roast and where it
    /// went (packed bags, label runs, gifts, use, discards, what is left). SUBJECT: `#12`,
    /// `latest` or a roast's id; or a coffee's name or id
    Trace(TraceArgs),
}

#[derive(Args, Debug)]
pub struct TraceArgs {
    pub subject: String,
    /// With `latest` or a batch number: on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[command(flatten)]
    pub how: How,
}

/// What a trace is of: `#12` or `latest` is a roast; else a coffee by name or id, else a
/// roast's id. Answers (`roast` | `bean`, its id).
pub async fn trace_subject(
    api: &Api,
    reference: &str,
    roaster: Option<&str>,
) -> Result<(&'static str, String), Failure> {
    if reference.starts_with('#') || reference.eq_ignore_ascii_case("latest") {
        return Ok(("roast", roast_id(api, reference, roaster).await?));
    }
    match api.bean(reference).await {
        Ok(b) => Ok(("bean", b["id"].as_str().unwrap_or_default().to_string())),
        Err(e) => {
            let path = format!("/api/sessions/{}", enc(reference));
            match api.call(Method::GET, &path, None).await {
                Ok(_) => Ok(("roast", reference.to_string())),
                Err(_) => Err(e),
            }
        }
    }
}

/// A trace in words: the coffee, its purchases, each roast and where it went
pub fn trace_text(t: &serde_json::Value) -> String {
    let g = |v: &serde_json::Value| -> String {
        v.as_f64().map_or("?".into(), |g| {
            if g.abs() >= 1000.0 {
                format!("{:.2} kg", g / 1000.0)
            } else {
                format!("{g:.0} g")
            }
        })
    };
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("–").to_string();
    let mut out = String::new();
    match t["coffee"].as_object() {
        Some(c) => {
            let _ = writeln!(
                out,
                "{}: supplier {}, lot {}, {} {}",
                s(&c["name"]),
                s(&c["supplier"]),
                s(&c["lot"]),
                s(&c["origin"]),
                c["process"].as_str().unwrap_or("")
            );
            for p in c["purchases"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "  bought {} {} from {}{}",
                    p["at"].as_str().and_then(|a| a.get(..10)).unwrap_or("?"),
                    g(&p["grams"]),
                    s(&p["supplier"]),
                    p["crop_year"]
                        .as_str()
                        .map(|c| format!(", crop {c}"))
                        .unwrap_or_default()
                );
            }
        }
        None => out.push_str("No green coffee recorded for this roast\n"),
    }
    for r in t["roasts"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "{} {} roasted {} on {}{}: {} → {} green → roasted, {} left",
            r["batch_number"]
                .as_i64()
                .map(|n| format!("#{n}"))
                .unwrap_or_default(),
            s(&r["name"]),
            s(&r["roasted_on"]),
            r["roaster"]
                .as_str()
                .unwrap_or(r["device_id"].as_str().unwrap_or("?")),
            r["profile"]
                .as_str()
                .map(|p| format!(" ({p})"))
                .unwrap_or_default(),
            g(&r["green_g"]),
            g(&r["roasted_g"]),
            g(&r["left_g"]),
        );
        for m in r["moves"].as_array().into_iter().flatten() {
            let what = match m["kind"].as_str() {
                Some("packed") => format!(
                    "packed {} × {} {}{}",
                    m["bags"].as_i64().unwrap_or(0),
                    g(&m["bag_g"]),
                    if m["form"] == "ground" {
                        "ground"
                    } else {
                        "whole bean"
                    },
                    m["grind"]
                        .as_str()
                        .map(|x| format!(" for {x}"))
                        .unwrap_or_default()
                ),
                Some("given") => format!(
                    "gave {}{}",
                    g(&serde_json::json!(-m["delta_g"].as_f64().unwrap_or(0.0))),
                    m["recipient"]
                        .as_str()
                        .map(|x| format!(" to {x}"))
                        .unwrap_or_default()
                ),
                Some(k) => format!(
                    "{k} {}",
                    g(&serde_json::json!(
                        m["delta_g"].as_f64().unwrap_or(0.0).abs()
                    ))
                ),
                None => "?".into(),
            };
            let _ = writeln!(
                out,
                "  {} {what}{}",
                m["at"].as_str().and_then(|a| a.get(..10)).unwrap_or("?"),
                m["note"]
                    .as_str()
                    .map(|n| format!(": {n}"))
                    .unwrap_or_default()
            );
        }
        for l in r["label_runs"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {} labels: {} ({})",
                l["created_at"]
                    .as_str()
                    .and_then(|a| a.get(..10))
                    .unwrap_or("?"),
                l["labels"].as_i64().unwrap_or(0),
                s(&l["state"])
            );
        }
    }
    out.trim_end().to_string()
}

/// `roast trace`: a roast back to its green coffee, or a coffee to every roast and bag.
pub async fn trace(
    api: &Api,
    out: &Out,
    subject: String,
    roaster: Option<String>,
) -> Result<u8, Failure> {
    let (kind, id) = trace_subject(api, &subject, roaster.as_deref()).await?;
    let t = api
        .get(&labels::with_tz(&format!("/api/trace/{kind}/{}", enc(&id))))
        .await?;
    out.emit(&t, trace_text);
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct PeriodArgs {
    /// A whole year: 2026
    #[arg(long, conflicts_with_all = ["month", "from", "to"])]
    pub year: Option<String>,
    /// A whole month: 2026-10
    #[arg(long, conflicts_with_all = ["from", "to"])]
    pub month: Option<String>,
    /// The first day, YYYY-MM-DD
    #[arg(long)]
    pub from: Option<String>,
    /// The last day, YYYY-MM-DD
    #[arg(long)]
    pub to: Option<String>,
    #[command(flatten)]
    pub how: How,
}

impl PeriodArgs {
    fn query(&self) -> String {
        let mut q = vec![self.how.query()];
        for (k, v) in [
            ("year", &self.year),
            ("month", &self.month),
            ("from", &self.from),
            ("to", &self.to),
        ] {
            if let Some(v) = v {
                q.push(format!("{k}={}", enc(v)));
            }
        }
        q.join("&")
    }
}

#[derive(Args, Debug)]
pub struct PlanArgs {
    /// Only what is planned on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct SubjectArgs {
    pub subject: String,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct ProfileArgs {
    pub profile: String,
    /// Only this roaster's roasts
    #[arg(long)]
    pub roaster: Option<String>,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct InventoryArgs {
    /// A stock-take sheet: each coffee's grams with boxes to write the count in
    #[arg(long)]
    pub count: bool,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct CuppingArgs {
    pub roasts: Vec<String>,
    /// A blank form to score on, rather than the scores
    #[arg(long)]
    pub blank: bool,
    /// On a blank form, this many samples of the owner's own as well (2 when no roast is named)
    #[arg(long)]
    pub samples: Option<usize>,
    /// On a blank form: 2 samples a page with scales to mark (default), or 6 with boxes
    #[arg(long, value_parser = per_page)]
    pub per_page: Option<u8>,
    /// The scores of the cuppings on this day (YYYY-MM-DD); default the latest
    #[arg(long, conflicts_with = "blank")]
    pub date: Option<String>,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// A (the one B is measured against), then B: ids, `#12` (batch numbers) or `latest`
    #[arg(num_args = 0..=2)]
    pub roasts: Vec<String>,
    /// With `latest`, batch numbers or no roast: on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// How close BT counts as together, °C (default 0.5)
    #[arg(long)]
    pub within: Option<f64>,
    #[command(flatten)]
    pub how: How,
}

#[derive(Args, Debug)]
pub struct RoastArgs {
    pub roast: String,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    #[command(flatten)]
    pub how: How,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Pdf,
    Svg,
    Png,
    Json,
}

/// 2 or 6 samples a page
fn per_page(s: &str) -> Result<u8, String> {
    match s.trim() {
        "2" => Ok(2),
        "6" => Ok(6),
        _ => Err(format!("'{s}': 2 (scales to mark) or 6 (boxes)")),
    }
}

/// How to make it and where to put it
#[derive(Args, Debug)]
pub struct How {
    /// Where to write it (`-` for stdout); default: the server's file name here
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// pdf (the default); svg (every page in one, a preview); png (one page: --page,
    /// --dpi; a cheap look); json (the data the page is made from, printed)
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    /// With png: the page, from 1
    #[arg(long)]
    pub page: Option<usize>,
    /// With png: dots per inch, 36 to 200 (default 96)
    #[arg(long)]
    pub dpi: Option<u32>,
    /// letter or a4 (default: the server's report_paper setting, else letter)
    #[arg(long)]
    pub paper: Option<String>,
    /// C or F (default: the server's report_units setting, else C)
    #[arg(long)]
    pub units: Option<String>,
}

impl How {
    fn query(&self) -> String {
        let mut q: Vec<String> = crate::commands::labels::tz()
            .map(|m| format!("tz={m}"))
            .into_iter()
            .collect();
        let format = match self.format {
            Some(Format::Pdf) | None => "pdf",
            Some(Format::Svg) => "svg",
            Some(Format::Png) => "png",
            Some(Format::Json) => "json",
        };
        q.push(format!("format={format}"));
        if let Some(p) = self.page {
            q.push(format!("page={p}"));
        }
        if let Some(d) = self.dpi {
            q.push(format!("dpi={d}"));
        }
        if let Some(p) = &self.paper {
            q.push(format!("paper={}", enc(p)));
        }
        if let Some(u) = &self.units {
            q.push(format!("units={}", enc(u)));
        }
        q.join("&")
    }
}

pub async fn report(api: &Api, out: &Out, cmd: ReportCmd) -> Result<u8, Failure> {
    match cmd.kind {
        Kind::List => {
            let kinds = api.get("/api/reports").await?;
            out.emit(&kinds, |k| {
                k.as_array()
                    .into_iter()
                    .flatten()
                    .map(|k| {
                        format!(
                            "{:<8} {}: {}",
                            k["id"].as_str().unwrap_or("?"),
                            k["title"].as_str().unwrap_or("?"),
                            k["description"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            Ok(code::OK)
        }
        Kind::Roast(a) => {
            let id = roast_id(api, &a.roast, a.roaster.as_deref()).await?;
            let path = format!("/api/reports/roast/{}?{}", enc(&id), a.how.query());
            fetch(api, out, &path, a.how, "roast-report.pdf").await
        }
        Kind::Compare(a) => {
            let mut ids = Vec::new();
            for r in &a.roasts {
                ids.push(roast_id(api, r, a.roaster.as_deref()).await?);
            }
            let mut q = vec![a.how.query()];
            if !ids.is_empty() {
                q.push(format!("roasts={}", enc(&ids.join(","))));
            }
            if let (true, Some(r)) = (ids.is_empty(), &a.roaster) {
                q.push(format!("roaster={}", enc(r)));
            }
            if let Some(w) = a.within {
                q.push(format!("within={w}"));
            }
            let path = format!("/api/reports/compare?{}", q.join("&"));
            fetch(api, out, &path, a.how, "compare.pdf").await
        }
        Kind::Cupping(a) => {
            let mut ids = Vec::new();
            for r in &a.roasts {
                ids.push(roast_id(api, r, a.roaster.as_deref()).await?);
            }
            let mut q = vec![a.how.query()];
            if !ids.is_empty() {
                q.push(format!("roasts={}", enc(&ids.join(","))));
            }
            if a.blank {
                q.push("blank=true".into());
            }
            if let Some(n) = a.samples {
                q.push(format!("samples={n}"));
            }
            if let Some(n) = a.per_page {
                q.push(format!("per_page={n}"));
            }
            if let Some(d) = &a.date {
                q.push(format!("date={}", enc(d)));
            }
            let path = format!("/api/reports/cupping?{}", q.join("&"));
            fetch(api, out, &path, a.how, "cupping.pdf").await
        }
        Kind::Inventory(a) => {
            let path = format!(
                "/api/reports/inventory?{}{}",
                a.how.query(),
                if a.count { "&count=true" } else { "" }
            );
            fetch(api, out, &path, a.how, "green-inventory.pdf").await
        }
        Kind::Coffee(a) => {
            let b = api.bean(&a.subject).await?;
            let id = b["id"].as_str().unwrap_or_default().to_string();
            let path = format!("/api/reports/coffee/{}?{}", enc(&id), a.how.query());
            fetch(api, out, &path, a.how, "coffee-report.pdf").await
        }
        Kind::Profile(a) => {
            let p = api.profile(&a.profile).await?;
            let id = p["id"].as_str().unwrap_or_default().to_string();
            let mut path = format!("/api/reports/profile/{}?{}", enc(&id), a.how.query());
            if let Some(r) = &a.roaster {
                let _ = write!(path, "&roaster={}", enc(r));
            }
            fetch(api, out, &path, a.how, "profile-report.pdf").await
        }
        Kind::Period(a) => {
            let path = format!("/api/reports/period?{}", a.query());
            fetch(api, out, &path, a.how, "period-report.pdf").await
        }
        Kind::Purchases(a) => {
            let path = format!("/api/reports/purchases?{}", a.query());
            fetch(api, out, &path, a.how, "purchases.pdf").await
        }
        Kind::Plan(a) => {
            let mut path = format!("/api/reports/plan?{}", a.how.query());
            if let Some(r) = &a.roaster {
                let _ = write!(path, "&roaster={}", enc(r));
            }
            fetch(api, out, &path, a.how, "roast-day.pdf").await
        }
        Kind::Trace(a) => {
            let (kind, id) = trace_subject(api, &a.subject, a.roaster.as_deref()).await?;
            let path = format!("/api/reports/trace?{kind}={}&{}", enc(&id), a.how.query());
            fetch(api, out, &path, a.how, "trace.pdf").await
        }
        Kind::Shopping(how) => {
            let path = format!("/api/reports/shopping?{}", how.query());
            fetch(api, out, &path, how, "shopping-list.pdf").await
        }
    }
}

/// The data is printed (or written with -o); a document is saved as a file
async fn fetch(api: &Api, out: &Out, path: &str, how: How, fallback: &str) -> Result<u8, Failure> {
    if matches!(how.format, Some(Format::Json)) && how.output.is_none() {
        let data = api.call(Method::GET, path, None).await?;
        out.emit(&data, |d| {
            serde_json::to_string_pretty(d).unwrap_or_default()
        });
        return Ok(code::OK);
    }
    save_file(api, out, path, how.output, fallback).await
}
