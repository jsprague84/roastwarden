//! `roast labels design check`: one label laid out on the server's renderer; Typst's errors with
//! their lines, or the layout report, and a small PNG when asked.

use std::fmt::Write as _;
use std::path::PathBuf;

use base64::Engine as _;
use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};

use super::{body, design_arg, Fill};
use crate::api::Api;
use crate::commands::roasts::roast_id;
use crate::out::{code, Failure, Out};

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// A design's .typ file (a draft), or a saved design's id
    pub design: String,
    /// Fill it from this roast (an id or `latest`) instead of sample data
    #[arg(long)]
    pub roast: Option<String>,
    /// With `--roast latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// A coffee (bean, by name or id) on sample roast data instead: its name, origin, process
    /// and notes, and its own brand
    #[arg(long, conflicts_with = "roast")]
    pub bean: Option<String>,
    #[command(flatten)]
    pub fill: Fill,
    /// Also write the label as a PNG (default label-check.png) to look at
    #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "label-check.png")]
    pub png: Option<PathBuf>,
    /// The PNG's resolution (36 to 200 dots per inch; 100 reads well and is small)
    #[arg(long, default_value_t = 100)]
    pub dpi: u32,
    /// Also lay out this design (`sam-gift@2`, a file) on the same data and say what changed:
    /// lines added, gone, moved or resized (mm), issues new and gone
    #[arg(long, value_name = "DESIGN")]
    pub diff: Option<String>,
}

pub async fn check(api: &Api, out: &Out, a: CheckArgs) -> Result<u8, Failure> {
    let roast = match &a.roast {
        Some(r) => Some(roast_id(api, r, a.roaster.as_deref()).await?),
        None => None,
    };
    let mut b = body(&a.fill, roast)?;
    if let Some(bean) = &a.bean {
        let found = api.bean(bean).await?;
        b["bean"] = found["id"].clone();
    }
    let shown = design_arg(&mut b, &a.design)?;
    if a.png.is_some() {
        b["png"] = json!(a.dpi);
    }
    let mut reply = api
        .call(Method::POST, "/api/labels/check", Some(&b))
        .await?;
    // The picture goes to a file; the answer names it
    if let (Some(file), Some(png)) = (&a.png, reply["png"].as_str()) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(png)
            .map_err(|e| Failure::new("BAD_RESPONSE", format!("the PNG: {e}")))?;
        std::fs::write(file, &bytes).map_err(|e| {
            Failure::new("FAILED", format!("could not write {}: {e}", file.display()))
        })?;
        reply["png"] = json!(file.display().to_string());
    }
    if reply["design"] == "source" {
        reply["design"] = json!(shown);
    }
    if let Some(other) = &a.diff {
        let mut ob = body(&a.fill, roast_for_diff(api, &a).await?)?;
        if let Some(bean) = &b.get("bean") {
            ob["bean"] = (*bean).clone();
        }
        design_arg(&mut ob, other)?;
        let before = api
            .call(Method::POST, "/api/labels/check", Some(&ob))
            .await?;
        reply["diff"] = layout_diff(&before, &reply, other);
    }
    let ok = reply["ok"] == true;
    let compiles = reply["errors"]
        .as_array()
        .is_none_or(std::vec::Vec::is_empty);
    out.emit(&reply, check_text);
    Ok(if ok {
        code::OK
    } else if compiles {
        code::FAILED
    } else {
        code::INVALID
    })
}

async fn roast_for_diff(api: &Api, a: &CheckArgs) -> Result<Option<String>, Failure> {
    match &a.roast {
        Some(r) => Ok(Some(roast_id(api, r, a.roaster.as_deref()).await?)),
        None => Ok(None),
    }
}

/// What changed from `before` (another design or version, on the same data) to `after`:
/// lines added and gone (by their text), moved or resized by more than 0.2 mm, issues new
/// and gone
fn layout_diff(before: &Value, after: &Value, against: &str) -> Value {
    let lines =
        |v: &Value| -> Vec<Value> { v["report"]["lines"].as_array().cloned().unwrap_or_default() };
    let (old, new) = (lines(before), lines(after));
    let text = |l: &Value| l["text"].as_str().unwrap_or("").to_string();
    let num = |l: &Value, k: &str| l[k].as_f64().unwrap_or(0.0);
    let mut changed = Vec::new();
    let mut added = Vec::new();
    for n in &new {
        match old.iter().find(|o| text(o) == text(n)) {
            None => added.push(json!(text(n))),
            Some(o) => {
                let d = |k: &str| ((num(n, k) - num(o, k)) * 10.0).round() / 10.0;
                let (dx, dy, dw, dh, dpt) = (d("x"), d("y"), d("w"), d("h"), d("pt"));
                if dx.abs() > 0.2
                    || dy.abs() > 0.2
                    || dw.abs() > 0.2
                    || dh.abs() > 0.2
                    || dpt.abs() > 0.05
                {
                    let mut c = json!({ "text": text(n) });
                    for (k, v) in [("dx", dx), ("dy", dy), ("dw", dw), ("dh", dh), ("dpt", dpt)] {
                        if v != 0.0 {
                            c[k] = json!(v);
                        }
                    }
                    changed.push(c);
                }
            }
        }
    }
    let gone: Vec<Value> = old
        .iter()
        .filter(|o| !new.iter().any(|n| text(n) == text(o)))
        .map(|o| json!(text(o)))
        .collect();
    let issues = |v: &Value| -> Vec<String> {
        v["report"]["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i["message"].as_str().map(str::to_string))
            .chain(
                v["errors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["message"].as_str().map(|m| format!("error: {m}"))),
            )
            .collect()
    };
    let (io, inew) = (issues(before), issues(after));
    json!({
        "against": against,
        "lines_added": added,
        "lines_gone": gone,
        "lines_changed": changed,
        "issues_new": inew.iter().filter(|i| !io.contains(i)).collect::<Vec<_>>(),
        "issues_gone": io.iter().filter(|i| !inew.contains(i)).collect::<Vec<_>>(),
    })
}

/// A check, for a person: errors, or issues, lines and fields.
fn check_text(r: &Value) -> String {
    let mut lines = Vec::new();
    let errors = r["errors"].as_array().cloned().unwrap_or_default();
    if !errors.is_empty() {
        lines.push(format!(
            "{} does not compile:",
            r["design"].as_str().unwrap_or("?")
        ));
        for e in errors {
            let place = match (e["file"].as_str(), e["line"].as_u64()) {
                (Some(f), Some(l)) => format!("{f}:{l}: "),
                _ => String::new(),
            };
            let hints: String = e["hints"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|h| h.as_str())
                .fold(String::new(), |mut s, h| {
                    let _ = write!(s, " (hint: {h})");
                    s
                });
            lines.push(format!(
                "  {place}{}{hints}",
                e["message"].as_str().unwrap_or("")
            ));
        }
        return lines.join("\n");
    }
    let rep = &r["report"];
    lines.push(format!(
        "{} on {} ({} × {} mm{}), {} data: {}",
        r["design"].as_str().unwrap_or("?"),
        r["sheet"].as_str().unwrap_or("?"),
        rep["w"],
        rep["h"],
        if rep["round"] == true { ", round" } else { "" },
        r["data"].as_str().unwrap_or("sample"),
        if r["ok"] == true { "ok" } else { "issues" }
    ));
    for i in rep["issues"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {}: {}",
            i["kind"].as_str().unwrap_or("?"),
            i["message"].as_str().unwrap_or("")
        ));
    }
    if let Some(ret) = r.get("retail") {
        lines.push(format!(
            "retail ({}, net weight letters {} mm): {}",
            ret["bag"].as_str().unwrap_or("?"),
            ret["min_net_mm"],
            if ret["ok"] == true {
                "nothing to fix"
            } else {
                "see the retail_ issues above"
            }
        ));
        for p in ret["passed"].as_array().into_iter().flatten() {
            lines.push(format!("  found: {}", p.as_str().unwrap_or("")));
        }
        lines.push("  no check can know:".into());
        for c in ret["cannot_check"].as_array().into_iter().flatten() {
            lines.push(format!("    {}", c.as_str().unwrap_or("")));
        }
    }
    if let Some(d) = r.get("diff") {
        lines.push(format!("against {}:", d["against"].as_str().unwrap_or("?")));
        for (k, what) in [
            ("lines_added", "added"),
            ("lines_gone", "gone"),
            ("issues_new", "new issue"),
            ("issues_gone", "issue gone"),
        ] {
            for v in d[k].as_array().into_iter().flatten() {
                lines.push(format!("  {what}: {}", v.as_str().unwrap_or("")));
            }
        }
        for c in d["lines_changed"].as_array().into_iter().flatten() {
            lines.push(format!("  changed: {c}"));
        }
    }
    lines.push("lines (mm from the top left):".into());
    for l in rep["lines"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {:>5} pt {:<26} at {}, {} size {} × {}  {}",
            l["pt"],
            l["font"].as_str().unwrap_or(""),
            l["x"],
            l["y"],
            l["w"],
            l["h"],
            l["text"]
        ));
    }
    for i in rep["images"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {} at {}, {} size {} × {}{}",
            i["kind"].as_str().unwrap_or("image"),
            i["x"],
            i["y"],
            i["w"],
            i["h"],
            i["module"]
                .as_f64()
                .map(|m| format!(" (squares {m} mm)"))
                .unwrap_or_default()
        ));
    }
    let list = |k: &str| {
        rep["fields"][k]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    lines.push(format!("fields shown: {}", list("shown")));
    let missing = list("missing");
    if !missing.is_empty() {
        lines.push(format!("fields with a value but not printed: {missing}"));
    }
    if let Some(p) = r["params"].as_object().or(rep["params"].as_object())
        && !p.is_empty() {
            lines.push(format!("params: {}", Value::Object(p.clone())));
        }
    if let Some(png) = r["png"].as_str() {
        lines.push(format!("png: {png}"));
    }
    lines.join("\n")
}
