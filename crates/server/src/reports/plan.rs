//! The roast-day plan sheet (`assets/reports/plan.typ`, schema `roastwarden.report.plan/1`):
//! the planned roasts in order, the green to weigh out and what to expect from it, the
//! profile's targets, and boxes to write charge, first crack, drop and the weight in; with
//! the green each coffee needs against what is in stock.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::expect;
use super::{
    DASH, Options, ReportError, clock, clock_opt, day_month, grams, long_date, meta, short_date,
};
use crate::coffee::inventory::{InventoryService, PlannedRoast};
use crate::profiles::history::ProfileHistoryRoast;
use crate::profiles::types::ProfileWithPoints;
use crate::roasts::types::{RoastEvent, RoastSession};

pub const MAX_ROASTS: usize = 40;

pub struct Planned {
    pub plan: PlannedRoast,
    pub profile: Option<ProfileWithPoints>,
    /// The coffee's last finished roast on this profile, with its events
    pub last: Option<(RoastSession, Vec<RoastEvent>)>,
}

/// What the notes of a roast say it came to, short: its "Result:" paragraph (else its last),
/// the end of it when it is long (where the conclusion is)
pub fn note_of(notes: Option<&str>) -> Option<String> {
    const MAX: usize = 200;
    let paras: Vec<&str> = notes?
        .split('\n')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let p = paras
        .iter()
        .rev()
        .find(|p| p.to_ascii_lowercase().starts_with("result"))
        .or(paras.last())?;
    let p = p
        .strip_prefix("Result:")
        .or_else(|| p.strip_prefix("Result"))
        .unwrap_or(p)
        .trim();
    if p.chars().count() <= MAX {
        return Some(p.to_string());
    }
    // Whole sentences from the end, as many as fit
    let sentences: Vec<&str> = p.split_inclusive(". ").collect();
    let mut tail = String::new();
    for s in sentences.iter().rev() {
        if tail.chars().count() + s.chars().count() > MAX {
            break;
        }
        tail.insert_str(0, s);
    }
    if tail.is_empty() {
        let cut: String = p.chars().take(MAX).collect();
        let cut = cut
            .rsplit_once(' ')
            .map_or(cut.as_str(), |c| c.0)
            .to_string();
        return Some(format!("{cut} …"));
    }
    Some(format!("… {}", tail.trim()))
}

pub struct Input {
    pub roasts: Vec<Planned>,
    pub roaster: Option<String>,
}

pub async fn gather(db: &SqlitePool, roaster: Option<&str>) -> Result<Input, ReportError> {
    let plans = InventoryService::new(db.clone())
        .list_plans(roaster, false)
        .await?;
    let mut roasts = Vec::new();
    for plan in plans.into_iter().take(MAX_ROASTS) {
        let profile = match &plan.profile_id {
            Some(p) => crate::profiles::store::get_profile_with_points(db, p).await?,
            None => None,
        };
        let last = match (&plan.bean_id, &plan.profile_id) {
            (Some(b), Some(p)) => {
                sqlx::query_as::<_, RoastSession>(
                    "SELECT * FROM roast_sessions WHERE status = 'completed' AND bean_id = ?
                     AND profile_id = ? ORDER BY COALESCE(start_time, created_at) DESC LIMIT 1",
                )
                .bind(b)
                .bind(p)
                .fetch_optional(db)
                .await?
            }
            _ => None,
        };
        let last = match last {
            Some(s) => {
                let events = crate::roasts::events::get_roast_events(db, &s.id).await?;
                Some((s, events))
            }
            None => None,
        };
        roasts.push(Planned {
            plan,
            profile,
            last,
        });
    }
    Ok(Input {
        roasts,
        roaster: roaster.map(str::to_string),
    })
}

/// BT on the profile's curve at `t` seconds, straight between its points
pub(crate) fn curve_at(p: &ProfileWithPoints, t: f64) -> Option<f64> {
    let pts: Vec<(f64, f64)> = p
        .points
        .iter()
        .map(|q| (f64::from(q.time_seconds), f64::from(q.target_temp)))
        .collect();
    let i = pts.iter().position(|q| q.0 >= t)?;
    if i == 0 {
        return Some(pts[0].1);
    }
    let (a, b) = (pts[i - 1], pts[i]);
    Some(
        a.1 + (b.1 - a.1)
            * if b.0 > a.0 {
                (t - a.0) / (b.0 - a.0)
            } else {
                0.0
            },
    )
}

pub fn build(i: &Input, o: &Options) -> Value {
    let kind = super::kind("plan").expect("the plan kind");
    let today = o.local(o.now);
    let mut need: BTreeMap<String, (f64, Option<f32>, usize)> = BTreeMap::new();
    let rows: Vec<Value> = i
        .roasts
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let p = &r.plan;
            let prof = r.profile.as_ref().map(|x| &x.profile);
            let coffee = p.bean_name.clone().unwrap_or_else(|| DASH.into());
            if let (Some(b), Some(g)) = (&p.bean_name, p.green_weight_g) {
                let e = need.entry(b.clone()).or_insert((0.0, p.bean_stock_g, 0));
                e.0 += f64::from(g);
                e.2 += 1;
            }
            // What to expect out: the level's (or the profile's own) loss range
            let ex = r.profile.as_ref().map(|x| {
                expect::expected(
                    x.profile.expectations.as_ref().map(|j| &j.0),
                    x.profile.roast_level.as_deref(),
                    None,
                    &[],
                    "",
                )
            });
            let loss = ex.as_ref().and_then(|e| e.weight_loss.clone());
            let expect_out = match (p.green_weight_g, &loss) {
                (Some(g), Some(l)) => Some(format!(
                    "about {}–{} g out (loss {}–{} %)",
                    (f64::from(g) * (1.0 - l.hi / 100.0)).round(),
                    (f64::from(g) * (1.0 - l.lo / 100.0)).round(),
                    l.lo, l.hi
                )),
                _ => None,
            };
            let fc_t = prof.and_then(|x| x.target_first_crack).map(f64::from);
            let fc_bt = r.profile.as_ref().zip(fc_t).and_then(|(x, t)| curve_at(x, t));
            let targets: Vec<Value> = [
                ("Charge", prof.and_then(|x| x.charge_temp).map(|c| o.temp_u(Some(f64::from(c)), 0))),
                ("First crack", fc_t.map(|t| match fc_bt {
                    Some(b) => format!("{} · {}", clock(t), o.temp_u(Some(b), 0)),
                    None => clock(t),
                })),
                ("Drop", match (prof.and_then(|x| x.target_total_time), prof.and_then(|x| x.target_end_temp)) {
                    (Some(t), Some(c)) => Some(format!("{} · {}", clock(f64::from(t)), o.temp_u(Some(f64::from(c)), 0))),
                    (Some(t), None) => Some(clock(f64::from(t))),
                    (None, Some(c)) => Some(o.temp_u(Some(f64::from(c)), 0)),
                    _ => None,
                }),
                ("Level", ex.as_ref().and_then(|e| e.level_label.map(str::to_string))),
            ]
            .into_iter()
            .filter_map(|(l, v)| v.map(|v| json!({ "label": l, "value": v })))
            .collect();
            // How this coffee went last time on this profile
            let last = r.last.as_ref().map(|(s, events)| {
                let n = ProfileHistoryRoast::from_roast(s, events);
                let at = |t: Option<f32>, bt: Option<f32>| match (t, bt) {
                    (Some(t), Some(b)) => format!("{} at {}", clock(f64::from(t)), o.temp_u(Some(f64::from(b)), 1)),
                    (t, _) => clock_opt(t.map(f64::from)),
                };
                json!({
                    "mark": s.batch_number.map_or_else(|| s.name.clone(), |b| format!("#{b}")),
                    "name": s.name,
                    "date": day_month(&o.local(s.start_time.unwrap_or(s.created_at))),
                    "fc": at(n.fc_s, n.fc_bt),
                    "drop": at(n.drop_s, n.drop_bt),
                    "loss": n.weight_loss_pct.map_or(DASH.into(), |l| format!("{l:.1} %")),
                    "note": note_of(s.notes.as_deref()),
                })
            });
            let first = last.is_none() && p.bean_id.is_some() && p.profile_id.is_some();
            json!({
                "last": last,
                "first": first,
                "n": k + 1,
                "title": p.name.clone().unwrap_or_else(|| coffee.clone()),
                "coffee": coffee,
                "profile": r.profile.as_ref().map(|x| x.profile.name.clone()),
                "roaster": p.device_id.clone().unwrap_or_else(|| "any roaster".into()),
                "green": p.green_weight_g.map_or(DASH.into(), |g| format!("{} g", grams(f64::from(g)))),
                "expect": expect_out,
                "targets": targets,
                "notes": p.notes,
            })
        })
        .collect();
    let weigh: Vec<Value> = need
        .iter()
        .map(|(coffee, (g, stock, n))| {
            let stock = stock.map(f64::from);
            json!({
                "coffee": coffee,
                "roasts": n,
                "green": grams(*g),
                "stock": stock.map_or(DASH.into(), grams),
                "enough": stock.is_none_or(|s| s + 0.5 >= *g),
                "short": stock.filter(|s| s + 0.5 < *g).map(|s| grams(g - s)),
            })
        })
        .collect();
    let subject = format!(
        "{}{}",
        short_date(&today),
        i.roaster
            .as_ref()
            .map_or(String::new(), |r| format!(" · {r}"))
    );
    json!({
        "schema": kind.schema,
        "meta": meta(kind, &subject, o),
        "day": long_date(&today),
        "roaster": i.roaster,
        "roasts": rows,
        "weigh": weigh,
        "notes": format!("Times from charge (mm:ss), temperatures {}. Expected output from the level's weight-loss range (or the profile's own). Write what happened in the boxes, then record it: the weight and any landmark that moved on the roast's page (or roast weigh and roast landmark).", o.units.temp_label()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Renderer;
    use crate::reports::roast::tests::{fixture, options};
    use crate::reports::{Format, Paper, Units, kind, render};

    fn planned(name: &str, g: f32, stock: f32) -> Planned {
        let now = chrono::Utc::now();
        Planned {
            plan: PlannedRoast {
                id: name.into(),
                position: 1.0,
                device_id: Some("esp32_roaster_01".into()),
                bean_id: Some("pal".into()),
                profile_id: Some("fluidbed-espresso-short".into()),
                green_weight_g: Some(g),
                name: Some(name.into()),
                notes: None,
                session_id: None,
                created_at: now,
                updated_at: now,
                bean_name: Some("Palmichal Vecindario".into()),
                bean_stock_g: Some(stock),
                profile_name: Some("Light espresso".into()),
                batch_number: None,
            },
            profile: fixture().profile,
            last: None,
        }
    }

    #[test]
    fn a_roasts_note_comes_to_its_result() {
        assert_eq!(note_of(None), None);
        assert_eq!(note_of(Some("  ")), None);
        assert_eq!(
            note_of(Some("Plan: one change.\nResult: softer flick. Keep 45 %.")),
            Some("softer flick. Keep 45 %.".into())
        );
        let long = format!(
            "Plan.\nResult: {} The heater opened at 55 %. Keep 45 %.",
            "Charge 25.9 °C, the same as before in every way that matters here. ".repeat(4)
        );
        let n = note_of(Some(&long)).unwrap();
        assert!(n.starts_with("… ") && n.ends_with("Keep 45 %."), "{n}");
        assert!(n.chars().count() <= 202, "{n}");
        assert_eq!(note_of(Some("Just notes")), Some("Just notes".into()));
    }

    #[tokio::test]
    async fn a_roast_day_with_its_targets_and_the_green_to_weigh() {
        let o = options(Units::C, Paper::Letter);
        // The first planned roast's coffee was roasted on its profile before: batch #5
        let mut once = planned("Espresso 1", 100.0, 150.0);
        let f = fixture();
        let mut s = f.session.clone();
        s.notes = Some("Same as #4.\nResult: FC on time, drop 10:05. Keep it.".into());
        once.last = Some((s, f.events.clone()));
        let i = Input {
            roasts: vec![once, planned("Espresso 2", 100.0, 150.0)],
            roaster: Some("esp32_roaster_01".into()),
        };
        let d = build(&i, &o);
        let first = &d["roasts"][0];
        let last = &first["last"];
        assert_eq!(last["mark"], "#5");
        assert_eq!(last["date"], "25 Oct");
        assert_eq!(last["loss"], "14.3 %");
        assert!(
            last["fc"].as_str().unwrap().starts_with("7:29 at "),
            "{last}"
        );
        assert_eq!(last["note"], "FC on time, drop 10:05. Keep it.");
        assert_eq!(first["first"], false);
        assert_eq!(d["roasts"][1]["first"], true, "never roasted on it");
        assert_eq!(first["green"], "100 g");
        // Light: 14–16 % loss
        assert_eq!(first["expect"], "about 84–86 g out (loss 14–16 %)");
        let targets = first["targets"].as_array().unwrap();
        assert!(
            targets
                .iter()
                .any(|t| t["label"] == "Drop" && t["value"] == "10:05 · 210 °C"),
            "{targets:?}"
        );
        assert!(targets.iter().any(|t| t["label"] == "First crack"));
        assert_eq!(d["weigh"][0]["green"], "200");
        assert_eq!(d["weigh"][0]["enough"], false);
        assert_eq!(d["weigh"][0]["short"], "50");
        let r = Renderer::in_process();
        let pdf = render(&r, kind("plan").unwrap(), d.clone(), Format::Pdf, 1, 96)
            .await
            .unwrap();
        assert_eq!(pdf.pages, Some(1));
        crate::reports::check::assert_clean("plan", "plan", &d);
        if let Ok(dir) = std::env::var("RR_REPORTS_OUT") {
            std::fs::create_dir_all(&dir).unwrap();
            let png = render(&r, kind("plan").unwrap(), d, Format::Png, 1, 110)
                .await
                .unwrap();
            std::fs::write(format!("{dir}/plan-1.png"), &png.bytes).unwrap();
        }
    }
}
