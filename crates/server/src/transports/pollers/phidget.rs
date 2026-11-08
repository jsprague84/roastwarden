//! A Phidgets temperature board through the Phidget22 network server: reads only, no
//! command sink (docs/phidgets.md).

use std::time::Duration;

use tokio::time::sleep;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::types::{DeviceConnection, DeviceStatus};

use super::MIN_POLL_MS;

pub(super) async fn phidget_loop(
    device_id: String,
    conn: DeviceConnection,
    telemetry: TelemetryService,
) {
    let setup = match crate::transports::phidget::Setup::from_config(&conn.config) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(%device_id, error = %e, "Invalid Phidget22 connection config");
            return;
        }
    };
    let every = Duration::from_millis(setup.config.poll_interval_ms.max(MIN_POLL_MS));
    let stale = crate::transports::phidget::stale_after(&setup.config);
    let mut backoff = Duration::from_secs(1);
    loop {
        match crate::transports::phidget::start(&setup).await {
            Ok((mut link, board, warnings)) => {
                tracing::info!(%device_id, server = %link.server, board = %board.label(),
                    inputs = ?setup.inputs, "Phidget board connected");
                for w in warnings {
                    tracing::warn!(%device_id, board = %board.label(), "Phidget setting not taken: {w}");
                }
                backoff = Duration::from_secs(1);
                let mut readings = crate::transports::phidget::Readings::default();
                for n in &setup.inputs {
                    if let Some(i) = board.input(*n) {
                        readings.watch(i);
                    }
                }
                let mut tick = tokio::time::interval(every);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                let gone = loop {
                    tokio::select! {
                        m = link.recv(Duration::from_secs(3600)) => match m {
                            Ok(Some(m)) => {
                                if m.header.kind == crate::transports::phidget::MSG_DEVICE
                                    && m.header.sub == crate::transports::phidget::SUB_DETACH
                                    && m.body["phid"].as_u64() == Some(board.phid)
                                {
                                    break format!("{} was unplugged", board.label());
                                }
                                readings.take(&m, std::time::Instant::now());
                            }
                            Ok(None) => {}
                            Err(e) => break e.0,
                        },
                        _ = tick.tick() => {
                            let raw = readings.sample(std::time::Instant::now(), stale);
                            if raw.is_empty() {
                                continue;
                            }
                            let raw = serde_json::Value::Object(raw);
                            telemetry.raw.record(&device_id, Some(&conn.id), "phidget22", None,
                                raw.to_string().as_bytes());
                            if let Some(sample) = setup.ingest.map(&raw) {
                                telemetry
                                    .process_telemetry(&device_id,
                                        &serde_json::Value::Object(sample),
                                        Some(&DeviceStatus::Active))
                                    .await;
                            }
                        }
                    }
                };
                tracing::warn!(%device_id, "Phidget board lost: {gone}; reconnecting");
            }
            Err(e) => {
                tracing::warn!(%device_id, error = %e, backoff_secs = backoff.as_secs(),
                    "Phidget22 connect failed, retrying");
            }
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}
