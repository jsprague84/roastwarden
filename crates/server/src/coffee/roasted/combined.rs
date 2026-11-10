//! Roasts in one bag or jar: a combined lot of one coffee or a blend, its moves split across
//! its roasts by what each has left, made and taken apart.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::AssertSqlSafe;
use uuid::Uuid;

use crate::ops::zone::Zone;

use super::lots::{LOT_SELECT, LotRow, combined_ref, lot, split};
use super::moves::{MOVE_COLUMNS, clean, insert_move, plan_move};
use super::types::{
    CombineRequest, CombinedLot, CombinedMove, CombinedRef, Form, MoveShare, RoastedLot,
    RoastedMove, RoastedMoveRequest,
};
use super::windows::Windows;
use super::{EMPTY_G, RoastedService, bad};

impl RoastedService {
    /// A move on a combined lot: planned against what its roasts have left together, then
    /// split across them by their share (`split`), the rows sharing one `combine_move` id.
    pub(super) async fn add_combined_move(
        &self,
        combine_id: &str,
        req: RoastedMoveRequest,
        label_run: Option<&str>,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let w = Windows::load(&self.db).await;
        let Some((c, members)) = self
            .combined_parts(combine_id, &w, zone.date(now), zone)
            .await?
        else {
            return Err(bad("no such combined lot"));
        };
        let lefts: Vec<f32> = members.iter().map(|l| l.left_g.unwrap_or(0.0)).collect();
        let weights: Vec<f32> = members.iter().map(|l| l.roasted_g.unwrap_or(0.0)).collect();
        let for_ = members
            .first()
            .map_or("any", |l| l.for_.as_str())
            .to_string();
        let (_, sizes) = crate::coffee::inventory::cost_settings(&self.db).await;
        let what = format!("#{} together", c.batches);
        let m = plan_move(req, c.left_g, &for_, &sizes, &what, now)?;
        if m.delta.abs() < 0.05 {
            return Ok(());
        }
        let group = Uuid::new_v4().to_string();
        let mut tx = crate::db::begin_write(&self.db).await?;
        for (l, d) in members.iter().zip(split(m.delta, &lefts, &weights)) {
            if d.abs() >= 0.05 {
                insert_move(&mut tx, &l.session_id, &m, d, label_run, Some(&group), now).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// The combined lot a roast is in
    pub(super) async fn combine_of(&self, session_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar(
                "SELECT combine_id FROM roasted_combine_roasts WHERE session_id = ?",
            )
            .bind(session_id)
            .fetch_optional(&self.db)
            .await?,
        )
    }

    /// A combined lot's summary and its roasts' lots (oldest first, no moves)
    pub(super) async fn combined_parts(
        &self,
        combine_id: &str,
        w: &Windows,
        today: NaiveDate,
        zone: Zone,
    ) -> Result<Option<(CombinedRef, Vec<RoastedLot>)>> {
        let name: Option<Option<String>> =
            sqlx::query_scalar("SELECT name FROM roasted_combines WHERE id = ?")
                .bind(combine_id)
                .fetch_optional(&self.db)
                .await?;
        let Some(name) = name else {
            return Ok(None);
        };
        let rows: Vec<LotRow> = sqlx::query_as(AssertSqlSafe(format!(
            "{LOT_SELECT} AND s.id IN (SELECT session_id FROM roasted_combine_roasts WHERE combine_id = ?)"
        )))
        .bind(combine_id)
        .fetch_all(&self.db)
        .await?;
        let mut members: Vec<RoastedLot> =
            rows.into_iter().map(|r| lot(r, w, today, zone)).collect();
        if members.is_empty() {
            return Ok(None);
        }
        members.sort_by_key(|l| l.roasted_at);
        let refs: Vec<&RoastedLot> = members.iter().collect();
        let c = combined_ref(combine_id, name.as_deref(), &refs, today);
        Ok(Some((c, members)))
    }

    /// Give each roast in a combined lot its lot's summary
    pub(super) async fn attach_combined(
        &self,
        lots: &mut [RoastedLot],
        w: &Windows,
        today: NaiveDate,
        zone: Zone,
    ) -> Result<()> {
        let pairs: Vec<(String, String)> =
            sqlx::query_as("SELECT session_id, combine_id FROM roasted_combine_roasts")
                .fetch_all(&self.db)
                .await?;
        let ids: std::collections::BTreeSet<&str> = pairs.iter().map(|p| p.1.as_str()).collect();
        for id in ids {
            if let Some((c, _)) = self.combined_parts(id, w, today, zone).await? {
                for l in lots.iter_mut().filter(|l| c.roasts.contains(&l.session_id)) {
                    l.combined = Some(c.clone());
                }
            }
        }
        Ok(())
    }

    /// A combined lot whole: by its id or any of its roasts' ids
    pub async fn combined(
        &self,
        id: &str,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<Option<CombinedLot>> {
        let w = Windows::load(&self.db).await;
        let today = zone.date(now);
        let cid = match self.combine_of(id).await? {
            Some(c) => c,
            None => id.to_string(),
        };
        let Some((c, members)) = self.combined_parts(&cid, &w, today, zone).await? else {
            return Ok(None);
        };
        let (note, created_at): (Option<String>, DateTime<Utc>) =
            sqlx::query_as("SELECT note, created_at FROM roasted_combines WHERE id = ?")
                .bind(&cid)
                .fetch_one(&self.db)
                .await?;
        let rows: Vec<RoastedMove> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT {MOVE_COLUMNS} FROM roasted_moves WHERE combine_move IS NOT NULL AND session_id IN \
             (SELECT session_id FROM roasted_combine_roasts WHERE combine_id = ?) ORDER BY at, created_at"
        )))
        .bind(&cid)
        .fetch_all(&self.db)
        .await?;
        let batch_of = |sid: &str| {
            members
                .iter()
                .find(|l| l.session_id == sid)
                .and_then(|l| l.batch_number)
        };
        let mut moves: Vec<CombinedMove> = Vec::new();
        for r in rows {
            let group = r.combine_move.clone().unwrap_or_default();
            let share = MoveShare {
                session_id: r.session_id.clone(),
                batch_number: batch_of(&r.session_id),
                delta_g: r.delta_g,
                move_id: r.id.clone(),
            };
            if let Some(m) = moves.iter_mut().find(|m| m.id == group) {
                m.delta_g = ((m.delta_g + r.delta_g) * 10.0).round() / 10.0;
                m.split.push(share);
                continue;
            }
            let best_until = (r.kind == "packed").then(|| {
                let form = Form::stored(r.form.as_deref(), r.grind.as_deref());
                w.best.bag_until(c.best_until, &form, zone.date(r.at))
            });
            moves.push(CombinedMove {
                id: group,
                at: r.at,
                kind: r.kind,
                delta_g: r.delta_g,
                recipient: r.recipient,
                note: r.note,
                bags: r.bags,
                bag_g: r.bag_g,
                form: r.form,
                grind: r.grind,
                label_run: r.label_run,
                best_until,
                split: vec![share],
            });
        }
        // Each split in the bag's order: its oldest roast first
        let rank = |sid: &str| members.iter().position(|l| l.session_id == sid);
        for m in &mut moves {
            m.split.sort_by_key(|x| rank(&x.session_id));
        }
        let first = &members[0];
        let sum = |f: fn(&RoastedLot) -> f32| {
            (members.iter().map(f).sum::<f32>() * 10.0).round() / 10.0 + 0.0
        };
        let (bean_id, bean_name) = if c.blend {
            (None, None)
        } else {
            (first.bean_id.clone(), first.bean_name.clone())
        };
        Ok(Some(CombinedLot {
            note,
            created_at,
            bean_id,
            bean_name,
            for_: first.for_.clone(),
            used_g: sum(|l| l.used_g),
            given_g: sum(|l| l.given_g),
            packed_g: sum(|l| l.packed_g),
            lot: c,
            members,
            moves,
        }))
    }

    /// Put roasts into one bag (`CombineRequest`): a new combined lot, or the others join the
    /// lot one of them is in. Each must be finished, weighed, have coffee left, have its coffee
    /// (bean) recorded and be for the same use; roasts of different coffees make a blend.
    pub async fn combine(
        &self,
        req: CombineRequest,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<CombinedLot> {
        let mut ids: Vec<String> = Vec::new();
        for r in req
            .roasts
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
        {
            if !ids.iter().any(|i| i == r) {
                ids.push(r.to_string());
            }
        }
        let mut lots: Vec<RoastedLot> = Vec::new();
        for id in &ids {
            let Some(l) = self.lot(id, zone, now).await? else {
                return Err(bad(format!("{id:?} is not a finished roast")));
            };
            lots.push(l);
        }
        let mark = |l: &RoastedLot| {
            l.batch_number
                .map_or_else(|| l.name.clone(), |b| format!("#{b}"))
        };
        let existing: std::collections::BTreeSet<String> = lots
            .iter()
            .filter_map(|l| l.combined.as_ref().map(|c| c.id.clone()))
            .collect();
        if existing.len() > 1 {
            return Err(bad(
                "these roasts are in different combined lots already: split one of them first",
            ));
        }
        let into = existing.into_iter().next();
        let mut all = lots.clone();
        if let Some(cid) = &into {
            let w = Windows::load(&self.db).await;
            if let Some((_, members)) = self.combined_parts(cid, &w, zone.date(now), zone).await? {
                for m in members {
                    if !all.iter().any(|l| l.session_id == m.session_id) {
                        all.push(m);
                    }
                }
            }
        }
        if all.len() < 2 {
            return Err(bad("a combined lot takes two roasts or more"));
        }
        for l in lots.iter().filter(|l| l.combined.is_none()) {
            let Some(left) = l.left_g else {
                return Err(bad(format!(
                    "{} has no roasted weight yet: record it first (roast weigh)",
                    mark(l)
                )));
            };
            if left <= EMPTY_G {
                return Err(bad(format!("{} has no coffee left", mark(l))));
            }
        }
        let first = &all[0];
        // A blend's label names every coffee in it: each roast needs its coffee
        if let Some(o) = all.iter().find(|l| l.bean_id.is_none()) {
            return Err(bad(format!(
                "{} has no coffee (bean) recorded: set it on the roast first (History → the \
                 roast), so the bag's label can name it",
                mark(o)
            )));
        }
        if let Some(o) = all.iter().find(|l| l.for_ != first.for_) {
            return Err(bad(format!(
                "{} is for {} and {} for {}: combine roasts for the same use",
                mark(o),
                o.for_,
                mark(first),
                first.for_
            )));
        }
        let name = clean(req.name);
        let note = clean(req.note);
        let mut tx = crate::db::begin_write(&self.db).await?;
        let cid = match into {
            Some(c) => {
                if name.is_some() || note.is_some() {
                    sqlx::query(
                        "UPDATE roasted_combines SET name = COALESCE(?, name), note = COALESCE(?, note) WHERE id = ?",
                    )
                    .bind(&name)
                    .bind(&note)
                    .bind(&c)
                    .execute(&mut *tx)
                    .await?;
                }
                c
            }
            None => {
                let c = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO roasted_combines (id, name, note, created_at) VALUES (?, ?, ?, ?)",
                )
                .bind(&c)
                .bind(&name)
                .bind(&note)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                c
            }
        };
        for l in lots.iter().filter(|l| l.combined.is_none()) {
            sqlx::query(
                "INSERT INTO roasted_combine_roasts (session_id, combine_id, added_at) VALUES (?, ?, ?)",
            )
            .bind(&l.session_id)
            .bind(&cid)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.combined(&cid, zone, now)
            .await?
            .context("the combined lot just made")
    }

    /// Split a combined lot apart (by its id or a roast's): its roasts are separate again,
    /// each with the moves it has. `None` when there is no such lot.
    pub async fn uncombine(&self, id: &str) -> Result<Option<Vec<String>>> {
        let cid = match self.combine_of(id).await? {
            Some(c) => c,
            None => id.to_string(),
        };
        let mut tx = crate::db::begin_write(&self.db).await?;
        let roasts: Vec<String> = sqlx::query_scalar(
            "SELECT session_id FROM roasted_combine_roasts WHERE combine_id = ?",
        )
        .bind(&cid)
        .fetch_all(&mut *tx)
        .await?;
        let gone = sqlx::query("DELETE FROM roasted_combines WHERE id = ?")
            .bind(&cid)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if gone == 0 {
            return Ok(None);
        }
        sqlx::query("DELETE FROM roasted_combine_roasts WHERE combine_id = ?")
            .bind(&cid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(roasts))
    }
}
