# Roast sequence

A roast sequence is what Charge and Drop do to the roaster, so you don't have to remember to
set the fan and switch the heater on at the start, or to switch it off and cool at the end.
Each roaster has its own, and it runs whoever marks the event: a button on any screen, an
alarm, the auto events or a profile's auto DROP.

Set it under Roasters → the roaster → Control → **Roast sequence**. On the Roast page the step
card says what Charge and Drop will do.

## What happens

| When | What the roaster does |
|---|---|
| **Charge** | The fan goes to its charge setting and heater power goes on. If the roast has a profile and "start Profile drive" is ticked, the heater starts at the set % and then [Profile drive](replay.md) takes over, live, with its default settings. A drive already running for the roast is left alone. |
| **Drop** | The drive stops, heater power goes off and the heater goes to 0 % in Manual, so switching power on later does not heat. The fan cools at its cooling setting until BT and ET are both below the cool-down temperature; then the fan goes off and **COOL END** is marked. |
| **Cancel** (Roast page) | The roast is discarded: the drive stops, heater power off, Manual at 0 %. A hot chamber cools as after Drop, without a mark; a cold one gets the fan off. This always runs, whatever the settings. |

Start roast only starts recording; it does not touch the roaster.

A roast has one Charge and one Drop. If Drop was already marked (by auto DROP while the Roast
page was in a background tab, say), pressing Drop again does nothing to the roaster: the page
says so and shows the roast as dropped, ready to complete. A page coming back from a background
tab also reads the roast's marks again by itself.

## Settings

| Setting | Default | What it does |
|---|---|---|
| At Charge: heater power on, fan to | on, 100 % | The fan at Charge. Untick to make Charge only a mark. |
| and start Profile drive when the roast has a profile | on | Starts the drive at Charge. |
| with the heater starting at | 60 % | The heater the roaster's PID takes over from. 0 starts it from nothing, which holds the heater low for the first minute or more. |
| At Drop: stop the drive, heater power off, cool at | on, 100 % | The cooling fan. Untick to make Drop only a mark. |
| until BT and ET are below | 45 °C | When cooling ends. 0 keeps the fan running until you turn it off. |

Both fans must be at the roaster's fan floor or more (45 % on a fluid-bed roaster:
[roaster setup](roaster-setup.md)). The fan is always set before heater power goes on.

Good to know:

- **Hot means BT or ET.** Just after a cold charge the beans can read 30 °C while the air is
  80 °C, so cooling waits for both.
- **Complete while it cools** and the roast keeps recording: the samples go on the roast
  until COOL END, which is marked on it, so weighing and notes need not wait for the fan. The
  Roast page says so under Saved ("Cooling is still recorded"). The statistics run CHARGE to
  DROP as before.
- **Cooling gives up after 20 minutes**, and a new Charge ends the last roast's cooling.
- **Each kind of roaster starts with its own sequence.** A fluid bed's acts as above; a drum's
  and a logger's only record, since a drum is preheated and keeps running. Changing the kind
  resets the sequence. A step for an output the roaster doesn't have is skipped.
- **A fan change from the sequence** pauses a running drive's fan, and becomes fan assist's
  ceiling, the same as moving the fan yourself.
- **Each step runs once per roast**, and an old mark does nothing, so reloading the page or
  restarting the server does not run it again.

## When the drive stops mid-roast

If the roaster cuts its heater during a drive, the drive stops, the roaster is put in Manual
at 0 % and the Roast page shows a banner with the time and the reason. **Heater on and resume
drive** switches heater power on at the starting heater % and picks the drive up at the
current profile time. **Roast by hand** hides the banner. A server restart that stopped the
drive gets the same banner; its resume picks up where the drive stopped, the rest of the
profile moved later by the time lost. A STOP you press yourself is not
a cut and offers no resume.

From the command line, `roast roaster show <id>` says what Charge and Drop do on that roaster.

<!-- site: end -->

## Design notes

For developers: how the roast sequence works and why. The site publishes only the part above.

Server: `crates/server/src/control/sequence.rs`, settings
`GET/PUT /api/roaster/{device_id}/sequence`, cancel `POST /api/sessions/{id}/cancel`.
Dashboard: Roasters → the roaster → Control → **Roast sequence**; the Roast page's step card
says what Charge and Drop will do.

## Why

If Start, Charge and Drop only recorded, the roaster would sit at heater 0 %, fan 0 % and
setpoint 0 until set by hand, and Profile drive would never switch heater power on: the roaster
would have to remember to turn the fan on above the minimum and the heater on before the roast
follows the profile, and nothing would happen at DROP. That should be automatic. Artisan does
this with event-button commands per machine; here it is one per-roaster sequence the server
runs.

## What it does

| When | Does (in order) | Default |
|---|---|---|
| **CHARGE** is marked | fan to `charge_fan_pct`; with a drive to start, the roaster to Manual, heater power on and the heater to `charge_heater_pct` (see "Starting the PID"), else just heater power on; Profile drive starts **live** if the roast has a profile (`charge_drive`), with the default drive settings (setpoint mode, events as the profile says, align on CHARGE, fan from the profile, fan assist as saved). A drive already running for the roast (a ghost, say) is left alone, fan included. | on, fan 100 %, heater 60 %, drive on |
| **DROP** is marked | the drive stops; heater power off; the roaster to Manual at heater 0 % (so heater power switched on later does not heat towards the last setpoint); fan to `cool_fan_pct`; once BT and ET are both below `cool_end_bt_c` the fan goes off and **COOL END** is marked (`0` keeps the fan running). Cooling gives up after 20 minutes. | on, 100 %, 45 °C |
| **Cancel** (Roast page) | the drive stops; heater power off; Manual at 0 %; if BT or ET is above `cool_end_bt_c` (45 °C when that is 0) it cools as after DROP, without a mark, else fan 0; the roast is deleted. Always runs, whatever the switches. | — |

Start roast still only starts recording: the fluid bed is charged cold, so there is no
preheat to run before CHARGE.

- **Any mark counts.** The service listens to the landmark broadcast the alarm service
  sends for every change (`session_events`), so a button on any screen, an alarm's `mark`
  action, the auto events' DROP detector and a drive's auto DROP all run the step.
- **Auto DROP** (`replay.md`, "Auto DROP"; a profile switches it on): the service also watches
  the drive's state, sends the notice when its countdown starts and marks DROP when it ends.
  A drive starts it only with this roaster's DROP step on: otherwise the drive would stop at
  DROP and nothing would switch the heater off. A mark older than a minute does nothing, so a
  server restart or a page reload does not replay it; each step runs once per roast.
- **A new CHARGE ends the last roast's cooling** (and so does a resume): left running, its
  fan-off below `cool_end_bt_c` would land in the new roast, pause its drive's fan channel
  and gate the heater.
- **Hot means BT or ET.** A hot-air heater wants airflow until the air is cool, not only the
  beans: just after a cold charge BT can be 30 °C while ET is 80 °C.
- **Complete while cooling keeps the trend.** The telemetry service stores a sample for the
  roast running on the roaster (`status = 'active'`) and, while the sequence cools a roast,
  for that roast once it is complete too (`TelemetryService::record_cooling`, set when
  cooling starts, cleared when it ends however it ends: the threshold, the 20-minute
  give-up, a new CHARGE). COOL END is marked on an active or a completed roast. A recorded
  roast had DROP, Complete a second later and its trend ending at DROP while the roaster
  cooled for two minutes; refusing Complete while cooling was the alternative, worse because
  the roast is weighed and noted right after DROP. The statistics stay DROP-based: the roast
  time runs CHARGE → DROP and the peaks are before DROP, so the cooling samples change none
  of them. `GET …/sequence` returns `cooling_roast_id`, the roast being cooled; the Roast
  page reads it after Complete and says "Cooling is still recorded" until it clears.
- **Fans stay at the roaster's fan floor or more** (validated; 45 % on the fluid bed, whose
  firmware's heater interlock is 40 %, `SAFETY_MIN_FAN_PCT`), and the fan is set before heater
  power goes on.
- **Each kind has its own sequence** ([`roaster-setup.md`](roaster-setup.md)): the
  fluid bed's acts as above; a drum's and a logger's only record (a drum is preheated and keeps
  running). A roaster with no saved sequence gets its kind's, and changing the kind resets it.
  Steps for an output the roaster has not declared are skipped.
- Commands go the way the alarm actions send them: a fan change pauses a running drive's
  fan channel and becomes the fan assist ceiling, and the gain schedule follows the fan.
- The dashboard's own DROP still switches heater power off as before, and on the virtual
  roaster Charge and Drop still load and dump the simulated beans.
- **One CHARGE and one DROP**: `POST …/events` refuses a second with 409 `ALREADY_MARKED`;
  moving one is `PUT`. The Roast page's socket closes in a hidden tab, so it can miss the auto
  DROP's `session_events` notice, keep its DROP button and mark DROP again (a recorded roast
  did, at 45 °C). The store reads the marks again on every reconnect, and on `ALREADY_MARKED`
  takes the server's list instead of acting on the roaster.

## Starting the PID

A recorded roast with the sequence on the real roaster (empty chamber), starting the PID from
heater 0 %, ran 7 to 11 °C behind the profile for the first minute and a half, with the heater at 2–40 % while BT
was 20 °C below its setpoint. The firmware starts its PID bumplessly from the output the
heater has (`Pid::reset`: `integ = currentOut − kp·e`), and with heater power just switched on
that output is 0 (`heater_control.cpp` holds it at 0 while power is off). With the gains at
100 % fan (kp 10.6 in 0..255 units per °C) and a 23 °C error the integral started near
−244 of 255, about −96 % of output, and took some 90 s to unwind at ki 0.13.

So at CHARGE, with a drive to start, the sequence puts the roaster in Manual, switches heater
power on, sets the heater to `charge_heater_pct` (default 60 %) and only then starts the
drive, whose switch to Auto takes over from that output instead of from 0. The order matters: the firmware drops a manual % set while heater
power is off. `charge_heater_pct` 0 starts the PID from 0 %. The virtual roaster copies the
firmware here (output held at 0 while heater power is off, PID restarted when it comes back).

Not the cause: the setpoint runs ahead of BT on the scope by the profile lookahead
(`profile_lookahead_seconds`, 20 s), +15 °C at a 46 °C/min RoR and +1 °C near the end; the
Δ profile readout compares BT with the profile itself, not the setpoint.

BT minus profile on the virtual roaster, Light espresso from a cold charge (°C):

| Start output | 0:10 | 0:20 | 0:40 | 1:00 | 1:30 | 2:00 |
|---|---|---|---|---|---|---|
| 0 % | −11.0 | −13.0 | −10.9 | −7.7 | −3.9 | −2.0 |
| 40 % | −7.2 | −6.5 | −3.7 | −2.2 | −0.2 | +0.5 |
| **60 % (default)** | −5.2 | −3.0 | −0.2 | +0.8 | +1.8 | +2.0 |
| 100 % | −1.7 | +3.3 | +6.6 | +6.1 | +5.3 | +4.5 |

About 5 °C of the first seconds is the profile itself: it starts at 30 °C, beans charged
at room temperature. 60 % is close to the holding power autotune measured at 100 % fan
(62 %). The real roaster's scheduled gains are stiffer than the simulator's, so its best
value may differ; it is a per-roaster setting.

## When the roaster cuts its heater mid-roast

A recorded roast had its heater cut at 5:20 with nothing on the server's side (no stop, no
heater-off, lease renewed 2 s before; cause unknown). What the server does when that happens:

- Profile drive stops when heater power goes off, marks the run `stopped_by_device`, puts the
  roaster in **Manual at 0 %** (switching heater power back on would otherwise heat towards the
  run's last setpoint: that roast sat at 170 °C for 2½ minutes on the old 173 °C setpoint), and
  keeps the stopped run so `GET /replay` still returns it after a reload.
- The reason: firmware 3.3.2 latches why it last cut the heater (`stopCode`,
  `stopUptimeMs`, `hardware-integration-guide.md`); when the record changed since the run
  last saw the heater on, the stop reason names it ("the roaster cut its heater: control
  lease expired …"). Older firmware gives "safe state or operator".
- The Roast page shows a banner (and a toast) with the time and the reason, and **Heater on
  and resume drive** (`POST /api/sessions/{id}/drive/resume`): Manual, heater power on, the
  heater at `charge_heater_pct`, then the drive with the stopped run's settings, picking up
  at the current profile time. "Roast by hand" hides the banner. A STOP pressed in the
  dashboard is not a cut: it stops the drive on purpose and offers no resume.
- The same banner and resume follow a server restart that cut the drive mid-roast
  (`stopped_by_restart`): the resume then picks up at the profile time where the drive
  stopped, the time lost added to the profile offset (`replay.md`, "Resume after a restart").

## Settings (`roast_sequence:{device_id}` in `settings`)

```json
{"charge_enabled": true, "charge_fan_pct": 100, "charge_drive": true, "charge_heater_pct": 60,
 "drop_enabled": true, "cool_fan_pct": 100, "cool_end_bt_c": 45}
```

`charge_fan_pct` and `cool_fan_pct` 45..100 %; `charge_heater_pct` 0..100 %; `cool_end_bt_c` 0
(off) or 25..120 °C.
`GET` also returns `cooling`: the roaster is cooling after a DROP or a cancel; and
`cooling_roast_id`, the roast whose cooling it is (null after a cancel), recorded until COOL END.

## Checked

`drive-check.mjs` (virtual roaster): Charge starts the drive with heater power on (it
fails if the drive needs a click or the heater warning shows), Drop leaves heater power
off and the fan at 100 %, Complete during the cooling says "Cooling is still recorded", and
Cool end is marked on the completed roast by itself, its trend reaching it.
`control::sequence::tests::complete_while_cooling_keeps_the_trend_to_cool_end` is the
server's test of the same.
