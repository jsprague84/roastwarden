# Alarms, actions and auto events

An alarm watches every roast for something you care about: BT too high, the roast falling
behind its profile, the roaster gone quiet, an RoR crash on its way. It can just tell you, or
act on the roaster: cut the heater, raise the fan, mark DROP. Alarms run on the server, so
they work with no browser open. The same loop marks the turning point, DRY end and DROP for
you.

## Setting up alarms

Settings → Alarms. **Add suggested** gives a starting set; **Add alarm** a blank one. Each
alarm has a name, a condition and a threshold, then optional extras. Click **Edit** to open
one, and **Save alarms** when done.

| Condition | Fires when |
|---|---|
| BT above / below, ET above / below | the temperature crosses the threshold |
| RoR above / below | BT's rate of rise crosses it (°C/min) |
| BT ahead of / behind profile by | BT minus the profile's BT now passes it; use a negative number for behind. Not after DROP |
| Channel above / below | one of the roaster's [declared channels](channels.md) crosses it, in that channel's unit. On a roaster without that channel it never fires |
| Time after landmark | that many seconds have passed since a landmark (CHARGE, first crack, DROP …) |
| No telemetry for | the roaster has sent nothing for that many seconds: sensor, Wi-Fi or broker lost |
| RoR crash / RoR flick / BT stall ahead within | the live prediction expects one within that many seconds (0 to 120), or sees it now. From DRY end to DROP; the message says why ([prediction](prediction.md)) |

The extras:

- **Armed after**: the alarm only counts once a landmark is marked (say, after the turning
  point, so the charge dip does not trip it).
- **and if "…" fired**: only after another alarm fired in this roast, to chain steps.
- **Repeat when it comes back**: fire again each time the condition returns, at most every
  so many seconds (60 by default). Otherwise an alarm fires once per roast.
- **Then**: actions, run in order. With none, the alarm only notifies.

Thresholds are entered in the unit the dashboard shows.

## Actions

Set setpoint, Set fan, Set heater, Set mode (auto or manual), Heater off, Emergency stop,
Mark landmark, Pause replay, Stop replay.

An action is a command like the one you would give from the Control tab, and it behaves the
same way: it takes that control back from a running [profile drive](replay.md), a fan action
becomes [fan assist](fan-assist.md)'s ceiling, and it goes through the roaster's control level
([roaster setup](roaster-setup.md)): with control off the roaster refuses every action, and
at manual it refuses Set setpoint and Set mode. Marking a
landmark that is already marked does nothing. The fired alarm lists each action with its
result, or why it failed.

## The suggested set

| Alarm | What it does |
|---|---|
| Too hot: heater off | BT above 235 °C: heater off, fan 100 % |
| Telemetry lost: heater off | nothing for 15 s: heater off |
| After drop: cool | at DROP: stop the replay, heater off, fan 100 % |
| Behind profile | BT 8 °C behind the profile, after the turning point; repeats, at most every 120 s |
| RoR crash ahead, RoR flick ahead | warns up to 60 s ahead |
| BT stall ahead | warns up to 120 s ahead |

Adjust the numbers to your roaster: 235 °C is a ceiling for a light to medium roast.

## Where an alarm shows

A fired alarm shows as a message on the Roast page of every open dashboard, with a beep if
"Sound in the dashboard" is on. It shows once, when it fires: coming back to the Roast page
later does not repeat it. It is also sent to the phones and browsers that turned alarm notifications on
([push](push.md)) and to every notification channel that is on, such as ntfy, Gotify, Matrix or
a webhook ([notifications](notifications.md)).

## Landmarks marked for you

With **Mark landmarks automatically** on (the default):

- **Turning point**: the lowest BT after CHARGE, once BT has risen 2 °C from it. A charge
  where BT never dips (beans charged into a cold roaster) gets none.
- **DRY end**: when BT reaches the profile's own dry-end temperature, else the "Dry end at
  BT" setting (150 °C by default), at least 60 s after CHARGE.
- **DROP** (its own switch): once first crack is marked or BT passed 190 °C, a sustained fall
  (BT 15 °C below its peak and falling 20 °C/min or faster). It is marked at the moment BT
  left the peak, and it ends a running profile drive.

First crack is never marked for you: an RoR dip is too unreliable. The Roast page prompts
you instead.

## Artisan alarm files

**Import** reads a Roastwarden export or an Artisan `.alrm` file (a file or pasted text);
**Add to list** keeps your alarms, **Replace list** swaps them. An `.alrm` does not record
its temperature unit, so the import asks which one Artisan ran in. **Export .alrm** writes
one in the unit shown. After each import or export the editor lists, row by row, what did not
carry over: Artisan has no names, no repeat and nothing like "behind profile" or the RoR
warnings, and Roastwarden has no Call Program or Event Button actions.

From the command line, `roast settings roast_alarms` shows the alarms as JSON, and
`roast settings set roast_alarms=-` writes them from stdin (it needs admin).

<!-- site: end -->

## Design notes

For developers: how alarms and auto events work and why. The site publishes only the part above.

Server: `crates/server/src/analysis/alarms.rs`.
Dashboard: Settings → Alarms (`/settings/alarms`, `AlarmEditor.svelte`).
Auto TP and DROP ignore samples before CHARGE when CHARGE is marked late.

Alarms are evaluated on the server for every active roast (every telemetry
sample, plus a 1 Hz watchdog for silence), so they work with no dashboard
open. A fired alarm is shown as a toast (with a beep), pushed as a notification to every
device that turned alarm notifications on (`docs/push.md`), sent to every notification
channel that is on (Gotify, ntfy, Matrix, webhook: `docs/notifications.md`), and runs its
actions. The Roast page announces each firing once (`lib/alarms/announce.ts`): the store keeps the
newest alarm, so the page opened again would toast and beep it again, even with no roast
running; it remembers the last one announced and skips one heard more than 15 s before the
page opened.

## Rules (`roast_alarms` setting, JSON list)

| field | meaning |
|---|---|
| `name` | unique name (used by `requires`) |
| `condition_type` | see below |
| `threshold` | °C, °C/min or seconds (always °C on the wire) |
| `reference_event` | `time_after_event`: the landmark the time counts from. Other conditions (legacy): arms the rule |
| `armed_after` | armed only once this landmark is marked |
| `requires` | armed only once the named alarm fired in this roast |
| `repeat`, `cooldown_seconds` | fire again each time the condition comes back, at most every cooldown (default 60 s); otherwise once per roast |
| `actions` | list, run in order; none = notify only |
| `enabled` | default true |

Conditions: `temp_above`/`temp_below` (BT), `et_above`/`et_below`,
`ror_above`/`ror_below`, `delta_above`/`delta_below` (BT minus the
profile BT now: the running replay's aligned target, else the session
profile at the time since CHARGE; negative = behind; none once DROP is marked, so cooling
beans are not "Behind profile"), `channel_above`/`channel_below` (a roaster's declared
channel, named by the rule's `channel` key, threshold in the channel's unit, °C for a `C`
channel: `{"name": "Exhaust hot", "condition_type": "channel_above", "channel": "t3",
"threshold": 250}`; on a roaster that has not declared it the rule never fires, and the editor
says so; [`channels.md`](channels.md)), `time_after_event`,
`no_telemetry` (seconds since the last sample), `crash_ahead`, `flick_ahead`, `stall_ahead`
(the live prediction's RoR warnings, [`prediction.md`](prediction.md) "RoR warnings": the
threshold is how far ahead it warns, 0 to 120 s, 0 only once it is seen in the RoR; a warning
counts once two predictions in a row gave it; the message is the warning's reason; armed
from DRY end by the watch itself; never after DROP).

Actions: `{"type":"setpoint","value":°C}`, `fan`/`heater` (`value` %),
`mode` (`"mode":"auto"|"manual"`), `heater_off`, `emergency_stop`,
`mark` (`"event":"drop"` etc., skipped when already marked),
`replay_pause`, `replay_stop`. Control actions behave like the
dashboard's manual controls: they pause the matching replay channel, a fan
action becomes the fan assist ceiling and switches the scheduled gains.
The fired alarm reports one line per action (`"heater off: failed: …"` on
error).

Bad rules (unknown condition, out-of-range action value) are skipped one
by one and logged once, not the whole list.

Example:

```json
[
  {"name": "Too hot: heater off", "condition_type": "temp_above", "threshold": 235,
   "actions": [{"type": "heater_off"}, {"type": "fan", "value": 100}]},
  {"name": "Telemetry lost: heater off", "condition_type": "no_telemetry", "threshold": 15,
   "actions": [{"type": "heater_off"}]},
  {"name": "After drop: cool", "condition_type": "time_after_event", "threshold": 0,
   "reference_event": "drop",
   "actions": [{"type": "replay_stop"}, {"type": "heater_off"}, {"type": "fan", "value": 100}]},
  {"name": "Behind profile", "condition_type": "delta_below", "threshold": -8,
   "armed_after": "turning_point", "repeat": true, "cooldown_seconds": 120}
]
```

These four, and "RoR crash ahead" (60 s), "RoR flick ahead" (60 s) and "BT stall ahead"
(120 s), notify only, are the editor's "Add suggested" set. Import/export as JSON from
the editor (a list, or `{"alarms": [...]}`).

## Artisan alarm files (`.alrm`)

The editor imports an Artisan `.alrm` (file or pasted JSON; it is
recognised by its `alarmflags` list) and exports one (**Export .alrm**). The conversion is
`apps/dashboard/src/lib/alarms/artisan.ts` (tested), and after each import or export the
editor lists, row by row, what did not carry over. An `.alrm` does not record the unit
Artisan ran in: import asks (default: the unit shown here); export writes the unit shown.

An `.alrm` holds one list per column, one row per alarm (`alarmflags`, `alarmguards`,
`alarmnegguards`, `alarmtimes`, `alarmoffsets`, `alarmconds`, `alarmsources`,
`alarmtemperatures`, `alarmactions`, `alarmbeep`, `alarmstrings`; meanings in Artisan's
`alarms.py` and `canvas.py`, summarised at the top of `artisan.ts`).

| Artisan | Roastwarden |
|---|---|
| Source BT / ET / ΔBT, condition above or below | `temp_*` / `et_*` / `ror_*` |
| Source none (time only), From a landmark, offset | `time_after_event` from that landmark |
| From CHARGE, TP, DRY END, FC, SC, DROP, COOL END (temperature rows) | `armed_after` |
| From START or ON | always armed (Roastwarden alarms run during a roast) |
| "Only if alarm N fired" (guard) | `requires` (by name) |
| One action per row; "If Alarm" rows with no condition or delay | one rule with a list of actions (export writes such a chain) |
| Pop Up text | the rule's name (notify only); other rows are named "Artisan N: BT > 205" |
| Slider of event type 1 (Air) / 4 (Burner) | `fan` / `heater` %, assuming Artisan's default event names |
| SV, PID ON/OFF | `setpoint`, `mode` auto/manual |
| CHARGE, DRY END, FC/SC START/END, DROP, COOL END actions | `mark` |
| Playback OFF | `replay_stop` |
| Off (flag 0) | `enabled: false` |

**Not carried over**, and listed when met:

- Import: ΔET and extra-device sources, `=` and `≠` conditions, temperatures relative to
  the reading when another alarm fired, time after another alarm, "only if alarm N has
  *not* fired" (the rule imports without it), the time offset of a temperature row (Artisan
  fires on either; the rule fires on the temperature), actions Call Program, Event Button,
  Drum and Damper sliders, START, OFF, ramp-soak, Playback ON, canvas colours (the rule
  imports as notify-only), and rows whose guard did not import.
- Export: `delta_*` (BT against the profile), `channel_*`, `*_ahead` (the RoR warnings) and
  `no_telemetry` rules, `repeat` (Artisan
  fires once per roast), `armed_after` on a timed rule, `replay_pause`, `emergency_stop`
  and marking the turning point. `heater_off` is written as the Burner slider at 0. Names
  survive only for notify-only rules (as Pop Up text): Artisan alarms have no names.

## Auto events

With `auto_event_detection` on (default), the server marks:

- **Turning point**: the BT minimum after CHARGE, once BT is 2 °C above it
  and 20 s have passed, if BT dipped at least 3 °C below its CHARGE reading. A cold
  charge (the fluid bed) never dips, so it gets no turning point (the minimum would land at
  0:00) and the Roast page says so once DRY end is marked. Rules armed after the turning
  point (`armed_after: turning_point`, such as "Behind profile") then arm at the moment BT
  was seen rising without a dip: with no TP ever marked they would never arm.
- **DRY end**: BT reaches the roast profile's own dry-end BT (its `drying_end` landmark,
  160 °C on Light espresso), else `auto_dry_temp` (default 150 °C), after the turning
  point (or once BT rose without one) and at least 60 s after CHARGE. With the profile's
  value the roast's phases line up with the profile's, and an align on DRY compares like
  with like (the global 150 °C put DRY about 35 s before the profile's).
- **DROP** (setting `auto_drop`, default on; not a profile's auto DROP, which drops the roast
  at the profile's drop, `replay.md`): once first crack is marked or BT passed
  190 °C, BT 15 °C below its peak and falling at −20 °C/min or faster for 5
  samples; marked at the last moment BT was within 2 °C of the peak. DROP
  ends a running replay.

First crack stays a prompt on the roast page (RoR dip): too unreliable to
mark on its own. Landmarks the server marks reach open dashboards at once
(`{"type":"session_events"}` on `/ws/v2/telemetry`).

## Staging check

Simulated roaster, PID at 200 °C: TP and DRY auto-marked; `hot` (BT > 180)
set the fan to 90 % and marked FC; `chain` (`requires: hot`, BT > 183) cut
the heater; a rule with fan 500 % was skipped; stopping the simulator fired
`no_telemetry` after 6 s.
