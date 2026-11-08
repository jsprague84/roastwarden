//! The `roast` binary against the whole server (`test_support::Harness`, served on a local port):
//! JSON on a pipe, exit codes, errors with codes and hints, token presets, a purchase recorded the
//! way an agent would, and the local backup and restore. One module per command family; `common`
//! runs the binary.

#![allow(
    clippy::unwrap_used,
    reason = "test code: a panic is the failure, and the helpers are not #[test] functions"
)]

mod common;

mod adding;
mod approvals;
mod beans;
mod labels;
mod orders;
mod reports;
mod roasted;
mod roasters;
mod roasts;
mod server;
mod setup;
