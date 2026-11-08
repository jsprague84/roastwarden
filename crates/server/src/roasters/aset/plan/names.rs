//! What an Artisan name means: an output's role, a setpoint, a unit, a temperature, and a
//! channel key made from it.

/// What an output-like name controls: heater, fan, drum or damper.
pub(super) fn output_role(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if has(&[
        "burner", "heater", "heat", "power", "gas", "flame", "brenner",
    ]) {
        Some("heater")
    } else if has(&["damper", "flap", "klappe"]) {
        Some("damper")
    } else if has(&["drum", "trommel"]) {
        Some("drum")
    } else if has(&["air", "fan", "blower", "luft", "lüfter"]) {
        Some("fan")
    } else {
        None
    }
}

/// A setpoint by its name (`SP-Air`, `Burner SV`): what an output aims at, not its read-back.
pub(super) fn setpoint_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "sp" | "sv" | "set" | "setpoint" | "target"))
}

/// A unit written in a curve's name (`Gas Pressure Pa`, `Drum RPM`), for a channel.
pub(super) fn unit_in(name: &str) -> Option<&'static str> {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric() && c != '%')
        .map(str::to_lowercase)
        .collect();
    let has = |w: &str| words.iter().any(|x| x == w);
    if name.contains('%') || has("humidity") {
        Some("%")
    } else if has("pa") {
        Some("Pa")
    } else if has("dapa") {
        Some("daPa")
    } else if has("kpa") {
        Some("kPa")
    } else if has("mbar") {
        Some("mbar")
    } else if has("rpm") {
        Some("rpm")
    } else if has("hz") {
        Some("Hz")
    } else if has("kwh") {
        Some("kWh")
    } else if has("kw") {
        Some("kW")
    } else {
        None
    }
}

/// A temperature by its name: Artisan's abbreviations and the usual words.
pub(super) fn temp_name(name: &str) -> bool {
    let n = name.to_lowercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).collect();
    ["it", "dt", "ct", "xt", "et", "bt", "met", "at", "st"]
        .iter()
        .any(|w| words.contains(w))
        || ["temp", "inlet", "exhaust", "stack", "env", "ambient", "°"]
            .iter()
            .any(|w| n.contains(w))
}

/// A channel key from a curve's name: `Inlet Temp` → `inlet_temp`.
pub(super) fn key_of(name: &str) -> String {
    let mut k = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            k.push(c.to_ascii_lowercase());
        } else if !k.ends_with('_') && !k.is_empty() {
            k.push('_');
        }
    }
    let k = k.trim_end_matches('_').to_string();
    let k = if k.starts_with(|c: char| c.is_ascii_alphabetic()) {
        k
    } else {
        format!("ch_{k}")
    };
    k.chars().take(32).collect()
}
