//! Does a roaster meet the interface standard (`docs/add-roaster.md`, "`roast roaster
//! check`")? The read-only check watches the ring:
//! the rate, BT and ET, plausible units, the declared outputs and channels echoed, what the
//! roaster says it can do, and its control level. The write check (owner key only, a cold
//! roaster) sends heater 10 % and always ends with emergency stop.

use std::time::Duration;

use roastwarden_core::{DeviceCommand, Sample};
use serde::Serialize;
use serde_json::{Map, Value, json};
use utoipa::ToSchema;

use crate::app::state::AppState;
use crate::roasters::caps::DeviceCaps;
use crate::roasters::setup::{Control, RoasterSetup};
use crate::transports::transport::SendOptions;

/// The window the rate is measured over, ms.
pub const WINDOW_MS: u64 = 30_000;
/// The standard's rate, Hz.
const MIN_HZ: f64 = 0.5;
const MAX_HZ: f64 = 4.0;
/// BT the standard accepts at all, °C.
const BT_RANGE: (f64, f64) = (-10.0, 300.0);
/// Room temperature at idle, °C.
const IDLE_RANGE: (f64, f64) = (5.0, 45.0);
/// An idle reading in this band is room temperature in °F.
const FAHRENHEIT_BAND: (f64, f64) = (50.0, 115.0);
/// The write check runs only below this BT, °C (busy.rs's "hot").
pub const COLD_C: f64 = crate::control::busy::HOT_C;
/// The heater the write check sends, %.
const WRITE_HEATER_PCT: f64 = 10.0;

/// How a line came out; the verdict is the worst line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

/// One check and its sentence.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CheckLine {
    /// `rate`, `bt_et`, `units`, `outputs`, `channels`, `caps` or `control`
    pub check: &'static str,
    pub status: Level,
    pub message: String,
}

/// The roaster against the interface standard.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoasterCheck {
    pub device_id: String,
    /// The worst line
    pub verdict: Level,
    pub lines: Vec<CheckLine>,
    /// Samples a second over the last 30 s
    pub rate_hz: Option<f64>,
    /// Samples in the last 30 s
    pub samples: usize,
    /// The newest sample (°C, %): bt, et, ror, heater, fan, setpoint and the extras the
    /// roaster sends (drum, damper, its channels); null when none since the server started
    #[schema(value_type = Option<Object>)]
    pub reading: Option<Value>,
}

/// Everything the read-only check judges.
pub struct Inputs<'a> {
    pub device_id: &'a str,
    pub now_ms: u64,
    /// The last 30 s of samples, oldest first
    pub window: &'a [Sample],
    /// The newest sample, however old
    pub latest: Option<&'a Sample>,
    pub setup: &'a RoasterSetup,
    /// The declared channels' keys (docs/channels.md)
    pub channels: &'a [(String, String)],
    pub caps: &'a DeviceCaps,
    /// Every connection only reads (a Phidgets board, docs/phidgets.md): outputs cannot be
    /// reported or sent through it.
    pub reads_only: bool,
}

fn line(check: &'static str, status: Level, message: impl Into<String>) -> CheckLine {
    CheckLine {
        check,
        status,
        message: message.into(),
    }
}

fn c1(v: f64) -> String {
    format!("{v:.1} °C")
}

/// The rate and its jitter from the window's timestamps.
fn rate(window: &[Sample]) -> Option<(f64, f64)> {
    if window.len() < 2 {
        return None;
    }
    let gaps: Vec<f64> = window
        .windows(2)
        .map(|w| w[1].ts_ms.saturating_sub(w[0].ts_ms) as f64 / 1000.0)
        .collect();
    let span: f64 = gaps.iter().sum();
    if span <= 0.0 {
        return None;
    }
    let mean = span / gaps.len() as f64;
    let var = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len() as f64;
    Some((1.0 / mean, var.sqrt()))
}

/// The newest sample as the API shows it.
pub fn reading(s: &Sample, now_ms: u64) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Option<f64>| {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            m.insert(k.into(), json!((v * 100.0).round() / 100.0));
        }
    };
    put("bt", s.bt);
    put("et", s.et);
    put("ror", s.ror_bt);
    put("heater", s.heater);
    put("fan", s.fan);
    put("setpoint", s.setpoint);
    for (k, v) in &s.extras {
        put(k, Some(*v));
    }
    m.insert("age_s".into(), json!(now_ms.saturating_sub(s.ts_ms) / 1000));
    Value::Object(m)
}

/// The read-only check. Pure: the route gathers the inputs.
pub fn check(i: Inputs) -> RoasterCheck {
    let mut lines = Vec::new();
    let id = i.device_id;
    let r = rate(i.window);

    // Rate
    lines.push(match r {
        None => match i.latest {
            None => line("rate", Level::Fail, "no sample since the server started"),
            Some(s) => line(
                "rate",
                Level::Fail,
                format!(
                    "{} in the last 30 s (the newest is {} s old): the standard wants 0.5–4 a second",
                    i.window.len(),
                    i.now_ms.saturating_sub(s.ts_ms) / 1000
                ),
            ),
        },
        Some((hz, jitter)) => {
            let text = format!("{hz:.2} Hz over the last 30 s, jitter ±{jitter:.2} s");
            if !(MIN_HZ..=MAX_HZ).contains(&hz) {
                line(
                    "rate",
                    Level::Fail,
                    format!("{text}: the standard wants 0.5–4 samples a second"),
                )
            } else if jitter > 0.5 / hz {
                line(
                    "rate",
                    Level::Warn,
                    format!("{text}: uneven, RoR will be noisy"),
                )
            } else {
                line("rate", Level::Ok, text)
            }
        }
    });

    // BT and ET
    let s = i.latest;
    let bt = s.and_then(|s| s.bt);
    let et = s.and_then(|s| s.et);
    lines.push(match (s, bt, et) {
        (None, ..) => line("bt_et", Level::Fail, "no sample to read BT and ET from"),
        (Some(_), None, _) => line(
            "bt_et",
            Level::Fail,
            "no BT in the newest sample: map bt (roast roaster map)",
        ),
        (Some(_), Some(b), _) if !(BT_RANGE.0..=BT_RANGE.1).contains(&b) => line(
            "bt_et",
            Level::Fail,
            format!(
                "BT {} is outside −10–300 °C: a missing probe, or the scale or units are wrong",
                c1(b)
            ),
        ),
        (Some(_), Some(b), None) => line(
            "bt_et",
            Level::Warn,
            format!(
                "BT {}, but no ET: map et if the roaster has a second probe",
                c1(b)
            ),
        ),
        (Some(_), Some(b), Some(e)) => {
            line("bt_et", Level::Ok, format!("BT {}, ET {}", c1(b), c1(e)))
        }
    });

    // Units, judged at idle
    let heater = s.and_then(|s| s.extras.get("heaterApplied").copied().or(s.heater));
    let idle = i.setup.outputs.heater.is_none() || heater.is_none_or(|h| h <= 0.0);
    let temps: Vec<(&str, f64)> = [("BT", bt), ("ET", et)]
        .into_iter()
        .filter_map(|(n, v)| v.map(|v| (n, v)))
        .collect();
    lines.push(if temps.is_empty() {
        line("units", Level::Warn, "no temperature to judge the units by")
    } else if !idle {
        line(
            "units",
            Level::Ok,
            "judged only at idle: the heater is on now (run the check cold)",
        )
    } else if let Some((n, v)) = temps
        .iter()
        .find(|(_, v)| (FAHRENHEIT_BAND.0..=FAHRENHEIT_BAND.1).contains(v))
    {
        line(
            "units",
            Level::Warn,
            format!(
                "{n} reads {} with the heater off: room temperature in °F? set `--units F` (roast roaster map)",
                c1(*v)
            ),
        )
    } else if temps
        .iter()
        .all(|(_, v)| (IDLE_RANGE.0..=IDLE_RANGE.1).contains(v))
    {
        line(
            "units",
            Level::Ok,
            "°C: the probes read room temperature with the heater off",
        )
    } else {
        let (n, v) = temps
            .iter()
            .find(|(_, v)| !(IDLE_RANGE.0..=IDLE_RANGE.1).contains(v))
            .copied()
            .unwrap_or(temps[0]);
        line(
            "units",
            Level::Warn,
            format!(
                "{n} reads {} with the heater off: still hot, or a scale or offset is wrong (check again cold)",
                c1(v)
            ),
        )
    });

    // Outputs echoed
    let o = &i.setup.outputs;
    let declared: Vec<(&str, &str)> = [
        (o.heater.as_ref(), "heater"),
        (o.fan.as_ref(), "fan"),
        (o.drum.as_ref(), "drum"),
        (o.damper.as_ref(), "damper"),
    ]
    .into_iter()
    .filter_map(|(out, key)| out.map(|out| (out.label.as_str(), key)))
    .collect();
    let echoed = |key: &str| {
        s.is_some_and(|s| match key {
            "heater" => s.heater.is_some(),
            "fan" => s.fan.is_some(),
            k => s.extras.contains_key(k),
        })
    };
    lines.push(if declared.is_empty() {
        line("outputs", Level::Ok, "no outputs declared: Roastwarden sends it nothing")
    } else {
        let missing: Vec<String> = declared
            .iter()
            .filter(|(_, k)| !echoed(k))
            .map(|(label, k)| format!("{label} ({k})"))
            .collect();
        let names: Vec<&str> = declared.iter().map(|(l, _)| *l).collect();
        if missing.is_empty() {
            line(
                "outputs",
                Level::Ok,
                format!("{} declared and reported", names.join(", ")),
            )
        } else if i.reads_only {
            line(
                "outputs",
                Level::Warn,
                format!(
                    "declared, but its connection reads temperatures only: {} (undeclare them \
                     under Setup → Outputs while it only logs, or add a connection that carries \
                     them)",
                    missing.join(", ")
                ),
            )
        } else {
            line(
                "outputs",
                Level::Warn,
                format!(
                    "declared but not in its telemetry: {} (map them, so the dashboard shows what they run at)",
                    missing.join(", ")
                ),
            )
        }
    });

    // Channels present
    lines.push(if i.channels.is_empty() {
        line("channels", Level::Ok, "no channels declared")
    } else {
        let missing: Vec<String> = i
            .channels
            .iter()
            .filter(|(k, _)| !s.is_some_and(|s| s.extras.contains_key(k)))
            .map(|(k, label)| format!("{label} ({k})"))
            .collect();
        if missing.is_empty() {
            let names: Vec<&str> = i.channels.iter().map(|(_, l)| l.as_str()).collect();
            line(
                "channels",
                Level::Ok,
                format!("{} present", names.join(", ")),
            )
        } else {
            line(
                "channels",
                Level::Warn,
                format!("declared but not in its telemetry: {}", missing.join(", ")),
            )
        }
    });

    // Caps
    let c = i.caps;
    let list: Vec<&str> = [
        (c.pid, "pid"),
        (c.ff, "ff"),
        (c.lease, "lease"),
        (c.tpc, "tpc"),
        (c.ror, "ror"),
    ]
    .into_iter()
    .filter_map(|(on, n)| on.then_some(n))
    .collect();
    lines.push(if c.reported {
        line(
            "caps",
            Level::Ok,
            format!(
                "it says it can do: {}",
                if list.is_empty() {
                    "nothing optional".into()
                } else {
                    list.join(", ")
                }
            ),
        )
    } else {
        line(
            "caps",
            Level::Ok,
            "it says nothing about what it can do: Profile drive runs the server's own PID loop",
        )
    });

    // Control
    let where_ = format!("Roasters → {id} → Setup");
    lines.push(line(
        "control",
        Level::Ok,
        match i.setup.control {
            Control::Off => format!(
                "control off: Roastwarden only records it; the owner switches control on under {where_}"
            ),
            Control::Manual => format!(
                "control manual: heater, fan, drum, damper and STOP from the dashboard; Profile drive needs full ({where_})"
            ),
            Control::Full => "control full: everything, Profile drive included".into(),
        },
    ));

    RoasterCheck {
        device_id: id.to_string(),
        verdict: lines.iter().map(|l| l.status).max().unwrap_or(Level::Ok),
        lines,
        rate_hz: r.map(|(hz, _)| (hz * 100.0).round() / 100.0),
        samples: i.window.len(),
        reading: s.map(|s| reading(s, i.now_ms)),
    }
}

/// The read-only check of a roaster now.
pub async fn check_now(state: &AppState, device_id: &str) -> RoasterCheck {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let window =
        state
            .telemetry_service
            .samples_since_ms(device_id, now_ms.saturating_sub(WINDOW_MS), 1000);
    let latest = state.telemetry_service.latest_sample(device_id);
    let setup = state.transports.setups().get(device_id).await;
    let channels = declared_channels(&setup);
    let caps = state.replay.device_caps(device_id);
    let reads_only =
        match crate::roasters::registry::get_device_by_device_id(&state.db, device_id).await {
            Ok(Some(d)) => {
                let on: Vec<_> = d.connections.iter().filter(|c| c.enabled).collect();
                !on.is_empty()
                    && on
                        .iter()
                        .all(|c| c.protocol == crate::roasters::types::Protocol::Phidget22)
            }
            _ => false,
        };
    check(Inputs {
        device_id,
        now_ms,
        window: &window,
        latest: latest.as_ref(),
        setup: &setup,
        channels: &channels,
        caps: &caps,
        reads_only,
    })
}

/// The setup's declared channels as (key, label) (docs/channels.md).
fn declared_channels(setup: &RoasterSetup) -> Vec<(String, String)> {
    setup
        .channels
        .iter()
        .map(|c| (c.key.clone(), c.label.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// The write check
// ---------------------------------------------------------------------------

/// One step of the write check.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WriteStep {
    pub step: String,
    pub ok: bool,
    /// When it ended, ms after the check started
    pub at_ms: u64,
    pub message: String,
}

/// What the write check did. It always ends with emergency stop.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WriteCheck {
    pub device_id: String,
    /// Every step passed
    pub ok: bool,
    pub steps: Vec<WriteStep>,
}

/// Why the write check did not start: a 409 (`CONTROL_OFF` for control off).
#[derive(Debug, PartialEq)]
pub struct Refused {
    pub code: &'static str,
    pub message: String,
}

fn refused(message: impl Into<String>) -> Refused {
    Refused {
        code: "CHECK_REFUSED",
        message: message.into(),
    }
}

/// What the write check needs of the roaster before it sends anything. Pure.
pub fn may_write(
    device_id: &str,
    setup: &RoasterSetup,
    latest: Option<&Sample>,
    now_ms: u64,
    roast_open: bool,
    busy: bool,
) -> Result<(), Refused> {
    if setup.control == Control::Off {
        return Err(Refused {
            code: "CONTROL_OFF",
            message: format!(
                "control is off for {device_id}: the write check needs manual or full control (Roasters → {device_id} → Setup)"
            ),
        });
    }
    if setup.outputs.heater.is_none() {
        return Err(refused(format!(
            "{device_id} has no heater declared: there is nothing to write (Roasters → {device_id} → Setup)"
        )));
    }
    let Some(s) = latest.filter(|s| now_ms.saturating_sub(s.ts_ms) < 10_000) else {
        return Err(refused(format!(
            "no sample from {device_id} in the last 10 s: the check needs its readings"
        )));
    };
    match s.bt {
        None => return Err(refused("no BT in its newest sample")),
        Some(bt) if bt >= COLD_C => {
            return Err(refused(format!(
                "BT is {bt:.1} °C: the write check runs only on a cold roaster (below {COLD_C} °C)"
            )));
        }
        _ => {}
    }
    if s.mode == Some(1) {
        return Err(refused(
            "its PID is in auto: switch it to manual first, so the heater % is what it runs",
        ));
    }
    if roast_open {
        return Err(refused("a roast is open on it: finish or cancel it first"));
    }
    if busy {
        return Err(refused(
            "it is busy (a profile drive, an autotune or cooling): wait until it is idle",
        ));
    }
    Ok(())
}

/// How long the write check waits for each echo.
#[derive(Clone, Copy)]
pub struct Waits {
    pub echo: Duration,
    pub off: Duration,
}

impl Default for Waits {
    fn default() -> Self {
        Self {
            echo: Duration::from_secs(5),
            off: Duration::from_secs(5),
        }
    }
}

/// Poll the newest sample until `ok` holds for one received after `after_ms`.
async fn wait_for(
    state: &AppState,
    device_id: &str,
    after_ms: u64,
    within: Duration,
    ok: impl Fn(&Sample) -> bool,
) -> Option<Sample> {
    let until = tokio::time::Instant::now() + within;
    loop {
        if let Some(s) = state
            .telemetry_service
            .latest_sample(device_id)
            .filter(|s| s.ts_ms > after_ms && ok(s))
        {
            return Some(s);
        }
        if tokio::time::Instant::now() >= until {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Start the write check: refuse (409) unless the roaster may be written now, else run it in
/// its own task (it survives the caller going away, so emergency stop is always sent) and
/// answer what it did.
pub async fn write_check(
    state: &AppState,
    device_id: &str,
    waits: Waits,
) -> Result<WriteCheck, Refused> {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let setup = state.transports.setups().get(device_id).await;
    let latest = state.telemetry_service.latest_sample(device_id);
    let roast_open = crate::roasts::sessions::get_active_session(&state.db, device_id)
        .await
        .ok()
        .flatten()
        .is_some();
    let busy = state.replay.drives_device(device_id).await
        || state.tune.active(device_id).await
        || state.sequence.cooling(device_id).await;
    may_write(device_id, &setup, latest.as_ref(), now_ms, roast_open, busy)?;
    let state = state.clone();
    let id = device_id.to_string();
    let enable_first = latest.as_ref().and_then(|s| s.heater_enable) == Some(false);
    let task = tokio::spawn(async move { run_write(&state, &id, waits, enable_first).await });
    task.await
        .map_err(|e| refused(format!("the write check stopped: {e}")))
}

async fn run_write(
    state: &AppState,
    device_id: &str,
    waits: Waits,
    enable_first: bool,
) -> WriteCheck {
    let started = tokio::time::Instant::now();
    let at = |t: tokio::time::Instant| t.duration_since(started).as_millis() as u64;
    let now_ms = || chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut steps = Vec::new();
    let mut step = |name: &str, ok: bool, message: String| {
        steps.push(WriteStep {
            step: name.into(),
            ok,
            at_ms: at(tokio::time::Instant::now()),
            message,
        });
    };
    let send = |cmd: DeviceCommand| async move {
        state
            .transports
            .send(device_id, &cmd, SendOptions::default())
            .await
            .map(std::string::ToString::to_string)
            .map_err(|e| e.to_string())
    };

    // Heater on at 10 %, and its echo. Anything that fails here still ends at STOP below.
    let mut heated = true;
    if enable_first {
        match send(DeviceCommand::HeaterEnable { enabled: true }).await {
            Ok(via) => step("heater power on", true, format!("sent via {via}")),
            Err(e) => {
                step("heater power on", false, e);
                heated = false;
            }
        }
    }
    if heated {
        let sent_at = now_ms();
        match send(DeviceCommand::Heater {
            percent: WRITE_HEATER_PCT,
        })
        .await
        {
            Ok(via) => {
                step(
                    "heater 10 %",
                    true,
                    format!("sent via {via} (shaped by the roaster's outputs)"),
                );
                match wait_for(state, device_id, sent_at, waits.echo, |s| {
                    s.heater.is_some_and(|h| h > 0.0)
                })
                .await
                {
                    Some(s) => step(
                        "heater echo",
                        true,
                        format!("it reports heater {} %", s.heater.unwrap_or(0.0)),
                    ),
                    None => step(
                        "heater echo",
                        false,
                        format!(
                            "no sample reported the heater on within {} s: is heater mapped, and does the roaster obey?",
                            waits.echo.as_secs_f64()
                        ),
                    ),
                }
            }
            Err(e) => step("heater 10 %", false, e),
        }
    }

    // Always: emergency stop, then the heater reading 0.
    let stop_at = now_ms();
    match send(DeviceCommand::EmergencyStop).await {
        Ok(via) => step("emergency stop", true, format!("sent via {via}")),
        Err(e) => step("emergency stop", false, e),
    }
    let off = |s: &Sample| {
        s.heater.is_none_or(|h| h <= 0.0) && s.extras.get("heaterApplied").is_none_or(|h| *h <= 0.0)
    };
    match wait_for(state, device_id, stop_at, waits.off, off).await {
        Some(_) => step("heater off", true, "it reports the heater at 0".into()),
        None => step(
            "heater off",
            false,
            format!(
                "the heater did not read 0 within {} s of STOP: check the roaster now",
                waits.off.as_secs_f64()
            ),
        ),
    }
    tracing::info!(device_id, steps = steps.len(), "write check done");
    WriteCheck {
        device_id: device_id.to_string(),
        ok: steps.iter().all(|s| s.ok),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roasters::setup::{Kind, Output, Outputs};
    use std::collections::BTreeMap;

    fn sample(ts_ms: u64, bt: Option<f64>, et: Option<f64>, heater: Option<f64>) -> Sample {
        Sample {
            seq: 0,
            ts_ms,
            bt,
            et,
            ror_bt: None,
            ror_et: None,
            ror_device: None,
            heater,
            fan: Some(50.0),
            setpoint: None,
            mode: Some(0),
            heater_enable: None,
            extras: BTreeMap::new(),
        }
    }

    fn setup(control: Control, outputs: Outputs) -> RoasterSetup {
        RoasterSetup {
            device_id: "r".into(),
            kind: Kind::Other,
            control,
            outputs,
            fan_floor_pct: 0.0,
            fixed: false,
            updated_at: None,
            channels: Vec::new(),
        }
    }

    fn run(window: &[Sample], setup: &RoasterSetup) -> RoasterCheck {
        check(Inputs {
            device_id: "r",
            now_ms: 100_000,
            window,
            latest: window.last(),
            setup,
            channels: &[],
            caps: &DeviceCaps::default(),
            reads_only: false,
        })
    }

    fn at(c: &RoasterCheck, name: &str) -> Level {
        c.lines.iter().find(|l| l.check == name).unwrap().status
    }

    #[expect(
        clippy::maybe_infinite_iter,
        reason = "take_while ends it: t only grows"
    )]
    fn every(hz: f64, f: impl Fn(u64) -> Sample) -> Vec<Sample> {
        let step = (1000.0 / hz) as u64;
        (0..)
            .map(|i| 70_000 + i * step)
            .take_while(|t| *t <= 100_000)
            .map(f)
            .collect()
    }

    #[test]
    fn the_rate_passes_at_1_hz_and_fails_at_a_fifth() {
        let s = setup(Control::Off, Kind::Other.outputs());
        let one = every(1.0, |t| sample(t, Some(24.0), Some(25.0), Some(0.0)));
        let c = run(&one, &s);
        assert_eq!(at(&c, "rate"), Level::Ok, "{:?}", c.lines);
        assert_eq!(c.rate_hz, Some(1.0));
        assert_eq!(c.verdict, Level::Ok, "{:?}", c.lines);
        let slow = every(0.2, |t| sample(t, Some(24.0), Some(25.0), Some(0.0)));
        let c = run(&slow, &s);
        assert_eq!(at(&c, "rate"), Level::Fail);
        assert_eq!(c.verdict, Level::Fail);
        let none = run(&[], &s);
        assert_eq!(at(&none, "rate"), Level::Fail);
        assert_eq!(at(&none, "bt_et"), Level::Fail);
        assert!(none.reading.is_none());
    }

    #[test]
    fn fahrenheit_at_idle_is_suspected_and_a_hot_heater_is_not_judged() {
        let s = setup(Control::Off, Kind::Other.outputs());
        let f = every(1.0, |t| sample(t, Some(72.5), Some(74.0), Some(0.0)));
        let c = run(&f, &s);
        assert_eq!(at(&c, "units"), Level::Warn);
        assert!(c.lines.iter().any(|l| l.message.contains("--units F")));
        let hot = every(1.0, |t| sample(t, Some(72.5), Some(150.0), Some(60.0)));
        assert_eq!(at(&run(&hot, &s), "units"), Level::Ok);
        let wild = every(1.0, |t| sample(t, Some(900.0), Some(25.0), Some(0.0)));
        assert_eq!(at(&run(&wild, &s), "bt_et"), Level::Fail);
        let no_et = every(1.0, |t| sample(t, Some(24.0), None, Some(0.0)));
        assert_eq!(at(&run(&no_et, &s), "bt_et"), Level::Warn);
    }

    #[test]
    fn a_declared_output_not_echoed_warns() {
        let mut outputs = Kind::DrumGas.outputs();
        outputs.drum = Some(Output {
            label: "Drum".into(),
            min: 40.0,
            max: 70.0,
            step: 0.0,
        });
        let s = setup(Control::Full, outputs);
        let w = every(1.0, |t| sample(t, Some(24.0), Some(25.0), Some(0.0)));
        let c = run(&w, &s);
        assert_eq!(at(&c, "outputs"), Level::Warn);
        assert!(c.lines.iter().any(|l| l.message.contains("Drum (drum)")));
        let w = every(1.0, |t| {
            let mut x = sample(t, Some(24.0), Some(25.0), Some(0.0));
            x.extras.insert("drum".into(), 55.0);
            x
        });
        let c = run(&w, &s);
        assert_eq!(at(&c, "outputs"), Level::Ok);
        assert_eq!(c.reading.unwrap()["drum"], 55.0);
    }

    /// Raw capture on a mapped MQTT topic and the preview through the API: values in °C, a
    /// missing path said, a bad mapping's 400, a stranger's connection 404, any token may try.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_newest_raw_message_and_the_preview() {
        use crate::test_support::{Harness, call, eventually};
        use axum::http::StatusCode;
        use rumqttc::QoS;
        let h = Harness::new().await;
        let dev = h
            .owner(
                "POST",
                "/api/devices",
                Some(json!({ "name": "Tasmota", "device_id": "tas1" })),
            )
            .await
            .json();
        let reg = dev["id"].as_str().unwrap().to_string();
        h.owner(
            "PUT",
            &format!("/api/devices/{reg}"),
            Some(json!({ "status": "active" })),
        )
        .await;
        let conn = h
            .owner(
                "POST",
                &format!("/api/devices/{reg}/connections"),
                Some(
                    json!({ "protocol": "mqtt", "enabled": true, "priority": 0, "config": {
                    "telemetry_topic": "tele/tas1/SENSOR",
                    "ingest": { "units": "F", "fields": { "bt": "MAX31855.Temperature" } } } }),
                ),
            )
            .await;
        assert_eq!(conn.status, StatusCode::CREATED, "{:?}", conn.json());
        let conn_id = conn.json()["id"].as_str().unwrap().to_string();
        let payload = r#"{"Time":"x","MAX31855":{"Temperature":415.4},"Fan":40}"#;
        let raw = || async { h.owner("GET", "/api/roaster/tas1/raw", None).await.json() };
        assert!(
            eventually(|| async {
                let _ = h
                    .state
                    .mqtt
                    .publish("tele/tas1/SENSOR", QoS::AtMostOnce, false, payload)
                    .await;
                raw().await.as_array().is_some_and(|a| !a.is_empty())
            })
            .await
        );
        let r = raw().await;
        assert_eq!(r[0]["connection_id"], conn_id.as_str());
        assert_eq!(r[0]["protocol"], "mqtt");
        assert_eq!(r[0]["topic"], "tele/tas1/SENSOR");
        assert_eq!(r[0]["payload"], payload);

        let preview = |ingest: Value, token: Option<String>| {
            let app = h.app.clone();
            let conn_id = conn_id.clone();
            async move {
                call(
                    &app,
                    "POST",
                    "/api/roaster/tas1/ingest/preview",
                    Some(token.as_deref().unwrap_or(crate::test_support::OWNER_KEY)),
                    Some(json!({ "connection_id": conn_id, "ingest": ingest })),
                )
                .await
            }
        };
        let p = preview(
            json!({ "units": "F", "fields": {
                "bt": "MAX31855.Temperature", "fan": "Fan", "et": "probe.x" } }),
            None,
        )
        .await;
        assert_eq!(p.status, StatusCode::OK, "{:?}", p.json());
        let p = p.json();
        let field = |k: &str| {
            p["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["key"] == k)
                .cloned()
                .unwrap()
        };
        assert_eq!(field("bt")["value"], 213.0);
        assert_eq!(field("bt")["field"], "beanTemp");
        assert_eq!(field("fan")["value"], 40.0);
        assert!(
            field("et")["problem"]
                .as_str()
                .unwrap()
                .contains("probe.x is not in the message")
        );
        assert_eq!(p["sample"]["beanTemp"], 213.0);
        // A bad mapping is the save's 400
        let p = preview(json!({ "fields": {} }), None).await;
        assert_eq!(p.status, StatusCode::BAD_REQUEST);
        // Any token may try one, on a roaster tokens may not control: nothing is sent
        let ro = h.token("read_only").await;
        let p = preview(json!({ "fields": { "bt": "Fan" } }), Some(ro.clone())).await;
        assert_eq!(p.status, StatusCode::OK, "{:?}", p.json());
        let c = call(&h.app, "GET", "/api/roaster/tas1/check", Some(&ro), None).await;
        assert_eq!(c.status, StatusCode::OK);
        assert_eq!(c.json()["lines"].as_array().unwrap().len(), 7);
        // Another roaster's connection is not found here
        let p = call(
            &h.app,
            "POST",
            "/api/roaster/esp32_x/ingest/preview",
            Some(crate::test_support::OWNER_KEY),
            Some(json!({ "connection_id": conn_id })),
        )
        .await;
        assert_eq!(p.status, StatusCode::NOT_FOUND);
    }

    /// The write check on the virtual roaster: heater 10 %, its echo, STOP, heater 0. A token
    /// may not; a roaster with control off is refused.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_write_check_on_the_virtual_roaster() {
        use crate::test_support::{Harness, call, eventually};
        use axum::http::StatusCode;
        let h = Harness::new().await;
        h.owner(
            "PUT",
            "/api/virtual-roaster",
            Some(json!({ "enabled": true })),
        )
        .await;
        assert!(
            eventually(|| async {
                h.state
                    .telemetry_service
                    .latest_sample("virtual_roaster")
                    .is_some()
            })
            .await
        );
        let admin = h.token("admin").await;
        let t = call(
            &h.app,
            "POST",
            "/api/roaster/virtual_roaster/check/write",
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(t.status, StatusCode::FORBIDDEN);
        let w = h
            .owner("POST", "/api/roaster/virtual_roaster/check/write", None)
            .await;
        assert_eq!(w.status, StatusCode::OK, "{:?}", w.json());
        let w = w.json();
        assert_eq!(w["ok"], true, "{w}");
        let steps: Vec<&str> = w["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["step"].as_str().unwrap())
            .collect();
        assert_eq!(
            steps,
            [
                "heater power on",
                "heater 10 %",
                "heater echo",
                "emergency stop",
                "heater off"
            ]
        );
        let s = h
            .state
            .telemetry_service
            .latest_sample("virtual_roaster")
            .unwrap();
        assert_eq!(s.heater, Some(0.0));

        // Control off: refused before anything is sent
        h.owner(
            "POST",
            "/api/test/emit-telemetry/logger_x",
            Some(json!({ "beanTemp": 24.0, "envTemp": 25.0 })),
        )
        .await;
        let r = h
            .owner("POST", "/api/roaster/logger_x/check/write", None)
            .await;
        assert_eq!(r.status, StatusCode::CONFLICT);
        assert_eq!(r.json()["error"]["code"], "CONTROL_OFF");
    }

    /// A roaster that never echoes: the heater times out and STOP is still sent.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_write_check_ends_with_stop_on_a_timeout() {
        use crate::test_support::{Harness, eventually};
        let h = Harness::new().await;
        let put = h
            .owner(
                "PUT",
                "/api/roaster/mute1/setup",
                Some(json!({ "kind": "other", "control": "full" })),
            )
            .await;
        assert!(put.status.is_success());
        h.owner(
            "POST",
            "/api/test/emit-telemetry/mute1",
            Some(json!({ "beanTemp": 24.0, "envTemp": 25.0, "heater": 0, "fan": 0 })),
        )
        .await;
        assert!(
            eventually(|| async { h.state.telemetry_service.latest_sample("mute1").is_some() })
                .await
        );
        let mut events = h.state.mqtt.events();
        let w = write_check(
            &h.state,
            "mute1",
            Waits {
                echo: Duration::from_millis(300),
                off: Duration::from_millis(300),
            },
        )
        .await
        .unwrap();
        assert!(!w.ok);
        let echo = w.steps.iter().find(|s| s.step == "heater echo").unwrap();
        assert!(!echo.ok);
        let stop = w.steps.iter().find(|s| s.step == "emergency stop").unwrap();
        assert!(stop.ok, "{stop:?}");
        let mut topics = Vec::new();
        while let Ok(ev) = events.try_recv() {
            if let roastwarden_mqtt::MqttEvent::Publish { topic, .. } = ev {
                topics.push(topic);
            }
        }
        let heater = topics
            .iter()
            .position(|t| t == "roaster/mute1/control/heater");
        let stop = topics
            .iter()
            .position(|t| t == "roaster/mute1/control/emergency_stop");
        assert!(heater.is_some() && stop > heater, "{topics:?}");
    }

    #[test]
    fn a_declared_channel_missing_warns() {
        let s = setup(Control::Off, Kind::Logger.outputs());
        let w = every(1.0, |t| {
            let mut x = sample(t, Some(24.0), Some(25.0), None);
            x.extras.insert("inlet".into(), 30.0);
            x
        });
        let run_with = |chans: &[(String, String)]| {
            check(Inputs {
                device_id: "r",
                now_ms: 100_000,
                window: &w,
                latest: w.last(),
                setup: &s,
                channels: chans,
                caps: &DeviceCaps::default(),
                reads_only: false,
            })
        };
        let ok = run_with(&[("inlet".into(), "Inlet".into())]);
        assert_eq!(at(&ok, "channels"), Level::Ok);
        let bad = run_with(&[("t3".into(), "Exhaust".into())]);
        assert_eq!(at(&bad, "channels"), Level::Warn);
        assert!(bad.lines.iter().any(|l| l.message.contains("Exhaust (t3)")));
    }

    #[test]
    fn the_write_check_refuses_off_no_heater_hot_auto_open_and_busy() {
        let now = 100_000;
        let cold = sample(now - 500, Some(24.0), Some(25.0), Some(0.0));
        let full = setup(Control::Full, Kind::Other.outputs());
        assert_eq!(
            may_write("r", &full, Some(&cold), now, false, false),
            Ok(())
        );
        let off = setup(Control::Off, Kind::Other.outputs());
        assert_eq!(
            may_write("r", &off, Some(&cold), now, false, false)
                .unwrap_err()
                .code,
            "CONTROL_OFF"
        );
        let logger = setup(Control::Manual, Kind::Logger.outputs());
        assert!(may_write("r", &logger, Some(&cold), now, false, false).is_err());
        let hot = sample(now - 500, Some(80.0), Some(85.0), Some(0.0));
        assert!(
            may_write("r", &full, Some(&hot), now, false, false)
                .unwrap_err()
                .message
                .contains("cold")
        );
        let mut auto = cold.clone();
        auto.mode = Some(1);
        assert!(may_write("r", &full, Some(&auto), now, false, false).is_err());
        assert!(may_write("r", &full, Some(&cold), now, true, false).is_err());
        assert!(may_write("r", &full, Some(&cold), now, false, true).is_err());
        assert!(may_write("r", &full, None, now, false, false).is_err());
        let stale = sample(now - 60_000, Some(24.0), Some(25.0), Some(0.0));
        assert!(may_write("r", &full, Some(&stale), now, false, false).is_err());
    }
}
