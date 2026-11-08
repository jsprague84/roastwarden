//! Roaster control beyond a single command: the roast sequence, fan assist, the gain schedule,
//! autotune runs and the busy check.

pub(crate) mod busy;
pub(crate) mod fan_assist;
pub(crate) mod schedule;
pub(crate) mod sequence;
pub(crate) mod tune;
pub(crate) mod tuning;
