//! Server-side profile replay (`docs/replay.md`).
//!
//! One task per replaying session, ticking at 1 Hz. Each tick reads the
//! session, its landmarks and the device's newest sample, advances the pure
//! [`engine::Engine`], sends the commands it returns through the transport
//! registry (unless the run is a ghost), persists the log entries and
//! broadcasts a [`ReplayState`] snapshot to dashboards. It runs without a
//! browser, survives dashboard reloads, and stops on session end, DROP or an
//! emergency stop. This replaces the old setpoint-only `FollowService`;
//! `mode: setpoint` with fan replay off behaves exactly like it.

pub mod auto_drop;
pub mod engine;
mod service;
mod types;

pub use service::ReplayService;
pub use types::{
    resolve_loop, EventOp, LoopPref, ReplayError, ReplayState, StartReplay, UpdateReplay,
};
