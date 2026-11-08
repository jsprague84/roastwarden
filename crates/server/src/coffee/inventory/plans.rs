//! Planned roasts: the queue in order, a plan made, edited, deleted and the queue reordered.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use super::types::{clean, finite_nonneg, PlanRequest, PlannedRoast, Val, PLAN_SELECT};
use super::{bad, InventoryService};

impl InventoryService {
    /// The queue in order; `done` adds the plans that ran (newest last). `device_id`
    /// keeps the plans for that roaster and those for any roaster.
    pub async fn list_plans(
        &self,
        device_id: Option<&str>,
        done: bool,
    ) -> Result<Vec<PlannedRoast>> {
        let mut q = format!("{PLAN_SELECT} WHERE 1 = 1");
        if !done {
            q.push_str(" AND p.session_id IS NULL");
        }
        if device_id.is_some() {
            q.push_str(" AND (p.device_id IS NULL OR p.device_id = ?)");
        }
        q.push_str(" ORDER BY p.session_id IS NOT NULL, p.position, p.created_at");
        let mut query = sqlx::query_as::<_, PlannedRoast>(&q);
        if let Some(d) = device_id {
            query = query.bind(d);
        }
        Ok(query.fetch_all(&self.db).await?)
    }

    pub async fn plan(&self, id: &str) -> Result<Option<PlannedRoast>> {
        Ok(
            sqlx::query_as::<_, PlannedRoast>(&format!("{PLAN_SELECT} WHERE p.id = ?"))
                .bind(id)
                .fetch_optional(&self.db)
                .await?,
        )
    }

    /// A plan's bean and profile must exist (a typo would start a roast with neither).
    async fn check_refs(&self, bean_id: Option<&str>, profile_id: Option<&str>) -> Result<()> {
        if let Some(b) = bean_id {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM beans WHERE id = ?")
                .bind(b)
                .fetch_one(&self.db)
                .await?;
            if n == 0 {
                return Err(bad(format!("no bean {b}")));
            }
        }
        if let Some(p) = profile_id {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roast_profiles WHERE id = ?")
                .bind(p)
                .fetch_one(&self.db)
                .await?;
            if n == 0 {
                return Err(bad(format!("no profile {p}")));
            }
        }
        Ok(())
    }

    pub async fn create_plan(&self, req: PlanRequest) -> Result<PlannedRoast> {
        let weight = finite_nonneg(req.green_weight_g.flatten(), "green weight")?;
        let bean_id = clean(req.bean_id.clone().flatten());
        let profile_id = clean(req.profile_id.clone().flatten());
        self.check_refs(bean_id.as_deref(), profile_id.as_deref())
            .await?;
        let position = match req.position {
            Some(p) => p,
            None => {
                let max: Option<f64> = sqlx::query_scalar(
                    "SELECT MAX(position) FROM planned_roasts WHERE session_id IS NULL",
                )
                .fetch_one(&self.db)
                .await?;
                max.map_or(1.0, |m| m.floor() + 1.0)
            }
        };
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO planned_roasts (id, position, device_id, bean_id, profile_id, \
             green_weight_g, name, notes, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(position)
        .bind(clean(req.device_id.flatten()))
        .bind(bean_id)
        .bind(profile_id)
        .bind(weight)
        .bind(clean(req.name.flatten()))
        .bind(clean(req.notes.flatten()))
        .bind(now)
        .bind(now)
        .execute(&self.db)
        .await?;
        self.plan(&id).await?.context("plan not found after insert")
    }

    pub async fn update_plan(&self, id: &str, req: PlanRequest) -> Result<Option<PlannedRoast>> {
        self.check_refs(
            clean(req.bean_id.clone().flatten()).as_deref(),
            clean(req.profile_id.clone().flatten()).as_deref(),
        )
        .await?;
        let mut sets: Vec<(&str, Val)> = Vec::new();
        for (col, v) in [
            ("device_id", req.device_id),
            ("bean_id", req.bean_id),
            ("profile_id", req.profile_id),
            ("name", req.name),
            ("notes", req.notes),
        ] {
            if let Some(v) = v {
                sets.push((col, Val::Text(clean(v))));
            }
        }
        if let Some(v) = req.green_weight_g {
            sets.push((
                "green_weight_g",
                Val::Real(finite_nonneg(v, "green weight")?),
            ));
        }
        if !sets.is_empty() || req.position.is_some() {
            let mut q = "UPDATE planned_roasts SET updated_at = ?".to_string();
            for (col, _) in &sets {
                let _ = write!(q, ", {col} = ?");
            }
            if req.position.is_some() {
                q.push_str(", position = ?");
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
            if let Some(p) = req.position {
                query = query.bind(p);
            }
            query.bind(id).execute(&self.db).await?;
        }
        self.plan(id).await
    }

    pub async fn delete_plan(&self, id: &str) -> Result<bool> {
        let r = sqlx::query("DELETE FROM planned_roasts WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// The queue's new order: positions 1, 2, … for the ids given, in that order.
    pub async fn reorder_plans(&self, ids: &[String]) -> Result<Vec<PlannedRoast>> {
        let mut tx = self.db.begin().await?;
        let now = Utc::now();
        for (i, id) in ids.iter().enumerate() {
            sqlx::query("UPDATE planned_roasts SET position = ?, updated_at = ? WHERE id = ?")
                .bind((i + 1) as f64)
                .bind(now)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.list_plans(None, false).await
    }
}
