//! In-memory recent-history ring per device.
//!
//! The dashboard needs the last couple of hours instantly on page load and a
//! gap-free catch-up after a reconnect. SQLite can answer both, but not in a
//! few milliseconds with a 300 MB telemetry table, so the server keeps the
//! recent samples in memory. Sequence numbers are per device, start at 1 and
//! never repeat for the lifetime of the process; a client resumes with
//! `since_seq` and gets exactly what it missed, or the whole ring if it has
//! been away longer than the ring covers.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use roastwarden_core::Sample;

/// Two hours at 1 Hz.
pub const DEFAULT_CAPACITY: usize = 7200;

#[derive(Debug)]
pub struct SampleRing {
    buf: VecDeque<Sample>,
    cap: usize,
    next_seq: u64,
}

impl SampleRing {
    pub fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap.min(8192)),
            cap: cap.max(1),
            next_seq: 1,
        }
    }

    /// Assign the next sequence number to `sample`, store it, and return the
    /// stored copy.
    pub fn push(&mut self, mut sample: Sample) -> Sample {
        sample.seq = self.next_seq;
        self.next_seq += 1;
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(sample.clone());
        sample
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn latest_seq(&self) -> u64 {
        self.next_seq - 1
    }

    pub fn oldest_seq(&self) -> Option<u64> {
        self.buf.front().map(|s| s.seq)
    }

    pub fn latest(&self) -> Option<&Sample> {
        self.buf.back()
    }

    /// Samples with `seq > since_seq`, oldest first, at most `limit`
    /// (the most recent ones if truncated).
    pub fn since_seq(&self, since_seq: u64, limit: usize) -> Vec<Sample> {
        // seq is contiguous in the ring, so index arithmetic beats a scan.
        let Some(oldest) = self.oldest_seq() else {
            return Vec::new();
        };
        let start_seq = since_seq.saturating_add(1).max(oldest);
        if start_seq > self.latest_seq() {
            return Vec::new();
        }
        let mut start = (start_seq - oldest) as usize;
        let n = self.buf.len() - start;
        if n > limit {
            start += n - limit;
        }
        self.buf.range(start..).cloned().collect()
    }

    /// Samples with `ts_ms >= since_ms`, oldest first, at most `limit`.
    pub fn since_ms(&self, since_ms: u64, limit: usize) -> Vec<Sample> {
        let start = self.buf.partition_point(|s| s.ts_ms < since_ms);
        let n = self.buf.len() - start;
        let start = if n > limit { start + n - limit } else { start };
        self.buf.range(start..).cloned().collect()
    }

    /// True when `since_seq` is older than the ring covers, i.e. a client
    /// resuming from it will miss samples no matter what.
    pub fn has_gap_before(&self, since_seq: u64) -> bool {
        match self.oldest_seq() {
            Some(oldest) => since_seq + 1 < oldest,
            None => since_seq < self.latest_seq(),
        }
    }
}

/// Summary of one device's ring, sent to v2 clients in the hello frame.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct RingInfo {
    pub device_id: String,
    pub oldest_seq: Option<u64>,
    pub latest_seq: u64,
    pub count: usize,
    pub latest_ts_ms: Option<u64>,
}

/// All rings, keyed by device id.
#[derive(Debug, Default)]
pub struct Rings {
    inner: Mutex<HashMap<String, SampleRing>>,
    cap: usize,
}

impl Rings {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            cap,
        }
    }

    pub fn push(&self, device_id: &str, sample: Sample) -> Sample {
        let mut map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.entry(device_id.to_string())
            .or_insert_with(|| SampleRing::new(self.cap))
            .push(sample)
    }

    pub fn since_seq(&self, device_id: &str, since_seq: u64, limit: usize) -> Vec<Sample> {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.get(device_id)
            .map(|r| r.since_seq(since_seq, limit))
            .unwrap_or_default()
    }

    pub fn since_ms(&self, device_id: &str, since_ms: u64, limit: usize) -> Vec<Sample> {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.get(device_id)
            .map(|r| r.since_ms(since_ms, limit))
            .unwrap_or_default()
    }

    pub fn has_gap_before(&self, device_id: &str, since_seq: u64) -> bool {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.get(device_id)
            .is_some_and(|r| r.has_gap_before(since_seq))
    }

    /// The newest sample for a device.
    pub fn latest(&self, device_id: &str) -> Option<Sample> {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.get(device_id).and_then(|r| r.latest().cloned())
    }

    pub fn info(&self, device_id: &str) -> Option<RingInfo> {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        map.get(device_id).map(|r| RingInfo {
            device_id: device_id.to_string(),
            oldest_seq: r.oldest_seq(),
            latest_seq: r.latest_seq(),
            count: r.len(),
            latest_ts_ms: r.latest().map(|s| s.ts_ms),
        })
    }

    pub fn summary(&self) -> Vec<RingInfo> {
        let map = self
            .inner
            .lock()
            .expect("the ring buffers' lock is poisoned");
        let mut v: Vec<RingInfo> = map
            .iter()
            .map(|(id, r)| RingInfo {
                device_id: id.clone(),
                oldest_seq: r.oldest_seq(),
                latest_seq: r.latest_seq(),
                count: r.len(),
                latest_ts_ms: r.latest().map(|s| s.ts_ms),
            })
            .collect();
        v.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(ts: u64) -> Sample {
        Sample {
            ts_ms: ts,
            bt: Some(ts as f64),
            ..Default::default()
        }
    }

    #[test]
    fn assigns_contiguous_seq_and_evicts_oldest() {
        let mut r = SampleRing::new(3);
        for ts in 1..=5 {
            let stored = r.push(s(ts * 1000));
            assert_eq!(stored.seq, ts);
        }
        assert_eq!(r.len(), 3);
        assert_eq!(r.oldest_seq(), Some(3));
        assert_eq!(r.latest_seq(), 5);
    }

    #[test]
    fn since_seq_returns_exactly_the_missed_samples() {
        let mut r = SampleRing::new(10);
        for ts in 1..=6 {
            r.push(s(ts));
        }
        let got = r.since_seq(4, 100);
        assert_eq!(got.iter().map(|x| x.seq).collect::<Vec<_>>(), vec![5, 6]);
        assert_eq!(r.since_seq(6, 100), [] as [roastwarden_core::Sample; 0]);
        assert_eq!(r.since_seq(99, 100), [] as [roastwarden_core::Sample; 0]);
        // From before the ring: everything
        assert_eq!(r.since_seq(0, 100).len(), 6);
        // Limit keeps the most recent
        let got = r.since_seq(0, 2);
        assert_eq!(got.iter().map(|x| x.seq).collect::<Vec<_>>(), vec![5, 6]);
    }

    #[test]
    fn gap_detection_after_eviction() {
        let mut r = SampleRing::new(3);
        for ts in 1..=5 {
            r.push(s(ts));
        }
        // oldest is seq 3: resuming from 2 is fine (gets 3,4,5), from 1 has a gap
        assert!(!r.has_gap_before(2));
        assert!(r.has_gap_before(1));
        assert_eq!(r.since_seq(1, 100).len(), 3);
    }

    #[test]
    fn since_ms_uses_timestamps() {
        let mut r = SampleRing::new(10);
        for ts in [100, 200, 300, 400] {
            r.push(s(ts));
        }
        assert_eq!(r.since_ms(250, 100).len(), 2);
        assert_eq!(r.since_ms(0, 3).len(), 3);
        assert_eq!(r.since_ms(500, 100).len(), 0);
    }

    #[test]
    fn rings_are_per_device() {
        let rings = Rings::new(5);
        rings.push("a", s(1));
        rings.push("a", s(2));
        rings.push("b", s(1));
        assert_eq!(rings.since_seq("a", 0, 10).len(), 2);
        assert_eq!(rings.since_seq("b", 0, 10).len(), 1);
        assert_eq!(
            rings.since_seq("c", 0, 10),
            [] as [roastwarden_core::Sample; 0]
        );
        let sum = rings.summary();
        assert_eq!(sum.len(), 2);
        assert_eq!(sum[0].device_id, "a");
        assert_eq!(sum[0].latest_seq, 2);
    }
}
