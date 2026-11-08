//! A new bean's first roast, and the next one (`docs/new-bean.md`).
//!
//! Before the first roast of a bean on a roaster: what to expect (B1–B6). After a roast,
//! while the bean has fewer than three on its profile: what to change for the next one, in
//! seconds or °C (A1–A7). The lines come from this roaster's own history and the roast's
//! analysis (`analysis.rs`), widened for what the bean is; the supplier is quoted, never
//! judged. Nothing here changes a roast, a profile or a plan.
//!
//! The rules are pure functions ([`before`], [`next`]) over plain inputs; [`guide`] gathers
//! them from the database for the one endpoint, `GET /api/beans/{id}/guide`.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::{IntoParams, ToSchema};

use crate::analysis::{self, RoastAnalysis};
use crate::coffee::inventory::{Bean, InventoryService};
use crate::predict::live::FcHistory;
use crate::profiles::types::{ProfileWithPoints, RoastProfile};
use crate::replay::auto_drop::{trigger_value, DropBy};
use crate::reports::{self, ReportError};
use crate::roasts::types::SessionStatus;

/// The first crack window for a washed coffee, ± °C. This roaster's own data set it: the
/// Kenya's first roast cracked 2.9 °C under the Light espresso profile's mean of eight
/// roasts of two other coffees (research § 4, derived).
const WINDOW_C: f64 = 3.0;
/// For a natural, honey, wet-hulled or unknown process: one more degree (Yergenson's
/// spread across coffees, SD 16–31 s ≈ 2–4 °C at this roaster's 8 °C/min; research § 2).
const WINDOW_WIDE_C: f64 = 4.0;
/// From this many finished roasts of the bean on its profile the profile's history reads
/// them (`roast-guidance.md`), and the guide stops.
const SETTLED: i64 = 3;
/// A development this close to the profile's needs no move, s.
const DEV_SLACK_S: f64 = 5.0;
/// Development changes in steps of this, s (research rule 9).
const STEP_S: f64 = 15.0;

// ------------------------------------------------------------------- the bean --

/// A coffee's process, as these rules read the supplier's free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Process {
    Washed,
    Natural,
    Honey,
    WetHulled,
    Unknown,
}

impl Process {
    pub fn read(s: Option<&str>) -> Process {
        let Some(s) = s.map(str::to_lowercase) else {
            return Process::Unknown;
        };
        let has = |w: &[&str]| w.iter().any(|w| s.contains(w));
        if has(&["wet hull", "wet-hull", "giling"]) {
            Process::WetHulled
        } else if has(&["honey", "pulped natural", "semi-washed", "semi washed"]) {
            Process::Honey
        } else if has(&["natural", "dry"]) {
            Process::Natural
        } else if has(&["washed", "wet process", "wet-process"]) {
            Process::Washed
        } else {
            Process::Unknown
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Process::Washed => "washed",
            Process::Natural => "natural",
            Process::Honey => "honey",
            Process::WetHulled => "wet-hulled",
            Process::Unknown => "unknown",
        }
    }

    /// Naturals, honeys and wet-hulled coffees: a quieter first crack, a wider window.
    fn uneven(self) -> bool {
        !matches!(self, Process::Washed)
    }
}

/// What the rules read from the bean's record: nothing new to enter.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct BeanReads {
    /// `washed`, `natural`, `honey`, `wet-hulled`, `unknown`
    pub process: &'static str,
    /// The supplier's words
    pub process_as_given: Option<String>,
    pub peaberry: bool,
    pub supplier: Option<String>,
    /// The supplier's roast level, quoted: "City to Full City+"
    pub level: Option<String>,
    /// The supplier on espresso, quoted, or `yes` / `not recommended`
    pub espresso: Option<String>,
    pub density_g_l: Option<f64>,
    pub moisture_pct: Option<f64>,
}

impl BeanReads {
    pub fn of(b: &Bean) -> BeanReads {
        let detail = |k: &str| {
            b.details
                .as_ref()
                .and_then(|d| d.0.get(k).cloned())
                .filter(|v| !v.trim().is_empty())
        };
        let peaberry = detail("Bean form").is_some_and(|f| f.to_lowercase().contains("peaberry"))
            || b.name.to_lowercase().contains("peaberry");
        // "City to Full City+. Good for espresso.": the level is the first sentence.
        let rec = b
            .roast_recommendation
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty());
        let level = rec.map(|r| {
            sentences(r)
                .next()
                .unwrap_or(r)
                .trim_end_matches('.')
                .to_string()
        });
        let said = [rec, b.cupping_notes.as_deref()]
            .into_iter()
            .flatten()
            .flat_map(sentences)
            .find(|s| s.to_lowercase().contains("espresso"))
            .map(|s| format!("“{}”", s.trim()));
        let flag = detail("Recommended espresso")
            .or_else(|| detail("Reference Recommended espresso"))
            .map(|v| match v.trim().to_lowercase().as_str() {
                "yes" | "true" => "yes".to_string(),
                "no" | "false" => "not recommended".to_string(),
                _ => format!("“{}”", v.trim()),
            });
        // "not recommended" outranks a sentence that only mentions espresso.
        let espresso = match (flag, said) {
            (Some(f), _) if f == "not recommended" => Some(f),
            (_, Some(s)) => Some(s),
            (f, None) => f,
        };
        BeanReads {
            process: Process::read(b.process.as_deref()).as_str(),
            process_as_given: b.process.clone(),
            peaberry,
            supplier: b.supplier.clone().filter(|s| !s.trim().is_empty()),
            level,
            espresso,
            density_g_l: b.density_g_l.map(f64::from),
            moisture_pct: b.moisture_pct.map(f64::from),
        }
    }

    fn process(&self) -> Process {
        match self.process {
            "washed" => Process::Washed,
            "natural" => Process::Natural,
            "honey" => Process::Honey,
            "wet-hulled" => Process::WetHulled,
            _ => Process::Unknown,
        }
    }
}

/// Sentences of a supplier's text (a full stop, ! or ? and a space).
fn sentences(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || {
        let r = rest.trim_start();
        if r.is_empty() {
            return None;
        }
        let end = r
            .char_indices()
            .find(|(i, c)| {
                matches!(c, '.' | '!' | '?')
                    && r[i + c.len_utf8()..].starts_with(char::is_whitespace)
            })
            .map_or(r.len(), |(i, c)| i + c.len_utf8());
        rest = &r[end..];
        Some(&r[..end])
    })
}

// ----------------------------------------------------------------- the lines --

/// One line of the guide.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct GuideLine {
    /// The rule: `b1`…`b6` before the first roast, `a1`…`a6` after one (`docs/new-bean.md`)
    pub key: &'static str,
    pub text: String,
    /// Where it comes from and its grade: "research § 2, rule 2 (a, derived)"
    pub source: &'static str,
    /// `line`, or `hint` (context, never amber)
    pub kind: &'static str,
    /// What the line offers to do, the owner's choice: `auto_drop_development` (the profile
    /// edit that drops by development)
    pub action: Option<&'static str>,
}

fn line(key: &'static str, text: String, source: &'static str) -> GuideLine {
    GuideLine {
        key,
        text,
        source,
        kind: "line",
        action: None,
    }
}

/// Where first crack is expected on this roaster.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct FcExpected {
    /// BT, °C
    pub bt: f64,
    /// ± °C
    pub window_c: f64,
    /// When the profile's curve reaches it, s from CHARGE
    pub t: Option<f64>,
    /// ± s at the profile's rise there
    pub window_s: Option<f64>,
    /// "8 roasts on Fluid bed — Light espresso"
    pub basis: String,
}

fn clock(s: f64) -> String {
    let r = s.round() as i64;
    let sign = if r < 0 { "−" } else { "" };
    format!("{sign}{}:{:02}", r.abs() / 60, r.abs() % 60)
}

/// "+15 s", "−20 s"
fn secs(v: f64) -> String {
    let r = v.round() as i64;
    if r < 0 {
        format!("−{} s", -r)
    } else {
        format!("+{r} s")
    }
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// When a profile's BT curve first reaches `bt`, and its rise there (°C/s).
fn curve_reaches(curve: &[(f64, f64)], bt: f64) -> Option<(f64, f64)> {
    curve.windows(2).find_map(|w| {
        let ((t0, b0), (t1, b1)) = (w[0], w[1]);
        (b0 < bt && b1 >= bt && t1 > t0).then(|| {
            let t = t0 + (bt - b0) / (b1 - b0) * (t1 - t0);
            (t, (b1 - b0) / (t1 - t0))
        })
    })
}

/// What the rules before a first roast read.
pub struct Before<'a> {
    pub reads: &'a BeanReads,
    pub profile: Option<&'a ProfileWithPoints>,
    /// This roaster's first crack BT (the prediction's tiers), else the profile's own
    pub fc: Option<FcHistory>,
}

/// The lines before the first roast of a bean (B1–B6), and where first crack is expected.
pub fn before(b: &Before) -> (Vec<GuideLine>, Option<FcExpected>) {
    let process = b.reads.process();
    let p = b.profile.map(|p| &p.profile);
    let mut out = Vec::new();

    // B1
    out.push(line(
        "b1",
        match p {
            Some(p) => match p.roast_purpose.as_deref() {
                Some(purpose @ ("espresso" | "filter" | "omni")) => format!(
                    "Start from your {purpose} profile ({}); the bean's process doesn't change the start.",
                    p.name
                ),
                _ => format!(
                    "Start from {}; the bean's process doesn't change the start.",
                    p.name
                ),
            },
            None => "Start from your usual profile for what this roast is for; the bean's process doesn't change the start.".into(),
        },
        "research rule 1 (b)",
    ));

    // B2
    let window_c = if process.uneven() {
        WINDOW_WIDE_C
    } else {
        WINDOW_C
    };
    let fc = b.fc.as_ref().map(|h| {
        let at = b.profile.and_then(|p| {
            let curve = crate::replay::engine::Plan::from_profile(p).bt;
            curve_reaches(&curve, h.bt_mean)
        });
        FcExpected {
            bt: r1(h.bt_mean),
            window_c,
            t: at.map(|(t, _)| r1(t)),
            window_s: at
                .filter(|(_, rise)| *rise > 0.01)
                .map(|(_, rise)| (window_c / rise / 5.0).round() * 5.0),
            basis: h.basis.clone(),
        }
    });
    if let Some(f) = &fc {
        let when = match (f.t, p) {
            (Some(t), Some(p)) => format!(", about {} on {}", clock(t), p.name),
            _ => String::new(),
        };
        let spread = match f.window_s {
            Some(s) => format!("± {window_c:.0} °C (≈ ± {s:.0} s)"),
            None => format!("± {window_c:.0} °C"),
        };
        let why = if process.uneven() {
            format!(
                "; wider for {}: no source says which way it moves",
                match process {
                    Process::Natural => "a natural",
                    Process::Honey => "a honey process",
                    Process::WetHulled => "a wet-hulled coffee",
                    _ => "a process not known",
                }
            )
        } else {
            String::new()
        };
        out.push(line(
            "b2",
            format!(
                "First crack expected near {:.1} °C ({}){when}; {spread}{why}.",
                f.bt, f.basis
            ),
            "research § 2, § 4, rule 2 (a, derived)",
        ));
    }

    // B3
    if matches!(
        process,
        Process::Natural | Process::Honey | Process::WetHulled
    ) {
        out.push(line(
            "b3",
            "Naturals often crack quietly, with stray pops first: mark first crack at the first run of pops.".into(),
            "research § 3 (b)",
        ));
    }

    // B4
    if let Some(p) = p.filter(|p| p.auto_drop.as_deref() == Some("time")) {
        let off = fc
            .as_ref()
            .and_then(|f| f.window_s)
            .map_or("early or late".to_string(), |s| format!("{s:.0} s off"));
        let can = trigger_value(
            DropBy::Development,
            p.target_total_time.map(f64::from),
            p.target_first_crack.map(f64::from),
            None,
        )
        .is_ok();
        out.push(GuideLine {
            key: "b4",
            text: format!(
                "This profile drops at {} whatever first crack does; a first crack {off} changes development as much. Drop by development for a first roast?",
                p.target_total_time
                    .map_or("its drop time".to_string(), |t| clock(f64::from(t)))
            ),
            source: "research § 5 (a)",
            kind: "line",
            action: can.then_some("auto_drop_development"),
        });
    }

    // B5
    let low_density = b.reads.density_g_l.is_some_and(|d| d < 650.0);
    let wet = b.reads.moisture_pct.is_some_and(|m| m > 12.0);
    if process == Process::WetHulled || low_density || wet {
        out.push(line(
            "b5",
            "Low density or wet: expect slower drying, about +30 s to first crack and more weight loss (+1–2 points at the same colour). A gentler start is an option, not tested here.".into(),
            "research § 6 (a, b)",
        ));
    }

    // B6
    let espresso_plan = p.is_none_or(|p| p.roast_purpose.as_deref() == Some("espresso"));
    let espresso = b.reads.espresso.as_ref().filter(|_| espresso_plan);
    if b.reads.level.is_some() || espresso.is_some() {
        let who = b.reads.supplier.as_deref().unwrap_or("The supplier");
        let mut parts = Vec::new();
        if let Some(l) = &b.reads.level {
            parts.push(l.clone());
        }
        if let Some(e) = espresso {
            parts.push(format!("for espresso {e}"));
        }
        let said = parts.join("; ");
        // A quote ends the line itself: no full stop after it.
        let stop = if said.ends_with('”') { "" } else { "." };
        out.push(line(
            "b6",
            format!("{who}: {said}{stop}"),
            "research § 7 (b), quoted",
        ));
    }
    (out, fc)
}

/// How a roast's DROP was marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DroppedBy {
    /// By hand
    Hand,
    /// Auto DROP, by time, BT or development
    Auto(DropBy),
}

impl DroppedBy {
    /// From the DROP mark's note: auto DROP writes "marked by auto DROP (…)".
    pub fn read(note: Option<&str>) -> DroppedBy {
        match note {
            Some(n) if n.starts_with("marked by auto DROP") => {
                DroppedBy::Auto(if n.contains("after first crack") {
                    DropBy::Development
                } else if n.contains("drop BT") {
                    DropBy::Bt
                } else {
                    DropBy::Time
                })
            }
            _ => DroppedBy::Hand,
        }
    }
}

/// What the rules after a roast read.
pub struct After<'a> {
    pub reads: &'a BeanReads,
    pub roast: &'a RoastAnalysis,
    pub profile: Option<&'a RoastProfile>,
    pub dropped_by: DroppedBy,
}

/// "If it tastes…" (A7): static, one change at a time.
pub const TASTING: &str = "If it tastes sour or grassy: +15–30 s development or a higher drop. Flat or bready: shorter development. Tipping (dark tips): a gentler start. One change at a time.";

/// Development needed to reach `[lo, hi]` from `v`, in steps of [`STEP_S`].
fn to_band(v: f64, lo: f64, hi: f64) -> Option<f64> {
    let need = if v < lo {
        lo - v
    } else if v > hi {
        hi - v
    } else {
        return None;
    };
    Some((need.abs() / STEP_S).ceil().max(1.0) * STEP_S * need.signum())
}

/// The lines for the next roast of the bean (A1–A6).
pub fn next(a: &After) -> Vec<GuideLine> {
    let r = a.roast;
    let process = a.reads.process();
    let mut out = Vec::new();
    let fc = r.marks.iter().find(|m| m.key == "first_crack_start");
    let profile_fc = a.profile.and_then(|p| p.target_first_crack).map(f64::from);
    let profile_drop = a.profile.and_then(|p| p.target_total_time).map(f64::from);

    // A1
    if let Some(m) = fc {
        let bt =
            m.bt.map_or("its mark".to_string(), |b| format!("{b:.1} °C"));
        let against = match profile_fc {
            Some(pf) => format!(", {} against the profile's {}", secs(m.t - pf), clock(pf)),
            None => format!(" at {}", clock(m.t)),
        };
        out.push(line(
            "a1",
            format!(
                "First crack came at {bt}{against}; the next roast of this bean expects it there."
            ),
            "research § 2, rule 8 (a, derived)",
        ));
    }

    // A2 / A3
    match (a.dropped_by, r.development_s) {
        (DroppedBy::Auto(DropBy::Development), Some(dev)) => out.push(line(
            "a3",
            format!(
                "Dropped {} after first crack as planned: nothing to move.",
                clock(dev)
            ),
            "rule 7",
        )),
        (_, Some(dev)) => {
            if let (Some(pf), Some(pd)) =
                (profile_fc, profile_drop.filter(|d| Some(*d) > profile_fc))
            {
                let want = pd - pf;
                let diff = dev - want;
                let what = if diff.abs() < DEV_SLACK_S {
                    "nothing to move".to_string()
                } else {
                    format!("move the drop {} to keep it", secs(-diff))
                };
                out.push(line(
                    "a2",
                    format!(
                        "Development {} ({} against the profile's {}): {what}.",
                        clock(dev),
                        secs(diff),
                        clock(want)
                    ),
                    "rule 7 (a, derived)",
                ));
            }
        }
        _ => {}
    }

    // A4 (and A6 on the weight-loss line)
    let judged = |k: &str| {
        r.expected
            .iter()
            .find(|j| j.key == k && matches!(j.verdict, Some("below" | "above")))
    };
    let range = |j: &analysis::Judged| match j.source {
        "history" => "usual",
        s => s,
    };
    let moved = out
        .iter()
        .any(|l| l.key == "a2" && l.text.contains("move the drop"));
    let dev_line = judged("development_s").and_then(|j| {
        let v = j.value?;
        let change = to_band(v, j.lo, j.hi)?;
        // One change at a time: a move to keep the profile's development (A2) comes first.
        let change = if moved {
            String::new()
        } else {
            format!(": change development by {}", secs(change))
        };
        Some(line(
            "a4",
            format!(
                "Development {} is {} the {} range {}–{}{change}.",
                clock(v),
                j.verdict.unwrap_or(""),
                range(j),
                clock(j.lo),
                clock(j.hi),
            ),
            "rule 9 (b), roast-guidance.md",
        ))
    });
    let wet_hulled = process == Process::WetHulled;
    let wet_note = "; wet-hulled beans lose 1–2 points more at the same colour";
    let loss_line = judged("weight_loss_pct").and_then(|j| {
        let v = j.value?;
        // One change at a time: development's own line, or A2's move, leads.
        let change = if dev_line.is_some() || moved {
            String::new()
        } else if v < j.lo {
            format!(": change development by {}", secs(STEP_S))
        } else {
            format!(": change development by {}", secs(-STEP_S))
        };
        Some(line(
            "a4",
            format!(
                "Weight loss {v:.1} % is {} the {} range {:.1}–{:.1} %{change}{}.",
                j.verdict.unwrap_or(""),
                range(j),
                j.lo,
                j.hi,
                if wet_hulled { wet_note } else { "" }
            ),
            "rule 9 (b), roast-guidance.md",
        ))
    });
    let had_loss_line = loss_line.is_some();
    out.extend(dev_line);
    out.extend(loss_line);
    if wet_hulled && !had_loss_line {
        if let Some(loss) = r.loss_pct {
            out.push(line(
                "a6",
                format!("Weight loss {loss:.1} %{wet_note}."),
                "rule 11 (derived)",
            ));
        }
    }

    // A5
    if let Some(m) = fc.filter(|_| r.dropped && r.total_s > 0.0) {
        let pct = m.t / r.total_s * 100.0;
        if pct > 80.0 {
            out.push(GuideLine {
                key: "a5",
                text: format!(
                    "First crack at {pct:.0} % of the roast: late first crack usually means short development."
                ),
                source: "rule 10 (b)",
                kind: "hint",
                action: None,
            });
        }
    }
    out
}

// --------------------------------------------------------------- the endpoint --

/// A roast the guide read.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct GuideRoast {
    pub session_id: String,
    pub name: String,
    pub batch_number: Option<i32>,
    pub roasted_at: String,
}

/// A bean's guide: before its first roast on a roaster, or for the next one after a roast.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct BeanGuide {
    pub bean_id: String,
    pub bean: String,
    /// `before`: no finished roast of the bean on the roaster; `next`: after `after`, for
    /// the next roast; `settled`: three or more on the profile, whose own history reads
    /// them now (`roast review`)
    pub stage: &'static str,
    /// The roaster's device id
    pub roaster: Option<String>,
    pub profile_id: Option<String>,
    pub profile: Option<String>,
    /// The roast the `next` lines read
    pub after: Option<GuideRoast>,
    pub reads: BeanReads,
    /// The bean's finished roasts on the roaster
    pub roasts: i64,
    /// On the profile, up to and with the roast `after` when there is one
    pub roasts_on_profile: i64,
    /// Before a first roast
    pub first_crack: Option<FcExpected>,
    pub lines: Vec<GuideLine>,
    /// After a roast: "If it tastes…" (A7)
    pub tasting: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GuideQuery {
    /// The profile's id (default: the bean's next planned roast's, or the roast `after`'s)
    pub profile: Option<String>,
    /// The roaster's device id (default: the plan's, the bean's last roast's, else the last
    /// roaster used)
    pub roaster: Option<String>,
    /// For the next roast after this one (a roast of the bean; default its newest)
    pub after: Option<String>,
}

/// Finished roasts of a bean on a roaster (and a profile), up to and with roast `until`.
async fn count(
    db: &SqlitePool,
    bean: &str,
    roaster: &str,
    profile: Option<&str>,
    until: Option<&str>,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM roast_sessions
         WHERE bean_id = ? AND device_id = ? AND status = ? AND (? IS NULL OR profile_id = ?)
           AND (? IS NULL OR start_time <= (SELECT start_time FROM roast_sessions WHERE id = ?))",
    )
    .bind(bean)
    .bind(roaster)
    .bind(SessionStatus::Completed.to_string())
    .bind(profile)
    .bind(profile)
    .bind(until)
    .bind(until)
    .fetch_one(db)
    .await?)
}

/// Gather the guide for a bean.
pub async fn guide(
    db: &SqlitePool,
    bean_id: &str,
    q: &GuideQuery,
) -> Result<BeanGuide, ReportError> {
    let inv = InventoryService::new(db.clone());
    let Some(bean) = inv.bean(bean_id).await? else {
        return Err(ReportError::NotFound(format!("no bean {bean_id}")));
    };
    let reads = BeanReads::of(&bean);
    let completed = SessionStatus::Completed.to_string();

    // The roast after which, when asked for
    let after: Option<(String, String, Option<String>)> = match &q.after {
        Some(id) => {
            let row: Option<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
                "SELECT id, bean_id, device_id, profile_id FROM roast_sessions WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(db)
            .await
            .map_err(anyhow::Error::from)?;
            match row {
                None => return Err(ReportError::NotFound(format!("no roast {id}"))),
                Some((_, b, _, _)) if b.as_deref() != Some(bean_id) => {
                    return Err(ReportError::Bad(format!(
                        "roast {id} is not of {}",
                        bean.name
                    )))
                }
                Some((id, _, dev, prof)) => Some((id, dev, prof)),
            }
        }
        None => None,
    };

    // The plan for this bean, next in line
    let plan: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT device_id, profile_id FROM planned_roasts
         WHERE bean_id = ? AND session_id IS NULL AND (? IS NULL OR device_id = ?)
         ORDER BY position, created_at LIMIT 1",
    )
    .bind(bean_id)
    .bind(q.roaster.as_deref())
    .bind(q.roaster.as_deref())
    .fetch_optional(db)
    .await
    .map_err(anyhow::Error::from)?;

    let roaster: Option<String> = match (&q.roaster, &after, &plan) {
        (Some(r), _, _) => Some(r.clone()),
        (None, Some((_, dev, _)), _) | (None, None, Some((dev, _))) => Some(dev.clone()),
        _ => {
            let own: Option<String> = sqlx::query_scalar(
                "SELECT device_id FROM roast_sessions WHERE bean_id = ? AND status = ?
                 ORDER BY start_time DESC LIMIT 1",
            )
            .bind(bean_id)
            .bind(&completed)
            .fetch_optional(db)
            .await
            .map_err(anyhow::Error::from)?;
            match own {
                Some(d) => Some(d),
                None => sqlx::query_scalar(
                    "SELECT device_id FROM roast_sessions WHERE status = ?
                     ORDER BY start_time DESC LIMIT 1",
                )
                .bind(&completed)
                .fetch_optional(db)
                .await
                .map_err(anyhow::Error::from)?,
            }
        }
    };

    let roasts = match &roaster {
        Some(r) => count(db, bean_id, r, None, None).await?,
        None => 0,
    };

    // The roast the next lines read: asked for, else the bean's newest here
    let after = match (after, &roaster) {
        (Some((id, _, prof)), _) => Some((id, prof)),
        (None, Some(r)) if roasts > 0 => {
            let row: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT id, profile_id FROM roast_sessions
                 WHERE bean_id = ? AND device_id = ? AND status = ?
                 ORDER BY start_time DESC LIMIT 1",
            )
            .bind(bean_id)
            .bind(r)
            .bind(&completed)
            .fetch_optional(db)
            .await
            .map_err(anyhow::Error::from)?;
            row
        }
        _ => None,
    };

    // After a roast, its profile; before the first, the one asked for or the plan's
    let profile_id = match (&after, &q.profile) {
        (Some((_, prof)), _) => prof.clone(),
        (None, Some(p)) => Some(p.clone()),
        (None, None) => plan.as_ref().and_then(|(_, p)| p.clone()),
    };
    let profile = match &profile_id {
        Some(p) => match crate::profiles::store::get_profile_with_points(db, p).await? {
            Some(p) => Some(p),
            None => return Err(ReportError::NotFound(format!("no profile {p}"))),
        },
        None => None,
    };
    // As of the roast read: History → an old roast shows what was to change then
    let until = after.as_ref().map(|(id, _)| id.as_str());
    let roasts_on_profile = match (&roaster, &profile_id) {
        (Some(r), Some(p)) => count(db, bean_id, r, Some(p), until).await?,
        _ => 0,
    };

    let mut g = BeanGuide {
        bean_id: bean.id.clone(),
        bean: bean.name.clone(),
        stage: "before",
        roaster: roaster.clone(),
        profile_id: profile_id.clone(),
        profile: profile.as_ref().map(|p| p.profile.name.clone()),
        after: None,
        reads,
        roasts,
        roasts_on_profile,
        first_crack: None,
        lines: vec![],
        tasting: None,
    };

    match after {
        None => {
            let fc = match &roaster {
                Some(r) => {
                    crate::predict::fc_history(db, r, None, Some(bean_id), profile_id.as_deref())
                        .await?
                }
                None => None,
            }
            .or_else(|| profile.as_ref().and_then(crate::predict::profile_fc));
            let (lines, fc) = before(&Before {
                reads: &g.reads,
                profile: profile.as_ref(),
                fc,
            });
            g.lines = lines;
            g.first_crack = fc;
        }
        Some((id, _)) => {
            let input = reports::roast::gather(db, &id).await?;
            let r = analysis::analyse(&input).analysis;
            g.after = Some(GuideRoast {
                session_id: r.session_id.clone(),
                name: r.name.clone(),
                batch_number: r.batch_number,
                roasted_at: r.roasted_at.clone(),
            });
            if roasts_on_profile >= SETTLED {
                g.stage = "settled";
                g.lines = vec![line(
                    "settled",
                    format!(
                        "{roasts_on_profile} roasts of {} on {}: the profile's own history reads them now (roast review).",
                        bean.name,
                        g.profile.as_deref().unwrap_or("this profile")
                    ),
                    "roast-guidance.md",
                )];
            } else {
                g.stage = "next";
                let dropped_by = DroppedBy::read(
                    input
                        .events
                        .iter()
                        .find(|e| e.event_type == crate::roasts::types::RoastEventType::Drop)
                        .and_then(|e| e.notes.as_deref()),
                );
                g.lines = next(&After {
                    reads: &g.reads,
                    roast: &r,
                    profile: input.profile.as_ref().map(|p| &p.profile),
                    dropped_by,
                });
                g.tasting = Some(TASTING.to_string());
            }
        }
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Judged, Mark};

    fn bean(name: &str, process: Option<&str>) -> Bean {
        let at = chrono::Utc::now();
        Bean {
            id: "b1".into(),
            name: name.into(),
            origin: None,
            variety: None,
            process: process.map(str::to_string),
            lot: None,
            supplier: Some("Sweet Maria's".into()),
            notes: None,
            moisture_pct: None,
            density_g_l: None,
            low_stock_g: 0.0,
            archived: false,
            region: None,
            producer: None,
            altitude: None,
            grade: None,
            screen_size: None,
            certifications: None,
            drying: None,
            score: None,
            roast_recommendation: None,
            cupping_notes: None,
            url: None,
            sku: None,
            details: None,
            label_design: None,
            label_params: None,
            label_brand: None,
            label_notes: None,
            short_name: None,
            label_notes_suggested: None,
            created_at: at,
            updated_at: at,
            stock_g: 0.0,
            roast_count: 0,
            low: false,
            on_order_g: 0.0,
            cost_per_kg: None,
            stock_value: None,
            currency: None,
        }
    }

    fn profile(auto_drop: Option<&str>) -> ProfileWithPoints {
        let points = [
            (0, 30.0),
            (60, 100.0),
            (420, 190.0),
            (480, 198.0),
            (605, 210.0),
        ]
        .iter()
        .map(|(t, c)| {
            serde_json::from_value(serde_json::json!({
                "id": format!("p{t}"), "profile_id": "lightesp", "time_seconds": t,
                "target_temp": c, "created_at": "2026-10-29T00:00:00Z"
            }))
            .unwrap()
        })
        .collect();
        let profile: RoastProfile = serde_json::from_value(serde_json::json!({
            "id": "lightesp", "name": "Fluid bed — Light espresso",
            "created_at": "2026-10-29T00:00:00Z", "updated_at": "2026-10-29T00:00:00Z",
            "is_public": false, "target_total_time": 605, "target_first_crack": 470,
            "target_end_temp": 210.0, "roast_level": "light", "roast_purpose": "espresso",
            "auto_drop": auto_drop
        }))
        .unwrap();
        ProfileWithPoints {
            profile,
            points,
            events: vec![],
            curves: None,
        }
    }

    fn history(bt: f64) -> FcHistory {
        FcHistory {
            bt_mean: bt,
            bt_sd: 1.6,
            roasts: 8,
            basis: "8 roasts on Fluid bed — Light espresso".into(),
        }
    }

    fn keys(l: &[GuideLine]) -> Vec<&str> {
        l.iter().map(|l| l.key).collect()
    }

    #[test]
    fn process_reads_the_suppliers_words() {
        let r = |s: &str| Process::read(Some(s));
        assert_eq!(r("Dry Process (Natural)"), Process::Natural);
        assert_eq!(r("Dry"), Process::Natural);
        assert_eq!(r("Wet Hulled"), Process::WetHulled);
        assert_eq!(r("Giling Basah"), Process::WetHulled);
        assert_eq!(r("Honey"), Process::Honey);
        assert_eq!(r("Pulped Natural"), Process::Honey);
        assert_eq!(r("Washed"), Process::Washed);
        assert_eq!(r("Fully washed"), Process::Washed);
        assert_eq!(r("Anaerobic"), Process::Unknown);
        assert_eq!(Process::read(None), Process::Unknown);
    }

    #[test]
    fn reads_quote_the_supplier() {
        let mut b = bean("Rwanda Kivu Kageyo Peaberry", Some("Washed"));
        b.roast_recommendation = Some("City to Full City+. Good for espresso.".into());
        let r = BeanReads::of(&b);
        assert!(r.peaberry);
        assert_eq!(r.level.as_deref(), Some("City to Full City+"));
        assert_eq!(r.espresso.as_deref(), Some("“Good for espresso.”"));

        let mut b = bean("Finca Rosma", Some("Dry"));
        b.roast_recommendation = Some("City to Full City".into());
        b.details = Some(sqlx::types::Json(
            [(
                "Reference Recommended espresso".to_string(),
                "No".to_string(),
            )]
            .into(),
        ));
        b.cupping_notes = Some("Juicy. Some say espresso. Fine.".into());
        let r = BeanReads::of(&b);
        assert!(!r.peaberry);
        assert_eq!(r.espresso.as_deref(), Some("not recommended"));

        let mut b = bean("Bali", Some("Wet Hulled"));
        b.details = Some(sqlx::types::Json(
            [("Recommended espresso".to_string(), "Yes".to_string())].into(),
        ));
        assert_eq!(BeanReads::of(&b).espresso.as_deref(), Some("yes"));
    }

    #[test]
    fn a_washed_bean_on_a_profile_by_development() {
        let mut b = bean("Kenya", Some("Washed"));
        b.roast_recommendation = Some("City to Full City+".into());
        let reads = BeanReads::of(&b);
        let p = profile(Some("development"));
        let (lines, fc) = before(&Before {
            reads: &reads,
            profile: Some(&p),
            fc: Some(history(195.9)),
        });
        assert_eq!(keys(&lines), ["b1", "b2", "b6"]);
        assert!(lines[0]
            .text
            .starts_with("Start from your espresso profile (Fluid bed — Light espresso)"));
        let fc = fc.unwrap();
        assert_eq!(fc.window_c, 3.0);
        // 190 → 198 over 420–480 s: 195.9 at 464 s, 0.133 °C/s → ± 22.5 s, to 5 s
        assert!((fc.t.unwrap() - 464.3).abs() < 0.2, "{fc:?}");
        assert_eq!(fc.window_s, Some(25.0));
        assert_eq!(
            lines[1].text,
            "First crack expected near 195.9 °C (8 roasts on Fluid bed — Light espresso), about 7:44 on Fluid bed — Light espresso; ± 3 °C (≈ ± 25 s)."
        );
        assert_eq!(lines[2].text, "Sweet Maria's: City to Full City+.");
    }

    #[test]
    fn a_natural_on_a_profile_by_time() {
        let mut b = bean("Burundi Dry Process Kibingo", Some("Dry Process (Natural)"));
        b.roast_recommendation = Some("City to Full City+".into());
        b.cupping_notes = Some("Rustic fruit. Good for espresso.".into());
        let reads = BeanReads::of(&b);
        let p = profile(Some("time"));
        let (lines, fc) = before(&Before {
            reads: &reads,
            profile: Some(&p),
            fc: Some(history(195.9)),
        });
        assert_eq!(keys(&lines), ["b1", "b2", "b3", "b4", "b6"]);
        assert_eq!(fc.unwrap().window_c, 4.0);
        assert!(
            lines[1].text.ends_with(
                "± 4 °C (≈ ± 30 s); wider for a natural: no source says which way it moves."
            ),
            "{}",
            lines[1].text
        );
        assert_eq!(
            lines[3].text,
            "This profile drops at 10:05 whatever first crack does; a first crack 30 s off changes development as much. Drop by development for a first roast?"
        );
        assert_eq!(lines[3].action, Some("auto_drop_development"));
        assert_eq!(
            lines[4].text,
            "Sweet Maria's: City to Full City+; for espresso “Good for espresso.”"
        );
    }

    /// Production before #10 (the Kenya's first roast): Light espresso's eight roasts of the
    /// Brazil and the Palmichal (#2–#9) marked first crack at 198.5, 197.3, 195.0, 195.3,
    /// 193.6, 195.4, 195.3 and 196.8 °C. The Kenya (no process recorded) cracked at 193.0 °C
    /// at 7:35 from CHARGE: inside the window, and inside the washed one too.
    #[test]
    fn the_kenya_before_its_first_roast() {
        let fcs = [198.5, 197.3, 195.0, 195.3, 193.56, 195.44, 195.31, 196.81];
        let mean = fcs.iter().sum::<f64>() / fcs.len() as f64;
        let reads = BeanReads::of(&bean("Kenya Nyeri Gatugi AA", None));
        let p = profile(Some("time"));
        let (_, fc) = before(&Before {
            reads: &reads,
            profile: Some(&p),
            fc: Some(history(r1(mean))),
        });
        let fc = fc.unwrap();
        assert_eq!(fc.bt, 195.9);
        assert_eq!(fc.window_c, 4.0);
        assert!((193.0 - fc.bt).abs() <= fc.window_c);
        assert!((193.0 - fc.bt).abs() <= WINDOW_C);
    }

    #[test]
    fn wet_hulled_low_density_and_wet_beans_get_b5() {
        let reads = BeanReads::of(&bean("Bali", Some("Wet Hulled")));
        let (lines, _) = before(&Before {
            reads: &reads,
            profile: None,
            fc: None,
        });
        assert_eq!(keys(&lines), ["b1", "b3", "b5"]);
        let mut b = bean("Brazil", Some("Washed"));
        b.density_g_l = Some(640.0);
        let reads = BeanReads::of(&b);
        assert!(keys(
            &before(&Before {
                reads: &reads,
                profile: None,
                fc: None
            })
            .0
        )
        .contains(&"b5"));
        b.density_g_l = Some(700.0);
        b.moisture_pct = Some(12.5);
        let reads = BeanReads::of(&b);
        assert!(keys(
            &before(&Before {
                reads: &reads,
                profile: None,
                fc: None
            })
            .0
        )
        .contains(&"b5"));
        b.moisture_pct = Some(10.5);
        let reads = BeanReads::of(&b);
        assert!(!keys(
            &before(&Before {
                reads: &reads,
                profile: None,
                fc: None
            })
            .0
        )
        .contains(&"b5"));
    }

    #[test]
    fn espresso_advice_only_for_an_espresso_plan() {
        let mut b = bean("Finca Rosma", Some("Dry"));
        b.details = Some(sqlx::types::Json(
            [(
                "Reference Recommended espresso".to_string(),
                "No".to_string(),
            )]
            .into(),
        ));
        let reads = BeanReads::of(&b);
        let mut p = profile(None);
        let (lines, _) = before(&Before {
            reads: &reads,
            profile: Some(&p),
            fc: None,
        });
        assert_eq!(
            lines.last().unwrap().text,
            "Sweet Maria's: for espresso not recommended."
        );
        p.profile.roast_purpose = Some("filter".into());
        let (lines, _) = before(&Before {
            reads: &reads,
            profile: Some(&p),
            fc: None,
        });
        assert!(!keys(&lines).contains(&"b6"));
    }

    fn roast(fc: Option<(f64, f64)>, drop: f64) -> RoastAnalysis {
        let mut marks = vec![];
        if let Some((t, bt)) = fc {
            marks.push(Mark {
                key: "first_crack_start",
                t,
                bt: Some(bt),
                et: None,
                ror: None,
                heater: None,
                fan: None,
            });
        }
        RoastAnalysis {
            session_id: "s".into(),
            name: "#14".into(),
            device_id: "esp32_roaster_01".into(),
            batch_number: Some(14),
            status: "completed".into(),
            roasted_at: "2026-10-30T00:00:00Z".into(),
            coffee: None,
            green_g: Some(100.0),
            roasted_g: Some(86.0),
            loss_pct: Some(14.0),
            charge_bt: None,
            total_s: drop,
            dropped: true,
            development_s: fc.map(|(t, _)| drop - t),
            dtr_pct: None,
            max_bt: None,
            auc: None,
            marks,
            phases: vec![],
            ror: crate::analysis::RorStats::default(),
            profile: None,
            control: crate::analysis::Control::default(),
            expected: vec![],
            notes: vec![],
            channels: vec![],
        }
    }

    fn judged(key: &'static str, v: f64, lo: f64, hi: f64) -> Judged {
        Judged {
            key,
            label: "",
            value: Some(v),
            lo,
            hi,
            source: "level",
            verdict: Some(if v < lo {
                "below"
            } else if v > hi {
                "above"
            } else {
                "within"
            }),
        }
    }

    #[test]
    fn after_a_roast_dropped_by_time() {
        let reads = BeanReads::of(&bean("Kenya", Some("Washed")));
        let p = profile(Some("time")).profile;
        // First crack 20 s late, dropped at the profile's 10:05: development 20 s short
        let r = roast(Some((490.0, 193.0)), 605.0);
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: Some(&p),
            dropped_by: DroppedBy::Auto(DropBy::Time),
        });
        // 490 s of 605: past 80 %, so A5's hint too
        assert_eq!(keys(&lines), ["a1", "a2", "a5"]);
        assert_eq!(
            lines[0].text,
            "First crack came at 193.0 °C, +20 s against the profile's 7:50; the next roast of this bean expects it there."
        );
        assert_eq!(
            lines[1].text,
            "Development 1:55 (−20 s against the profile's 2:15): move the drop +20 s to keep it."
        );
        // Within 5 s: nothing to move
        let r = roast(Some((472.0, 195.0)), 605.0);
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: Some(&p),
            dropped_by: DroppedBy::Hand,
        });
        assert!(
            lines[1].text.ends_with("nothing to move."),
            "{}",
            lines[1].text
        );
    }

    #[test]
    fn after_a_roast_dropped_by_development() {
        let reads = BeanReads::of(&bean("Burundi", Some("Natural")));
        let p = profile(Some("development")).profile;
        let r = roast(Some((480.0, 196.0)), 615.0);
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: Some(&p),
            dropped_by: DroppedBy::Auto(DropBy::Development),
        });
        assert_eq!(keys(&lines), ["a1", "a3"]);
        assert_eq!(
            lines[1].text,
            "Dropped 2:15 after first crack as planned: nothing to move."
        );
    }

    #[test]
    fn outside_the_levels_band_in_steps_of_15_s() {
        let reads = BeanReads::of(&bean("Bali", Some("Wet Hulled")));
        let p = profile(None).profile;
        let mut r = roast(Some((540.0, 196.0)), 590.0);
        r.expected = vec![
            judged("development_s", 50.0, 60.0, 120.0),
            judged("weight_loss_pct", 12.0, 14.0, 16.0),
        ];
        let by_dev = DroppedBy::Auto(DropBy::Development);
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: Some(&p),
            dropped_by: by_dev,
        });
        assert_eq!(keys(&lines), ["a1", "a3", "a4", "a4", "a5"]);
        assert_eq!(
            lines[2].text,
            "Development 0:50 is below the level range 1:00–2:00: change development by +15 s."
        );
        // Development leads: the loss line says no change of its own, and adds wet-hulled
        assert_eq!(
            lines[3].text,
            "Weight loss 12.0 % is below the level range 14.0–16.0 %; wet-hulled beans lose 1–2 points more at the same colour."
        );
        assert_eq!(lines[4].kind, "hint");
        assert_eq!(
            lines[4].text,
            "First crack at 92 % of the roast: late first crack usually means short development."
        );

        // Dropped by hand, A2 asks to move the drop: A4 keeps its line without a number
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: Some(&p),
            dropped_by: DroppedBy::Hand,
        });
        assert_eq!(keys(&lines), ["a1", "a2", "a4", "a4", "a5"]);
        assert_eq!(
            lines[1].text,
            "Development 0:50 (−1:25 against the profile's 2:15): move the drop +85 s to keep it."
                .replace("−1:25", "−85 s")
        );
        assert_eq!(
            lines[2].text,
            "Development 0:50 is below the level range 1:00–2:00."
        );

        assert_eq!(to_band(50.0, 60.0, 120.0), Some(15.0));
        assert_eq!(to_band(31.0, 60.0, 120.0), Some(30.0));
        assert_eq!(to_band(150.0, 60.0, 120.0), Some(-30.0));
        assert_eq!(to_band(90.0, 60.0, 120.0), None);
    }

    #[test]
    fn loss_alone_outside_takes_one_step() {
        let reads = BeanReads::of(&bean("Kenya", Some("Washed")));
        let mut r = roast(None, 605.0);
        r.expected = vec![judged("weight_loss_pct", 17.0, 14.0, 16.0)];
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: None,
            dropped_by: DroppedBy::Hand,
        });
        assert_eq!(keys(&lines), ["a4"]);
        assert!(
            lines[0].text.ends_with("change development by −15 s."),
            "{}",
            lines[0].text
        );
        // A wet-hulled bean with the loss inside its range still gets A6
        let reads = BeanReads::of(&bean("Bali", Some("Wet Hulled")));
        let r = roast(None, 605.0);
        let lines = next(&After {
            reads: &reads,
            roast: &r,
            profile: None,
            dropped_by: DroppedBy::Hand,
        });
        assert_eq!(keys(&lines), ["a6"]);
    }

    #[test]
    fn the_drop_marks_note_says_how_it_dropped() {
        assert_eq!(DroppedBy::read(None), DroppedBy::Hand);
        assert_eq!(DroppedBy::read(Some("good")), DroppedBy::Hand);
        assert_eq!(
            DroppedBy::read(Some("marked by auto DROP (at the profile's drop, 10:05)")),
            DroppedBy::Auto(DropBy::Time)
        );
        assert_eq!(
            DroppedBy::read(Some("marked by auto DROP (2:15 after first crack)")),
            DroppedBy::Auto(DropBy::Development)
        );
        assert_eq!(
            DroppedBy::read(Some(
                "marked by auto DROP (at the profile's drop BT, 210.0 °C)"
            )),
            DroppedBy::Auto(DropBy::Bt)
        );
    }

    #[test]
    fn sentences_split_on_stops() {
        let v: Vec<&str> = sentences("City to Full City+. Good for espresso.").collect();
        assert_eq!(v, ["City to Full City+.", "Good for espresso."]);
        let v: Vec<&str> = sentences("3.5 kg! Fine").collect();
        assert_eq!(v, ["3.5 kg!", "Fine"]);
    }
}
