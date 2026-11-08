//! A profile compiled for replay: the BT, heater and fan curves, the events in the order they
//! are expected, the landmarks the profile knows; and interpolation over a curve.

use std::collections::BTreeMap;

use serde::Serialize;

use super::config::{AlignOn, Channel};
use crate::profiles::types::{ProfileAction, ProfileWithPoints, TriggerKind};
use crate::roasts::types::RoastEventType;

/// One profile event, ready to replay.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PlanEvent {
    pub id: String,
    pub action: ProfileAction,
    pub value: Option<f64>,
    pub label: Option<String>,
    pub trigger_kind: TriggerKind,
    pub at_seconds: Option<f64>,
    pub at_bt: Option<f64>,
    /// Profile time the event is expected at, for ordering and display.
    pub expected_t: Option<f64>,
}

impl PlanEvent {
    /// The output channel the event acts on; cues (landmark, note) have none.
    /// A mode switch belongs to the heater: it decides who drives it.
    pub fn channel(&self) -> Option<Channel> {
        match self.action {
            ProfileAction::Setpoint => Some(Channel::Setpoint),
            ProfileAction::Heater | ProfileAction::Mode => Some(Channel::Heater),
            ProfileAction::Fan => Some(Channel::Fan),
            ProfileAction::Landmark | ProfileAction::Note => None,
        }
    }
}

/// A profile compiled for replay. Times are seconds from CHARGE.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// BT target curve from the points (setpoint source, trim reference).
    pub bt: Vec<(f64, f64)>,
    /// Best BT reference: the recorded curve when the profile has one.
    pub bt_ref: Vec<(f64, f64)>,
    pub heater: Vec<(f64, f64)>,
    pub fan: Vec<(f64, f64)>,
    pub events: Vec<PlanEvent>,
    /// Profile time of each landmark the profile knows.
    pub landmarks: BTreeMap<AlignOn, f64>,
}

impl Plan {
    pub fn from_profile(p: &ProfileWithPoints) -> Plan {
        let mut pts: Vec<_> = p.points.iter().collect();
        pts.sort_by_key(|x| x.time_seconds);
        let bt: Vec<(f64, f64)> = pts
            .iter()
            .map(|x| (f64::from(x.time_seconds), f64::from(x.target_temp)))
            .collect();
        let heater: Vec<(f64, f64)> = pts
            .iter()
            .filter_map(|x| {
                x.heater_pct
                    .map(|h| (f64::from(x.time_seconds), f64::from(h)))
            })
            .collect();
        let fan: Vec<(f64, f64)> = pts
            .iter()
            .filter_map(|x| {
                x.fan_speed
                    .map(|f| (f64::from(x.time_seconds), f64::from(f)))
            })
            .collect();
        let recorded: Vec<(f64, f64)> = p
            .curves
            .as_ref()
            .map(|c| {
                let step = if c.interval_seconds > 0.0 {
                    c.interval_seconds
                } else {
                    1.0
                };
                c.bt.iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.map(|v| (i as f64 * step, f64::from(v))))
                    .collect()
            })
            .unwrap_or_default();
        let bt_ref = if recorded.len() >= 2 {
            recorded
        } else {
            bt.clone()
        };

        let mut landmarks = BTreeMap::new();
        landmarks.insert(AlignOn::Charge, 0.0);
        for e in &p.events {
            if e.action != ProfileAction::Landmark {
                continue;
            }
            let (Some(label), Some(t)) = (e.label.as_deref(), e.at_seconds) else {
                continue;
            };
            if let Some(on) = label
                .parse::<RoastEventType>()
                .ok()
                .as_ref()
                .and_then(AlignOn::from_event_type)
            {
                landmarks.entry(on).or_insert(t);
            }
        }
        if let Some(tp) = curve_minimum(&bt_ref) {
            landmarks.entry(AlignOn::TurningPoint).or_insert(tp);
        }
        if let Some(fc) = p.profile.target_first_crack {
            landmarks
                .entry(AlignOn::FirstCrackStart)
                .or_insert(f64::from(fc));
        }

        let tp_t = landmarks
            .get(&AlignOn::TurningPoint)
            .copied()
            .unwrap_or(0.0);
        let mut events: Vec<PlanEvent> = p
            .events
            .iter()
            .map(|e| {
                let expected_t = match (e.trigger_kind, e.at_seconds, e.at_bt) {
                    (TriggerKind::Time, Some(t), _) => Some(t),
                    (TriggerKind::Bt, _, Some(b)) => {
                        crossing_time(&bt_ref, tp_t, b).or(e.at_seconds)
                    }
                    (_, t, b) => t.or_else(|| b.and_then(|b| crossing_time(&bt_ref, tp_t, b))),
                };
                PlanEvent {
                    id: e.id.clone(),
                    action: e.action,
                    value: e.value,
                    label: e.label.clone(),
                    trigger_kind: e.trigger_kind,
                    at_seconds: e.at_seconds,
                    at_bt: e.at_bt,
                    expected_t,
                }
            })
            .collect();
        events.sort_by(|a, b| {
            a.expected_t
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b.expected_t.unwrap_or(f64::INFINITY))
        });

        Plan {
            bt,
            bt_ref,
            heater,
            fan,
            events,
            landmarks,
        }
    }

    pub(super) fn has_events_for(&self, ch: Channel) -> bool {
        self.events
            .iter()
            .any(|e| e.channel() == Some(ch) && e.action != ProfileAction::Mode)
    }

    /// Whether the profile has anything to drive this channel with.
    pub fn has_data_for(&self, ch: Channel) -> bool {
        self.has_events_for(ch)
            || match ch {
                Channel::Setpoint => !self.bt.is_empty(),
                Channel::Heater => !self.heater.is_empty(),
                Channel::Fan => !self.fan.is_empty(),
            }
    }
}

/// Linear interpolation over a time-sorted curve; holds the first value
/// before the curve starts and the last value after it ends.
pub fn interpolate(curve: &[(f64, f64)], t: f64) -> Option<f64> {
    let (first, last) = (curve.first()?, curve.last()?);
    if t <= first.0 {
        return Some(first.1);
    }
    if t >= last.0 {
        return Some(last.1);
    }
    let i = curve.partition_point(|p| p.0 <= t);
    let (t0, v0) = curve[i - 1];
    let (t1, v1) = curve[i];
    if t1 <= t0 {
        return Some(v1);
    }
    Some(v0 + (t - t0) / (t1 - t0) * (v1 - v0))
}

/// Time of the curve minimum in its first half, if it is not the first point
/// (a designed profile that only rises has no turning point).
fn curve_minimum(curve: &[(f64, f64)]) -> Option<f64> {
    let last_t = curve.last()?.0;
    let half: Vec<&(f64, f64)> = curve.iter().filter(|p| p.0 <= last_t / 2.0).collect();
    let (i, p) = half
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))?;
    (i > 0).then_some(p.0)
}

/// First time after `from_t` at which the curve rises through `bt`.
fn crossing_time(curve: &[(f64, f64)], from_t: f64, bt: f64) -> Option<f64> {
    curve.windows(2).find_map(|w| {
        let ((t0, a), (t1, b)) = (w[0], w[1]);
        if t1 < from_t || !(a <= bt && b >= bt) {
            return None;
        }
        let f = if b == a { 1.0 } else { (bt - a) / (b - a) };
        Some(t0 + f * (t1 - t0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_is_crossed_where_it_rises_through_the_temperature() {
        let c = [(0.0, 100.0), (10.0, 90.0), (20.0, 110.0), (30.0, 130.0)];
        assert_eq!(crossing_time(&c, 0.0, 95.0), Some(12.5));
        assert_eq!(
            crossing_time(&c, 0.0, 100.0),
            Some(15.0),
            "a falling segment does not count"
        );
        assert_eq!(
            crossing_time(&c, 20.0, 100.0),
            Some(15.0),
            "a segment that ends at from_t still counts"
        );
        assert_eq!(crossing_time(&c, 21.0, 100.0), None);
        assert_eq!(
            crossing_time(&[(0.0, 100.0), (10.0, 100.0)], 0.0, 100.0),
            Some(10.0),
            "a flat segment at the temperature ends there"
        );
    }

    #[test]
    fn the_curve_minimum_is_in_the_first_half_and_never_the_first_point() {
        let dip = [(0.0, 200.0), (30.0, 90.0), (60.0, 120.0), (120.0, 50.0)];
        assert_eq!(curve_minimum(&dip), Some(30.0));
        let rising = [(0.0, 80.0), (30.0, 120.0), (60.0, 160.0)];
        assert_eq!(curve_minimum(&rising), None);
        assert_eq!(curve_minimum(&[]), None);
    }

    #[test]
    fn a_channel_has_data_from_its_events_or_its_curve_and_a_mode_switch_is_neither() {
        let ev = |action: ProfileAction| PlanEvent {
            id: "e".into(),
            action,
            value: Some(50.0),
            label: None,
            trigger_kind: TriggerKind::Time,
            at_seconds: Some(60.0),
            at_bt: None,
            expected_t: Some(60.0),
        };
        let plan = Plan {
            bt: vec![(0.0, 100.0)],
            events: vec![ev(ProfileAction::Mode), ev(ProfileAction::Fan)],
            ..Default::default()
        };
        assert!(plan.has_data_for(Channel::Setpoint));
        assert!(!plan.has_events_for(Channel::Setpoint));
        assert!(plan.has_events_for(Channel::Fan));
        assert!(plan.has_data_for(Channel::Fan));
        assert!(!plan.has_events_for(Channel::Heater));
        assert!(!plan.has_data_for(Channel::Heater));
    }
}
