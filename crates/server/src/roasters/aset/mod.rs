//! Artisan machine setups (`.aset`) as Roastwarden roasters (docs/artisan-machines.md).
//!
//! Artisan ships its machine catalogue as data: one Qt INI file per machine, naming the device
//! Artisan reads it through, its Modbus inputs (register, function, divider, float, °C/°F,
//! slave), its sliders (the command each writes) and its extra curves. This module reads such a
//! file and plans a Roastwarden roaster from it: a connection, a register map with roles, outputs
//! from the sliders and channels from the extra curves, at control `off` (the owner checks the
//! readings first). It writes nothing: the CLI and the dashboard apply the plan through the
//! normal device, register map and setup calls.
//!
//! The catalogue itself is embedded (`assets/artisan/machines.txt`, made by
//! `scripts/artisan-machines.py` from a pinned Artisan commit, AGPL like Roastwarden).

mod ini;
mod machines;
mod plan;
mod qt;
#[cfg(test)]
mod tests;

pub use machines::{Machine, machines};
pub use plan::{Plan, plan, plan_machine};

/// The embedded catalogue: `@@ Brand/Model` then that machine's trimmed `.aset` lines.
const CATALOGUE: &str = include_str!("../../../assets/artisan/machines.txt");

/// The Artisan commit the catalogue was trimmed from (its header says so too).
pub fn catalogue_source() -> &'static str {
    CATALOGUE
        .lines()
        .find_map(|l| l.strip_prefix("; Source: "))
        .unwrap_or("Artisan")
        .trim_end_matches(',')
}
