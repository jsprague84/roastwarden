//! Telemetry WebSocket, protocol 2.
//!
//! `GET /ws/v2/telemetry`
//!
//! Text frames (JSON) carry control and low-rate events, binary frames
//! (CBOR) carry samples. Flow:
//!
//! 1. Server sends `{"type":"hello","protocol":2,"server_time_ms":…,"rings":[…]}`
//!    listing every device it has recent samples for, with the oldest and
//!    latest sequence numbers.
//! 2. Client sends `{"type":"subscribe","device_id":"…","since_seq":N}`.
//!    `since_seq` is the last sequence number the client already has (0 or
//!    omitted for "send what you have"); `since_ms` may be given instead;
//!    `limit` caps the catch-up (default: the whole ring).
//! 3. Server replies with one or more binary frames
//!    `{"type":"samples","device_id":…,"samples":[…],"catchup":true,"gap":bool}`
//!    (`gap` is true when `since_seq` is older than the ring covers, so the
//!    client knows a hole exists), then streams live samples as
//!    `{"type":"samples",…,"catchup":false}` frames with one sample each.
//! 4. `replay` and `alarm` events are forwarded as text frames
//!    with the same shape as protocol 1.
//!
//! A client may re-subscribe at any time (e.g. switching devices); the new
//! subscription replaces the old one. Samples are per device; a subscription
//! without `device_id` receives live samples from every device and no
//! catch-up. See `docs/websocket-v2.md`.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use roastwarden_core::Sample;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::analysis::ring::RingInfo;
use crate::app::state::AppState;

/// Random id for this server process (see `TextFrame::Hello::boot_id`).
fn boot_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

/// Samples per catch-up frame.
const CATCHUP_BATCH: usize = 500;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TextFrame<'a> {
    Hello {
        protocol: u8,
        server_time_ms: u64,
        /// Changes on every server start: sequence numbers restart with it,
        /// so a client resuming with a seq from another boot starts over.
        boot_id: &'static str,
        rings: Vec<RingInfo>,
    },
    Subscribed {
        device_id: Option<&'a str>,
        from_seq: u64,
        latest_seq: u64,
        gap: bool,
    },
    Pong {
        server_time_ms: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SamplesFrame {
    #[serde(rename = "type")]
    pub kind: String,
    pub device_id: String,
    pub samples: Vec<Sample>,
    pub catchup: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gap: bool,
}

impl SamplesFrame {
    fn new(device_id: &str, samples: Vec<Sample>, catchup: bool, gap: bool) -> Self {
        Self {
            kind: "samples".to_string(),
            device_id: device_id.to_string(),
            samples,
            catchup,
            gap,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Subscribe {
        #[serde(default)]
        device_id: Option<String>,
        #[serde(default)]
        since_seq: Option<u64>,
        #[serde(default)]
        since_ms: Option<u64>,
        #[serde(default)]
        limit: Option<usize>,
    },
    Ping,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn encode_cbor<T: Serialize>(v: &T) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    // Serialising our own structs cannot fail short of OOM.
    let _ = ciborium::into_writer(v, &mut buf);
    buf
}

fn text<T: Serialize>(v: &T) -> Message {
    Message::Text(serde_json::to_string(v).unwrap_or_default().into())
}

pub async fn ws_telemetry_v2(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| telemetry_ws_v2_loop(state, socket))
}

/// Resolve a subscribe request against the ring: (samples, gap, `from_seq`).
fn catchup_for(
    state: &AppState,
    dev: &str,
    since_seq: Option<u64>,
    since_ms: Option<u64>,
    limit: usize,
) -> (Vec<Sample>, bool, u64) {
    let ts = &state.telemetry_service;
    match (since_seq, since_ms) {
        (Some(seq), _) => (
            ts.samples_since_seq(dev, seq, limit),
            ts.has_gap_before(dev, seq),
            seq,
        ),
        (None, Some(ms)) => (ts.samples_since_ms(dev, ms, limit), false, 0),
        (None, None) => (ts.samples_since_seq(dev, 0, limit), false, 0),
    }
}

async fn telemetry_ws_v2_loop(state: AppState, socket: WebSocket) {
    state.metrics.ws_clients.inc();
    let (mut tx, mut rx) = socket.split();

    let hello = TextFrame::Hello {
        protocol: 2,
        server_time_ms: now_ms(),
        boot_id: boot_id(),
        rings: state.telemetry_service.ring_summary(),
    };
    if tx.send(text(&hello)).await.is_err() {
        state.metrics.ws_clients.dec();
        return;
    }

    let mut telemetry_rx = state.telemetry_service.subscribe();
    let mut replay_rx = state.replay.subscribe();
    let mut alarm_rx = state.alarms.subscribe();
    let mut marks_rx = state.alarms.subscribe_marks();

    // Current subscription: None = all devices, live only.
    let mut subscribed: Option<String> = None;
    // Highest seq already sent for the subscribed device, so a live sample
    // that was queued in the broadcast channel before the catch-up ran is
    // not sent twice.
    let mut last_sent_seq: u64 = 0;

    'conn: loop {
        tokio::select! {
            incoming = rx.next() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    Message::Text(body) => match serde_json::from_str::<ClientFrame>(&body) {
                        Ok(ClientFrame::Ping) => {
                            if tx.send(text(&TextFrame::Pong { server_time_ms: now_ms() })).await.is_err() { break; }
                        }
                        Ok(ClientFrame::Subscribe { device_id, since_seq, since_ms, limit }) => {
                            subscribed = device_id.clone();
                            last_sent_seq = 0;
                            let Some(dev) = device_id.as_deref() else {
                                let ack = TextFrame::Subscribed { device_id: None, from_seq: 0, latest_seq: 0, gap: false };
                                if tx.send(text(&ack)).await.is_err() { break; }
                                continue;
                            };
                            let limit = limit.unwrap_or(usize::MAX).max(1);
                            let (samples, gap, from_seq) = catchup_for(&state, dev, since_seq, since_ms, limit);
                            let latest_seq = state.telemetry_service.ring_info(dev).map_or(0, |i| i.latest_seq);
                            let ack = TextFrame::Subscribed { device_id: Some(dev), from_seq, latest_seq, gap };
                            if tx.send(text(&ack)).await.is_err() { break; }
                            // Never filter live samples against a seq this ring has not
                            // reached (a client resuming from before a restart).
                            last_sent_seq = samples.last().map_or(since_seq.unwrap_or(0).min(latest_seq), |s| s.seq);
                            if samples.is_empty() {
                                // Empty catch-up still tells the client it is now live.
                                let frame = SamplesFrame::new(dev, Vec::new(), true, gap);
                                if tx.send(Message::Binary(encode_cbor(&frame).into())).await.is_err() { break 'conn; }
                            }
                            for (i, chunk) in samples.chunks(CATCHUP_BATCH).enumerate() {
                                let frame = SamplesFrame::new(dev, chunk.to_vec(), true, gap && i == 0);
                                if tx.send(Message::Binary(encode_cbor(&frame).into())).await.is_err() { break 'conn; }
                            }
                        }
                        Err(e) => {
                            let f = TextFrame::Error { message: format!("bad frame: {e}") };
                            if tx.send(text(&f)).await.is_err() { break; }
                        }
                    },
                    Message::Close(_) => break,
                    Message::Ping(p) => { if tx.send(Message::Pong(p)).await.is_err() { break; } }
                    _ => {}
                }
            }
            evt = telemetry_rx.recv() => {
                match evt {
                    Ok(te) => {
                        if let Some(dev) = subscribed.as_deref() {
                            if te.device_id != dev { continue; }
                            if te.sample.seq <= last_sent_seq { continue; }
                            last_sent_seq = te.sample.seq;
                        }
                        let frame = SamplesFrame::new(&te.device_id, vec![te.sample], false, false);
                        if tx.send(Message::Binary(encode_cbor(&frame).into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "v2 WS client lagged; client should resubscribe from its last seq");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            replay_evt = replay_rx.recv() => {
                match replay_evt {
                    Ok(st) => {
                        if subscribed.as_deref().is_some_and(|d| d != st.device_id) { continue; }
                        let body = serde_json::json!({"type": "replay", "device_id": st.device_id, "replay": st});
                        if tx.send(text(&body)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            alarm_evt = alarm_rx.recv() => {
                match alarm_evt {
                    Ok(alarm) => {
                        if subscribed.as_deref().is_some_and(|d| d != alarm.device_id) { continue; }
                        let body = serde_json::json!({"type": "alarm", "device_id": alarm.device_id, "alarm": alarm});
                        if tx.send(text(&body)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            marks_evt = marks_rx.recv() => {
                match marks_evt {
                    Ok(m) => {
                        let body = serde_json::json!({"type": "session_events", "session_id": m.session_id});
                        if tx.send(text(&body)).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    let _ = tx.close().await;
    state.metrics.ws_clients.dec();
}

// ---------------------------------------------------------------------------
// HTTP access to the same ring (fallback, tests, scripts)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SamplesQuery {
    /// Samples after this sequence number
    pub since_seq: Option<u64>,
    /// Samples after this time, Unix ms
    pub since_ms: Option<u64>,
    /// At most this many
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SamplesResponse {
    pub device_id: String,
    pub count: usize,
    pub latest_seq: u64,
    pub gap: bool,
    pub samples: Vec<Sample>,
}

/// The live stream's REST twin (docs/websocket-v2.md).
pub fn routes() -> utoipa_axum::router::OpenApiRouter<AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(utoipa_axum::routes!(api_get_samples))
        .routes(utoipa_axum::routes!(api_get_rings))
}

/// The roaster's recent samples from the server's 2 h memory (the WebSocket's catch-up); `gap`
/// says older ones the request asked for are gone.
#[utoipa::path(get, path = "/api/roaster/{device_id}/samples", tag = "telemetry",
    params(("device_id" = String, Path), SamplesQuery),
    responses((status = 200, body = SamplesResponse),
        (status = 404, description = "No samples from this roaster", body = crate::http::errors::ErrorBody)))]
pub async fn api_get_samples(
    Path(device_id): Path<String>,
    State(state): State<AppState>,
    Query(q): Query<SamplesQuery>,
) -> Response {
    let Some(info) = state.telemetry_service.ring_info(&device_id) else {
        return (StatusCode::NOT_FOUND, "no samples for device").into_response();
    };
    let limit = q.limit.unwrap_or(usize::MAX).max(1);
    let (samples, gap, _) = catchup_for(&state, &device_id, q.since_seq, q.since_ms, limit);
    Json(SamplesResponse {
        device_id,
        count: samples.len(),
        latest_seq: info.latest_seq,
        gap,
        samples,
    })
    .into_response()
}

/// What the server holds in memory, per roaster.
#[utoipa::path(get, path = "/api/telemetry/rings", tag = "telemetry",
    responses((status = 200, body = Vec<RingInfo>)))]
pub async fn api_get_rings(State(state): State<AppState>) -> Json<Vec<RingInfo>> {
    Json(state.telemetry_service.ring_summary())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_frame_round_trips_through_cbor() {
        let frame = SamplesFrame::new(
            "dev",
            vec![
                Sample {
                    seq: 1,
                    ts_ms: 10,
                    bt: Some(100.5),
                    heater: Some(50.0),
                    ..Default::default()
                },
                Sample {
                    seq: 2,
                    ts_ms: 11,
                    bt: Some(101.0),
                    fan: Some(78.4),
                    ..Default::default()
                },
            ],
            true,
            true,
        );
        let bytes = encode_cbor(&frame);
        // Compact: two samples with a handful of fields stay well under 200 bytes.
        assert!(bytes.len() < 200, "frame was {} bytes", bytes.len());
        let back: SamplesFrame = ciborium::from_reader(&bytes[..]).unwrap();
        assert_eq!(back.samples, frame.samples);
        assert!(back.gap && back.catchup);
    }

    /// The golden frame the dashboard's vitest decodes with cbor-x
    /// (`lib/stores/telemetry.test.ts`): a wire change on either side fails a test.
    fn golden_frame() -> SamplesFrame {
        SamplesFrame::new(
            "esp32_roaster_01",
            vec![Sample {
                seq: 3,
                ts_ms: 1_792_580_333_555,
                bt: Some(201.25),
                et: Some(230.5),
                ror_bt: Some(8.75),
                heater: Some(100.0),
                fan: Some(72.0),
                setpoint: Some(205.0),
                mode: Some(1),
                heater_enable: Some(true),
                extras: [("pid_out".to_string(), 0.5)].into(),
                ..Default::default()
            }],
            false,
            true,
        )
    }

    #[test]
    fn samples_frame_matches_the_golden_bytes() {
        use std::fmt::Write as _;
        let hex = encode_cbor(&golden_frame())
            .iter()
            .fold(String::new(), |mut s, b| {
                let _ = write!(s, "{b:02x}");
                s
            });
        let golden = include_str!("../../testdata/ws2-samples-frame.cbor.hex").trim();
        assert_eq!(
            hex, golden,
            "the CBOR wire format changed; update the fixture and the dashboard together"
        );
    }

    #[test]
    fn client_frames_parse() {
        let f: ClientFrame =
            serde_json::from_str(r#"{"type":"subscribe","device_id":"a","since_seq":5}"#).unwrap();
        assert!(matches!(
            f,
            ClientFrame::Subscribe {
                since_seq: Some(5),
                ..
            }
        ));
        let f: ClientFrame = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(f, ClientFrame::Ping));
        assert!(serde_json::from_str::<ClientFrame>(r#"{"type":"nope"}"#).is_err());
    }
}
