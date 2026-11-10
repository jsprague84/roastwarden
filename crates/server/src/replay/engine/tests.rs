//! The engine's tests: plans, triggers, alignment, the server loop, lookahead, fan assist.

use super::plan::PlanEvent;
use super::tick::TickOutput;
use super::*;
use crate::control::fan_assist::{AssistStatus, FanAssistConfig};
use crate::profiles::types::{ProfileAction, ProfileWithPoints, TriggerKind};
use roastwarden_core::{ControlMode, DeviceCommand};

fn ev(
    id: &str,
    action: ProfileAction,
    kind: TriggerKind,
    at_seconds: Option<f64>,
    at_bt: Option<f64>,
    value: Option<f64>,
    label: Option<&str>,
) -> PlanEvent {
    PlanEvent {
        id: id.into(),
        action,
        value,
        label: label.map(Into::into),
        trigger_kind: kind,
        at_seconds,
        at_bt,
        expected_t: at_seconds,
    }
}

fn fan_at(id: &str, t: f64, pct: f64) -> PlanEvent {
    ev(
        id,
        ProfileAction::Fan,
        TriggerKind::Time,
        Some(t),
        None,
        Some(pct),
        None,
    )
}

/// BT 200 at CHARGE, dips to 100 at 60 s, then 150 at 300 s and 200 at 600 s.
fn plan(events: Vec<PlanEvent>) -> Plan {
    let bt = vec![(0.0, 200.0), (60.0, 100.0), (300.0, 150.0), (600.0, 200.0)];
    let mut landmarks = BTreeMap::new();
    landmarks.insert(AlignOn::Charge, 0.0);
    landmarks.insert(AlignOn::TurningPoint, 60.0);
    landmarks.insert(AlignOn::FirstCrackStart, 400.0);
    Plan {
        bt_ref: bt.clone(),
        bt,
        heater: vec![(0.0, 80.0), (600.0, 40.0)],
        fan: vec![(0.0, 90.0), (600.0, 60.0)],
        events,
        landmarks,
    }
}

fn cfg(mode: ReplayMode) -> RunConfig {
    RunConfig {
        mode,
        ..RunConfig::default()
    }
}

fn at(elapsed: f64, bt: Option<f64>) -> TickInput {
    TickInput {
        elapsed,
        bt,
        ror: None,
        heater: None,
        fan: None,
        auto: None,
        marks: BTreeMap::new(),
    }
}

fn with_charge(mut i: TickInput, charge: f64) -> TickInput {
    i.marks.insert(
        AlignOn::Charge,
        Mark {
            elapsed: charge,
            bt: Some(200.0),
        },
    );
    i
}

fn setpoints(out: &TickOutput) -> Vec<f64> {
    out.commands
        .iter()
        .filter_map(|(_, c)| match c {
            DeviceCommand::Setpoint { value } => Some(*value),
            _ => None,
        })
        .collect()
}

fn fans(out: &TickOutput) -> Vec<f64> {
    out.commands
        .iter()
        .filter_map(|(_, c)| match c {
            DeviceCommand::Fan { percent } => Some(*percent),
            _ => None,
        })
        .collect()
}

fn heaters(out: &TickOutput) -> Vec<f64> {
    out.commands
        .iter()
        .filter_map(|(_, c)| match c {
            DeviceCommand::Heater { percent } => Some(*percent),
            _ => None,
        })
        .collect()
}

fn status(e: &Engine, id: &str) -> EventStatus {
    e.events_view()
        .into_iter()
        .find(|v| v.event.id == id)
        .unwrap()
        .status
}

#[test]
fn interpolation_holds_ends_and_is_linear_between() {
    let curve = vec![(0.0, 100.0), (60.0, 130.0), (120.0, 160.0)];
    assert_eq!(interpolate(&curve, -5.0), Some(100.0));
    assert_eq!(interpolate(&curve, 0.0), Some(100.0));
    assert_eq!(interpolate(&curve, 30.0), Some(115.0));
    assert_eq!(interpolate(&curve, 90.0), Some(145.0));
    assert_eq!(interpolate(&curve, 500.0), Some(160.0));
    assert_eq!(interpolate(&[], 10.0), None);
}

#[test]
fn duplicate_times_do_not_divide_by_zero() {
    let curve = vec![(0.0, 100.0), (10.0, 110.0), (10.0, 120.0), (20.0, 130.0)];
    let v = interpolate(&curve, 10.0).unwrap();
    assert!((110.0..=130.0).contains(&v));
    assert_eq!(interpolate(&curve, 15.0), Some(125.0));
}

#[test]
fn profile_time_zero_is_charge_not_session_start() {
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    // CHARGE marked 90 s after the session started; lookahead 20 s.
    let out = e.tick(&with_charge(at(90.0, Some(200.0)), 90.0));
    assert_eq!(e.clock().live_t, 0.0);
    // Profile at 0 + 20 s: 200 → 100 over 60 s → 166.7.
    assert_eq!(setpoints(&out), vec![166.7]);
    // Without a CHARGE mark the session start is CHARGE.
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    e.tick(&at(90.0, Some(200.0)));
    assert_eq!(e.clock().live_t, 90.0);
}

#[test]
fn setpoint_respects_hysteresis_and_refreshes() {
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    // Rising part: 100 → 150 over 240 s = 0.208 °C/s.
    let first = e.tick(&at(100.0, Some(120.0)));
    assert_eq!(setpoints(&first).len(), 1);
    // 2 s later the target moved ~0.4 °C: below the 1 °C hysteresis.
    assert_eq!(setpoints(&e.tick(&at(102.0, Some(120.0)))), [] as [f64; 0]);
    // 6 s after the first send it moved ~1.25 °C: sent.
    assert_eq!(setpoints(&e.tick(&at(106.0, Some(120.0)))).len(), 1);
    // The profile holds after its end, but the value is refreshed every 15 s.
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    assert_eq!(setpoints(&e.tick(&at(700.0, None))), vec![200.0]);
    assert_eq!(setpoints(&e.tick(&at(710.0, None))), [] as [f64; 0]);
    assert_eq!(setpoints(&e.tick(&at(715.0, None))), vec![200.0]);
}

#[test]
fn plan_ahead_reads_the_setpoints_it_will_send() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.lookahead_seconds = 8.0;
    c.adapt_lookahead = false;
    let mut e = Engine::new(plan(vec![]), c);
    e.tick(&at(100.0, Some(120.0)));
    let p = e.plan_ahead(120);
    let sp = p.setpoints.unwrap();
    assert_eq!(sp.len(), 121);
    // 100 → 150 over 60..300 s: at 108 s (8 s ahead) and at 228 s.
    let curve = |t: f64| 100.0 + (t - 60.0) * 50.0 / 240.0;
    assert!((sp[0] - curve(108.0)).abs() < 1e-9);
    assert!((sp[120] - curve(228.0)).abs() < 1e-9);
    assert!((p.targets[0] - curve(100.0)).abs() < 1e-9);
    // Setpoint mode runs a BT loop: no open-loop heater curve, and the fan is not driven.
    assert!(p.heater.is_none());
    assert!(p.fan.is_none());
    // Feedforward: the heater curve, no setpoints.
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Feedforward));
    e.tick(&at(100.0, Some(120.0)));
    let p = e.plan_ahead(10);
    assert!(p.setpoints.is_none());
    assert_eq!(p.heater.unwrap().len(), 11);
}

#[test]
fn time_events_fire_on_profile_time_and_drive_the_fan() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(plan(vec![fan_at("f1", 120.0, 60.0)]), c);
    // Before the first event the fan holds the profile's CHARGE value (90 %).
    assert_eq!(
        fans(&e.tick(&with_charge(at(110.0, None), 0.0))),
        vec![90.0]
    );
    assert_eq!(status(&e, "f1"), EventStatus::Pending);
    let out = e.tick(&with_charge(at(120.0, None), 0.0));
    assert_eq!(fans(&out), vec![60.0]);
    assert_eq!(status(&e, "f1"), EventStatus::Fired);
    assert_eq!(out.log[0].kind, LogKind::Fired);
    // The fan is event-driven: the points' fan curve is ignored.
    assert_eq!(
        fans(&e.tick(&with_charge(at(121.0, None), 0.0))),
        [] as [f64; 0]
    );
}

#[test]
fn starting_mid_roast_applies_only_the_latest_event_per_channel() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let events = vec![
        fan_at("f1", 60.0, 90.0),
        ev(
            "tp",
            ProfileAction::Landmark,
            TriggerKind::Time,
            Some(100.0),
            None,
            None,
            Some("turning_point"),
        ),
        fan_at("f2", 120.0, 80.0),
        fan_at("f3", 240.0, 70.0),
        fan_at("f4", 400.0, 60.0),
    ];
    let mut e = Engine::new(plan(events), c);
    let out = e.tick(&at(300.0, Some(140.0)));
    assert_eq!(fans(&out), vec![70.0]);
    assert_eq!(status(&e, "f1"), EventStatus::Superseded);
    assert_eq!(status(&e, "f2"), EventStatus::Superseded);
    assert_eq!(status(&e, "f3"), EventStatus::Fired);
    assert!(matches!(status(&e, "tp"), EventStatus::Skipped { .. }));
    assert_eq!(status(&e, "f4"), EventStatus::Pending);
}

#[test]
fn bt_events_wait_for_the_turning_point() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let e1 = ev(
        "b1",
        ProfileAction::Fan,
        TriggerKind::Bt,
        None,
        Some(150.0),
        Some(50.0),
        None,
    );
    let mut e = Engine::new(plan(vec![e1]), c);
    // Right after CHARGE BT is hot and falling: above 150 but before TP.
    assert!(!fans(&e.tick(&at(1.0, Some(190.0)))).contains(&50.0));
    assert!(!fans(&e.tick(&at(30.0, Some(110.0)))).contains(&50.0));
    assert!(!fans(&e.tick(&at(60.0, Some(100.0)))).contains(&50.0));
    // Turned (2 °C above the minimum, ≥ 20 s after CHARGE) but below 150.
    assert!(!fans(&e.tick(&at(90.0, Some(103.0)))).contains(&50.0));
    assert!(e.view().past_turning_point);
    assert_eq!(fans(&e.tick(&at(300.0, Some(150.5)))), vec![50.0]);
}

#[test]
fn a_marked_turning_point_counts_and_stale_bt_never_fires() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let e1 = ev(
        "b1",
        ProfileAction::Fan,
        TriggerKind::Bt,
        None,
        Some(150.0),
        Some(50.0),
        None,
    );
    let mut e = Engine::new(plan(vec![e1]), c);
    let mut i = at(200.0, None);
    i.marks.insert(
        AlignOn::TurningPoint,
        Mark {
            elapsed: 70.0,
            bt: Some(99.0),
        },
    );
    assert!(!fans(&e.tick(&i)).contains(&50.0));
    i.elapsed = 201.0;
    i.bt = Some(151.0);
    assert_eq!(fans(&e.tick(&i)).len(), 1);
}

#[test]
fn the_run_trigger_policy_overrides_the_event_kind() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    c.trigger = TriggerPolicy::Bt;
    let both = ev(
        "x",
        ProfileAction::Fan,
        TriggerKind::Time,
        Some(100.0),
        Some(150.0),
        Some(50.0),
        None,
    );
    let mut e = Engine::new(plan(vec![both]), c);
    let mut i = at(150.0, Some(120.0));
    i.marks.insert(
        AlignOn::TurningPoint,
        Mark {
            elapsed: 60.0,
            bt: Some(100.0),
        },
    );
    // Past its time, but the run replays by BT.
    assert!(!fans(&e.tick(&i)).contains(&50.0));
    assert_eq!(e.events_view()[0].trigger, TriggerKind::Bt);
    i.elapsed = 151.0;
    i.bt = Some(150.0);
    assert_eq!(fans(&e.tick(&i)).len(), 1);
    // An event without a BT falls back to its time.
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    c.trigger = TriggerPolicy::Bt;
    let mut e = Engine::new(plan(vec![fan_at("t", 100.0, 40.0)]), c);
    e.tick(&at(99.0, None));
    assert_eq!(fans(&e.tick(&at(100.0, None))).len(), 1);
}

#[test]
fn a_paused_channel_holds_events_and_resume_returns_to_the_profile() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(
        plan(vec![fan_at("f1", 100.0, 90.0), fan_at("f2", 200.0, 50.0)]),
        c,
    );
    e.tick(&at(100.0, None));
    assert!(e.pause(Channel::Fan, "manual control", None).is_some());
    assert!(e.pause(Channel::Fan, "manual control", None).is_none());
    let out = e.tick(&at(200.0, None));
    assert_eq!(fans(&out), [] as [f64; 0]);
    assert_eq!(status(&e, "f2"), EventStatus::Held);
    assert_eq!(out.log[0].kind, LogKind::Held);
    // Nothing is refreshed while paused either.
    assert_eq!(fans(&e.tick(&at(230.0, None))), [] as [f64; 0]);
    let r = e.resume(Channel::Fan).unwrap();
    assert_eq!(r.value, Some(50.0));
    assert_eq!(fans(&e.tick(&at(231.0, None))), vec![50.0]);
}

#[test]
fn align_on_first_crack_shifts_profile_time_and_reports_the_bt_offset() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.align = AlignOn::FirstCrackStart;
    c.lookahead_seconds = 0.0;
    let mut e = Engine::new(plan(vec![]), c);
    e.tick(&at(420.0, Some(170.0)));
    // FC not marked yet: CHARGE alignment, flagged as not applied.
    let v = e.view();
    assert!(!v.align.applied);
    assert_eq!(v.clock.profile_t, 420.0);
    // FC marked 30 s later than the profile's 400 s (166.7 °C), 3 °C hotter.
    let mut i = at(431.0, Some(171.0));
    i.marks.insert(
        AlignOn::FirstCrackStart,
        Mark {
            elapsed: 430.0,
            bt: Some(169.6667),
        },
    );
    let out = e.tick(&i);
    let v = e.view();
    assert!(v.align.applied);
    assert_eq!(v.align.offset_seconds, 30.0);
    assert_eq!(v.clock.profile_t, 401.0);
    assert!((v.align.offset_c.unwrap() - 3.0).abs() < 0.01);
    assert!(out.log.iter().any(|l| l.kind == LogKind::Align));
}

#[test]
fn a_resumed_run_waits_out_the_lost_time_until_a_later_landmark_places_it() {
    // Cut at profile 300 s; resumed 40 s later, at 430 s on the roast clock (CHARGE at 90).
    let mut c = cfg(ReplayMode::Setpoint);
    c.align = AlignOn::FirstCrackStart;
    c.lookahead_seconds = 0.0;
    c.hold = Some(Hold {
        seconds: 40.0,
        since_elapsed: 430.0,
    });
    let mut e = Engine::new(plan(vec![]), c.clone());
    let charge = |i| with_charge(i, 90.0);
    assert_eq!(
        e.profile_time_at(430.0, &charge(at(0.0, None)).marks),
        300.0
    );
    e.tick(&charge(at(430.0, Some(150.0))));
    assert_eq!(e.view().clock.profile_t, 300.0);
    assert_eq!(e.view().align.offset_seconds, 40.0);
    e.tick(&charge(at(431.0, Some(150.0))));
    assert_eq!(e.view().clock.profile_t, 301.0);
    // First crack marked after the resume, 50 s after the profile's (400 s): it places
    // the profile by itself, the hold no longer counts.
    let mut i = charge(at(541.0, Some(170.0)));
    i.marks.insert(
        AlignOn::FirstCrackStart,
        Mark {
            elapsed: 540.0,
            bt: Some(170.0),
        },
    );
    e.tick(&i);
    assert_eq!(e.view().align.offset_seconds, 50.0);
    assert_eq!(e.view().clock.profile_t, 401.0);
    // A landmark marked before the cut keeps its offset, and the hold adds to it.
    let mut early = charge(at(0.0, None));
    early.marks.insert(
        AlignOn::FirstCrackStart,
        Mark {
            elapsed: 500.0,
            bt: Some(170.0),
        },
    );
    c.hold = Some(Hold {
        seconds: 40.0,
        since_elapsed: 600.0,
    });
    let e = Engine::new(plan(vec![]), c);
    // FC 10 s after the profile's: offset 10 + 40.
    assert_eq!(e.profile_time_at(600.0, &early.marks), 600.0 - 90.0 - 50.0);
}

#[test]
fn placing_the_clock_runs_nothing() {
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    e.place(330.0, &with_charge(at(0.0, None), 90.0).marks);
    let v = e.view();
    assert_eq!((v.clock.live_t, v.clock.profile_t), (240.0, 240.0));
    assert!(v.charge_marked);
    assert_eq!(v.recent, [] as [crate::replay::engine::LogEntry; 0]);
    assert!(v.channels.iter().all(|c| c.last_sent.is_none()));
}

#[test]
fn charge_marked_later_reanchors_and_rearms_what_fired_before_it() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(plan(vec![fan_at("f1", 30.0, 50.0)]), c);
    e.tick(&at(10.0, None));
    assert_eq!(fans(&e.tick(&at(40.0, None))), vec![50.0]);
    // CHARGE marked at 50 s: profile time restarts there.
    let out = e.tick(&with_charge(at(55.0, None), 50.0));
    assert_eq!(e.clock().live_t, 5.0);
    assert!(out.log.iter().any(|l| l.kind == LogKind::Anchor));
    assert_eq!(status(&e, "f1"), EventStatus::Pending);
    assert_eq!(
        fans(&e.tick(&with_charge(at(80.0, None), 50.0))),
        vec![50.0]
    );
}

#[test]
fn feedforward_sends_the_heater_curve_and_no_setpoint() {
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Feedforward));
    let out = e.tick(&at(300.0, Some(150.0)));
    assert_eq!(heaters(&out), vec![60.0]);
    assert_eq!(setpoints(&out), [] as [f64; 0]);
    // Fan replay is off by default.
    assert_eq!(fans(&out), [] as [f64; 0]);
}

fn feedforwards(out: &TickOutput) -> Vec<Option<f64>> {
    out.commands
        .iter()
        .filter_map(|(_, c)| match c {
            DeviceCommand::Feedforward { value } => Some(*value),
            _ => None,
        })
        .collect()
}

fn server(mode: ReplayMode) -> RunConfig {
    RunConfig {
        mode,
        loop_at: LoopAt::Server,
        lookahead_seconds: 0.0,
        server_pid: PidGains {
            kp: 2.0,
            ki: 0.1,
            kd: 0.0,
        },
        ..RunConfig::default()
    }
}

#[test]
fn feedforward_pid_on_the_device_streams_setpoint_and_feedforward() {
    let mut c = cfg(ReplayMode::FeedforwardPid);
    c.lookahead_seconds = 0.0;
    let mut e = Engine::new(plan(vec![]), c.clone());
    assert_eq!(c.device_mode(), ControlMode::Auto);
    let out = e.tick(&at(300.0, Some(147.0)));
    assert_eq!(setpoints(&out), vec![150.0]);
    assert_eq!(feedforwards(&out), vec![Some(60.0)]);
    assert_eq!(heaters(&out), [] as [f64; 0]);
}

#[test]
fn server_loop_sends_pid_heater_and_keeps_the_setpoint_internal() {
    let c = server(ReplayMode::Setpoint);
    assert_eq!(c.device_mode(), ControlMode::Manual);
    let mut e = Engine::new(plan(vec![]), c);
    // Bumpless: the device reports 40 % heater, so the first output is 40.
    let mut i = at(300.0, Some(147.0));
    i.heater = Some(40.0);
    let out = e.tick(&i);
    assert_eq!(setpoints(&out), [] as [f64; 0]);
    assert_eq!(heaters(&out), vec![40.0]);
    // 3 °C short for 10 s: the integral adds 0.1 × 3 × 10 = 3 %.
    for k in 1..=10 {
        e.tick(&at(300.0 + k as f64, Some(147.0 + 0.2083 * k as f64)));
    }
    let v = e.view();
    assert_eq!(v.loop_at, LoopAt::Server);
    assert!(v.pid_output.unwrap() > 41.0, "{:?}", v.pid_output);
    // Stale telemetry: the output holds where it was.
    let last = v.pid_output.unwrap();
    e.tick(&at(311.0, None));
    assert!((e.view().pid_output.unwrap() - last).abs() < 1e-9);
}

#[test]
fn server_loop_feedforward_adds_the_profile_heater() {
    let mut e = Engine::new(plan(vec![]), server(ReplayMode::FeedforwardPid));
    // On target, no measured heater: starts at the feedforward (60 %).
    let out = e.tick(&at(300.0, Some(150.0)));
    assert_eq!(heaters(&out), vec![60.0]);
    // 5 °C too hot for 1 s: ff 59.9, P -10, I -0.5 → 49 %.
    let out = e.tick(&at(301.0, Some(155.2083)));
    assert_eq!(heaters(&out), vec![49.0]);
}

#[test]
fn a_manual_setpoint_steers_a_server_loop() {
    let mut e = Engine::new(plan(vec![]), server(ReplayMode::Setpoint));
    let mut i = at(300.0, Some(150.0));
    i.heater = Some(50.0);
    e.tick(&i);
    e.pause(Channel::Setpoint, "manual setpoint command", Some(160.0));
    let out = e.tick(&at(301.0, Some(150.0)));
    // 10 °C below the operator's 160: well above the held 50 %.
    assert!(heaters(&out)[0] >= 70.0, "{:?}", heaters(&out));
    let sp = e
        .view()
        .channels
        .into_iter()
        .find(|c| c.channel == Channel::Setpoint)
        .unwrap();
    assert_eq!(sp.target, Some(160.0));
    assert!(sp.paused);
}

/// A roaster whose BT follows the setpoint `lag` seconds late, on a 20 °C/min
/// ramp; returns the lookahead after each tick.
fn run_lag(lag: usize, adapt: bool, ghost: bool, secs: usize) -> Vec<f64> {
    let mut p = plan(vec![]);
    p.bt = vec![(0.0, 30.0), (900.0, 330.0)];
    p.bt_ref = p.bt.clone();
    let mut c = cfg(ReplayMode::Setpoint);
    c.hysteresis_c = 0.0;
    c.adapt_lookahead = adapt;
    c.ghost = ghost;
    let mut e = Engine::new(p, c);
    let mut targets: Vec<f64> = Vec::new();
    let mut leads = Vec::new();
    for t in 0..secs {
        // BT now is the setpoint of `lag` ticks ago (the profile at CHARGE before that).
        let bt = if t > lag { targets[t - 1 - lag] } else { 30.0 };
        e.tick(&with_charge(at(t as f64, Some(bt)), 0.0));
        let sp = e
            .view()
            .channels
            .iter()
            .find(|c| c.channel == Channel::Setpoint)
            .and_then(|c| c.target)
            .unwrap();
        targets.push(sp);
        leads.push(e.view().lookahead_now);
    }
    leads
}

#[test]
fn the_lookahead_adapts_to_the_loops_lag() {
    // A loop with no lag: the 20 s lead only puts BT ahead, so it goes.
    let leads = run_lag(0, true, false, 600);
    assert_eq!(leads[30], 20.0, "nothing adapts in the first minute");
    assert!(leads[599] < 1.5, "lead {}", leads[599]);
    // A loop 10 s late keeps about 10 s of it.
    let leads = run_lag(10, true, false, 600);
    assert!((leads[599] - 10.0).abs() < 1.5, "lead {}", leads[599]);
    // More lag than the configured lookahead: it stays at the most.
    let leads = run_lag(30, true, false, 600);
    assert_eq!(leads[599], 20.0);
    // Off, or a ghost run: the lookahead is what was set.
    assert!(run_lag(0, false, false, 300).iter().all(|l| *l == 20.0));
    assert!(run_lag(0, true, true, 300).iter().all(|l| *l == 20.0));
}

#[test]
fn the_lookahead_holds_where_the_profile_is_flat() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.adapt_lookahead = true;
    let mut e = Engine::new(flat_plan(), c);
    for t in 0..300 {
        // BT 5 °C over a flat profile: a degree there is no number of seconds.
        e.tick(&with_charge(at(f64::from(t), Some(195.0)), 0.0));
    }
    assert_eq!(e.view().lookahead_now, 20.0);
}

#[test]
fn a_profile_fan_below_the_interlock_goes_out_at_45() {
    let mut p = plan(vec![]);
    p.fan = vec![(0.0, 30.0), (600.0, 30.0)];
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(p, c);
    let out = e.tick(&with_charge(at(0.0, Some(200.0)), 0.0));
    assert_eq!(fans(&out), vec![45.0]);
    assert!(out.log.iter().any(|l| {
        l.detail
            .as_deref()
            .is_some_and(|d| d.contains("raised to 45"))
    }));
    let out = e.tick(&with_charge(at(16.0, Some(200.0)), 0.0));
    assert_eq!(fans(&out), vec![45.0], "the refresh too");
    assert!(out.log.is_empty(), "logged once");
}

#[test]
fn server_pid_gain_changes_and_stale_readings_do_not_step_the_output() {
    let mut p = ServerPid::new(PidGains {
        kp: 3.0,
        ki: 0.03,
        kd: 0.0,
    });
    p.reset(200.0, 195.0, 60.0, 0.0);
    let before = p.compute(200.0, Some(195.0), 1.0, 0.0);
    // Stale: the output holds (it used to drop the P term, 15 %).
    assert_eq!(p.compute(200.0, None, 1.0, 0.0), before);
    // Stiffer gains: the next output moves by the I term only.
    p.set_gains(PidGains {
        kp: 6.0,
        ki: 0.03,
        kd: 0.0,
    });
    let after = p.compute(200.0, Some(195.0), 1.0, 0.0);
    assert!((after - before).abs() < 0.2, "{before} → {after}");
}

#[test]
fn engaging_a_ghost_server_loop_starts_its_pid_from_the_heater_now() {
    let mut c = server(ReplayMode::Setpoint);
    c.ghost = true;
    let mut e = Engine::new(plan(vec![]), c);
    // BT far behind for minutes while the operator roasts at 50 %.
    for t in 0..200 {
        let mut i = with_charge(at(f64::from(t), Some(80.0)), 0.0);
        i.heater = Some(50.0);
        e.tick(&i);
    }
    e.set_ghost(false);
    let mut i = with_charge(at(200.0, Some(140.0)), 0.0);
    i.heater = Some(50.0);
    let out = e.tick(&i);
    let h = heaters(&out);
    assert_eq!(h.len(), 1);
    // From 50 % (one tick of integral and rounding off), not 100 %.
    assert!((h[0] - 50.0).abs() <= 2.0, "engaged at {} %", h[0]);
}

#[test]
fn server_pid_does_not_wind_up_when_saturated() {
    let mut p = ServerPid::new(PidGains {
        kp: 1.0,
        ki: 1.0,
        kd: 0.0,
    });
    p.reset(200.0, 100.0, 100.0, 80.0);
    for _ in 0..60 {
        assert_eq!(p.compute(200.0, Some(100.0), 1.0, 80.0), 100.0);
    }
    assert!(p.integ <= 20.0 + 1e-9);
    assert!(p.compute(200.0, Some(205.0), 1.0, 80.0) < 100.0);
}

#[test]
fn server_gains_switch_mid_run_without_resetting_the_integrator() {
    let mut e = Engine::new(plan(vec![]), server(ReplayMode::Setpoint));
    e.pid.integ = 12.5;
    let g = PidGains {
        kp: 4.0,
        ki: 0.05,
        kd: 0.0,
    };
    assert!(e.set_server_gains(g));
    assert!(!e.set_server_gains(g));
    assert_eq!(e.config().server_pid, g);
    assert_eq!(e.pid.gains, g);
    assert_eq!(e.pid.integ, 12.5);
}

#[test]
fn events_on_undriven_channels_are_skipped_with_a_reason() {
    let heat = ev(
        "h",
        ProfileAction::Heater,
        TriggerKind::Time,
        Some(10.0),
        None,
        Some(70.0),
        None,
    );
    let mut e = Engine::new(
        plan(vec![heat, fan_at("f", 10.0, 50.0)]),
        cfg(ReplayMode::Setpoint),
    );
    e.tick(&at(5.0, None));
    e.tick(&at(10.0, None));
    match status(&e, "h") {
        EventStatus::Skipped { reason } => assert!(reason.contains("not driven")),
        s => panic!("{s:?}"),
    }
    match status(&e, "f") {
        EventStatus::Skipped { reason } => assert!(reason.contains("fan replay is off")),
        s => panic!("{s:?}"),
    }
}

#[test]
fn mode_events_switch_the_device_in_feedforward() {
    let m = ev(
        "m",
        ProfileAction::Mode,
        TriggerKind::Time,
        Some(10.0),
        None,
        None,
        Some("auto"),
    );
    let mut e = Engine::new(plan(vec![m]), cfg(ReplayMode::Feedforward));
    e.tick(&at(5.0, None));
    let out = e.tick(&at(10.0, None));
    assert!(out.commands.iter().any(|(ch, c)| ch.is_none()
        && *c
            == DeviceCommand::Mode {
                mode: ControlMode::Auto
            }));
}

#[test]
fn skip_delay_and_fire_now() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(
        plan(vec![
            fan_at("a", 100.0, 40.0),
            fan_at("b", 200.0, 50.0),
            fan_at("c", 300.0, 60.0),
        ]),
        c,
    );
    e.tick(&at(50.0, None));
    e.skip("a").unwrap();
    assert!(e.skip("a").is_err());
    e.delay("b", 15.0).unwrap();
    e.fire("c").unwrap();
    let out = e.tick(&at(51.0, None));
    assert_eq!(fans(&out), vec![60.0]);
    // Only the 15 s refresh of the current value goes out.
    assert_eq!(fans(&e.tick(&at(100.0, None))), vec![60.0]);
    assert_eq!(fans(&e.tick(&at(200.0, None))), vec![60.0]);
    assert!(matches!(status(&e, "b"), EventStatus::Waiting { .. }));
    assert_eq!(fans(&e.tick(&at(215.0, None))), vec![50.0]);
    assert!(e.delay("x", 5.0).is_err());
    assert!(e.delay("b", 5.0).is_err());
}

#[test]
fn a_failed_send_is_retried_next_tick() {
    let mut e = Engine::new(plan(vec![]), cfg(ReplayMode::Setpoint));
    assert_eq!(setpoints(&e.tick(&at(100.0, None))).len(), 1);
    e.send_failed(Channel::Setpoint);
    assert_eq!(setpoints(&e.tick(&at(101.0, None))).len(), 1);
}

#[test]
fn upcoming_lists_open_events_by_eta() {
    let mut c = cfg(ReplayMode::Setpoint);
    c.fan = true;
    let bt = ev(
        "bt",
        ProfileAction::Fan,
        TriggerKind::Bt,
        None,
        Some(160.0),
        Some(30.0),
        None,
    );
    let mut e = Engine::new(
        plan(vec![
            fan_at("late", 500.0, 40.0),
            fan_at("soon", 130.0, 50.0),
            bt,
        ]),
        c,
    );
    let mut i = at(100.0, Some(130.0));
    i.ror = Some(10.0);
    e.tick(&i);
    let up = e.view().upcoming;
    assert_eq!(up[0].event.id, "soon");
    assert_eq!(up[0].eta_seconds, Some(30.0));
    assert_eq!(up[1].event.id, "bt");
    assert_eq!(up[1].to_go_c, Some(30.0));
    assert_eq!(up[1].eta_seconds, Some(180.0));
    assert_eq!(up[2].event.id, "late");
}

#[test]
fn plan_from_profile_finds_landmarks_and_orders_events() {
    use crate::profiles::types::{ProfileCurves, ProfileEvent, ProfilePoint, RoastProfile};
    use chrono::Utc;
    let now = Utc::now();
    let point = |t: i32, bt: f32, h: Option<i32>| ProfilePoint {
        id: format!("p{t}"),
        profile_id: "p".into(),
        time_seconds: t,
        target_temp: bt,
        fan_speed: None,
        notes: None,
        created_at: now,
        target_env_temp: None,
        heater_pct: h,
    };
    let pe = |id: &str,
              kind: TriggerKind,
              t: Option<f64>,
              b: Option<f64>,
              action: ProfileAction,
              label: Option<&str>| ProfileEvent {
        id: id.into(),
        profile_id: "p".into(),
        trigger_kind: kind,
        at_seconds: t,
        at_bt: b,
        action,
        value: Some(50.0),
        label: label.map(Into::into),
        created_at: now,
    };
    let p = ProfileWithPoints {
        profile: RoastProfile {
            id: "p".into(),
            name: "x".into(),
            description: None,
            created_by: None,
            created_at: now,
            updated_at: now,
            is_public: false,
            target_total_time: Some(600),
            target_first_crack: Some(480),
            target_end_temp: None,
            preheat_temp: None,
            charge_temp: None,
            beans: None,
            batch_size_g: None,
            roaster: None,
            roast_level: None,
            roast_purpose: None,
            expectations: None,
            auto_drop: None,
            roaster_kind: None,
        },
        points: vec![
            point(600, 210.0, Some(40)),
            point(0, 200.0, Some(80)),
            point(90, 95.0, None),
            point(300, 150.0, Some(60)),
        ],
        events: vec![
            pe(
                "late",
                TriggerKind::Time,
                Some(500.0),
                None,
                ProfileAction::Fan,
                None,
            ),
            pe(
                "bt",
                TriggerKind::Bt,
                None,
                Some(150.0),
                ProfileAction::Fan,
                None,
            ),
            pe(
                "dry",
                TriggerKind::Time,
                Some(250.0),
                Some(150.0),
                ProfileAction::Landmark,
                Some("drying_end"),
            ),
        ],
        curves: Some(ProfileCurves {
            source_session_id: None,
            interval_seconds: 1.0,
            bt: (0..=600)
                .map(|i| {
                    Some(if i < 80 {
                        200.0 - i as f32
                    } else {
                        120.0 + (i - 80) as f32 * 0.1
                    })
                })
                .collect(),
            ..ProfileCurves::default()
        }),
    };
    let plan = Plan::from_profile(&p);
    assert_eq!(plan.bt.first(), Some(&(0.0, 200.0)));
    assert_eq!(plan.heater.len(), 3);
    assert_eq!(plan.bt_ref.len(), 601);
    assert_eq!(plan.landmarks[&AlignOn::TurningPoint], 80.0);
    assert_eq!(plan.landmarks[&AlignOn::DryingEnd], 250.0);
    assert_eq!(plan.landmarks[&AlignOn::FirstCrackStart], 480.0);
    // BT 150 on the recorded curve after TP: 80 + 30/0.1 = 380 s.
    let ids: Vec<&str> = plan.events.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec!["dry", "bt", "late"]);
    assert!((plan.events[1].expected_t.unwrap() - 380.0).abs() < 0.5);
}

// ------------------------------------------------------ fan assist --

/// The test plan with BT flat at 190 °C, so a roaster at 180 °C is 10 °C
/// behind it the whole time.
fn flat_plan() -> Plan {
    let mut p = plan(vec![]);
    p.bt = vec![(0.0, 190.0), (900.0, 190.0)];
    p.bt_ref = p.bt.clone();
    p
}

fn assist_cfg(mode: ReplayMode) -> RunConfig {
    RunConfig {
        mode,
        fan_assist: Some(FanAssistConfig {
            enabled: true,
            ..FanAssistConfig::default()
        }),
        ..RunConfig::default()
    }
}

/// Ticks from `from` to `to` s with CHARGE at 0, the device PID in auto
/// at `heater` %, BT `bt` and the fan `fan` %; returns every fan sent.
fn run_assist(e: &mut Engine, from: usize, to: usize, heater: f64, bt: f64, fan: f64) -> Vec<f64> {
    let mut sent = Vec::new();
    for t in from..to {
        let mut i = with_charge(at(t as f64, Some(bt)), 0.0);
        i.heater = Some(heater);
        i.fan = Some(fan);
        i.auto = Some(true);
        sent.extend(fans(&e.tick(&i)));
    }
    sent
}

#[test]
fn fan_assist_trims_the_operators_fan_when_the_device_heater_is_pinned() {
    // Setpoint run, fan replay off: the fan channel exists for the assist,
    // and the fan at the start (100 %) is the ceiling.
    let mut e = Engine::new(flat_plan(), assist_cfg(ReplayMode::Setpoint));
    let sent = run_assist(&mut e, 0, 1, 100.0, 180.0, 100.0);
    assert_eq!(sent, vec![100.0]);
    // 10 °C behind with the heater pinned: the 15 %/min cap.
    let sent = run_assist(&mut e, 1, 61, 100.0, 180.0, 100.0);
    let last = *sent.last().unwrap();
    assert!((84.0..=86.0).contains(&last), "fan {last}");
    assert_eq!(e.view().fan_assist.unwrap().lag, Some(10.0));
    // Never below the 70 % floor, and the floor is logged once.
    let sent = run_assist(&mut e, 125, 600, 100.0, 180.0, 100.0);
    assert_eq!(*sent.last().unwrap(), 70.0);
    let v = e.view();
    let a = v.fan_assist.unwrap();
    assert_eq!(a.status, AssistStatus::AtFloor);
    let fan_ch = v
        .channels
        .iter()
        .find(|c| c.channel == Channel::Fan)
        .unwrap();
    assert_eq!(fan_ch.source, Some("assist"));
    let notes: Vec<_> = v
        .recent
        .iter()
        .filter(|l| l.kind == LogKind::Assist)
        .filter_map(|l| l.detail.clone())
        .collect();
    assert_eq!(
        notes.iter().filter(|n| n.contains("floor")).count(),
        1,
        "{notes:?}"
    );
}

#[test]
fn fan_assist_waits_for_the_arming_bt_and_for_auto() {
    let mut e = Engine::new(plan(vec![]), assist_cfg(ReplayMode::Setpoint));
    // Pinned heater at 120 °C (drying): the fan stays put.
    let sent = run_assist(&mut e, 0, 300, 100.0, 120.0, 100.0);
    assert!(sent.iter().all(|f| *f == 100.0), "{sent:?}");
    // In manual the heater is the operator's, not the loop's: hold.
    for t in 300..400 {
        let mut i = with_charge(at(t as f64, Some(180.0)), 0.0);
        i.heater = Some(100.0);
        i.fan = Some(100.0);
        i.auto = Some(false);
        assert!(fans(&e.tick(&i)).iter().all(|f| *f == 100.0));
    }
}

#[test]
fn manual_fan_is_the_new_ceiling_unless_the_profile_drives_the_fan() {
    let mut e = Engine::new(flat_plan(), assist_cfg(ReplayMode::Setpoint));
    run_assist(&mut e, 0, 120, 100.0, 180.0, 100.0);
    assert!(e.set_fan_ceiling(90.0));
    let sent = run_assist(&mut e, 120, 130, 85.0, 180.0, 90.0);
    assert!(sent.iter().all(|f| *f == 90.0), "{sent:?}");

    let mut c = assist_cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(plan(vec![]), c);
    assert!(!e.set_fan_ceiling(90.0));
}

#[test]
fn fan_assist_trims_below_the_profile_fan_curve() {
    let mut c = assist_cfg(ReplayMode::Setpoint);
    c.fan = true;
    let mut e = Engine::new(flat_plan(), c);
    // Profile fan: 90 % at CHARGE falling to 60 % at 600 s (below the floor).
    let sent = run_assist(&mut e, 0, 2, 100.0, 180.0, 90.0);
    assert_eq!(sent, vec![90.0]);
    run_assist(&mut e, 2, 120, 100.0, 180.0, 90.0);
    let v = e.view();
    let a = v.fan_assist.unwrap();
    let profile = 90.0 - 30.0 * 119.0 / 600.0;
    assert!((a.ceiling - profile).abs() < 0.1);
    assert!(a.trim > 10.0, "{a:?}");
    // Past the point where the profile itself goes below 70 %, the
    // profile wins: no trim.
    let sent = run_assist(&mut e, 120, 600, 100.0, 180.0, 90.0);
    assert_eq!(e.view().fan_assist.unwrap().trim, 0.0);
    assert!(*sent.last().unwrap() < 70.0);
}

#[test]
fn fan_assist_works_from_the_server_pid_output() {
    let mut c = server(ReplayMode::Setpoint);
    c.fan_assist = assist_cfg(ReplayMode::Setpoint).fan_assist;
    let mut e = Engine::new(plan(vec![]), c);
    // BT stuck at 155 °C while the profile climbs to 200: the server PID
    // saturates at 100 % and the assist takes air away.
    let sent = run_assist(&mut e, 0, 600, 30.0, 155.0, 100.0);
    assert_eq!(e.view().pid_output, Some(100.0));
    assert!(*sent.last().unwrap() < 90.0, "{sent:?}");
}

#[test]
fn open_loop_runs_have_no_fan_assist_and_it_can_be_switched_off_mid_run() {
    let e = Engine::new(plan(vec![]), assist_cfg(ReplayMode::Feedforward));
    let fan_ch = e
        .view()
        .channels
        .into_iter()
        .find(|c| c.channel == Channel::Fan)
        .unwrap();
    assert!(!fan_ch.driven);

    let mut e = Engine::new(flat_plan(), assist_cfg(ReplayMode::Setpoint));
    run_assist(&mut e, 0, 200, 100.0, 180.0, 100.0);
    assert!(e.view().fan_assist.unwrap().trim > 5.0);
    let off = FanAssistConfig {
        enabled: false,
        ..FanAssistConfig::default()
    };
    assert!(e.set_fan_assist(off).is_some());
    let sent = run_assist(&mut e, 200, 202, 100.0, 180.0, 100.0);
    assert_eq!(sent, vec![100.0]);
    assert!(e.view().fan_assist.is_none());
}

#[test]
fn the_turning_point_tracker_follows_the_minimum_until_bt_rises_two_degrees_after_20_s() {
    let mut tp = TpTracker::default();
    tp.update(-1.0, Some(50.0));
    assert_eq!(tp.min, None, "nothing counts before CHARGE");
    tp.update(0.0, Some(100.0));
    assert_eq!(tp.min, Some(100.0));
    tp.update(2.0, Some(90.0));
    tp.update(3.0, Some(95.0));
    assert_eq!(tp.min, Some(90.0));
    assert!(!tp.turned);
    tp.update(10.0, Some(92.5));
    assert!(!tp.turned, "the rise counts only from 20 s");
    tp.update(25.0, Some(91.0));
    assert!(!tp.turned, "a degree is not a turn");
    tp.update(26.0, Some(92.0));
    assert!(tp.turned);
    tp.update(27.0, Some(10.0));
    assert_eq!(tp.min, Some(90.0), "turned: nothing more is tracked");
}

#[test]
fn placing_the_clock_counts_the_align_offset() {
    let plan = Plan {
        bt: vec![(0.0, 100.0), (600.0, 210.0)],
        bt_ref: vec![(0.0, 100.0), (600.0, 210.0)],
        landmarks: BTreeMap::from([(AlignOn::Charge, 0.0), (AlignOn::FirstCrackStart, 480.0)]),
        ..Default::default()
    };
    let cfg = RunConfig {
        align: AlignOn::FirstCrackStart,
        ..RunConfig::default()
    };
    let mut e = Engine::new(plan, cfg);
    // CHARGE at 10 s, first crack at 500 s: 490 s after CHARGE against the profile's 480, so
    // the profile runs 10 s behind the live clock.
    let marks = BTreeMap::from([
        (
            AlignOn::Charge,
            Mark {
                elapsed: 10.0,
                bt: Some(200.0),
            },
        ),
        (
            AlignOn::FirstCrackStart,
            Mark {
                elapsed: 500.0,
                bt: Some(195.0),
            },
        ),
    ]);
    e.place(600.0, &marks);
    assert_eq!(e.clock().live_t, 590.0);
    assert_eq!(e.clock().profile_t, 580.0);
    assert_eq!(e.profile_time_at(600.0, &marks), 580.0);
}

#[test]
fn a_recorded_curve_is_the_bt_reference_at_its_own_interval() {
    use crate::profiles::types::{ProfileCurves, ProfilePoint, RoastProfile};
    use chrono::Utc;
    let now = Utc::now();
    let point = |t: i32, bt: f32| ProfilePoint {
        id: format!("p{t}"),
        profile_id: "p".into(),
        time_seconds: t,
        target_temp: bt,
        fan_speed: None,
        notes: None,
        created_at: now,
        target_env_temp: None,
        heater_pct: None,
    };
    let with_interval = |interval_seconds: f64| ProfileWithPoints {
        profile: RoastProfile {
            id: "p".into(),
            name: "x".into(),
            description: None,
            created_by: None,
            created_at: now,
            updated_at: now,
            is_public: false,
            target_total_time: None,
            target_first_crack: None,
            target_end_temp: None,
            preheat_temp: None,
            charge_temp: None,
            beans: None,
            batch_size_g: None,
            roaster: None,
            roast_level: None,
            roast_purpose: None,
            expectations: None,
            auto_drop: None,
            roaster_kind: None,
        },
        points: vec![point(0, 100.0), point(600, 210.0)],
        events: vec![],
        curves: Some(ProfileCurves {
            source_session_id: None,
            interval_seconds,
            bt: vec![Some(100.0), Some(110.0), Some(120.0)],
            et: vec![],
            ror: vec![],
            heater: vec![],
            fan: vec![],
        }),
    };
    assert_eq!(
        Plan::from_profile(&with_interval(2.0)).bt_ref,
        vec![(0.0, 100.0), (2.0, 110.0), (4.0, 120.0)]
    );
    assert_eq!(
        Plan::from_profile(&with_interval(0.0)).bt_ref,
        vec![(0.0, 100.0), (1.0, 110.0), (2.0, 120.0)],
        "no interval recorded: one second"
    );
}
