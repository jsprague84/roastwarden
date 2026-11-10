//! The cupping sheet (`assets/reports/cupping.typ`, schema `roastwarden.report.cupping/1`,
//! `docs/cupping.md`): a blank form after the classic SCA protocol, with blind codes and the
//! key on a page of its own, or a session's scores as a table with a dot strip per attribute.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::{DASH, Options, ReportError, day_month, long_date, meta, short_date};
use crate::coffee::inventory::InventoryService;
use crate::roasts::cupping::{ATTRIBUTES, CUPS, Kind};
use crate::roasts::types::{CuppingWithAttributes, RoastSession};

/// At most this many samples on a blank sheet, roasts in a session table
pub const MAX_SAMPLES: usize = 24;
pub const MAX_ROASTS: usize = 30;

/// What to make: a blank form (for these roasts, and `samples` more of the owner's own), or
/// the scores of these roasts (else of the cupping session on `date`, else the latest).
#[derive(Debug, Clone, Default)]
pub struct Ask {
    pub roasts: Vec<String>,
    pub blank: bool,
    pub samples: usize,
    /// 2 (scales to mark) or 6 (boxes to write numbers in)
    pub per_page: u8,
    pub date: Option<NaiveDate>,
}

/// A roast as the sheet names it
pub struct Roasted {
    pub session: RoastSession,
    pub coffee: String,
    pub profile: Option<String>,
    pub cupping: Option<CuppingWithAttributes>,
}

/// The short names the table's columns use, in the form's order
const SHORT: [&str; 10] = [
    "Aroma", "Flavor", "Aftert.", "Acidity", "Body", "Balance", "Unif.", "Clean", "Sweet.",
    "Overall",
];

pub async fn gather(db: &SqlitePool, ask: &Ask, o: &Options) -> Result<Vec<Roasted>, ReportError> {
    let mut ids: Vec<String> = ask.roasts.clone();
    if ids.is_empty() && !ask.blank {
        // A session: the cuppings of one day, the latest unless asked
        let rows: Vec<(String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT session_id, created_at FROM cupping_scores ORDER BY created_at DESC LIMIT 500",
        )
        .fetch_all(db)
        .await?;
        let day = ask
            .date
            .or_else(|| rows.first().map(|(_, t)| o.local(*t).date_naive()));
        let Some(day) = day else {
            return Err(ReportError::Bad(
                "no roast is cupped yet: `blank` makes a sheet to score on".into(),
            ));
        };
        ids = rows
            .into_iter()
            .filter(|(_, t)| o.local(*t).date_naive() == day)
            .map(|(id, _)| id)
            .collect();
        ids.reverse();
        if ids.is_empty() {
            return Err(ReportError::NotFound(format!("no cupping on {day}")));
        }
    }
    if ids.len() > MAX_SAMPLES.max(MAX_ROASTS) {
        return Err(ReportError::Bad(format!(
            "{} roasts: at most {MAX_ROASTS} on one sheet",
            ids.len()
        )));
    }
    let inv = InventoryService::new(db.clone());
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(session) = crate::roasts::sessions::get_session(db, &id).await? else {
            return Err(ReportError::NotFound(format!("no roast {id}")));
        };
        let coffee = match &session.bean_id {
            Some(b) => inv.bean(b).await?.map(|b| b.name),
            None => None,
        }
        .unwrap_or_else(|| session.name.clone());
        let profile: Option<String> = match &session.profile_id {
            Some(p) => {
                sqlx::query_scalar("SELECT name FROM roast_profiles WHERE id = ?")
                    .bind(p)
                    .fetch_optional(db)
                    .await?
            }
            None => None,
        };
        let cupping = crate::roasts::cupping::get_cupping(db, &id).await?;
        out.push(Roasted {
            session,
            coffee,
            profile,
            cupping,
        });
    }
    Ok(out)
}

/// A three-digit blind code for a sample, the same all day for the same roast, none twice
fn code(seed: &str, day: NaiveDate, taken: &mut HashSet<u16>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.bytes().chain(day.to_string().bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let mut n = 100 + (h % 900) as u16;
    while !taken.insert(n) {
        n = 100 + (n - 100 + 1) % 900;
    }
    n.to_string()
}

/// "1 day", "3 days", "under a day"
fn rest(from: NaiveDate, to: NaiveDate) -> String {
    match (to - from).num_days() {
        d if d <= 0 => "under a day".into(),
        1 => "1 day".into(),
        d => format!("{d} days"),
    }
}

fn roasted_on(s: &RoastSession, o: &Options) -> NaiveDate {
    o.local(s.start_time.unwrap_or(s.created_at)).date_naive()
}

/// `#5 · Palmichal Vecindario`
fn name(r: &Roasted) -> String {
    match r.session.batch_number {
        Some(n) => format!("#{n} · {}", r.coffee),
        None => r.coffee.clone(),
    }
}

fn attributes() -> Vec<Value> {
    ATTRIBUTES
        .iter()
        .zip(SHORT)
        .map(|((key, label, kind, _), short)| {
            json!({
                "key": key, "label": label, "short": short,
                "kind": if *kind == Kind::Cups { "cups" } else { "quality" },
            })
        })
        .collect()
}

const BLANK_NOTE: &str = "Scored after the SCA cupping protocol (2004): 8.25 g of coffee per 150 ml, water near 93 °C, five cups a sample, roasted 8–24 h before. Quality from 6 (good) to 10 in quarter points; uniformity, clean cup and sweetness 2 points a cup; a taint takes 2 and a fault 4 for each cup. Final = total − defects. The 2024 CVA forms are SCA's own: sca.coffee/value-assessment.";
const FILLED_NOTE: &str = "Classic SCA form (2004 protocol): seven quality attributes 6–10 in quarter points; uniformity, clean cup and sweetness 2 a cup over five cups; a taint takes 2 and a fault 4 a cup off the total. 80 and over is specialty: 80 very good, 85 excellent, 90 outstanding. Never converted to the 2024 CVA scores.";

pub fn build(roasts: &[Roasted], ask: &Ask, o: &Options) -> Result<Value, ReportError> {
    let kind = super::kind("cupping").expect("the cupping kind");
    let today = o.local(o.now).date_naive();
    if ask.blank {
        let n = roasts.len() + ask.samples;
        let n = if n == 0 { 2 } else { n };
        if n > MAX_SAMPLES {
            return Err(ReportError::Bad(format!(
                "{n} samples: at most {MAX_SAMPLES} on one sheet"
            )));
        }
        let mut taken = HashSet::new();
        let mut samples = Vec::new();
        let mut key = Vec::new();
        for r in roasts {
            let c = code(&r.session.id, today, &mut taken);
            let on = roasted_on(&r.session, o);
            samples.push(json!({
                "code": c,
                "roasted": day_month(&o.local(r.session.start_time.unwrap_or(r.session.created_at))),
                "rest": rest(on, today),
            }));
            let label = match &r.profile {
                Some(p) => format!("{} · {p}", name(r)),
                None => name(r),
            };
            key.push(json!({
                "code": c,
                "label": label,
                "roasted": short_date(&o.local(r.session.start_time.unwrap_or(r.session.created_at))),
            }));
        }
        for i in roasts.len()..n {
            let c = code(&format!("sample {i}"), today, &mut taken);
            samples.push(json!({ "code": c, "roasted": null, "rest": null }));
            key.push(json!({ "code": c, "label": "a sample of your own", "roasted": DASH }));
        }
        // On the form in the codes' order, so the order the roasts were picked in tells nothing
        let mut pairs: Vec<(Value, Value)> = samples.into_iter().zip(key).collect();
        pairs.sort_by(|a, b| a.0["code"].as_str().cmp(&b.0["code"].as_str()));
        let (samples, key): (Vec<Value>, Vec<Value>) = pairs.into_iter().unzip();
        let subject = format!(
            "{} · {} sample{}",
            short_date(&o.local(o.now)),
            n,
            if n == 1 { "" } else { "s" }
        );
        return Ok(json!({
            "schema": kind.schema,
            "meta": meta(kind, &subject, o),
            "mode": "blank",
            "per_page": if ask.per_page >= 6 { 6 } else { 2 },
            "cups": CUPS,
            "attributes": attributes(),
            "samples": samples,
            "key": key,
            "protocol": BLANK_NOTE,
        }));
    }

    // A session's scores
    let cupped: Vec<(&Roasted, &CuppingWithAttributes)> = roasts
        .iter()
        .filter_map(|r| r.cupping.as_ref().map(|c| (r, c)))
        .collect();
    if cupped.is_empty() {
        return Err(ReportError::Bad(
            "none of these roasts is cupped yet: `blank` makes a sheet to score on".into(),
        ));
    }
    let latest = cupped
        .iter()
        .map(|(_, c)| c.cupping.created_at)
        .max()
        .expect("one at least");
    let mut rows = Vec::new();
    let mut strip: Vec<Vec<Value>> = vec![Vec::new(); ATTRIBUTES.len()];
    let mut best: Option<(f32, String)> = None;
    for (i, (r, c)) in cupped.iter().enumerate() {
        let mark = r.session.batch_number.map_or_else(
            || ((b'A' + (i % 26) as u8) as char).to_string(),
            |n| n.to_string(),
        );
        let score = |key: &str| {
            c.attributes
                .iter()
                .find(|a| a.attribute_name == key)
                .map(|a| a.score)
        };
        let scores: Vec<String> = ATTRIBUTES
            .iter()
            .map(|(k, _, kind, _)| match (score(k), kind) {
                (None, _) => DASH.to_string(),
                (Some(s), Kind::Cups) => format!("{s:.0}"),
                (Some(s), Kind::Quality) => format!("{s:.2}"),
            })
            .collect();
        for (j, (k, _, kind, _)) in ATTRIBUTES.iter().enumerate() {
            if let (Some(s), Kind::Quality) = (score(k), kind) {
                strip[j].push(json!({ "mark": mark, "v": s }));
            }
        }
        if let Some(f) = c.cupping.final_score
            && best.as_ref().is_none_or(|(b, _)| f > *b)
        {
            best = Some((
                f,
                format!(
                    "{}, {f:.2} ({})",
                    name(r).split(" · ").next().unwrap_or_default(),
                    c.band.clone().unwrap_or_default()
                ),
            ));
        }
        let on = roasted_on(&r.session, o);
        let cupped_on = o.local(c.cupping.created_at).date_naive();
        let cups: Vec<String> = ["uniformity", "clean_cup", "sweetness"]
            .iter()
            .map(|k| score(k).map_or(DASH.into(), |s| format!("{s:.0}")))
            .collect();
        rows.push(json!({
            "mark": mark,
            "label": name(r),
            "sub": format!(
                "{}roasted {}, rested {}",
                r.profile.as_ref().map_or(String::new(), |p| format!("{p} · ")),
                day_month(&o.local(r.session.start_time.unwrap_or(r.session.created_at))),
                rest(on, cupped_on)
            ),
            "scores": scores,
            "cups": cups.join(" "),
            "total": c.total_score.map_or(DASH.into(), |t| format!("{t:.2}")),
            "defects": c.defects.to_string(),
            "final": c.cupping.final_score.map_or(DASH.into(), |f| format!("{f:.2}")),
            "band": c.band.clone().unwrap_or_else(|| "not all ten scored".into()),
            "notes": c.cupping.notes.as_deref().filter(|n| !n.trim().is_empty()),
        }));
    }
    let strip_rows: Vec<Value> = ATTRIBUTES
        .iter()
        .zip(strip)
        .filter(|((_, _, kind, _), _)| *kind == Kind::Quality)
        .map(|((_, label, _, _), dots)| json!({ "label": label, "dots": dots }))
        .collect();
    let day = o.local(latest);
    let subject = format!(
        "{} · {} roast{}",
        short_date(&day),
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );
    Ok(json!({
        "schema": kind.schema,
        "meta": meta(kind, &subject, o),
        "mode": "filled",
        "per_page": 2,
        "cups": CUPS,
        "attributes": attributes(),
        "session": {
            "date": long_date(&day),
            "roasts": rows.len(),
            "best": best.map(|(_, b)| b),
        },
        "rows": rows,
        "strip": { "min": 6, "max": 10, "ticks": [6, 6.5, 7, 7.5, 8, 8.5, 9, 9.5, 10], "rows": strip_rows },
        "protocol": FILLED_NOTE,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};
    use crate::roasts::types::{CuppingAttribute, CuppingScore};

    fn roast(batch: i32, scores: Option<(f32, i64)>) -> Roasted {
        let mut s = fixture().session;
        s.id = format!("roast-{batch}");
        s.batch_number = Some(batch);
        let now: DateTime<Utc> = "2026-10-26T14:00:00Z".parse().unwrap();
        let cupping = scores.map(|(q, taint)| {
            let attributes: Vec<CuppingAttribute> = ATTRIBUTES
                .iter()
                .map(|(k, _, kind, _)| CuppingAttribute {
                    id: "a".into(),
                    cupping_id: "c".into(),
                    attribute_name: k.to_string(),
                    score: if *kind == Kind::Cups { 10.0 } else { q },
                    created_at: now,
                })
                .collect();
            let total = q * 7.0 + 30.0;
            let fin = total - 2.0 * taint as f32;
            CuppingWithAttributes {
                cupping: CuppingScore {
                    id: "c".into(),
                    session_id: s.id.clone(),
                    scoring_framework: "sca".into(),
                    final_score: Some(fin),
                    notes: Some("stone fruit".into()),
                    taint_cups: taint,
                    fault_cups: 0,
                    created_at: now,
                    updated_at: now,
                },
                total_score: Some(total),
                defects: 2 * taint,
                band: Some(crate::roasts::cupping::band(fin).into()),
                attributes,
            }
        });
        Roasted {
            session: s,
            coffee: "Palmichal Vecindario".into(),
            profile: Some("Light espresso".into()),
            cupping,
        }
    }

    #[test]
    fn blank_codes_are_three_digits_stable_for_the_day_and_never_twice() {
        let o = options(Units::C, Paper::Letter);
        let rs = [roast(5, None), roast(4, None)];
        let ask = Ask {
            blank: true,
            samples: 2,
            ..Ask::default()
        };
        let a = build(&rs, &ask, &o).unwrap();
        let b = build(&rs, &ask, &o).unwrap();
        let codes: Vec<&str> = a["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes.len(), 4);
        assert!(
            codes
                .iter()
                .all(|c| c.len() == 3 && c.parse::<u16>().is_ok_and(|n| (100..1000).contains(&n)))
        );
        assert_eq!(codes.iter().collect::<HashSet<_>>().len(), 4, "{codes:?}");
        assert_eq!(a["samples"], b["samples"], "the same all day");
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "in the codes' order, not the roasts'");
        let key = a["key"].as_array().unwrap();
        let k5 = key
            .iter()
            .position(|k| k["label"] == "#5 · Palmichal Vecindario · Light espresso")
            .unwrap();
        assert_eq!(
            a["samples"][k5]["code"], key[k5]["code"],
            "the key matches the form"
        );
        let own = key.iter().filter(|k| k["label"] == "a sample of your own");
        assert_eq!(own.count(), 2);
        // Roasted 25 Oct (local), made 26 Oct
        assert_eq!(a["samples"][k5]["roasted"], "25 Oct");
        assert_eq!(a["samples"][k5]["rest"], "1 day");
        assert_eq!(a["meta"]["subject"], "26 Oct 2026 · 4 samples");
        // Too many
        let many = Ask {
            blank: true,
            samples: 30,
            ..Ask::default()
        };
        assert!(matches!(build(&rs, &many, &o), Err(ReportError::Bad(_))));
        // None given: two to fill in
        let two = build(
            &[],
            &Ask {
                blank: true,
                ..Ask::default()
            },
            &o,
        )
        .unwrap();
        assert_eq!(two["samples"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_session_table_ranks_nothing_and_names_the_best() {
        let o = options(Units::C, Paper::Letter);
        let rs = [
            roast(5, Some((7.5, 1))),
            roast(4, Some((8.0, 0))),
            roast(3, None),
        ];
        let d = build(&rs, &Ask::default(), &o).unwrap();
        let rows = d["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "the roast not cupped is left out");
        assert_eq!(rows[0]["mark"], "5");
        // 7.5 × 7 + 30 = 82.5, less one taint: 80.5
        assert_eq!(
            (rows[0]["total"].as_str(), rows[0]["final"].as_str()),
            (Some("82.50"), Some("80.50"))
        );
        assert_eq!(rows[0]["cups"], "10 10 10");
        assert_eq!(rows[0]["scores"][0], "7.50");
        assert_eq!(rows[0]["scores"][6], "10");
        assert_eq!(d["session"]["best"], "#4, 86.00 (excellent)");
        assert_eq!(d["strip"]["rows"].as_array().unwrap().len(), 7);
        assert_eq!(d["strip"]["rows"][0]["dots"].as_array().unwrap().len(), 2);
        let none = build(&[roast(3, None)], &Ask::default(), &o);
        assert!(matches!(none, Err(ReportError::Bad(_))));
    }

    #[tokio::test]
    async fn the_sheets_lay_out() {
        let r = Renderer::in_process();
        let k = kind("cupping").unwrap();
        let o = options(Units::C, Paper::Letter);
        let out = std::env::var("RR_REPORTS_OUT").ok();
        let blank = [roast(5, None), roast(4, None), roast(3, None)];
        let cupped = [
            roast(5, Some((7.5, 1))),
            roast(4, Some((8.0, 0))),
            roast(3, Some((8.25, 0))),
        ];
        for (name, data, pages) in [
            (
                "cupping-blank",
                build(
                    &blank,
                    &Ask {
                        blank: true,
                        samples: 1,
                        per_page: 2,
                        ..Ask::default()
                    },
                    &o,
                )
                .unwrap(),
                3,
            ),
            (
                "cupping-grid",
                build(
                    &blank,
                    &Ask {
                        blank: true,
                        samples: 3,
                        per_page: 6,
                        ..Ask::default()
                    },
                    &o,
                )
                .unwrap(),
                2,
            ),
            (
                "cupping-filled",
                build(&cupped, &Ask::default(), &o).unwrap(),
                1,
            ),
        ] {
            let pdf = render(&r, k, data.clone(), Format::Pdf, 1, 96)
                .await
                .unwrap();
            assert_eq!(pdf.pages, Some(pages), "{name}");
            crate::reports::check::assert_clean("cupping", name, &data);
            if let Some(dir) = &out {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(format!("{dir}/{name}.pdf"), &pdf.bytes).unwrap();
                for p in 1..=pages {
                    let png = render(&r, k, data.clone(), Format::Png, p, 110)
                        .await
                        .unwrap();
                    std::fs::write(format!("{dir}/{name}-{p}.png"), &png.bytes).unwrap();
                }
            }
        }
    }
}
