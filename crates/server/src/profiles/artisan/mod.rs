//! Artisan Roaster Scope interoperability.
//!
//! Artisan stores profiles (`.alog`) as the `repr()` of a Python dict, not as
//! JSON: `True`/`False`/`None`, single-quoted strings, tuples. Its loader uses
//! `ast.literal_eval`, so files we write must be valid Python literals too.
//! This module has a small Python-literal parser and writer, the profile
//! importer, and the session exporter that produces an Artisan-shaped dict.
//!
//! Field reference (Artisan `profile` dict):
//! - `timex`: sample times in seconds from the start of recording
//! - `temp1`: ET samples, `temp2`: BT samples, `-1` for missing
//! - `timeindex`: `[CHARGE, DRY, FCs, FCe, SCs, SCe, DROP, COOL]` as indices
//!   into `timex`, `-1` when not set (CHARGE uses `0` when unset in old files)
//! - `specialevents`: indices into `timex`; `specialeventstype`: 0..=4 into
//!   `etypes` (default `Air, Drum, Damper, Burner, --`); `specialeventsvalue`:
//!   encoded as `percent / 10 + 1`; `specialeventsStrings`: labels
//! - `mode`: `'C'` or `'F'`
//! - `extradevices`, `extraname1/2`, `extratimex`, `extratemp1/2`, …: extra devices, two
//!   curves each; written only for a roaster's declared channels (docs/channels.md)

use serde_json::Value;

pub(crate) mod literal;
mod parse;
mod profile;
mod roast;
#[cfg(test)]
mod tests;

pub use literal::to_python_literal;
pub use parse::{ParsedSession, parse_profile, parse_session};
pub use profile::build_profile_alog;
pub use roast::build_alog;

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Render the dict as an `.alog` file body.
pub fn render_alog(profile: &Value) -> String {
    let mut s = to_python_literal(profile);
    s.push('\n');
    s
}
