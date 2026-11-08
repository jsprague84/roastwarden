//! What a roast was expected to show, for reports: the dashboard's roast guidance
//! (`docs/roast-guidance.md`, `apps/dashboard/src/lib/guidance/expect.ts`) in Rust. The
//! profile's own ranges first, then its roast level's defaults (loss shifted for green
//! moisture), then this roaster's own history of the profile (mean ± max(2σ, floor), from
//! three roasts). A test keeps the level table in step with `references.ts`.

use crate::profiles::history::ProfileHistoryRoast;

/// Weight loss %, by level (Mondry's Agtron → loss curve; Sweet Maria's; Schenker)
pub const LOSS: [(&str, f64, f64); 7] = [
    ("extremely_light", 11.0, 13.0),
    ("very_light", 12.0, 14.0),
    ("light", 14.0, 16.0),
    ("medium_light", 15.0, 17.0),
    ("medium", 15.0, 18.0),
    ("medium_dark", 18.0, 20.0),
    ("dark", 20.0, 22.0),
];

/// Development, first crack to drop, seconds, by level (Wendelboe; Alstrup et al.)
pub const DEVELOPMENT: [(&str, f64, f64); 5] = [
    ("extremely_light", 30.0, 75.0),
    ("very_light", 45.0, 90.0),
    ("light", 60.0, 120.0),
    ("medium_light", 90.0, 150.0),
    ("medium", 120.0, 210.0),
];

/// The level's name and its ground Agtron (Gourmet) band: the SCA colour tiles, ±5 each
pub const LEVELS: [(&str, &str, f64, f64); 7] = [
    ("extremely_light", "Extremely light", 90.0, 150.0),
    ("very_light", "Very light", 80.0, 90.0),
    ("light", "Light", 70.0, 80.0),
    ("medium_light", "Medium light", 60.0, 70.0),
    ("medium", "Medium", 50.0, 60.0),
    ("medium_dark", "Medium dark", 40.0, 50.0),
    ("dark", "Dark", 0.0, 40.0),
];

/// Typical specialty green moisture, %, which the loss table assumes
pub const BASE_MOISTURE: f64 = 10.5;
/// Own-history bands need this many other roasts of the profile on the roaster
pub const MIN_HISTORY: usize = 3;

pub fn level_label(key: Option<&str>) -> Option<&'static str> {
    LEVELS.iter().find(|l| Some(l.0) == key).map(|l| l.1)
}

fn of(table: &[(&str, f64, f64)], level: &str) -> Option<(f64, f64)> {
    table.iter().find(|r| r.0 == level).map(|r| (r.1, r.2))
}

/// Where a range came from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Set on the profile
    Profile,
    /// The roast level's default
    Level,
    /// This roaster's own roasts of the profile
    History,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Within,
    Below,
    Above,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Within => "within",
            Verdict::Below => "below",
            Verdict::Above => "above",
        }
    }
}

impl Range {
    pub fn judge(&self, v: Option<f64>) -> Option<Verdict> {
        let v = v.filter(|v| v.is_finite())?;
        Some(if v < self.lo {
            Verdict::Below
        } else if v > self.hi {
            Verdict::Above
        } else {
            Verdict::Within
        })
    }
}

/// mean ± max(2σ, floor), from [`MIN_HISTORY`] values. Every measure judged so is a time, a
/// temperature, a loss or a colour, never below zero: a band that would start below zero (one
/// odd roast widens σ) starts at the smallest value seen instead. The dashboard's
/// `historyBand` (`lib/guidance/expect.ts`) does the same.
pub fn history_band(values: &[f64], floor: f64) -> Option<(f64, f64)> {
    let v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < MIN_HISTORY {
        return None;
    }
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let half = (2.0 * sd).max(floor);
    let lo = mean - half;
    let lo = if lo < 0.0 {
        v.iter().copied().fold(f64::INFINITY, f64::min).max(0.0)
    } else {
        lo
    };
    Some((lo, mean + half))
}

/// What one roast was expected to show
#[derive(Debug, Clone, Default)]
pub struct Expected {
    pub level_label: Option<&'static str>,
    pub weight_loss: Option<Range>,
    pub development_s: Option<Range>,
    pub color_ground: Option<Range>,
    pub fc_s: Option<Range>,
    pub drop_bt: Option<Range>,
    pub total_s: Option<Range>,
    /// Other roasts of the profile on this roaster
    pub history: usize,
}

/// The profile's overrides (`{"weight_loss_pct": [14, 16], …}`), its level (else the roast's
/// target level), the green moisture, and its history on this roaster (this roast left out).
pub fn expected(
    overrides: Option<&serde_json::Value>,
    level: Option<&str>,
    moisture: Option<f64>,
    history: &[ProfileHistoryRoast],
    this_roast: &str,
) -> Expected {
    let level = LEVELS.iter().find(|l| Some(l.0) == level).map(|l| l.0);
    let past: Vec<&ProfileHistoryRoast> = history
        .iter()
        .filter(|h| h.session_id != this_roast)
        .collect();
    let own = |key: &str| -> Option<Range> {
        let r = overrides?.get(key)?.as_array()?;
        let (lo, hi) = (r.first()?.as_f64()?, r.get(1)?.as_f64()?);
        Some(Range {
            lo,
            hi,
            source: Source::Profile,
        })
    };
    let lvl = |table: &[(&str, f64, f64)]| -> Option<Range> {
        let (lo, hi) = of(table, level?)?;
        Some(Range {
            lo,
            hi,
            source: Source::Level,
        })
    };
    let hist = |pick: &dyn Fn(&ProfileHistoryRoast) -> Option<f32>, floor: f64| -> Option<Range> {
        let values: Vec<f64> = past.iter().filter_map(|h| pick(h).map(f64::from)).collect();
        let (lo, hi) = history_band(&values, floor)?;
        Some(Range {
            lo,
            hi,
            source: Source::History,
        })
    };
    let moisture = moisture.filter(|m| m.is_finite());
    let loss = own("weight_loss_pct")
        .or_else(|| {
            lvl(&LOSS).map(|mut r| {
                if let Some(m) = moisture.filter(|m| (m - BASE_MOISTURE).abs() >= 0.05) {
                    let d = m - BASE_MOISTURE;
                    r.lo = ((r.lo + d) * 10.0).round() / 10.0;
                    r.hi = ((r.hi + d) * 10.0).round() / 10.0;
                }
                r
            })
        })
        .or_else(|| hist(&|h| h.weight_loss_pct, 0.5));
    let colour = own("color_ground")
        .or_else(|| {
            let l = LEVELS.iter().find(|l| Some(l.0) == level)?;
            Some(Range {
                lo: l.2,
                hi: l.3,
                source: Source::Level,
            })
        })
        .or_else(|| {
            hist(
                &|h| {
                    h.color_ground
                        .filter(|_| h.color_scale.as_deref().is_none_or(|s| s == "agtron"))
                },
                2.0,
            )
        });
    Expected {
        level_label: level_label(level),
        weight_loss: loss,
        development_s: own("development_s").or_else(|| lvl(&DEVELOPMENT)),
        color_ground: colour,
        fc_s: hist(&|h| h.fc_s, 15.0),
        drop_bt: hist(&|h| h.drop_bt, 2.0),
        total_s: hist(&|h| h.total_s, 15.0),
        history: past.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level table here is the dashboard's (`references.ts`): a change there must come here.
    #[test]
    fn the_level_table_matches_the_dashboards() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/dashboard/src/lib/guidance/references.ts"
        );
        let Ok(ts) = std::fs::read_to_string(path) else {
            eprintln!("skipped: {path} is not here");
            return;
        };
        // Compared without whitespace, so the dashboard's formatting (Prettier) does not matter
        let flat = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let ts = flat(&ts);
        // Each reference's text up to its first `}`: `metric:WL,levels:[…],range:[11,13],…`
        let refs: Vec<&str> = ts
            .split("{metric:")
            .skip(1)
            .map(|r| r.split('}').next().unwrap_or(""))
            .collect();
        assert!(refs.len() > 10, "found too few references: {}", refs.len());
        for (metric, table) in [("WL", &LOSS[..]), ("DEV", &DEVELOPMENT[..])] {
            for (level, lo, hi) in table {
                let found = refs.iter().any(|r| {
                    r.starts_with(&format!("{metric},levels:["))
                        && r.contains(&format!("'{level}'"))
                        && r.contains(&format!("range:[{lo},{hi}]"))
                });
                assert!(
                    found,
                    "{metric} {level} [{lo}, {hi}] is not in references.ts"
                );
            }
        }
        for (key, label, lo, hi) in LEVELS {
            assert!(
                ts.contains(&flat(&format!(
                    "{{ key: '{key}', label: '{label}', agtron: [{lo}, {hi}] }}"
                ))),
                "level {key} differs from references.ts"
            );
        }
        assert!(ts.contains(&format!("exportconstBASE_MOISTURE={BASE_MOISTURE};")));
    }

    /// The same cases as the dashboard's `historyBand` test (`expect.test.ts`)
    #[test]
    fn a_history_band_never_starts_below_zero() {
        assert_eq!(history_band(&[200.0, 200.0], 2.0), None);
        assert_eq!(history_band(&[200.0; 3], 2.0), Some((198.0, 202.0)));
        // Total times 0:30, 10:00, 10:10: mean ± 2σ would start at −4:11
        let (lo, hi) = history_band(&[30.0, 600.0, 610.0], 15.0).unwrap();
        assert_eq!(lo, 30.0);
        assert!(hi > 610.0);
        assert_eq!(history_band(&[-3.0, 1.0, 2.0], 0.5).unwrap().0, 0.0);
    }

    fn roast(id: &str, fc: f32, loss: f32) -> ProfileHistoryRoast {
        ProfileHistoryRoast {
            session_id: id.into(),
            name: id.into(),
            device_id: "r".into(),
            batch_number: None,
            roasted_at: chrono::Utc::now(),
            fc_s: Some(fc),
            fc_bt: Some(196.0),
            drop_s: Some(600.0),
            drop_bt: Some(210.0),
            dry_s: None,
            dry_bt: None,
            development_s: None,
            delta_t_fc_drop: None,
            total_s: Some(600.0),
            weight_loss_pct: Some(loss),
            color_ground: None,
            color_scale: None,
            rmse: None,
        }
    }

    #[test]
    fn profile_then_level_then_history() {
        let h = [
            roast("a", 450.0, 14.0),
            roast("b", 460.0, 14.5),
            roast("c", 470.0, 15.0),
            roast("me", 900.0, 30.0),
        ];
        let ov = serde_json::json!({ "development_s": [100, 140] });
        let e = expected(Some(&ov), Some("light"), Some(11.5), &h, "me");
        assert_eq!(e.history, 3, "the roast itself is left out");
        let d = e.development_s.unwrap();
        assert_eq!((d.lo, d.hi, d.source), (100.0, 140.0, Source::Profile));
        // Light 14–16 %, one point up for 11.5 % moisture
        let l = e.weight_loss.unwrap();
        assert_eq!((l.lo, l.hi, l.source), (15.0, 17.0, Source::Level));
        let c = e.color_ground.unwrap();
        assert_eq!((c.lo, c.hi), (70.0, 80.0));
        // FC 450, 460, 470: mean 460, σ 10, ± 20
        let fc = e.fc_s.unwrap();
        assert_eq!((fc.lo, fc.hi, fc.source), (440.0, 480.0, Source::History));
        assert_eq!(fc.judge(Some(485.0)), Some(Verdict::Above));
        // Drop BT all 210: the floor, ± 2
        let bt = e.drop_bt.unwrap();
        assert_eq!((bt.lo, bt.hi), (208.0, 212.0));
        // No level: loss from history
        let e = expected(None, None, None, &h, "me");
        assert_eq!(e.weight_loss.unwrap().source, Source::History);
        assert!(e.development_s.is_none());
        // Too little history
        let e = expected(None, None, None, &h[..2], "me");
        assert!(e.fc_s.is_none());
    }
}
