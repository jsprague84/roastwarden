//! The rest window: how long coffee keeps its best (light, darker, ground), the windows
//! read from the settings, and a purpose's name.

use chrono::{Duration, NaiveDate};
use sqlx::SqlitePool;

use crate::labels::RestDays;

use super::types::Form;
use super::{BEST_UNTIL_KEYS, GROUND_KEY, LOW_KEY};

/// How long coffee keeps its best, days: a light roast, a darker one, and once ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BestDays {
    pub light: i64,
    pub darker: i64,
    pub ground: i64,
}

impl Default for BestDays {
    fn default() -> Self {
        BestDays {
            light: BEST_UNTIL_KEYS[0].1,
            darker: BEST_UNTIL_KEYS[1].1,
            ground: GROUND_KEY.1,
        }
    }
}

impl BestDays {
    pub async fn load(db: &SqlitePool) -> Self {
        let days = |key: &'static str, default: i64| async move {
            crate::auth::settings::get(db, key)
                .await
                .and_then(|s| s.trim().parse().ok())
                .filter(|d| (1..=365).contains(d))
                .unwrap_or(default)
        };
        BestDays {
            light: days(BEST_UNTIL_KEYS[0].0, BEST_UNTIL_KEYS[0].1).await,
            darker: days(BEST_UNTIL_KEYS[1].0, BEST_UNTIL_KEYS[1].1).await,
            ground: days(GROUND_KEY.0, GROUND_KEY.1).await,
        }
    }

    /// A lot's best-until: the roast date plus its days by level, never before its rest ends
    pub fn until(&self, roasted_on: NaiveDate, level: Option<&str>, rest_days: i64) -> NaiveDate {
        let days = if is_light(level) {
            self.light
        } else {
            self.darker
        };
        roasted_on + Duration::days(days.max(rest_days))
    }

    /// A bag's best-until: the lot's, or for ground coffee the earlier of that and the day it
    /// was packed plus `ground`
    pub fn bag_until(&self, lot_until: NaiveDate, form: &Form, packed_on: NaiveDate) -> NaiveDate {
        if form.ground {
            lot_until.min(packed_on + Duration::days(self.ground))
        } else {
            lot_until
        }
    }
}

/// The settings the rest window reads.
#[derive(Debug, Clone, Copy)]
pub struct Windows {
    pub rest: RestDays,
    pub best: BestDays,
    pub low_g: f64,
}

impl Windows {
    pub async fn load(db: &SqlitePool) -> Self {
        let low_g = crate::auth::settings::get(db, LOW_KEY.0)
            .await
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|g| g.is_finite() && *g >= 0.0)
            .unwrap_or(LOW_KEY.1);
        Windows {
            rest: crate::labels::rest_days(db).await,
            best: BestDays::load(db).await,
            low_g,
        }
    }
}

/// Up to medium-light, or no level (as the labels read it).
pub(super) fn is_light(level: Option<&str>) -> bool {
    level.is_none_or(|l| {
        matches!(
            l,
            "extremely_light" | "very_light" | "light" | "medium_light"
        )
    })
}

/// What a roast is for: espresso, filter, or any.
pub fn for_purpose(purpose: Option<&str>) -> &'static str {
    match purpose {
        Some("espresso") => "espresso",
        Some("filter") => "filter",
        _ => "any",
    }
}
