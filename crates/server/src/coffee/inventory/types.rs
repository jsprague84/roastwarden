//! The records the API gives and takes: a bean and its stock, a move, the requests (a
//! bean, stock, a purchase, an order, a plan), the cost report, and the checks on their fields.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::http::patch::double_option;
use crate::roasts::types::RoastSession;

use super::bad;

#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct Bean {
    pub id: String,
    pub name: String,
    pub origin: Option<String>,
    pub variety: Option<String>,
    pub process: Option<String>,
    pub lot: Option<String>,
    pub supplier: Option<String>,
    pub notes: Option<String>,
    pub moisture_pct: Option<f32>,
    pub density_g_l: Option<f32>,
    pub low_stock_g: f32,
    pub archived: bool,
    // What the supplier publishes (migration 028, docs/green-coffee.md)
    pub region: Option<String>,
    /// Farm, station, cooperative or grower
    pub producer: Option<String>,
    /// As published: "1900–2200 masl"
    pub altitude: Option<String>,
    pub grade: Option<String>,
    pub screen_size: Option<String>,
    /// Comma-separated: "Organic, Fairtrade"
    pub certifications: Option<String>,
    pub drying: Option<String>,
    /// The supplier's cupping score
    pub score: Option<f32>,
    pub roast_recommendation: Option<String>,
    pub cupping_notes: Option<String>,
    /// The offer
    pub url: Option<String>,
    pub sku: Option<String>,
    /// What the supplier says that has no field of its own: {label: value}
    #[schema(value_type = Option<std::collections::BTreeMap<String, String>>)]
    pub details: Option<sqlx::types::Json<BTreeMap<String, String>>>,
    /// Its own label (docs/labels.md, "Label studio"): a design, `id` or `id@N`
    pub label_design: Option<String>,
    /// That design's parameters
    #[schema(value_type = Option<Object>)]
    pub label_params: Option<sqlx::types::Json<serde_json::Map<String, serde_json::Value>>>,
    /// A brand kit's id
    pub label_brand: Option<String>,
    /// A few tasting words for its labels, "chocolate · coconut · apple" (migration 044):
    /// printed instead of the supplier's notes
    pub label_notes: Option<String>,
    /// The name its labels print when the bean's own is long, "Kenya Gatugi"
    pub short_name: Option<String>,
    /// With no `label_notes`: tasting words picked from the supplier's notes, to start from
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_notes_suggested: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// SUM of the ledger.
    pub stock_g: f32,
    pub roast_count: i32,
    /// Stock below `low_stock_g`.
    pub low: bool,
    /// Ordered or shipped and not received yet (docs/green-coffee.md, "Orders")
    pub on_order_g: f32,
    /// What a kg of the stock cost: the weighted average of the priced purchases mixed into
    /// what was left (absent before any priced purchase)
    #[sqlx(skip)]
    pub cost_per_kg: Option<f64>,
    /// The stock at `cost_per_kg`
    #[sqlx(skip)]
    pub stock_value: Option<f64>,
    /// The home currency, with a cost
    #[sqlx(skip)]
    pub currency: Option<String>,
}

pub(super) const BEAN_SELECT: &str = "SELECT b.*, \
     ROUND(COALESCE((SELECT SUM(delta_g) FROM bean_stock_moves m WHERE m.bean_id = b.id), 0.0), 1) AS stock_g, \
     (SELECT COUNT(*) FROM roast_sessions s WHERE s.bean_id = b.id) AS roast_count, \
     ROUND(COALESCE((SELECT SUM(delta_g) FROM bean_stock_moves m WHERE m.bean_id = b.id), 0.0), 1) < b.low_stock_g AS low, \
     ROUND(COALESCE((SELECT SUM(l.weight_g) FROM purchase_order_lines l \
       JOIN purchase_orders o ON o.id = l.order_id \
       WHERE l.bean_id = b.id AND l.kind = 'coffee' AND o.status IN ('ordered', 'shipped')), 0.0), 1) AS on_order_g \
     FROM beans b";

#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct StockMove {
    pub id: String,
    pub bean_id: String,
    pub at: DateTime<Utc>,
    pub delta_g: f32,
    /// purchase | adjust | roast
    pub kind: String,
    pub session_id: Option<String>,
    pub note: Option<String>,
    /// A purchase: the coffee's price for these grams
    pub price: Option<f64>,
    /// This purchase's share of the shipping
    pub shipping: Option<f64>,
    /// Its share of taxes, duty and fees
    pub fees: Option<f64>,
    /// ISO 4217; absent means the home currency
    pub currency: Option<String>,
    pub supplier: Option<String>,
    /// "2026" or "2025/26"
    pub crop_year: Option<String>,
    /// When the lot landed at the supplier, e.g. "2026-07"
    pub arrival: Option<String>,
    /// The moves of one order (`POST /api/purchases`) share it
    pub order_id: Option<String>,
    /// A priced purchase: what it cost (price + shipping + fees). A draw: what the beans
    /// were worth at the average of that moment. Home currency.
    #[sqlx(skip)]
    pub cost: Option<f64>,
    /// The bean's cost per kg after this move
    #[sqlx(skip)]
    pub cost_per_kg: Option<f64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BeanDetail {
    #[serde(flatten)]
    pub bean: Bean,
    /// Newest first.
    pub moves: Vec<StockMove>,
    /// Newest first.
    pub roasts: Vec<RoastSession>,
}

/// Create or edit a bean. On edit, absent fields are kept; `null` clears the optional ones.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct BeanRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub origin: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub variety: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub process: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub lot: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub supplier: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub moisture_pct: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub density_g_l: Option<Option<f32>>,
    pub low_stock_g: Option<f32>,
    pub archived: Option<bool>,
    /// Create only: the first purchase.
    pub stock_g: Option<f32>,
    #[serde(default, deserialize_with = "double_option")]
    pub region: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub producer: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub altitude: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub grade: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub screen_size: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub certifications: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub drying: Option<Option<String>>,
    /// 0 to 100
    #[serde(default, deserialize_with = "double_option")]
    pub score: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub roast_recommendation: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cupping_notes: Option<Option<String>>,
    /// A few tasting words for its labels ("chocolate · coconut · apple"), at most 120
    /// characters
    #[serde(default, deserialize_with = "double_option")]
    pub label_notes: Option<Option<String>>,
    /// The name its labels print, at most 40 characters
    #[serde(default, deserialize_with = "double_option")]
    pub short_name: Option<Option<String>>,
    /// An http(s) link to the offer
    #[serde(default, deserialize_with = "double_option")]
    pub url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub sku: Option<Option<String>>,
    /// {label: value}; replaces what was there
    #[serde(default, deserialize_with = "double_option")]
    pub details: Option<Option<BTreeMap<String, String>>>,
}

impl BeanRequest {
    /// The supplier-detail text columns and their values.
    pub(super) fn detail_texts(&self) -> [(&'static str, Option<Option<String>>); 13] {
        [
            ("region", self.region.clone()),
            ("producer", self.producer.clone()),
            ("altitude", self.altitude.clone()),
            ("grade", self.grade.clone()),
            ("screen_size", self.screen_size.clone()),
            ("certifications", self.certifications.clone()),
            ("drying", self.drying.clone()),
            ("roast_recommendation", self.roast_recommendation.clone()),
            ("cupping_notes", self.cupping_notes.clone()),
            ("label_notes", self.label_notes.clone()),
            ("short_name", self.short_name.clone()),
            ("url", self.url.clone()),
            ("sku", self.sku.clone()),
        ]
    }

    pub(super) fn check_details(&self) -> Result<()> {
        if let Some(Some(sc)) = self.score
            && (!sc.is_finite() || !(0.0..=100.0).contains(&sc))
        {
            return Err(bad("a score is from 0 to 100"));
        }
        if let Some(Some(n)) = &self.label_notes
            && n.trim().chars().count() > 120
        {
            return Err(bad(
                "label notes are a few tasting words, at most 120 characters (\"chocolate · coconut · apple\")",
            ));
        }
        if let Some(Some(n)) = &self.short_name
            && n.trim().chars().count() > 40
        {
            return Err(bad("a short name is at most 40 characters"));
        }
        if let Some(Some(u)) = &self.url {
            let u = u.trim();
            let web = u.is_empty() || u.starts_with("https://") || u.starts_with("http://");
            if !web {
                return Err(bad("a link starts with https:// or http://"));
            }
        }
        Ok(())
    }
}

pub(super) fn details_json(d: Option<BTreeMap<String, String>>) -> Option<String> {
    d.filter(|m| !m.is_empty())
        .and_then(|m| serde_json::to_string(&m).ok())
}

/// A purchase (`delta_g`, or any signed change) or "set stock to" (`set_g`). The price
/// fields go with a purchase (`delta_g` above 0) only.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct StockRequest {
    pub delta_g: Option<f32>,
    pub set_g: Option<f32>,
    pub note: Option<String>,
    #[serde(flatten)]
    pub purchase: PurchaseFields,
}

/// What a purchase cost and where it came from. Money in `currency` (default: the home
/// currency, Settings → Roasting).
#[derive(Debug, Default, Clone, Deserialize, utoipa::ToSchema)]
pub struct PurchaseFields {
    /// The coffee's price for the weight bought
    pub price: Option<f64>,
    pub shipping: Option<f64>,
    /// Taxes, duty, card fees
    pub fees: Option<f64>,
    /// ISO 4217, e.g. USD
    pub currency: Option<String>,
    pub supplier: Option<String>,
    /// "2026" or "2025/26"
    pub crop_year: Option<String>,
    /// When the lot landed at the supplier, e.g. "2026-07"
    pub arrival: Option<String>,
    /// When it was bought: RFC 3339 or YYYY-MM-DD (default now)
    pub at: Option<String>,
}

impl PurchaseFields {
    pub(super) fn is_empty(&self) -> bool {
        self.price.is_none()
            && self.shipping.is_none()
            && self.fees.is_none()
            && self.currency.is_none()
            && self.supplier.is_none()
            && self.crop_year.is_none()
            && self.arrival.is_none()
    }
}

/// Edit a purchase or a correction: absent fields are kept, `null` clears. A roast's move
/// follows its roast (edit the roast's green weight instead).
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct MoveRequest {
    pub delta_g: Option<f32>,
    /// RFC 3339 or YYYY-MM-DD
    pub at: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub note: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub price: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub shipping: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub fees: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub currency: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub supplier: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub crop_year: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub arrival: Option<Option<String>>,
}

/// One order from a supplier: several beans, one shipping and fee total, split over the
/// lines by weight.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct OrderRequest {
    pub supplier: Option<String>,
    /// ISO 4217 (default: the home currency)
    pub currency: Option<String>,
    /// The order's shipping, split by weight
    pub shipping: Option<f64>,
    /// The order's taxes and fees, split by weight
    pub fees: Option<f64>,
    /// RFC 3339 or YYYY-MM-DD (default now)
    pub at: Option<String>,
    pub note: Option<String>,
    pub lines: Vec<OrderLine>,
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct OrderLine {
    pub bean_id: String,
    pub weight_g: f32,
    /// This coffee's price for the weight
    pub price: Option<f64>,
    pub crop_year: Option<String>,
    pub arrival: Option<String>,
}

/// What an order recorded.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrderRecorded {
    pub order_id: String,
    /// The purchase moves, one per line, with their share of shipping and fees
    pub moves: Vec<StockMove>,
}

/// One month of green coffee: what was bought and spent, what was roasted and what that
/// coffee cost.
#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
pub struct MonthCosts {
    /// YYYY-MM (UTC)
    pub month: String,
    /// Priced purchases in the home currency: coffee, shipping and fees
    pub spent: f64,
    /// Purchases in other currencies, by currency (not in `spent`)
    pub spent_other: BTreeMap<String, f64>,
    pub bought_g: f64,
    /// Roasts started this month
    pub roasts: i64,
    pub green_roasted_g: f64,
    /// Roasted weight of the roasts that have one
    pub roasted_g: f64,
    /// The green coffee of the roasts with a bean and a known cost
    pub roast_cost: f64,
    /// Roasts whose cost is known (a bean with priced purchases)
    pub costed_roasts: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CostReport {
    pub currency: String,
    /// Newest first
    pub months: Vec<MonthCosts>,
}

#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct PlannedRoast {
    pub id: String,
    pub position: f64,
    pub device_id: Option<String>,
    pub bean_id: Option<String>,
    pub profile_id: Option<String>,
    pub green_weight_g: Option<f32>,
    pub name: Option<String>,
    pub notes: Option<String>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Joined for lists.
    pub bean_name: Option<String>,
    pub bean_stock_g: Option<f32>,
    pub profile_name: Option<String>,
    pub batch_number: Option<i32>,
}

pub(super) const PLAN_SELECT: &str = "SELECT p.*, b.name AS bean_name, \
     CASE WHEN b.id IS NULL THEN NULL ELSE \
     ROUND(COALESCE((SELECT SUM(delta_g) FROM bean_stock_moves m WHERE m.bean_id = p.bean_id), 0.0), 1) END AS bean_stock_g, \
     r.name AS profile_name, s.batch_number AS batch_number \
     FROM planned_roasts p \
     LEFT JOIN beans b ON b.id = p.bean_id \
     LEFT JOIN roast_profiles r ON r.id = p.profile_id \
     LEFT JOIN roast_sessions s ON s.id = p.session_id";

/// Create or edit a planned roast. On edit, absent fields are kept; `null` clears.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct PlanRequest {
    #[serde(default, deserialize_with = "double_option")]
    pub device_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub bean_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub profile_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub green_weight_g: Option<Option<f32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    pub position: Option<f64>,
}

/// What deleting a bean did.
#[derive(Debug, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BeanDeleted {
    Deleted,
    /// It has roasts: hidden instead, so they keep their bean.
    Archived,
    NotFound,
}

pub(crate) fn clean(v: Option<String>) -> Option<String> {
    v.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

pub(super) fn finite_nonneg(v: Option<f32>, what: &str) -> Result<Option<f32>> {
    match v {
        Some(x) if !x.is_finite() || x < 0.0 => Err(bad(format!("{what} must be 0 or more"))),
        other => Ok(other),
    }
}

pub(crate) fn money(v: Option<f64>, what: &str) -> Result<Option<f64>> {
    match v {
        Some(x) if !x.is_finite() || x < 0.0 => Err(bad(format!("{what} must be 0 or more"))),
        other => Ok(other),
    }
}

/// ISO 4217: three letters, kept upper case.
pub(crate) fn currency_code(v: Option<String>) -> Result<Option<String>> {
    match clean(v) {
        None => Ok(None),
        Some(c) if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) => {
            Ok(Some(c.to_ascii_uppercase()))
        }
        Some(c) => Err(bad(format!(
            "a currency is a three-letter code such as USD or EUR, not {c:?}"
        ))),
    }
}

/// When a purchase was made: RFC 3339, or a date (noon UTC, so it stays that day in any
/// time zone near ours). Not in the future.
pub(crate) fn purchase_time(v: Option<&str>) -> Result<DateTime<Utc>> {
    let Some(t) = v.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(Utc::now());
    };
    let at = if let Ok(d) = DateTime::parse_from_rfc3339(t) {
        d.with_timezone(&Utc)
    } else if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        d.and_hms_opt(12, 0, 0).expect("noon").and_utc()
    } else {
        return Err(bad(format!("a date is YYYY-MM-DD or RFC 3339, not {t:?}")));
    };
    if at > Utc::now() + chrono::Duration::days(1) {
        return Err(bad("a purchase cannot be in the future"));
    }
    Ok(at)
}

pub(super) enum Val {
    Text(Option<String>),
    Real(Option<f32>),
    Bool(bool),
}
