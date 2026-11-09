//! Autotune runs and stored PID gain changes (`docs/autotune-2.md`, "Stored gains and undo").
//!
//! Runs are the server's Autotune 2 (`docs/autotune-2.md`) and, from before firmware 3.5.0,
//! the roaster's own relay test, kept readable. A firmware run reported the ultimate gain
//! `ku` (device PID units, 0..255 output per °C) and period `tu` (s); every tuning rule is a
//! fixed function of those two, so the server computes all of them here, with the same
//! formulas and gain clamps as the firmware's `gainsFor` had.
//!
//! Every stored gain change goes to `pid_changes` with the gains it replaced, which is what
//! undo restores. The newest row is the gains the server holds for the roaster: firmware
//! 3.5.0 keeps gains in RAM only, and the scheduler sends them after a boot
//! (`schedule.rs`, `GainScheduler::on_sample`).

use chrono::{DateTime, Utc};
use roastwarden_autotune::analysis::{Analysis, Quality};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::AssertSqlSafe;
use sqlx::{Row, SqlitePool};

/// PID gains in device units (Kp per °C on the 0..255 output, Ki per °C·s,
/// Kd per °C/s), as `control/pid` takes them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Gains {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
}

impl Gains {
    pub fn is_valid(&self) -> bool {
        [self.kp, self.ki, self.kd]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
    }
}

pub struct Rule {
    pub id: &'static str,
    pub label: &'static str,
    /// No derivative term (better with 1 Hz MAX6675 samples).
    pub pi: bool,
}

/// The firmware's rules, in the order the dashboard lists them.
pub const RULES: [Rule; 6] = [
    Rule {
        id: "tyreus_luyben_pi",
        label: "Tyreus–Luyben PI",
        pi: true,
    },
    Rule {
        id: "zn_pi",
        label: "Ziegler–Nichols PI",
        pi: true,
    },
    Rule {
        id: "tyreus_luyben",
        label: "Tyreus–Luyben PID",
        pi: false,
    },
    Rule {
        id: "zn_no_overshoot",
        label: "Z–N no overshoot",
        pi: false,
    },
    Rule {
        id: "zn_some_overshoot",
        label: "Z–N some overshoot",
        pi: false,
    },
    Rule {
        id: "zn_classic",
        label: "Z–N classic",
        pi: false,
    },
];

pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// Gains a rule gives for one run.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct RuleGains {
    pub rule: &'static str,
    pub label: &'static str,
    pub pi: bool,
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    /// A gain hit the firmware's limit and was clamped.
    pub clamped: bool,
}

impl RuleGains {
    pub fn gains(&self) -> Gains {
        Gains {
            kp: self.kp,
            ki: self.ki,
            kd: self.kd,
        }
    }
}

/// Firmware gain limits (`autotune.cpp` `finishRun`).
const KP_RANGE: (f64, f64) = (0.1, 100.0);
const KI_RANGE: (f64, f64) = (0.0, 10.0);
const KD_RANGE: (f64, f64) = (0.0, 200.0);

/// Parallel-form gains for a rule: `Ki = Kp / Ti`, `Kd = Kp · Td`, clamped
/// like the firmware clamps them.
pub fn gains_for(rule: &'static Rule, ku: f64, tu: f64) -> Option<RuleGains> {
    if !(ku.is_finite() && tu.is_finite() && ku > 0.0 && tu > 0.0) {
        return None;
    }
    let (p, ti, td) = match rule.id {
        "tyreus_luyben" => (ku / 2.2, 2.2 * tu, tu / 6.3),
        "zn_some_overshoot" => (0.33 * ku, tu / 2.0, tu / 3.0),
        "zn_no_overshoot" => (0.2 * ku, tu / 2.0, tu / 3.0),
        "zn_pi" => (0.45 * ku, tu / 1.2, 0.0),
        "tyreus_luyben_pi" => (ku / 3.2, 2.2 * tu, 0.0),
        _ => (0.6 * ku, tu / 2.0, tu / 8.0), // zn_classic
    };
    let mut clamped = false;
    let mut clamp = |v: f64, (lo, hi): (f64, f64)| {
        if v < lo || v > hi {
            clamped = true;
        }
        v.clamp(lo, hi)
    };
    let kp = clamp(p, KP_RANGE);
    let ki = clamp(if ti > 0.0 { p / ti } else { 0.0 }, KI_RANGE);
    let kd = clamp(p * td, KD_RANGE);
    Some(RuleGains {
        rule: rule.id,
        label: rule.label,
        pi: rule.pi,
        kp,
        ki,
        kd,
        clamped,
    })
}

pub fn all_rules(ku: f64, tu: f64) -> Vec<RuleGains> {
    RULES.iter().filter_map(|r| gains_for(r, ku, tu)).collect()
}

/// One autotune results message, as stored.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AutotuneRun {
    pub id: String,
    pub device_id: String,
    pub created_at: DateTime<Utc>,
    pub state: String,
    pub quality: Option<String>,
    pub message: Option<String>,
    pub fan_pct: Option<f64>,
    pub target_temperature: Option<f64>,
    pub test_temperature: Option<f64>,
    pub ku: Option<f64>,
    pub tu: Option<f64>,
    pub bias: Option<f64>,
    pub tuning_method: Option<String>,
    pub warnings: Vec<String>,
    /// The firmware's results message; for a server run, the experiment's summary (its
    /// switches and settings).
    pub payload: Value,
    /// `firmware` (the roaster's own relay test) or `server` (Autotune 2, docs/autotune-2.md).
    pub source: String,
}

impl AutotuneRun {
    /// Build a run from the firmware's `autotune/results` payload (firmware 3.2 to 3.4; the
    /// onboard autotune is gone, its stored runs stay readable). Tests make firmware runs with it.
    #[cfg(test)]
    pub fn from_results(device_id: &str, payload: &Value) -> AutotuneRun {
        let num = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
        };
        let text = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        AutotuneRun {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            created_at: Utc::now(),
            state: text("state").unwrap_or_else(|| "complete".into()),
            quality: text("quality"),
            message: text("message"),
            // Firmware 3.3+ reports the fan in %.
            fan_pct: num("fan").filter(|f| (0.0..=100.0).contains(f)),
            target_temperature: num("target_temperature"),
            test_temperature: num("test_temperature"),
            ku: num("ku").or_else(|| num("ultimate_gain")),
            tu: num("tu").or_else(|| num("oscillation_period")),
            bias: num("bias"),
            tuning_method: text("tuning_method"),
            warnings: payload
                .get("warnings")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|w| w.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            payload: payload.clone(),
            source: "firmware".into(),
        }
    }

    pub fn is_server(&self) -> bool {
        self.source == "server"
    }

    /// Whether gains from this run may be applied: it completed, the
    /// firmware did not grade it poor, and Ku/Tu are usable.
    pub fn usable(&self) -> Result<(f64, f64), String> {
        if self.state != "complete" {
            return Err(format!(
                "the run {}{}",
                self.state,
                self.message
                    .as_deref()
                    .map(|m| format!(": {m}"))
                    .unwrap_or_default()
            ));
        }
        if self.quality.as_deref() == Some("poor") {
            return Err("the run was graded poor".into());
        }
        match (self.ku, self.tu) {
            (Some(ku), Some(tu)) if ku > 0.0 && tu > 0.0 => Ok((ku, tu)),
            _ => Err("the run reported no ultimate gain and period".into()),
        }
    }

    fn gains_at(&self, prefix: &str) -> Option<Gains> {
        let g = |k: &str| {
            self.payload
                .get(format!("{prefix}_{k}"))
                .and_then(Value::as_f64)
        };
        let g = Gains {
            kp: g("kp")?,
            ki: g("ki")?,
            kd: g("kd")?,
        };
        g.is_valid().then_some(g)
    }

    /// The API view: the run plus gains for every rule (a firmware run), or its analysis (a
    /// server run: the designs are in it).
    pub fn view(&self, analysis: Option<Analysis>) -> AutotuneRunView {
        if self.is_server() {
            let usable = server_usable(self, analysis.as_ref());
            return AutotuneRunView {
                run: self.clone(),
                usable: usable.is_ok(),
                unusable_reason: usable.err(),
                rules: Vec::new(),
                recommended: analysis
                    .as_ref()
                    .and_then(|a| a.designs.get(a.recommended?))
                    .map(|d| device_gains(&d.pid)),
                original: None,
                analysis,
            };
        }
        let usable = self.usable();
        AutotuneRunView {
            run: self.clone(),
            usable: usable.is_ok(),
            unusable_reason: usable.err(),
            rules: match (self.ku, self.tu) {
                (Some(ku), Some(tu)) => all_rules(ku, tu),
                _ => Vec::new(),
            },
            recommended: self.gains_at("recommended"),
            original: self.gains_at("original"),
            analysis: None,
        }
    }
}

/// A run with what can be done with it, as the API shows it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AutotuneRunView {
    #[serde(flatten)]
    pub run: AutotuneRun,
    /// Its Ku and Tu can give gains
    pub usable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unusable_reason: Option<String>,
    /// The gains each tuning rule gives
    pub rules: Vec<RuleGains>,
    /// The firmware's recommended gains
    pub recommended: Option<Gains>,
    /// The gains in use before the run
    pub original: Option<Gains>,
    /// A server run's analysis: models, checks, designs (docs/autotune-2.md)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Analysis>,
}

/// Whether a server run's designs may be applied: the test finished and the analysis is not
/// poor.
pub fn server_usable(run: &AutotuneRun, analysis: Option<&Analysis>) -> Result<(), String> {
    if run.state != "complete" {
        return Err(format!(
            "the test {}{}",
            run.state,
            run.message
                .as_deref()
                .map(|m| format!(": {m}"))
                .unwrap_or_default()
        ));
    }
    let Some(a) = analysis else {
        return Err("the run has no analysis".into());
    };
    if a.quality == Quality::Poor {
        return Err("the analysis was graded poor".into());
    }
    if a.design_model.is_none() {
        return Err("no model to design on".into());
    }
    Ok(())
}

/// PI gains in process units (% per °C) as the firmware takes them (0–255 per °C), D off.
pub fn device_gains(pid: &roastwarden_autotune::Pid) -> Gains {
    Gains {
        kp: pid.kp * 2.55,
        ki: pid.ki * 2.55,
        kd: 0.0,
    }
}

/// A run without a record (a firmware run; tests).
#[cfg(test)]
pub async fn insert_run(db: &SqlitePool, run: &AutotuneRun) -> Result<(), sqlx::Error> {
    insert_with(db, run, None, None).await
}

/// A server run, with its record and analysis.
pub async fn insert_server_run(
    db: &SqlitePool,
    stored: &crate::control::tune::StoredRun,
) -> Result<(), sqlx::Error> {
    insert_with(
        db,
        &stored.run,
        serde_json::to_string(&stored.samples).ok(),
        stored
            .analysis
            .as_ref()
            .and_then(|a| serde_json::to_string(a).ok()),
    )
    .await
}

async fn insert_with(
    db: &SqlitePool,
    run: &AutotuneRun,
    samples: Option<String>,
    analysis: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO autotune_runs (id, device_id, created_at, state, quality, message,
            fan_pct, target_temperature, test_temperature, ku, tu, bias, tuning_method, warnings, payload,
            source, samples, analysis)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&run.id)
    .bind(&run.device_id)
    .bind(run.created_at)
    .bind(&run.state)
    .bind(&run.quality)
    .bind(&run.message)
    .bind(run.fan_pct)
    .bind(run.target_temperature)
    .bind(run.test_temperature)
    .bind(run.ku)
    .bind(run.tu)
    .bind(run.bias)
    .bind(&run.tuning_method)
    .bind(serde_json::to_string(&run.warnings).unwrap_or_else(|_| "[]".into()))
    .bind(run.payload.to_string())
    .bind(&run.source)
    .bind(samples)
    .bind(analysis)
    .execute(db)
    .await
    .map(|_| ())
}

fn run_from_row(r: &sqlx::sqlite::SqliteRow) -> AutotuneRun {
    let warnings: String = r.get("warnings");
    let payload: String = r.get("payload");
    AutotuneRun {
        id: r.get("id"),
        device_id: r.get("device_id"),
        created_at: r.get("created_at"),
        state: r.get("state"),
        quality: r.get("quality"),
        message: r.get("message"),
        fan_pct: r.get("fan_pct"),
        target_temperature: r.get("target_temperature"),
        test_temperature: r.get("test_temperature"),
        ku: r.get("ku"),
        tu: r.get("tu"),
        bias: r.get("bias"),
        tuning_method: r.get("tuning_method"),
        warnings: serde_json::from_str(&warnings).unwrap_or_default(),
        payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
        source: r.get("source"),
    }
}

/// A row's analysis (server runs).
fn analysis_from_row(r: &sqlx::sqlite::SqliteRow) -> Option<Analysis> {
    let raw: Option<String> = r.try_get("analysis").ok().flatten();
    serde_json::from_str(raw.as_deref()?).ok()
}

/// The columns a list reads: everything but the record.
const RUN_COLUMNS: &str = "id, device_id, created_at, state, quality, message, fan_pct,
    target_temperature, test_temperature, ku, tu, bias, tuning_method, warnings, payload, source, analysis";

/// Newest first.
/// Newest first, each with its analysis (server runs).
pub async fn list_runs_with_analysis(
    db: &SqlitePool,
    device_id: &str,
    limit: i64,
) -> Result<Vec<(AutotuneRun, Option<Analysis>)>, sqlx::Error> {
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM autotune_runs WHERE device_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?"
    )))
    .bind(device_id)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (run_from_row(r), analysis_from_row(r)))
        .collect())
}

pub async fn get_run(
    db: &SqlitePool,
    device_id: &str,
    run_id: &str,
) -> Result<Option<AutotuneRun>, sqlx::Error> {
    Ok(get_run_with_analysis(db, device_id, run_id)
        .await?
        .map(|(r, _)| r))
}

pub async fn get_run_with_analysis(
    db: &SqlitePool,
    device_id: &str,
    run_id: &str,
) -> Result<Option<(AutotuneRun, Option<Analysis>)>, sqlx::Error> {
    let row = sqlx::query(AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM autotune_runs WHERE device_id = ? AND id = ?"
    )))
    .bind(device_id)
    .bind(run_id)
    .fetch_optional(db)
    .await?;
    Ok(row
        .as_ref()
        .map(|r| (run_from_row(r), analysis_from_row(r))))
}

/// A server run's record.
pub async fn get_samples(
    db: &SqlitePool,
    device_id: &str,
    run_id: &str,
) -> Result<Option<crate::control::tune::Samples>, sqlx::Error> {
    let raw: Option<Option<String>> =
        sqlx::query_scalar("SELECT samples FROM autotune_runs WHERE device_id = ? AND id = ?")
            .bind(device_id)
            .bind(run_id)
            .fetch_optional(db)
            .await?;
    Ok(raw.flatten().and_then(|s| serde_json::from_str(&s).ok()))
}

/// Replace a server run's analysis (after analysing its record again).
pub async fn set_analysis(
    db: &SqlitePool,
    run: &AutotuneRun,
    analysis: &Analysis,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE autotune_runs SET analysis = ?, quality = ?, warnings = ?, ku = ?, tu = ?, bias = ?
         WHERE device_id = ? AND id = ?",
    )
    .bind(serde_json::to_string(analysis).ok())
    .bind(&run.quality)
    .bind(serde_json::to_string(&run.warnings).unwrap_or_else(|_| "[]".into()))
    .bind(run.ku)
    .bind(run.tu)
    .bind(run.bias)
    .bind(&run.device_id)
    .bind(&run.id)
    .execute(db)
    .await
    .map(|_| ())
}

/// One stored gain change.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PidChange {
    pub id: i64,
    pub device_id: String,
    pub created_at: DateTime<Utc>,
    pub gains: Gains,
    pub previous: Option<Gains>,
    pub source: String,
    pub run_id: Option<String>,
    pub rule: Option<String>,
    pub undone: bool,
}

pub struct NewPidChange<'a> {
    pub device_id: &'a str,
    pub gains: Gains,
    pub previous: Option<Gains>,
    pub source: &'a str,
    pub run_id: Option<&'a str>,
    pub rule: Option<&'a str>,
}

pub async fn record_change(db: &SqlitePool, c: NewPidChange<'_>) -> Result<i64, sqlx::Error> {
    let r = sqlx::query(
        "INSERT INTO pid_changes (device_id, created_at, kp, ki, kd, prev_kp, prev_ki, prev_kd, source, run_id, rule)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(c.device_id)
    .bind(Utc::now())
    .bind(c.gains.kp)
    .bind(c.gains.ki)
    .bind(c.gains.kd)
    .bind(c.previous.map(|g| g.kp))
    .bind(c.previous.map(|g| g.ki))
    .bind(c.previous.map(|g| g.kd))
    .bind(c.source)
    .bind(c.run_id)
    .bind(c.rule)
    .execute(db)
    .await?;
    Ok(r.last_insert_rowid())
}

fn change_from_row(r: &sqlx::sqlite::SqliteRow) -> PidChange {
    let prev = match (
        r.get::<Option<f64>, _>("prev_kp"),
        r.get::<Option<f64>, _>("prev_ki"),
        r.get::<Option<f64>, _>("prev_kd"),
    ) {
        (Some(kp), Some(ki), Some(kd)) => Some(Gains { kp, ki, kd }),
        _ => None,
    };
    PidChange {
        id: r.get("id"),
        device_id: r.get("device_id"),
        created_at: r.get("created_at"),
        gains: Gains {
            kp: r.get("kp"),
            ki: r.get("ki"),
            kd: r.get("kd"),
        },
        previous: prev,
        source: r.get("source"),
        run_id: r.get("run_id"),
        rule: r.get("rule"),
        undone: r.get::<i64, _>("undone") != 0,
    }
}

/// Newest first.
pub async fn list_changes(
    db: &SqlitePool,
    device_id: &str,
    limit: i64,
) -> Result<Vec<PidChange>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT * FROM pid_changes WHERE device_id = ? ORDER BY id DESC LIMIT ?")
            .bind(device_id)
            .bind(limit)
            .fetch_all(db)
            .await?;
    Ok(rows.iter().map(change_from_row).collect())
}

/// The change an undo reverts: the newest one that is neither an undo nor
/// already undone. Undoing repeatedly walks back through the history.
pub async fn last_undoable(
    db: &SqlitePool,
    device_id: &str,
) -> Result<Option<PidChange>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT * FROM pid_changes WHERE device_id = ? AND undone = 0 AND source != 'undo'
         ORDER BY id DESC LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await?;
    Ok(row.as_ref().map(change_from_row))
}

pub async fn mark_undone(db: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE pid_changes SET undone = 1 WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn rules_match_the_firmware_formulas_and_clamps() {
        // Ku 20 (0..255 per °C), Tu 60 s.
        let tl = gains_for(rule("tyreus_luyben_pi").unwrap(), 20.0, 60.0).unwrap();
        assert!(close(tl.kp, 6.25));
        assert!(close(tl.ki, 6.25 / 132.0));
        assert_eq!(tl.kd, 0.0);
        assert!(!tl.clamped && tl.pi);

        let zn = gains_for(rule("zn_classic").unwrap(), 20.0, 60.0).unwrap();
        assert!(close(zn.kp, 12.0));
        assert!(close(zn.ki, 0.4));
        assert!(close(zn.kd, 90.0));

        let zpi = gains_for(rule("zn_pi").unwrap(), 20.0, 60.0).unwrap();
        assert!(close(zpi.kp, 9.0) && close(zpi.ki, 0.18));

        // Kd over 200 is clamped and flagged.
        let big = gains_for(rule("zn_classic").unwrap(), 50.0, 120.0).unwrap();
        assert_eq!(big.kd, 200.0);
        assert!(big.clamped);

        assert_eq!(all_rules(20.0, 60.0).len(), RULES.len());
        assert!(all_rules(0.0, 60.0).is_empty());
        assert!(gains_for(rule("zn_pi").unwrap(), 20.0, f64::NAN).is_none());
    }

    #[test]
    fn results_message_becomes_a_run() {
        let payload = json!({
            "state": "complete", "quality": "acceptable", "warnings": ["asymmetric"],
            "recommended_kp": 6.25, "recommended_ki": 0.047, "recommended_kd": 0.0,
            "original_kp": 18.79, "original_ki": 0.386, "original_kd": 66.05,
            "target_temperature": 200.0, "test_temperature": 200.0,
            "tuning_method": "tyreus_luyben_pi", "ku": 20.0, "tu": 60.0,
            "bias": 61.5, "fan": 74.9
        });
        let run = AutotuneRun::from_results("r1", &payload);
        assert_eq!(run.fan_pct, Some(74.9));
        assert_eq!(run.usable().unwrap(), (20.0, 60.0));
        assert_eq!(run.warnings, vec!["asymmetric".to_string()]);
        let v = serde_json::to_value(run.view(None)).unwrap();
        assert_eq!(v["usable"], true);
        assert_eq!(v["rules"].as_array().unwrap().len(), 6);
        assert_eq!(v["original"]["kp"], 18.79);
        assert_eq!(v["recommended"]["kp"], 6.25);

        let failed = AutotuneRun::from_results(
            "r1",
            &json!({"state": "failed", "quality": "poor", "message": "amplitude too small",
                    "recommended_kp": null, "ku": 0.0}),
        );
        let why = failed.usable().unwrap_err();
        assert!(
            why.contains("failed") && why.contains("amplitude too small"),
            "{why}"
        );
        assert_eq!(
            serde_json::to_value(failed.view(None)).unwrap()["recommended"],
            Value::Null
        );

        // Older firmware: ultimate_gain/oscillation_period, no fan.
        let old = AutotuneRun::from_results(
            "r1",
            &json!({"quality": "good", "ultimate_gain": 30.0, "oscillation_period": 45.0}),
        );
        assert_eq!(
            (old.ku, old.tu, old.fan_pct),
            (Some(30.0), Some(45.0), None)
        );
        assert!(old.usable().is_ok());
    }

    #[tokio::test]
    async fn runs_and_changes_round_trip_and_undo_walks_back() {
        let db = crate::db::open_memory().await;
        let run = AutotuneRun::from_results(
            "r1",
            &json!({"state": "complete", "quality": "good", "ku": 20.0, "tu": 60.0, "fan": 100.0}),
        );
        insert_run(&db, &run).await.unwrap();
        let mut server = AutotuneRun::from_results("r1", &json!({"state": "failed"}));
        server.source = "server".into();
        insert_run(&db, &server).await.unwrap();
        let got = get_run(&db, "r1", &run.id).await.unwrap().unwrap();
        assert_eq!(got.fan_pct, Some(100.0));
        assert_eq!(
            list_runs_with_analysis(&db, "r1", 10).await.unwrap().len(),
            2,
            "the firmware run and the server run"
        );
        assert!(get_run(&db, "r2", &run.id).await.unwrap().is_none());

        let a = Gains {
            kp: 18.79,
            ki: 0.386,
            kd: 66.05,
        };
        let b = Gains {
            kp: 6.25,
            ki: 0.047,
            kd: 0.0,
        };
        let c = Gains {
            kp: 9.0,
            ki: 0.18,
            kd: 0.0,
        };
        record_change(
            &db,
            NewPidChange {
                device_id: "r1",
                gains: b,
                previous: Some(a),
                source: "autotune",
                run_id: Some(&run.id),
                rule: Some("tyreus_luyben_pi"),
            },
        )
        .await
        .unwrap();
        record_change(
            &db,
            NewPidChange {
                device_id: "r1",
                gains: c,
                previous: Some(b),
                source: "manual",
                run_id: None,
                rule: None,
            },
        )
        .await
        .unwrap();

        let last = last_undoable(&db, "r1").await.unwrap().unwrap();
        assert_eq!(last.previous, Some(b));
        mark_undone(&db, last.id).await.unwrap();
        record_change(
            &db,
            NewPidChange {
                device_id: "r1",
                gains: b,
                previous: Some(c),
                source: "undo",
                run_id: None,
                rule: None,
            },
        )
        .await
        .unwrap();
        let next = last_undoable(&db, "r1").await.unwrap().unwrap();
        assert_eq!(next.source, "autotune");
        assert_eq!(next.previous, Some(a));
        let all = list_changes(&db, "r1", 10).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].source, "undo");
        assert!(all[1].undone);
    }
}
