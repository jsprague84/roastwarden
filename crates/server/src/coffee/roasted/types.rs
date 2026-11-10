//! Roasted coffee's records as the API gives and takes them: a lot and its state, a move,
//! a packed bag's form, a combined lot and its shares, the stock and a move request.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::labels::RestDays;

use super::GRIND_MAX;

/// Where a lot is in its life on the owner's today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LotState {
    /// Before its ready date
    Resting,
    /// From its ready date to its best-until date
    Ready,
    /// After its best-until date, some left
    PastBest,
    /// Nothing left (or untouched two months past its best)
    Finished,
    /// No roasted weight recorded yet
    Unweighed,
}

impl LotState {
    pub(super) fn rank(self) -> u8 {
        match self {
            LotState::Ready => 0,
            LotState::Resting => 1,
            LotState::PastBest => 2,
            LotState::Unweighed => 3,
            LotState::Finished => 4,
        }
    }
}

/// One change to a roast's roasted coffee.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct RoastedMove {
    pub id: String,
    pub session_id: String,
    pub at: DateTime<Utc>,
    /// Grams: below 0 takes coffee away
    pub delta_g: f32,
    /// used | given | packed | discarded | adjust
    pub kind: String,
    /// A gift: who it went to
    pub recipient: Option<String>,
    pub note: Option<String>,
    /// Packed: how many bags
    pub bags: Option<i64>,
    /// Packed: grams a bag
    pub bag_g: Option<f32>,
    /// Packed: whole | ground
    pub form: Option<String>,
    /// Packed ground: the grind ("espresso", "filter", "moka", "French press", or the owner's word)
    pub grind: Option<String>,
    /// Packed by a label print run: its id
    pub label_run: Option<String>,
    /// A move on a combined lot: the id its share on each roast carries
    pub combine_move: Option<String>,
    /// Packed: the bags' best-until (ground keeps its best for less than the lot)
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_until: Option<NaiveDate>,
}

/// Whole bean, or ground with a grind: a packed bag's form, and what its label says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub ground: bool,
    /// The grind, when ground and said
    pub grind: Option<String>,
}

impl Form {
    /// From `form` (whole | ground) and `grind`; a grind alone means ground.
    pub fn parse(form: Option<&str>, grind: Option<&str>) -> Result<Form, String> {
        let grind = grind.map(str::trim).filter(|g| !g.is_empty());
        if grind.is_some_and(|g| g.chars().count() > GRIND_MAX) {
            return Err(format!("a grind is at most {GRIND_MAX} characters"));
        }
        let ground = match form.map(|f| f.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") => grind.is_some(),
            Some("whole" | "whole bean" | "whole_bean" | "beans") => false,
            Some("ground") => true,
            Some(f) => return Err(format!("form is whole or ground, not {f:?}")),
        };
        if !ground && grind.is_some() {
            return Err("a grind goes with form ground".into());
        }
        Ok(Form {
            ground,
            grind: grind.map(str::to_string),
        })
    }

    /// From a stored move's `form` and `grind`
    pub fn stored(form: Option<&str>, grind: Option<&str>) -> Form {
        Form {
            ground: form == Some("ground"),
            grind: grind.map(str::to_string),
        }
    }

    /// whole | ground
    pub fn word(&self) -> &'static str {
        if self.ground { "ground" } else { "whole" }
    }

    /// What a label prints: "Whole bean", "Ground · espresso", "Ground"
    pub fn text(&self) -> String {
        match (self.ground, &self.grind) {
            (false, _) => "Whole bean".into(),
            (true, Some(g)) => format!("Ground · {g}"),
            (true, None) => "Ground".into(),
        }
    }
}

/// A roast's roasted coffee.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoastedLot {
    pub session_id: String,
    pub name: String,
    pub device_id: String,
    pub batch_number: Option<i32>,
    pub bean_id: Option<String>,
    pub bean_name: Option<String>,
    /// espresso | filter | omni | other
    pub purpose: Option<String>,
    pub level: Option<String>,
    /// espresso, filter, or any (omni, other or no purpose: for either)
    #[serde(rename = "for")]
    pub for_: String,
    /// The DROP (or the roast's end)
    pub roasted_at: DateTime<Utc>,
    /// The roast date in the owner's time zone, YYYY-MM-DD
    pub roasted_on: NaiveDate,
    pub age_days: i64,
    /// The roasted weight
    pub roasted_g: Option<f32>,
    /// What is left
    pub left_g: Option<f32>,
    pub used_g: f32,
    pub given_g: f32,
    /// Packed into bags
    pub packed_g: f32,
    pub state: LotState,
    /// The roast date plus the rest for its purpose and level: the label's "best from"
    pub ready_from: NaiveDate,
    /// Resting: days until ready
    pub ready_in_days: Option<i64>,
    /// At its best until
    pub best_until: NaiveDate,
    pub rest_days: i64,
    /// With one roast only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moves: Option<Vec<RoastedMove>>,
    /// In one bag with other roasts: the combined lot (its moves take coffee off each)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combined: Option<CombinedRef>,
}

/// The combined lot a roast is in: roasts in one bag or jar, of one coffee or a blend.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CombinedRef {
    pub id: String,
    /// Its name, or the coffee's with the batches ("Kenya Nyeri #16+17"); a blend's coffees
    /// ("Kenya Nyeri + Ethiopia Guji #16+18")
    pub name: String,
    /// Roasts of more than one coffee
    pub blend: bool,
    /// Its coffees, the largest share first (one for roasts of one coffee)
    pub coffees: Vec<LotCoffee>,
    /// The batch numbers, oldest roast first: "16+17"
    pub batches: String,
    /// Its roasts, oldest first
    pub roasts: Vec<String>,
    /// The roasts' roasted weight together
    pub roasted_g: f32,
    /// What is left of them together
    pub left_g: f32,
    pub state: LotState,
    /// Ready when its newest roast is
    pub ready_from: NaiveDate,
    pub ready_in_days: Option<i64>,
    /// At its best until its oldest roast's best-until
    pub best_until: NaiveDate,
    /// The oldest roast's date
    pub first_roasted_on: NaiveDate,
    /// The newest roast's date
    pub last_roasted_on: NaiveDate,
}

/// A coffee in a combined lot and its share of the bag.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LotCoffee {
    pub bean_id: String,
    pub name: String,
    /// Its share of what is left in the bag (of the roasted weight once nothing is), whole
    /// percent; the shares add up to 100
    pub share_pct: u32,
}

/// Whole percents of `weights` that add up to 100 (largest remainder); all 0 when they sum to
/// nothing.
pub fn percents(weights: &[f64]) -> Vec<u32> {
    let total: f64 = weights.iter().map(|w| w.max(0.0)).sum();
    if total <= 0.0 {
        return vec![0; weights.len()];
    }
    let exact: Vec<f64> = weights.iter().map(|w| w.max(0.0) * 100.0 / total).collect();
    let mut out: Vec<u32> = exact.iter().map(|e| e.floor() as u32).collect();
    let short = 100 - out.iter().sum::<u32>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&a, &b| {
        (exact[b] - exact[b].floor())
            .partial_cmp(&(exact[a] - exact[a].floor()))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    for &i in order.iter().take(short as usize) {
        out[i] += 1;
    }
    out
}

/// A combined lot read whole (`GET /api/roasted/combined/{id}`).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CombinedLot {
    #[serde(flatten)]
    pub lot: CombinedRef,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Its coffee (none for a blend: `coffees`)
    pub bean_id: Option<String>,
    pub bean_name: Option<String>,
    /// espresso, filter, or any
    #[serde(rename = "for")]
    pub for_: String,
    pub used_g: f32,
    pub given_g: f32,
    pub packed_g: f32,
    /// Its roasts, oldest first, each with what is left of it (no moves)
    pub members: Vec<RoastedLot>,
    /// The moves made on the lot, each split across its roasts
    pub moves: Vec<CombinedMove>,
}

/// One move on a combined lot, and how it was split.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CombinedMove {
    /// The `combine_move` id its rows share
    pub id: String,
    pub at: DateTime<Utc>,
    pub kind: String,
    /// Grams, the roasts together
    pub delta_g: f32,
    pub recipient: Option<String>,
    pub note: Option<String>,
    pub bags: Option<i64>,
    pub bag_g: Option<f32>,
    pub form: Option<String>,
    pub grind: Option<String>,
    pub label_run: Option<String>,
    pub best_until: Option<NaiveDate>,
    /// Each roast's share: its id, batch number and grams
    pub split: Vec<MoveShare>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MoveShare {
    pub session_id: String,
    pub batch_number: Option<i32>,
    pub delta_g: f32,
    /// The row's own id on that roast
    pub move_id: String,
}

/// Put roasts of one coffee into one bag: a new combined lot, or, when one of them is in a
/// lot already, the others join it.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CombineRequest {
    /// The roasts (ids), two or more, or one with a roast already in a lot
    pub roasts: Vec<String>,
    /// A name for the lot (default: the coffee's with the batches; a blend's, its coffees')
    pub name: Option<String>,
    pub note: Option<String>,
}

/// Roasted coffee for one purpose.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoastedGroup {
    /// espresso, filter, or any (omni, other or no purpose)
    #[serde(rename = "for")]
    pub for_: String,
    pub ready_g: f32,
    pub resting_g: f32,
    pub past_best_g: f32,
    /// Everything left (ready, resting and past its best)
    pub left_g: f32,
    /// Lots with coffee left
    pub lots: usize,
    pub unweighed: usize,
    /// When the next resting lot is ready, and how much it holds
    pub next_ready: Option<NaiveDate>,
    pub next_ready_g: Option<f32>,
    /// Grams used a day, from the last four weeks' "used" moves (none before any)
    pub per_day_g: Option<f32>,
    /// How long what is left lasts at that rate
    pub lasts_days: Option<f32>,
    /// Roast by this day so a new roast has rested when this runs out (today at the latest)
    pub roast_by: Option<NaiveDate>,
    /// Less left than `roasted_low_g`, or a roast is due
    pub low: bool,
}

/// What `GET /api/roasted` answers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoastedStock {
    /// The owner's today
    pub today: NaiveDate,
    pub lots: Vec<RoastedLot>,
    /// espresso, filter and any, those with anything in them
    pub groups: Vec<RoastedGroup>,
    pub rest_days: RestDays,
    pub best_until_days_light: i64,
    pub best_until_days: i64,
    /// Ground coffee, days from the day it was packed
    pub best_until_days_ground: i64,
    pub low_g: f64,
}

/// Used, given or thrown away (`grams`, `doses`, `bags` or `all`), packed into bags (`bags`
/// of `bag_g`, whole or ground), or a count of what is left (`adjust` with `set_g`).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RoastedMoveRequest {
    /// used | given | packed | discarded | adjust
    pub kind: String,
    /// Grams taken
    pub grams: Option<f32>,
    /// Doses taken: the espresso or filter dose by the roast's purpose (Settings → Roasting)
    pub doses: Option<f32>,
    /// Bags taken, of the bag size (Settings → Roasting); `packed`: how many bags, a whole number
    pub bags: Option<f32>,
    /// `packed`: grams a bag (default the bag size, Settings → Roasting)
    pub bag_g: Option<f32>,
    /// `packed`: whole (default) or ground
    pub form: Option<String>,
    /// `packed` ground: the grind, e.g. espresso, filter, moka, French press (implies ground)
    pub grind: Option<String>,
    /// Everything that is left
    #[serde(default)]
    pub all: bool,
    /// `adjust`: what is left now, grams
    pub set_g: Option<f32>,
    /// `given`: who it went to
    pub recipient: Option<String>,
    pub note: Option<String>,
    /// When, YYYY-MM-DD or RFC 3339 (default now)
    pub at: Option<String>,
}
