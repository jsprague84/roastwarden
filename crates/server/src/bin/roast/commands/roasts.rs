//! Roasts: the list, one roast shown, weighed or given a landmark; and a roast named by its
//! id, `#12` or `latest`.

use std::fmt::Write as _;
use std::time::Duration;

use clap::Args;
use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{self, enc, Api};
use crate::out::{code, for_finding, Failure, Out};
use crate::text::{clock, parse_clock, parse_weight, roast_text, roasts_text, RoastRow};

/// `roast roasts`: the newest roasts with their numbers.
pub async fn list(
    api: &Api,
    out: &Out,
    roaster: Option<String>,
    limit: u32,
) -> Result<u8, Failure> {
    let mut path = format!("/api/sessions?limit={}", limit.clamp(1, 500));
    if let Some(r) = roaster {
        let _ = write!(path, "&device_id={}", enc(&r));
    }
    let list = api.get(&path).await?;
    let roasts: Vec<RoastRow> = api::decode(list.clone())?;
    if out.json {
        out.emit(&for_finding(list, &["created_at", "updated_at"]), |_| {
            String::new()
        });
    } else {
        out.emit(&roasts, |r| roasts_text(r));
    }
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    pub roast: String,
    #[arg(long)]
    pub telemetry: bool,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
}

/// `roast show`: one roast with its events, cost and cupping.
pub async fn show(api: &Api, out: &Out, a: ShowArgs) -> Result<u8, Failure> {
    let ShowArgs {
        roast,
        telemetry,
        roaster,
    } = a;
    let id = enc(&roast_id(api, &roast, roaster.as_deref()).await?);
    let mut one = api.get(&format!("/api/sessions/{id}")).await?;
    if !telemetry {
        if let Some(o) = one.as_object_mut() {
            let samples = o
                .get("telemetry")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            o.remove("telemetry");
            o.insert("samples".into(), json!(samples));
        }
    }
    let events = api.get(&format!("/api/sessions/{id}/events")).await?;
    one["events"] = events;
    // Known when the roast has a bean with a priced purchase before it (else `null`).
    if let Ok(cost) = api.get(&format!("/api/sessions/{id}/cost")).await {
        if !cost.is_null() {
            one["cost"] = cost;
        }
    }
    if let Ok(c) = api.get(&format!("/api/sessions/{id}/cupping")).await {
        if !c.is_null() {
            one["cupping"] = c;
        }
    }
    out.emit(&one, roast_text);
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct WeighArgs {
    pub roast: String,
    /// The roasted coffee, after cooling and without chaff: `85.75`, `85.75g`
    #[arg(value_parser = parse_weight)]
    pub roasted: f64,
    /// The green weight, if it differs from what the roast has
    #[arg(long, value_parser = parse_weight)]
    pub green: Option<f64>,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
}

/// `roast weigh`: a roast's roasted (and green) weight; the loss follows.
pub async fn weigh(api: &Api, out: &Out, a: WeighArgs) -> Result<u8, Failure> {
    let WeighArgs {
        roast,
        roasted,
        green,
        roaster,
    } = a;
    let id = roast_id(api, &roast, roaster.as_deref()).await?;
    let mut body = json!({ "roasted_weight": roasted });
    if let Some(g) = green {
        body["green_weight"] = json!(g);
    }
    let one = api
        .call(
            Method::PUT,
            &format!("/api/sessions/{}", enc(&id)),
            Some(&body),
        )
        .await?;
    let keep = [
        "id",
        "name",
        "batch_number",
        "green_weight",
        "roasted_weight",
        "weight_loss_pct",
    ];
    let short: serde_json::Map<String, Value> = keep
        .iter()
        .filter_map(|k| one.get(*k).map(|v| (k.to_string(), v.clone())))
        .collect();
    out.emit(&Value::Object(short), |r| {
        format!(
            "{}: {} g → {} g, weight loss {:.1} %",
            r["name"].as_str().unwrap_or("?"),
            r["green_weight"],
            r["roasted_weight"],
            r["weight_loss_pct"].as_f64().unwrap_or(f64::NAN)
        )
    });
    Ok(code::OK)
}

#[derive(Args, Debug)]
pub struct LandmarkArgs {
    /// The roast's id, or `latest` (--roaster narrows it)
    pub roast: String,
    /// charge, tp, dry, fc, fc-end, sc, sc-end, drop, cool (or the full name, e.g.
    /// first_crack_start)
    pub kind: String,
    /// When, from CHARGE as the app shows it: `7:30`, or seconds
    #[arg(long, required_unless_present = "rm")]
    pub at: Option<String>,
    /// Remove the landmark instead
    #[arg(long, conflicts_with_all = ["at", "temp", "note"])]
    pub rm: bool,
    /// BT then, °C (default: the roast's own reading at that moment)
    #[arg(long)]
    pub temp: Option<f64>,
    /// Why it moved (kept with the event)
    #[arg(long)]
    pub note: Option<String>,
    /// With `latest`: the newest roast on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
}

/// `roast landmark`: move, add or remove a landmark; a finished roast's numbers follow.
pub async fn landmark(api: &Api, out: &Out, a: LandmarkArgs) -> Result<u8, Failure> {
    let LandmarkArgs {
        roast,
        kind,
        at,
        rm,
        temp,
        note,
        roaster,
    } = a;
    let kind = landmark_kind(&kind).map_err(|m| Failure::new("USAGE", m))?;
    let id = roast_id(api, &roast, roaster.as_deref()).await?;
    let path = format!("/api/sessions/{}", enc(&id));
    let events = api.get(&format!("{path}/events")).await?;
    let events = events.as_array().cloned().unwrap_or_default();
    if rm {
        let Some(eid) = events
            .iter()
            .find(|e| e["event_type"] == kind)
            .and_then(|e| e["id"].as_str())
        else {
            return Err(Failure {
                status: Some(404),
                ..Failure::new("NOT_FOUND", format!("this roast has no {kind}"))
            });
        };
        api.send(
            Method::DELETE,
            &format!("{path}/events/{}", enc(eid)),
            None,
            Duration::from_secs(30),
        )
        .await?;
        let after = api.get(&path).await?;
        let result = json!({
            "removed": kind,
            "first_crack_time": after["first_crack_time"],
            "development_time_ratio": after["development_time_ratio"],
        });
        out.emit(&result, |_| format!("removed {kind}"));
        return Ok(code::OK);
    }
    let at = at.unwrap_or_default();
    let since_charge = parse_clock(&at).map_err(|m| Failure::new("USAGE", m))?;
    let one = api.get(&path).await?;
    // Event times count from the session start; the app shows them from CHARGE
    let charge = events
        .iter()
        .find(|e| e["event_type"] == "charge")
        .and_then(|e| e["elapsed_seconds"].as_f64())
        .unwrap_or(0.0);
    let elapsed = if kind == "charge" {
        since_charge
    } else {
        charge + since_charge
    };
    let temp = temp.or_else(|| bt_at(&one["telemetry"], elapsed));
    let mut body = json!({ "elapsed_seconds": elapsed });
    if let Some(t) = temp {
        body["temperature"] = json!((t * 10.0).round() / 10.0);
    }
    if let Some(n) = &note {
        body["notes"] = json!(n);
    }
    let existing = events.iter().find(|e| e["event_type"] == kind);
    let event = match existing.and_then(|e| e["id"].as_str()) {
        Some(eid) => {
            api.call(
                Method::PUT,
                &format!("{path}/events/{}", enc(eid)),
                Some(&body),
            )
            .await?
        }
        None => {
            body["event_type"] = json!(kind);
            api.call(Method::POST, &format!("{path}/events"), Some(&body))
                .await?
        }
    };
    // The roast as it now reads (a finished one has its numbers recomputed)
    let after = api.get(&path).await?;
    let result = json!({
        "event": event,
        "first_crack_time": after["first_crack_time"],
        "development_time_ratio": after["development_time_ratio"],
        "status": after["status"],
    });
    out.emit(&result, |r| {
        format!(
            "{} at {} from CHARGE ({} °C); DTR now {}",
            kind,
            clock(since_charge),
            r["event"]["temperature"],
            r["development_time_ratio"]
                .as_f64()
                .map_or("-".into(), |d| format!("{:.1} %", d * 100.0))
        )
    });
    Ok(code::OK)
}

/// A roast by id, `#12` (its batch number; on `roaster` when given), or `latest`: the newest
/// roast (on `roaster` when given).
pub async fn roast_id(
    api: &Api,
    reference: &str,
    roaster: Option<&str>,
) -> Result<String, Failure> {
    if let Some(n) = reference
        .strip_prefix('#')
        .and_then(|n| n.parse::<i64>().ok())
    {
        let mut path = "/api/sessions?limit=1000".to_string();
        if let Some(r) = roaster {
            let _ = write!(path, "&device_id={}", enc(r));
        }
        let list = api.get(&path).await?;
        let found: Vec<&Value> = list
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["batch_number"].as_i64() == Some(n))
            .filter(|s| roaster.is_none_or(|r| s["device_id"] == r))
            .collect();
        return match found.as_slice() {
            [one] => Ok(one["id"].as_str().unwrap_or_default().to_string()),
            [] => Err(Failure::new("NOT_FOUND", format!("no roast #{n}"))),
            _ => Err(Failure::new(
                "BAD_REQUEST",
                format!("#{n} is a batch on more than one roaster: say which with --roaster"),
            )),
        };
    }
    if !reference.eq_ignore_ascii_case("latest") {
        return Ok(reference.to_string());
    }
    let mut path = "/api/sessions?limit=50".to_string();
    if let Some(r) = roaster {
        let _ = write!(path, "&device_id={}", enc(r));
    }
    let list = api.get(&path).await?;
    let start = |s: &Value| {
        s["start_time"]
            .as_str()
            .or(s["created_at"].as_str())
            .unwrap_or("")
            .to_string()
    };
    list.as_array()
        .and_then(|l| {
            l.iter()
                .filter(|s| roaster.is_none_or(|r| s["device_id"] == r))
                .max_by_key(|s| start(s))
        })
        .and_then(|s| s["id"].as_str().map(str::to_string))
        .ok_or_else(|| Failure::new("NOT_FOUND", "no roast yet"))
}

/// A landmark's event type from a short name.
pub fn landmark_kind(s: &str) -> Result<&'static str, String> {
    let k = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    Ok(match k.as_str() {
        "charge" => "charge",
        "tp" | "turning_point" => "turning_point",
        "dry" | "dry_end" | "drying_end" => "drying_end",
        "fc" | "first_crack" | "first_crack_start" => "first_crack_start",
        "fc_end" | "first_crack_end" => "first_crack_end",
        "sc" | "second_crack" | "second_crack_start" => "second_crack_start",
        "sc_end" | "second_crack_end" => "second_crack_end",
        "drop" => "drop",
        "cool" | "cool_end" => "cool_end",
        _ => {
            return Err(format!(
                "unknown landmark '{s}': charge, tp, dry, fc, fc-end, sc, sc-end, drop or cool"
            ))
        }
    })
}

/// BT at `elapsed` (seconds from the session start) from a roast's telemetry, nearest sample.
pub fn bt_at(telemetry: &Value, elapsed: f64) -> Option<f64> {
    telemetry
        .as_array()?
        .iter()
        .filter_map(|p| Some((p["elapsed_seconds"].as_f64()?, p["bean_temp"].as_f64()?)))
        .min_by(|a, b| (a.0 - elapsed).abs().total_cmp(&(b.0 - elapsed).abs()))
        .map(|(_, bt)| bt)
}
