//! Orders from receipts (`docs/green-coffee.md`, "Orders"; migration 054).
//!
//! An agent reads the supplier's confirmation (email, PDF, a photo) into a structured order;
//! the server checks it: the order number is unique per supplier, the lines add up to the
//! receipt's total, and each coffee line is matched to a bean (the owner's choice, then the
//! shop's variant id, the supplier's coffee code, the shopping list, then the name). An order
//! goes ordered → shipped → received, and only received writes the purchase moves, with the
//! shipping split over the coffee by weight and the discount and tax by price. Until then the
//! coffee is "on order" (`Bean::on_order_g`). No email parser here: a supplier changing its
//! email changes nothing in the server.
//!
//! The receipts are kept with their order as received, the record of what was paid; they are
//! private: they hold the owner's address, phone and card digits. Only the owner and an inventory
//! or admin token download one (`auth.rs`, `tokens.rs`); never a share page, a report or a support
//! bundle.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::coffee::import::{label_grams, Draft};
use crate::coffee::inventory::{
    bad, clean, currency_code, insert_move, money, purchase_time, BeanRequest, InventoryService,
    NewMove,
};

/// The largest receipt kept (a forwarded email with its images, a PDF invoice)
pub const MAX_RECEIPT_BYTES: usize = 5 << 20;
/// Receipts per order: the confirmation, the shipping notice, an invoice, a photo or two
const MAX_RECEIPTS: i64 = 10;
/// All of an order's receipts together
const MAX_ORDER_RECEIPT_BYTES: i64 = 20 << 20;
/// Totals add up when they agree to the cent
const CENT: f64 = 0.005;

/// The same order or the same receipt again: the handlers answer 409 naming the one kept.
#[derive(Debug)]
pub struct OrderConflict(pub String);

impl std::fmt::Display for OrderConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OrderConflict {}

/// A receipt read into an order: what the agent sends (`roast order import`).
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct OrderImport {
    pub supplier: String,
    /// As the supplier prints it ("000412345")
    pub order_number: String,
    /// YYYY-MM-DD or RFC 3339 (default now)
    pub placed_at: Option<String>,
    /// ISO 4217 (default: the home currency)
    pub currency: Option<String>,
    /// The receipt's subtotal, checked against the lines when given
    pub subtotal: Option<f64>,
    /// The order's discount, as a positive amount
    pub discount: Option<f64>,
    pub shipping: Option<f64>,
    pub tax: Option<f64>,
    /// What was paid: the lines − discount + shipping + tax must come to it within a cent
    pub total: f64,
    /// ordered (default), shipped or received (an order that came long ago)
    pub status: Option<String>,
    pub carrier: Option<String>,
    pub tracking: Option<String>,
    /// YYYY-MM-DD or RFC 3339
    pub shipped_at: Option<String>,
    /// With status received: when it came (default `placed_at`)
    pub received_at: Option<String>,
    /// Record totals that do not add up anyway, and why (kept on the order)
    pub accept_mismatch: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<ImportLine>,
    /// SHA-256 (hex) of the receipts it was read from: one kept already makes this a duplicate
    #[serde(default)]
    pub receipt_sha256: Vec<String>,
}

/// One line of a receipt.
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct ImportLine {
    /// coffee (default) or other: a colour card, chaff bags. Other lines cost no bean anything
    /// and take no shipping.
    pub kind: Option<String>,
    pub name: String,
    pub sku: Option<String>,
    /// The shop's id for the bag (Shopify variant, WooCommerce product)
    pub variant_id: Option<String>,
    /// The bag as printed: "1 LB", "5 lb", "2 kg"
    pub bag: Option<String>,
    /// Bags (default 1)
    pub quantity: Option<f64>,
    /// All of the line's coffee (default: the bag × the quantity)
    pub weight_g: Option<f64>,
    /// The line's price, all of it
    pub price: f64,
    /// The receipt says tax was charged on this line
    pub taxed: Option<bool>,
    /// The coffee's page, for a new bean
    pub url: Option<String>,
    /// This bean, whatever matching would say (the owner's answer to an ambiguous line)
    pub bean_id: Option<String>,
    /// A new bean, whatever matching would say
    #[serde(default)]
    pub new_bean: bool,
}

/// What a check found about a receipt (`dry_run`) and what an import recorded.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrderCheck {
    /// Nothing stops the import
    pub ok: bool,
    /// What stops it, in words
    pub problems: Vec<String>,
    /// The same order (supplier and number) or the same receipt is kept already
    pub duplicate: Option<Duplicate>,
    pub totals: TotalsCheck,
    pub lines: Vec<LineCheck>,
    /// Coffee in the order
    pub coffee_g: f64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Duplicate {
    pub order_id: String,
    pub supplier: String,
    pub order_number: String,
    pub status: String,
    /// order_number | receipt
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    pub by: String,
}

#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
pub struct TotalsCheck {
    /// Sum of the lines' prices
    pub lines: f64,
    pub discount: f64,
    pub shipping: f64,
    pub tax: f64,
    /// lines − discount + shipping + tax
    pub computed: f64,
    /// The receipt's total
    pub total: f64,
    /// total − computed
    pub difference: f64,
    pub subtotal: Option<f64>,
    /// subtotal − lines, when the receipt has a subtotal
    pub subtotal_difference: Option<f64>,
    /// Within a cent, subtotal included
    pub adds_up: bool,
    /// Recorded although it does not add up, because …
    pub accepted: Option<String>,
}

/// One line as the server reads it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct LineCheck {
    /// 1, 2, … as on the receipt
    pub position: i64,
    pub name: String,
    /// coffee | other
    pub kind: String,
    pub weight_g: Option<f64>,
    pub price: f64,
    /// chosen (the line named its bean) | matched | new (a bean is made) | ambiguous (several
    /// beans fit: name one with `bean_id`, or `new_bean`) | other (not coffee)
    pub result: String,
    /// What matched: variant_id | sku | wanted | name
    #[expect(
        clippy::doc_markdown,
        reason = "OpenAPI description: a change here changes docs/openapi.json"
    )]
    pub by: Option<String>,
    pub bean_id: Option<String>,
    pub bean_name: Option<String>,
    /// The beans an ambiguous line could be
    pub candidates: Vec<BeanRef>,
    /// The shopping-list entry it fills
    pub wanted_id: Option<String>,
    /// Its share of the order's discount, shipping and tax, and what the coffee costs in all
    pub discount: f64,
    pub shipping: f64,
    pub tax: f64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct BeanRef {
    pub id: String,
    pub name: String,
}

/// An order as kept.
#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct PurchaseOrder {
    pub id: String,
    pub supplier: String,
    pub order_number: String,
    pub placed_at: DateTime<Utc>,
    /// ordered | shipped | received | cancelled
    pub status: String,
    pub currency: Option<String>,
    pub subtotal: Option<f64>,
    pub discount: f64,
    pub shipping: f64,
    pub tax: f64,
    pub total: f64,
    /// Why totals that did not add up were accepted
    pub mismatch_reason: Option<String>,
    pub carrier: Option<String>,
    pub tracking: Option<String>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub received_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[sqlx(skip)]
    pub lines: Vec<OrderLine>,
    /// What is kept of each receipt (the file itself: `GET …/receipts/{id}`)
    #[sqlx(skip)]
    pub receipts: Vec<Receipt>,
    /// Coffee in the order
    #[sqlx(skip)]
    pub coffee_g: f64,
}

#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct OrderLine {
    pub id: String,
    pub position: i64,
    /// coffee | other
    pub kind: String,
    pub name: String,
    pub sku: Option<String>,
    pub variant_id: Option<String>,
    pub bag: Option<String>,
    pub quantity: f64,
    pub weight_g: Option<f64>,
    pub price: f64,
    pub taxed: Option<bool>,
    pub url: Option<String>,
    pub bean_id: Option<String>,
    /// Joined
    pub bean_name: Option<String>,
    pub wanted_id: Option<String>,
    /// The purchase written when the order was received
    pub move_id: Option<String>,
    /// What arrived, when it differed
    pub received_g: Option<f64>,
}

/// A receipt kept with its order (the file itself is private).
#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct Receipt {
    pub id: String,
    pub order_id: String,
    /// confirmation | shipping | invoice | other
    pub kind: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub message_id: Option<String>,
    pub sha256: String,
    pub size_bytes: i64,
    pub added_at: DateTime<Utc>,
}

/// What an import answers: the check, and the order when it was recorded.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrderImported {
    pub dry_run: bool,
    pub check: OrderCheck,
    pub order: Option<PurchaseOrder>,
}

/// It shipped: the shipping notice's carrier and tracking.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct ShippedRequest {
    pub carrier: Option<String>,
    pub tracking: Option<String>,
    /// YYYY-MM-DD or RFC 3339 (default now)
    pub at: Option<String>,
}

/// It came: when, and any line that differs from the order.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct ReceivedRequest {
    /// YYYY-MM-DD or RFC 3339 (default now)
    pub at: Option<String>,
    #[serde(default)]
    pub lines: Vec<ReceivedLine>,
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct ReceivedLine {
    /// The line's position (1, 2, …) or id
    pub line: String,
    /// What arrived, when it is not what was ordered
    pub weight_g: Option<f64>,
    /// Put it on this bean instead
    pub bean_id: Option<String>,
}

/// Which orders to list.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OrdersQuery {
    /// open (ordered or shipped, the default), all, or one status
    pub status: Option<String>,
    /// The supplier's order number
    pub number: Option<String>,
    pub supplier: Option<String>,
}

/// "Sweet Maria's" and "sweet marias" are one supplier.
pub fn supplier_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A bag's SKU less its size: "GCX-8087-001" → "GCX-8087" (the form the import keeps for a
/// coffee). A SKU of one or two parts is the coffee's already.
fn coffee_code(sku: &str) -> String {
    let s = sku.trim();
    let parts: Vec<&str> = s.split(['-', '_', '/', '.']).collect();
    if parts.len() >= 3
        && parts
            .last()
            .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
    {
        let cut = s.len() - parts.last().map_or(0, |p| p.len()) - 1;
        return s[..cut].to_ascii_uppercase();
    }
    s.to_ascii_uppercase()
}

fn same_sku(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim()) || coffee_code(a) == coffee_code(b)
}

/// Lower case, letters and digits, single spaces, no " green" at the end.
fn name_key(s: &str) -> String {
    let words: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut w = words.as_slice();
    while let [rest @ .., last] = w {
        if matches!(last.as_str(), "green" | "coffee" | "beans") && !rest.is_empty() {
            w = rest;
        } else {
            break;
        }
    }
    w.join(" ")
}

/// `total` split over `weights` in whole cents that add up to it exactly (largest remainder).
/// No weight at all: equal parts.
pub fn split_cents(total: f64, weights: &[f64]) -> Vec<f64> {
    if weights.is_empty() {
        return Vec::new();
    }
    let cents = (total * 100.0).round() as i64;
    let sum: f64 = weights.iter().map(|w| w.max(0.0)).sum();
    let w: Vec<f64> = if sum > 0.0 {
        weights.iter().map(|x| x.max(0.0) / sum).collect()
    } else {
        vec![1.0 / weights.len() as f64; weights.len()]
    };
    let exact: Vec<f64> = w.iter().map(|x| x * cents as f64).collect();
    let mut parts: Vec<i64> = exact.iter().map(|x| x.floor() as i64).collect();
    let mut left = cents - parts.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|&a, &b| {
        (exact[b] - exact[b].floor())
            .partial_cmp(&(exact[a] - exact[a].floor()))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    for i in order.iter().cycle() {
        if left <= 0 {
            break;
        }
        parts[*i] += 1;
        left -= 1;
    }
    parts.into_iter().map(|c| c as f64 / 100.0).collect()
}

/// Each line's share of the discount, shipping and tax. Shipping goes over the coffee lines
/// by weight; the discount over every line by price; the tax over the lines it was charged
/// on (when the receipt says), else every line, by price. An other line's shares cost no bean.
fn shares(
    lines: &[(bool, f64, Option<f64>, Option<bool>)],
    discount: f64,
    shipping: f64,
    tax: f64,
) -> Vec<(f64, f64, f64)> {
    let prices: Vec<f64> = lines.iter().map(|l| l.1).collect();
    let d = split_cents(discount, &prices);
    let coffee: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].0).collect();
    let mut s = vec![0.0; lines.len()];
    if !coffee.is_empty() {
        let w: Vec<f64> = coffee.iter().map(|&i| lines[i].2.unwrap_or(0.0)).collect();
        for (k, v) in coffee.iter().zip(split_cents(shipping, &w)) {
            s[*k] = v;
        }
    }
    let taxed: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].3 == Some(true))
        .collect();
    let on: Vec<usize> = if taxed.is_empty() {
        (0..lines.len()).collect()
    } else {
        taxed
    };
    let mut t = vec![0.0; lines.len()];
    let w: Vec<f64> = on.iter().map(|&i| prices[i]).collect();
    for (k, v) in on.iter().zip(split_cents(tax, &w)) {
        t[*k] = v;
    }
    (0..lines.len()).map(|i| (d[i], s[i], t[i])).collect()
}

fn cents(v: f64) -> f64 {
    crate::coffee::costs::cents(v)
}

fn status_ok(s: &str) -> bool {
    matches!(s, "ordered" | "shipped" | "received" | "cancelled")
}

/// A receipt's type: what it says, else its file name's. Emails, web pages, PDFs, images and
/// text; nothing else is kept.
pub fn receipt_type(content_type: Option<&str>, file_name: Option<&str>) -> Option<&'static str> {
    const KNOWN: &[(&str, &[&str])] = &[
        ("message/rfc822", &["eml"]),
        ("text/html", &["html", "htm"]),
        ("application/pdf", &["pdf"]),
        ("image/png", &["png"]),
        ("image/jpeg", &["jpg", "jpeg"]),
        ("image/webp", &["webp"]),
        ("image/heic", &["heic"]),
        ("text/plain", &["txt"]),
    ];
    if let Some(ct) = content_type {
        let ct = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if let Some((k, _)) = KNOWN.iter().find(|(k, _)| *k == ct) {
            return Some(k);
        }
    }
    let ext = file_name?.rsplit_once('.')?.1.to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(_, e)| e.contains(&ext.as_str()))
        .map(|(k, _)| *k)
}

/// What matching reads once per check.
struct Known {
    /// id, name, supplier, sku (beans not archived)
    beans: Vec<(String, String, Option<String>, Option<String>)>,
    /// variant id or sku → bean, from earlier orders
    lines: Vec<(Option<String>, Option<String>, String)>,
    /// Open shopping-list entries
    wanted: Vec<WantedRow>,
}

#[derive(FromRow)]
struct WantedRow {
    id: String,
    name: String,
    supplier: Option<String>,
    bean_id: Option<String>,
    draft: Option<sqlx::types::Json<Draft>>,
}

/// One line's match: result, by, bean, candidates, wanted entry.
struct Match {
    result: &'static str,
    by: Option<&'static str>,
    bean: Option<(String, String)>,
    candidates: Vec<BeanRef>,
    wanted: Option<String>,
}

impl Match {
    fn new() -> Self {
        Match {
            result: "new",
            by: None,
            bean: None,
            candidates: Vec::new(),
            wanted: None,
        }
    }
}

#[derive(Clone)]
pub struct OrderService {
    db: SqlitePool,
}

impl OrderService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    async fn known(&self) -> Result<Known> {
        let beans = sqlx::query_as(
            "SELECT id, name, supplier, sku FROM beans WHERE archived = 0 ORDER BY created_at",
        )
        .fetch_all(&self.db)
        .await?;
        let lines = sqlx::query_as(
            "SELECT variant_id, sku, bean_id FROM purchase_order_lines \
             WHERE bean_id IS NOT NULL AND kind = 'coffee'",
        )
        .fetch_all(&self.db)
        .await?;
        let wanted = sqlx::query_as::<_, WantedRow>(
            "SELECT id, name, supplier, bean_id, draft FROM wanted WHERE status = 'wanted' \
             ORDER BY created_at",
        )
        .fetch_all(&self.db)
        .await?;
        Ok(Known {
            beans,
            lines,
            wanted,
        })
    }

    /// A line matched to a bean: the variant id, the supplier's coffee code, the shopping
    /// list, then the name. Several beans by one rule: ambiguous.
    fn match_line(k: &Known, supplier: &str, l: &ImportLine, taken: &[String]) -> Match {
        let skey = supplier_key(supplier);
        let ours = |s: &Option<String>| s.as_deref().is_none_or(|s| supplier_key(s) == skey);
        let bean = |id: &str| k.beans.iter().find(|b| b.0 == id);
        let one = |ids: Vec<&str>, by: &'static str| -> Option<Match> {
            let mut ids: Vec<&str> = ids.into_iter().filter(|i| bean(i).is_some()).collect();
            ids.sort_unstable();
            ids.dedup();
            match ids.as_slice() {
                [] => None,
                [id] => {
                    let b = bean(id)?;
                    Some(Match {
                        result: "matched",
                        by: Some(by),
                        bean: Some((b.0.clone(), b.1.clone())),
                        ..Match::new()
                    })
                }
                many => Some(Match {
                    result: "ambiguous",
                    by: Some(by),
                    candidates: many
                        .iter()
                        .filter_map(|i| bean(i))
                        .map(|b| BeanRef {
                            id: b.0.clone(),
                            name: b.1.clone(),
                        })
                        .collect(),
                    ..Match::new()
                }),
            }
        };
        // A shopping-list entry the line fills: by variant id, SKU, then name
        let wanted = k
            .wanted
            .iter()
            .filter(|w| ours(&w.supplier) && !taken.contains(&w.id))
            .find(|w| {
                let d = w.draft.as_deref();
                let by_variant = l.variant_id.as_deref().is_some_and(|v| {
                    d.is_some_and(|d| d.sizes.iter().any(|z| z.variant_id.as_deref() == Some(v)))
                });
                let by_sku = l.sku.as_deref().is_some_and(|s| {
                    d.is_some_and(|d| {
                        d.sku.as_deref().is_some_and(|x| same_sku(x, s))
                            || d.sizes
                                .iter()
                                .any(|z| z.sku.as_deref().is_some_and(|x| same_sku(x, s)))
                    })
                });
                by_variant || by_sku || name_key(&w.name) == name_key(&l.name)
            });
        let with_wanted = |mut m: Match| {
            m.wanted = wanted.map(|w| w.id.clone());
            m
        };
        if let Some(v) = l.variant_id.as_deref().filter(|v| !v.trim().is_empty()) {
            let ids = k
                .lines
                .iter()
                .filter(|x| x.0.as_deref() == Some(v))
                .map(|x| x.2.as_str())
                .collect();
            if let Some(m) = one(ids, "variant_id") {
                return with_wanted(m);
            }
        }
        if let Some(s) = l.sku.as_deref().filter(|s| !s.trim().is_empty()) {
            let mut ids: Vec<&str> = k
                .beans
                .iter()
                .filter(|b| ours(&b.2) && b.3.as_deref().is_some_and(|x| same_sku(x, s)))
                .map(|b| b.0.as_str())
                .collect();
            ids.extend(
                k.lines
                    .iter()
                    .filter(|x| x.1.as_deref().is_some_and(|x| same_sku(x, s)))
                    .map(|x| x.2.as_str()),
            );
            if let Some(m) = one(ids, "sku") {
                return with_wanted(m);
            }
        }
        if let Some(w) = wanted {
            let m = match w.bean_id.as_deref().and_then(bean) {
                Some(b) => Match {
                    result: "matched",
                    by: Some("wanted"),
                    bean: Some((b.0.clone(), b.1.clone())),
                    ..Match::new()
                },
                None => Match {
                    by: Some("wanted"),
                    ..Match::new()
                },
            };
            return with_wanted(m);
        }
        let key = name_key(&l.name);
        let exact: Vec<&str> = k
            .beans
            .iter()
            .filter(|b| ours(&b.2) && name_key(&b.1) == key)
            .map(|b| b.0.as_str())
            .collect();
        if let Some(m) = one(exact, "name") {
            return m;
        }
        // One name inside the other ("Kenya Nyeri Gatugi" in "Kenya Nyeri Gatugi AA Lot 7"):
        // close, never sure. The owner picks.
        let close: Vec<BeanRef> = k
            .beans
            .iter()
            .filter(|b| {
                let n = name_key(&b.1);
                ours(&b.2)
                    && n.split(' ').count() >= 2
                    && key.split(' ').count() >= 2
                    && (format!(" {key} ").contains(&format!(" {n} "))
                        || format!(" {n} ").contains(&format!(" {key} ")))
            })
            .map(|b| BeanRef {
                id: b.0.clone(),
                name: b.1.clone(),
            })
            .collect();
        if !close.is_empty() {
            return Match {
                result: "ambiguous",
                by: Some("name"),
                candidates: close,
                ..Match::new()
            };
        }
        Match::new()
    }

    /// Check a receipt read into an order; nothing is written.
    pub async fn check(&self, req: &OrderImport) -> Result<OrderCheck> {
        let supplier =
            clean(Some(req.supplier.clone())).ok_or_else(|| bad("an order needs its supplier"))?;
        let number = clean(Some(req.order_number.clone()))
            .ok_or_else(|| bad("an order needs the supplier's order number"))?;
        if req.lines.is_empty() {
            return Err(bad("an order needs at least one line"));
        }
        if !req.total.is_finite() || req.total < 0.0 {
            return Err(bad("total must be 0 or more"));
        }
        money(req.subtotal, "subtotal")?;
        let discount = money(req.discount, "discount")?.unwrap_or(0.0);
        let shipping = money(req.shipping, "shipping")?.unwrap_or(0.0);
        let tax = money(req.tax, "tax")?.unwrap_or(0.0);
        currency_code(req.currency.clone())?;
        purchase_time(req.placed_at.as_deref())?;
        let status = req.status.as_deref().unwrap_or("ordered");
        if !matches!(status, "ordered" | "shipped" | "received") {
            return Err(bad(format!(
                "an order comes in as ordered, shipped or received, not {status:?}"
            )));
        }
        for (i, l) in req.lines.iter().enumerate() {
            let n = i + 1;
            if l.name.trim().is_empty() {
                return Err(bad(format!("line {n} needs a name")));
            }
            if !matches!(l.kind.as_deref().unwrap_or("coffee"), "coffee" | "other") {
                return Err(bad(format!("line {n}: a line is coffee or other")));
            }
            if !l.price.is_finite() || l.price < 0.0 {
                return Err(bad(format!(
                    "line {n}: its price must be 0 or more (a discount goes in `discount`)"
                )));
            }
            if l.quantity.is_some_and(|q| !q.is_finite() || q <= 0.0) {
                return Err(bad(format!("line {n}: its quantity must be above 0")));
            }
            if l.weight_g.is_some_and(|g| !g.is_finite() || g <= 0.0) {
                return Err(bad(format!("line {n}: its weight must be above 0")));
            }
        }

        let mut problems = Vec::new();
        let duplicate = self
            .duplicate(&supplier, &number, &req.receipt_sha256)
            .await?;
        if let Some(d) = &duplicate {
            problems.push(match d.by.as_str() {
                "receipt" => format!(
                    "this receipt is kept already, with {} order {} ({})",
                    d.supplier, d.order_number, d.status
                ),
                _ => format!(
                    "{} order {} is recorded already ({})",
                    d.supplier, d.order_number, d.status
                ),
            });
        }

        let lines_sum = cents(req.lines.iter().map(|l| l.price).sum());
        let computed = cents(lines_sum - discount + shipping + tax);
        let difference = cents(req.total - computed);
        let subtotal_difference = req.subtotal.map(|s| cents(s - lines_sum));
        let adds_up = difference.abs() < CENT && subtotal_difference.is_none_or(|d| d.abs() < CENT);
        let accepted = clean(req.accept_mismatch.clone());
        if !adds_up && accepted.is_none() {
            let mut why = format!(
                "the lines ({lines_sum:.2}) − discount ({discount:.2}) + shipping ({shipping:.2}) \
                 + tax ({tax:.2}) come to {computed:.2}, not the total {:.2} (off by {difference:+.2})",
                req.total
            );
            if let Some(d) = subtotal_difference.filter(|d| d.abs() >= CENT) {
                why = format!(
                    "the lines come to {lines_sum:.2}, not the subtotal {:.2} (off by {d:+.2}); {why}",
                    req.subtotal.unwrap_or_default()
                );
            }
            problems.push(format!(
                "{why}. Read the receipt again, or give accept_mismatch with the reason"
            ));
        }

        let known = self.known().await?;
        let mut checks = Vec::new();
        let mut taken: Vec<String> = Vec::new();
        for (i, l) in req.lines.iter().enumerate() {
            let position = i as i64 + 1;
            let kind = l.kind.clone().unwrap_or_else(|| "coffee".into());
            let quantity = l.quantity.unwrap_or(1.0);
            let weight = l.weight_g.or_else(|| {
                l.bag
                    .as_deref()
                    .and_then(label_grams)
                    .map(|g| (g * quantity * 10.0).round() / 10.0)
            });
            let m = if kind == "other" {
                Match {
                    result: "other",
                    ..Match::new()
                }
            } else if let Some(b) = clean(l.bean_id.clone()) {
                let found = InventoryService::new(self.db.clone()).bean(&b).await?;
                match found {
                    Some(bean) => Match {
                        result: "chosen",
                        bean: Some((bean.id, bean.name)),
                        ..Match::new()
                    },
                    None => return Err(bad(format!("line {position}: no bean {b}"))),
                }
            } else if l.new_bean {
                Match::new()
            } else {
                Self::match_line(&known, &supplier, l, &taken)
            };
            if let Some(w) = &m.wanted {
                taken.push(w.clone());
            }
            if kind == "coffee" && weight.is_none() {
                problems.push(format!(
                    "line {position} ({}): no weight; give its bag (\"1 LB\") or weight_g",
                    l.name
                ));
            }
            if m.result == "ambiguous" {
                problems.push(format!(
                    "line {position} ({}) could be {}: give its bean_id, or new_bean",
                    l.name,
                    m.candidates
                        .iter()
                        .map(|c| format!("{} ({})", c.name, c.id))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ));
            }
            checks.push(LineCheck {
                position,
                name: l.name.trim().to_string(),
                kind,
                weight_g: weight,
                price: l.price,
                result: m.result.into(),
                by: m.by.map(Into::into),
                bean_name: m.bean.as_ref().map(|b| b.1.clone()),
                bean_id: m.bean.map(|b| b.0),
                candidates: m.candidates,
                wanted_id: m.wanted,
                discount: 0.0,
                shipping: 0.0,
                tax: 0.0,
                cost: 0.0,
            });
        }
        let parts: Vec<_> = checks
            .iter()
            .zip(&req.lines)
            .map(|(c, l)| (c.kind == "coffee", c.price, c.weight_g, l.taxed))
            .collect();
        for (c, (d, s, t)) in checks
            .iter_mut()
            .zip(shares(&parts, discount, shipping, tax))
        {
            c.discount = d;
            c.shipping = s;
            c.tax = t;
            c.cost = cents(c.price - d + s + t);
        }
        let coffee_g = checks
            .iter()
            .filter(|c| c.kind == "coffee")
            .filter_map(|c| c.weight_g)
            .sum::<f64>();
        Ok(OrderCheck {
            ok: problems.is_empty(),
            problems,
            duplicate,
            totals: TotalsCheck {
                lines: lines_sum,
                discount,
                shipping,
                tax,
                computed,
                total: req.total,
                difference,
                subtotal: req.subtotal,
                subtotal_difference,
                adds_up,
                accepted: (!adds_up).then_some(accepted).flatten(),
            },
            lines: checks,
            coffee_g: (coffee_g * 10.0).round() / 10.0,
        })
    }

    async fn duplicate(
        &self,
        supplier: &str,
        number: &str,
        receipts: &[String],
    ) -> Result<Option<Duplicate>> {
        let by_number: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, supplier, order_number, status FROM purchase_orders \
             WHERE supplier_key = ? AND order_number = ?",
        )
        .bind(supplier_key(supplier))
        .bind(number)
        .fetch_optional(&self.db)
        .await?;
        if let Some((order_id, supplier, order_number, status)) = by_number {
            return Ok(Some(Duplicate {
                order_id,
                supplier,
                order_number,
                status,
                by: "order_number".into(),
            }));
        }
        for sha in receipts {
            let row: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT o.id, o.supplier, o.order_number, o.status FROM purchase_receipts r \
                 JOIN purchase_orders o ON o.id = r.order_id WHERE r.sha256 = ?",
            )
            .bind(sha.trim().to_ascii_lowercase())
            .fetch_optional(&self.db)
            .await?;
            if let Some((order_id, supplier, order_number, status)) = row {
                return Ok(Some(Duplicate {
                    order_id,
                    supplier,
                    order_number,
                    status,
                    by: "receipt".into(),
                }));
            }
        }
        Ok(None)
    }

    /// Check, and unless `dry_run` record: a bean made for each new coffee, the order and its
    /// lines, the shopping-list entries it fills marked ordered. Received writes the purchases.
    pub async fn import(&self, req: OrderImport, dry_run: bool) -> Result<OrderImported> {
        let check = self.check(&req).await?;
        if dry_run {
            return Ok(OrderImported {
                dry_run,
                check,
                order: None,
            });
        }
        if let Some(d) = &check.duplicate {
            return Err(
                OrderConflict(check.problems.first().cloned().unwrap_or_else(|| {
                    format!(
                        "{} order {} is recorded already",
                        d.supplier, d.order_number
                    )
                }))
                .into(),
            );
        }
        if !check.ok {
            return Err(bad(check.problems.join("; ")));
        }
        let supplier = req.supplier.trim().to_string();
        let number = req.order_number.trim().to_string();
        let placed = purchase_time(req.placed_at.as_deref())?;
        let status = req.status.clone().unwrap_or_else(|| "ordered".into());

        // New beans first, from the shopping list's draft when the line fills an entry
        let inv = InventoryService::new(self.db.clone());
        let mut made = Vec::new();
        let mut bean_of: Vec<Option<String>> =
            check.lines.iter().map(|c| c.bean_id.clone()).collect();
        for (i, c) in check.lines.iter().enumerate() {
            if c.result != "new" {
                continue;
            }
            let l = &req.lines[i];
            let draft = match &c.wanted_id {
                Some(w) => sqlx::query_scalar::<_, Option<sqlx::types::Json<Draft>>>(
                    "SELECT draft FROM wanted WHERE id = ?",
                )
                .bind(w)
                .fetch_optional(&self.db)
                .await?
                .flatten(),
                None => None,
            };
            let mut r = match draft {
                Some(d) => crate::coffee::wanted::bean_from_draft(&d),
                None => BeanRequest {
                    name: Some(l.name.trim().to_string()),
                    supplier: Some(Some(supplier.clone())),
                    url: Some(clean(l.url.clone())),
                    sku: Some(l.sku.as_deref().map(coffee_code)),
                    ..Default::default()
                },
            };
            r.stock_g = None;
            let bean = inv.create_bean(r).await?;
            made.push(bean.id.clone());
            bean_of[i] = Some(bean.id);
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let written: Result<()> = async {
            let mut tx = crate::db::begin_write(&self.db).await?;
            sqlx::query(
                "INSERT INTO purchase_orders (id, supplier, supplier_key, order_number, placed_at, \
                 status, currency, subtotal, discount, shipping, tax, total, mismatch_reason, \
                 carrier, tracking, shipped_at, note, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, 'ordered', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&supplier)
            .bind(supplier_key(&supplier))
            .bind(&number)
            .bind(placed)
            .bind(currency_code(req.currency.clone())?)
            .bind(req.subtotal)
            .bind(check.totals.discount)
            .bind(check.totals.shipping)
            .bind(check.totals.tax)
            .bind(req.total)
            .bind(&check.totals.accepted)
            .bind(clean(req.carrier.clone()))
            .bind(clean(req.tracking.clone()))
            .bind(match req.shipped_at.as_deref() {
                Some(t) => Some(purchase_time(Some(t))?),
                None => None,
            })
            .bind(clean(req.note.clone()))
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(d) if d.message().contains("UNIQUE") => {
                    OrderConflict(format!("{supplier} order {number} is recorded already")).into()
                }
                _ => anyhow::Error::from(e),
            })?;
            for (i, (c, l)) in check.lines.iter().zip(&req.lines).enumerate() {
                let line_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO purchase_order_lines (id, order_id, position, kind, name, sku, \
                     variant_id, bag, quantity, weight_g, price, taxed, url, bean_id, wanted_id) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&line_id)
                .bind(&id)
                .bind(c.position)
                .bind(&c.kind)
                .bind(&c.name)
                .bind(clean(l.sku.clone()))
                .bind(clean(l.variant_id.clone()))
                .bind(clean(l.bag.clone()))
                .bind(l.quantity.unwrap_or(1.0))
                .bind(c.weight_g)
                .bind(c.price)
                .bind(l.taxed)
                .bind(clean(l.url.clone()))
                .bind(&bean_of[i])
                .bind(&c.wanted_id)
                .execute(&mut *tx)
                .await?;
                if let Some(w) = &c.wanted_id {
                    sqlx::query(
                        "UPDATE wanted SET status = 'ordered', order_line_id = ?, \
                         bean_id = COALESCE(?, bean_id), updated_at = ? WHERE id = ?",
                    )
                    .bind(&line_id)
                    .bind(&bean_of[i])
                    .bind(now)
                    .bind(w)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            tx.commit().await?;
            Ok(())
        }
        .await;
        if let Err(e) = written {
            for b in made {
                let _ = inv.delete_bean(&b).await;
            }
            return Err(e);
        }
        if status == "shipped" || status == "received" {
            self.shipped(
                &id,
                ShippedRequest {
                    carrier: req.carrier.clone(),
                    tracking: req.tracking.clone(),
                    at: req.shipped_at.clone().or(req.placed_at.clone()),
                },
            )
            .await?;
        }
        if status == "received" {
            self.received(
                &id,
                ReceivedRequest {
                    at: req.received_at.clone().or(req.placed_at.clone()),
                    lines: Vec::new(),
                },
            )
            .await?;
        }
        Ok(OrderImported {
            dry_run,
            check,
            order: self.get(&id).await?,
        })
    }

    pub async fn list(&self, q: &OrdersQuery) -> Result<Vec<PurchaseOrder>> {
        let mut sql = String::from("SELECT * FROM purchase_orders WHERE 1 = 1");
        let status = q.status.as_deref().unwrap_or("open");
        match status {
            "open" => sql.push_str(" AND status IN ('ordered', 'shipped')"),
            "all" => {}
            s if status_ok(s) => sql.push_str(" AND status = ?"),
            s => {
                return Err(bad(format!(
                    "status is open, all, ordered, shipped, received or cancelled, not {s:?}"
                )))
            }
        }
        if q.number.is_some() {
            sql.push_str(" AND order_number = ?");
        }
        if q.supplier.is_some() {
            sql.push_str(" AND supplier_key = ?");
        }
        sql.push_str(" ORDER BY placed_at DESC, created_at DESC");
        let mut query = sqlx::query_as::<_, PurchaseOrder>(&sql);
        if status_ok(status) {
            query = query.bind(status);
        }
        if let Some(n) = &q.number {
            query = query.bind(n.trim());
        }
        if let Some(s) = &q.supplier {
            query = query.bind(supplier_key(s));
        }
        let mut orders = query.fetch_all(&self.db).await?;
        for o in &mut orders {
            self.fill(o).await?;
        }
        Ok(orders)
    }

    async fn fill(&self, o: &mut PurchaseOrder) -> Result<()> {
        o.lines = sqlx::query_as::<_, OrderLine>(
            "SELECT l.id, l.position, l.kind, l.name, l.sku, l.variant_id, l.bag, l.quantity, \
             l.weight_g, l.price, l.taxed, l.url, l.bean_id, b.name AS bean_name, l.wanted_id, \
             l.move_id, l.received_g FROM purchase_order_lines l \
             LEFT JOIN beans b ON b.id = l.bean_id WHERE l.order_id = ? ORDER BY l.position",
        )
        .bind(&o.id)
        .fetch_all(&self.db)
        .await?;
        o.receipts = sqlx::query_as::<_, Receipt>(
            "SELECT id, order_id, kind, content_type, file_name, message_id, sha256, size_bytes, \
             added_at FROM purchase_receipts WHERE order_id = ? ORDER BY added_at",
        )
        .bind(&o.id)
        .fetch_all(&self.db)
        .await?;
        o.coffee_g = o
            .lines
            .iter()
            .filter(|l| l.kind == "coffee")
            .filter_map(|l| l.received_g.or(l.weight_g))
            .sum::<f64>();
        o.coffee_g = (o.coffee_g * 10.0).round() / 10.0;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Result<Option<PurchaseOrder>> {
        let Some(mut o) =
            sqlx::query_as::<_, PurchaseOrder>("SELECT * FROM purchase_orders WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.db)
                .await?
        else {
            return Ok(None);
        };
        self.fill(&mut o).await?;
        Ok(Some(o))
    }

    /// It shipped (again: the tracking corrected).
    pub async fn shipped(&self, id: &str, req: ShippedRequest) -> Result<Option<PurchaseOrder>> {
        let Some(o) = self.get(id).await? else {
            return Ok(None);
        };
        if !matches!(o.status.as_str(), "ordered" | "shipped") {
            return Err(bad(format!("this order is {} already", o.status)));
        }
        let at = purchase_time(req.at.as_deref())?;
        sqlx::query(
            "UPDATE purchase_orders SET status = 'shipped', carrier = COALESCE(?, carrier), \
             tracking = COALESCE(?, tracking), shipped_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(clean(req.carrier))
        .bind(clean(req.tracking))
        .bind(at)
        .bind(Utc::now())
        .bind(id)
        .execute(&self.db)
        .await?;
        self.get(id).await
    }

    /// It came: one purchase per coffee line, with its share of the discount, shipping and
    /// tax. The shopping-list entries it filled are bought.
    pub async fn received(&self, id: &str, req: ReceivedRequest) -> Result<Option<PurchaseOrder>> {
        let Some(o) = self.get(id).await? else {
            return Ok(None);
        };
        if !matches!(o.status.as_str(), "ordered" | "shipped") {
            return Err(bad(format!("this order is {} already", o.status)));
        }
        let at = purchase_time(req.at.as_deref())?;
        let mut weight: Vec<Option<f64>> = o.lines.iter().map(|l| l.weight_g).collect();
        let mut arrived: Vec<Option<f64>> = vec![None; o.lines.len()];
        let mut bean: Vec<Option<String>> = o.lines.iter().map(|l| l.bean_id.clone()).collect();
        let inv = InventoryService::new(self.db.clone());
        for r in &req.lines {
            let key = r.line.trim();
            let i = o
                .lines
                .iter()
                .position(|l| l.id == key || l.position.to_string() == key)
                .ok_or_else(|| bad(format!("no line {key} in this order")))?;
            if let Some(g) = r.weight_g {
                if !g.is_finite() || g < 0.0 {
                    return Err(bad(format!("line {key}: a weight is 0 or more")));
                }
                weight[i] = Some(g);
                arrived[i] = Some(g);
            }
            if let Some(b) = clean(r.bean_id.clone()) {
                inv.bean(&b)
                    .await?
                    .ok_or_else(|| bad(format!("no bean {b}")))?;
                bean[i] = Some(b);
            }
        }
        for (i, l) in o.lines.iter().enumerate() {
            if l.kind != "coffee" {
                continue;
            }
            if weight[i].is_none() {
                return Err(bad(format!(
                    "line {} ({}) has no weight: give what arrived",
                    l.position, l.name
                )));
            }
            if bean[i].is_none() {
                return Err(bad(format!(
                    "line {} ({}) has no bean (it was removed): give one",
                    l.position, l.name
                )));
            }
        }
        let parts: Vec<_> = o
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| (l.kind == "coffee", l.price, weight[i], l.taxed))
            .collect();
        let split = shares(&parts, o.discount, o.shipping, o.tax);
        let arrival: HashMap<String, Option<String>> = self.arrivals(&o).await?;
        let now = Utc::now();
        let mut tx = crate::db::begin_write(&self.db).await?;
        for (i, l) in o.lines.iter().enumerate() {
            if l.kind != "coffee" {
                continue;
            }
            let g = weight[i].unwrap_or(0.0);
            let (d, s, t) = split[i];
            let mut move_id = None;
            if g > 0.0 {
                let b = bean[i].as_deref().unwrap_or_default();
                move_id = Some(
                    insert_move(
                        &mut tx,
                        NewMove {
                            bean_id: b,
                            at: Some(at),
                            delta_g: g as f32,
                            kind: "purchase",
                            note: Some(format!("{} order {}", o.supplier, o.order_number)),
                            price: Some(cents((l.price - d).max(0.0))),
                            shipping: Some(s),
                            fees: Some(t),
                            currency: o.currency.clone(),
                            supplier: Some(o.supplier.clone()),
                            crop_year: None,
                            arrival: arrival.get(&l.id).cloned().flatten(),
                            order_id: Some(o.id.clone()),
                        },
                    )
                    .await?,
                );
            }
            sqlx::query(
                "UPDATE purchase_order_lines SET move_id = ?, received_g = ?, bean_id = ? \
                 WHERE id = ?",
            )
            .bind(&move_id)
            .bind(arrived[i])
            .bind(&bean[i])
            .bind(&l.id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE wanted SET status = 'bought', move_id = ?, bean_id = ?, bought_at = ?, \
                 updated_at = ? WHERE order_line_id = ?",
            )
            .bind(&move_id)
            .bind(&bean[i])
            .bind(now)
            .bind(now)
            .bind(&l.id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "UPDATE purchase_orders SET status = 'received', received_at = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(at)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get(id).await
    }

    /// When each line's lot landed at the supplier, from the shopping list's draft.
    async fn arrivals(&self, o: &PurchaseOrder) -> Result<HashMap<String, Option<String>>> {
        let mut out = HashMap::new();
        for l in &o.lines {
            let Some(w) = &l.wanted_id else { continue };
            let d = sqlx::query_scalar::<_, Option<sqlx::types::Json<Draft>>>(
                "SELECT draft FROM wanted WHERE id = ?",
            )
            .bind(w)
            .fetch_optional(&self.db)
            .await?
            .flatten();
            out.insert(l.id.clone(), d.and_then(|d| d.arrival.clone()));
        }
        Ok(out)
    }

    /// It will not come: the shopping-list entries it filled are wanted again.
    pub async fn cancel(&self, id: &str) -> Result<Option<PurchaseOrder>> {
        let Some(o) = self.get(id).await? else {
            return Ok(None);
        };
        if !matches!(o.status.as_str(), "ordered" | "shipped") {
            return Err(bad(format!("this order is {} already", o.status)));
        }
        let mut tx = crate::db::begin_write(&self.db).await?;
        self.unfill(&mut tx, id).await?;
        sqlx::query("UPDATE purchase_orders SET status = 'cancelled', updated_at = ? WHERE id = ?")
            .bind(Utc::now())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.get(id).await
    }

    /// Shopping-list entries an open order filled are wanted again; bought ones stay bought.
    async fn unfill(&self, tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE wanted SET status = CASE WHEN status = 'ordered' THEN 'wanted' ELSE status END, \
             order_line_id = NULL, updated_at = ? \
             WHERE order_line_id IN (SELECT id FROM purchase_order_lines WHERE order_id = ?)",
        )
        .bind(Utc::now())
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Remove an order, its lines and receipts; a received one's purchases too (the stock and
    /// costs change). Beans made for it stay.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let mut tx = crate::db::begin_write(&self.db).await?;
        let found: Option<String> =
            sqlx::query_scalar("SELECT status FROM purchase_orders WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if found.is_none() {
            return Ok(false);
        }
        self.unfill(&mut tx, id).await?;
        sqlx::query(
            "UPDATE wanted SET move_id = NULL WHERE move_id IN \
             (SELECT id FROM bean_stock_moves WHERE order_id = ? AND kind = 'purchase')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM bean_stock_moves WHERE order_id = ? AND kind = 'purchase'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM purchase_orders WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    // ---- receipts ----

    /// Keep a receipt with its order. The same file twice (by SHA-256) is refused, naming the
    /// order that has it.
    pub async fn add_receipt(
        &self,
        order_id: &str,
        kind: Option<&str>,
        content_type: Option<&str>,
        file_name: Option<&str>,
        message_id: Option<&str>,
        body: &[u8],
    ) -> Result<Option<Receipt>> {
        if self.get(order_id).await?.is_none() {
            return Ok(None);
        }
        let kind = kind.unwrap_or("confirmation");
        if !matches!(kind, "confirmation" | "shipping" | "invoice" | "other") {
            return Err(bad(
                "a receipt is a confirmation, shipping (notice), invoice or other",
            ));
        }
        if body.is_empty() {
            return Err(bad("the receipt is empty"));
        }
        if body.len() > MAX_RECEIPT_BYTES {
            return Err(bad(format!(
                "a receipt is at most {} MB",
                MAX_RECEIPT_BYTES >> 20
            )));
        }
        let file_name = file_name
            .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).trim())
            .filter(|f| !f.is_empty())
            .map(|f| {
                f.chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect::<String>()
            });
        let ct = receipt_type(content_type, file_name.as_deref()).ok_or_else(|| {
            bad("a receipt is an email (.eml), a web page (.html), a PDF, an image (PNG, JPEG, WebP, HEIC) or text")
        })?;
        let sha = format!("{:x}", Sha256::digest(body));
        let held: Option<(String, String)> = sqlx::query_as(
            "SELECT o.supplier, o.order_number FROM purchase_receipts r \
             JOIN purchase_orders o ON o.id = r.order_id WHERE r.sha256 = ?",
        )
        .bind(&sha)
        .fetch_optional(&self.db)
        .await?;
        if let Some((s, n)) = held {
            return Err(
                OrderConflict(format!("this receipt is kept already, with {s} order {n}")).into(),
            );
        }
        let (count, bytes): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM purchase_receipts WHERE order_id = ?",
        )
        .bind(order_id)
        .fetch_one(&self.db)
        .await?;
        if count >= MAX_RECEIPTS || bytes + body.len() as i64 > MAX_ORDER_RECEIPT_BYTES {
            return Err(bad(format!(
                "an order keeps at most {MAX_RECEIPTS} receipts and {} MB",
                MAX_ORDER_RECEIPT_BYTES >> 20
            )));
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO purchase_receipts (id, order_id, kind, content_type, file_name, \
             message_id, sha256, size_bytes, body, added_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(order_id)
        .bind(kind)
        .bind(ct)
        .bind(file_name)
        .bind(message_id.map(str::trim).filter(|m| !m.is_empty()))
        .bind(&sha)
        .bind(body.len() as i64)
        .bind(body)
        .bind(Utc::now())
        .execute(&self.db)
        .await?;
        let r = sqlx::query_as::<_, Receipt>(
            "SELECT id, order_id, kind, content_type, file_name, message_id, sha256, size_bytes, \
             added_at FROM purchase_receipts WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&self.db)
        .await?;
        Ok(Some(r))
    }

    /// A receipt's file.
    pub async fn receipt(&self, order_id: &str, id: &str) -> Result<Option<(Receipt, Vec<u8>)>> {
        let r = sqlx::query_as::<_, Receipt>(
            "SELECT id, order_id, kind, content_type, file_name, message_id, sha256, size_bytes, \
             added_at FROM purchase_receipts WHERE id = ? AND order_id = ?",
        )
        .bind(id)
        .bind(order_id)
        .fetch_optional(&self.db)
        .await?;
        let Some(r) = r else { return Ok(None) };
        let body: Vec<u8> = sqlx::query_scalar("SELECT body FROM purchase_receipts WHERE id = ?")
            .bind(id)
            .fetch_one(&self.db)
            .await
            .context("receipt vanished")?;
        Ok(Some((r, body)))
    }

    pub async fn delete_receipt(&self, order_id: &str, id: &str) -> Result<bool> {
        Ok(
            sqlx::query("DELETE FROM purchase_receipts WHERE id = ? AND order_id = ?")
                .bind(id)
                .bind(order_id)
                .execute(&self.db)
                .await?
                .rows_affected()
                > 0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::inventory::BeanRequest;

    #[test]
    fn splits_add_up_to_the_cent() {
        assert_eq!(split_cents(10.0, &[1.0, 1.0, 1.0]), vec![3.34, 3.33, 3.33]);
        assert_eq!(split_cents(8.99, &[453.6, 907.2]), vec![3.0, 5.99]);
        assert_eq!(split_cents(0.21, &[0.0, 0.0]), vec![0.11, 0.1]);
        let s = split_cents(14.0, &[1360.8, 907.2]);
        assert_eq!(s, vec![8.4, 5.6]);
        for (t, w) in [(128.06, vec![3.0, 7.0, 11.0]), (0.01, vec![5.0, 5.0])] {
            let parts = split_cents(t, &w);
            assert!((parts.iter().sum::<f64>() - t).abs() < 1e-9, "{parts:?}");
        }
    }

    #[test]
    fn codes_and_names() {
        assert_eq!(coffee_code("GCX-8087-001"), "GCX-8087");
        assert_eq!(coffee_code("gcx-8087"), "GCX-8087");
        assert!(same_sku("GCX-8087-005", "GCX-8087"));
        assert!(!same_sku("GCX-8087-005", "GCX-8088-005"));
        assert_eq!(
            name_key("Kenya Nyeri Gatugi AA - Green"),
            "kenya nyeri gatugi aa"
        );
        assert_eq!(supplier_key("Sweet Maria's"), supplier_key("sweet marias"));
        assert_eq!(
            receipt_type(None, Some("Order 123.eml")),
            Some("message/rfc822")
        );
        assert_eq!(
            receipt_type(Some("application/pdf"), None),
            Some("application/pdf")
        );
        assert_eq!(
            receipt_type(Some("application/x-msdownload"), Some("a.exe")),
            None
        );
    }

    fn bean(name: &str, sku: Option<&str>) -> BeanRequest {
        BeanRequest {
            name: Some(name.into()),
            supplier: Some(Some("Sweet Maria's".into())),
            sku: Some(sku.map(Into::into)),
            ..Default::default()
        }
    }

    /// The owner's 2024 order, shortened: two coffees, a colour card that was taxed.
    fn receipt() -> OrderImport {
        OrderImport {
            supplier: "Sweet Maria's".into(),
            order_number: "000412345".into(),
            placed_at: Some("2024-12-31".into()),
            currency: Some("USD".into()),
            subtotal: Some(21.65),
            shipping: Some(8.99),
            tax: Some(0.21),
            total: 30.85,
            lines: vec![
                ImportLine {
                    name: "Kenya Nyeri Gatugi AA".into(),
                    sku: Some("GCX-8087-001".into()),
                    bag: Some("1 LB".into()),
                    quantity: Some(1.0),
                    price: 9.15,
                    ..Default::default()
                },
                ImportLine {
                    name: "Ethiopia Guji Hambela".into(),
                    bag: Some("1 LB".into()),
                    quantity: Some(1.0),
                    price: 9.50,
                    ..Default::default()
                },
                ImportLine {
                    kind: Some("other".into()),
                    name: "Roasted Coffee Color Card".into(),
                    sku: Some("MISC-CFCL-0422".into()),
                    price: 3.00,
                    taxed: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_receipt_is_checked_ordered_received_and_never_twice() {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        let kenya = inv
            .create_bean(bean("Kenya Nyeri Gatugi AA", Some("GCX-8087")))
            .await
            .unwrap();
        let svc = OrderService::new(db.clone());

        // The dry run: the totals add up, Kenya by its code, the Guji new, the card other
        let dry = svc.import(receipt(), true).await.unwrap();
        let c = &dry.check;
        assert!(c.ok, "{:?}", c.problems);
        assert!(c.totals.adds_up);
        assert_eq!(c.totals.computed, 30.85);
        assert_eq!(
            c.lines
                .iter()
                .map(|l| l.result.as_str())
                .collect::<Vec<_>>(),
            ["matched", "new", "other"]
        );
        assert_eq!(c.lines[0].by.as_deref(), Some("sku"));
        assert_eq!(c.lines[0].bean_id.as_deref(), Some(kenya.id.as_str()));
        // Shipping by weight over the coffee, the tax on the card alone
        assert_eq!(
            (
                c.lines[0].shipping,
                c.lines[1].shipping,
                c.lines[2].shipping
            ),
            (4.5, 4.49, 0.0)
        );
        assert_eq!((c.lines[0].tax, c.lines[2].tax), (0.0, 0.21));
        assert_eq!(c.coffee_g, 907.2);
        assert!(svc.list(&OrdersQuery::default()).await.unwrap().is_empty());

        // Recorded: on order, no stock yet
        let o = svc.import(receipt(), false).await.unwrap().order.unwrap();
        assert_eq!(o.status, "ordered");
        assert_eq!(o.lines.len(), 3);
        let k = inv.bean(&kenya.id).await.unwrap().unwrap();
        assert_eq!((k.stock_g, k.on_order_g), (0.0, 453.6));
        let guji = o.lines[1].bean_id.clone().unwrap();
        assert_eq!(
            inv.bean(&guji).await.unwrap().unwrap().supplier.as_deref(),
            Some("Sweet Maria's")
        );

        // The same order again: a conflict, nothing more
        let again = svc.import(receipt(), false).await.unwrap_err();
        assert!(again.downcast_ref::<OrderConflict>().is_some(), "{again}");
        let dup = svc.import(receipt(), true).await.unwrap().check;
        assert_eq!(dup.duplicate.unwrap().by, "order_number");

        // Shipped, then received with the Guji a little light
        let o = svc
            .shipped(
                &o.id,
                ShippedRequest {
                    carrier: Some("USPS".into()),
                    tracking: Some("9400".into()),
                    at: Some("2025-01-03".into()),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(o.status, "shipped");
        let o = svc
            .received(
                &o.id,
                ReceivedRequest {
                    at: Some("2025-01-06".into()),
                    lines: vec![ReceivedLine {
                        line: "2".into(),
                        weight_g: Some(440.0),
                        bean_id: None,
                    }],
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(o.status, "received");
        let k = inv.bean_detail(&kenya.id).await.unwrap().unwrap();
        assert_eq!((k.bean.stock_g, k.bean.on_order_g), (453.6, 0.0));
        let m = &k.moves[0];
        assert_eq!(
            (m.price, m.fees, m.order_id.as_deref()),
            (Some(9.15), Some(0.0), Some(o.id.as_str()))
        );
        // 8.99 split 453.6 : 440
        assert_eq!(m.shipping, Some(4.56));
        assert_eq!(inv.bean(&guji).await.unwrap().unwrap().stock_g, 440.0);
        assert!(svc
            .received(&o.id, ReceivedRequest::default())
            .await
            .is_err());

        // Removing it takes its purchases with it
        assert!(svc.delete(&o.id).await.unwrap());
        assert_eq!(inv.bean(&kenya.id).await.unwrap().unwrap().stock_g, 0.0);
    }

    #[tokio::test]
    async fn totals_that_do_not_add_up_are_refused_unless_accepted() {
        let db = crate::db::open_memory().await;
        let svc = OrderService::new(db);
        let mut r = receipt();
        r.total = 31.85;
        let c = svc.check(&r).await.unwrap();
        assert!(!c.ok && !c.totals.adds_up);
        assert_eq!(c.totals.difference, 1.0);
        assert!(c.problems[0].contains("off by +1.00"), "{}", c.problems[0]);
        assert!(svc.import(r.clone(), false).await.is_err());
        r.accept_mismatch = Some("a $1 tip the receipt does not show".into());
        let o = svc.import(r, false).await.unwrap();
        assert_eq!(
            o.order.unwrap().mismatch_reason.as_deref(),
            Some("a $1 tip the receipt does not show")
        );
    }

    #[tokio::test]
    async fn names_close_but_not_the_same_ask_the_owner() {
        let db = crate::db::open_memory().await;
        let inv = InventoryService::new(db.clone());
        let a = inv.create_bean(bean("Kenya Nyeri", None)).await.unwrap();
        inv.create_bean(bean("Ethiopia Guji Hambela", None))
            .await
            .unwrap();
        let svc = OrderService::new(db);
        let mut r = receipt();
        r.lines[0].sku = None;
        r.lines[0].name = "Kenya Nyeri Gatugi AA".into();
        let c = svc.check(&r).await.unwrap();
        assert_eq!(c.lines[0].result, "ambiguous");
        assert_eq!(c.lines[0].candidates[0].id, a.id);
        assert_eq!(
            (c.lines[1].result.as_str(), c.lines[1].by.as_deref()),
            ("matched", Some("name"))
        );
        assert!(!c.ok);
        r.lines[0].new_bean = true;
        assert!(svc.check(&r).await.unwrap().ok);
        r.lines[0].new_bean = false;
        r.lines[0].bean_id = Some(a.id.clone());
        let c = svc.check(&r).await.unwrap();
        assert_eq!(
            (c.lines[0].result.as_str(), c.lines[0].bean_id.as_deref()),
            ("chosen", Some(a.id.as_str()))
        );
    }

    #[tokio::test]
    async fn a_shopping_list_entry_is_ordered_then_bought() {
        let db = crate::db::open_memory().await;
        let wanted = crate::coffee::wanted::WantedService::new(db.clone());
        let d = crate::coffee::import::shopify(
            include_str!("../../testdata/suppliers/bodhileaf-honduras-copan.json"),
            "Bodhi Leaf",
            "https://bodhileafcoffee.com/products/honduras-copan-organic-green",
        )
        .unwrap();
        let variant = d
            .sizes
            .iter()
            .find(|z| z.label == "5 LB")
            .unwrap()
            .variant_id
            .clone();
        let w = wanted
            .add(crate::coffee::wanted::WantedRequest {
                draft: Some(d),
                size: Some("5 lb".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let svc = OrderService::new(db.clone());
        let r = OrderImport {
            supplier: "Bodhi Leaf".into(),
            order_number: "#1001".into(),
            total: 50.09,
            shipping: Some(8.0),
            lines: vec![ImportLine {
                name: "Honduras Organic Copan - Green".into(),
                variant_id: variant,
                bag: Some("5 lb".into()),
                price: 42.09,
                ..Default::default()
            }],
            ..Default::default()
        };
        let c = svc.check(&r).await.unwrap();
        assert_eq!(
            (c.lines[0].result.as_str(), c.lines[0].by.as_deref()),
            ("new", Some("wanted"))
        );
        assert_eq!(c.lines[0].wanted_id.as_deref(), Some(w.id.as_str()));
        let o = svc.import(r, false).await.unwrap().order.unwrap();
        let e = wanted.get(&w.id).await.unwrap().unwrap();
        assert_eq!(e.status, "ordered");
        // The bean is the draft's, with its process
        let b = InventoryService::new(db.clone())
            .bean(o.lines[0].bean_id.as_deref().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (b.name.as_str(), b.process.as_deref()),
            ("Honduras Organic Copan", Some("Washed"))
        );
        assert!(wanted
            .bought(&w.id, crate::coffee::wanted::BoughtRequest::default())
            .await
            .is_err());
        svc.received(&o.id, ReceivedRequest::default())
            .await
            .unwrap();
        let e = wanted.get(&w.id).await.unwrap().unwrap();
        assert_eq!(e.status, "bought");
        assert!(e.move_id.is_some());
    }

    #[tokio::test]
    async fn receipts_are_kept_once_and_cancel_puts_the_list_back() {
        let db = crate::db::open_memory().await;
        let svc = OrderService::new(db.clone());
        let o = svc.import(receipt(), false).await.unwrap().order.unwrap();
        let eml = b"From: orders@example.com\r\nSubject: Order 000412345\r\n\r\nThanks";
        let r = svc
            .add_receipt(
                &o.id,
                None,
                None,
                Some("confirmation.eml"),
                Some("<a@b>"),
                eml,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (r.content_type.as_str(), r.size_bytes),
            ("message/rfc822", eml.len() as i64)
        );
        let twice = svc
            .add_receipt(&o.id, Some("shipping"), None, Some("x.eml"), None, eml)
            .await
            .unwrap_err();
        assert!(twice.downcast_ref::<OrderConflict>().is_some());
        assert!(svc
            .add_receipt(&o.id, None, None, Some("a.exe"), None, b"MZ")
            .await
            .is_err());
        let (meta, body) = svc.receipt(&o.id, &r.id).await.unwrap().unwrap();
        assert_eq!((meta.sha256.len(), body.as_slice()), (64, &eml[..]));
        // A second import naming that receipt is a duplicate, whatever its number says
        let mut again = receipt();
        again.order_number = "000412346".into();
        again.receipt_sha256 = vec![meta.sha256.clone()];
        let c = svc.check(&again).await.unwrap();
        assert_eq!(c.duplicate.unwrap().by, "receipt");

        let o = svc.cancel(&o.id).await.unwrap().unwrap();
        assert_eq!(o.status, "cancelled");
        assert!(svc.cancel(&o.id).await.is_err());
        assert!(svc.delete(&o.id).await.unwrap());
        assert!(svc.receipt(&o.id, &r.id).await.unwrap().is_none());
    }
}
