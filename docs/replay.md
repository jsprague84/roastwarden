# Replay engine

Profile drive lets the server roast from a profile. Every second it sends the roaster the
profile's setpoint, heater and fan, and fires the profile's events (a fan step at 4:00) by time
or by bean temperature. It runs in the server, so the roast carries on with every browser
closed. Or leave it off and roast by hand against the profile on the scope.

It needs a roaster with full control and a heater output ([roaster setup](roaster-setup.md)).
With control off or manual, the tab offers only a ghost run: the overlay, nothing sent.

## Driving a roast

1. **Pick the profile** in the Roast page's Next step card (Profile). It is drawn on the scope
   from CHARGE.
2. **Start the roast**, open the **Profile drive** tab, choose a mode and press **Drive the
   roaster**. The roast sequence can start it for you at CHARGE
   ([roast sequence](roast-sequence.md)).
3. **Or try it first**: tick **Ghost first (dry run)**. A ghost run shows what it would send
   and sends nothing; **Engage** switches it to live.
4. **While it runs** the tab shows the profile time, each channel's value and the next events,
   each with **+15s**, **Now** and **Skip**.

The drive never switches heater power on. If it is off, the tab says so and offers **Turn it
on**.

## Modes

| Mode | What it does |
|---|---|
| Setpoint | The PID follows the profile's BT curve (closed loop). The default. |
| Feedforward | Plays the profile's heater % open loop; BT is not corrected. It repeats a recorded roast exactly: good for comparing beans, not for another batch size. |
| FF + PID | The profile's heater % does most of the work and the PID trims the error. |

Feedforward and FF + PID need a profile with heater data, such as one saved from a roast.

**Replay fan** plays the profile's fan as well (on when the profile has fan data; off, the fan
stays where you set it). With [fan assist](fan-assist.md) on, the profile's fan is the ceiling
and the assist trims below it. A drive never sends a fan below the roaster's fan floor: a
lower profile fan goes out at the floor.

## Timing and loop

Under **Timing and loop** on the tab:

| Setting | Choices | What it does |
|---|---|---|
| Events fire | Each event's own (default), By time, By BT | By BT fires an event when BT reaches its temperature after the turning point, so a slow or fast batch still gets its changes at the same bean temperature. |
| Align on | CHARGE (default), TP, DRY, FC | Once that landmark is marked live, the profile shifts so it lines up with this roast. The temperature difference there is shown, not applied. Until then it runs from CHARGE. |
| PID runs | Auto, On the roaster, On the server | Where the BT loop runs ([control](control.md)). Auto picks the roaster when it can. |

Events fire and Align on can be changed while the drive runs.

In Setpoint mode the setpoint is the profile's BT a little ahead, so the PID starts turning
before the curve does: Settings → Roasting → **Profile lookahead**, default 20 s. With **Adapt
to the roaster** on (the default) the lead shrinks, from a minute after CHARGE, by however far
BT runs ahead of the profile, so BT does not end up ahead of it.

## Taking over

- **Touch a control** (setpoint, heater, fan or mode) and that channel pauses ("paused:
  yours"); the profile keeps running underneath. **Resume** or **Resume all** sends the
  profile's current value and puts the roaster back in the drive's mode.
- **Stop** ends the drive and leaves the outputs where they are. Emergency stop, DROP and
  completing the roast stop it too. Pausing the recording does not.
- **A stopped drive**: if the roaster cuts its heater or the server restarts mid-roast, the
  Roast page offers **Heater on and resume drive** (it picks up where it stopped) or **Roast by
  hand**.
- **Lost readings**: with no fresh sample for 8 s the drive stops renewing the roaster's
  control lease, so a roaster that keeps a lease falls into its safe state by itself. At 20 s
  the drive stops.

## Auto DROP

Once a profile is dialed in, Profile drive can mark DROP for you. It is off until you choose
it in the profile's editor (Profiles → the profile → Edit → **Auto DROP**):

| Auto DROP | Drops when | The profile needs |
|---|---|---|
| At the drop time (the curve's end) | the profile time reaches the drop time | a drop time |
| At the end temperature | BT reaches the drop temperature, counted only once first crack is marked or the profile is past its first crack time, so an early spike cannot drop the roast | a drop temperature |
| Development after first crack | the profile's development time (drop minus first crack) has passed since you marked first crack | first crack and drop times |

Before it drops, the Roast page counts down with **Cancel** and your phones and notification
channels get a notice. The countdown (Settings → Roasting → **Auto DROP countdown**, 3–60 s,
default 10 s) ends on the drop itself. Cancel stops it for this roast only. Then DROP runs as
when you press it: heater off, the fan to cool.

Auto DROP stays off when the roaster's DROP step is off ([roast sequence](roast-sequence.md))
and when no drive runs. A ghost run shows when it would drop and never counts down.

## From the command line

`roast live` shows a running drive: the profile time, auto DROP's state and, for a stopped
drive, why it stopped and the call that resumes it. `roast profile edit <profile> --auto-drop
time|bt|development|off` sets a profile's auto DROP ([the roast CLI](cli.md)).

<!-- site: end -->

## Design notes

For developers: how Profile drive works and why. The site publishes only the part above.

The code: `crates/server/src/replay/` has `engine/`, the pure state machine (`mod.rs` the engine
and its construction, `config.rs` the run's vocabulary and configuration, `pid.rs` the server PID,
`plan.rs` the profile compiled for replay, `tick.rs` the second's step, `events.rs` the profile's
events, `fan.rs` fan assist inside the drive, `controls.rs` the operator's mid-run changes,
`view.rs` the state snapshot and the log, `tests.rs`), `types.rs` the requests, the state and the
errors, `service.rs` the drives (`service/run.rs` one run's 1 Hz task) and `auto_drop.rs`.

`ReplayService` drives a roast from a profile v2 (points, events, curves). Control stays in the
server; the dashboard starts, steers and watches a run. Everything on the wire and in storage is
°C.

## Time base: profile t = 0 is CHARGE

Profile v2 is CHARGE-relative (the extractor rebases on CHARGE), so profile `t = 0` is CHARGE,
not the session start: counted from the session start, a roast whose CHARGE is marked 90 s
after the session started would run 90 s early.

- **Live time** `live_t` = seconds since CHARGE: `elapsed_since_start −
  charge.elapsed_seconds`, or since the session start when no CHARGE is
  marked (the existing convention). Pre-charge time is negative; curves hold
  their first value and no event fires before `t = 0`.
- **Profile time** `profile_t = live_t − align_offset`.
- **Align** (Artisan "align on"): the run anchors on CHARGE (default), TP,
  DRY or FC. Once that landmark is marked live and the profile knows its own
  time for it, `align_offset = live_landmark_t − profile_landmark_t`. Until
  then the offset is 0 (CHARGE). The temperature difference at the landmark
  (`live BT − profile BT`) is reported and drawn but not applied.
- Profile landmark times come from the profile's `landmark` events, then
  `target_first_crack` for FC and the curve minimum for TP.
- The scope uses the same rule: profile curves and ticks are drawn at
  `scope_t = profile_t + align_offset`, where scope `t = 0` is CHARGE, and the
  offset comes from the running replay (0 when none runs).

## Run configuration

| Field | Values | Default |
|---|---|---|
| `mode` | `setpoint` (closed loop on the BT curve, the old follow), `feedforward` (open loop: heater % from the profile), `feedforward_pid` (closed loop plus the profile heater as feedforward) | `setpoint` |
| `loop` | where the BT loop runs: `auto`, `device`, `server` (see `docs/control.md`) | `auto` |
| `trigger` | `profile` (each event's own `trigger_kind`), `time`, `bt` | `profile` |
| `align` | `charge`, `turning_point`, `drying_end`, `first_crack_start` | `charge` |
| `fan` | replay the fan channel | true when the profile has fan data |
| `ghost` | compute and show, send nothing | false |
| `lookahead_seconds` | setpoint lead, 0–300 (the most it uses when it adapts) | setting `profile_lookahead_seconds` (20) |
| `adapt_lookahead` | the lead follows the loop's lag ("Lookahead" below) | setting `profile_lookahead_adapt` (on) |
| `auto_drop` | the drive marks DROP itself ("Auto DROP" below); a start with `false` begins it cancelled | the profile's `auto_drop` |

Where the loop runs and how is in `docs/control.md`: the device PID (with
feedforward on firmware 3.1+) or a server PID (gains in the `server_pid`
setting) for roasters that only take heater %.

`ghost` and `align`, `trigger` and `lookahead_seconds` can be changed while a
run is active. Turning ghost off "engages" the run: the device mode is set
and the current channel values are sent.

## Channels and what drives them

Three output channels: **setpoint**, **heater**, **fan**. Each is driven by
either the profile's events for that channel (step changes, the Artisan way)
or, when the profile has no events for it, its curve from the points
(`target_temp` with lookahead for setpoint, `heater_pct`, `fan_speed`). A
profile saved from a roast with a stepped heater therefore replays steps at
their exact time or BT; a PID-shaped heater replays from the points. Until
its first event fires, an event-driven channel holds the curve's CHARGE value
(the profile's starting fan, for example).

| Mode | setpoint | heater | fan (if enabled) |
|---|---|---|---|
| `setpoint` | driven | — (device PID) | driven |
| `feedforward` | — | driven | driven |
| `feedforward_pid` | — | driven + trim | driven |

Sends are deduplicated: setpoint on a change ≥ `follow_hysteresis_c` (0.2 °C: at 1 °C,
late in a roast the device PID saw a 1 °C step every 7–12 s, each a kick of kp·1 °C, and
BT lagged half a step; migration 035 moved a stored 1.0 to 0.2),
heater and fan on a whole-percent change, plus a refresh every 15 s so a
lost MQTT command does not stick. `mode` events act on the heater channel.
Heater and fan are % on the wire too (firmware 3.3).
In closed-loop modes the fan target can be trimmed by fan assist
([`fan-assist.md`](fan-assist.md)): the profile fan is the ceiling. A drive never sends a
fan below the roaster's fan floor (45 % on the fluid bed; none on a drum,
[`roaster-setup.md`](roaster-setup.md)): a profile fan under it goes out at the floor (logged
once), since below the firmware's 40 % interlock the heater is gated and BT would stall with
the drive "in control". A roaster with no fan output gets no fan channel; one with no heater
cannot be driven (409 `OUTPUT_NOT_DECLARED`, ghost still works).

## Lookahead

In setpoint mode the setpoint is the profile `lookahead_seconds` ahead, so the PID starts
turning before the curve does. A fixed lead is right only while the loop lags that much.
On the real roaster (five recorded roasts) the loop lagged about 17 s while drying,
10 s from dry end to first crack and none after it, so a 20 s lead put BT 10 s, then the
whole 20 s, ahead of the profile: first crack at 7:29 against the profile's 7:50, DTR 25.8 %
against its 22 %. The virtual roaster shows the same (16–17 s ahead from dry end on).

With `adapt_lookahead` (default on) the lead starts at `lookahead_seconds` and, from a minute
after CHARGE, moves each tick by how many seconds BT is ahead of the profile itself
(BT minus the profile now, over the profile's slope), with a 45 s time constant, between 0
and `lookahead_seconds`. Where the profile climbs slower than 3 °C/min (a flat end) a degree
is many seconds, so the lead holds there. It does not adapt in a ghost run, with the setpoint
channel paused or event-driven, or before CHARGE. The state's `lookahead_now` is the lead in
use (the Profile drive tab shows it as "lead"), and each 5 s step is logged. Changing
`lookahead_seconds` mid-run starts the lead again from the new value.

`landmark` and `note` events are **cues**: they appear in the timeline and
the log but never mark a session landmark (marking FC is the operator's or
the auto-detector's job). Landmark events also supply align times.

## Events

- Effective trigger per event: the run's `trigger`, falling back to the
  event's own kind when the event lacks that reference.
- **By time**: fires when `profile_t ≥ at_seconds`.
- **By BT**: fires once BT has turned (TP marked, or BT 2 °C above its
  post-charge minimum after 20 s, as the auto-TP rule) and BT ≥ `at_bt`.
  Needs fresh telemetry.
- **Catch-up coalescing**: when several events on one channel trigger in the
  same tick (starting mid-roast, or a re-align jump), only the last one is
  applied; the rest are logged as superseded. Cues that are already past when
  a run starts are skipped.
- Operator actions per pending event: **skip**, **delay** (adds N seconds
  after the trigger is met), **fire now**.

## Manual override

A manual control call (`/api/roaster/{id}/control/setpoint|heater|fan|mode`) pauses the matching channel of the device's live (non-ghost)
run; a mode change pauses setpoint and heater. While paused, events on that
channel still advance the profile's value but nothing is sent ("held").
**Resume** (per channel or all) sends the profile's current value at once. Resuming the
setpoint or the heater puts the roaster back in the drive's mode first: a Manual or Auto
pressed by hand pauses them and leaves the roaster in the other mode, and without it BT would
stop following while the tab showed the drive live.
Emergency stop, session complete/delete and DROP stop the run, and a run
cannot start once DROP is marked. Stopping a run
never changes the outputs; the operator owns them from there. A stop waits for the run's
task to end, so a tick caught mid-send cannot deliver a command after DROP's heater off and
cooling fan.

Pausing the roast (Roast page → Pause: "stop recording for a moment; the roaster carries
on") does not pause the drive: it keeps driving and holding its lease. (Skipping its ticks
would let the lease run out, and the roaster would cut its heater 10 s later.)

**Resume after a restart.** A crash, a power cut or a restart by hand
(deploys wait for the roaster, `GET /api/busy`) ends the process and its runs. On boot every
run left open is closed ("server restarted"); a live one whose roast is still on (active or
paused, no DROP, its profile still there) is rebuilt as a stopped run: `stopped_by_restart`,
`stopped_reason` "the server restarted", and the clock placed where it stopped (the roast's
last sample before the restart, else the run's last log entry), so `profile_t` is the profile
time it had reached. `GET /api/sessions/{id}/replay` returns it; the Roast page shows a banner
and `roast live` the resume call. The roaster meanwhile went to its safe state when its
lease ran out (firmware with `lease`), or held its last output.

**Heater on and resume drive** (`POST /api/sessions/{id}/drive/resume`, as after a heater
cut) starts a run with the old settings and `resume_at_profile_seconds` set to that profile
time. A start with it (`POST /replay` takes it too) puts the time lost since on the profile
offset: `config.hold` `{seconds, since_elapsed}`, added to `align.offset_seconds`, so the
profile picks up where it stopped and its rest comes that much later. With `align` on a later
landmark, one marked after the resume places the profile by itself and the hold no longer
counts; one marked before the cut keeps its offset with the hold on top. The first tick
catches up as any mid-roast start does: the last value per channel applies, past cues are
skipped; BT-triggered events fire again once BT gets back to them. A resume after a heater cut
still picks up at the profile time now.

**Lost telemetry.** With no fresh sample for 8 s the drive stops renewing its lease, so a
roaster that can still hear the server but cannot be watched falls into its safe state by
itself; at 20 s the drive stops ("no telemetry from the roaster for 20 s") and, if commands
still reach it, leaves the roaster in Manual at 0 %. While a sample is stale the server PID
holds its last output.

## Auto DROP

Auto DROP is for a profile that is dialed in: once its roasts repeat within about 0.5 °C and
drop within seconds of the profile's drop time, marking DROP by hand adds nothing.
Code: `replay/auto_drop.rs` (pure, unit-tested), the engine's `step_auto_drop`, the roast
sequence's `on_drive` (`sequence.rs`).

A profile's `auto_drop` (migration 041) says what it drops by; off (null) is the default, so
every profile starts without it:

| `auto_drop` | Drops at | Needs |
|---|---|---|
| `time` | profile time = the drop time (`target_total_time`). Profile time, so an align on a later landmark or a resumed drive's hold moves the drop with the rest of the profile | the drop time |
| `bt` | BT = the drop temperature (`target_end_temp`), counted only once first crack is marked or the profile is past its first crack time (else 80 % of its drop time): a spike or a hot probe early cannot drop the roast. Two ticks in a row within reach; the countdown ends where the RoR says BT gets there | the drop temperature (100–300 °C) |
| `development` | the profile's development time (drop minus first crack) after first crack is **marked**: first crack stays the operator's mark | first crack and drop times, drop after first crack |

The profile is refused (400) when it lacks what its trigger needs, and so is an update that
would take it away while auto DROP stays on ("switch auto DROP off first").

**The countdown.** At the drive's start the profile's auto DROP goes into the run
configuration (`config.auto_drop`, with `countdown_seconds` from the setting
`auto_drop_countdown_seconds`, 3–60 s, default 10). Each tick estimates the seconds to the
drop; once that is within the countdown, the countdown starts so that it **ends on the drop**
(DROP at the profile's 10:05, not 10 s after it). A drive started past the drop, or a first
crack marked late, gets the full countdown, so there is always time to cancel. Once it runs,
nothing but a cancel stops it (a BT dip does not). A moved CHARGE starts it over.

**What happens.** The roast sequence watches the drive's state: when the countdown starts it
sends a notice as an alarm ("Auto DROP: DROP in 10 s, at the profile's drop, 10:05. Cancel on the
Roast page to roast on.": the Roast page's toast, phones and the notification channels); when
it ends (`state: due`) it marks DROP (`notes`: "marked by auto DROP (…)"), and DROP's step runs
as for any mark: the drive stops, heater power off, Manual at 0 %, the fan to cool. Each step
runs once per drive.

**When it stays off** (the start log says why): the roaster's DROP step is off (nothing would
switch the heater off once the drive stops), or the profile lacks its target. A ghost run shows
when it would drop and never counts down; engaged, it does. No drive, no auto DROP: roasting by
hand with a profile on the scope drops by hand.

**Cancel** (the Roast page's Cancel, `PATCH /api/sessions/{id}/replay` `{"auto_drop": false}`)
stops it for this roast only; `true` puts it back (it counts down again if the drop is past).
A drive resumed after a heater cut or a restart keeps it cancelled. The state (`auto_drop` in
the drive's state) has `by`, `at`, `countdown_seconds`, `cancelled`, `state` (`armed`,
`counting`, `due`, `cancelled`), `eta_seconds` (the countdown, else an estimate),
`drop_at_elapsed`, `waiting_for` (CHARGE, first crack, a fresh BT), `ghost` and `describe`.

Checked on the virtual roaster by `drive-check.mjs`: a copy of the profile with auto DROP by
time 40 s after the drive's minutes; the countdown shows, the Roast page fits at 1280×720
during it (below 820 px tall the landmark buttons shorten and their turning-point note hides
while the auto DROP line shows), and DROP lands on the drop (100.1 s for 100 s).

## Logging

Migration 011 adds `replay_runs` (one row per run: session, profile, config,
start/end, end reason) and `replay_log` (start, stop, engage, align change,
event fired / held / superseded / skipped / delayed, channel paused /
resumed), each entry with `live_t`, `profile_t`, channel, value and the
profile event id. Curve-driven sends are not logged one by one; the run
config plus the session telemetry (setpoint, heater, fan per second)
reconstruct them. Runs left open by a server restart are closed on boot (and one whose roast
is still on is offered for a resume, above).

## API

| Method | Path | |
|---|---|---|
| GET | `/api/sessions/{id}/replay` | state, or `{active:false}` |
| POST | `/api/sessions/{id}/replay` | start with a run configuration |
| PATCH | `/api/sessions/{id}/replay` | change `ghost`, `align`, `trigger`, `lookahead_seconds`; `auto_drop: false` cancels auto DROP for the roast |
| DELETE | `/api/sessions/{id}/replay` | stop |
| POST | `/api/sessions/{id}/replay/pause` / `resume` | `{channel?}`; none = all |
| POST | `/api/sessions/{id}/replay/events/{event_id}` | `{op: skip \| delay \| fire, seconds?}` |
| GET | `/api/sessions/{id}/replay/log` | runs and log entries |
| GET | `/api/replay` | all active runs |

`/api/sessions/{id}/follow` and `/api/follow` are aliases (mode
`setpoint`, fan off). The JSON telemetry WebSocket streams `{device_id, replay: state}` once
per tick.

State per tick: config, `live_t`, `profile_t`, align offset (s and °C), per
channel `{driven, paused, target, last_sent, last_sent_at}`, the next five
events with ETA (seconds, or °C to go plus an ETA from the current RoR), the
last ten log entries, sends, last error, stale-telemetry flag.

## Code layout

- `replay/auto_drop.rs` — auto DROP's trigger, countdown and state (pure).
- `replay/engine/` — pure state machine: compiled profile plan + run
  config + per-tick inputs → channel targets, event transitions, log
  entries. No I/O; all the behaviour above is unit-tested here.
- `replay/service.rs` — `ReplayService`: per-session task (1 Hz), reads the
  session, its landmarks and the device's latest ring sample, sends commands
  through the transport registry, persists the log, broadcasts state.
- Dashboard: `ReplayPanel.svelte` (start
  options, channel pause/resume, next five events with countdowns and
  skip/delay/fire, ghost engage, log tail); `scopeData.ts` anchors profile
  curves and ticks at CHARGE plus the align offset; the scope shows the
  offset and dims fired or skipped event ticks.

## Not built

- Drawing replay logs in the comparator (it would read `replay_log`).
