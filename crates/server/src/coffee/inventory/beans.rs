//! Beans: listed, read with their ledger and cost, made, edited and deleted or archived.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::coffee::costs::{self, Ledger, MoveIn};
use crate::roasts::types::RoastSession;

use super::stock::{insert_move, move_in, NewMove};
use super::types::{
    clean, details_json, finite_nonneg, Bean, BeanDeleted, BeanDetail, BeanRequest, StockMove, Val,
    BEAN_SELECT,
};
use super::{bad, cost_settings, InventoryService};

impl InventoryService {
    pub async fn list_beans(&self, include_archived: bool) -> Result<Vec<Bean>> {
        let q = if include_archived {
            format!("{BEAN_SELECT} ORDER BY b.archived, b.name COLLATE NOCASE")
        } else {
            format!("{BEAN_SELECT} WHERE b.archived = 0 ORDER BY b.name COLLATE NOCASE")
        };
        let mut beans = sqlx::query_as::<_, Bean>(&q).fetch_all(&self.db).await?;
        let (home, _) = cost_settings(&self.db).await;
        let ledgers = self.ledgers(None, &home).await?;
        for b in &mut beans {
            if let Some(l) = ledgers.get(&b.id) {
                fill_cost(b, l, &home);
            }
            suggest_label_notes(b);
        }
        Ok(beans)
    }

    pub async fn bean(&self, id: &str) -> Result<Option<Bean>> {
        let Some(mut b) = sqlx::query_as::<_, Bean>(&format!("{BEAN_SELECT} WHERE b.id = ?"))
            .bind(id)
            .fetch_optional(&self.db)
            .await?
        else {
            return Ok(None);
        };
        let (home, _) = cost_settings(&self.db).await;
        if let Some(l) = self.ledgers(Some(id), &home).await?.get(id) {
            fill_cost(&mut b, l, &home);
        }
        suggest_label_notes(&mut b);
        Ok(Some(b))
    }

    /// Every bean's ledger (or one bean's), walked oldest first.
    pub(super) async fn ledgers(
        &self,
        bean_id: Option<&str>,
        home: &str,
    ) -> Result<HashMap<String, Ledger>> {
        let q = format!(
            "SELECT * FROM bean_stock_moves {} ORDER BY bean_id, at, rowid",
            if bean_id.is_some() {
                "WHERE bean_id = ?"
            } else {
                ""
            }
        );
        let mut query = sqlx::query_as::<_, StockMove>(&q);
        if let Some(b) = bean_id {
            query = query.bind(b);
        }
        let moves = query.fetch_all(&self.db).await?;
        let mut by_bean: HashMap<String, Vec<MoveIn>> = HashMap::new();
        for m in moves {
            by_bean
                .entry(m.bean_id.clone())
                .or_default()
                .push(move_in(&m));
        }
        Ok(by_bean
            .into_iter()
            .map(|(b, ms)| (b, costs::walk(&ms, home)))
            .collect())
    }

    pub async fn bean_detail(&self, id: &str) -> Result<Option<BeanDetail>> {
        let Some(bean) = self.bean(id).await? else {
            return Ok(None);
        };
        let mut moves = sqlx::query_as::<_, StockMove>(
            "SELECT * FROM bean_stock_moves WHERE bean_id = ? ORDER BY at DESC, rowid DESC",
        )
        .bind(id)
        .fetch_all(&self.db)
        .await?;
        let (home, _) = cost_settings(&self.db).await;
        if let Some(l) = self.ledgers(Some(id), &home).await?.get(id) {
            for m in &mut moves {
                if let Some(c) = l.moves.get(&m.id) {
                    m.cost = c.cost.map(costs::cents);
                    m.cost_per_kg = c.avg_per_kg.map(costs::cents);
                }
            }
        }
        let roasts = sqlx::query_as::<_, RoastSession>(
            "SELECT * FROM roast_sessions WHERE bean_id = ? ORDER BY created_at DESC",
        )
        .bind(id)
        .fetch_all(&self.db)
        .await?;
        Ok(Some(BeanDetail {
            bean,
            moves,
            roasts,
        }))
    }

    pub async fn create_bean(&self, req: BeanRequest) -> Result<Bean> {
        req.check_details()?;
        let texts = req.detail_texts();
        let score = req.score.flatten();
        let details = details_json(req.details.clone().flatten());
        let name = clean(req.name).ok_or_else(|| bad("a bean needs a name"))?;
        let low = finite_nonneg(req.low_stock_g, "low stock")?.unwrap_or(500.0);
        let stock = finite_nonneg(req.stock_g, "stock")?;
        let moisture = finite_nonneg(req.moisture_pct.flatten(), "moisture")?;
        let density = finite_nonneg(req.density_g_l.flatten(), "density")?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let mut tx = self.db.begin().await?;
        sqlx::query(
            "INSERT INTO beans (id, name, origin, variety, process, lot, supplier, notes, \
             moisture_pct, density_g_l, low_stock_g, archived, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(clean(req.origin.flatten()))
        .bind(clean(req.variety.flatten()))
        .bind(clean(req.process.flatten()))
        .bind(clean(req.lot.flatten()))
        .bind(clean(req.supplier.clone().flatten()))
        .bind(clean(req.notes.flatten()))
        .bind(moisture)
        .bind(density)
        .bind(low)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for (col, v) in texts {
            if let Some(t) = clean(v.flatten()) {
                sqlx::query(&format!("UPDATE beans SET {col} = ? WHERE id = ?"))
                    .bind(t)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("UPDATE beans SET score = ?, details = ? WHERE id = ?")
            .bind(score)
            .bind(details)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if let Some(g) = stock.filter(|g| *g > 0.0) {
            insert_move(
                &mut tx,
                NewMove {
                    bean_id: &id,
                    delta_g: g,
                    kind: "purchase",
                    note: Some("first stock".into()),
                    supplier: clean(req.supplier.clone().flatten()),
                    ..Default::default()
                },
            )
            .await?;
        }
        tx.commit().await?;
        self.bean(&id).await?.context("bean not found after insert")
    }

    pub async fn update_bean(&self, id: &str, req: BeanRequest) -> Result<Option<Bean>> {
        req.check_details()?;
        let mut sets: Vec<(&str, Val)> = Vec::new();
        let texts = req.detail_texts();
        for (col, v) in texts {
            if let Some(v) = v {
                sets.push((col, Val::Text(clean(v))));
            }
        }
        if let Some(v) = req.score {
            sets.push(("score", Val::Real(v)));
        }
        if let Some(v) = req.details.clone() {
            sets.push(("details", Val::Text(details_json(v))));
        }
        if let Some(n) = req.name {
            let n = clean(Some(n)).ok_or_else(|| bad("a bean needs a name"))?;
            sets.push(("name", Val::Text(Some(n))));
        }
        for (col, v) in [
            ("origin", req.origin),
            ("variety", req.variety),
            ("process", req.process),
            ("lot", req.lot),
            ("supplier", req.supplier),
            ("notes", req.notes),
        ] {
            if let Some(v) = v {
                sets.push((col, Val::Text(clean(v))));
            }
        }
        if let Some(v) = req.moisture_pct {
            sets.push(("moisture_pct", Val::Real(finite_nonneg(v, "moisture")?)));
        }
        if let Some(v) = req.density_g_l {
            sets.push(("density_g_l", Val::Real(finite_nonneg(v, "density")?)));
        }
        if let Some(v) = req.low_stock_g {
            sets.push((
                "low_stock_g",
                Val::Real(finite_nonneg(Some(v), "low stock")?),
            ));
        }
        if let Some(v) = req.archived {
            sets.push(("archived", Val::Bool(v)));
        }
        if !sets.is_empty() {
            let mut q = "UPDATE beans SET updated_at = ?".to_string();
            for (col, _) in &sets {
                let _ = write!(q, ", {col} = ?");
            }
            q.push_str(" WHERE id = ?");
            let mut query = sqlx::query(&q).bind(Utc::now());
            for (_, v) in sets {
                query = match v {
                    Val::Text(t) => query.bind(t),
                    Val::Real(r) => query.bind(r),
                    Val::Bool(b) => query.bind(b),
                };
            }
            query.bind(id).execute(&self.db).await?;
        }
        self.bean(id).await
    }

    /// Deletes a bean nobody roasted; archives one with roasts.
    pub async fn delete_bean(&self, id: &str) -> Result<BeanDeleted> {
        let Some(bean) = self.bean(id).await? else {
            return Ok(BeanDeleted::NotFound);
        };
        if bean.roast_count > 0 {
            sqlx::query("UPDATE beans SET archived = 1, updated_at = ? WHERE id = ?")
                .bind(Utc::now())
                .bind(id)
                .execute(&self.db)
                .await?;
            return Ok(BeanDeleted::Archived);
        }
        sqlx::query("DELETE FROM beans WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(BeanDeleted::Deleted)
    }
}

/// A bean's supplier link as stored; `None` without one, or without the bean.
pub async fn bean_link(db: &sqlx::SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT url FROM beans WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
}

/// Tasting words from the supplier's notes, to start a bean's label notes from
fn suggest_label_notes(b: &mut Bean) {
    b.label_notes_suggested = match (&b.label_notes, &b.cupping_notes) {
        (None, Some(prose)) => crate::labels::suggest_notes(prose),
        _ => None,
    };
}

fn fill_cost(b: &mut Bean, l: &Ledger, home: &str) {
    b.cost_per_kg = l.avg_per_kg.map(costs::cents);
    b.stock_value = l.stock_value().map(costs::cents);
    b.currency = l.avg_per_kg.map(|_| home.to_string());
}
