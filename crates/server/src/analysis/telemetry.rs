use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use prometheus::IntGaugeVec;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

use roastwarden_core::Sample;

use crate::analysis::ring::{RingInfo, Rings};
use crate::roasters::types::DeviceStatus;

/// Event broadcast when any device sends telemetry (from any protocol).
#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub device_id: String,
    /// Enriched v1 JSON (camelCase), what `/ws/telemetry` clients receive.
    pub payload: serde_json::Value,
    /// The same reading as a typed, sequenced sample (v2 stream).
    pub sample: Sample,
}

/// Typed telemetry struct matching the ESP32 JSON output.
/// Fields use camelCase serde rename to match the ESP32 firmware.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPayload {
    pub bean_temp: f64,
    pub env_temp: f64,
    #[serde(default)]
    pub rate_of_rise: Option<f64>,
    /// Heater command, %.
    pub heater: f64,
    /// %
    #[serde(default)]
    pub heater_applied: Option<f64>,
    /// Fan, %.
    pub fan: f64,
    pub setpoint: f64,
    pub control_mode: i32,
    pub heater_enable: i32,
    #[serde(default)]
    pub uptime: Option<u64>,
    #[serde(default, rename = "Kp")]
    pub kp: Option<f64>,
    #[serde(default, rename = "Ki")]
    pub ki: Option<f64>,
    #[serde(default, rename = "Kd")]
    pub kd: Option<f64>,
    #[serde(default)]
    pub free_heap: Option<u64>,
    #[serde(default)]
    pub rssi: Option<i64>,
    #[serde(default)]
    pub system_status: Option<i32>,
    #[serde(default)]
    pub timestamp: Option<u64>,
}

/// Attempt to deserialize a telemetry payload, logging warnings for unknown fields.
#[allow(dead_code)] // Utility for future typed telemetry processing
pub fn parse_telemetry(payload: &[u8]) -> Option<TelemetryPayload> {
    // First try strict deserialization
    match serde_json::from_slice::<TelemetryPayload>(payload) {
        Ok(t) => Some(t),
        Err(e) => {
            // Try as generic JSON to detect unknown fields
            if let Ok(raw) = serde_json::from_slice::<serde_json::Value>(payload) {
                tracing::warn!(
                    error = %e,
                    "Telemetry deserialization failed, payload has unexpected structure"
                );
                // Try a more lenient parse with deny_unknown_fields disabled (default)
                serde_json::from_value(raw).ok()
            } else {
                tracing::warn!(error = %e, "Telemetry payload is not valid JSON");
                None
            }
        }
    }
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// `ROASTWARDEN_RING_SAMPLES` overrides the per-device in-memory history size.
fn ring_capacity_from_env() -> usize {
    std::env::var("ROASTWARDEN_RING_SAMPLES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| (60..=100_000).contains(n))
        .unwrap_or(crate::analysis::ring::DEFAULT_CAPACITY)
}

fn epoch_secs_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

// ---------------------------------------------------------------------------
// Rate of rise
//
// One canonical RoR is computed here, on the server, for every transport, so
// the live chart, the stored session telemetry and the phase statistics all
// agree. The firmware's own estimate (if any) is kept as `deviceRateOfRise`.
// The algorithms mirror Artisan's delta-span smoothing options.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RorAlgorithm {
    /// Endpoint difference over the window.
    MovingAverage,
    /// Linearly weighted least-squares slope; recent samples count more.
    WeightedMovingAverage,
    /// Second-order polynomial fit, derivative at the window centre.
    SavitzkyGolay,
}

impl std::str::FromStr for RorAlgorithm {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "moving_average" => Ok(RorAlgorithm::MovingAverage),
            "weighted_moving_average" => Ok(RorAlgorithm::WeightedMovingAverage),
            "savitzky_golay" => Ok(RorAlgorithm::SavitzkyGolay),
            other => Err(format!("unknown RoR algorithm: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RorConfig {
    pub window_secs: f64,
    pub algorithm: RorAlgorithm,
}

impl Default for RorConfig {
    fn default() -> Self {
        Self {
            window_secs: 30.0,
            algorithm: RorAlgorithm::MovingAverage,
        }
    }
}

/// Per-device bean-temperature history used for RoR.
#[derive(Default)]
pub struct RorTracker {
    samples: VecDeque<(f64, f64)>,
}

const ROR_HISTORY_SECS: f64 = 180.0;

impl RorTracker {
    pub fn push(&mut self, t: f64, bean_temp: f64) {
        // Ignore out-of-order or duplicate timestamps.
        if let Some(&(last_t, _)) = self.samples.back()
            && t <= last_t
        {
            return;
        }
        self.samples.push_back((t, bean_temp));
        while let Some(&(t0, _)) = self.samples.front() {
            if t - t0 > ROR_HISTORY_SECS {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// RoR in °C/min over the configured window, rounded to 0.1.
    pub fn rate(&self, cfg: &RorConfig) -> Option<f64> {
        let now = self.samples.back()?.0;
        let start = now - cfg.window_secs;
        let window: Vec<(f64, f64)> = self
            .samples
            .iter()
            .copied()
            .filter(|(t, _)| *t >= start)
            .collect();
        let v = match cfg.algorithm {
            RorAlgorithm::MovingAverage => ror_moving_average(&window),
            RorAlgorithm::WeightedMovingAverage => ror_weighted(&window),
            RorAlgorithm::SavitzkyGolay => ror_savitzky_golay(&window),
        }?;
        if !v.is_finite() {
            return None;
        }
        Some((v * 10.0).round() / 10.0)
    }
}

pub fn ror_moving_average(w: &[(f64, f64)]) -> Option<f64> {
    if w.len() < 2 {
        return None;
    }
    let (t0, y0) = w[0];
    let (t1, y1) = w[w.len() - 1];
    let dt = t1 - t0;
    if dt <= 0.0 {
        return None;
    }
    Some((y1 - y0) / dt * 60.0)
}

pub fn ror_weighted(w: &[(f64, f64)]) -> Option<f64> {
    if w.len() < 2 {
        return None;
    }
    let n = w.len();
    let w_sum = (n * (n + 1)) as f64 / 2.0;
    let t0 = w[0].0;
    let (mut sw, mut swt, mut swy, mut swtt, mut swty) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, &(t, y)) in w.iter().enumerate() {
        let wt = (i + 1) as f64 / w_sum;
        let t = t - t0;
        sw += wt;
        swt += wt * t;
        swy += wt * y;
        swtt += wt * t * t;
        swty += wt * t * y;
    }
    let denom = sw * swtt - swt * swt;
    if denom.abs() < 1e-12 {
        return None;
    }
    Some((sw * swty - swt * swy) / denom * 60.0)
}

pub fn ror_savitzky_golay(w: &[(f64, f64)]) -> Option<f64> {
    if w.len() < 3 {
        return ror_moving_average(w);
    }
    let n = w.len();
    let t_mid = w[n / 2].0;
    let (mut s0, mut s1, mut s2, mut s3, mut s4) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut r0, mut r1, mut r2) = (0.0, 0.0, 0.0);
    for &(t, y) in w {
        let t = t - t_mid;
        let t2 = t * t;
        s0 += 1.0;
        s1 += t;
        s2 += t2;
        s3 += t * t2;
        s4 += t2 * t2;
        r0 += y;
        r1 += t * y;
        r2 += t2 * y;
    }
    // Solve the 3x3 normal equations for [a, b, c] in y = a + b t + c t²; b is the slope at t_mid.
    let det = s0 * (s2 * s4 - s3 * s3) - s1 * (s1 * s4 - s2 * s3) + s2 * (s1 * s3 - s2 * s2);
    if det.abs() < 1e-12 {
        return None;
    }
    let det_b = s0 * (r1 * s4 - r2 * s3) - r0 * (s1 * s4 - s2 * s3) + s2 * (s1 * r2 - s2 * r1);
    Some(det_b / det * 60.0)
}

/// Shared telemetry processing service used by MQTT consumer, device WebSocket handler,
/// and (future) Modbus TCP polling. Handles cache updates, DB persistence, session
/// recording, metrics, and debounced last-seen updates.
#[derive(Clone)]
pub struct TelemetryService {
    pub(crate) telemetry_cache: Arc<RwLock<HashMap<String, (serde_json::Value, u64)>>>,
    db: SqlitePool,
    telemetry_last_seen: IntGaugeVec,
    last_seen_debounce: Arc<std::sync::Mutex<HashMap<String, Instant>>>,
    /// Broadcast channel for all processed telemetry events (any protocol).
    telemetry_tx: broadcast::Sender<TelemetryEvent>,
    ror_config: Arc<std::sync::RwLock<RorConfig>>,
    ror_trackers: Arc<std::sync::Mutex<HashMap<String, RorTracker>>>,
    /// ET rate of rise, same algorithm as BT.
    ror_et_trackers: Arc<std::sync::Mutex<HashMap<String, RorTracker>>>,
    /// Recent samples per device for instant history and reconnect catch-up.
    rings: Arc<Rings>,
    /// Each roaster's setup: its declared channels are kept with a roast (docs/channels.md).
    setups: Arc<crate::roasters::setup::Setups>,
    /// The newest raw message of each roaster's connection (docs/add-roaster.md)
    pub(crate) raw: Arc<crate::roasters::raw::RawStore>,
    /// Per roaster, the roast whose cooling is still recorded after Complete: the roast
    /// sequence sets it at DROP and clears it at COOL END (docs/roast-sequence.md).
    cooling_roasts: Arc<std::sync::RwLock<HashMap<String, String>>>,
}

impl TelemetryService {
    pub fn new(
        telemetry_cache: Arc<RwLock<HashMap<String, (serde_json::Value, u64)>>>,
        db: SqlitePool,
        telemetry_last_seen: IntGaugeVec,
        setups: Arc<crate::roasters::setup::Setups>,
    ) -> Self {
        let (telemetry_tx, _) = broadcast::channel(256);
        Self {
            telemetry_cache,
            db,
            telemetry_last_seen,
            last_seen_debounce: Arc::new(std::sync::Mutex::new(HashMap::new())),
            telemetry_tx,
            ror_config: Arc::new(std::sync::RwLock::new(RorConfig::default())),
            ror_trackers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ror_et_trackers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            rings: Arc::new(Rings::new(ring_capacity_from_env())),
            setups,
            raw: Arc::new(crate::roasters::raw::RawStore::default()),
            cooling_roasts: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Keep storing this roaster's samples for `session_id` once it is complete, until
    /// `cooling_over` (the roast sequence's cooling: a roast completed a second after DROP
    /// would otherwise end its trend at DROP while the roaster cools for minutes).
    pub fn record_cooling(&self, device_id: &str, session_id: &str) {
        self.cooling_roasts
            .write()
            .expect("a telemetry lock is poisoned")
            .insert(device_id.to_string(), session_id.to_string());
    }

    /// The cooling is over (or ended by a new roast): a completed roast gets no more samples.
    pub fn cooling_over(&self, device_id: &str) {
        self.cooling_roasts
            .write()
            .expect("a telemetry lock is poisoned")
            .remove(device_id);
    }

    /// The roast whose cooling this roaster still records, if any.
    pub fn cooling_roast(&self, device_id: &str) -> Option<String> {
        self.cooling_roasts
            .read()
            .expect("a telemetry lock is poisoned")
            .get(device_id)
            .cloned()
    }

    /// Recent samples with `seq > since_seq` (oldest first).
    pub fn samples_since_seq(&self, device_id: &str, since_seq: u64, limit: usize) -> Vec<Sample> {
        self.rings.since_seq(device_id, since_seq, limit)
    }

    /// Recent samples with `ts_ms >= since_ms` (oldest first).
    pub fn samples_since_ms(&self, device_id: &str, since_ms: u64, limit: usize) -> Vec<Sample> {
        self.rings.since_ms(device_id, since_ms, limit)
    }

    /// True if resuming from `since_seq` would skip samples the ring no longer holds.
    pub fn has_gap_before(&self, device_id: &str, since_seq: u64) -> bool {
        self.rings.has_gap_before(device_id, since_seq)
    }

    /// The newest sample for a device, from the in-memory ring.
    pub fn latest_sample(&self, device_id: &str) -> Option<roastwarden_core::Sample> {
        self.rings.latest(device_id)
    }

    pub fn ring_info(&self, device_id: &str) -> Option<RingInfo> {
        self.rings.info(device_id)
    }

    pub fn ring_summary(&self) -> Vec<RingInfo> {
        self.rings.summary()
    }

    pub fn ror_config(&self) -> RorConfig {
        *self
            .ror_config
            .read()
            .expect("a telemetry lock is poisoned")
    }

    pub fn set_ror_config(&self, cfg: RorConfig) {
        *self
            .ror_config
            .write()
            .expect("a telemetry lock is poisoned") = cfg;
    }

    /// Read `ror_window_seconds` / `ror_smoothing_algorithm` from the settings table.
    pub async fn load_ror_settings(&self) {
        let mut cfg = RorConfig::default();
        if let Ok(Some(v)) = sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'ror_window_seconds'",
        )
        .fetch_optional(&self.db)
        .await
            && let Ok(w) = v.parse::<f64>()
            && (5.0..=120.0).contains(&w)
        {
            cfg.window_secs = w;
        }
        if let Ok(Some(v)) = sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'ror_smoothing_algorithm'",
        )
        .fetch_optional(&self.db)
        .await
            && let Ok(a) = v.parse::<RorAlgorithm>()
        {
            cfg.algorithm = a;
        }
        tracing::info!(window_secs = cfg.window_secs, algorithm = ?cfg.algorithm, "RoR settings loaded");
        self.set_ror_config(cfg);
    }

    /// Add the canonical `rateOfRise` to a telemetry payload. The device's own
    /// value, if any, moves to `deviceRateOfRise`.
    fn enrich_with_ror(&self, device_id: &str, payload: &serde_json::Value) -> serde_json::Value {
        let mut out = payload.clone();
        let Some(obj) = out.as_object_mut() else {
            return out;
        };
        let Some(bt) = obj.get("beanTemp").and_then(serde_json::Value::as_f64) else {
            return out;
        };
        if !bt.is_finite() {
            return out;
        }
        let cfg = self.ror_config();
        let computed = {
            let mut trackers = self
                .ror_trackers
                .lock()
                .expect("a telemetry lock is poisoned");
            let tracker = trackers.entry(device_id.to_string()).or_default();
            tracker.push(epoch_secs_f64(), bt);
            tracker.rate(&cfg)
        };
        let computed_et = obj
            .get("envTemp")
            .and_then(serde_json::Value::as_f64)
            .filter(|v| v.is_finite())
            .and_then(|et| {
                let mut trackers = self
                    .ror_et_trackers
                    .lock()
                    .expect("a telemetry lock is poisoned");
                let tracker = trackers.entry(device_id.to_string()).or_default();
                tracker.push(epoch_secs_f64(), et);
                tracker.rate(&cfg)
            });
        if let Some(v) = computed_et {
            obj.insert("rateOfRiseEt".to_string(), serde_json::json!(v));
        }
        if let Some(device_ror) = obj.remove("rateOfRise") {
            obj.insert("deviceRateOfRise".to_string(), device_ror);
        }
        match computed {
            Some(v) => {
                obj.insert("rateOfRise".to_string(), serde_json::json!(v));
            }
            None => {
                // Not enough history yet: fall back to the device's estimate so the
                // field is still present early in a roast.
                if let Some(d) = obj.get("deviceRateOfRise").cloned() {
                    obj.insert("rateOfRise".to_string(), d);
                }
            }
        }
        out
    }

    /// Subscribe to all processed telemetry events.
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryEvent> {
        self.telemetry_tx.subscribe()
    }

    /// Process incoming telemetry from any protocol (MQTT, WebSocket, Modbus).
    /// Updates telemetry cache, persists to DB, records to active sessions,
    /// updates metrics, and performs debounced last-seen updates.
    pub async fn process_telemetry(
        &self,
        device_id: &str,
        payload: &serde_json::Value,
        device_status: Option<&DeviceStatus>,
    ) {
        let now = epoch_secs();
        let now_ms = epoch_millis();
        let enriched = self.enrich_with_ror(device_id, payload);
        let payload = &enriched;
        let sample = self
            .rings
            .push(device_id, Sample::from_telemetry_json(0, now_ms, payload));

        // Update metric
        self.telemetry_last_seen
            .with_label_values(&[device_id])
            .set(now as i64);

        // Always update telemetry cache
        self.telemetry_cache
            .write()
            .await
            .insert(device_id.to_string(), (payload.clone(), now));

        // Broadcast to dashboard WebSocket clients
        let _ = self.telemetry_tx.send(TelemetryEvent {
            device_id: device_id.to_string(),
            payload: payload.clone(),
            sample,
        });

        // Record to the roast running on the roaster, and to the one it still cools after
        // Complete (skip for disabled devices)
        let is_disabled = device_status == Some(&DeviceStatus::Disabled);
        if !is_disabled {
            let payload_str = serde_json::to_string(payload).unwrap_or_default();
            // The declared channels, from the in-memory setups (no SQL per sample).
            let declared = self.setups.channels(device_id).await;
            let keys: Vec<&str> = declared.iter().map(|c| c.key.as_str()).collect();
            let extras = control_extras(payload, &keys);
            // Elapsed to the millisecond (julianday keeps the start's fraction; strftime('%s')
            // dropped it, so samples sat up to a second off the event marks).
            let now_s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(now as f64, |d| d.as_secs_f64());
            let point = StoredPoint {
                now,
                now_s,
                payload: &payload_str,
                extras: &extras,
                device_id,
            };
            self.store_point(&point, RoastFilter::Active).await;
            if let Some(sid) = self.cooling_roast(device_id) {
                self.store_point(&point, RoastFilter::Completed(&sid)).await;
            }
        }

        // Debounced last-seen update (at most once per 10 seconds per device)
        let debounce_interval = std::time::Duration::from_secs(10);
        let should_update = {
            let debounce = self
                .last_seen_debounce
                .lock()
                .expect("a telemetry lock is poisoned");
            debounce
                .get(device_id)
                .is_none_or(|last| last.elapsed() >= debounce_interval)
        };
        if should_update {
            if let Err(e) = crate::roasters::registry::update_last_seen(&self.db, device_id).await {
                tracing::warn!(%device_id, error = %e, "Failed to update last_seen");
            }
            self.last_seen_debounce
                .lock()
                .expect("a telemetry lock is poisoned")
                .insert(device_id.to_string(), Instant::now());
        }
    }
}

/// The control fields a roast keeps with each sample (migration 037), only those the
/// device sent, and the roaster's declared channels (`channels`, docs/channels.md): JSON, or
/// None when it sent none of them. Drum speed and damper (a drum roaster's outputs,
/// docs/ingest-mapping.md) are kept here too.
fn control_extras(payload: &serde_json::Value, channels: &[&str]) -> Option<String> {
    const KEYS: [&str; 11] = [
        "heaterApplied",
        "drum",
        "damper",
        "rorLimited",
        "Kp",
        "Ki",
        "Kd",
        "controlMode",
        "heaterEnable",
        "stopCode",
        "ff",
    ];
    let obj = payload.as_object()?;
    let mut m: serde_json::Map<String, serde_json::Value> = KEYS
        .iter()
        .filter_map(|k| {
            obj.get(*k)
                .filter(|v| v.is_number() || v.is_boolean())
                .map(|v| (k.to_string(), v.clone()))
        })
        .collect();
    for k in channels {
        if let Some(v) = crate::roasters::channels::value_in(payload, k) {
            m.insert(k.to_string(), serde_json::json!(v));
        }
    }
    (!m.is_empty()).then(|| serde_json::Value::Object(m).to_string())
}

/// Which roast a sample is stored for: the one running on the roaster, or a completed one
/// whose cooling is still recorded.
enum RoastFilter<'a> {
    Active,
    Completed(&'a str),
}

/// One sample as the store writes it, before the roast is picked.
struct StoredPoint<'a> {
    now: u64,
    now_s: f64,
    payload: &'a str,
    extras: &'a Option<String>,
    device_id: &'a str,
}

/// A sample stored for a roast of the roaster, the tail saying which roast: the id, the
/// time, the elapsed seconds from the roast's start, the sample's fields, the extras, the
/// roaster, then the tail's own values.
macro_rules! store_sample {
    ($which:literal) => {
        concat!(
            "INSERT INTO session_telemetry (id, session_id, timestamp, elapsed_seconds, bean_temp, env_temp, rate_of_rise, heater, fan, setpoint, extras)
             SELECT ?, s.id, ?,
                    CASE WHEN s.start_time IS NOT NULL
                         THEN ROUND(CAST(? AS REAL) - (julianday(s.start_time) - 2440587.5) * 86400.0, 3)
                         ELSE 0.0
                    END,
                    json_extract(?, '$.beanTemp'),
                    json_extract(?, '$.envTemp'),
                    json_extract(?, '$.rateOfRise'),
                    json_extract(?, '$.heater'),
                    json_extract(?, '$.fan'),
                    json_extract(?, '$.setpoint'),
                    ?
             FROM roast_sessions s
             WHERE s.device_id = ? AND ",
            $which
        )
    };
}

impl TelemetryService {
    async fn store_point(&self, p: &StoredPoint<'_>, filter: RoastFilter<'_>) {
        let sql = match filter {
            RoastFilter::Active => store_sample!("s.status = 'active'"),
            // Completed only: while the roast still runs the statement above stores it.
            RoastFilter::Completed(_) => store_sample!("s.status = 'completed' AND s.id = ?"),
        };
        let point_id = Uuid::new_v4().to_string();
        let mut q = sqlx::query(sql)
            .bind(&point_id)
            .bind(p.now as i64)
            .bind(p.now_s)
            .bind(p.payload)
            .bind(p.payload)
            .bind(p.payload)
            .bind(p.payload)
            .bind(p.payload)
            .bind(p.payload)
            .bind(p.extras)
            .bind(p.device_id);
        if let RoastFilter::Completed(sid) = filter {
            q = q.bind(sid);
        }
        if let Err(e) = q.execute(&self.db).await {
            tracing::warn!(device_id = %p.device_id, error = %e, "Failed to insert session telemetry");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_keeps_the_control_fields_the_device_sent() {
        let p = serde_json::json!({"beanTemp": 190.0, "heater": 80, "heaterApplied": 64.0,
            "rorLimited": 0, "Kp": 10.6, "controlMode": 1, "note": "x", "stopCode": null});
        let v: serde_json::Value = serde_json::from_str(&control_extras(&p, &[]).unwrap()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"heaterApplied": 64.0, "rorLimited": 0, "Kp": 10.6, "controlMode": 1})
        );
        assert_eq!(
            control_extras(&serde_json::json!({"beanTemp": 20.0}), &[]),
            None
        );
        let drum = serde_json::json!({"beanTemp": 150.0, "drum": 55, "damper": 30.5});
        let v: serde_json::Value =
            serde_json::from_str(&control_extras(&drum, &[]).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({"drum": 55, "damper": 30.5}));
    }

    /// The whole path: a roaster declares `t3`; a roast on it keeps `t3` with each sample and
    /// not `t4`; the control fields stay; the `.alog` export carries `t3` as an extra device.
    #[tokio::test]
    async fn a_roast_keeps_its_roasters_declared_channels() {
        use crate::roasts::types::CreateSessionRequest;
        use crate::test_support::Harness;
        use serde_json::json;

        let h = Harness::new().await;
        let put = h
            .owner(
                "PUT",
                "/api/roaster/tc4/setup",
                Some(json!({ "kind": "drum_gas", "control": "off",
                    "channels": [{ "key": "t3", "label": "Exhaust", "unit": "C" }] })),
            )
            .await;
        assert!(put.status.is_success(), "{:?}", put.json());
        let db = &h.state.db;
        let s = crate::roasts::sessions::create_session(
            db,
            CreateSessionRequest {
                name: "drum".into(),
                device_id: "tc4".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crate::roasts::sessions::start_session(db, &s.id)
            .await
            .unwrap();
        let ts = &h.state.telemetry_service;
        for (bt, t3) in [(180.0, 150.0), (181.0, 151.5)] {
            ts.process_telemetry(
                "tc4",
                &json!({ "beanTemp": bt, "envTemp": 220.0, "t3": t3, "t4": 40.0,
                    "heaterApplied": 55.0 }),
                None,
            )
            .await;
        }
        let rows = crate::roasts::telemetry_store::get_session_telemetry(db, &s.id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        let x = rows[1].extras.clone().unwrap();
        assert_eq!(x["t3"], json!(151.5));
        assert_eq!(x["heaterApplied"], json!(55.0));
        assert!(x.get("t4").is_none(), "an undeclared extra stays live only");
        // Another roaster sending t3 keeps nothing of it.
        assert_eq!(
            h.state.transports.setups().channels("esp32_x").await,
            [] as [crate::roasters::channels::Channel; 0]
        );

        let (alog, _) = crate::roasts::export::export_alog(db, &s.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(alog["extradevices"], json!([25]));
        assert_eq!(alog["extraname1"], json!(["Exhaust"]));
        assert_eq!(alog["extratemp1"], json!([[150.0, 151.5]]));
    }

    /// A declared channel is kept beside the control fields; an undeclared extra is not.
    #[test]
    fn a_sample_keeps_its_declared_channels_only() {
        let p = serde_json::json!({"beanTemp": 190.0, "t3": 151.5, "t4": 40.0,
            "ambientTemp": 22.0, "heaterApplied": 64.0, "exhaust": "hot"});
        let v: serde_json::Value =
            serde_json::from_str(&control_extras(&p, &["t3", "exhaust", "gone"]).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({"heaterApplied": 64.0, "t3": 151.5}));
        let only: serde_json::Value = serde_json::from_str(
            &control_extras(&serde_json::json!({"beanTemp": 20.0, "t3": 30.0}), &["t3"]).unwrap(),
        )
        .unwrap();
        assert_eq!(only, serde_json::json!({"t3": 30.0}));
    }

    fn ramp(rate_per_min: f64, secs: usize) -> Vec<(f64, f64)> {
        (0..=secs)
            .map(|i| (i as f64, 100.0 + rate_per_min / 60.0 * i as f64))
            .collect()
    }

    #[test]
    fn ror_algorithms_agree_on_a_linear_ramp() {
        let w = ramp(12.0, 30);
        assert!((ror_moving_average(&w).unwrap() - 12.0).abs() < 1e-6);
        assert!((ror_weighted(&w).unwrap() - 12.0).abs() < 1e-6);
        assert!((ror_savitzky_golay(&w).unwrap() - 12.0).abs() < 1e-6);
        assert!(ror_moving_average(&w[..1]).is_none());
    }

    #[test]
    fn savitzky_golay_tracks_curvature_at_window_centre() {
        // y = 100 + 0.1 t² over t = 0..=20: slope at centre (t=10) is 2 °C/s = 120 °C/min
        let w: Vec<(f64, f64)> = (0..=20)
            .map(|i| (i as f64, 100.0 + 0.1 * (i * i) as f64))
            .collect();
        assert!((ror_savitzky_golay(&w).unwrap() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn tracker_windows_and_rounds() {
        let mut t = RorTracker::default();
        for i in 0..=60 {
            t.push(1000.0 + i as f64, 150.0 + 0.25 * i as f64); // 15 °C/min
        }
        let cfg = RorConfig {
            window_secs: 30.0,
            algorithm: RorAlgorithm::MovingAverage,
        };
        assert_eq!(t.rate(&cfg), Some(15.0));
        // Out-of-order sample is ignored
        t.push(900.0, 0.0);
        assert_eq!(t.rate(&cfg), Some(15.0));
    }

    #[test]
    fn test_parse_esp32_payload() {
        let payload = r#"{
            "timestamp": 1234567890,
            "beanTemp": 185.5,
            "envTemp": 200.3,
            "rateOfRise": 12.5,
            "heater": 75.0,
            "heaterApplied": 75.0,
            "fan": 70.6,
            "setpoint": 200.0,
            "controlMode": 1,
            "heaterEnable": 1,
            "uptime": 300,
            "Kp": 15.0,
            "Ki": 1.0,
            "Kd": 25.0,
            "freeHeap": 180000,
            "rssi": -45,
            "systemStatus": 0
        }"#;

        let t = parse_telemetry(payload.as_bytes()).expect("Should parse");
        assert!((t.bean_temp - 185.5).abs() < 0.01);
        assert!((t.env_temp - 200.3).abs() < 0.01);
        assert_eq!(t.heater, 75.0);
        assert_eq!(t.heater_applied, Some(75.0));
        assert_eq!(t.fan, 70.6);
        assert_eq!(t.control_mode, 1);
        assert_eq!(t.heater_enable, 1);
        assert!((t.kp.unwrap() - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_with_unknown_fields() {
        let payload = r#"{
            "beanTemp": 100.0,
            "envTemp": 90.0,
            "heater": 50,
            "fan": 50,
            "setpoint": 200.0,
            "controlMode": 0,
            "heaterEnable": 1,
            "unknownField": "should not break"
        }"#;

        let t = parse_telemetry(payload.as_bytes()).expect("Should parse despite unknown fields");
        assert!((t.bean_temp - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_invalid_json() {
        let payload = b"not json at all";
        assert!(parse_telemetry(payload).is_none());
    }
}
