//! Orders from receipts (docs/green-coffee.md, "Orders"; docs/cli.md): `roast order import`
//! checks a receipt read into JSON and records it with the receipt files; `shipped`,
//! `received`, `cancel`, `receipt`, `show`, `rm`; `roast orders` lists them. An order is
//! named by its number (`--supplier` when two suppliers share one) or its id.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::api::{enc, Api, Upload};
use crate::cli::Wait;
use crate::commands::approvals::held_or_done;
use crate::out::{self, code, Failure, Out};
use crate::text::{grams, parse_weight};

#[derive(Subcommand, Debug)]
pub enum OrderCommand {
    /// A receipt read into an order (JSON: supplier, order_number, total, lines; docs/cli.md):
    /// checked (the order number once per supplier, the totals to the cent, each line's bean),
    /// then recorded with the receipt files. --dry-run shows the check and writes nothing.
    /// Only `received` adds the coffee to stock
    Import {
        /// The order as JSON (`-` reads it from stdin)
        file: String,
        /// The receipt it was read from, kept with the order (an .eml, .html, .pdf or image);
        /// repeat for each. `shipping=FILE` for a shipping notice, `invoice=FILE`, `other=FILE`
        #[arg(long = "receipt", value_name = "[KIND=]FILE")]
        receipts: Vec<String>,
        /// Check only: totals, a duplicate, each line's bean; nothing is written
        #[arg(long)]
        dry_run: bool,
    },
    /// One order: its lines, totals, status and receipts
    Show(OrderRef),
    /// It shipped: carrier and tracking from the shipping notice
    Shipped {
        #[command(flatten)]
        order: OrderRef,
        #[arg(long)]
        carrier: Option<String>,
        #[arg(long)]
        tracking: Option<String>,
        /// YYYY-MM-DD (default today)
        #[arg(long)]
        date: Option<String>,
    },
    /// It came: each coffee line becomes a purchase with its share of the discount, shipping
    /// and tax. --weight 2=440g for a line that arrived different, --bean 2=<bean> to put a
    /// line on another bean
    Received {
        #[command(flatten)]
        order: OrderRef,
        /// YYYY-MM-DD (default today)
        #[arg(long)]
        date: Option<String>,
        /// LINE=WEIGHT: what a line (its position) weighed when it came
        #[arg(long = "weight", value_name = "LINE=WEIGHT")]
        weights: Vec<String>,
        /// LINE=BEAN: put a line on this bean (id or name)
        #[arg(long = "bean", value_name = "LINE=BEAN")]
        beans: Vec<String>,
    },
    /// It will not come: cancelled, its shopping-list entries wanted again
    Cancel(OrderRef),
    /// An order's receipts: list them; give one (its number in the list, or id) to save it
    /// (--out); --add FILE keeps another; --rm removes one. Private: an inventory or admin token
    Receipt {
        #[command(flatten)]
        order: OrderRef,
        /// The receipt to save: 1, 2, … as listed, or its id
        receipt: Option<String>,
        /// Where to save it (default: its own name, here)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Keep this file with the order
        #[arg(long, conflicts_with_all = ["receipt", "rm"])]
        add: Option<PathBuf>,
        /// With --add: confirmation (default), shipping, invoice or other
        #[arg(long, requires = "add")]
        kind: Option<String>,
        /// Remove this receipt (1, 2, … or its id)
        #[arg(long, conflicts_with = "receipt")]
        rm: Option<String>,
    },
    /// Remove an order and its receipts (a received one's purchases too); may wait for the
    /// owner's approval
    Rm {
        #[command(flatten)]
        order: OrderRef,
        #[command(flatten)]
        wait: Wait,
    },
}

/// An order by its number, or its id
#[derive(Args, Debug)]
pub struct OrderRef {
    /// The supplier's order number, or the order's id
    pub order: String,
    /// When two suppliers share that number
    #[arg(long)]
    pub supplier: Option<String>,
}

#[derive(Args, Debug)]
pub struct OrdersArgs {
    /// Every order, received and cancelled too
    #[arg(long, conflicts_with = "status")]
    pub all: bool,
    /// ordered, shipped, received or cancelled
    #[arg(long)]
    pub status: Option<String>,
}

pub async fn run(api: &Api, out: &Out, cmd: OrderCommand) -> Result<u8, Failure> {
    match cmd {
        OrderCommand::Import {
            file,
            receipts,
            dry_run,
        } => import(api, out, &file, &receipts, dry_run).await,
        OrderCommand::Show(r) => {
            let o = find(api, &r).await?;
            out.emit(&o, order_text);
            Ok(code::OK)
        }
        OrderCommand::Shipped {
            order,
            carrier,
            tracking,
            date,
        } => {
            let o = find(api, &order).await?;
            let mut body = json!({ "carrier": carrier, "tracking": tracking, "at": date });
            out::slim(&mut body);
            let r = api
                .call(
                    Method::POST,
                    &format!("/api/orders/{}/shipped", id(&o)),
                    Some(&body),
                )
                .await?;
            out.emit(&r, order_text);
            Ok(code::OK)
        }
        OrderCommand::Received {
            order,
            date,
            weights,
            beans,
        } => {
            let o = find(api, &order).await?;
            let mut lines: Vec<Value> = Vec::new();
            for w in &weights {
                let (line, g) = w.split_once('=').ok_or_else(|| {
                    Failure::new("USAGE", format!("'{w}': use LINE=WEIGHT, e.g. 2=440g"))
                })?;
                let g = parse_weight(g).map_err(|m| Failure::new("USAGE", m))?;
                lines.push(json!({ "line": line.trim(), "weight_g": g }));
            }
            for b in &beans {
                let (line, bean) = b
                    .split_once('=')
                    .ok_or_else(|| Failure::new("USAGE", format!("'{b}': use LINE=BEAN")))?;
                let bean = api.bean(bean.trim()).await?;
                lines.push(json!({ "line": line.trim(), "bean_id": bean["id"] }));
            }
            let mut body = json!({ "at": date, "lines": lines });
            out::slim(&mut body);
            let r = api
                .call(
                    Method::POST,
                    &format!("/api/orders/{}/received", id(&o)),
                    Some(&body),
                )
                .await?;
            out.emit(&r, |r| format!("received:\n{}", order_text(r)));
            Ok(code::OK)
        }
        OrderCommand::Cancel(r) => {
            let o = find(api, &r).await?;
            let r = api
                .call(
                    Method::POST,
                    &format!("/api/orders/{}/cancel", id(&o)),
                    None,
                )
                .await?;
            out.emit(&r, order_text);
            Ok(code::OK)
        }
        OrderCommand::Receipt {
            order,
            receipt,
            out: path,
            add,
            kind,
            rm,
        } => {
            let o = find(api, &order).await?;
            if let Some(file) = add {
                let r = upload(
                    api,
                    &id(&o),
                    &file,
                    kind.as_deref().unwrap_or("confirmation"),
                )
                .await?;
                out.emit(&r, |r| format!("kept {}", receipt_line(r)));
                return Ok(code::OK);
            }
            if let Some(which) = rm {
                let rid = receipt_id(&o, &which)?;
                api.send(
                    Method::DELETE,
                    &format!("/api/orders/{}/receipts/{}", id(&o), enc(&rid)),
                    None,
                    Duration::from_secs(30),
                )
                .await?;
                out.emit(&json!({ "removed": rid }), |_| {
                    format!("removed receipt {rid}")
                });
                return Ok(code::OK);
            }
            let Some(which) = receipt else {
                let list = o["receipts"].clone();
                out.emit(&list, |l| {
                    let rows: Vec<String> = l
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(|(i, r)| format!("{}  {}", i + 1, receipt_line(r)))
                        .collect();
                    if rows.is_empty() {
                        "no receipts kept".into()
                    } else {
                        rows.join("\n")
                    }
                });
                return Ok(code::OK);
            };
            let rid = receipt_id(&o, &which)?;
            let (bytes, name) = api
                .download(&format!("/api/orders/{}/receipts/{}", id(&o), enc(&rid)))
                .await?;
            let to = path.unwrap_or_else(|| {
                PathBuf::from(name.as_deref().map_or_else(
                    || format!("receipt-{rid}"),
                    |n| n.rsplit(['/', '\\']).next().unwrap_or(n).to_string(),
                ))
            });
            std::fs::write(&to, &bytes).map_err(|e| {
                Failure::new("FAILED", format!("could not write {}: {e}", to.display()))
            })?;
            let saved = json!({ "saved": to.display().to_string(), "size_bytes": bytes.len() });
            out.emit(&saved, |s| {
                format!(
                    "saved {} ({} bytes)",
                    s["saved"].as_str().unwrap_or("?"),
                    bytes.len()
                )
            });
            Ok(code::OK)
        }
        OrderCommand::Rm { order, wait } => {
            let o = find(api, &order).await?;
            let reply = api
                .send(
                    Method::DELETE,
                    &format!("/api/orders/{}", id(&o)),
                    None,
                    Duration::from_secs(60),
                )
                .await?;
            let what = order_name(&o);
            held_or_done(api, out, reply, wait, move |_| format!("removed {what}")).await
        }
    }
}

pub async fn list(api: &Api, out: &Out, a: OrdersArgs) -> Result<u8, Failure> {
    let status = if a.all {
        "all".to_string()
    } else {
        a.status.unwrap_or_else(|| "open".into())
    };
    let list = api
        .get(&format!("/api/orders?status={}", enc(&status)))
        .await?;
    out.emit(&list, |l| {
        let rows: Vec<String> = l
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| {
                format!(
                    "{}  {}  {} lines, {} coffee  {}  {}",
                    day(&o["placed_at"]),
                    order_name(o),
                    o["lines"].as_array().map_or(0, Vec::len),
                    grams(o["coffee_g"].as_f64().unwrap_or(0.0)),
                    money(&o["total"], &o["currency"]),
                    o["status"].as_str().unwrap_or("?"),
                )
            })
            .collect();
        if rows.is_empty() {
            format!("no {status} orders")
        } else {
            rows.join("\n")
        }
    });
    Ok(code::OK)
}

fn id(o: &Value) -> String {
    enc(o["id"].as_str().unwrap_or_default())
}

fn order_name(o: &Value) -> String {
    format!(
        "{} order {}",
        o["supplier"].as_str().unwrap_or("?"),
        o["order_number"].as_str().unwrap_or("?")
    )
}

/// 2.0 → "2", 1.5 → "1.5"
fn qty(v: &Value) -> String {
    let q = v.as_f64().unwrap_or(1.0);
    if q.fract() == 0.0 {
        format!("{q:.0}")
    } else {
        q.to_string()
    }
}

fn day(v: &Value) -> String {
    v.as_str().unwrap_or("").chars().take(10).collect()
}

fn money(v: &Value, currency: &Value) -> String {
    let n = v.as_f64().unwrap_or(0.0);
    match currency.as_str() {
        Some(c) => format!("{n:.2} {c}"),
        None => format!("{n:.2}"),
    }
}

/// An order by its id or its number (and supplier).
async fn find(api: &Api, r: &OrderRef) -> Result<Value, Failure> {
    let key = r.order.trim();
    if key.len() == 36 && key.chars().filter(|c| *c == '-').count() == 4 {
        if let Ok(o) = api.get(&format!("/api/orders/{}", enc(key))).await {
            return Ok(o);
        }
    }
    let mut path = format!("/api/orders?status=all&number={}", enc(key));
    if let Some(s) = &r.supplier {
        let _ = write!(path, "&supplier={}", enc(s));
    }
    let list = api.get(&path).await?;
    let all = list.as_array().cloned().unwrap_or_default();
    match all.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Failure::new(
            "NOT_FOUND",
            format!(
                "no order {key}{}; `roast orders --all` lists them",
                r.supplier
                    .as_deref()
                    .map(|s| format!(" from {s}"))
                    .unwrap_or_default()
            ),
        )),
        many => {
            let mut f = Failure::new(
                "AMBIGUOUS",
                format!(
                    "{} orders are numbered {key}: give --supplier ({})",
                    many.len(),
                    many.iter()
                        .map(|o| o["supplier"].as_str().unwrap_or("?").to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            f.details = Some(Box::new(Value::Array(many.to_vec())));
            Err(f)
        }
    }
}

fn receipt_id(o: &Value, which: &str) -> Result<String, Failure> {
    let list = o["receipts"].as_array().cloned().unwrap_or_default();
    if let Ok(n) = which.trim().parse::<usize>() {
        if let Some(r) = n.checked_sub(1).and_then(|i| list.get(i)) {
            return Ok(r["id"].as_str().unwrap_or_default().to_string());
        }
    }
    list.iter()
        .find(|r| r["id"].as_str() == Some(which.trim()))
        .map(|r| r["id"].as_str().unwrap_or_default().to_string())
        .ok_or_else(|| {
            Failure::new(
                "NOT_FOUND",
                format!(
                    "no receipt {which} on {}; `roast order receipt {}` lists them",
                    order_name(o),
                    o["order_number"].as_str().unwrap_or("?")
                ),
            )
        })
}

fn receipt_line(r: &Value) -> String {
    format!(
        "{}  {}  {}  {} bytes  {}",
        r["kind"].as_str().unwrap_or("?"),
        r["file_name"].as_str().unwrap_or("(no name)"),
        r["content_type"].as_str().unwrap_or("?"),
        r["size_bytes"].as_i64().unwrap_or(0),
        r["id"].as_str().unwrap_or("?"),
    )
}

/// "shipping=notice.eml" → ("shipping", notice.eml); a bare path is the confirmation.
fn receipt_arg(s: &str) -> (String, PathBuf) {
    if let Some((k, f)) = s.split_once('=') {
        if matches!(k, "confirmation" | "shipping" | "invoice" | "other") {
            return (k.to_string(), PathBuf::from(f));
        }
    }
    ("confirmation".into(), PathBuf::from(s))
}

fn read(path: &Path) -> Result<Vec<u8>, Failure> {
    std::fs::read(path)
        .map_err(|e| Failure::new("NO_FILE", format!("could not read {}: {e}", path.display())))
}

/// An email's Message-ID, from its headers.
fn message_id(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(64 << 10)]);
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if l.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            if k.trim().eq_ignore_ascii_case("message-id") {
                let mut v = v.trim().to_string();
                while lines.peek().is_some_and(|n| n.starts_with([' ', '\t'])) {
                    v.push_str(lines.next().unwrap_or("").trim());
                }
                return Some(v).filter(|v| !v.is_empty());
            }
        }
    }
    None
}

async fn upload(api: &Api, order_id: &str, file: &Path, kind: &str) -> Result<Value, Failure> {
    let bytes = read(file)?;
    let name = file
        .file_name()
        .map_or_else(|| "receipt".into(), |n| n.to_string_lossy().to_string());
    let mut path = format!(
        "/api/orders/{order_id}/receipts?kind={}&file_name={}",
        enc(kind),
        enc(&name)
    );
    if let Some(m) = message_id(&bytes) {
        let _ = write!(path, "&message_id={}", enc(&m));
    }
    let (body, _) = api
        .transfer(
            Method::POST,
            &path,
            Upload::Raw(&bytes, "application/octet-stream"),
        )
        .await?;
    serde_json::from_slice(&body)
        .map_err(|e| Failure::new("BAD_RESPONSE", format!("the receipt's answer: {e}")))
}

async fn import(
    api: &Api,
    out: &Out,
    file: &str,
    receipts: &[String],
    dry_run: bool,
) -> Result<u8, Failure> {
    let text = if file == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .map_err(|e| Failure::new("USAGE", format!("could not read stdin: {e}")))?;
        s
    } else {
        String::from_utf8(read(Path::new(file))?)
            .map_err(|_| Failure::new("USAGE", format!("{file} is not text (the order is JSON)")))?
    };
    let mut body: Value = serde_json::from_str(&text)
        .map_err(|e| Failure::new("USAGE", format!("{file} is not the order's JSON: {e}")))?;
    let files: Vec<(String, PathBuf)> = receipts.iter().map(|r| receipt_arg(r)).collect();
    let mut shas = Vec::new();
    for (_, f) in &files {
        let bytes = read(f)?;
        shas.push(format!("{:x}", Sha256::digest(&bytes)));
    }
    if !shas.is_empty() {
        body["receipt_sha256"] = json!(shas);
    }
    let path = if dry_run {
        "/api/orders?dry_run=true"
    } else {
        "/api/orders"
    };
    let mut r = api.call(Method::POST, path, Some(&body)).await?;
    if !dry_run {
        let oid = enc(r["order"]["id"].as_str().unwrap_or_default());
        let mut kept = Vec::new();
        for (kind, f) in &files {
            kept.push(upload(api, &oid, f, kind).await?);
        }
        if !kept.is_empty() {
            r["order"] = api.get(&format!("/api/orders/{oid}")).await?;
        }
    }
    let ok = r["check"]["ok"].as_bool().unwrap_or(false);
    out.emit(&r, check_text);
    Ok(if dry_run && !ok {
        code::INVALID
    } else {
        code::OK
    })
}

fn check_text(r: &Value) -> String {
    let c = &r["check"];
    let t = &c["totals"];
    let f = |v: &Value| format!("{:.2}", v.as_f64().unwrap_or(0.0));
    let mut out = vec![if r["dry_run"].as_bool() == Some(true) {
        "dry run: nothing recorded".to_string()
    } else {
        format!("recorded: {}", r["order"]["status"].as_str().unwrap_or("?"))
    }];
    out.push(format!(
        "totals: lines {} − discount {} + shipping {} + tax {} = {}; the receipt says {} {}",
        f(&t["lines"]),
        f(&t["discount"]),
        f(&t["shipping"]),
        f(&t["tax"]),
        f(&t["computed"]),
        f(&t["total"]),
        if t["adds_up"].as_bool() == Some(true) {
            "(adds up)".to_string()
        } else {
            format!("(off by {})", f(&t["difference"]))
        }
    ));
    for l in c["lines"].as_array().into_iter().flatten() {
        let w = l["weight_g"]
            .as_f64()
            .map_or_else(|| "no weight".into(), grams);
        let what = match l["result"].as_str().unwrap_or("?") {
            "other" => "not coffee".to_string(),
            "new" => format!(
                "a new bean{}",
                l["by"]
                    .as_str()
                    .map(|b| format!(" (from the shopping list, {b})"))
                    .unwrap_or_default()
            ),
            "ambiguous" => format!(
                "AMBIGUOUS: {}",
                l["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|b| format!(
                        "{} ({})",
                        b["name"].as_str().unwrap_or("?"),
                        b["id"].as_str().unwrap_or("?")
                    ))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            res => format!(
                "{} ({})",
                l["bean_name"].as_str().unwrap_or("?"),
                l["by"].as_str().unwrap_or(res)
            ),
        };
        out.push(format!(
            "  {} {}  {w}  {} → {what}  cost {}",
            l["position"],
            l["name"].as_str().unwrap_or("?"),
            f(&l["price"]),
            f(&l["cost"]),
        ));
    }
    for p in c["problems"].as_array().into_iter().flatten() {
        out.push(format!("problem: {}", p.as_str().unwrap_or("?")));
    }
    if !r["order"].is_null() {
        out.push(order_text(&r["order"]));
    }
    out.join("\n")
}

fn order_text(o: &Value) -> String {
    let mut out = vec![format!(
        "{} ({}), placed {}: {}, total {}{}",
        order_name(o),
        o["id"].as_str().unwrap_or("?"),
        day(&o["placed_at"]),
        o["status"].as_str().unwrap_or("?"),
        money(&o["total"], &o["currency"]),
        o["tracking"]
            .as_str()
            .map(|t| format!(", {} {t}", o["carrier"].as_str().unwrap_or("tracking")))
            .unwrap_or_default(),
    )];
    for l in o["lines"].as_array().into_iter().flatten() {
        out.push(format!(
            "  {} {}{}  {}  {:.2}{}",
            l["position"],
            l["name"].as_str().unwrap_or("?"),
            l["bag"]
                .as_str()
                .map(|b| format!(" ({b} × {})", qty(&l["quantity"])))
                .unwrap_or_default(),
            l["received_g"]
                .as_f64()
                .or(l["weight_g"].as_f64())
                .map_or_else(|| "-".into(), grams),
            l["price"].as_f64().unwrap_or(0.0),
            match (l["kind"].as_str(), l["bean_name"].as_str()) {
                (Some("other"), _) => "  not coffee".to_string(),
                (_, Some(b)) => format!("  → {b}"),
                _ => "  → no bean".to_string(),
            }
        ));
    }
    let n = o["receipts"].as_array().map_or(0, Vec::len);
    out.push(format!(
        "  {n} receipt{} kept",
        if n == 1 { "" } else { "s" }
    ));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_and_message_ids() {
        assert_eq!(
            receipt_arg("shipping=a.eml"),
            ("shipping".into(), PathBuf::from("a.eml"))
        );
        assert_eq!(receipt_arg("x=y.eml").0, "confirmation");
        let eml = b"From: a@b\r\nMessage-ID:\r\n <123@shop.example>\r\nSubject: x\r\n\r\nMessage-ID: <no>";
        assert_eq!(message_id(eml).as_deref(), Some("<123@shop.example>"));
        assert_eq!(message_id(b"%PDF-1.7"), None);
    }
}
