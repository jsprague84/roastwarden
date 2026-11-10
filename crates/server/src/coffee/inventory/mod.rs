//! Beans (green stock) and planned roasts (`docs/batches.md`, migration 018), purchases and
//! what the coffee cost (`docs/green-coffee.md`, migration 027).
//!
//! Stock is a ledger (`bean_stock_moves`); the moves a roast makes are kept by triggers, so
//! this module only writes purchases and adjustments. Costs are walked from the ledger on
//! every read (`costs.rs`): nothing derived is stored, so editing a purchase reprices the
//! roasts after it.

use sqlx::SqlitePool;

use crate::coffee::costs::Sizes;

mod beans;
mod plans;
mod report;
mod stock;
#[cfg(test)]
mod tests;
mod types;

pub use beans::bean_link;
pub(crate) use stock::{NewMove, insert_move};
pub use types::{
    Bean, BeanDeleted, BeanDetail, BeanRequest, CostReport, MoveRequest, OrderRecorded,
    OrderRequest, PlanRequest, PlannedRoast, PurchaseFields, StockMove, StockRequest,
};
pub(crate) use types::{clean, currency_code, money, purchase_time};

/// Bad input: the handlers answer 400 with the reason.
#[derive(Debug)]
pub struct InventoryInputError(pub String);

impl std::fmt::Display for InventoryInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InventoryInputError {}

pub(crate) fn bad(msg: impl Into<String>) -> anyhow::Error {
    InventoryInputError(msg.into()).into()
}

/// The home currency and the dose and bag sizes (Settings → Roasting).
pub async fn cost_settings(db: &SqlitePool) -> (String, Sizes) {
    let num = |v: Option<String>, d: f64| {
        v.and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|x| x.is_finite() && *x > 0.0)
            .unwrap_or(d)
    };
    let currency = crate::auth::settings::get(db, "currency")
        .await
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| c.len() == 3)
        .unwrap_or_else(|| "USD".into());
    let d = Sizes::default();
    let sizes = Sizes {
        espresso_g: num(
            crate::auth::settings::get(db, "dose_espresso_g").await,
            d.espresso_g,
        ),
        filter_g: num(
            crate::auth::settings::get(db, "dose_filter_g").await,
            d.filter_g,
        ),
        bag_g: num(crate::auth::settings::get(db, "bag_g").await, d.bag_g),
    };
    (currency, sizes)
}

#[derive(Clone)]
pub struct InventoryService {
    db: SqlitePool,
}

impl InventoryService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
}
