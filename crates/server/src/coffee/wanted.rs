//! The shopping list (`docs/green-coffee.md`, "Shopping list", migration 029): coffees
//! wanted, often from a supplier's link. "Bought" turns an entry into a purchase: the bean
//! (made from the import's draft when it is new) and a stock move with the price.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::AssertSqlSafe;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::coffee::import::Draft;
use crate::coffee::inventory::{
    Bean, BeanRequest, InventoryInputError, InventoryService, PurchaseFields, StockMove,
    StockRequest,
};

fn bad(msg: impl Into<String>) -> anyhow::Error {
    InventoryInputError(msg.into()).into()
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

#[derive(Debug, Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct Wanted {
    pub id: String,
    pub name: String,
    pub supplier: Option<String>,
    pub url: Option<String>,
    /// An existing bean to buy again
    pub bean_id: Option<String>,
    /// The bag size chosen: "5 LB"
    pub size_label: Option<String>,
    pub weight_g: Option<f64>,
    /// The price of that size when it was seen (one bag)
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub notes: Option<String>,
    /// Bags of that size (migration 055)
    pub quantity: i64,
    /// The import's draft, to make the bean from
    #[schema(value_type = Option<Draft>)]
    pub draft: Option<sqlx::types::Json<Draft>>,
    /// wanted | ordered (an order's line fills it, docs/green-coffee.md "Orders") | bought
    pub status: String,
    /// The purchase it became
    pub move_id: Option<String>,
    /// The order line that fills it
    pub order_line_id: Option<String>,
    pub bought_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Joined: the bean's name
    pub bean_name: Option<String>,
}

const SELECT: &str =
    "SELECT w.*, b.name AS bean_name FROM wanted w LEFT JOIN beans b ON b.id = w.bean_id";

/// Put a coffee on the list: from an import's `draft` (pick the bag by `size` label or
/// `weight_g`), a known `bean_id`, or just a name.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct WantedRequest {
    pub draft: Option<Draft>,
    pub bean_id: Option<String>,
    pub name: Option<String>,
    pub supplier: Option<String>,
    pub url: Option<String>,
    /// A size label from the draft ("5 LB")
    pub size: Option<String>,
    pub weight_g: Option<f64>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub notes: Option<String>,
    /// Bags of that size (default 1)
    pub quantity: Option<i64>,
}

/// Edit an entry: absent fields are kept.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct WantedUpdate {
    pub name: Option<String>,
    pub size: Option<String>,
    pub weight_g: Option<f64>,
    pub price: Option<f64>,
    pub notes: Option<String>,
    pub bean_id: Option<String>,
    /// Bags of that size
    pub quantity: Option<i64>,
}

/// It was bought: what arrived, if it differs from the entry, and what it cost.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct BoughtRequest {
    /// Default: the entry's weight × its quantity
    pub weight_g: Option<f64>,
    /// Default: the entry's price × its quantity (scaled to the weight that came)
    pub price: Option<f64>,
    pub shipping: Option<f64>,
    pub fees: Option<f64>,
    /// YYYY-MM-DD or RFC 3339 (default now)
    pub at: Option<String>,
    /// Add to this bean instead of the entry's (or a new one from the draft)
    pub bean_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Bought {
    pub wanted: Wanted,
    pub bean: Bean,
    /// The purchase
    pub purchase: Option<StockMove>,
}

/// What a draft makes as a bean.
pub fn bean_from_draft(d: &Draft) -> BeanRequest {
    let t = |v: &Option<String>| Some(v.clone());
    BeanRequest {
        name: Some(d.name.clone()),
        origin: t(&d.origin),
        variety: t(&d.variety),
        process: t(&d.process),
        lot: t(&d.lot),
        supplier: t(&d.supplier),
        region: t(&d.region),
        producer: t(&d.producer),
        altitude: t(&d.altitude),
        grade: t(&d.grade),
        screen_size: t(&d.screen_size),
        certifications: t(&d.certifications),
        drying: t(&d.drying),
        score: Some(d.score),
        roast_recommendation: t(&d.roast_recommendation),
        cupping_notes: t(&d.cupping_notes),
        url: Some(Some(d.url.clone())),
        sku: t(&d.sku),
        details: (!d.details.is_empty()).then(|| Some(d.details.clone())),
        ..Default::default()
    }
}

/// Bags: a whole number from 1 to 100.
fn quantity(q: Option<i64>) -> Result<Option<i64>> {
    match q {
        Some(n) if !(1..=100).contains(&n) => Err(bad("a quantity is 1 to 100 bags")),
        other => Ok(other),
    }
}

#[derive(Clone)]
pub struct WantedService {
    db: SqlitePool,
}

impl WantedService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    fn inventory(&self) -> InventoryService {
        InventoryService::new(self.db.clone())
    }

    /// Wanted and on order first (oldest first), then, with `all`, what was bought (newest
    /// first).
    pub async fn list(&self, all: bool) -> Result<Vec<Wanted>> {
        let q = if all {
            format!("{SELECT} ORDER BY w.status = 'bought', CASE WHEN w.status = 'bought' THEN w.bought_at END DESC, w.created_at")
        } else {
            format!("{SELECT} WHERE w.status IN ('wanted', 'ordered') ORDER BY w.created_at")
        };
        Ok(sqlx::query_as::<_, Wanted>(AssertSqlSafe(q.as_str()))
            .fetch_all(&self.db)
            .await?)
    }

    pub async fn get(&self, id: &str) -> Result<Option<Wanted>> {
        Ok(
            sqlx::query_as::<_, Wanted>(AssertSqlSafe(format!("{SELECT} WHERE w.id = ?")))
                .bind(id)
                .fetch_optional(&self.db)
                .await?,
        )
    }

    pub async fn add(&self, req: WantedRequest) -> Result<Wanted> {
        let mut name = clean(req.name);
        let mut supplier = clean(req.supplier);
        let mut url = clean(req.url);
        let (mut label, mut weight, mut price, mut currency) = (
            clean(req.size.clone()),
            req.weight_g,
            req.price,
            clean(req.currency),
        );
        let bean_id = clean(req.bean_id);
        if let Some(b) = &bean_id {
            let bean = self
                .inventory()
                .bean(b)
                .await?
                .ok_or_else(|| bad(format!("no bean {b}")))?;
            name = name.or(Some(bean.name));
            supplier = supplier.or(bean.supplier);
            url = url.or(bean.url);
        }
        if let Some(d) = &req.draft {
            name = name.or(Some(d.name.clone()));
            supplier = supplier.or(d.supplier.clone());
            url = url.or(Some(d.url.clone()));
            let size = match (&req.size, req.weight_g) {
                (Some(l), _) => Some(
                    d.sizes
                        .iter()
                        .find(|z| z.label.eq_ignore_ascii_case(l.trim()))
                        .ok_or_else(|| {
                            bad(format!(
                                "no size {l:?}; the sizes are {}",
                                d.sizes
                                    .iter()
                                    .map(|z| z.label.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                        })?,
                ),
                (None, Some(g)) => d.size_for(g),
                (None, None) => None,
            };
            if let Some(z) = size {
                label = Some(z.label.clone());
                weight = weight.or(z.weight_g);
                price = price.or(z.price);
                currency = currency.or(z.currency.clone());
            }
        }
        let name = name.ok_or_else(|| bad("a wanted coffee needs a name, a bean or a draft"))?;
        let quantity = quantity(req.quantity)?.unwrap_or(1);
        for (v, what) in [(weight, "weight"), (price, "price")] {
            if v.is_some_and(|x| !x.is_finite() || x < 0.0) {
                return Err(bad(format!("{what} must be 0 or more")));
            }
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO wanted (id, name, supplier, url, bean_id, size_label, weight_g, price, \
             currency, notes, draft, quantity, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'wanted', ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(supplier)
        .bind(url)
        .bind(bean_id)
        .bind(label)
        .bind(weight)
        .bind(price)
        .bind(currency.map(|c| c.to_ascii_uppercase()))
        .bind(clean(req.notes))
        .bind(
            req.draft
                .as_ref()
                .and_then(|d| serde_json::to_string(d).ok()),
        )
        .bind(quantity)
        .bind(now)
        .bind(now)
        .execute(&self.db)
        .await?;
        self.get(&id)
            .await?
            .context("wanted entry not found after insert")
    }

    pub async fn update(&self, id: &str, req: WantedUpdate) -> Result<Option<Wanted>> {
        let Some(w) = self.get(id).await? else {
            return Ok(None);
        };
        let mut label = w.size_label.clone();
        let mut weight = req.weight_g.or(w.weight_g);
        let mut price = req.price.or(w.price);
        if let (Some(l), Some(d)) = (&req.size, &w.draft) {
            let z = d
                .sizes
                .iter()
                .find(|z| z.label.eq_ignore_ascii_case(l.trim()))
                .ok_or_else(|| bad(format!("no size {l:?}")))?;
            label = Some(z.label.clone());
            weight = req.weight_g.or(z.weight_g);
            price = req.price.or(z.price);
        }
        sqlx::query(
            "UPDATE wanted SET name = ?, size_label = ?, weight_g = ?, price = ?, notes = ?, \
             bean_id = ?, quantity = ?, updated_at = ? WHERE id = ?",
        )
        .bind(clean(req.name).unwrap_or(w.name))
        .bind(label)
        .bind(weight)
        .bind(price)
        .bind(match req.notes {
            Some(n) => clean(Some(n)),
            None => w.notes,
        })
        .bind(clean(req.bean_id).or(w.bean_id))
        .bind(quantity(req.quantity)?.unwrap_or(w.quantity))
        .bind(Utc::now())
        .bind(id)
        .execute(&self.db)
        .await?;
        self.get(id).await
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM wanted WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?
            .rows_affected()
            > 0)
    }

    /// It arrived: the bean (made from the draft when it is new) gets a purchase.
    pub async fn bought(&self, id: &str, req: BoughtRequest) -> Result<Option<Bought>> {
        let Some(w) = self.get(id).await? else {
            return Ok(None);
        };
        if w.status == "bought" {
            return Err(bad("this was bought already"));
        }
        if w.status == "ordered" {
            return Err(bad(
                "this is on order: record the order received (`roast order received`)",
            ));
        }
        let bags = w.quantity.max(1) as f64;
        let weight = req
            .weight_g
            .or(w.weight_g.map(|g| g * bags))
            .filter(|g| g.is_finite() && *g > 0.0)
            .ok_or_else(|| bad("how much arrived? give weight_g"))?;
        // The entry's price is for one bag: scale it to the weight that arrived.
        let price = req.price.or(match (w.price, w.weight_g) {
            (Some(p), Some(g)) if g > 0.0 => Some(p * weight / g),
            _ => None,
        });
        let inv = self.inventory();
        let bean = match clean(req.bean_id).or(w.bean_id.clone()) {
            Some(b) => inv
                .bean(&b)
                .await?
                .ok_or_else(|| bad(format!("no bean {b}")))?,
            None => {
                let mut r = match &w.draft {
                    Some(d) => bean_from_draft(d),
                    None => BeanRequest {
                        name: Some(w.name.clone()),
                        supplier: Some(w.supplier.clone()),
                        url: Some(w.url.clone()),
                        ..Default::default()
                    },
                };
                r.stock_g = None;
                inv.create_bean(r).await?
            }
        };
        let after = inv
            .add_stock(
                &bean.id,
                StockRequest {
                    delta_g: Some(weight as f32),
                    note: clean(req.note).or_else(|| {
                        w.size_label
                            .clone()
                            .map(|l| format!("{l} from the shopping list"))
                    }),
                    purchase: PurchaseFields {
                        price: price.map(crate::coffee::costs::cents),
                        shipping: req.shipping,
                        fees: req.fees,
                        currency: w.currency.clone(),
                        supplier: w.supplier.clone(),
                        crop_year: None,
                        arrival: w.draft.as_ref().and_then(|d| d.arrival.clone()),
                        at: req.at,
                    },
                    ..Default::default()
                },
            )
            .await?
            .context("bean vanished")?;
        let detail = inv.bean_detail(&bean.id).await?.context("bean vanished")?;
        let purchase = detail
            .moves
            .into_iter()
            .filter(|m| m.kind == "purchase")
            .max_by(|a, b| a.at.cmp(&b.at));
        sqlx::query(
            "UPDATE wanted SET status = 'bought', bean_id = ?, move_id = ?, bought_at = ?, \
             updated_at = ? WHERE id = ?",
        )
        .bind(&bean.id)
        .bind(purchase.as_ref().map(|m| m.id.clone()))
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(id)
        .execute(&self.db)
        .await?;
        Ok(Some(Bought {
            wanted: self.get(id).await?.context("entry vanished")?,
            bean: after,
            purchase,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffee::import::shopify;

    #[tokio::test]
    async fn a_draft_on_the_list_becomes_a_bean_and_a_purchase() {
        let db = crate::db::open_memory().await;
        let svc = WantedService::new(db.clone());
        let d = shopify(
            include_str!("../../testdata/suppliers/bodhileaf-honduras-copan.json"),
            "Bodhi Leaf",
            "https://bodhileafcoffee.com/products/honduras-copan-organic-green",
        )
        .unwrap();
        let w = svc
            .add(WantedRequest {
                draft: Some(d.clone()),
                size: Some("5 lb".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(w.name, "Honduras Organic Copan");
        assert_eq!(
            (w.size_label.as_deref(), w.price),
            (Some("5 LB"), Some(42.09))
        );
        assert_eq!(w.weight_g, Some(2268.0));
        assert!(svc
            .add(WantedRequest {
                draft: Some(d),
                size: Some("7 lb".into()),
                ..Default::default()
            })
            .await
            .is_err());
        assert_eq!(svc.list(false).await.unwrap().len(), 1);

        let b = svc
            .bought(
                &w.id,
                BoughtRequest {
                    shipping: Some(8.0),
                    at: Some("2026-10-20".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(b.bean.name, "Honduras Organic Copan");
        assert_eq!(b.bean.process.as_deref(), Some("Washed"));
        assert_eq!(b.bean.stock_g, 2268.0);
        // (42.09 + 8) / 2.268 kg
        assert_eq!(b.bean.cost_per_kg, Some(22.09));
        let p = b.purchase.unwrap();
        assert_eq!(
            (p.price, p.supplier.as_deref()),
            (Some(42.09), Some("Bodhi Leaf"))
        );
        assert_eq!(b.wanted.status, "bought");
        assert_eq!(b.wanted.bean_id.as_deref(), Some(b.bean.id.as_str()));
        assert!(svc.list(false).await.unwrap().is_empty());
        assert_eq!(svc.list(true).await.unwrap().len(), 1);
        assert!(svc.bought(&w.id, BoughtRequest::default()).await.is_err());

        // Again, for the known bean, with a different weight: the price scales.
        let again = svc
            .add(WantedRequest {
                bean_id: Some(b.bean.id.clone()),
                weight_g: Some(1000.0),
                price: Some(20.0),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(again.bean_name.as_deref(), Some("Honduras Organic Copan"));
        let b2 = svc
            .bought(
                &again.id,
                BoughtRequest {
                    weight_g: Some(500.0),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(b2.bean.id, b.bean.id);
        assert_eq!(b2.purchase.unwrap().price, Some(10.0));
    }
}
