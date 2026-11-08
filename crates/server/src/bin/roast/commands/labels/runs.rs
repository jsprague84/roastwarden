//! `roast labels run|runs|reprint|sheets`: print runs made, sent to a printer or printed by hand,
//! reprinted in part; each sheet's next free label.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use super::{body, read_file, tz, Fill, Via};
use crate::api::{enc, Api};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::commands::records::save_file;
use crate::commands::roasts::roast_id;
use crate::out::{code, Failure, Out};

#[derive(Args, Debug)]
pub struct RunArgs {
    /// A roast's labels on the sheet (an id or `latest`); repeat it for more roasts, in order
    #[arg(long = "roast", value_name = "ROAST")]
    pub roasts: Vec<String>,
    /// With `--roast latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// Each roast's labels in detail instead, a JSON array or @FILE:
    /// [{"roast": "latest", "count": 2, "design": "gift", "message": "for Sam",
    /// "params": {"art": "cup"}, "brand": "home", "weight_g": 250, "notes": "…",
    /// "hide": ["batch"]}]
    #[arg(long, conflicts_with = "roasts")]
    pub items: Option<String>,
    /// Labels for each --roast
    #[arg(long, default_value_t = 1)]
    pub count: usize,
    /// The design for each --roast (default its coffee's own, else plain)
    #[arg(long)]
    pub design: Option<String>,
    #[command(flatten)]
    pub fill: Fill,
    /// The first label to print on, from 1 (default the sheet's next free label)
    #[arg(long)]
    pub start: Option<usize>,
    /// A note kept with the run ("gift baskets")
    #[arg(long)]
    pub note: Option<String>,
    /// Always a new run: by default the same labels as a run made in the last 24 hours and
    /// not yet printed answer that run (`reused: true`)
    #[arg(long)]
    pub new: bool,
    /// Take the bags off each roast: --count bags of the label's weight (--weight), packed
    /// whole or --ground (docs/roasted.md, "Packed bags"); refused when that much is not left
    #[arg(long)]
    pub pack: bool,
    /// Send it to the label printer instead of writing the PDF
    #[arg(long, conflicts_with = "output")]
    pub print: bool,
    /// The PDF's file (default labels-run-<id>.pdf here); `-` for stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[command(flatten)]
    pub via: Via,
    #[command(flatten)]
    pub wait: Wait,
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct RunsCmd {
    #[command(subcommand)]
    pub sub: Option<RunsSub>,
    /// How many, newest first
    #[arg(long, default_value_t = 10)]
    pub limit: u32,
}

#[derive(Subcommand, Debug)]
pub enum RunsSub {
    /// One run: each item (the roast, design@version, copies, what its labels carried) and
    /// the printer's job states with their times
    Show {
        /// The run's id, or `latest`
        run: String,
    },
    /// A run's PDF, as it was made
    Pdf {
        /// The run's id, or `latest`
        run: String,
        /// The file (default labels-run-<id>.pdf here); `-` for stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print a run on the label printer (a token's print waits for the owner's approval:
    /// exit 3, with the sheet to load and the first label)
    Print {
        /// The run's id, or `latest`
        run: String,
        #[command(flatten)]
        via: Via,
        #[command(flatten)]
        wait: Wait,
    },
    /// A run printed by hand from its PDF: mark it printed, so the sheet's next free label
    /// moves on
    Done {
        /// The run's id, or `latest`
        run: String,
    },
}

#[derive(Args, Debug)]
pub struct ReprintArgs {
    /// The run's id, or `latest`
    pub run: String,
    /// The first label to print on (default the sheet's next free label)
    #[arg(long)]
    pub start: Option<usize>,
    /// Only these labels of the run, from 1 in its order: --labels 3,4
    #[arg(long, value_delimiter = ',')]
    pub labels: Vec<usize>,
    /// A note kept with the new run
    #[arg(long)]
    pub note: Option<String>,
    /// Send it to the label printer instead of writing the PDF
    #[arg(long, conflicts_with = "output")]
    pub print: bool,
    /// The PDF's file (default labels-run-<id>.pdf here); `-` for stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[command(flatten)]
    pub via: Via,
    #[command(flatten)]
    pub wait: Wait,
}

/// A run's id: `latest` is the newest run
async fn run_ref(api: &Api, run: &str) -> Result<String, Failure> {
    if run != "latest" {
        return Ok(run.to_string());
    }
    let list = api.get("/api/label-runs?limit=1").await?;
    list[0]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Failure::new("NOT_FOUND", "there is no label run yet"))
}

/// "Run 1f2e3d4c: 6 labels (Guji (#7) ×2, …) on 5163 from label 1; made"
fn run_line(r: &Value) -> String {
    format!(
        "Run {}: {} label(s) ({}) on {} from label {}; {}{}",
        r["id"]
            .as_str()
            .unwrap_or("?")
            .chars()
            .take(8)
            .collect::<String>(),
        r["labels"],
        r["summary"].as_str().unwrap_or(""),
        r["sheet"].as_str().unwrap_or("?"),
        r["start"],
        r["state"].as_str().unwrap_or("?"),
        r["created_at"]
            .as_str()
            .map(|t| format!(" ({t})"))
            .unwrap_or_default()
    )
}

/// After making a run: print it, or write its PDF
async fn deliver(
    api: &Api,
    out: &Out,
    made: Value,
    print: Option<&Via>,
    output: Option<PathBuf>,
    wait: Wait,
) -> Result<u8, Failure> {
    let id = made["id"].as_str().unwrap_or_default().to_string();
    if let Some(via) = print {
        let reply = api
            .send(
                Method::POST,
                &format!("/api/label-runs/{}/print", enc(&id)),
                Some(&via.body()),
                Duration::from_secs(120),
            )
            .await?;
        return held_or_done(api, out, reply, wait, |p| {
            format!(
                "{} ({}, job {}: {} label(s) on {} from label {}; run {})",
                p["message"].as_str().unwrap_or("sent"),
                p["printer"].as_str().unwrap_or("the printer"),
                p["job_id"],
                p["count"],
                p["sheet"].as_str().unwrap_or("?"),
                p["start"],
                p["run"].as_str().unwrap_or("?")
            )
        })
        .await;
    }
    let file = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "labels-run-{}.pdf",
            id.chars().take(8).collect::<String>()
        ))
    });
    let (bytes, _) = api
        .download(&format!("/api/label-runs/{}/pdf", enc(&id)))
        .await?;
    if file == Path::new("-") {
        use std::io::Write;
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|e| Failure::new("FAILED", e.to_string()))?;
        return Ok(code::OK);
    }
    std::fs::write(&file, &bytes)
        .map_err(|e| Failure::new("FAILED", format!("could not write {}: {e}", file.display())))?;
    let mut answer = json!({
        "run": id,
        "file": file.display().to_string(),
        "bytes": bytes.len(),
        "labels": made["labels"],
        "sheet": made["sheet"],
        "start": made["start"],
        "summary": made["summary"],
        "hint": "print it at Actual size on label stock; then `roast labels runs done <run>` moves the sheet's next free label on",
    });
    if made["reused"] == true {
        // The same labels as a run not yet printed: that run again, not a second copy
        answer["reused"] = json!(true);
    }
    out.emit(&answer, |a| {
        format!(
            "wrote {}{}: {} label(s) ({}) on {} from label {}. Print it at Actual size; then `roast labels runs done {}`.",
            a["file"].as_str().unwrap_or("?"),
            if a["reused"] == true {
                " (the same run as before, not yet printed)"
            } else {
                ""
            },
            a["labels"],
            a["summary"].as_str().unwrap_or(""),
            a["sheet"].as_str().unwrap_or("?"),
            a["start"],
            a["run"].as_str().unwrap_or("?")
        )
    });
    Ok(code::OK)
}

pub async fn run(api: &Api, out: &Out, a: RunArgs) -> Result<u8, Failure> {
    let mut items: Vec<Value> = match &a.items {
        Some(text) => {
            let raw = match text.strip_prefix('@') {
                Some(file) => read_file(Path::new(file))?,
                None => text.clone(),
            };
            let v: Value = serde_json::from_str(&raw)
                .map_err(|e| Failure::new("USAGE", format!("--items is a JSON array: {e}")))?;
            v.as_array()
                .cloned()
                .ok_or_else(|| Failure::new("USAGE", "--items is a JSON array of items"))?
        }
        None => {
            let shared = body(&a.fill, None)?;
            a.roasts
                .iter()
                .map(|r| {
                    let mut it =
                        json!({ "roast": r, "count": a.count, "params": shared["params"] });
                    for k in [
                        "weight_g", "form", "grind", "message", "notes", "brand", "hide", "retail",
                        "qr",
                    ] {
                        if !shared[k].is_null() {
                            it[k] = shared[k].clone();
                        }
                    }
                    if a.pack {
                        it["pack"] = json!(true);
                    }
                    if let Some(d) = &a.design {
                        it["design"] = json!(d);
                    }
                    it
                })
                .collect()
        }
    };
    if items.is_empty() {
        return Err(Failure::new(
            "USAGE",
            "name the roasts: --roast R (repeat for more) or --items '[{\"roast\": \"latest\"}]'",
        ));
    }
    for it in &mut items {
        let r = it["roast"].as_str().unwrap_or("latest").to_string();
        it["roast"] = json!(roast_id(api, &r, a.roaster.as_deref()).await?);
    }
    let mut b = json!({ "items": items, "tz": tz() });
    if let Some(s) = &a.fill.sheet {
        b["sheet"] = json!(s);
    }
    if let Some(s) = a.start {
        b["start"] = json!(s);
    }
    if let Some(n) = &a.note {
        b["note"] = json!(n);
    }
    if let Some(p) = &a.via.printer {
        b["printer"] = json!(p);
    }
    if a.new {
        b["new"] = json!(true);
    }
    let made = api.call(Method::POST, "/api/label-runs", Some(&b)).await?;
    deliver(api, out, made, a.print.then_some(&a.via), a.output, a.wait).await
}

pub async fn runs(api: &Api, out: &Out, c: RunsCmd) -> Result<u8, Failure> {
    match c.sub {
        None => {
            let list = api
                .get(&format!("/api/label-runs?limit={}", c.limit))
                .await?;
            out.emit(&list, |l| {
                let lines: Vec<String> = l.as_array().into_iter().flatten().map(run_line).collect();
                if lines.is_empty() {
                    "no label runs yet".into()
                } else {
                    lines.join("\n")
                }
            });
            Ok(code::OK)
        }
        Some(RunsSub::Show { run }) => {
            let id = run_ref(api, &run).await?;
            let r = api.get(&format!("/api/label-runs/{}", enc(&id))).await?;
            out.emit(&r, |r| {
                let mut lines = vec![run_line(r)];
                for it in r["items"].as_array().into_iter().flatten() {
                    lines.push(format!(
                        "  {} ×{} in {}{}",
                        it["name"].as_str().unwrap_or("?"),
                        it["count"],
                        it["design"].as_str().unwrap_or("?"),
                        it["data"]["message"]
                            .as_str()
                            .map(|m| format!(", \"{m}\""))
                            .unwrap_or_default()
                    ));
                }
                for st in r["states"].as_array().into_iter().flatten() {
                    lines.push(format!(
                        "  {} {}",
                        st["at"].as_str().unwrap_or("?"),
                        st["state"].as_str().unwrap_or("?")
                    ));
                }
                lines.join("\n")
            });
            Ok(code::OK)
        }
        Some(RunsSub::Pdf { run, output }) => {
            let id = run_ref(api, &run).await?;
            let name = format!("labels-run-{}.pdf", id.chars().take(8).collect::<String>());
            save_file(
                api,
                out,
                &format!("/api/label-runs/{}/pdf", enc(&id)),
                Some(output.unwrap_or_else(|| PathBuf::from(&name))),
                &name,
            )
            .await
        }
        Some(RunsSub::Print { run, via, wait }) => {
            let id = run_ref(api, &run).await?;
            let made = api.get(&format!("/api/label-runs/{}", enc(&id))).await?;
            deliver(api, out, made, Some(&via), None, wait).await
        }
        Some(RunsSub::Done { run }) => {
            let id = run_ref(api, &run).await?;
            let r = api
                .call(
                    Method::POST,
                    &format!("/api/label-runs/{}/done", enc(&id)),
                    None,
                )
                .await?;
            out.emit(&r, run_line);
            Ok(code::OK)
        }
    }
}

pub async fn reprint(api: &Api, out: &Out, a: ReprintArgs) -> Result<u8, Failure> {
    let id = run_ref(api, &a.run).await?;
    let mut b = json!({});
    if let Some(s) = a.start {
        b["start"] = json!(s);
    }
    if !a.labels.is_empty() {
        b["labels"] = json!(a.labels);
    }
    if let Some(n) = &a.note {
        b["note"] = json!(n);
    }
    let made = api
        .call(
            Method::POST,
            &format!("/api/label-runs/{}/reprint", enc(&id)),
            Some(&b),
        )
        .await?;
    deliver(api, out, made, a.print.then_some(&a.via), a.output, a.wait).await
}

pub async fn sheets(api: &Api, out: &Out, set: Option<String>) -> Result<u8, Failure> {
    if let Some(s) = set {
        let Some((sheet, n)) = s.split_once('=') else {
            return Err(Failure::new("USAGE", "--set takes SHEET=N, e.g. 5163=1"));
        };
        let next: i64 = n
            .trim()
            .parse()
            .map_err(|_| Failure::new("USAGE", format!("{n:?} is not a label number")))?;
        api.call(
            Method::PUT,
            &format!("/api/label-sheets/{}", enc(sheet.trim())),
            Some(&json!({ "next": next })),
        )
        .await?;
    }
    let list = api.get("/api/label-sheets").await?;
    out.emit(&list, |l| {
        l.as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                format!(
                    "{:<8} next free label {} of {}  ({})",
                    s["sheet"].as_str().unwrap_or("?"),
                    s["next_label"],
                    s["per_sheet"],
                    s["name"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(code::OK)
}
