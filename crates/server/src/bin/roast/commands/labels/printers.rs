//! `roast labels printer …`: the label printers, what one says now, their calibrations per sheet.

use std::fmt::Write as _;
use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{enc, Api};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::out::{code, Failure, Out};

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct PrinterCmd {
    #[command(subcommand)]
    pub sub: Option<PrinterSub>,
    /// A printer's id (default the default printer)
    pub printer: Option<String>,
}

/// A printer's settings, for add and edit
#[derive(Args, Debug, Default)]
pub struct PrinterSet {
    /// What it is called
    #[arg(long)]
    pub name: Option<String>,
    /// The tray to ask for (`media-source`, e.g. tray-1); empty: the tray holding the sheet's
    /// size
    #[arg(long)]
    pub tray: Option<String>,
    /// monochrome or color; empty: monochrome unless a label has colour
    #[arg(long)]
    pub color: Option<String>,
    /// How labels go to it: auto (PDF, else PWG Raster, else URF), pdf, pwg, urf
    #[arg(long)]
    pub send: Option<String>,
    /// For ipps: the SHA-256 of its certificate, from `printer certificate` (check it on the
    /// printer's own page before trusting it)
    #[arg(long)]
    pub trust: Option<String>,
    /// Make it the default printer
    #[arg(long)]
    pub default: bool,
}

#[derive(Subcommand, Debug)]
pub enum PrinterSub {
    /// Every label printer, the default first: how labels go to it (pdf, pwg or urf), its
    /// calibrations per sheet, and what it said when last asked
    List,
    /// Add a printer by its address (ipp://host:631/ipp/print); it is asked what it can do
    Add {
        /// Its id: letters, digits, - and _
        id: String,
        /// ipp://host:631/ipp/print, or ipps://… with --trust
        address: String,
        #[command(flatten)]
        set: PrinterSet,
    },
    /// Change a printer: the flags given (an empty value clears one)
    Edit {
        id: String,
        /// A new address
        #[arg(long)]
        address: Option<String>,
        #[command(flatten)]
        set: PrinterSet,
    },
    /// Delete a printer and its calibrations; may be held for approval
    Rm {
        id: String,
        #[command(flatten)]
        wait: Wait,
    },
    /// Labels go to this printer unless another is named
    Default { id: String },
    /// Look for IPP printers on the server's network for 4 s (mDNS; only on the server's own
    /// network segment)
    Find,
    /// The certificate an ipps:// printer shows: its SHA-256, to check and then trust
    Certificate {
        /// ipps://host/ipp/print
        address: String,
    },
    /// A printer's offset for a sheet: --right/--down mm, or --first R,D and --last R,D
    /// measured on the calibration page (they must agree within 0.5 mm); --clear forgets it
    Calibrate {
        id: String,
        #[arg(long)]
        sheet: String,
        /// mm right (negative: left)
        #[arg(long, allow_hyphen_values = true)]
        right: Option<f64>,
        /// mm down (negative: up)
        #[arg(long, allow_hyphen_values = true)]
        down: Option<f64>,
        /// The first label's marks: R,D mm
        #[arg(long, allow_hyphen_values = true)]
        first: Option<String>,
        /// The last label's marks: R,D mm
        #[arg(long, allow_hyphen_values = true)]
        last: Option<String>,
        /// The tray it was measured from
        #[arg(long)]
        tray: Option<String>,
        /// Forget this sheet's calibration on this printer
        #[arg(long, conflicts_with_all = ["right", "down", "first", "last"])]
        clear: bool,
    },
}

/// "office-laser (default): HP Color LaserJet Pro M252dw, idle; pdf; 5163 +1.1/+0.5 mm"
fn printer_line(p: &Value) -> String {
    let info = &p["info"];
    let cal: Vec<String> = p["calibrations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            format!(
                "{} {:+}/{:+} mm",
                c["sheet"].as_str().unwrap_or("?"),
                c["x_mm"].as_f64().unwrap_or(0.0),
                c["y_mm"].as_f64().unwrap_or(0.0)
            )
        })
        .collect();
    let mut line = format!(
        "{}{}: {} at {}",
        p["id"].as_str().unwrap_or("?"),
        if p["is_default"] == true {
            " (default)"
        } else {
            ""
        },
        info["make_and_model"]
            .as_str()
            .or(p["name"].as_str())
            .unwrap_or("?"),
        p["url"].as_str().unwrap_or("?")
    );
    if let Some(s) = info["state"].as_str() {
        let _ = write!(line, ", {s}");
    }
    if let Some(s) = p["sends"].as_str() {
        let _ = write!(line, "; sends {s}");
    }
    if !cal.is_empty() {
        let _ = write!(line, "; {}", cal.join(", "));
    }
    if let Some(pr) = p["problem"].as_str() {
        let _ = write!(line, "\n  problem: {pr}");
    }
    line
}

fn set_body(set: &PrinterSet, mut b: Value) -> Value {
    for (k, v) in [
        ("name", &set.name),
        ("tray", &set.tray),
        ("color", &set.color),
        ("send", &set.send),
        ("fingerprint", &set.trust),
    ] {
        if let Some(v) = v {
            b[k] = if v.trim().is_empty() {
                Value::Null
            } else {
                json!(v.trim())
            };
        }
    }
    if set.default {
        b["is_default"] = json!(true);
    }
    b
}

/// "1.0,0.4" → [1.0, 0.4]
fn pair(s: &str) -> Result<Value, Failure> {
    let parts: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| Failure::new("USAGE", format!("{s:?} is R,D in mm, e.g. 1.0,-0.4")))?;
    match parts[..] {
        [r, d] => Ok(json!([r, d])),
        _ => Err(Failure::new(
            "USAGE",
            format!("{s:?} is R,D in mm, e.g. 1.0,-0.4"),
        )),
    }
}

pub async fn printer(api: &Api, out: &Out, c: PrinterCmd) -> Result<u8, Failure> {
    match c.sub {
        None => {
            // One printer asked now: the default (`/api/labels/printer`), or by id
            let p = match &c.printer {
                Some(id) => api.get(&format!("/api/label-printers/{}", enc(id))).await?,
                None => api.get("/api/labels/printer").await?,
            };
            out.emit(&p, |p| {
                if p["id"].is_string() {
                    return printer_line(p);
                }
                let info = &p["printer"];
                let mut lines = vec![match info["make_and_model"].as_str() {
                    Some(m) => format!("{m} at {}", info["url"].as_str().unwrap_or("?")),
                    None if p["configured"] == false => "no printer is set".to_string(),
                    None => "the printer did not answer".to_string(),
                }];
                if let Some(s) = info["state"].as_str() {
                    let yes = |k: &str| if info[k] == true { "yes" } else { "no" };
                    lines.push(format!(
                        "state {s}; PDF {}; label stock {}; actual size {}; colour {}",
                        yes("pdf"),
                        yes("labels_media"),
                        yes("actual_size"),
                        yes("color")
                    ));
                }
                if let Some(pr) = p["problem"].as_str() {
                    lines.push(format!("problem: {pr}"));
                }
                lines.join("\n")
            });
            Ok(if p["problem"].is_string() {
                code::FAILED
            } else {
                code::OK
            })
        }
        Some(PrinterSub::List) => {
            let list = api.get("/api/label-printers").await?;
            out.emit(&list, |l| {
                let lines: Vec<String> = l
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(printer_line)
                    .collect();
                if lines.is_empty() {
                    "no printer is set up: `roast labels printer add <id> ipp://host:631/ipp/print`"
                        .into()
                } else {
                    lines.join("\n")
                }
            });
            Ok(code::OK)
        }
        Some(PrinterSub::Add { id, address, set }) => {
            let mut b = json!({ "id": id, "url": address, "send": "auto" });
            b["name"] = json!(set.name.clone().unwrap_or_else(|| id.clone()));
            let b = set_body(&set, b);
            let p = api
                .call(Method::POST, "/api/label-printers", Some(&b))
                .await?;
            out.emit(&p, printer_line);
            Ok(if p["problem"].is_string() {
                code::FAILED
            } else {
                code::OK
            })
        }
        Some(PrinterSub::Edit { id, address, set }) => {
            let now = api
                .get(&format!("/api/label-printers/{}", enc(&id)))
                .await?;
            let mut b = json!({
                "name": now["name"], "url": now["url"], "tray": now["tray"], "color": now["color"],
                "send": now["send"], "fingerprint": now["fingerprint"], "is_default": now["is_default"],
            });
            if let Some(a) = address {
                b["url"] = json!(a);
            }
            let b = set_body(&set, b);
            let p = api
                .call(
                    Method::PUT,
                    &format!("/api/label-printers/{}", enc(&id)),
                    Some(&b),
                )
                .await?;
            out.emit(&p, printer_line);
            Ok(if p["problem"].is_string() {
                code::FAILED
            } else {
                code::OK
            })
        }
        Some(PrinterSub::Rm { id, wait }) => {
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/label-printers/{}", enc(&id)),
                    None,
                    Duration::from_secs(30),
                )
                .await?;
            held_or_done(api, out, reply, wait, |_| format!("deleted printer {id}")).await
        }
        Some(PrinterSub::Default { id }) => {
            let p = api
                .call(
                    Method::POST,
                    &format!("/api/label-printers/{}/default", enc(&id)),
                    None,
                )
                .await?;
            out.emit(&p, printer_line);
            Ok(code::OK)
        }
        Some(PrinterSub::Find) => {
            let f = api.get("/api/label-printers/find").await?;
            out.emit(&f, |f| {
                let found: Vec<String> = f["printers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| {
                        format!(
                            "{}: {}{}{}",
                            p["name"].as_str().unwrap_or("?"),
                            p["url"].as_str().unwrap_or("?"),
                            p["model"]
                                .as_str()
                                .map(|m| format!(" ({m})"))
                                .unwrap_or_default(),
                            if p["tls"] == true { ", ipps too" } else { "" }
                        )
                    })
                    .collect();
                if found.is_empty() {
                    format!("none found. {}", f["note"].as_str().unwrap_or(""))
                } else {
                    found.join("\n")
                }
            });
            Ok(code::OK)
        }
        Some(PrinterSub::Certificate { address }) => {
            let c = api
                .call(
                    Method::POST,
                    "/api/label-printers/certificate",
                    Some(&json!({ "url": address })),
                )
                .await?;
            out.emit(&c, |c| {
                format!(
                    "SHA-256 {}: check it on the printer's own page, then `roast labels printer add <id> {} --trust {}`",
                    c["fingerprint"].as_str().unwrap_or("?"),
                    c["url"].as_str().unwrap_or("?"),
                    c["fingerprint"].as_str().unwrap_or("?")
                )
            });
            Ok(code::OK)
        }
        Some(PrinterSub::Calibrate {
            id,
            sheet,
            right,
            down,
            first,
            last,
            tray,
            clear,
        }) => {
            let path = format!(
                "/api/label-printers/{}/calibration/{}",
                enc(&id),
                enc(&sheet)
            );
            if clear {
                api.call(Method::DELETE, &path, None).await?;
                let p = api
                    .get(&format!("/api/label-printers/{}", enc(&id)))
                    .await?;
                out.emit(&p, printer_line);
                return Ok(code::OK);
            }
            let mut b = json!({});
            match (first, right, down) {
                (Some(f), _, _) => {
                    b["first"] = pair(&f)?;
                    if let Some(l) = last {
                        b["last"] = pair(&l)?;
                    }
                }
                (None, Some(r), Some(d)) => {
                    b["x_mm"] = json!(r);
                    b["y_mm"] = json!(d);
                }
                _ => {
                    return Err(Failure::new(
                        "USAGE",
                        "give --right and --down, or --first R,D (and --last R,D)",
                    ))
                }
            }
            if let Some(t) = tray {
                b["tray"] = json!(t);
            }
            let p = api.call(Method::PUT, &path, Some(&b)).await?;
            out.emit(&p, printer_line);
            Ok(code::OK)
        }
    }
}
