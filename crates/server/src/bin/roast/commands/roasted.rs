//! `roast roasted` (docs/cli.md, docs/roasted.md): roasted coffee, resting and ready; what was
//! used, given, packed into bags or thrown away; roasts of one coffee put into one bag.

use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Value, json};
use std::fmt::Write as _;

use crate::api::{Api, enc};
use crate::out::{Failure, Out, code};

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct RoastedArgs {
    #[command(subcommand)]
    pub cmd: Option<RoastedCommand>,
    /// Only coffee for espresso or filter (omni and unset roasts count for both)
    #[arg(long = "for", value_name = "PURPOSE", value_parser = ["espresso", "filter"])]
    pub for_: Option<String>,
    /// Only what is ready to drink now
    #[arg(long, conflicts_with = "resting")]
    pub ready: bool,
    /// Only what is still resting
    #[arg(long)]
    pub resting: bool,
    /// Finished roasts too
    #[arg(long)]
    pub all: bool,
}

#[derive(Subcommand, Debug)]
pub enum RoastedCommand {
    /// One roast's coffee: what is left, its rest window and every move
    Show {
        /// The roast: its id, `#12` (batch number), `latest` or a name
        roast: String,
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Coffee used: `36g`, `2 doses` (shots or brews, the dose by the roast's purpose),
    /// `1 bag` (the bag size), or `all` (finished)
    Use(MoveArgs),
    /// Coffee given away (or sold): an amount as for `use`, --to who
    Give {
        #[command(flatten)]
        m: MoveArgs,
        /// Who it went to
        #[arg(long)]
        to: Option<String>,
    },
    /// Bags packed from it: `3x250g` (3 bags of 250 g), or `3` (the bag size, Settings →
    /// Roasting); whole bean unless --ground GRIND (espresso, filter, moka, "French press", …)
    Pack {
        /// The roast: its id, `#12` (batch number), `latest` or a name
        roast: String,
        /// `3x250g`, `2 x 100 g`, `3`
        #[arg(num_args = 1.., required = true)]
        bags: Vec<String>,
        /// Ground, with this grind (a ground bag keeps its best for less)
        #[arg(long, value_name = "GRIND")]
        ground: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// When: YYYY-MM-DD or RFC 3339 (default now)
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Coffee thrown away (stale, spilled): an amount as for `use`, default all
    Toss {
        roast: String,
        #[arg(default_value = "all")]
        amount: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        roaster: Option<String>,
    },
    /// What is left now, weighed: the stock is set to it
    Count {
        roast: String,
        /// Grams left, e.g. 42g
        left: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Roasts put into one bag or jar: a combined lot (a roast already in one: the others join
    /// it), of one coffee or, from different coffees, a blend. From then on a move on any of
    /// them is split across them, and a label for any of them prints the bag (every batch, its
    /// dates; a blend's coffees by origin with their shares)
    Combine {
        /// The roasts: ids, `#16`, `latest` or names; two or more
        #[arg(num_args = 1.., required = true)]
        roasts: Vec<String>,
        /// A name for the bag (default: the coffee's with the batches, "… #16+17"; a blend's
        /// coffees, "Guji + Kenya Nyeri #16+19"). A blend's name prints on its label
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// With `latest` or batch numbers: on this roaster
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Split a combined lot apart: its roasts are separate again, each with its share of
    /// what was taken from the bag
    Uncombine {
        /// Any of its roasts
        roast: String,
        #[arg(long)]
        roaster: Option<String>,
    },
    /// Take a move back: the last one, or --move ID
    Undo {
        roast: String,
        #[arg(long = "move", value_name = "ID")]
        move_id: Option<String>,
        #[arg(long)]
        roaster: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct MoveArgs {
    /// The roast: its id, `#12` (batch number), `latest` or a name
    pub roast: String,
    /// `36g`, `2 doses`, `1 bag` or `all`
    #[arg(num_args = 1.., required = true)]
    pub amount: Vec<String>,
    #[arg(long)]
    pub note: Option<String>,
    /// When: YYYY-MM-DD or RFC 3339 (default now)
    #[arg(long)]
    pub at: Option<String>,
    /// With `latest` or a batch number: on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
}

/// An amount as people say it: a weight, `N doses|shots|cups|brews`, `N bags`, or `all`.
pub fn parse_amount(s: &str) -> Result<Value, String> {
    let t = s.trim().to_ascii_lowercase();
    if matches!(t.as_str(), "all" | "everything" | "rest" | "the rest") {
        return Ok(json!({ "all": true }));
    }
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let unit = unit.trim();
    let n = || -> Result<f64, String> {
        let n: f64 = num
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| format!("not an amount: {s:?} (36g, 2 doses, 1 bag or all)"))?;
        if n.is_finite() && n > 0.0 {
            Ok(n)
        } else {
            Err(format!("an amount is more than 0, not {s:?}"))
        }
    };
    match unit {
        "dose" | "doses" | "shot" | "shots" | "cup" | "cups" | "brew" | "brews" => {
            Ok(json!({ "doses": n()? }))
        }
        "bag" | "bags" => Ok(json!({ "bags": n()? })),
        _ => {
            let g = crate::text::parse_weight(&t)
                .map_err(|_| format!("not an amount: {s:?} (36g, 2 doses, 1 bag or all)"))?;
            if g > 0.0 {
                Ok(json!({ "grams": g }))
            } else {
                Err(format!("an amount is more than 0, not {s:?}"))
            }
        }
    }
}

/// Bags as people say them: `3x250g`, `3 × 250 g`, `2*8oz`, or a count of the bag size
/// (`3`, `3 bags`): `{bags, bag_g?}`.
pub fn parse_bags(s: &str) -> Result<Value, String> {
    let t = s.trim().to_ascii_lowercase().replace(['×', '*'], "x");
    let usage = || format!("not bags: {s:?} (3x250g, or 3 for the bag size)");
    let (count, size) = match t.split_once('x') {
        Some((n, g)) => (n.trim(), Some(g.trim())),
        None => (
            t.trim_end_matches("bags").trim_end_matches("bag").trim(),
            None,
        ),
    };
    let n: f64 = count.parse().map_err(|_| usage())?;
    if !(n >= 1.0 && n.fract() == 0.0) {
        return Err(format!("bags are a whole number from 1, not {s:?}"));
    }
    let mut out = json!({ "bags": n });
    if let Some(g) = size {
        let g = crate::text::parse_weight(g).map_err(|_| usage())?;
        if g <= 0.0 {
            return Err(usage());
        }
        out["bag_g"] = json!(g);
    }
    Ok(out)
}

/// A roast by id, `#N`, `latest`, or a name (or part of one) among its roasted coffee.
async fn lot_id(api: &Api, reference: &str, roaster: Option<&str>) -> Result<String, Failure> {
    if reference.starts_with('#') || reference.eq_ignore_ascii_case("latest") {
        return super::roasts::roast_id(api, reference, roaster).await;
    }
    let stock = api
        .get(&super::labels::with_tz("/api/roasted?all=true"))
        .await?;
    let lots = stock["lots"].as_array().cloned().unwrap_or_default();
    if lots.iter().any(|l| l["session_id"] == reference) {
        return Ok(reference.to_string());
    }
    let want = reference.to_lowercase();
    let named = |exact: bool| -> Vec<&Value> {
        lots.iter()
            .filter(|l| roaster.is_none_or(|r| l["device_id"] == r))
            .filter(|l| {
                let n = l["name"].as_str().unwrap_or("").to_lowercase();
                if exact { n == want } else { n.contains(&want) }
            })
            .collect()
    };
    let mut found = named(true);
    if found.is_empty() {
        found = named(false);
    }
    match found.as_slice() {
        [one] => Ok(one["session_id"].as_str().unwrap_or_default().to_string()),
        [] => Ok(reference.to_string()),
        many => Err(Failure::new(
            "AMBIGUOUS",
            format!(
                "{} roasts match {reference:?}: {}; use #N or the id",
                many.len(),
                many.iter()
                    .map(|l| l["name"].as_str().unwrap_or("?"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

pub async fn roasted(api: &Api, out: &Out, a: RoastedArgs) -> Result<u8, Failure> {
    let with_tz = super::labels::with_tz;
    let Some(cmd) = a.cmd else {
        let path = with_tz(if a.all {
            "/api/roasted?all=true"
        } else {
            "/api/roasted"
        });
        let mut stock = api.get(&path).await?;
        if let Some(lots) = stock["lots"].as_array_mut() {
            lots.retain(|l| {
                let f = l["for"].as_str().unwrap_or("any");
                // A roast in one bag is as ready as the bag
                let s = l["combined"]["state"]
                    .as_str()
                    .or(l["state"].as_str())
                    .unwrap_or("");
                a.for_.as_deref().is_none_or(|want| f == want || f == "any")
                    && (!a.ready || s == "ready")
                    && (!a.resting || s == "resting")
            });
        }
        if let (Some(want), Some(groups)) = (a.for_.as_deref(), stock["groups"].as_array_mut()) {
            groups.retain(|g| g["for"] == want || g["for"] == "any");
        }
        out.emit(&stock, stock_text);
        return Ok(code::OK);
    };
    let path = |id: &str| format!("/api/roasted/{}", enc(id));
    let (id, body) = match cmd {
        RoastedCommand::Show { roast, roaster } => {
            let id = lot_id(api, &roast, roaster.as_deref()).await?;
            let lot = api.get(&with_tz(&path(&id))).await?;
            out.emit(&lot, lot_text);
            return Ok(code::OK);
        }
        RoastedCommand::Combine {
            roasts,
            name,
            note,
            roaster,
        } => {
            let mut ids = Vec::new();
            for r in &roasts {
                ids.push(lot_id(api, r, roaster.as_deref()).await?);
            }
            let lot = api
                .call(
                    Method::POST,
                    &with_tz("/api/roasted/combined"),
                    Some(&json!({ "roasts": ids, "name": name, "note": note })),
                )
                .await?;
            out.emit(&lot, combined_text);
            return Ok(code::OK);
        }
        RoastedCommand::Uncombine { roast, roaster } => {
            let id = lot_id(api, &roast, roaster.as_deref()).await?;
            let r = api
                .call(
                    Method::DELETE,
                    &format!("/api/roasted/combined/{}", enc(&id)),
                    None,
                )
                .await?;
            out.emit(&r, |v| {
                format!(
                    "Split apart: {} roasts are separate again, each keeping its share of what was taken.",
                    v["roasts"].as_array().map_or(0, Vec::len)
                )
            });
            return Ok(code::OK);
        }
        RoastedCommand::Use(m) => move_body(api, "used", m, None).await?,
        RoastedCommand::Pack {
            roast,
            bags,
            ground,
            note,
            at,
            roaster,
        } => {
            let mut body = parse_bags(&bags.join(" ")).map_err(|e| Failure::new("USAGE", e))?;
            let id = lot_id(api, &roast, roaster.as_deref()).await?;
            body["kind"] = json!("packed");
            body["note"] = json!(note);
            body["at"] = json!(at);
            if let Some(g) = ground {
                body["form"] = json!("ground");
                body["grind"] = json!(g);
            }
            (id, body)
        }
        RoastedCommand::Give { m, to } => move_body(api, "given", m, to).await?,
        RoastedCommand::Toss {
            roast,
            amount,
            note,
            at,
            roaster,
        } => {
            let m = MoveArgs {
                roast,
                amount: vec![amount],
                note,
                at,
                roaster,
            };
            move_body(api, "discarded", m, None).await?
        }
        RoastedCommand::Count {
            roast,
            left,
            note,
            roaster,
        } => {
            let g = crate::text::parse_weight(&left).map_err(|e| Failure::new("USAGE", e))?;
            let id = lot_id(api, &roast, roaster.as_deref()).await?;
            (id, json!({ "kind": "adjust", "set_g": g, "note": note }))
        }
        RoastedCommand::Undo {
            roast,
            move_id,
            roaster,
        } => {
            let id = lot_id(api, &roast, roaster.as_deref()).await?;
            let move_id = match move_id {
                Some(m) => m,
                None => {
                    let lot = api.get(&with_tz(&path(&id))).await?;
                    lot["moves"]
                        .as_array()
                        .and_then(|m| m.last())
                        .and_then(|m| m["id"].as_str())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            Failure::new("NOT_FOUND", "this roast has no moves to take back")
                        })?
                }
            };
            let lot = api
                .call(
                    Method::DELETE,
                    &with_tz(&format!("{}/moves/{}", path(&id), enc(&move_id))),
                    None,
                )
                .await?;
            out.emit(&lot, lot_text);
            return Ok(code::OK);
        }
    };
    let lot = api
        .call(
            Method::POST,
            &with_tz(&format!("{}/moves", path(&id))),
            Some(&body),
        )
        .await?;
    out.emit(&lot, lot_text);
    Ok(code::OK)
}

async fn move_body(
    api: &Api,
    kind: &str,
    m: MoveArgs,
    to: Option<String>,
) -> Result<(String, Value), Failure> {
    let amount = parse_amount(&m.amount.join(" ")).map_err(|e| Failure::new("USAGE", e))?;
    let id = lot_id(api, &m.roast, m.roaster.as_deref()).await?;
    let mut body = json!({ "kind": kind, "note": m.note, "at": m.at, "recipient": to });
    if let (Some(b), Some(a)) = (body.as_object_mut(), amount.as_object()) {
        b.extend(a.clone());
    }
    Ok((id, body))
}

// ---- text ----

fn day(v: &Value) -> String {
    v.as_str()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .map_or_else(|| "?".into(), |d| d.format("%-d %b").to_string())
}

fn g(v: &Value) -> String {
    v.as_f64().map_or_else(
        || "?".into(),
        |g| {
            if (g - g.round()).abs() < 0.05 {
                format!("{g:.0} g")
            } else {
                format!("{g:.1} g")
            }
        },
    )
}

fn roast_name(l: &Value) -> String {
    match l["batch_number"].as_i64() {
        Some(n) => format!("#{n} {}", l["name"].as_str().unwrap_or("?")),
        None => l["name"].as_str().unwrap_or("?").to_string(),
    }
}

fn lot_line(l: &Value) -> String {
    let left = g(&l["left_g"]);
    let of = g(&l["roasted_g"]);
    let roasted = day(&l["roasted_on"]);
    let what = match l["state"].as_str() {
        Some("resting") => format!(
            "resting, ready {} (in {} d)",
            day(&l["ready_from"]),
            l["ready_in_days"].as_i64().unwrap_or(0)
        ),
        Some("ready") => format!(
            "ready, day {}, at its best until {}",
            l["age_days"].as_i64().unwrap_or(0),
            day(&l["best_until"])
        ),
        Some("past_best") => format!("past its best since {}", day(&l["best_until"])),
        Some("unweighed") => "not weighed: roast weigh".into(),
        Some("finished") => "finished".into(),
        _ => "?".into(),
    };
    let for_ = l["for"].as_str().unwrap_or("any");
    format!(
        "{}  {left} of {of}  for {for_}, roasted {roasted}: {what}",
        roast_name(l)
    )
}

fn stock_text(s: &Value) -> String {
    let mut t = String::new();
    for gr in s["groups"].as_array().into_iter().flatten() {
        let for_ = match gr["for"].as_str() {
            Some("espresso") => "Espresso",
            Some("filter") => "Filter",
            _ => "Either",
        };
        let _ = write!(
            t,
            "{for_}: {} ready, {} resting",
            g(&gr["ready_g"]),
            g(&gr["resting_g"])
        );
        if gr["next_ready"].is_string() {
            let _ = write!(t, " (next ready {})", day(&gr["next_ready"]));
        }
        if gr["past_best_g"].as_f64().unwrap_or(0.0) > 0.0 {
            let _ = write!(t, ", {} past its best", g(&gr["past_best_g"]));
        }
        if let Some(r) = gr["per_day_g"].as_f64() {
            let _ = write!(
                t,
                "; {r:.0} g a day, lasts about {:.0} days, roast by {}",
                gr["lasts_days"].as_f64().unwrap_or(0.0),
                day(&gr["roast_by"])
            );
        }
        if gr["low"] == json!(true) {
            t.push_str("  RUNNING LOW");
        }
        t.push('\n');
    }
    let lots = s["lots"].as_array().cloned().unwrap_or_default();
    if lots.is_empty() {
        t.push_str("No roasted coffee: a finished roast with its roasted weight is listed here.");
    }
    let mut bags_shown: Vec<String> = Vec::new();
    for l in &lots {
        match l["combined"]["id"].as_str() {
            Some(id) => {
                if !bags_shown.iter().any(|b| b == id) {
                    bags_shown.push(id.to_string());
                    let _ = writeln!(t, "  {}", bag_line(&l["combined"], l));
                }
                let _ = writeln!(
                    t,
                    "      {}  {} of {}, roasted {}",
                    roast_name(l),
                    g(&l["left_g"]),
                    g(&l["roasted_g"]),
                    day(&l["roasted_on"])
                );
            }
            None => {
                let _ = writeln!(t, "  {}", lot_line(l));
            }
        }
    }
    t.trim_end().to_string()
}

/// A bag of several roasts: its batches, what is left of them together and its rest window
fn bag_line(c: &Value, l: &Value) -> String {
    let first = day(&c["first_roasted_on"]);
    let last = day(&c["last_roasted_on"]);
    let roasted = if first == last {
        first
    } else {
        format!("{first}–{last}")
    };
    let what = match c["state"].as_str() {
        Some("resting") => format!(
            "resting, ready {} (in {} d)",
            day(&c["ready_from"]),
            c["ready_in_days"].as_i64().unwrap_or(0)
        ),
        Some("ready") => format!("ready, at its best until {}", day(&c["best_until"])),
        Some("past_best") => format!("past its best since {}", day(&c["best_until"])),
        Some("finished") => "finished".into(),
        _ => "?".into(),
    };
    // A blend: its coffees and their shares, "60 % Guji · 40 % Kenya Nyeri"
    let blend = (c["blend"] == true).then(|| {
        c["coffees"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|k| {
                format!(
                    "{} % {}",
                    k["share_pct"].as_i64().unwrap_or(0),
                    k["name"].as_str().unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    });
    format!(
        "{} {}{}  {} of {}  for {}, roasted {roasted}: {what}",
        if blend.is_some() { "Blend" } else { "Bag" },
        c["name"].as_str().unwrap_or("?"),
        blend.map(|b| format!(" ({b})")).unwrap_or_default(),
        g(&c["left_g"]),
        g(&c["roasted_g"]),
        l["for"].as_str().unwrap_or("any"),
    )
}

/// A combined lot: the bag, its roasts and the moves made on it
fn combined_text(c: &Value) -> String {
    let mut t = bag_line(c, c);
    for m in c["members"].as_array().into_iter().flatten() {
        let _ = write!(
            t,
            "\n    {}  {} of {}, roasted {}",
            roast_name(m),
            g(&m["left_g"]),
            g(&m["roasted_g"]),
            day(&m["roasted_on"])
        );
    }
    for m in c["moves"].as_array().into_iter().flatten() {
        let at = m["at"].as_str().and_then(|s| s.get(..10)).unwrap_or("?");
        let split = m["split"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                format!(
                    "#{} {:+.1}",
                    s["batch_number"]
                        .as_i64()
                        .map_or("?".into(), |b| b.to_string()),
                    s["delta_g"].as_f64().unwrap_or(0.0)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            t,
            "\n  {at}  {:+.1} g  {}  ({split})",
            m["delta_g"].as_f64().unwrap_or(0.0),
            m["kind"].as_str().unwrap_or("?"),
        );
    }
    t
}

fn lot_text(l: &Value) -> String {
    let mut t = lot_line(l);
    if l["combined"].is_object() {
        let _ = write!(
            t,
            "\n  In one bag: {} (a move on any of its roasts is split across them)",
            bag_line(&l["combined"], l)
        );
    }
    for m in l["moves"].as_array().into_iter().flatten() {
        let at = m["at"].as_str().and_then(|s| s.get(..10)).unwrap_or("?");
        let bags = match (m["bags"].as_i64(), m["bag_g"].as_f64()) {
            (Some(n), Some(b)) => format!(
                " {n} × {} {}{}",
                g(&json!(b)),
                if m["form"] == "ground" {
                    "ground"
                } else {
                    "whole bean"
                },
                m["grind"]
                    .as_str()
                    .map(|x| format!(" for {x}"))
                    .unwrap_or_default()
            ),
            _ => String::new(),
        };
        let _ = write!(
            t,
            "\n  {at}  {:+.1} g  {}{bags}{}{}  [{}]",
            m["delta_g"].as_f64().unwrap_or(0.0),
            m["kind"].as_str().unwrap_or("?"),
            m["recipient"]
                .as_str()
                .map(|r| format!(" to {r}"))
                .unwrap_or_default(),
            m["note"]
                .as_str()
                .map(|n| format!(": {n}"))
                .unwrap_or_default(),
            m["id"].as_str().unwrap_or("?"),
        );
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_as_people_say_them() {
        assert_eq!(parse_amount("36g").unwrap(), json!({"grams": 36.0}));
        assert_eq!(parse_amount("2 doses").unwrap(), json!({"doses": 2.0}));
        assert_eq!(parse_amount("2shots").unwrap(), json!({"doses": 2.0}));
        assert_eq!(parse_amount("1 bag").unwrap(), json!({"bags": 1.0}));
        assert_eq!(parse_amount("All").unwrap(), json!({"all": true}));
        assert_eq!(parse_amount("4oz").unwrap(), json!({"grams": 113.4}));
        assert!(parse_amount("0g").is_err());
        assert!(parse_amount("lots").is_err());
    }

    #[test]
    fn bags_as_people_say_them() {
        assert_eq!(
            parse_bags("3x250g").unwrap(),
            json!({"bags": 3.0, "bag_g": 250.0})
        );
        assert_eq!(
            parse_bags("2 × 100 g").unwrap(),
            json!({"bags": 2.0, "bag_g": 100.0})
        );
        assert_eq!(parse_bags("3 bags").unwrap(), json!({"bags": 3.0}));
        assert_eq!(parse_bags("1*8oz").unwrap()["bag_g"], json!(226.8));
        assert!(parse_bags("1.5x250g").is_err());
        assert!(parse_bags("0x250g").is_err());
        assert!(parse_bags("some").is_err());
    }
}
