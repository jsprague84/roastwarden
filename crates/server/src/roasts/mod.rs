//! Roast sessions: their records and statistics, landmarks, stored telemetry, cupping, and the
//! files a roast goes out as or comes in from.

pub(crate) mod cupping;
pub(crate) mod events;
pub(crate) mod export;
pub(crate) mod import;
pub(crate) mod sessions;
pub(crate) mod stats;
pub(crate) mod telemetry_store;
#[cfg(test)]
pub(crate) mod testing;
pub(crate) mod types;
