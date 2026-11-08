//! What coffee cost: a roast's green coffee, and month by month what was spent, bought and
//! roasted.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::coffee::costs::{self, Ledger, MoveIn, RoastCost};

use super::stock::move_in;
use super::types::{CostReport, MonthCosts, StockMove};
use super::{cost_settings, InventoryService};

impl InventoryService {
    /// What a roast's green coffee cost; `None` without a bean, a green weight or a priced
    /// purchase before it.
    pub async fn roast_cost(&self, session_id: &str) -> Result<Option<RoastCost>> {
        let row: Option<(Option<String>, Option<f32>, Option<f32>)> = sqlx::query_as(
            "SELECT bean_id, green_weight, roasted_weight FROM roast_sessions WHERE id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.db)
        .await?;
        let Some((Some(bean_id), Some(green), roasted)) = row else {
            return Ok(None);
        };
        let move_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM bean_stock_moves WHERE session_id = ? AND kind = 'roast'",
        )
        .bind(session_id)
        .fetch_optional(&self.db)
        .await?;
        let Some(move_id) = move_id else {
            return Ok(None);
        };
        let (home, sizes) = cost_settings(&self.db).await;
        let ledgers = self.ledgers(Some(&bean_id), &home).await?;
        let Some(cost) = ledgers
            .get(&bean_id)
            .and_then(|l| l.moves.get(&move_id))
            .and_then(|c| c.cost)
        else {
            return Ok(None);
        };
        Ok(costs::roast_cost(
            cost,
            green as f64,
            roasted.map(f64::from),
            &home,
            sizes,
        ))
    }

    /// Month by month (UTC): spent, bought, roasted, and what the roasted coffee cost.
    pub async fn cost_report(&self, months: usize) -> Result<CostReport> {
        let (home, _) = cost_settings(&self.db).await;
        let moves = sqlx::query_as::<_, StockMove>(
            "SELECT * FROM bean_stock_moves ORDER BY bean_id, at, rowid",
        )
        .fetch_all(&self.db)
        .await?;
        let mut by_bean: HashMap<String, Vec<MoveIn>> = HashMap::new();
        for m in &moves {
            by_bean
                .entry(m.bean_id.clone())
                .or_default()
                .push(move_in(m));
        }
        let ledgers: HashMap<String, Ledger> = by_bean
            .into_iter()
            .map(|(b, ms)| (b, costs::walk(&ms, &home)))
            .collect();
        let mut out: BTreeMap<String, MonthCosts> = BTreeMap::new();
        let month_of = |t: &DateTime<Utc>| t.format("%Y-%m").to_string();
        for m in &moves {
            let key = month_of(&m.at);
            let e = out.entry(key.clone()).or_insert_with(|| MonthCosts {
                month: key,
                ..Default::default()
            });
            let walked = ledgers.get(&m.bean_id).and_then(|l| l.moves.get(&m.id));
            match m.kind.as_str() {
                "purchase" if m.delta_g > 0.0 => {
                    e.bought_g += m.delta_g as f64;
                    if let Some(price) = m.price {
                        let total = price + m.shipping.unwrap_or(0.0) + m.fees.unwrap_or(0.0);
                        let cur = m.currency.clone().unwrap_or_else(|| home.clone());
                        if cur.eq_ignore_ascii_case(&home) {
                            e.spent += total;
                        } else {
                            *e.spent_other.entry(cur).or_default() += total;
                        }
                    }
                }
                "roast" => {
                    if let Some(c) = walked.and_then(|w| w.cost) {
                        e.roast_cost += c;
                        e.costed_roasts += 1;
                    }
                }
                _ => {}
            }
        }
        let roasts: Vec<RoastWeights> = sqlx::query_as(
            "SELECT created_at, start_time, green_weight, roasted_weight FROM roast_sessions",
        )
        .fetch_all(&self.db)
        .await?;
        for (created, started, green, roasted) in roasts {
            let key = month_of(&started.unwrap_or(created));
            let e = out.entry(key.clone()).or_insert_with(|| MonthCosts {
                month: key,
                ..Default::default()
            });
            e.roasts += 1;
            e.green_roasted_g += green.unwrap_or(0.0) as f64;
            e.roasted_g += roasted.unwrap_or(0.0) as f64;
        }
        let mut months_out: Vec<MonthCosts> = out
            .into_values()
            .rev()
            .take(months)
            .map(|mut m| {
                m.spent = costs::cents(m.spent);
                m.roast_cost = costs::cents(m.roast_cost);
                m.bought_g = (m.bought_g * 10.0).round() / 10.0;
                m.green_roasted_g = (m.green_roasted_g * 10.0).round() / 10.0;
                m.roasted_g = (m.roasted_g * 10.0).round() / 10.0;
                for v in m.spent_other.values_mut() {
                    *v = costs::cents(*v);
                }
                m
            })
            .collect();
        months_out.shrink_to_fit();
        Ok(CostReport {
            currency: home,
            months: months_out,
        })
    }
}

/// A roast's created and start time, green and roasted weight.
type RoastWeights = (
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<f32>,
    Option<f32>,
);
