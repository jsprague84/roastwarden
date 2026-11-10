//! A profile request checked before it is stored: points, events and curves, the roast guidance
//! fields, auto DROP against its targets, the roaster kind. `ProfileInputError` is the 400.

use anyhow::Result;

use crate::profiles::types::{CreateProfileRequest, ValidProfileEvent};

/// Recognise a profile previously exported by Roastwarden (`GET /api/profiles/{id}/export`)
/// or any JSON with a `points` array of `{time_seconds, target_temp, ...}`.
/// Trimmed text, or `None` when empty.
pub(crate) fn non_blank(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Bad profile input (a 400, not a 500).
#[derive(Debug)]
pub struct ProfileInputError(pub String);

impl std::fmt::Display for ProfileInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileInputError {}

/// Validate points, events and curves; returns the validated events when the
/// request carries any.
/// Roast level keys: the SCA colour tiles on ground Agtron Gourmet (docs/roast-guidance.md).
pub const ROAST_LEVELS: [&str; 7] = [
    "extremely_light",
    "very_light",
    "light",
    "medium_light",
    "medium",
    "medium_dark",
    "dark",
];

pub const ROAST_PURPOSES: [&str; 4] = ["espresso", "filter", "omni", "other"];

/// Expected ranges a profile may override, with the widest sane bounds.
const EXPECTATION_KEYS: [(&str, f64, f64); 4] = [
    ("weight_loss_pct", 0.0, 40.0),
    ("development_s", 0.0, 900.0),
    ("dtr_pct", 0.0, 60.0),
    ("color_ground", 0.0, 150.0),
];

/// Roast guidance fields of a profile request, checked; `None` = absent
/// (keep on update), `Some(None)` = clear.
pub(crate) struct GuidanceInput {
    pub(crate) level: Option<Option<String>>,
    pub(crate) purpose: Option<Option<String>>,
    /// Normalised JSON text.
    pub(crate) expectations: Option<Option<String>>,
}

/// Auto DROP: what the request sets (`None` keeps the stored one, `Some(None)` switches it off),
/// checked against the request's targets; a kept one (`stored`) is checked too, so a profile
/// cannot lose the drop time its auto DROP goes by.
pub(crate) fn validate_auto_drop(
    req: &CreateProfileRequest,
    stored: Option<String>,
) -> Result<Option<Option<String>>> {
    use crate::replay::auto_drop::{DropBy, trigger_value};
    let bad = |m: String| anyhow::Error::new(ProfileInputError(m));
    let set = req.auto_drop.as_ref().map(|v| non_blank(v.as_deref()));
    let check = match (&set, &stored) {
        (Some(Some(v)), _) => Some(v.clone()),
        (Some(None), _) => None,
        (None, kept) => kept.clone(),
    };
    if let Some(v) = check {
        let by: DropBy = v.parse().map_err(bad)?;
        trigger_value(
            by,
            req.target_total_time.map(f64::from),
            req.target_first_crack.map(f64::from),
            req.target_end_temp.map(f64::from),
        )
        .map_err(|m| {
            bad(if set.is_none() {
                format!("{m} (switch auto DROP off first)")
            } else {
                m
            })
        })?;
    }
    Ok(set)
}

pub(crate) fn validate_guidance(req: &CreateProfileRequest) -> Result<GuidanceInput> {
    let bad = |m: String| anyhow::Error::new(ProfileInputError(m));
    let pick = |v: &Option<Option<String>>, allowed: &[&str], what: &str| match v
        .as_ref()
        .map(|v| non_blank(v.as_deref()))
    {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(s)) if allowed.contains(&s.as_str()) => Ok(Some(Some(s))),
        Some(Some(s)) => Err(bad(format!(
            "unknown {what} {s:?} (one of {})",
            allowed.join(", ")
        ))),
    };
    let level = pick(&req.roast_level, &ROAST_LEVELS, "roast level")?;
    let purpose = pick(&req.roast_purpose, &ROAST_PURPOSES, "roast purpose")?;
    let expectations = match &req.expectations {
        None => None,
        Some(None | Some(serde_json::Value::Null)) => Some(None),
        Some(Some(serde_json::Value::Object(map))) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                let Some(&(_, min, max)) = EXPECTATION_KEYS.iter().find(|(k, ..)| k == key) else {
                    return Err(bad(format!("unknown expectation {key:?}")));
                };
                if value.is_null() {
                    continue;
                }
                let pair = value
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))
                    .filter(|(lo, hi)| min <= *lo && lo <= hi && *hi <= max)
                    .ok_or_else(|| {
                        bad(format!(
                            "expectation {key:?} must be [low, high] within {min}..{max}"
                        ))
                    })?;
                out.insert(key.clone(), serde_json::json!([pair.0, pair.1]));
            }
            Some((!out.is_empty()).then(|| serde_json::Value::Object(out).to_string()))
        }
        Some(Some(_)) => return Err(bad("expectations must be an object".into())),
    };
    Ok(GuidanceInput {
        level,
        purpose,
        expectations,
    })
}

/// Whether `k` is one of the setup kinds (`setup::Kind`).
pub(crate) fn is_roaster_kind(k: &str) -> bool {
    crate::roasters::setup::Kind::ALL
        .iter()
        .any(|kind| kind.as_str() == k)
}

/// A profile's `roaster_kind`: absent keeps it, `null` or blank clears it, else a setup kind.
pub(crate) fn validate_roaster_kind(v: Option<&Option<String>>) -> Result<Option<Option<String>>> {
    match v.map(|k| non_blank(k.as_deref())) {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(k)) if is_roaster_kind(&k) => Ok(Some(Some(k))),
        Some(Some(k)) => Err(anyhow::Error::new(ProfileInputError(format!(
            "unknown roaster kind {k:?} (one of {})",
            crate::roasters::setup::Kind::ALL
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))),
    }
}

pub(crate) fn validate_profile_input(
    req: &CreateProfileRequest,
) -> Result<Option<Vec<ValidProfileEvent>>> {
    let bad = |m: String| anyhow::Error::new(ProfileInputError(m));
    for p in &req.points {
        if p.time_seconds < 0 || !p.target_temp.is_finite() {
            return Err(bad(format!("invalid point at {} s", p.time_seconds)));
        }
        if let Some(h) = p.heater_pct
            && !(0..=100).contains(&h)
        {
            return Err(bad(format!(
                "heater_pct {h} at {} s is outside 0..=100",
                p.time_seconds
            )));
        }
    }
    if let Some(Some(c)) = &req.curves {
        c.validate().map_err(bad)?;
    }
    let Some(events) = &req.events else {
        return Ok(None);
    };
    if events.len() > 500 {
        return Err(bad("a profile holds at most 500 events".into()));
    }
    events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            e.validate()
                .map_err(|m| bad(format!("event {}: {m}", i + 1)))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}
