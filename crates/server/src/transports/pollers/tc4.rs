//! A TC4-protocol device (aArtisan) over a serial port: one task owns the port, polls it
//! and carries its commands.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_serial::SerialPortBuilderExt;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::types::{DeviceConnection, DeviceStatus};
use crate::transports::ingest::Kind;
use crate::transports::tc4;
use crate::transports::transport::{SerialRequest, SerialSink};
use crate::transports::types::SerialTc4ConnectionConfig;

use super::Tracked;

/// Poll a TC4-protocol device over a serial port and carry its commands.
/// One task owns the port: `READ` polls and command lines are interleaved so
/// request/response pairs never cross.
pub(super) async fn serial_tc4_loop(
    device_id: String,
    conn: DeviceConnection,
    telemetry_service: TelemetryService,
    transports: Tracked,
) {
    let config: SerialTc4ConnectionConfig = match serde_json::from_value(conn.config.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%device_id, error = %e, "Invalid serial TC4 connection config");
            return;
        }
    };
    let mut setup = tc4::Tc4Setup::from_config(&config);
    let (_, commands) = crate::transports::ingest::read_config(Kind::Tc4, &conn.config);
    let poll_interval = Duration::from_millis(config.poll_interval_ms.max(250));
    let read_timeout = Duration::from_millis(1500);

    tracing::info!(%device_id, port = %config.port, baud = config.baud, ?poll_interval,
        chan = %setup.chan_line(), reply = ?setup.reply, fan = ?setup.fan,
        "Starting serial TC4 poller");

    let mut backoff = Duration::from_secs(1);
    loop {
        match tokio_serial::new(&config.port, config.baud).open_native_async() {
            Ok(port) => {
                tracing::info!(%device_id, port = %config.port, "Serial TC4 port opened");
                backoff = Duration::from_secs(1);

                let (reader, mut writer) = tokio::io::split(port);
                let mut reader = BufReader::new(reader);

                let mut init_ok = true;
                for line in setup.init_lines() {
                    if let Err(e) = tc4::write_line(&mut writer, &line).await {
                        tracing::warn!(%device_id, error = %e, "Serial TC4 init write failed");
                        init_ok = false;
                        break;
                    }
                }

                if init_ok {
                    let (tx, mut rx) = mpsc::channel::<SerialRequest>(16);
                    let handle = transports
                        .register(
                            &device_id,
                            conn.priority,
                            Arc::new(SerialSink::new(tx, setup.fan).with_mapping(
                                commands.clone(),
                                setup.units,
                                &config.port,
                            )),
                        )
                        .await;

                    let mut next_poll = tokio::time::Instant::now();
                    let mut misses: u32 = 0;
                    loop {
                        tokio::select! {
                            () = tokio::time::sleep_until(next_poll) => {
                                next_poll = tokio::time::Instant::now() + poll_interval;
                                if let Err(e) = tc4::write_line(&mut writer, "READ").await {
                                    tracing::warn!(%device_id, error = %e, "Serial TC4 write failed, reopening");
                                    break;
                                }
                                if setup.columns.is_some() {
                                    // A column naming a declared °C channel follows the unit
                                    setup.temp_channels = transports.registry.setups().temp_channels(&device_id).await;
                                }
                                let mut seen = None;
                                let read = tc4::read_sample_seen(&mut reader, &setup, read_timeout, &mut seen).await;
                                if let Some(line) = seen {
                                    telemetry_service.raw.record(&device_id, Some(&conn.id), "serial_tc4", None, line.as_bytes());
                                }
                                match read {
                                    Ok(Some((_, mut sample))) => {
                                        misses = 0;
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs();
                                        sample.insert("timestamp".into(), json!(now));
                                        telemetry_service
                                            .process_telemetry(&device_id, &serde_json::Value::Object(sample), Some(&DeviceStatus::Active))
                                            .await;
                                    }
                                    Ok(None) => {
                                        misses += 1;
                                        if misses >= 5 {
                                            tracing::warn!(%device_id, "Serial TC4 device not answering READ, reopening port");
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(%device_id, error = %e, "Serial TC4 read failed, reopening");
                                        break;
                                    }
                                }
                            }
                            Some(req) = rx.recv() => {
                                let mut result: Result<(), String> = Ok(());
                                for line in &req.lines {
                                    if let Err(e) = tc4::write_line(&mut writer, line).await {
                                        result = Err(format!("io: {e}"));
                                        break;
                                    }
                                }
                                let io_failed = result.is_err();
                                let _ = req.reply.send(result);
                                if io_failed {
                                    tracing::warn!(%device_id, "Serial TC4 command write failed, reopening");
                                    break;
                                }
                            }
                        }
                    }
                    transports.unregister(&handle).await;
                }
            }
            Err(e) => {
                tracing::warn!(%device_id, port = %config.port, error = %e, backoff_secs = backoff.as_secs(),
                    "Serial TC4 port open failed, retrying");
            }
        }

        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}
