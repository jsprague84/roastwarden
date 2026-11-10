//! An `.alog` read: as a profile (its curve, events and landmarks) or as a recorded roast
//! (its samples, events and weights) for the imports.

use anyhow::{Result, bail};

use super::literal::parse_alog;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEvent {
    pub name: String,
    /// Seconds from CHARGE.
    pub time: f64,
    pub bean_temp: f32,
    pub env_temp: f32,
    /// Artisan short code: CHARGE, TP, DRY, FCs, FCe, SCs, SCe, DROP, COOL.
    pub event_type: String,
}

#[derive(Debug, Clone)]
pub struct ParsedProfile {
    pub title: String,
    pub roast_date: String,
    pub events: Vec<ParsedEvent>,
}

const TIMEINDEX_SLOTS: [(usize, &str, &str); 7] = [
    (1, "DRY", "Dry End"),
    (2, "FCs", "First Crack Start"),
    (3, "FCe", "First Crack End"),
    (4, "SCs", "Second Crack Start"),
    (5, "SCe", "Second Crack End"),
    (6, "DROP", "Drop"),
    (7, "COOL", "Cool End"),
];

fn event_label(code: &str) -> &'static str {
    match code {
        "CHARGE" => "Charge",
        "TP" => "Turning Point",
        "DRY" => "Dry End",
        "FCs" => "First Crack Start",
        "FCe" => "First Crack End",
        "SCs" => "Second Crack Start",
        "SCe" => "Second Crack End",
        "DROP" => "Drop",
        "COOL" => "Cool End",
        _ => "Event",
    }
}

/// Parse an Artisan profile's title, date and landmark events, rebased so
/// that CHARGE is `t = 0` and temperatures are in °C. The curve itself comes
/// from [`parse_session`].
pub fn parse_profile(content: &str) -> Result<ParsedProfile> {
    let data = parse_alog(content)?;
    let fahrenheit = data
        .get("mode")
        .and_then(|m| m.as_str())
        .is_some_and(|m| m.eq_ignore_ascii_case("F"));
    let conv = |t: f64| -> f32 {
        if fahrenheit {
            ((t - 32.0) * 5.0 / 9.0) as f32
        } else {
            t as f32
        }
    };

    let nums = |key: &str| -> Vec<Option<f64>> {
        data.get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(serde_json::Value::as_f64).collect())
            .unwrap_or_default()
    };
    let timex = nums("timex");
    let temp1 = nums("temp1");
    let temp2 = nums("temp2");
    if timex.is_empty() {
        bail!("profile has no 'timex' samples");
    }

    let timeindex: Vec<i64> = data
        .get("timeindex")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(-1)).collect())
        .unwrap_or_default();
    let charge_idx = match timeindex.first() {
        Some(i) if *i >= 0 && (*i as usize) < timex.len() => *i as usize,
        _ => 0,
    };
    let t0 = timex[charge_idx].unwrap_or(0.0);

    let sample = |i: usize| -> Option<(f64, f32, f32)> {
        let t = timex.get(i).copied().flatten()?;
        let bt = temp2.get(i).copied().flatten().unwrap_or(-1.0);
        let et = temp1.get(i).copied().flatten().unwrap_or(-1.0);
        Some((t - t0, conv(bt), conv(et)))
    };

    // Landmark events, from timeindex when present, else from `computed`.
    let mut events = Vec::new();
    let computed = data
        .get("computed")
        .and_then(|c| c.as_object())
        .cloned()
        .unwrap_or_default();
    let has_timeindex = timeindex.len() >= 7;
    if has_timeindex {
        if timeindex[0] >= 0
            && let Some((t, bt, et)) = sample(charge_idx)
        {
            events.push(ParsedEvent {
                name: "Charge".into(),
                time: t,
                bean_temp: bt,
                env_temp: et,
                event_type: "CHARGE".into(),
            });
        }
        for (slot, code, label) in TIMEINDEX_SLOTS {
            let idx = timeindex[slot];
            if idx > 0
                && let Some((t, bt, et)) = sample(idx as usize)
            {
                events.push(ParsedEvent {
                    name: label.into(),
                    time: t,
                    bean_temp: bt,
                    env_temp: et,
                    event_type: code.into(),
                });
            }
        }
    }
    // `computed` carries TP, which timeindex does not, and is the only source
    // for files without timeindex.
    let codes: &[&str] = if has_timeindex {
        &["TP"]
    } else {
        &["CHARGE", "TP", "DRY", "FCs", "FCe", "SCs", "SCe", "DROP"]
    };
    for code in codes {
        if let Some(t) = computed
            .get(&format!("{code}_time"))
            .and_then(serde_json::Value::as_f64)
        {
            let bt = computed
                .get(&format!("{code}_BT"))
                .and_then(serde_json::Value::as_f64)
                .map_or(0.0, conv);
            let et = computed
                .get(&format!("{code}_ET"))
                .and_then(serde_json::Value::as_f64)
                .map_or(0.0, conv);
            events.push(ParsedEvent {
                name: event_label(code).into(),
                time: t,
                bean_temp: bt,
                env_temp: et,
                event_type: (*code).into(),
            });
        }
    }
    events.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported Profile")
        .to_string();
    let roast_date = data
        .get("roastisodate")
        .or_else(|| data.get("roastdate"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(ParsedProfile {
        title,
        roast_date,
        events,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSample {
    /// Seconds from CHARGE (may be negative for pre-charge samples, which are dropped).
    pub elapsed: f64,
    pub bean_temp: Option<f32>,
    pub env_temp: Option<f32>,
    /// Heater % from Burner events, carried forward.
    pub heater: Option<f64>,
    /// Fan % from Air events, carried forward.
    pub fan: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ParsedSession {
    pub title: String,
    pub roast_start: Option<chrono::DateTime<chrono::Utc>>,
    pub beans: Option<String>,
    pub green_weight_g: Option<f32>,
    pub roasted_weight_g: Option<f32>,
    pub notes: Option<String>,
    pub ambient_temp: Option<f32>,
    pub humidity: Option<f32>,
    /// Artisan `roastertype`: the machine, free text.
    pub roaster_type: Option<String>,
    pub samples: Vec<ParsedSample>,
    pub events: Vec<ParsedEvent>,
}

fn weight_to_grams(value: f64, unit: &str) -> f32 {
    let f = match unit.to_ascii_lowercase().as_str() {
        "kg" => 1000.0,
        "lb" | "lbs" => 453.592,
        "oz" => 28.3495,
        _ => 1.0,
    };
    (value * f) as f32
}

/// Decode an Artisan event value back to a percentage (Artisan's
/// `eventsInternal2ExternalValue`): `0.0` (anything in −1..=1) is 0 %,
/// `p / 10 + 1` is `p` %. Negative values (Artisan allows −100) clamp to 0.
pub(super) fn artisan_event_percent(v: f64) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    if v.abs() <= 1.0 {
        return Some(0.0);
    }
    let p = if v > 0.0 {
        (v * 10.0).round() - 10.0
    } else {
        (v * 10.0).round() + 10.0
    };
    Some(p.clamp(0.0, 100.0))
}

/// Parse a recorded Artisan roast. Samples before CHARGE are discarded, times
/// are rebased on CHARGE, temperatures are converted to °C, and Air/Burner
/// special events are turned into fan/heater curves.
pub fn parse_session(content: &str) -> Result<ParsedSession> {
    let data = parse_alog(content)?;
    let fahrenheit = data
        .get("mode")
        .and_then(|m| m.as_str())
        .is_some_and(|m| m.eq_ignore_ascii_case("F"));
    let conv = |t: f64| -> f32 {
        if fahrenheit {
            ((t - 32.0) * 5.0 / 9.0) as f32
        } else {
            t as f32
        }
    };
    let nums = |key: &str| -> Vec<Option<f64>> {
        data.get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(serde_json::Value::as_f64).collect())
            .unwrap_or_default()
    };
    let timex = nums("timex");
    let temp1 = nums("temp1");
    let temp2 = nums("temp2");
    if timex.is_empty() {
        bail!("roast log has no 'timex' samples");
    }
    let timeindex: Vec<i64> = data
        .get("timeindex")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(-1)).collect())
        .unwrap_or_default();
    let charge_idx = match timeindex.first() {
        Some(i) if *i >= 0 && (*i as usize) < timex.len() => *i as usize,
        _ => 0,
    };
    let t0 = timex[charge_idx].unwrap_or(0.0);

    // Air (0) / Burner (3) events become control curves; other types are ignored.
    let etypes: Vec<String> = data.get("etypes").and_then(|v| v.as_array()).map_or_else(
        || {
            vec![
                "air".into(),
                "drum".into(),
                "damper".into(),
                "burner".into(),
            ]
        },
        |a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_lowercase))
                .collect()
        },
    );
    let sp_idx = nums("specialevents");
    let sp_type = nums("specialeventstype");
    let sp_val = nums("specialeventsvalue");
    let mut fan_by_idx = std::collections::BTreeMap::<usize, f64>::new();
    let mut heater_by_idx = std::collections::BTreeMap::<usize, f64>::new();
    for k in 0..sp_idx.len() {
        let (Some(idx), Some(ty), Some(val)) = (
            sp_idx.get(k).copied().flatten(),
            sp_type.get(k).copied().flatten(),
            sp_val.get(k).copied().flatten(),
        ) else {
            continue;
        };
        let Some(pct) = artisan_event_percent(val) else {
            continue;
        };
        let name = etypes
            .get(ty as usize)
            .map_or("", std::string::String::as_str);
        let idx = idx.max(0.0) as usize;
        if name.contains("air") || name.contains("fan") {
            fan_by_idx.insert(idx, pct.clamp(0.0, 100.0));
        } else if name.contains("burner")
            || name.contains("heat")
            || name.contains("power")
            || name.contains("gas")
        {
            heater_by_idx.insert(idx, pct.clamp(0.0, 100.0));
        }
    }

    let mut samples = Vec::new();
    let (mut fan, mut heater): (Option<f64>, Option<f64>) = (None, None);
    for (i, t) in timex.iter().enumerate() {
        if let Some(v) = fan_by_idx.get(&i) {
            fan = Some(*v);
        }
        if let Some(v) = heater_by_idx.get(&i) {
            heater = Some(*v);
        }
        if i < charge_idx {
            continue;
        }
        let Some(t) = t else { continue };
        let bt = temp2
            .get(i)
            .copied()
            .flatten()
            .filter(|v| *v > -1.0)
            .map(conv);
        let et = temp1
            .get(i)
            .copied()
            .flatten()
            .filter(|v| *v > -1.0)
            .map(conv);
        if bt.is_none() && et.is_none() {
            continue;
        }
        samples.push(ParsedSample {
            elapsed: t - t0,
            bean_temp: bt,
            env_temp: et,
            heater,
            fan,
        });
    }

    // Landmarks via the shared profile parser (same rebase and unit handling).
    let events = parse_profile(content).map(|p| p.events).unwrap_or_default();

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported roast")
        .to_string();
    let roast_start = data
        .get("roastepoch")
        .and_then(serde_json::Value::as_i64)
        .filter(|e| *e > 0)
        .and_then(|e| chrono::DateTime::<chrono::Utc>::from_timestamp(e, 0))
        .or_else(|| {
            let d = data.get("roastisodate").and_then(|v| v.as_str())?;
            let t = data
                .get("roasttime")
                .and_then(|v| v.as_str())
                .unwrap_or("00:00:00");
            chrono::NaiveDateTime::parse_from_str(&format!("{d} {t}"), "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| n.and_utc())
        });
    let beans = data
        .get("beans")
        .and_then(|v| v.as_str())
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    let (green, roasted) = match data.get("weight").and_then(|v| v.as_array()) {
        Some(w) if w.len() >= 2 => {
            let unit = w.get(2).and_then(|u| u.as_str()).unwrap_or("g");
            let g = w[0]
                .as_f64()
                .filter(|v| *v > 0.0)
                .map(|v| weight_to_grams(v, unit));
            let r = w[1]
                .as_f64()
                .filter(|v| *v > 0.0)
                .map(|v| weight_to_grams(v, unit));
            (g, r)
        }
        _ => (None, None),
    };
    let notes = data
        .get("roastingnotes")
        .and_then(|v| v.as_str())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    let ambient_temp = data
        .get("ambientTemp")
        .and_then(serde_json::Value::as_f64)
        .filter(|v| *v > 0.0)
        .map(conv);
    let humidity = data
        .get("ambient_humidity")
        .and_then(serde_json::Value::as_f64)
        .filter(|v| *v > 0.0)
        .map(|v| v as f32);

    let roaster_type = data
        .get("roastertype")
        .and_then(|v| v.as_str())
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty());

    Ok(ParsedSession {
        title,
        roast_start,
        beans,
        green_weight_g: green,
        roasted_weight_g: roasted,
        notes,
        ambient_temp,
        humidity,
        roaster_type,
        samples,
        events,
    })
}
