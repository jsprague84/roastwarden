//! Artisan's sliders as outputs: heater, fan, drum and damper, each with the command it
//! writes, scaled to Roastwarden's percent; a Python format as a template.

use crate::roasters::setup::{Output, Outputs};

use super::inputs::Etypes;
use super::names::output_role;
use crate::roasters::aset::ini::Ini;

/// One of Artisan's sliders as an output: Roastwarden's % × `k` + `o` is what Artisan writes
/// (its `value × factor + offset`; a slider in Hz or rpm is % of its top).
pub(super) struct Slider {
    pub(super) label: String,
    pub(super) role: &'static str,
    pub(super) output: Output,
    pub(super) k: f64,
    pub(super) o: f64,
    /// The value written at the slider's bottom (below 0: a signed register)
    pub(super) low: f64,
    /// The slider's whole command, its first part and the rest (`;`-separated)
    pub(super) cmd: String,
    pub(super) first: String,
    pub(super) rest: Vec<String>,
}

/// Artisan's sliders with the action `wanted` (1 a serial command, 2 a Modbus write, 18 a
/// WebSocket command) as outputs; the others are noted.
pub(super) fn sliders(
    ini: &Ini,
    et: &Etypes,
    wanted: &str,
    what: &str,
    notes: &mut Vec<String>,
) -> Vec<Slider> {
    let actions = ini.list("Sliders", "slideractions");
    let commands = ini.list("Sliders", "slidercommands");
    let factors = ini.list("Sliders", "sliderfactors");
    let offsets = ini.list("Sliders", "slideroffsets");
    let mins = ini.list("Sliders", "slidermin");
    let maxs = ini.list("Sliders", "slidermax");
    let units = ini.list("Sliders", "eventsliderunits");
    let num_at = |l: &Vec<String>, i: usize, d: f64| -> f64 {
        l.get(i).and_then(|s| s.trim().parse().ok()).unwrap_or(d)
    };
    let mut out = Vec::new();
    for (i, action) in actions.iter().take(4).enumerate() {
        let action = action.trim();
        let label = et.get(i);
        let cmd = commands.get(i).map_or("", |s| s.trim());
        if action == "0" || action.is_empty() || cmd.is_empty() {
            continue;
        }
        if action != wanted {
            notes.push(format!(
                "slider {label}: its action is not a {what} ({cmd}); left out"
            ));
            continue;
        }
        let mut parts = cmd.split(';').map(str::trim).filter(|s| !s.is_empty());
        let first = parts.next().unwrap_or("").to_string();
        let rest: Vec<String> = parts.map(str::to_string).collect();
        let Some(role) = output_role(&label) else {
            notes.push(format!(
                "slider {label} (`{first}`): not a heater, fan, drum or damper; left out"
            ));
            continue;
        };
        let (f, o) = (num_at(&factors, i, 1.0), num_at(&offsets, i, 0.0));
        let (lo, hi) = (num_at(&mins, i, 0.0), num_at(&maxs, i, 100.0));
        let unit = units.get(i).map_or("", |s| s.trim());
        if f == 0.0
            || !f.is_finite()
            || lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less)
            || lo < 0.0
        {
            notes.push(format!(
                "slider {label}: its range {lo}–{hi} or factor {f} cannot be an output; left out"
            ));
            continue;
        }
        if unit.eq_ignore_ascii_case("c") || unit.eq_ignore_ascii_case("f") {
            notes.push(format!(
                "slider {label} sets a temperature ({lo}–{hi} {unit}); left out"
            ));
            continue;
        }
        // Roastwarden's outputs are %. A slider in % (or none) is that %; a slider in another
        // unit (Hz, rpm, Pa) becomes % of its top: 100 % is its max.
        let (output, k) = if (unit.is_empty() || unit == "%") && hi <= 100.0 {
            (
                Output {
                    label: label.chars().take(24).collect(),
                    min: lo,
                    max: hi,
                    step: 0.0,
                },
                f,
            )
        } else {
            let what = if unit.is_empty() {
                format!("{hi}")
            } else {
                format!("{hi} {unit}")
            };
            let lbl = format!("{label} (% of {what})");
            notes.push(format!(
                "slider {label} goes {lo}–{hi}{}: its output is % of {what}",
                if unit.is_empty() {
                    String::new()
                } else {
                    format!(" {unit}")
                }
            ));
            (
                Output {
                    label: if lbl.chars().count() <= 24 {
                        lbl
                    } else {
                        label.chars().take(24).collect()
                    },
                    min: ((lo / hi * 100.0) * 10.0).round() / 10.0,
                    max: 100.0,
                    step: 0.0,
                },
                hi * f / 100.0,
            )
        };
        out.push(Slider {
            label,
            role,
            output,
            k,
            o,
            low: lo * f + o,
            cmd: cmd.to_string(),
            first,
            rest,
        });
    }
    out
}

/// The output a role declares.
pub(super) fn slot<'a>(o: &'a mut Outputs, role: &str) -> &'a mut Option<Output> {
    match role {
        "heater" => &mut o.heater,
        "fan" => &mut o.fan,
        "drum" => &mut o.drum,
        _ => &mut o.damper,
    }
}

/// Python's `str.format` with one `{}`: `{{` and `}}` are braces, `{}` becomes `value`. `None`
/// when there is no `{}` or more than one, or a lone brace.
pub(super) fn py_format(s: &str, value: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    let mut n = 0;
    while let Some(c) = it.next() {
        match (c, it.peek()) {
            ('{', Some('{')) => {
                it.next();
                out.push('{');
            }
            ('{', Some('}')) => {
                it.next();
                out.push_str(value);
                n += 1;
            }
            ('}', Some('}')) => {
                it.next();
                out.push('}');
            }
            ('{' | '}', _) => return None,
            (c, _) => out.push(c),
        }
    }
    (n == 1).then_some(out)
}
