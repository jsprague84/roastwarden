//! Roasted coffee: resting and ready (`docs/roasted.md`).
//!
//! Every finished roast with a roasted weight is a lot of roasted coffee. What is left is that
//! weight plus its moves (`roasted_moves`: used, given, packed into bags, discarded, or a count
//! of what is left). A packed bag is whole bean or ground with a grind; ground coffee keeps its
//! best for less (`best_until_days_ground`).
//! Its rest window comes from the roast date, its purpose and its level: ready from the rest
//! the bag labels print ("best from", `labels::RestDays`), at its best until `best_until_days`.
//! Nothing derived is stored: the state is worked out on every read for the owner's today.
//!
//! Roasts can go into one bag or jar: a **combined lot** (`roasted_combines`, migration 061),
//! of one coffee or a blend of several. Each roast keeps its own moves, so costs, use rates and the trace stay per
//! roast; a move on any of its roasts is split across them by what each has left, the rows
//! sharing a `combine_move` id so they are taken back together. The lot is ready when its
//! newest roast is and at its best until its oldest roast's best-until.

use sqlx::SqlitePool;

use crate::coffee::inventory::InventoryInputError;

mod combined;
mod lots;
mod moves;
#[cfg(test)]
mod tests;
mod types;
mod windows;

pub use moves::{MOVE_COLUMNS, label_lot, packed_by_run};
pub use types::{
    CombineRequest, CombinedLot, CombinedRef, Form, LotState, RoastedLot, RoastedMove,
    RoastedMoveRequest, RoastedStock,
};
pub use windows::BestDays;

fn bad(msg: impl Into<String>) -> anyhow::Error {
    InventoryInputError(msg.into()).into()
}

/// How long a roast stays at its best, days from the roast date (light, darker): Simon &
/// Bearns, Square Mile and Wendelboe end at "a month to five weeks".
pub const BEST_UNTIL_KEYS: [(&str, i64); 2] =
    [("best_until_days_light", 35), ("best_until_days", 28)];
/// How long ground coffee keeps its best, days from the day it was packed: Workshop Coffee,
/// who sell pre-ground bags, say "within two weeks of the 'Ground On' date"
/// (workshopcoffee.com/pages/more-on-our-pre-ground-coffee; Stumptown say a couple of days
/// for coffee ground at home and kept unsealed).
pub const GROUND_KEY: (&str, i64) = ("best_until_days_ground", 14);
/// The longest a grind's name may be
const GRIND_MAX: usize = 40;
/// The most bags one packed move takes
const BAGS_MAX: f32 = 1000.0;
/// Below this much roasted coffee for a purpose, it is running low.
pub const LOW_KEY: (&str, f64) = ("roasted_low_g", 200.0);
/// The days of use a rate is read over.
const RATE_WINDOW_DAYS: i64 = 28;
/// A rate is read over at least this many days, so one morning's use is not a day's.
const RATE_MIN_DAYS: i64 = 7;
/// An untouched roast this long past its best counts as gone: an old import, a forgotten jar.
const FORGOTTEN_DAYS: i64 = 60;
/// Unweighed roasts are listed this long, so the owner is asked to weigh them.
const UNWEIGHED_DAYS: i64 = 14;
/// Grams that count as nothing left.
const EMPTY_G: f32 = 0.5;

#[derive(Clone)]
pub struct RoastedService {
    db: SqlitePool,
}

impl RoastedService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
}
