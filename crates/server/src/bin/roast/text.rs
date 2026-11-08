//! Weights, times and sizes read and written, and the text views of roasts, beans, planned
//! roasts and the shopping list: what `roast` prints when a person reads the answer.

use serde::Deserialize;
use serde_json::{json, Value};

/// `7:30` or `450` (seconds) as seconds.
pub fn parse_clock(s: &str) -> Result<f64, String> {
    let t = s.trim();
    let bad = || format!("'{s}' is not a time: use m:ss (7:30) or seconds (450)");
    let v = match t.split_once(':') {
        Some((m, sec)) => {
            let m: f64 = m.parse().map_err(|_| bad())?;
            let sec: f64 = sec.parse().map_err(|_| bad())?;
            if !(0.0..60.0).contains(&sec) {
                return Err(bad());
            }
            m * 60.0 + sec
        }
        None => t.parse().map_err(|_| bad())?,
    };
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(bad())
    }
}

pub fn clock(secs: f64) -> String {
    let s = secs.round() as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A weight in grams: `5000`, `5000g`, `5 kg`, `2lb`, `12oz`, `-250g`.
pub fn parse_weight(s: &str) -> Result<f64, String> {
    let t = s.trim().to_ascii_lowercase().replace(' ', "");
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: f64 = num
        .replace(',', ".")
        .parse()
        .map_err(|_| format!("not a weight: {s:?} (e.g. 5kg, 2500g, 2lb)"))?;
    let factor = match unit {
        "" | "g" | "gr" | "gram" | "grams" => 1.0,
        "kg" | "kgs" | "kilo" | "kilos" => 1000.0,
        "lb" | "lbs" | "pound" | "pounds" => 453.592_37,
        "oz" | "ounce" | "ounces" => 28.349_523_125,
        other => return Err(format!("unknown unit {other:?} (g, kg, lb or oz)")),
    };
    let g = n * factor;
    if !g.is_finite() {
        return Err(format!("not a weight: {s:?}"));
    }
    // Grams as typed (a scale reads 85.75 g); a converted unit to 0.1 g
    Ok(if factor == 1.0 {
        g
    } else {
        (g * 10.0).round() / 10.0
    })
}

pub fn grams(g: f64) -> String {
    if g.abs() >= 1000.0 {
        format!("{:.2} kg", g / 1000.0)
    } else {
        format!("{g:.0} g")
    }
}

pub fn bytes(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.1} MB", b as f64 / (1 << 20) as f64)
    } else {
        format!("{} kB", b / 1024)
    }
}

pub fn duration(s: i64) -> String {
    match s {
        s if s < 120 => format!("{s} s"),
        s if s < 7200 => format!("{} min", s / 60),
        s if s < 172_800 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86400),
    }
}

pub fn mmss(s: Option<i64>) -> String {
    s.map_or("-".into(), |s| format!("{}:{:02}", s / 60, s % 60))
}

#[derive(Deserialize, serde::Serialize, Default)]
pub struct RoastRow {
    id: String,
    name: String,
    device_id: String,
    status: String,
    start_time: Option<String>,
    batch_number: Option<i64>,
    green_weight: Option<f64>,
    weight_loss_pct: Option<f64>,
    total_time_seconds: Option<i64>,
    first_crack_time: Option<i64>,
    development_time_ratio: Option<f64>,
}

pub fn roasts_text(rows: &[RoastRow]) -> String {
    if rows.is_empty() {
        return "No roasts".into();
    }
    rows.iter()
        .map(|r| {
            format!(
                "{}  {:<16} #{:<4} {:<24} {:<9} {:>6} loss {:>5}  FC {:>5}  total {:>5}  dev {}",
                r.start_time
                    .as_deref()
                    .and_then(|t| t.get(..16))
                    .unwrap_or("not started     "),
                r.device_id,
                r.batch_number.map_or("-".into(), |n| n.to_string()),
                r.name.chars().take(24).collect::<String>(),
                r.status,
                r.green_weight.map_or("-".into(), |g| format!("{g:.0} g")),
                r.weight_loss_pct.map_or("-".into(), |l| format!("{l:.1}%")),
                mmss(r.first_crack_time),
                mmss(r.total_time_seconds),
                r.development_time_ratio
                    .map_or("-".into(), |d| format!("{:.0}%", d * 100.0)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn draft_text(d: &Value) -> String {
    let mut lines = vec![format!(
        "{}  ({})",
        d["name"].as_str().unwrap_or("?"),
        d["supplier"].as_str().unwrap_or("?")
    )];
    for (k, label) in [
        ("origin", "origin"),
        ("region", "region"),
        ("producer", "producer"),
        ("variety", "variety"),
        ("process", "process"),
        ("drying", "drying"),
        ("altitude", "altitude"),
        ("grade", "grade"),
        ("arrival", "arrival"),
        ("lot", "lot"),
        ("score", "score"),
        ("roast_recommendation", "roast"),
        ("cupping_notes", "notes"),
    ] {
        match &d[k] {
            Value::String(v) => lines.push(format!("  {label}: {v}")),
            Value::Number(n) => lines.push(format!("  {label}: {n}")),
            _ => {}
        }
    }
    for z in d["sizes"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  size {:<16} {:>9}  {}{}",
            z["label"].as_str().unwrap_or("?"),
            z["weight_g"].as_f64().map(grams).unwrap_or_default(),
            z["price"]
                .as_f64()
                .map_or_else(|| "?".into(), |p| format!("{p:.2}")),
            if z["available"] == json!(false) {
                "  sold out"
            } else {
                ""
            }
        ));
    }
    lines.join("\n")
}

pub fn wanted_line(w: &Value) -> String {
    format!(
        "{}  {}{}{}{}{}",
        w["id"].as_str().unwrap_or("?"),
        w["name"].as_str().unwrap_or("?"),
        w["size_label"]
            .as_str()
            .map(|l| format!(", {l}"))
            .unwrap_or_default(),
        match w["quantity"].as_i64() {
            Some(n) if n > 1 => format!(" × {n}"),
            _ => String::new(),
        },
        w["price"]
            .as_f64()
            .map(|p| format!(" at {p:.2}"))
            .unwrap_or_default(),
        match w["status"].as_str() {
            Some("bought") => " (bought)",
            Some("ordered") => " (on order)",
            _ => "",
        }
    )
}

pub fn carts_text(r: &Value) -> String {
    let mut out = Vec::new();
    for c in r["carts"].as_array().into_iter().flatten() {
        out.push(format!(
            "{} ({} lines, {:.2}{}):",
            c["supplier"].as_str().unwrap_or("?"),
            c["lines"]
                .as_array()
                .map_or(0, |l| l.iter().filter(|x| x["in_link"] == true).count()),
            c["total"].as_f64().unwrap_or(0.0),
            c["currency"]
                .as_str()
                .map(|c| format!(" {c}"))
                .unwrap_or_default()
        ));
        out.push(format!(
            "  {}",
            c["url"]
                .as_str()
                .unwrap_or("(no link: nothing can go in it)")
        ));
        for l in c["lines"].as_array().into_iter().flatten() {
            out.push(format!(
                "  {} {}{} × {}{}{}",
                if l["in_link"] == true { "+" } else { "-" },
                l["name"].as_str().unwrap_or("?"),
                l["size_label"]
                    .as_str()
                    .map(|s| format!(", {s}"))
                    .unwrap_or_default(),
                l["quantity"]
                    .as_f64()
                    .map_or("?".into(), |q| format!("{q}")),
                l["price"]
                    .as_f64()
                    .map(|p| format!(" at {p:.2}"))
                    .unwrap_or_default(),
                l["note"]
                    .as_str()
                    .map(|n| format!(" ({n})"))
                    .unwrap_or_default(),
            ));
        }
        for w in c["warnings"].as_array().into_iter().flatten() {
            out.push(format!("  note: {}", w.as_str().unwrap_or("")));
        }
    }
    for l in r["left_out"].as_array().into_iter().flatten() {
        out.push(format!(
            "left out: {} ({})",
            l["name"].as_str().unwrap_or("?"),
            l["why"].as_str().unwrap_or("?")
        ));
    }
    if out.is_empty() {
        "Nothing on the shopping list to put in a cart".into()
    } else {
        out.join("\n")
    }
}

pub fn wanted_text(list: &Value) -> String {
    let rows: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(wanted_line)
        .collect();
    if rows.is_empty() {
        "Nothing on the shopping list".into()
    } else {
        rows.join("\n")
    }
}

pub fn costs_text(r: &Value) -> String {
    let cur = r["currency"].as_str().unwrap_or("");
    let months = r["months"].as_array().cloned().unwrap_or_default();
    if months.is_empty() {
        return "No purchases or roasts yet".into();
    }
    let mut lines = vec![format!(
        "month    spent {cur:<4}  bought     roasts  green      roasted    coffee cost"
    )];
    for m in months {
        let n = |k: &str| m[k].as_f64().unwrap_or(0.0);
        let other: Vec<String> = m["spent_other"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(c, v)| format!("+{:.2} {c}", v.as_f64().unwrap_or(0.0)))
            .collect();
        let costed = m["costed_roasts"].as_i64().unwrap_or(0);
        let roasts = m["roasts"].as_i64().unwrap_or(0);
        lines.push(format!(
            "{}  {:>10.2}  {:>9}  {:>6}  {:>9}  {:>9}  {:>8}{}{}",
            m["month"].as_str().unwrap_or("?"),
            n("spent"),
            grams(n("bought_g")),
            roasts,
            grams(n("green_roasted_g")),
            grams(n("roasted_g")),
            if costed > 0 {
                format!("{:.2}", n("roast_cost"))
            } else {
                "-".into()
            },
            if costed > 0 && costed != roasts {
                format!(" ({costed} of {roasts} roasts)")
            } else {
                String::new()
            },
            if other.is_empty() {
                String::new()
            } else {
                format!("  {}", other.join(" "))
            }
        ));
    }
    lines.join("\n")
}

pub fn roast_text(r: &Value) -> String {
    let row: RoastRow = serde_json::from_value(r.clone()).unwrap_or_default();
    let mut lines = vec![roasts_text(std::slice::from_ref(&row))];
    let c = &r["cost"];
    if let (Some(g), Some(cur)) = (c["green_cost"].as_f64(), c["currency"].as_str()) {
        lines.push(format!(
            "  coffee {g:.2} {cur}{}{}{}",
            c["roasted_per_kg"]
                .as_f64()
                .map(|v| format!(", {v:.2}/kg roasted"))
                .unwrap_or_default(),
            c["per_espresso_dose"]
                .as_f64()
                .map(|v| format!(
                    ", {v:.2} per {} g espresso",
                    c["espresso_dose_g"].as_f64().unwrap_or(0.0)
                ))
                .unwrap_or_default(),
            c["per_filter_dose"]
                .as_f64()
                .map(|v| format!(
                    ", {v:.2} per {} g filter",
                    c["filter_dose_g"].as_f64().unwrap_or(0.0)
                ))
                .unwrap_or_default(),
        ));
    }
    for e in r["events"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {:>5}  {:<12} {}",
            mmss(e["elapsed_seconds"].as_i64()),
            e["event_type"].as_str().unwrap_or("?"),
            e["temperature"]
                .as_f64()
                .map_or(String::new(), |t| format!("{t:.1} °C"))
        ));
    }
    lines.join("\n")
}

#[derive(Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct BeanRow {
    id: String,
    name: String,
    origin: Option<String>,
    process: Option<String>,
    stock_g: f64,
    low: bool,
    archived: bool,
    roast_count: i64,
    cost_per_kg: Option<f64>,
    currency: Option<String>,
    /// Ordered or shipped, not received (docs/green-coffee.md, "Orders")
    #[serde(default)]
    on_order_g: f64,
}

pub fn bean_line(b: &BeanRow) -> String {
    format!(
        "{}  {:<28} {:>10}{}{}{}  {} roasts  {}{}",
        b.id,
        b.name,
        grams(b.stock_g),
        if b.low { " LOW" } else { "" },
        if b.on_order_g > 0.0 {
            format!(" (+{} on order)", grams(b.on_order_g))
        } else {
            String::new()
        },
        match (b.cost_per_kg, &b.currency) {
            (Some(c), Some(cur)) => format!("  {c:.2} {cur}/kg"),
            _ => String::new(),
        },
        b.roast_count,
        [b.origin.as_deref(), b.process.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", "),
        if b.archived { " (archived)" } else { "" }
    )
}

pub fn beans_text(rows: &[BeanRow]) -> String {
    if rows.is_empty() {
        return "No beans".into();
    }
    rows.iter().map(bean_line).collect::<Vec<_>>().join("\n")
}

#[derive(Deserialize, serde::Serialize, Default)]
pub struct PlanRow {
    id: String,
    device_id: Option<String>,
    green_weight_g: Option<f64>,
    name: Option<String>,
    session_id: Option<String>,
    bean_name: Option<String>,
    profile_name: Option<String>,
    batch_number: Option<i64>,
}

pub fn plan_line(p: &PlanRow) -> String {
    format!(
        "{}  {} {} on {} with {}{}{}",
        p.id,
        p.green_weight_g.map_or("?".into(), grams),
        p.bean_name.as_deref().unwrap_or("(no bean)"),
        p.device_id.as_deref().unwrap_or("any roaster"),
        p.profile_name.as_deref().unwrap_or("no profile"),
        p.name
            .as_deref()
            .map(|n| format!(" \"{n}\""))
            .unwrap_or_default(),
        match (&p.session_id, p.batch_number) {
            (Some(_), Some(n)) => format!(" (roasted, batch #{n})"),
            (Some(_), None) => " (roasted)".into(),
            _ => String::new(),
        }
    )
}

#[derive(Deserialize, serde::Serialize)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub roast_level: Option<String>,
    pub roast_purpose: Option<String>,
    #[serde(default)]
    pub roaster_kind: Option<String>,
}

#[derive(Deserialize, serde::Serialize)]
pub struct AuditRow {
    pub at: String,
    #[serde(default)]
    pub kind: String,
    pub actor: String,
    pub token_name: Option<String>,
    pub method: String,
    pub path: String,
    pub status: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weights_read_as_grams() {
        assert_eq!(parse_weight("5000").unwrap(), 5000.0);
        assert_eq!(parse_weight("5kg").unwrap(), 5000.0);
        assert_eq!(parse_weight("5 KG").unwrap(), 5000.0);
        assert_eq!(parse_weight("2,5kg").unwrap(), 2500.0);
        assert_eq!(parse_weight("250g").unwrap(), 250.0);
        assert_eq!(
            parse_weight("85.75g").unwrap(),
            85.75,
            "a scale's reading as typed"
        );
        assert_eq!(parse_weight("-250g").unwrap(), -250.0);
        assert_eq!(parse_weight("2lb").unwrap(), 907.2);
        assert_eq!(parse_weight("12oz").unwrap(), 340.2);
        assert!(parse_weight("5 stone").is_err());
        assert!(parse_weight("lots").is_err());
    }
}
