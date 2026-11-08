//! Each roast's lot on the owner's today: its row read, its state and dates worked out,
//! the lots grouped by purpose with their use rate, and the stock.

use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::ops::zone::Zone;

use super::moves::MOVE_COLUMNS;
use super::types::{
    percents, CombinedRef, Form, LotCoffee, LotState, RoastedGroup, RoastedLot, RoastedMove,
    RoastedStock,
};
use super::windows::{for_purpose, Windows};
use super::{
    RoastedService, EMPTY_G, FORGOTTEN_DAYS, RATE_MIN_DAYS, RATE_WINDOW_DAYS, UNWEIGHED_DAYS,
};

/// One roast as read from the database.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct LotRow {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) device_id: String,
    pub(super) batch_number: Option<i32>,
    pub(super) bean_id: Option<String>,
    pub(super) bean_name: Option<String>,
    pub(super) roast_purpose: Option<String>,
    pub(super) target_roast_level: Option<String>,
    pub(super) roasted_at: DateTime<Utc>,
    pub(super) roasted_weight: Option<f32>,
    pub(super) moved_g: f32,
    pub(super) used_g: f32,
    pub(super) given_g: f32,
    pub(super) packed_g: f32,
    pub(super) move_count: i64,
}

pub(super) const LOT_SELECT: &str = "SELECT s.id, s.name, s.device_id, s.batch_number, s.bean_id, \
     b.name AS bean_name, s.roast_purpose, s.target_roast_level, \
     COALESCE(s.end_time, s.start_time, s.created_at) AS roasted_at, s.roasted_weight, \
     CAST(COALESCE((SELECT SUM(delta_g) FROM roasted_moves m WHERE m.session_id = s.id), 0.0) AS REAL) AS moved_g, \
     CAST(COALESCE((SELECT -SUM(delta_g) FROM roasted_moves m WHERE m.session_id = s.id AND m.kind = 'used'), 0.0) AS REAL) AS used_g, \
     CAST(COALESCE((SELECT -SUM(delta_g) FROM roasted_moves m WHERE m.session_id = s.id AND m.kind = 'given'), 0.0) AS REAL) AS given_g, \
     CAST(COALESCE((SELECT -SUM(delta_g) FROM roasted_moves m WHERE m.session_id = s.id AND m.kind = 'packed'), 0.0) AS REAL) AS packed_g, \
     (SELECT COUNT(*) FROM roasted_moves m WHERE m.session_id = s.id) AS move_count \
     FROM roast_sessions s LEFT JOIN beans b ON b.id = s.bean_id \
     WHERE s.status = 'completed'";

/// A lot from its row on `today`.
pub(super) fn lot(r: LotRow, w: &Windows, today: NaiveDate, zone: Zone) -> RoastedLot {
    let roasted_on = zone.date(r.roasted_at);
    let purpose = r.roast_purpose.as_deref();
    let level = r.target_roast_level.as_deref();
    let rest_days = w.rest.for_roast(purpose, level);
    let ready_from = roasted_on + Duration::days(rest_days);
    let best_until = w.best.until(roasted_on, level, rest_days);
    let left = r
        .roasted_weight
        .filter(|g| *g > 0.0)
        .map(|g| ((g + r.moved_g) * 10.0).round() / 10.0);
    let forgotten = r.move_count == 0 && today > best_until + Duration::days(FORGOTTEN_DAYS);
    let state = match left {
        None => LotState::Unweighed,
        Some(l) if l <= EMPTY_G || forgotten => LotState::Finished,
        Some(_) if today < ready_from => LotState::Resting,
        Some(_) if today <= best_until => LotState::Ready,
        Some(_) => LotState::PastBest,
    };
    RoastedLot {
        session_id: r.id,
        name: r.name,
        device_id: r.device_id,
        batch_number: r.batch_number,
        bean_id: r.bean_id,
        bean_name: r.bean_name,
        for_: for_purpose(purpose).into(),
        purpose: r.roast_purpose,
        level: r.target_roast_level,
        roasted_at: r.roasted_at,
        roasted_on,
        age_days: (today - roasted_on).num_days(),
        roasted_g: r.roasted_weight,
        left_g: left.map(|l| l.max(0.0)),
        used_g: r.used_g,
        given_g: r.given_g,
        packed_g: r.packed_g,
        state,
        ready_from,
        ready_in_days: (state == LotState::Resting).then(|| (ready_from - today).num_days()),
        best_until,
        rest_days,
        moves: None,
        combined: None,
    }
}

/// A lot's state, or its combined lot's when it is in one: what counts for the lists.
pub(super) fn eff_state(l: &RoastedLot) -> LotState {
    l.combined.as_ref().map_or(l.state, |c| c.state)
}

/// A lot's ready date, or its combined lot's.
pub(super) fn eff_ready(l: &RoastedLot) -> NaiveDate {
    l.combined.as_ref().map_or(l.ready_from, |c| c.ready_from)
}

/// The batch numbers of roasts, oldest first: "16+17"
pub(super) fn batches_text(lots: &[&RoastedLot]) -> String {
    lots.iter()
        .filter_map(|l| l.batch_number)
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join("+")
}

/// A combined lot from its roasts (oldest first) on `today`.
pub(super) fn combined_ref(
    id: &str,
    name: Option<&str>,
    members: &[&RoastedLot],
    today: NaiveDate,
) -> CombinedRef {
    let batches = batches_text(members);
    let first = members.first().expect("a combined lot has roasts");
    let ready_from = members
        .iter()
        .map(|l| l.ready_from)
        .max()
        .unwrap_or(first.ready_from);
    let best_until = members
        .iter()
        .map(|l| l.best_until)
        .min()
        .unwrap_or(first.best_until);
    let left: f32 = members.iter().filter_map(|l| l.left_g).sum();
    let left = (left * 10.0).round() / 10.0 + 0.0;
    let state = if left <= EMPTY_G {
        LotState::Finished
    } else if today < ready_from {
        LotState::Resting
    } else if today <= best_until {
        LotState::Ready
    } else {
        LotState::PastBest
    };
    // Its coffees by share of what is left (of the roasted weight once nothing is)
    let by_left = left > EMPTY_G;
    let mut coffees: Vec<(String, String, f64)> = Vec::new();
    for l in members {
        let g = f64::from(if by_left {
            l.left_g.unwrap_or(0.0)
        } else {
            l.roasted_g.unwrap_or(0.0)
        });
        let id = l.bean_id.clone().unwrap_or_default();
        match coffees.iter_mut().find(|c| c.0 == id) {
            Some(c) => c.2 += g,
            None => coffees.push((id, l.bean_name.clone().unwrap_or_else(|| l.name.clone()), g)),
        }
    }
    let pcts = percents(&coffees.iter().map(|c| c.2).collect::<Vec<_>>());
    let mut coffees: Vec<LotCoffee> = coffees
        .into_iter()
        .zip(pcts)
        .map(|((bean_id, name, _), share_pct)| LotCoffee {
            bean_id,
            name,
            share_pct,
        })
        .collect();
    coffees.sort_by_key(|c| std::cmp::Reverse(c.share_pct));
    let blend = coffees.len() > 1;
    let coffee = coffees
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(" + ");
    CombinedRef {
        id: id.into(),
        name: name.map(str::trim).filter(|n| !n.is_empty()).map_or_else(
            || {
                if batches.is_empty() {
                    coffee
                } else {
                    format!("{coffee} #{batches}")
                }
            },
            str::to_string,
        ),
        blend,
        coffees,
        batches,
        roasts: members.iter().map(|l| l.session_id.clone()).collect(),
        roasted_g: members.iter().filter_map(|l| l.roasted_g).sum(),
        left_g: left,
        state,
        ready_from,
        ready_in_days: (state == LotState::Resting).then(|| (ready_from - today).num_days()),
        best_until,
        first_roasted_on: members
            .iter()
            .map(|l| l.roasted_on)
            .min()
            .unwrap_or(first.roasted_on),
        last_roasted_on: members
            .iter()
            .map(|l| l.roasted_on)
            .max()
            .unwrap_or(first.roasted_on),
    }
}

/// A move of `delta` grams split across roasts with `lefts` grams left, by their share of
/// what is left (by `weights`, their roasted weights, when nothing is): to a tenth of a gram,
/// the last taking the rounding, none taking more than it has.
pub(super) fn split(delta: f32, lefts: &[f32], weights: &[f32]) -> Vec<f32> {
    let total: f32 = lefts.iter().sum();
    let (by, sum) = if total > EMPTY_G {
        (lefts, total)
    } else {
        (weights, weights.iter().sum::<f32>().max(f32::MIN_POSITIVE))
    };
    let round = |g: f32| (g * 10.0).round() / 10.0;
    let mut out = Vec::with_capacity(lefts.len());
    let mut acc = 0.0;
    for (i, left) in lefts.iter().enumerate() {
        let d = if i + 1 == lefts.len() {
            round(delta - acc)
        } else {
            round(delta * by[i] / sum)
        };
        let d = d.max(-left);
        acc += d;
        out.push(d);
    }
    out
}

/// The "used" moves of a purpose, (owner's date, grams), for its rate.
pub(super) type Use = (NaiveDate, f32);

/// Roasted coffee for one purpose on `today`: what is ready and resting, how fast it goes
/// and when to roast again.
pub(super) fn group(
    for_: &str,
    lots: &[RoastedLot],
    uses: &[Use],
    w: &Windows,
    today: NaiveDate,
) -> RoastedGroup {
    let mine: Vec<&RoastedLot> = lots.iter().filter(|l| l.for_ == for_).collect();
    let sum = |s: LotState| -> f32 {
        mine.iter()
            .filter(|l| eff_state(l) == s)
            .filter_map(|l| l.left_g)
            .sum()
    };
    let (ready, resting, past) = (
        sum(LotState::Ready),
        sum(LotState::Resting),
        sum(LotState::PastBest),
    );
    let left = ready + resting + past;
    let next = mine
        .iter()
        .filter(|l| eff_state(l) == LotState::Resting)
        .min_by_key(|l| eff_ready(l));
    let next_ready = next.map(|l| eff_ready(l));
    let next_ready_g = next_ready.map(|d| {
        mine.iter()
            .filter(|l| eff_state(l) == LotState::Resting && eff_ready(l) == d)
            .filter_map(|l| l.left_g)
            .sum()
    });
    // The rate: what was used in the last four weeks, over the days since the first use in
    // them (a week at least).
    let from = today - Duration::days(RATE_WINDOW_DAYS);
    let recent: Vec<&Use> = uses
        .iter()
        .filter(|(d, _)| *d > from && *d <= today)
        .collect();
    let per_day = recent.iter().map(|(d, _)| *d).min().and_then(|first| {
        let days = ((today - first).num_days() + 1).clamp(RATE_MIN_DAYS, RATE_WINDOW_DAYS);
        let used: f32 = recent.iter().map(|(_, g)| *g).sum();
        (used > 0.0).then(|| used / days as f32)
    });
    let lasts = per_day.map(|r| left / r);
    // A roast made today is ready after the rest of the newest lot's kind (a light one when
    // there is none): roast that many days before this runs out.
    let rest = mine.iter().max_by_key(|l| l.roasted_at).map_or_else(
        || {
            w.rest.for_roast(
                match for_ {
                    "espresso" => Some("espresso"),
                    _ => Some("filter"),
                },
                None,
            )
        },
        |l| l.rest_days,
    );
    let roast_by = lasts.map(|d| (today + Duration::days(d.floor() as i64 - rest)).max(today));
    let low = (left as f64) < w.low_g || roast_by.is_some_and(|d| d <= today);
    // + 0.0: an empty sum is -0.0, which agents would read as "-0.0 g"
    let round = |g: f32| (g * 10.0).round() / 10.0 + 0.0;
    RoastedGroup {
        for_: for_.into(),
        ready_g: round(ready),
        resting_g: round(resting),
        past_best_g: round(past),
        left_g: round(left),
        // A combined lot counts once
        lots: mine
            .iter()
            .filter(|l| {
                l.combined
                    .as_ref()
                    .is_none_or(|c| c.roasts.first() == Some(&l.session_id))
            })
            .filter(|l| {
                matches!(
                    eff_state(l),
                    LotState::Ready | LotState::Resting | LotState::PastBest
                )
            })
            .count(),
        unweighed: mine
            .iter()
            .filter(|l| l.state == LotState::Unweighed)
            .count(),
        next_ready,
        next_ready_g: next_ready_g.map(round),
        per_day_g: per_day.map(round),
        lasts_days: lasts.map(|d| (d * 10.0).round() / 10.0),
        roast_by,
        low,
    }
}

/// Ready first (oldest first: drink those first), then resting by when they are ready, then
/// past their best, unweighed and finished.
pub(super) fn sort(lots: &mut [RoastedLot]) {
    // A combined lot's roasts sort as one: by its state and ready date, then its first roast
    lots.sort_by(|a, b| {
        (eff_state(a).rank(), eff_ready(a), a.roasted_at).cmp(&(
            eff_state(b).rank(),
            eff_ready(b),
            b.roasted_at,
        ))
    });
    // Keep a combined lot's roasts together, oldest first, where its first one sorted
    let mut out: Vec<RoastedLot> = Vec::with_capacity(lots.len());
    let mut taken = vec![false; lots.len()];
    for i in 0..lots.len() {
        if taken[i] {
            continue;
        }
        taken[i] = true;
        out.push(lots[i].clone());
        if let Some(c) = lots[i].combined.clone() {
            for j in i + 1..lots.len() {
                if !taken[j] && lots[j].combined.as_ref().is_some_and(|o| o.id == c.id) {
                    taken[j] = true;
                    out.push(lots[j].clone());
                }
            }
        }
    }
    lots.clone_from_slice(&out);
}

impl RoastedService {
    /// Every roast's roasted coffee on the owner's today: those with coffee left (and
    /// unweighed ones of the last two weeks), or every one with `all`.
    pub async fn stock(&self, zone: Zone, all: bool, now: DateTime<Utc>) -> Result<RoastedStock> {
        let w = Windows::load(&self.db).await;
        let today = zone.date(now);
        let rows: Vec<LotRow> = sqlx::query_as(LOT_SELECT).fetch_all(&self.db).await?;
        let mut lots: Vec<RoastedLot> = rows
            .into_iter()
            .filter(|r| {
                r.roasted_weight.is_some_and(|g| g > 0.0)
                    || (!crate::sim::virtual_roaster::is_simulated(&r.device_id)
                        && r.roasted_at > now - Duration::days(UNWEIGHED_DAYS))
            })
            .map(|r| lot(r, &w, today, zone))
            .collect();
        self.attach_combined(&mut lots, &w, today, zone).await?;
        let uses: Vec<(String, DateTime<Utc>, f32)> = sqlx::query_as(
            "SELECT session_id, at, CAST(-delta_g AS REAL) FROM roasted_moves WHERE kind = 'used' AND at > ?",
        )
        .bind(now - Duration::days(RATE_WINDOW_DAYS + 2))
        .fetch_all(&self.db)
        .await?;
        let for_of: std::collections::HashMap<&str, &str> = lots
            .iter()
            .map(|l| (l.session_id.as_str(), l.for_.as_str()))
            .collect();
        let groups = ["espresso", "filter", "any"]
            .into_iter()
            .map(|f| {
                let mine: Vec<Use> = uses
                    .iter()
                    .filter(|(s, _, _)| for_of.get(s.as_str()) == Some(&f))
                    .map(|(_, at, g)| (zone.date(*at), *g))
                    .collect();
                group(f, &lots, &mine, &w, today)
            })
            .filter(|g| g.lots > 0 || g.unweighed > 0 || g.per_day_g.is_some())
            .collect();
        if !all {
            lots.retain(|l| eff_state(l) != LotState::Finished);
        }
        sort(&mut lots);
        Ok(RoastedStock {
            today,
            lots,
            groups,
            rest_days: w.rest,
            best_until_days_light: w.best.light,
            best_until_days: w.best.darker,
            best_until_days_ground: w.best.ground,
            low_g: w.low_g,
        })
    }

    /// One roast's roasted coffee with its moves; none when there is no finished roast.
    pub async fn lot(
        &self,
        session_id: &str,
        zone: Zone,
        now: DateTime<Utc>,
    ) -> Result<Option<RoastedLot>> {
        let w = Windows::load(&self.db).await;
        let row: Option<LotRow> = sqlx::query_as(&format!("{LOT_SELECT} AND s.id = ?"))
            .bind(session_id)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let today = zone.date(now);
        let mut l = lot(row, &w, today, zone);
        if let Some(cid) = self.combine_of(session_id).await? {
            if let Some((c, _)) = self.combined_parts(&cid, &w, today, zone).await? {
                l.combined = Some(c);
            }
        }
        let mut moves: Vec<RoastedMove> = sqlx::query_as(&format!(
            "SELECT {MOVE_COLUMNS} FROM roasted_moves WHERE session_id = ? ORDER BY at, created_at"
        ))
        .bind(session_id)
        .fetch_all(&self.db)
        .await?;
        let lot_until = l.combined.as_ref().map_or(l.best_until, |c| c.best_until);
        for m in moves.iter_mut().filter(|m| m.kind == "packed") {
            let form = Form::stored(m.form.as_deref(), m.grind.as_deref());
            m.best_until = Some(w.best.bag_until(lot_until, &form, zone.date(m.at)));
        }
        l.moves = Some(moves);
        Ok(Some(l))
    }
}
