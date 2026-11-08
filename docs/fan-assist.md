# Fan assist (valve-position control of the fan)

On a fluid-bed roaster the fan carries the heat, and late in the roast it can carry it away
faster than the heater puts it in: the heater sits at 100 % and bean temperature stops
climbing. A roaster then lowers the fan by hand. Fan assist does it for you: while the
heater is pinned it takes a little air away, and once the heater has room again it gives the
air back.

It needs a roaster with both a heater and a fan output, with temperature control on (the
roaster's own PID in auto, or a profile driving the bean temperature).

## Using it

- **Switch it on** on the Roast page's Control tab ("Fan assist", next to the fan slider), or
  for a roaster by default under Roasters → the roaster → Control → Fan assist.
- **The fan you set is the most it uses.** The slider becomes the ceiling; the assist only
  ever takes air away from it, and the readout shows the trim next to the fan
  (`assist −8 %`). While a profile drives the fan, the profile's fan is the ceiling.
- **Moving the fan yourself** sets a new ceiling with no trim. Switching the roaster to manual
  or turning the heater off stops the assist and hands the fan back.
- **It waits for the beans to dry.** Right after CHARGE the heater is pinned on purpose, so the
  assist only starts once bean temperature passes 150 °C.

## Settings

Roasters → the roaster → Control → Fan assist:

| Setting | Default | What it does |
|---|---|---|
| Minimum fan | 70 % | Never below this. It cannot go under the roaster's fan floor while heating ([roaster setup](roaster-setup.md)). Keep it where the beans still move. |
| Start above BT | 150 °C | Below this the assist waits; 10 °C under it, it lets go. |
| Lower fan when heater above | 92 % | The heater (smoothed over 10 s) above this takes air away. |
| Give fan back when heater below | 80 % | Below this the trim is returned. Between the two, nothing moves. |
| Lower at most / give back at most | 15 / 6 %/min | How fast the fan may change. |
| Faster per °C behind | 2 %/min | Extra speed for each °C the bean temperature is behind its target. 0 = heater only. |

If the fan and the heater hunt, widen the gap between the two heater thresholds or slow the
rates. With [gain scheduling](autotune-2.md) the PID gains follow the fan the assist sets, the
same as for a fan you move yourself.

From the command line, `roast roaster show <id>` includes the fan assist's settings and state.

<!-- site: end -->

## Design notes

For developers: how fan assist works and why. The site publishes only the part above.

## Why

The roaster is heat-limited. At 100 % fan the heater cannot carry BT past
roughly 185 °C, so the operator lowers the fan by hand late in the roast to
reach 200 °C. Fan assist does that automatically: it watches the BT loop's
heater output and, while the heater is pinned, takes a little air away; once
the heater has room again it gives the air back.

This is valve-position control in process-control terms: the fan is a slow
secondary handle that keeps the primary loop (heater on BT) off its limit.

## Behaviour

- **Ceiling.** The fan you set (or the profile's fan during a replay) is the
  most the assist will use. It only ever takes air away.
- **Floor.** `min_fan_pct`, default **70 %**. Minimum
  allowed value is the roaster's fan floor while heating: 45 % on the fluid bed, a margin
  above the firmware's 40 % fan interlock ([`roaster-setup.md`](roaster-setup.md), "The
  interlock"). If you set the fan below the floor yourself, the assist leaves it alone. The
  assist runs only on a roaster with both a heater and a fan output.
- **Band.** The heater output is filtered (10 s time constant). It is the heater the roaster
  applied (`heaterApplied`: after its RoR limiter and fan interlock) when it reports one, so
  a heater the limiter is holding back does not count as pinned. Above
  `high_heater_pct` (92 %) the fan goes down at 1 %/min per % of heater over
  the band, plus `lag_gain_pct_per_min_per_c` (2) %/min per °C BT is behind
  its target beyond a 1 °C deadband, capped at `max_down_pct_per_min` (15).
  Heater pinned but BT more than 1 °C *ahead*: hold (the loop is about to
  back off). Below `low_heater_pct` (80 %) the trim is given back the same
  way (headroom, plus the lag term when BT is ahead), capped at
  `max_up_pct_per_min` (6). Inside the band nothing moves, so the two loops
  do not fight.
- **Target.** The setpoint the BT loop chases: in a replay the profile with
  its lookahead (the operator's setpoint while that channel is paused),
  without a replay the roaster's setpoint. (Measured against the profile
  *now*, BT runs a lookahead's worth "ahead" by design and the assist would
  hold while the heater was pinned.)
- **Arming.** Only once BT reaches `arm_bt_c` (150 °C); it disarms 10 °C below.
  Right after CHARGE the heater is pinned while the beans dry, and the fan
  should stay where you put it. Disarmed, any trim drains back at the up rate.
- **When it acts.** With the roaster PID in auto (or a closed-loop replay) and
  the heater enabled. It holds on stale telemetry. Heater off (safe state,
  e-stop): it stops and leaves the fan to the roaster. Roaster switched to
  manual: it stops and returns a trimmed fan to the ceiling.
- **Manual fan.** A fan command from the dashboard or API becomes the new
  ceiling with no trim. (In a replay whose profile drives the fan, a manual
  fan pauses the fan channel as without the assist, and the assist with it.)
- **Gain schedule.** Every assist fan change goes through the gain scheduler,
  so the PID gains follow the fan as they do for manual changes.

## Where it runs

- **No replay** (`fan_assist.rs`, `FanAssistService`): a 1 Hz loop per device
  with the assist on. Ceiling: your last fan command, else the fan the roaster
  reported when the assist first armed. After a replay, the fan the replay
  left becomes the ceiling. A server restart forgets the ceiling, so a
  trimmed fan then counts as the ceiling; set the fan again to restore it.
- **In a replay** (`replay/engine.rs`): closed-loop modes only. The run takes
  the device's saved setting at start (`fan_assist: true|false` in the start
  request overrides it). Ceiling: the profile fan when fan replay is on, else
  the fan at the start of the run (a fan channel with source `assist`). The
  heater signal is the server PID output with the loop on the server, else
  the heater the device reports. Changes to the setting reach a running
  replay. Trimming, reaching the floor and giving the trim back are logged as
  `assist` entries in the replay log; the state snapshot carries
  `fan_assist`.

## API

`GET /api/roaster/{id}/fan_assist` → `{device_id, config, state}`; `state` has
`source` (`replay`, `manual` or null), `active`, `reason` and `view`
(`status`, `ceiling`, `floor`, `trim`, `fan`, `heater`, `armed`).

`PUT /api/roaster/{id}/fan_assist` with the full config (missing fields take
their defaults):

```json
{"enabled": true, "min_fan_pct": 70, "arm_bt_c": 150,
 "high_heater_pct": 92, "low_heater_pct": 80,
 "max_down_pct_per_min": 15, "max_up_pct_per_min": 6,
 "lag_gain_pct_per_min_per_c": 2}
```

Stored as setting `fan_assist:<device_id>`.

## Dashboard

- Roast page → Control tab: a "Fan assist" switch next to the fan slider, and a
  status line under it (trim, what it is doing, filtered heater). The slider
  is the ceiling ("max"); the readout strip shows the trim next to the fan.
- Roasters → the roaster → Control: the Fan assist panel with every setting and the live status.
- Roast page → Profile drive tab: the fan channel shows `assist −N %` while trimmed.

## Staging check

Simulated heat-limited roaster (PID in auto, BT levels off at ~185 °C at
100 % fan), setpoint 200 °C, defaults: the assist lowered the fan 100 → 85 % at
10 %/min, the heater came off 100 % to the 92 % band edge and BT reached
199 °C. Setpoint down to 180 °C: the fan came back to 87 % and held with the
heater at 84 %. Manual fan 90 %: new ceiling, no trim. Assist off while
trimming: fan back to 90 % at once.

## First roast on the roaster

Medium profile BT curve, fan replay off, device PID with the gain schedule,
assist on with the then defaults (10 %/min, heater-only). BT tracked the
profile within 1.7 °C from 2 to 7 min and 1.1 °C from 9:20 to the drop
(218.8 °C at 14:05); the assist took the fan from 100 % to 75 % on its own.
Between 7 and 9:20 min BT fell up to 11 °C behind: the heater was pinned
but filtered at 96–98 %, so the heater-only rate was 4–6 %/min. Hence the
lag term and the 15 %/min cap (the same stretch now trims at the cap).

## Open

- Tune on the roaster: the band and rates are first guesses. If the fan and
  heater hunt, widen the band or slow the rates.
- With gain schedule rows at 100 % and ~75 % fan, the PID gains will follow
  the assist's fan; run those two autotunes ([`autotune-2.md`](autotune-2.md), "Gain
  schedule").
