//! The stock ledger's own writes: a purchase or a correction added, edited and removed, an
//! order recorded as a purchase per line; a move read with its cost.

use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::coffee::costs::{self, MoveIn};

use super::types::{
    Bean, MoveRequest, OrderRecorded, OrderRequest, StockMove, StockRequest, clean, currency_code,
    finite_nonneg, money, purchase_time,
};
use super::{InventoryService, bad};

/// A purchase move to write.
#[derive(Default)]
pub(crate) struct NewMove<'a> {
    pub(crate) bean_id: &'a str,
    pub(crate) at: Option<DateTime<Utc>>,
    pub(crate) delta_g: f32,
    pub(crate) kind: &'a str,
    pub(crate) note: Option<String>,
    pub(crate) price: Option<f64>,
    pub(crate) shipping: Option<f64>,
    pub(crate) fees: Option<f64>,
    pub(crate) currency: Option<String>,
    pub(crate) supplier: Option<String>,
    pub(crate) crop_year: Option<String>,
    pub(crate) arrival: Option<String>,
    pub(crate) order_id: Option<String>,
}

impl InventoryService {
    /// A purchase or a correction. `set_g` writes the difference to the current stock.
    pub async fn add_stock(&self, id: &str, req: StockRequest) -> Result<Option<Bean>> {
        let Some(bean) = self.bean(id).await? else {
            return Ok(None);
        };
        let (delta, kind) = match (req.set_g, req.delta_g) {
            (Some(set), _) => {
                let set = finite_nonneg(Some(set), "stock")?.unwrap_or(0.0);
                (set - bean.stock_g, "adjust")
            }
            (None, Some(d)) if d.is_finite() && d != 0.0 => {
                (d, if d > 0.0 { "purchase" } else { "adjust" })
            }
            _ => return Err(bad("give delta_g (not 0) or set_g")),
        };
        let p = req.purchase;
        if kind != "purchase" && !p.is_empty() {
            return Err(bad(
                "a price, supplier, crop year or arrival goes with a purchase (delta_g above 0)",
            ));
        }
        let at = purchase_time(p.at.as_deref())?;
        if delta.abs() < 1e-6 {
            return Ok(Some(bean));
        }
        let mut tx = self.db.begin().await?;
        insert_move(
            &mut tx,
            NewMove {
                bean_id: id,
                at: Some(at),
                delta_g: delta,
                kind,
                note: clean(req.note),
                price: money(p.price, "price")?,
                shipping: money(p.shipping, "shipping")?,
                fees: money(p.fees, "fees")?,
                currency: currency_code(p.currency)?,
                supplier: clean(p.supplier),
                crop_year: clean(p.crop_year),
                arrival: clean(p.arrival),
                order_id: None,
            },
        )
        .await?;
        tx.commit().await?;
        self.bean(id).await
    }

    /// Edit a purchase or a correction. A roast's move is the roast's: refused.
    pub async fn update_move(
        &self,
        bean_id: &str,
        move_id: &str,
        req: MoveRequest,
    ) -> Result<Option<StockMove>> {
        let Some(m) = self.stock_move(bean_id, move_id).await? else {
            return Ok(None);
        };
        if m.kind == "roast" {
            return Err(bad(
                "a roast's move follows the roast: change its green weight or bean there",
            ));
        }
        let delta = match req.delta_g {
            Some(d) if !d.is_finite() || d == 0.0 => return Err(bad("delta_g must not be 0")),
            Some(d) if m.kind == "purchase" && d < 0.0 => {
                return Err(bad("a purchase adds stock: delta_g above 0"));
            }
            Some(d) => d,
            None => m.delta_g,
        };
        let priced = [
            req.price.is_some(),
            req.shipping.is_some(),
            req.fees.is_some(),
            req.currency.is_some(),
            req.supplier.is_some(),
            req.crop_year.is_some(),
            req.arrival.is_some(),
        ]
        .contains(&true);
        if m.kind != "purchase" && priced {
            return Err(bad(
                "only a purchase has a price, supplier, crop year or arrival",
            ));
        }
        let pick_money = |new: Option<Option<f64>>, old: Option<f64>, what: &str| match new {
            Some(v) => money(v, what),
            None => Ok(old),
        };
        let pick_text = |new: Option<Option<String>>, old: Option<String>| match new {
            Some(v) => clean(v),
            None => old,
        };
        let at = match req.at.as_deref() {
            Some(t) => purchase_time(Some(t))?,
            None => m.at,
        };
        let currency = match req.currency {
            Some(v) => currency_code(v)?,
            None => m.currency.clone(),
        };
        sqlx::query(
            "UPDATE bean_stock_moves SET delta_g = ?, at = ?, note = ?, price = ?, shipping = ?, \
             fees = ?, currency = ?, supplier = ?, crop_year = ?, arrival = ? WHERE id = ?",
        )
        .bind(delta)
        .bind(at)
        .bind(pick_text(req.note, m.note.clone()))
        .bind(pick_money(req.price, m.price, "price")?)
        .bind(pick_money(req.shipping, m.shipping, "shipping")?)
        .bind(pick_money(req.fees, m.fees, "fees")?)
        .bind(currency)
        .bind(pick_text(req.supplier, m.supplier.clone()))
        .bind(pick_text(req.crop_year, m.crop_year.clone()))
        .bind(pick_text(req.arrival, m.arrival.clone()))
        .bind(move_id)
        .execute(&self.db)
        .await?;
        self.detail_move(bean_id, move_id).await
    }

    /// Remove a purchase or a correction from the ledger. `Ok(false)`: no such move.
    pub async fn delete_move(&self, bean_id: &str, move_id: &str) -> Result<bool> {
        let Some(m) = self.stock_move(bean_id, move_id).await? else {
            return Ok(false);
        };
        if m.kind == "roast" {
            return Err(bad("a roast's move goes when the roast does"));
        }
        sqlx::query("DELETE FROM bean_stock_moves WHERE id = ?")
            .bind(move_id)
            .execute(&self.db)
            .await?;
        Ok(true)
    }

    async fn stock_move(&self, bean_id: &str, move_id: &str) -> Result<Option<StockMove>> {
        Ok(sqlx::query_as::<_, StockMove>(
            "SELECT * FROM bean_stock_moves WHERE id = ? AND bean_id = ?",
        )
        .bind(move_id)
        .bind(bean_id)
        .fetch_optional(&self.db)
        .await?)
    }

    /// One move with its cost, as the bean's page lists it.
    async fn detail_move(&self, bean_id: &str, move_id: &str) -> Result<Option<StockMove>> {
        Ok(self
            .bean_detail(bean_id)
            .await?
            .and_then(|d| d.moves.into_iter().find(|m| m.id == move_id)))
    }

    /// One order: a purchase per line, the order's shipping and fees split by weight.
    pub async fn record_order(&self, req: OrderRequest) -> Result<OrderRecorded> {
        if req.lines.is_empty() {
            return Err(bad("an order needs at least one line"));
        }
        let at = purchase_time(req.at.as_deref())?;
        let currency = currency_code(req.currency)?;
        let shipping = money(req.shipping, "shipping")?;
        let fees = money(req.fees, "fees")?;
        let mut total_g = 0.0f64;
        for l in &req.lines {
            if !l.weight_g.is_finite() || l.weight_g <= 0.0 {
                return Err(bad("each line needs a weight above 0"));
            }
            money(l.price, "price")?;
            if self.bean(&l.bean_id).await?.is_none() {
                return Err(bad(format!("no bean {}", l.bean_id)));
            }
            total_g += l.weight_g as f64;
        }
        let order_id = Uuid::new_v4().to_string();
        let share =
            |total: Option<f64>, g: f32| total.map(|t| costs::cents(t * g as f64 / total_g));
        let mut tx = self.db.begin().await?;
        let mut ids = Vec::new();
        for l in &req.lines {
            ids.push(
                insert_move(
                    &mut tx,
                    NewMove {
                        bean_id: &l.bean_id,
                        at: Some(at),
                        delta_g: l.weight_g,
                        kind: "purchase",
                        note: clean(req.note.clone()),
                        price: l.price,
                        shipping: share(shipping, l.weight_g),
                        fees: share(fees, l.weight_g),
                        currency: currency.clone(),
                        supplier: clean(req.supplier.clone()),
                        crop_year: clean(l.crop_year.clone()),
                        arrival: clean(l.arrival.clone()),
                        order_id: Some(order_id.clone()),
                    },
                )
                .await?,
            );
        }
        tx.commit().await?;
        let mut moves = Vec::new();
        for (l, id) in req.lines.iter().zip(ids) {
            if let Some(m) = self.detail_move(&l.bean_id, &id).await? {
                moves.push(m);
            }
        }
        Ok(OrderRecorded { order_id, moves })
    }
}

pub(crate) async fn insert_move(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    m: NewMove<'_>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO bean_stock_moves (id, bean_id, at, delta_g, kind, note, price, shipping, \
         fees, currency, supplier, crop_year, arrival, order_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(m.bean_id)
    .bind(m.at.unwrap_or_else(Utc::now))
    .bind(m.delta_g)
    .bind(m.kind)
    .bind(m.note)
    .bind(m.price)
    .bind(m.shipping)
    .bind(m.fees)
    .bind(m.currency)
    .bind(m.supplier)
    .bind(m.crop_year)
    .bind(m.arrival)
    .bind(m.order_id)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

pub(super) fn move_in(m: &StockMove) -> MoveIn {
    MoveIn {
        id: m.id.clone(),
        delta_g: (m.delta_g as f64 * 10.0).round() / 10.0,
        kind: m.kind.clone(),
        price: m.price,
        shipping: m.shipping,
        fees: m.fees,
        currency: m.currency.clone(),
    }
}
