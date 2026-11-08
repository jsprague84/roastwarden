//! What a roaster's own model predicts: `roast roaster model`, and the prediction, RoR warning and
//! drive lines `roast live` and `review` print.

use std::fmt::Write as _;
use std::time::Duration;

use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{enc, Api};
use crate::out::{code, Failure, Out};

/// A drive's auto DROP for `roast live` (`docs/replay.md`, "Auto DROP"): what it goes by, its
/// state, the seconds to DROP, and the call that cancels it (the person at the roaster cancels
/// on the Roast page; an agent only on virtual_roaster).
pub fn auto_drop_brief(d: &Value) -> Value {
    let a = &d["auto_drop"];
    if !a.is_object() {
        return Value::Null;
    }
    let mut v = json!({
        "by": a["by"], "at": a["at"], "describe": a["describe"], "state": a["state"],
        "eta_seconds": a["eta_seconds"], "waiting_for": a["waiting_for"], "ghost": a["ghost"],
    });
    if d["active"] == json!(true) && matches!(a["state"].as_str(), Some("armed" | "counting")) {
        v["cancel"] = json!(format!(
            "roast api PATCH /api/sessions/{}/replay '{{\"auto_drop\":false}}'",
            d["session_id"].as_str().unwrap_or_default()
        ));
    }
    v
}

/// The auto DROP line under a running drive.
fn auto_drop_line(a: &Value) -> String {
    if !a.is_object() {
        return String::new();
    }
    let what = a["describe"].as_str().unwrap_or("?");
    let eta = a["eta_seconds"].as_f64();
    let state = match a["state"].as_str() {
        Some("counting") => format!("DROP in {:.0} s", eta.unwrap_or(0.0)),
        Some("due") => "DROP now".into(),
        Some("cancelled") => "cancelled for this roast".into(),
        _ => match (a["waiting_for"].as_str(), eta) {
            (Some(w), _) => format!("armed, waits for {w}"),
            (None, Some(e)) if e > 0.0 => {
                format!(
                    "armed, about {}:{:02} to go",
                    (e as i64) / 60,
                    (e as i64) % 60
                )
            }
            _ => "armed".into(),
        },
    };
    let ghost = if a["ghost"] == json!(true) {
        " (ghost: marks nothing)"
    } else {
        ""
    };
    format!("\n  auto DROP {what}: {state}{ghost}")
}

/// `roast live`'s Profile drive line: running, or stopped with what a resume does.
fn mmss(t: f64) -> String {
    let t = t.max(0.0).round() as i64;
    format!("{}:{:02}", t / 60, t % 60)
}

/// The live prediction in two lines: where BT goes, first crack and DROP, and why.
pub fn prediction_lines(p: &Value) -> String {
    if p.is_null() {
        return String::new();
    }
    let n = |k: &str| p[k].as_f64().map_or("?".into(), |v| format!("{v:.1}"));
    let mut line = format!(
        "\n  predicted: BT {} at +1 min, {} ± {} at +2 min",
        n("bt_60"),
        n("bt_120"),
        n("band_120")
    );
    if let Some(j) = p["projection_120"].as_f64() {
        let _ = write!(line, " (projection {j:.1})");
    }
    let mark = |l: &Value, what: &str| {
        let t = l["t"].as_f64()?;
        let range = match (l["low"].as_f64(), l["high"].as_f64()) {
            (Some(a), Some(b)) if b - a >= 2.0 => format!(" ({}–{})", mmss(a), mmss(b)),
            _ => String::new(),
        };
        let bt = l["bt"]
            .as_f64()
            .map(|b| format!(" at {b:.1} °C"))
            .unwrap_or_default();
        Some(format!("; {what} {}{range}{bt}", mmss(t)))
    };
    line.push_str(&mark(&p["first_crack"], "first crack").unwrap_or_default());
    line.push_str(&mark(&p["drop"], "DROP").unwrap_or_default());
    let _ = write!(line, "\n  why: {}", p["reason"].as_str().unwrap_or("?"));
    for (k, what) in [("first_crack", "first crack"), ("drop", "DROP")] {
        if let Some(r) = p[k]["reason"].as_str() {
            let _ = write!(line, "; {what}: {r}");
        }
    }
    line.push_str(&ror_lines(&p["ror"]));
    line
}

/// The RoR watch (docs/prediction.md, "RoR warnings"): the RoR now and ahead, and each
/// warning with its reason.
fn ror_lines(w: &Value) -> String {
    let Some(now) = w["now"].as_f64() else {
        return String::new();
    };
    let mut line = format!("\n  RoR: {now:.1} °C/min now");
    if let Some(tr) = w["trend"].as_f64() {
        let _ = write!(line, " ({tr:+.1} a minute)");
    }
    if let (Some(a), Some(b)) = (w["at_60"].as_f64(), w["at_120"].as_f64()) {
        let _ = write!(line, ", the model {a:.1} at +1 min, {b:.1} at +2 min");
    }
    if w["armed"] == json!(false) {
        line.push_str(" (warnings from DRY end)");
    }
    for x in w["warnings"].as_array().into_iter().flatten() {
        let kind = x["kind"].as_str().unwrap_or("?");
        let when = x["t"].as_f64().map(mmss).unwrap_or_default();
        let how = if x["seen"] == json!(true) {
            format!("seen, from {when}")
        } else {
            format!("expected at {when}")
        };
        let _ = write!(
            line,
            "\n  warning: {kind} ({how}): {}",
            x["reason"].as_str().unwrap_or("?")
        );
    }
    line
}

/// A roast's RoR warnings against what its RoR did, in one line: "flick warned 7:35 for
/// 8:39, came 8:40 (65 s ahead); stall at 9:10, not warned".
pub fn ror_score_text(s: &Value) -> Option<String> {
    let mut parts = Vec::new();
    let warned = s["ror_warnings"].as_array().cloned().unwrap_or_default();
    for w in &warned {
        let kind = w["kind"].as_str().unwrap_or("?");
        let at = w["warned_at"].as_f64().map(mmss).unwrap_or_default();
        let mut p = if w["seen"] == json!(true) {
            format!("{kind} seen {at}")
        } else {
            format!(
                "{kind} warned {at} for {}",
                w["expected_t"].as_f64().map(mmss).unwrap_or_default()
            )
        };
        match (w["came_t"].as_f64(), w["lead_s"].as_f64()) {
            (Some(c), Some(l)) if l > 0.0 => {
                let _ = write!(p, ", came {} ({l:.0} s ahead)", mmss(c));
            }
            (Some(c), _) => {
                let _ = write!(p, ", came {}", mmss(c));
            }
            (None, _) => p.push_str(", did not come"),
        }
        parts.push(p);
    }
    for e in s["ror_events"].as_array().into_iter().flatten() {
        let kind = e["kind"].as_str().unwrap_or("?");
        if warned.iter().all(|w| w["kind"].as_str() != Some(kind)) {
            parts.push(format!(
                "{kind} at {}, not warned",
                e["t"].as_f64().map(mmss).unwrap_or_default()
            ));
        }
    }
    (!parts.is_empty()).then(|| format!("RoR: {}", parts.join("; ")))
}

fn err_text(h: &Value) -> String {
    match (h["p90"].as_f64(), h["rmse"].as_f64()) {
        (Some(p), Some(r)) => format!("±{p:.1} (RMS {r:.1}) °C"),
        _ => "-".into(),
    }
}

/// A roast's prediction score in one line (also `roast review`).
pub fn score_line(s: &Value) -> Option<String> {
    let at = |v: &Value| {
        v.as_array()
            .into_iter()
            .flatten()
            .find(|h| h["horizon_s"] == json!(120))
            .and_then(|h| h["rmse"].as_f64())
    };
    let m = at(&s["model"])?;
    let mut line = format!(
        "2 min ahead RMS {m:.1} °C, projection {} over {} predictions",
        at(&s["projection"]).map_or("-".into(), |v| format!("{v:.1} °C")),
        s["predictions"].as_u64().unwrap_or(0)
    );
    for (k, what) in [("first_crack", "first crack"), ("drop", "DROP")] {
        let l = &s[k];
        if let (Some(p), Some(a), Some(made), Some(e)) = (
            l["predicted_t"].as_f64(),
            l["actual_t"].as_f64(),
            l["made_at_t"].as_f64(),
            l["error_s"].as_f64(),
        ) {
            let _ = write!(
                line,
                "; {what} said {} at {}, came {} ({e:+.0} s)",
                mmss(p),
                mmss(made),
                mmss(a)
            );
        }
    }
    if let Some(r) = ror_score_text(s) {
        let _ = write!(line, "; {r}");
    }
    Some(line)
}

pub async fn roaster_model(
    api: &Api,
    out: &Out,
    roaster: String,
    fit: bool,
) -> Result<u8, Failure> {
    let r = api.roaster(&roaster).await?;
    let device_id = r["device_id"].as_str().unwrap_or_default().to_string();
    let path = format!("/api/roaster/{}/model", enc(&device_id));
    let v = if fit {
        api.send(
            Method::POST,
            &format!("{path}/fit"),
            Some(&json!({})),
            Duration::from_secs(120),
        )
        .await?
        .body
    } else {
        api.get(&path).await?
    };
    out.emit(&v, |v| {
        let m = &v["model"];
        let mut lines = vec![match v["fit"].as_object() {
            Some(f) => format!(
                "{} model: fitted {} on {} roasts ({} s of roasting); lag {:.0} s, gain {:.2} °C/s",
                v["device_id"].as_str().unwrap_or("?"),
                f["fitted_at"].as_str().unwrap_or("?").get(..16).unwrap_or("?").replace('T', " "),
                f["roasts"],
                f["samples"],
                m["tau_s"].as_f64().unwrap_or(0.0),
                m["gain"].as_f64().unwrap_or(0.0)
            ),
            None => format!(
                "{} model: the virtual roaster's (no completed roast with a DROP yet)",
                v["device_id"].as_str().unwrap_or("?")
            ),
        }];
        let modes = v["fit"]["quality"]["modes"].as_array().cloned().unwrap_or_default();
        if !modes.is_empty() {
            lines.push("  2 min ahead, each roast left out of the fit (90 % within, RMS):".into());
            for (mode, what) in [
                ("recorded", "heater and fan as they were"),
                ("plan", "the PID following the plan"),
                ("held", "heater and fan held"),
                ("projection", "today's projection"),
            ] {
                if let Some(h) = modes
                    .iter()
                    .find(|x| x["mode"] == json!(mode))
                    .and_then(|x| x["horizons"].as_array())
                    .and_then(|hs| hs.iter().find(|h| h["horizon_s"] == json!(120)))
                {
                    lines.push(format!("    {what:<30} {}", err_text(h)));
                }
            }
        }
        let settles: Vec<String> = v["settles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                format!(
                    "{:.0} %/{:.0} % {}",
                    s["heater_pct"].as_f64().unwrap_or(0.0),
                    s["fan_pct"].as_f64().unwrap_or(0.0),
                    s["bt"].as_f64().map_or("over 300 °C".into(), |b| format!("{b:.0} °C"))
                )
            })
            .collect();
        lines.push(format!("  BT settles at (heater/fan): {}", settles.join(" · ")));
        let p = &v["pooled"];
        if let (Some(m), Some(j)) = (p["model_120"].as_object(), p["projection_120"].as_object()) {
            lines.push(format!(
                "  live, last {} roasts: 2 min ahead RMS {:.1} °C against the projection's {:.1} °C: {}",
                p["roasts"],
                m["rmse"].as_f64().unwrap_or(0.0),
                j["rmse"].as_f64().unwrap_or(0.0),
                if p["beats_projection"] == json!(true) { "beats it" } else { "not yet" }
            ));
        }
        for r in v["recent"].as_array().into_iter().flatten() {
            if let Some(s) = score_line(r) {
                lines.push(format!(
                    "    {}{}: {s}",
                    r["batch_number"].as_i64().map(|b| format!("#{b} ")).unwrap_or_default(),
                    r["name"].as_str().unwrap_or("?")
                ));
            }
        }
        lines.join("\n")
    });
    Ok(code::OK)
}

pub fn drive_line(profile: &str, d: &Value) -> String {
    let clock = |t: f64| {
        let t = t.max(0.0).round() as i64;
        format!("{}:{:02}", t / 60, t % 60)
    };
    let at = d["profile_t"].as_f64().map_or_else(|| "?".into(), clock);
    if d["active"] == json!(true) {
        let held = d["hold_seconds"]
            .as_f64()
            .map(|h| format!(", resumed {} later", clock(h)))
            .unwrap_or_default();
        return format!(
            "\n  Profile drive: {profile} at profile {at}{held}{}",
            auto_drop_line(&d["auto_drop"])
        );
    }
    let picks_up = if d["stopped_by_restart"] == json!(true) {
        format!("picks up at profile {at}")
    } else {
        "picks up at the profile time now".to_string()
    };
    format!(
        "\n  Profile drive: {profile} stopped at profile {at}: {}\n  resume (heater on, the old settings, {picks_up}): {}",
        d["stopped_reason"].as_str().unwrap_or("?"),
        d["resume"].as_str().unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ror_warnings_live_and_scored_in_lines() {
        let live = json!({"now": 7.4, "trend": -1.5, "at_60": 5.9, "at_120": 4.4, "armed": true,
            "warnings": [{"kind": "flick", "t": 519.0, "seen": false, "since": 455.0,
                "reason": "after first crack the model expects it to rise"}]});
        let l = ror_lines(&live);
        assert!(
            l.contains("RoR: 7.4 °C/min now (-1.5 a minute), the model 5.9 at +1 min"),
            "{l}"
        );
        assert!(
            l.contains("warning: flick (expected at 8:39): after first crack"),
            "{l}"
        );
        assert_eq!(ror_lines(&json!(null)), "");
        let score = json!({"ror_warnings": [{"kind": "flick", "warned_at": 455.0, "expected_t": 519.0,
                "seen": false, "came_t": 520.0, "lead_s": 65.0}],
            "ror_events": [{"kind": "flick", "t": 520.0}, {"kind": "stall", "t": 560.0}]});
        assert_eq!(
            ror_score_text(&score).unwrap(),
            "RoR: flick warned 7:35 for 8:39, came 8:40 (65 s ahead); stall at 9:20, not warned"
        );
        assert!(ror_score_text(&json!({"ror_warnings": [], "ror_events": []})).is_none());
    }

    #[test]
    fn a_drive_line_says_where_a_resume_picks_up() {
        let running = json!({ "active": true, "profile_t": 290.4, "hold_seconds": 40.0 });
        assert_eq!(
            drive_line("Light espresso", &running),
            "\n  Profile drive: Light espresso at profile 4:50, resumed 0:40 later"
        );
        let cut = json!({ "active": false, "profile_t": 290.0, "stopped_by_restart": true,
            "stopped_reason": "the server restarted",
            "resume": "roast api POST /api/sessions/s1/drive/resume" });
        assert_eq!(
            drive_line("Light espresso", &cut),
            "\n  Profile drive: Light espresso stopped at profile 4:50: the server restarted\n  resume (heater on, the old settings, picks up at profile 4:50): roast api POST /api/sessions/s1/drive/resume"
        );
        let counting = json!({ "active": true, "profile_t": 598.0, "auto_drop": {
            "describe": "at the profile's drop, 10:05", "state": "counting", "eta_seconds": 7.2 } });
        assert_eq!(
            drive_line("Light espresso", &counting),
            "\n  Profile drive: Light espresso at profile 9:58\n  auto DROP at the profile's drop, 10:05: DROP in 7 s"
        );
        let waits = json!({ "describe": "2:15 after first crack", "state": "armed",
            "waiting_for": "first crack", "eta_seconds": null });
        assert_eq!(
            auto_drop_line(&waits),
            "\n  auto DROP 2:15 after first crack: armed, waits for first crack"
        );
        let brief = auto_drop_brief(&json!({ "active": true, "session_id": "s1",
            "auto_drop": { "by": "time", "state": "armed" } }));
        assert_eq!(
            brief["cancel"],
            json!("roast api PATCH /api/sessions/s1/replay '{\"auto_drop\":false}'")
        );
        let heater = json!({ "active": false, "profile_t": 290.0, "stopped_by_device": true,
            "stopped_reason": "the roaster cut its heater: emergency stop", "resume": "r" });
        assert!(drive_line("P", &heater).contains("picks up at the profile time now"));
    }
}
