//! A move on a lot: used, given, packed into bags, discarded, or a count of what is left;
//! planned against what is left, stored, taken back; the lot a label prints.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::ops::zone::Zone;

use super::types::{Form, RoastedLot, RoastedMoveRequest, percents};
use super::{BAGS_MAX, EMPTY_G, RoastedService, bad};

/// The columns of a [`RoastedMove`]
pub const MOVE_COLUMNS: &str = "id, session_id, at, delta_g, kind, recipient, note, bags, bag_g, form, grind, label_run, \
     combine_move";

/// The roasts a label run has taken bags off already.
pub async fn packed_by_run(db: &SqlitePool, run: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT DISTINCT session_id FROM roasted_moves WHERE label_run = ?")
        .bind(run)
        .fetch_all(db)
        .await
}

/// The combined lot a roast is in, as its label prints it: the batches, the oldest and newest
/// roasts' times, their roasted weight together, the bag's own name and its coffees with their
/// shares (more than one for a blend). `None` when it is in none.
pub async fn label_lot(
    db: &SqlitePool,
    session_id: &str,
) -> Result<Option<crate::labels::LotFacts>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Option<i32>, DateTime<Utc>, Option<f32>, Option<String>, f32, Option<String>)> =
        sqlx::query_as(
            "SELECT s.batch_number, COALESCE(s.end_time, s.start_time, s.created_at) AS at, \
             s.roasted_weight, s.bean_id, \
             CAST(COALESCE(s.roasted_weight, 0) + COALESCE((SELECT SUM(delta_g) FROM roasted_moves m \
             WHERE m.session_id = s.id), 0.0) AS REAL) AS left_g, c.name \
             FROM roast_sessions s JOIN roasted_combine_roasts r ON r.session_id = s.id \
             JOIN roasted_combines c ON c.id = r.combine_id \
             WHERE r.combine_id = (SELECT combine_id FROM roasted_combine_roasts WHERE session_id = ?) \
             ORDER BY at",
        )
        .bind(session_id)
        .fetch_all(db)
        .await?;
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return Ok(None);
    };
    // Its coffees by share of what is left (of the roasted weight once nothing is), as the
    // lot's list has them
    let by_left = rows.iter().map(|r| r.4.max(0.0)).sum::<f32>() > EMPTY_G;
    let mut coffees: Vec<(String, f64)> = Vec::new();
    for r in &rows {
        let Some(bean) = &r.3 else { continue };
        let g = f64::from(if by_left {
            r.4.max(0.0)
        } else {
            r.2.unwrap_or(0.0)
        });
        match coffees.iter_mut().find(|c| &c.0 == bean) {
            Some(c) => c.1 += g,
            None => coffees.push((bean.clone(), g)),
        }
    }
    let pcts = percents(&coffees.iter().map(|c| c.1).collect::<Vec<_>>());
    let mut coffees: Vec<(String, u32)> = coffees
        .into_iter()
        .zip(pcts)
        .map(|((b, _), p)| (b, p))
        .collect();
    coffees.sort_by_key(|c| std::cmp::Reverse(c.1));
    Ok(Some(crate::labels::LotFacts {
        batches: rows
            .iter()
            .filter_map(|r| r.0)
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join("+"),
        first_at: first.1,
        last_at: last.1,
        roasted_g: rows.iter().filter_map(|r| r.2).map(f64::from).sum(),
        name: first.5.clone().filter(|n| !n.trim().is_empty()),
        coffees,
        parts: Vec::new(),
    }))
}

/// A move checked and worked out against what is left, ready to record.
pub(super) struct Planned {
    pub(super) kind: String,
    /// Grams, below 0 takes coffee away
    pub(super) delta: f32,
    pub(super) recipient: Option<String>,
    pub(super) note: Option<String>,
    /// Packed: bags and grams a bag
    pub(super) bag: Option<(i64, f32)>,
    pub(super) form: Form,
    pub(super) at: DateTime<Utc>,
}

/// Check a move against `left` grams (of `what`: "this roast", "#16+17 together") and work out
/// its grams: used, given or thrown away (grams, doses, bags or all), packed bags, or a count.
pub(super) fn plan_move(
    req: RoastedMoveRequest,
    left: f32,
    for_: &str,
    sizes: &crate::coffee::costs::Sizes,
    what: &str,
    now: DateTime<Utc>,
) -> Result<Planned> {
    let kind = req.kind.trim().to_ascii_lowercase();
    let recipient = clean(req.recipient);
    if recipient.is_some() && kind != "given" {
        return Err(bad("a recipient goes with kind given"));
    }
    let packed = kind == "packed";
    if !packed && (req.bag_g.is_some() || req.form.is_some() || req.grind.is_some()) {
        return Err(bad("bag_g, form and grind go with kind packed"));
    }
    let form = Form::parse(req.form.as_deref(), req.grind.as_deref()).map_err(bad)?;
    let mut bag: Option<(i64, f32)> = None;
    let delta = if packed {
        if req.grams.is_some() || req.doses.is_some() || req.all || req.set_g.is_some() {
            return Err(bad(
                "packed takes bags (how many) and bag_g (grams a bag), not grams, doses or all",
            ));
        }
        let n = match req.bags {
            Some(n) if n.is_finite() && (1.0..=BAGS_MAX).contains(&n) && n.fract() == 0.0 => n,
            _ => return Err(bad("packed needs bags, a whole number of bags from 1")),
        };
        let size = match req.bag_g {
            Some(g) if g.is_finite() && g > 0.0 && g <= 100_000.0 => g,
            Some(_) => return Err(bad("bag_g must be more than 0")),
            None => sizes.bag_g as f32,
        };
        let grams = n * size;
        if grams > left + EMPTY_G {
            return Err(bad(format!(
                "only {left:.1} g of {what} is left: {n} × {size} g is {grams:.1} g"
            )));
        }
        bag = Some((n as i64, size));
        -grams.min(left)
    } else if kind == "adjust" {
        if req.grams.is_some() || req.doses.is_some() || req.bags.is_some() || req.all {
            return Err(bad("adjust takes set_g (what is left now) only"));
        }
        match req.set_g {
            Some(g) if g.is_finite() && g >= 0.0 => g - left,
            _ => return Err(bad("adjust needs set_g, the grams left now (0 or more)")),
        }
    } else if matches!(kind.as_str(), "used" | "given" | "discarded") {
        if req.set_g.is_some() {
            return Err(bad("set_g goes with kind adjust"));
        }
        let given = [
            req.grams.is_some(),
            req.doses.is_some(),
            req.bags.is_some(),
            req.all,
        ];
        if given.iter().filter(|g| **g).count() != 1 {
            return Err(bad("say how much: one of grams, doses, bags or all"));
        }
        let positive = |v: f32, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(v)
            } else {
                Err(bad(format!("{what} must be more than 0")))
            }
        };
        let grams = if req.all {
            left
        } else if let Some(g) = req.grams {
            positive(g, "grams")?
        } else if let Some(d) = req.doses {
            let dose = match for_ {
                "espresso" => sizes.espresso_g,
                "filter" => sizes.filter_g,
                _ => {
                    return Err(bad(
                        "this roast is for espresso or filter: say the grams instead of doses",
                    ));
                }
            };
            positive(d, "doses")? * dose as f32
        } else {
            positive(req.bags.unwrap_or(0.0), "bags")? * sizes.bag_g as f32
        };
        if grams > left + EMPTY_G {
            return Err(bad(format!(
                "only {left:.1} g of {what} is left: take at most that (all), or count \
                 what is left first (adjust)"
            )));
        }
        -grams.min(left)
    } else {
        return Err(bad("kind is used, given, packed, discarded or adjust"));
    };
    Ok(Planned {
        at: move_time(req.at.as_deref(), now)?,
        kind,
        delta,
        recipient,
        note: clean(req.note),
        bag,
        form,
    })
}

/// Record a planned move's `delta` grams on one roast.
pub(super) async fn insert_move(
    tx: &mut sqlx::Transaction<'static, sqlx::Sqlite>,
    session_id: &str,
    m: &Planned,
    delta: f32,
    label_run: Option<&str>,
    combine_move: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let packed = m.kind == "packed";
    sqlx::query(
        "INSERT INTO roasted_moves (id, session_id, at, delta_g, kind, recipient, note, created_at, \
         bags, bag_g, form, grind, label_run, combine_move) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(session_id)
    .bind(m.at)
    .bind((delta * 10.0).round() / 10.0)
    .bind(&m.kind)
    .bind(&m.recipient)
    .bind(&m.note)
    .bind(now)
    .bind(m.bag.map(|b| b.0))
    .bind(m.bag.map(|b| b.1))
    .bind(packed.then(|| m.form.word()))
    .bind(m.form.grind.as_deref().filter(|_| packed))
    .bind(label_run.filter(|_| packed))
    .bind(combine_move)
    .execute(&mut **tx)
    .await
    .context("insert roasted move")?;
    Ok(())
}

pub(super) fn clean(v: Option<String>) -> Option<String> {
    v.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

pub(super) fn move_time(v: Option<&str>, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let Some(t) = v.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(now);
    };
    let at = if let Ok(d) = DateTime::parse_from_rfc3339(t) {
        d.with_timezone(&Utc)
    } else if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        d.and_hms_opt(12, 0, 0).expect("noon").and_utc()
    } else {
        return Err(bad(format!("a date is YYYY-MM-DD or RFC 3339, not {t:?}")));
    };
    if at > now + Duration::days(1) {
        return Err(bad("a move cannot be in the future"));
    }
    Ok(at)
}

impl RoastedService {
    /// Record coffee used, given, packed or thrown away, or a count of what is left.
    pub async fn add_move(
        &self,
        session_id: &str,
        req: RoastedMoveRequest,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<Option<RoastedLot>> {
        self.add_move_for(session_id, req, None, zone, now).await
    }

    /// As [`add_move`](Self::add_move); a packed move made by a label print run names it. A
    /// roast in a combined lot passes the move to the lot: it is split across its roasts.
    pub async fn add_move_for(
        &self,
        session_id: &str,
        req: RoastedMoveRequest,
        label_run: Option<&str>,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<Option<RoastedLot>> {
        let Some(lot) = self.lot(session_id, zone, now).await? else {
            return Ok(None);
        };
        if let Some(c) = &lot.combined {
            self.add_combined_move(&c.id, req, label_run, zone, now)
                .await?;
            return self.lot(session_id, zone, now).await;
        }
        let Some(left) = lot.left_g else {
            return Err(bad(
                "this roast has no roasted weight yet: record it first (roast weigh)",
            ));
        };
        let (_, sizes) = crate::coffee::inventory::cost_settings(&self.db).await;
        let m = plan_move(req, left, &lot.for_, &sizes, "this roast", now)?;
        if m.delta.abs() < 0.05 {
            return Ok(Some(lot));
        }
        let mut tx = crate::db::begin_write(&self.db).await?;
        insert_move(&mut tx, session_id, &m, m.delta, label_run, None, now).await?;
        tx.commit().await?;
        self.lot(session_id, zone, now).await
    }

    /// Take a move back. `None` when there is no such move on the roast.
    pub async fn delete_move(
        &self,
        session_id: &str,
        move_id: &str,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<Option<RoastedLot>> {
        // A share of a move on a combined lot takes the whole move back, on every roast
        let group: Option<Option<String>> = sqlx::query_scalar(
            "SELECT combine_move FROM roasted_moves WHERE id = ? AND session_id = ?",
        )
        .bind(move_id)
        .bind(session_id)
        .fetch_optional(&self.db)
        .await?;
        let gone = match group {
            None => 0,
            Some(Some(g)) => sqlx::query("DELETE FROM roasted_moves WHERE combine_move = ?")
                .bind(g)
                .execute(&self.db)
                .await?
                .rows_affected(),
            Some(None) => sqlx::query("DELETE FROM roasted_moves WHERE id = ? AND session_id = ?")
                .bind(move_id)
                .bind(session_id)
                .execute(&self.db)
                .await?
                .rows_affected(),
        };
        if gone == 0 {
            return Ok(None);
        }
        self.lot(session_id, zone, now).await
    }
}
