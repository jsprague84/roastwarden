//! The WebSockets: the dashboard's telemetry (JSON), the MQTT debug feed and a device's own
//! connection that pushes telemetry and takes commands.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::broadcast;

use crate::app::state::AppState;
use crate::roasters::types::DeviceStatus;
use crate::transports;

pub(crate) async fn ws_telemetry(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| telemetry_ws_loop(state, socket))
}

pub(crate) async fn ws_debug(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| debug_ws_loop(state, socket))
}

async fn telemetry_ws_loop(state: AppState, mut socket: WebSocket) {
    // Count WS client
    state.metrics.ws_clients.inc();
    tracing::info!(
        "WebSocket telemetry client connected, total: {}",
        state.metrics.ws_clients.get()
    );

    // Subscribe to unified telemetry broadcast (covers MQTT, device WS, Modbus)
    let mut telemetry_rx = state.telemetry_service.subscribe();
    // Server-side profile replay state
    let mut replay_rx = state.replay.subscribe();
    // Server-side alarms
    let mut alarm_rx = state.alarms.subscribe();

    loop {
        tokio::select! {
            alarm_evt = alarm_rx.recv() => {
                match alarm_evt {
                    Ok(alarm) => {
                        let msg_text = serde_json::json!({
                            "device_id": alarm.device_id,
                            "alarm": alarm,
                        }).to_string();
                        if socket.send(Message::Text(msg_text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            replay_evt = replay_rx.recv() => {
                match replay_evt {
                    Ok(st) => {
                        let msg_text = serde_json::json!({
                            "device_id": st.device_id,
                            "replay": st,
                        }).to_string();
                        if socket.send(Message::Text(msg_text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            evt = telemetry_rx.recv() => {
                match evt {
                    Ok(te) => {
                        let msg_text = serde_json::json!({
                            "device_id": te.device_id,
                            "telemetry": te.payload,
                        }).to_string();
                        if socket.send(Message::Text(msg_text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "Dashboard WS client lagged, dropped messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
    state.metrics.ws_clients.dec();
}

async fn debug_ws_loop(state: AppState, mut socket: WebSocket) {
    use axum::extract::ws::Message;
    use tokio::select;

    // Count WS client
    state.metrics.ws_clients.inc();
    tracing::info!("Debug WebSocket client connected");

    let mut rx = state.mqtt.events();

    loop {
        select! {
            // Handle incoming WebSocket messages from client
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle ping messages from client
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            if parsed.get("type").and_then(|v| v.as_str()) == Some("ping") {
                                let pong = serde_json::json!({"type": "pong"});
                                if socket.send(Message::Text(pong.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Debug WebSocket client disconnected gracefully");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(?e, "Debug WebSocket error reading message");
                        break;
                    }
                    _ => {
                        // Ignore other message types like binary, ping, pong
                    }
                }
            }
            // Handle MQTT events and forward to client
            mqtt_evt = rx.recv() => {
                let Ok(evt) = mqtt_evt else {
                        tracing::warn!("Debug WebSocket: MQTT event channel closed");
                        break;
                };

                let msg = match evt {
                    roastwarden_mqtt::MqttEvent::Publish { topic, payload } => {
                        // Parse device ID from topic if it's a roaster topic
                        let device_id = if topic.starts_with("roaster/") {
                            topic.split('/').nth(1).map(std::string::ToString::to_string)
                        } else {
                            None
                        };

                        // Try to parse payload as JSON, otherwise use raw string
                        let payload_value = match serde_json::from_slice::<serde_json::Value>(&payload) {
                            Ok(json) => json,
                            Err(_) => serde_json::Value::String(String::from_utf8_lossy(&payload).to_string())
                        };

                        serde_json::json!({
                            "mqtt": {
                                "topic": topic,
                                "payload": payload_value,
                                "direction": "incoming",
                                "device_id": device_id
                            }
                        })
                    }
                    roastwarden_mqtt::MqttEvent::Connected => {
                        serde_json::json!({
                            "mqtt": {
                                "topic": "system/connected",
                                "payload": "MQTT connected",
                                "direction": "incoming"
                            }
                        })
                    }
                    roastwarden_mqtt::MqttEvent::Disconnected => {
                        serde_json::json!({
                            "mqtt": {
                                "topic": "system/disconnected",
                                "payload": "MQTT disconnected",
                                "direction": "incoming"
                            }
                        })
                    }
                    roastwarden_mqtt::MqttEvent::PubAck(packet_id) => {
                        serde_json::json!({
                            "mqtt": {
                                "topic": "system/puback",
                                "payload": format!("PubAck received for packet {}", packet_id),
                                "direction": "incoming"
                            }
                        })
                    }
                };

                if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                    tracing::info!("Debug WebSocket client disconnected during send");
                    break;
                }
            }
        }
    }

    let _ = socket.send(Message::Close(None)).await;
    state.metrics.ws_clients.dec();
    tracing::info!("Debug WebSocket connection closed");
}

pub(crate) async fn ws_device_telemetry(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate device exists and is active
    match crate::roasters::registry::get_device_by_device_id(&state.db, &device_id).await {
        Ok(Some(dev)) => {
            if dev.device.status != DeviceStatus::Active {
                return (
                    StatusCode::FORBIDDEN,
                    format!(
                        "Device '{}' is not active (status: {})",
                        device_id, dev.device.status
                    ),
                )
                    .into_response();
            }
            ws.on_upgrade(move |socket| device_ws_loop(state, device_id, socket))
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            format!("Device '{device_id}' not found"),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(%device_id, error = %e, "Failed to look up device for WebSocket");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
        }
    }
}

async fn device_ws_loop(state: AppState, device_id: String, mut socket: WebSocket) {
    tracing::info!(%device_id, "Device WebSocket connected");

    // Control path: commands for this device are delivered through the transport registry.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let sink = Arc::new(transports::transport::WebSocketSink::new(
        "device_websocket",
        tx,
    ));
    let handle = state.transports.register(&device_id, 0, sink).await;

    loop {
        tokio::select! {
            // Incoming telemetry from device
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        state.telemetry_service.raw.record(&device_id, None, "websocket", None, text.as_bytes());
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(val) => {
                                // Look up device status
                                let device_status = crate::roasters::registry::get_device_by_device_id(&state.db, &device_id).await
                                    .ok()
                                    .flatten()
                                    .map(|d| d.device.status);

                                state.telemetry_service.process_telemetry(
                                    &device_id,
                                    &val,
                                    device_status.as_ref(),
                                ).await;
                            }
                            Err(_) => {
                                tracing::warn!(%device_id, "Invalid JSON from device WebSocket");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!(%device_id, "Device WebSocket disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(%device_id, error = %e, "Device WebSocket error");
                        break;
                    }
                    _ => {} // Ignore binary, ping, pong
                }
            }

            // Outgoing control commands to device
            Some(cmd) = rx.recv() => {
                if socket.send(Message::Text(cmd.into())).await.is_err() {
                    tracing::warn!(%device_id, "Failed to send control command via device WebSocket");
                    break;
                }
            }
        }
    }

    // Cleanup: remove sender and close socket
    state.transports.unregister(&handle).await;
    let _ = socket.send(Message::Close(None)).await;
    tracing::info!(%device_id, "Device WebSocket cleaned up");
}
