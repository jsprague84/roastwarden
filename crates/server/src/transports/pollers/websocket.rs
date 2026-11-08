//! The WebSocket client: a device polled or streamed over its own WebSocket server, with
//! the device's field mapping and command templates.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::analysis::telemetry::TelemetryService;
use crate::roasters::types::{DeviceConnection, DeviceStatus};
use crate::transports::ingest::Kind;
use crate::transports::transport::WebSocketSink;
use crate::transports::types::WebSocketConnectionConfig;

use super::Tracked;

/// The shortest interval a WebSocket poll may ask for.
pub(super) const MIN_WS_POLL_MS: u64 = 200;

/// A poll's frame with its `{id}` (docs/ingest-mapping.md, "The WebSocket client").
pub(super) fn poll_message(message: &str, id: u64) -> String {
    message.replace("{id}", &id.to_string())
}

pub(super) async fn websocket_client_loop(
    device_id: String,
    conn: DeviceConnection,
    telemetry_service: TelemetryService,
    transports: Tracked,
) {
    let config: WebSocketConnectionConfig = match serde_json::from_value(conn.config.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%device_id, error = %e, "Invalid WebSocket connection config");
            return;
        }
    };

    let (ingest, commands) = crate::transports::ingest::read_config(Kind::WebSocket, &conn.config);
    let units = ingest.as_ref().map(|i| i.units).unwrap_or_default();
    tracing::info!(%device_id, url = %config.url, mapped = ingest.is_some(),
        poll = config.poll.is_some(), "Starting WebSocket device client");

    let mut backoff = Duration::from_secs(1);
    // The poll's `{id}`: one counter for the task's life, so ids never repeat after a reconnect.
    let mut poll_id: u64 = 0;

    loop {
        match tokio_tungstenite::connect_async(&config.url).await {
            Ok((ws_stream, _)) => {
                tracing::info!(%device_id, "WebSocket device connected");
                backoff = Duration::from_secs(1);

                let (mut write, mut read) = ws_stream.split();

                // Control path: a writer task drains the sink's channel into the socket.
                let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                let writer_device = device_id.clone();
                let writer = tokio::spawn(async move {
                    while let Some(frame) = rx.recv().await {
                        if let Err(e) = write
                            .send(tokio_tungstenite::tungstenite::Message::Text(frame.into()))
                            .await
                        {
                            tracing::warn!(device_id = %writer_device, error = %e, "WebSocket device write failed");
                            break;
                        }
                    }
                });
                let poll_tx = tx.clone();
                let sink = Arc::new(
                    WebSocketSink::new("websocket", tx).with_mapping(commands.clone(), units),
                );
                let handle = transports.register(&device_id, conn.priority, sink).await;
                // A device that answers requests (Artisan's protocol) is asked every interval.
                let mut poll_every = config.poll.as_ref().map(|p| {
                    let mut i = tokio::time::interval(Duration::from_millis(
                        p.interval_ms.max(MIN_WS_POLL_MS),
                    ));
                    i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    i
                });
                let poll_text = config.poll.as_ref().map(|p| p.message.clone());

                loop {
                    let msg = tokio::select! {
                        m = read.next() => m,
                        () = async {
                            match poll_every.as_mut() {
                                Some(i) => { i.tick().await; }
                                None => std::future::pending::<()>().await,
                            }
                        } => {
                            if let Some(message) = &poll_text {
                                poll_id += 1;
                                let _ = poll_tx.send(poll_message(message, poll_id));
                            }
                            continue;
                        }
                    };
                    let Some(msg) = msg else { break };
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            telemetry_service.raw.record(
                                &device_id,
                                Some(&conn.id),
                                "websocket",
                                None,
                                text.as_bytes(),
                            );
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(val) => {
                                    // With a mapping, only its fields pass; a frame with
                                    // neither BT nor ET (an answer to a command) is no sample.
                                    let sample = match &ingest {
                                        Some(i) => i.map(&val).map(serde_json::Value::Object),
                                        None => Some(val),
                                    };
                                    if let Some(sample) = sample {
                                        telemetry_service
                                            .process_telemetry(
                                                &device_id,
                                                &sample,
                                                Some(&DeviceStatus::Active),
                                            )
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(%device_id, error = %e,
                                        "Invalid JSON from WebSocket device");
                                }
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                            tracing::info!(%device_id, "WebSocket device closed connection");
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(%device_id, error = %e, "WebSocket device error");
                            break;
                        }
                        _ => {} // Ignore binary, ping, pong
                    }
                }

                transports.unregister(&handle).await;
                writer.abort();
            }
            Err(e) => {
                tracing::warn!(%device_id, error = %e, backoff_secs = backoff.as_secs(),
                    "WebSocket device connect failed, retrying");
            }
        }

        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}
