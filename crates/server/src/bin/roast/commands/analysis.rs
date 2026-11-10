//! `roast review` and `roast compare` (docs/cli.md, docs/analysis.md): a roast read, and
//! several roasts lined up against one, from the server's analysis.

use clap::Args;
use serde_json::Value;
use std::fmt::Write as _;

use crate::api::{Api, enc};
use crate::out::{Failure, Out, code};

#[derive(Args, Debug)]
pub struct ReviewArgs {
    /// The roast: its id, `#12` (its batch number) or `latest`
    pub roast: String,
    /// Also line it up against this roast (the reference): `roast compare` for two
    #[arg(long)]
    pub vs: Option<String>,
    /// With `latest` or a batch number: on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// With --vs: how close BT counts as together, °C
    #[arg(long, default_value_t = 0.5)]
    pub within: f64,
}

#[derive(Args, Debug)]
pub struct GuideArgs {
    /// The bean: its id or name
    pub bean: String,
    /// Before its first roast: on this profile (id or name; default the bean's next planned
    /// roast's)
    #[arg(long)]
    pub profile: Option<String>,
    /// On this roaster (default the plan's, the bean's last roast's, else the last one used)
    #[arg(long)]
    pub roaster: Option<String>,
    /// For the next roast after this one (id, `#12` or `latest`; default the bean's newest)
    #[arg(long)]
    pub after: Option<String>,
}

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// 2 to 8 roasts: ids, `#12` (batch numbers) or `latest`; the first is the reference
    /// unless --ref names another
    #[arg(num_args = 2..=8, required = true)]
    pub roasts: Vec<String>,
    /// The roast the others are measured against
    #[arg(long = "ref")]
    pub reference: Option<String>,
    /// With `latest` or batch numbers: on this roaster
    #[arg(long)]
    pub roaster: Option<String>,
    /// How close BT counts as together, °C
    #[arg(long, default_value_t = 0.5)]
    pub within: f64,
    /// Print BT, RoR, heater and fan every 30 s too
    #[arg(long)]
    pub curve: bool,
}

pub async fn review(api: &Api, out: &Out, a: ReviewArgs) -> Result<u8, Failure> {
    let id = super::roasts::roast_id(api, &a.roast, a.roaster.as_deref()).await?;
    match a.vs {
        None => {
            let mut one = api
                .get(&format!("/api/sessions/{}/analysis", enc(&id)))
                .await?;
            // How the live prediction did (docs/prediction.md); roasts before it have none.
            if let Ok(p) = api
                .get(&format!("/api/sessions/{}/prediction", enc(&id)))
                .await
            {
                one["prediction"] = p;
            }
            // A new bean's next roast (docs/new-bean.md), while it has fewer than three
            if let Some(g) = next_roast(api, &id).await {
                one["next_roast"] = g;
            }
            out.emit(&one, |a| {
                let mut t = review_text(a);
                if let Some(line) = super::prediction::score_line(&a["prediction"]) {
                    let _ = write!(t, "\nPrediction: {line}");
                }
                if a["next_roast"]["stage"] == "next" {
                    let _ = write!(
                        t,
                        "\nNext roast of this bean:\n{}",
                        guide_lines(&a["next_roast"])
                    );
                }
                t
            });
        }
        Some(vs) => {
            let base = super::roasts::roast_id(api, &vs, a.roaster.as_deref()).await?;
            let c = api
                .get(&format!(
                    "/api/analysis/compare?roasts={},{}&reference={}&within={}",
                    enc(&base),
                    enc(&id),
                    enc(&base),
                    a.within
                ))
                .await?;
            out.emit(&c, |c| {
                let roast = c["roasts"].get(1).map(review_text).unwrap_or_default();
                format!("{roast}\n\n{}", compare_text(c, false))
            });
        }
    }
    Ok(code::OK)
}

/// The bean's guide after this roast, when the roast has a bean.
async fn next_roast(api: &Api, id: &str) -> Option<Value> {
    let s = api.get(&format!("/api/sessions/{}", enc(id))).await.ok()?;
    let bean = s["bean_id"].as_str()?;
    api.get(&format!("/api/beans/{}/guide?after={}", enc(bean), enc(id)))
        .await
        .ok()
}

/// `roast bean guide`: before a bean's first roast, what to expect; after one, what to change.
pub async fn guide(api: &Api, out: &Out, a: GuideArgs) -> Result<u8, Failure> {
    let b = api.bean(&a.bean).await?;
    let id = b["id"].as_str().unwrap_or_default().to_string();
    let mut q: Vec<String> = vec![];
    if let Some(p) = &a.profile {
        let p = api.profile(p).await?;
        q.push(format!(
            "profile={}",
            enc(p["id"].as_str().unwrap_or_default())
        ));
    }
    let roaster = match &a.roaster {
        Some(r) => Some(
            api.roaster(r).await?["device_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        ),
        None => None,
    };
    if let Some(r) = &roaster {
        q.push(format!("roaster={}", enc(r)));
    }
    if let Some(x) = &a.after {
        let sid = super::roasts::roast_id(api, x, roaster.as_deref()).await?;
        q.push(format!("after={}", enc(&sid)));
    }
    let path = format!(
        "/api/beans/{}/guide{}{}",
        enc(&id),
        if q.is_empty() { "" } else { "?" },
        q.join("&")
    );
    let g = api.get(&path).await?;
    out.emit(&g, guide_text);
    Ok(code::OK)
}

pub async fn compare(api: &Api, out: &Out, a: CompareArgs) -> Result<u8, Failure> {
    let mut ids = Vec::with_capacity(a.roasts.len());
    for r in &a.roasts {
        ids.push(super::roasts::roast_id(api, r, a.roaster.as_deref()).await?);
    }
    let mut path = format!(
        "/api/analysis/compare?roasts={}&within={}",
        ids.iter().map(|i| enc(i)).collect::<Vec<_>>().join(","),
        a.within
    );
    if let Some(r) = &a.reference {
        let r = super::roasts::roast_id(api, r, a.roaster.as_deref()).await?;
        let _ = write!(path, "&reference={}", enc(&r));
    }
    let c = api.get(&path).await?;
    let curve = a.curve;
    out.emit(&c, |c| compare_text(c, curve));
    Ok(code::OK)
}

// ---- text ----

/// A bean's guide: its head and its lines.
pub fn guide_text(g: &Value) -> String {
    let bean = g["bean"].as_str().unwrap_or("?");
    let roaster = g["roaster"].as_str().unwrap_or("no roaster yet");
    let profile = g["profile"]
        .as_str()
        .map_or(" · no profile (say one with --profile)".to_string(), |p| {
            format!(" · {p}")
        });
    let stage = match g["stage"].as_str() {
        Some("before") => "before its first roast".to_string(),
        Some(_) => format!(
            "next roast, after {}",
            g["after"]
                .as_object()
                .map_or_else(|| "its last".into(), |_| title(&g["after"]))
        ),
        None => "?".into(),
    };
    let reads = &g["reads"];
    let mut head = format!("{bean} · {stage} on {roaster}{profile}");
    let _ = write!(
        head,
        "\n  process {}{}",
        reads["process"].as_str().unwrap_or("unknown"),
        if reads["peaberry"] == true {
            ", peaberry"
        } else {
            ""
        }
    );
    format!("{head}\n{}", guide_lines(g))
}

/// The guide's lines: one per rule, hints marked, the profile edit an action offers.
pub fn guide_lines(g: &Value) -> String {
    let mut out: Vec<String> = vec![];
    for l in g["lines"].as_array().into_iter().flatten() {
        let text = l["text"].as_str().unwrap_or("");
        let hint = if l["kind"] == "hint" { "(hint) " } else { "" };
        out.push(format!("  - {hint}{text}"));
        if l["action"] == "auto_drop_development"
            && let Some(p) = g["profile_id"].as_str()
        {
            out.push(format!(
                "    to switch: roast profile edit {p} --auto-drop development"
            ));
        }
    }
    if let Some(t) = g["tasting"].as_str() {
        out.push(format!("  {t}"));
    }
    out.join("\n")
}

fn clock(s: f64) -> String {
    let r = s.round() as i64;
    let sign = if r < 0 { "-" } else { "" };
    format!("{sign}{}:{:02}", r.abs() / 60, r.abs() % 60)
}

fn f(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn signed(v: f64, digits: usize) -> String {
    let s = format!("{v:+.digits$}");
    s.replace('-', "−")
}

/// `#12 Kenya C` or the name.
fn title(r: &Value) -> String {
    let name = r["name"].as_str().unwrap_or("?");
    match r["batch_number"].as_i64() {
        Some(n) => format!("#{n} {name}"),
        None => name.to_string(),
    }
}

fn short(r: &Value) -> String {
    match r["batch_number"].as_i64() {
        Some(n) => format!("#{n}"),
        None => r["session_id"]
            .as_str()
            .unwrap_or("?")
            .chars()
            .take(8)
            .collect(),
    }
}

const MARKS: &[(&str, &str)] = &[
    ("charge", "CHARGE"),
    ("turning_point", "TP"),
    ("drying_end", "DRY"),
    ("first_crack_start", "FC"),
    ("first_crack_end", "FCe"),
    ("second_crack_start", "SC"),
    ("drop", "DROP"),
];

pub fn review_text(r: &Value) -> String {
    let mut lines = vec![format!(
        "{} · {} · {} · {}",
        title(r),
        r["device_id"].as_str().unwrap_or("?"),
        r["roasted_at"]
            .as_str()
            .unwrap_or("")
            .get(..16)
            .unwrap_or("")
            .replace('T', " "),
        r["status"].as_str().unwrap_or("")
    )];
    if let Some(c) = r["coffee"].as_str() {
        lines.push(format!("  coffee {c}"));
    }
    let marks: Vec<String> = MARKS
        .iter()
        .filter_map(|(key, label)| {
            let m = r["marks"].as_array()?.iter().find(|m| m["key"] == *key)?;
            let t = f(&m["t"])?;
            let bt = f(&m["bt"]).map_or(String::new(), |b| format!(" {b:.1} °C"));
            Some(if *key == "charge" {
                format!("{label}{bt}")
            } else {
                format!("{label} {}{bt}", clock(t))
            })
        })
        .collect();
    let mut head = marks.join(" · ");
    if let Some(d) = f(&r["development_s"]) {
        let _ = write!(
            head,
            " · development {}{}",
            clock(d),
            f(&r["dtr_pct"]).map_or(String::new(), |p| format!(" (DTR {p:.1} %)"))
        );
    }
    if let Some(l) = f(&r["loss_pct"]) {
        let _ = write!(head, " · loss {l:.1} %");
    }
    if r["dropped"] != true {
        let _ = write!(
            head,
            " · not dropped ({} so far)",
            clock(f(&r["total_s"]).unwrap_or(0.0))
        );
    }
    lines.push(format!("  {head}"));
    let phases: Vec<String> = r["phases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            format!(
                "{} {} ({:.1} %{})",
                p["key"].as_str().unwrap_or("?"),
                clock(f(&p["duration_s"]).unwrap_or(0.0)),
                f(&p["pct"]).unwrap_or(0.0),
                f(&p["mean_ror"]).map_or(String::new(), |v| format!(", RoR {v:.1}"))
            )
        })
        .collect();
    if !phases.is_empty() {
        lines.push(format!("  phases: {}", phases.join(" · ")));
    }
    let ror = &r["ror"];
    let mut rl = Vec::new();
    if let (Some(v), Some(t)) = (f(&ror["peak"]["value"]), f(&ror["peak"]["t"])) {
        rl.push(format!("peak {v:.1} at {}", clock(t)));
    }
    let minutes: Vec<String> = ror["by_minute"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some(format!("{} {:.1}", clock(f(&p["t"])?), f(&p["value"])?)))
        .collect();
    if !minutes.is_empty() {
        rl.push(minutes.join(", "));
    }
    if let (Some(v), Some(t)) = (f(&ror["late_min"]["value"]), f(&ror["late_min"]["t"])) {
        rl.push(format!("late low {v:.1} at {}", clock(t)));
    }
    if let Some(v) = f(&ror["at_drop"]) {
        rl.push(format!("at drop {v:.1}"));
    }
    if let Some(v) = f(&ror["fc_drop_slope"]) {
        rl.push(format!("FC→drop {} /min", signed(v, 1)));
    }
    if !rl.is_empty() {
        lines.push(format!("  RoR °C/min: {}", rl.join(" · ")));
    }
    // Declared channels at CHARGE, first crack and DROP (docs/channels.md)
    for c in r["channels"].as_array().into_iter().flatten() {
        let unit = match c["unit"].as_str().unwrap_or("") {
            "C" => " °C".to_string(),
            u => format!(" {u}"),
        };
        let v = |k: &str| f(&c[k]).map_or("–".to_string(), |x| format!("{x:.1}{unit}"));
        lines.push(format!(
            "  {} ({}): CHARGE {} · FC {} · DROP {}",
            c["label"].as_str().unwrap_or("?"),
            c["key"].as_str().unwrap_or("?"),
            v("charge"),
            v("first_crack"),
            v("drop")
        ));
    }
    let p = &r["profile"];
    if p.is_object() {
        let mut pl = Vec::new();
        let early: Vec<String> = p["early"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| {
                Some(format!(
                    "{} {}",
                    clock(f(&e["t"])?),
                    signed(f(&e["value"])?, 1)
                ))
            })
            .collect();
        if !early.is_empty() {
            pl.push(format!("BT − profile {}", early.join(", ")));
        }
        if let (Some(v), Some(t)) = (f(&p["early_peak"]["value"]), f(&p["early_peak"]["t"])) {
            pl.push(format!("early peak {} at {}", signed(v, 1), clock(t)));
        }
        if let Some(rmse) = f(&p["rmse"]) {
            pl.push(format!(
                "from TP RMS {rmse:.2}{}{}",
                f(&p["bias"]).map_or(String::new(), |b| format!(", bias {}", signed(b, 2))),
                f(&p["max_dev"]).map_or(String::new(), |m| format!(", max {m:.1}"))
            ));
        }
        if let Some(d) = f(&p["fc_vs_profile_s"]) {
            pl.push(format!("FC {} s", signed(d, 0)));
        }
        if let Some(d) = f(&p["total_vs_profile_s"]) {
            pl.push(format!("drop {} s", signed(d, 0)));
        }
        lines.push(format!(
            "  profile \"{}\": {}",
            p["profile"].as_str().unwrap_or("?"),
            pl.join(" · ")
        ));
    }
    let c = &r["control"];
    let mut cl = Vec::new();
    if let Some(h) = f(&c["heater_opening"]) {
        cl.push(format!("{h:.0} % first 30 s"));
    }
    if let (Some(m), Some(x)) = (f(&c["heater_mean"]), f(&c["heater_max"])) {
        cl.push(format!("mean {m:.0}, max {x:.0}"));
    }
    if let Some(h) = f(&c["heater_high_s"]).filter(|h| *h > 0.0) {
        cl.push(format!("≥ 90 % for {}", clock(h)));
    }
    if let Some(h) = f(&c["late_heater"]) {
        cl.push(format!(
            "last minute {h:.0} %{}",
            f(&c["late_fan"]).map_or(String::new(), |v| format!(" (fan {v:.0} %)"))
        ));
    }
    if let Some(s) = f(&c["heater_step_per_s"]) {
        cl.push(format!("steps {s:.2} %/s"));
    }
    if let (Some(b), Some(sd)) = (f(&c["setpoint"]["bias"]), f(&c["setpoint"]["sd"])) {
        cl.push(format!("BT − setpoint {} ± {sd:.1}", signed(b, 1)));
    }
    if !cl.is_empty() {
        lines.push(format!("  heater: {}", cl.join(" · ")));
    }
    let ex: Vec<String> = r["expected"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|j| {
            let key = j["key"].as_str().unwrap_or("");
            let show = |v: f64| match key {
                "total_s" | "fc_s" | "development_s" => clock(v),
                "color_ground" => format!("{v:.0}"),
                _ => format!("{v:.1}"),
            };
            let unit = match key {
                "weight_loss_pct" => " %",
                "drop_bt" => " °C",
                _ => "",
            };
            format!(
                "{} {}{unit} {} {} {}–{}{unit}",
                j["label"].as_str().unwrap_or("?"),
                f(&j["value"]).map_or("—".into(), show),
                j["verdict"].as_str().unwrap_or("unjudged"),
                match j["source"].as_str() {
                    Some("history") => "usual",
                    Some(s) => s,
                    None => "",
                },
                f(&j["lo"]).map_or("?".into(), show),
                f(&j["hi"]).map_or("?".into(), show),
            )
        })
        .collect();
    if !ex.is_empty() {
        lines.push(format!("  expected: {}", ex.join(" · ")));
    }
    for n in r["notes"].as_array().into_iter().flatten() {
        if let Some(n) = n.as_str() {
            lines.push(format!("  - {n}"));
        }
    }
    lines.join("\n")
}

fn cell(unit: &str, v: Option<f64>) -> String {
    match v {
        None => "—".into(),
        Some(v) if unit == "s" => clock(v),
        Some(v) if unit == "°C/min/min" => format!("{v:.2}").replace('-', "−"),
        Some(v) => format!("{v:.1}").replace('-', "−"),
    }
}

fn delta(unit: &str, d: Option<f64>) -> String {
    let digits = match unit {
        "s" => 0,
        "°C/min/min" => 2,
        _ => 1,
    };
    match d {
        None => String::new(),
        // Nothing to show at the digits shown: not "−0.0"
        Some(d) if (d * 10f64.powi(digits as i32)).round() == 0.0 => " (=)".into(),
        Some(d) if unit == "s" => format!(" ({} s)", signed(d, 0)),
        Some(d) => format!(" ({})", signed(d, digits)),
    }
}

pub fn compare_text(c: &Value, curve: bool) -> String {
    let roasts: Vec<&Value> = c["roasts"].as_array().into_iter().flatten().collect();
    let reference = c["reference"].as_str().unwrap_or("");
    let rk = roasts
        .iter()
        .position(|r| r["session_id"] == reference)
        .unwrap_or(0);
    let mut lines = vec![format!(
        "{} against {}",
        roasts
            .iter()
            .enumerate()
            .filter(|(k, _)| *k != rk)
            .map(|(_, r)| short(r))
            .collect::<Vec<_>>()
            .join(", "),
        title(roasts.get(rk).copied().unwrap_or(&Value::Null)),
    )];
    let label_w = 38;
    let col_w = 16;
    let mut head = format!("  {:<label_w$}", "");
    for r in &roasts {
        let _ = write!(head, "{:>col_w$}", short(r));
    }
    lines.push(head);
    for row in c["rows"].as_array().into_iter().flatten() {
        let unit = row["unit"].as_str().unwrap_or("");
        let mut line = format!(
            "  {:<label_w$}",
            format!(
                "{} {}",
                row["label"].as_str().unwrap_or("?"),
                unit_tag(unit)
            )
        );
        for (k, v) in row["values"].as_array().into_iter().flatten().enumerate() {
            let d = if k == rk { None } else { f(&row["deltas"][k]) };
            let _ = write!(
                line,
                "{:>col_w$}",
                format!("{}{}", cell(unit, f(v)), delta(unit, d))
            );
        }
        lines.push(line);
    }
    let within = f(&c["within"]).unwrap_or(0.5);
    for o in c["overlay"].as_array().into_iter().flatten() {
        let who = roasts
            .iter()
            .find(|r| r["session_id"] == o["session_id"])
            .map_or("?".into(), |r| short(r));
        let from =
            f(&o["within_from_s"]).map_or("never settles".into(), |t| format!("from {}", clock(t)));
        let max = match (f(&o["max_bt_delta"]["value"]), f(&o["max_bt_delta"]["t"])) {
            (Some(v), Some(t)) => format!(", largest {} °C at {}", signed(v, 1), clock(t)),
            _ => String::new(),
        };
        lines.push(format!(
            "  {who}: BT within {within} °C of {} {from} (RMS {}{max})",
            short(roasts[rk]),
            f(&o["rms_bt_delta"]).map_or("—".into(), |v| format!("{v:.2}"))
        ));
    }
    if curve {
        lines.push(String::new());
        let mut head = format!("  {:>5}", "t");
        for r in &roasts {
            let _ = write!(head, "{:>24}", format!("{} BT/RoR/H/F", short(r)));
        }
        lines.push(head);
        for p in c["curve"].as_array().into_iter().flatten() {
            let mut line = format!("  {:>5}", clock(f(&p["t"]).unwrap_or(0.0)));
            for k in 0..roasts.len() {
                let g = |key: &str, d: usize| {
                    f(&p[key][k]).map_or("—".into(), |v| format!("{v:.d$}"))
                };
                let _ = write!(
                    line,
                    "{:>24}",
                    format!(
                        "{}/{}/{}/{}",
                        g("bt", 1),
                        g("ror", 1),
                        g("heater", 0),
                        g("fan", 0)
                    )
                );
            }
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn unit_tag(unit: &str) -> &str {
    match unit {
        "s" | "" => "",
        u => u,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_difference_too_small_to_show_is_equal() {
        assert_eq!(delta("°C", Some(-0.04)), " (=)");
        assert_eq!(delta("°C", Some(-0.06)), " (−0.1)");
        assert_eq!(delta("s", Some(0.4)), " (=)");
        assert_eq!(delta("s", Some(-10.0)), " (−10 s)");
        assert_eq!(delta("°C/min/min", Some(0.004)), " (=)");
        assert_eq!(delta("°C/min/min", Some(0.29)), " (+0.29)");
    }

    #[test]
    fn a_review_prints_each_declared_channel_at_the_marks() {
        let r = serde_json::json!({ "name": "r", "device_id": "tc4", "status": "completed",
            "marks": [], "phases": [], "ror": {}, "channels": [
                { "key": "t3", "label": "Exhaust", "unit": "C",
                  "charge": 150.0, "first_crack": 195.04, "drop": null },
                { "key": "gas", "label": "Gas", "unit": "kPa",
                  "charge": 2.5, "first_crack": 1.5, "drop": 1.0 }
            ] });
        let t = review_text(&r);
        assert!(
            t.contains("Exhaust (t3): CHARGE 150.0 °C · FC 195.0 °C · DROP –"),
            "{t}"
        );
        assert!(t.contains("Gas (gas): CHARGE 2.5 kPa · FC 1.5 kPa · DROP 1.0 kPa"));
    }
}
