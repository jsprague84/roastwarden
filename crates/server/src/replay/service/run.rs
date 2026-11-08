//! A run's task: one tick a second, the engine's commands sent through the transports (a
//! ghost sends nothing), the roaster's lease renewed, the log persisted, the state broadcast;
//! and why a run was cut.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use tokio::sync::Mutex;

use super::{marks_from, ReplayService, RunInner, LEASE_MS, STALE_MS, STALE_STOP, TICK};
use crate::replay::engine::{Channel, TickInput};
use crate::roasts::types::{RoastEventType, SessionStatus};
use crate::transports::transport::SendOptions;
use roastwarden_core::{ControlMode, DeviceCommand};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Why the roaster cut its heater, from the firmware's latched stop record (3.3.2+:
/// `stopCode`, `stopUptimeMs`) when it changed since `seen` (the record while heating),
/// else the old text.
/// `renewed_ago`: how long before the cut the server last sent the lease. A lease that
/// "expired" within its own length of a renewal is the roaster's fault, not a quiet server
/// (firmware 3.4.0 and older could read a renewal landing mid-check as 49 days old).
fn cut_reason(
    sample: Option<&roastwarden_core::Sample>,
    seen: Option<f64>,
    renewed_ago: Option<Duration>,
) -> String {
    let x = |k: &str| sample.and_then(|s| s.extras.get(k).copied());
    let fresh = x("stopUptimeMs").is_some_and(|at| at > 0.0 && Some(at) != seen);
    match x("stopCode") {
        Some(c)
            if fresh
                && c == 7.0
                && renewed_ago.is_some_and(|d| d.as_millis() < u128::from(LEASE_MS)) =>
        {
            format!(
                "the roaster cut its heater: control lease expired, though the server renewed it {:.0} s before (the roaster missed the renewal)",
                renewed_ago.unwrap_or_default().as_secs_f64()
            )
        }
        Some(c) if fresh && c > 0.0 => {
            format!("the roaster cut its heater: {}", stop_reason_text(c as i64))
        }
        _ => "the device disabled its heater (safe state or operator)".to_string(),
    }
}

/// The firmware's stop codes (`Esp32_FluidBedCoffeeRoaster` 3.3.2, `stopCode`).
fn stop_reason_text(code: i64) -> &'static str {
    match code {
        1 => "emergency stop",
        2 => "STOP on the serial port",
        3 => "bean temperature over its limit",
        4 => "environment temperature over its limit",
        5 => "bean thermocouple failed",
        6 => "environment thermocouple failed",
        7 => "control lease expired (the server went quiet)",
        8 => "no controller: MQTT and serial lost",
        9 => "the Roastwarden server reported offline",
        10 => "firmware update started",
        11 => "autotune switched the heater off",
        _ => "unknown reason",
    }
}

pub(super) async fn replay_loop(
    svc: Arc<ReplayService>,
    session_id: String,
    inner: Arc<Mutex<RunInner>>,
) {
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;

        let session = match crate::roasts::sessions::get_session(&svc.db, &session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                svc.finish(&session_id, "session deleted").await;
                return;
            }
            Err(e) => {
                inner.lock().await.meta.last_error = Some(format!("database: {e}"));
                continue;
            }
        };
        match session.status {
            // Pause stops the recording, not the roaster ("the roaster
            // carries on"): the drive keeps driving and holding its lease.
            SessionStatus::Active | SessionStatus::Paused => {}
            other => {
                svc.finish(&session_id, &format!("session {other}")).await;
                return;
            }
        }
        let Some(start) = session.start_time else {
            continue;
        };
        let elapsed = (Utc::now() - start).num_milliseconds() as f64 / 1000.0;

        let events = crate::roasts::events::get_roast_events(&svc.db, &session_id)
            .await
            .unwrap_or_default();
        if events.iter().any(|e| e.event_type == RoastEventType::Drop) {
            svc.finish(&session_id, "DROP marked").await;
            return;
        }
        let marks = marks_from(&events);

        let device_id = session.device_id.clone();
        let sample = svc
            .telemetry
            .latest_sample(&device_id)
            .filter(|s| now_ms().saturating_sub(s.ts_ms) <= STALE_MS);
        let input = TickInput {
            elapsed,
            bt: sample.as_ref().and_then(|s| s.bt),
            ror: sample.as_ref().and_then(|s| s.ror_bt),
            // The heater the roaster applied (after its RoR limiter and interlock,
            // firmware 3.2+), else the one it reports commanded.
            heater: sample
                .as_ref()
                .and_then(|s| s.extras.get("heaterApplied").copied().or(s.heater)),
            fan: svc
                .scheduler
                .commanded_fan(&device_id)
                .await
                .or(sample.as_ref().and_then(|s| s.fan)),
            auto: sample.as_ref().and_then(|s| s.mode).map(|m| m == 1),
            marks,
        };

        // Heater enable going from on to off mid-run means the device entered
        // its safe state (lease expiry, over-temperature, sensor failure) or
        // the operator cut the heater: either way the replay is over.
        let lost = {
            let mut g = inner.lock().await;
            if sample.is_some() {
                g.meta.stale_since = None;
            } else if g.meta.stale_since.is_none() {
                g.meta.stale_since = Some(Instant::now());
            }
            !g.engine.config().ghost
                && g.meta
                    .stale_since
                    .is_some_and(|t| t.elapsed() >= STALE_STOP)
        };
        if lost {
            svc.finish(&session_id, "no telemetry from the roaster for 20 s")
                .await;
            // Best effort: if commands still reach it, leave it in manual at 0 %.
            for cmd in [
                DeviceCommand::Mode {
                    mode: ControlMode::Manual,
                },
                DeviceCommand::Heater { percent: 0.0 },
            ] {
                let _ = svc
                    .transports
                    .send(&session.device_id, &cmd, SendOptions::default())
                    .await;
            }
            return;
        }

        let heater_cut = {
            let mut g = inner.lock().await;
            match sample.as_ref().and_then(|s| s.heater_enable) {
                Some(true) => {
                    g.meta.heater_enabled_seen = true;
                    g.meta.stop_seen = sample
                        .as_ref()
                        .and_then(|s| s.extras.get("stopUptimeMs").copied());
                    false
                }
                Some(false) => g.meta.heater_enabled_seen && !g.engine.config().ghost,
                None => false,
            }
        };
        if heater_cut {
            let reason = {
                let mut g = inner.lock().await;
                g.meta.stopped_by_device = true;
                cut_reason(
                    sample.as_ref(),
                    g.meta.stop_seen,
                    g.meta.lease_sent_at.map(|t| t.elapsed()),
                )
            };
            svc.finish(&session_id, &reason).await;
            // Leave the roaster in manual at 0 %: switching heater power back on would
            // otherwise start its PID heating towards the run's last setpoint.
            for cmd in [
                DeviceCommand::Mode {
                    mode: ControlMode::Manual,
                },
                DeviceCommand::Heater { percent: 0.0 },
            ] {
                if let Err(e) = svc
                    .transports
                    .send(&session.device_id, &cmd, SendOptions::default())
                    .await
                {
                    tracing::warn!(device_id = %session.device_id, error = %e, "could not put the roaster in manual after a heater cut");
                }
            }
            return;
        }

        let (out, ghost, run_id) = {
            let mut g = inner.lock().await;
            g.meta.telemetry_stale = sample.is_none();
            let out = g.engine.tick(&input);
            (out, g.engine.config().ghost, g.meta.run_id.clone())
        };

        let mut failed: Vec<(Option<Channel>, String)> = Vec::new();
        let mut sent = 0u64;
        if !ghost {
            for (ch, cmd) in &out.commands {
                match svc
                    .transports
                    .send(&device_id, cmd, SendOptions::default())
                    .await
                {
                    Ok(_) => {
                        sent += 1;
                        if let DeviceCommand::Fan { percent } = cmd {
                            svc.scheduler.note_fan(&device_id, *percent).await;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(session_id, device_id, command = cmd.name(), error = %e, "replay send failed");
                        failed.push((*ch, e.to_string()));
                    }
                }
            }
        }
        svc.persist(&run_id, &session_id, ghost, &out.log).await;
        svc.refresh_lease(&inner, false).await;
        svc.follow_gain_schedule(&inner, &device_id, sample.as_ref())
            .await;

        let snapshot = {
            let mut g = inner.lock().await;
            if sent > 0 {
                g.meta.sends += sent;
                g.meta.last_sent_at = Some(Utc::now());
            }
            if failed.is_empty() {
                if sent > 0 {
                    g.meta.last_error = None;
                }
            } else {
                for (ch, _) in &failed {
                    if let Some(ch) = ch {
                        g.engine.send_failed(*ch);
                    }
                }
                g.meta.last_error = failed.last().map(|(_, e)| e.clone());
            }
            g.snapshot()
        };
        let _ = svc.events_tx.send(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(extras: &[(&str, f64)]) -> roastwarden_core::Sample {
        let mut s = roastwarden_core::Sample::default();
        for (k, v) in extras {
            s.extras.insert((*k).to_string(), *v);
        }
        s
    }

    #[test]
    fn names_a_fresh_stop_and_falls_back_otherwise() {
        let s = sample(&[("stopCode", 7.0), ("stopUptimeMs", 101_000.0)]);
        assert_eq!(
            cut_reason(Some(&s), Some(40_000.0), None),
            "the roaster cut its heater: control lease expired (the server went quiet)"
        );
        assert_eq!(
            cut_reason(Some(&s), None, Some(Duration::from_secs(12))),
            "the roaster cut its heater: control lease expired (the server went quiet)"
        );
        // The record latches until the next stop: the same one as while heating is not this cut.
        assert_eq!(
            cut_reason(Some(&s), Some(101_000.0), None),
            "the device disabled its heater (safe state or operator)"
        );
        // Firmware before 3.3.2 sends no record.
        assert_eq!(
            cut_reason(Some(&sample(&[])), None, None),
            "the device disabled its heater (safe state or operator)"
        );
        assert_eq!(
            cut_reason(None, None, None),
            "the device disabled its heater (safe state or operator)"
        );
    }

    #[test]
    fn a_lease_cut_right_after_a_renewal_is_the_roasters_fault() {
        // A recorded roast: firmware 3.4.0 cut 0.5 s after the server's last renewal.
        let s = sample(&[("stopCode", 7.0), ("stopUptimeMs", 101_000.0)]);
        assert_eq!(
            cut_reason(Some(&s), None, Some(Duration::from_millis(2_600))),
            "the roaster cut its heater: control lease expired, though the server renewed it 3 s before (the roaster missed the renewal)"
        );
        // Other stops keep their reason whenever the lease was sent.
        let s = sample(&[("stopCode", 3.0), ("stopUptimeMs", 101_000.0)]);
        assert_eq!(
            cut_reason(Some(&s), None, Some(Duration::from_secs(1))),
            "the roaster cut its heater: bean temperature over its limit"
        );
    }
}
