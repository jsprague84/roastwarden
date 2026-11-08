# Prediction from the roaster's own model

Roastwarden learns how your roaster heats from your own completed roasts and uses that model
to look ahead during a roast: where bean temperature (BT) will be in one and two minutes, when
first crack and DROP should come, and whether the rate of rise (RoR) is about to crash, flick
or stall. It runs in the server, it says why it predicts what it does, and it keeps score of
itself on every roast, against the plain projection the scope already draws.

## What the model is

BT rises with the heater's share over the fan, through a short lag, and falls by a curve in BT
that your roasts teach it: the losses, drying, the reactions near first crack. With no roasts
yet it starts from the virtual roaster's physics; a few roasts outweigh that. It uses the
newest 30 roasts with a CHARGE, a DROP at least two minutes later, and heater and fan in their
telemetry (a roast imported with BT alone is left out), and refits by itself after a roast.

## Seeing it on the Roast page

- **Switch it on** in the scope's series toggles under the chart: **BT model**. It is off by
  default and remembered per browser.
- **The line** is the model's BT with its 90 % band, from CHARGE + 30 s to DROP, updated every
  5 s. The dashed **BT →** projection stays beside it.
- **The readout** above the chart then shows the model's BT in 1:00 and 2:00 with the band,
  first crack and DROP with their ranges, when the heater will reach 100 %, and any RoR
  warning ("BT stall ~9:05"). Hover it for the reason: the plan it assumed and how many roasts
  the model came from.

What it assumes the roaster will do next:

| The roaster is | The model predicts with |
|---|---|
| Driven by a profile ([Profile drive](replay.md)) | the profile's setpoints for the next minutes, the roaster's PID following them, the fan from the profile if it drives the fan, else held |
| In auto with a setpoint, no drive | that setpoint held |
| In manual | heater and fan held where they are |

**First crack** is predicted at the BT first crack came at on this roaster: this bean on this
profile if there is one, else this profile, else this bean, else every roast. With none marked
yet, the profile's own first crack. **DROP** follows the profile: its auto DROP if it has one,
else its development time after first crack, else its drop BT.

## RoR warnings and alarms

From DRY end (or BT 160 °C) to DROP the model watches the RoR, both as it is now and as the
model expects it in the next two minutes:

| Warning | It means |
|---|---|
| Crash | the RoR falls 8 °C/min or more within a minute, and by at least half |
| Flick | after first crack the RoR rises 2 °C/min above its lowest since, for 20 s |
| Stall | the RoR at 1 °C/min or less for 30 s before DROP, while the plan still wants BT to rise (a setpoint reached and held is not a stall) |

Each warning says what causes it: the heater reaching 100 %, the heater or fan moving, BT
passing first crack's temperature, the profile's own RoR.

To be told, add an alarm under Settings → Alarms: **RoR crash ahead within**, **RoR flick
ahead within** or **BT stall ahead within**, with how many seconds ahead (up to 120). It fires
when two predictions in a row agree. See [alarms](alarms.md).

## The Model panel

Roasters → the roaster → Overview → **Model** shows:

- when it was fitted, on how many roasts, and the heater lag it found;
- **BT two minutes ahead**: the RMS and 90 % error for "Heater and fan as they were" (the
  model's own quality), "The PID following the plan" (what a drive or a held setpoint shows
  live), "Heater and fan held" (what manual shows live), and "Today's projection" (what the
  model has to beat);
- **BT settles at heater/fan**: where BT stops rising at a few heater and fan values, your
  roaster's headroom;
- how the live prediction did on the last roasts, with their RoR warnings against what came;
- **Refit**, to fit it again now.

## Limits

- Batch size is not in the model. A charge much larger than your usual heats slower than
  predicted until roasts of that size are in the fit.
- Fan assist's moves are not planned: the fan is held where it is.
- A heater cut, an emergency stop or your own hand on the heater are not foreseen.
- Above the highest BT your roasts reached, the curve is the starting physics.
- Room temperature, the beans' moisture and the roaster body's warmth are not inputs, so the
  first roast of a session may differ from the next, and where BT settles is uncertain by
  several degrees.

From the command line: `roast roaster model <roaster> [--fit]` for the model and its scores,
`roast live` for the prediction now and its RoR warnings, `roast review <roast>` for a roast's
score ([command line](cli.md)).

<!-- site: end -->

## Design notes

For developers: how the prediction works and why. The site publishes only the part above.

Server: `crates/server/src/predict/` (`model.rs` the model, its fit and scores; `live.rs` the live
prediction; `ror_watch.rs` the RoR warnings; `mod.rs` the service, storage and scores),
migrations 042 and 056. Dashboard: the scope's **BT model** series (Roast page),
Roasters → a roaster → Overview → **Model**. Agents: `roast roaster model`, `roast live`,
`roast review`.

Cropster's Smart Predictions and Aillio's Smart Predict are trained on other people's roasts and
do not say why they predict what they do. This one is fitted on this
roaster's own roasts, runs in the server, gives its reason in words, and keeps score of itself
on every roast against the projection the scope has always drawn.

## The model

```text
dBT/dt = gain · x + g(BT)          x = lag_τ(heater / fan)
```

- `x` is the heater's share over the fan's, both as fractions (the air temperature the heater
  sets: the virtual roaster's `25 + 164 · heater / fan`), through a first-order lag `τ` (the
  element and the air). Below the firmware's fan interlock (40 %) no heat reaches the beans.
- `g(BT)` is a curve in BT, linear between knots every 20 °C from 20 to 260 °C: the losses, and
  whatever the beans do at that temperature (drying early, the reactions near first crack).
  The roasts teach it.
- ET is not used: on the fluid bed it is read above the bed and falls below BT late in a roast
  (on a recorded roast, ET 195 °C with BT 208 °C before DROP), so it is not the air that heats
  the beans.

**The prior** is the virtual roaster's physics in this form (`virtual_roaster.rs`: τ 8 s, gain
2.65 °C/s, `g = (25 − BT)/60`). A roaster with no roasts uses it; a fit starts from it.

**The fit** (`model::fit`): with `τ` fixed every other parameter enters linearly, so it is a
least-squares solve of `dBT/dt` (BT smoothed 2 s forward and back, then a central difference)
against `x` and the knots' hat functions, from CHARGE + 20 s to DROP, pulled lightly toward the
prior (a few roasts outweigh it) and kept smooth along BT (a second-difference penalty). `τ`
comes from a grid (2–25 s): the one whose fits, each made without one roast, predict that roast
best two minutes ahead. The newest 30 completed roasts with a CHARGE, a DROP at least 2 min
after it, and heater and fan in their telemetry (a roast imported with BT alone says nothing of
the heat and is left out). A roast without stored RoR gets one from BT (the slope over 30 s).

**Its score** (`FitQuality`): every roast predicted by a fit made without it, from CHARGE +
30 s every 15 s, at 30, 60, 90 and 120 s ahead (RMS, the 90 % bound, the bias), four ways:

| mode | what it predicts with | what it is for |
|---|---|---|
| `recorded` | the heater and fan the roast then had | the model's own quality |
| `plan` | the roaster's PID (its gains then) following the setpoint the roast then had, fan held; only where the roaster stayed in auto the whole time | what a drive or a held setpoint shows live |
| `held` | heater and fan held where they were | what manual control shows live |
| `projection` | the scope's projection: BT now plus the RoR now, bent by a falling RoR | what the model has to beat |

On esp32_roaster_01's first 12 roasts (τ 2 s, gain 1.84 °C/s), two minutes ahead:

| mode | RMS | 90 % within |
|---|---|---|
| recorded | 1.6 °C | 2.6 °C |
| plan | 1.6 °C | 2.9 °C |
| held | 18.2 °C | 23.8 °C |
| projection | 34.5 °C | 37.1 °C |

The plan is scored only where the roaster stayed in auto for the whole two minutes: no plan
foresees the roaster leaving it (a recorded roast where a lease bug put it in manual with the
heater off for 20 s), and live the prediction then changes to heater and fan held. Scored across
that cut, the plan's RMS was 4.4 °C. The roast itself stays in the fit: BT fell 28 °C in those
20 s and rose again when the heater came back, the only step in the heater in these roasts, and
the roast that pins the lag (without it the fit's lag runs to the end of the grid). The
projection's error is largest in the first minutes, where the RoR swings (35–47 °C RMS before
5:00, about 8 °C from 5:00 to first crack).

**Where BT settles** (`ModelView.settles`): the BT where `gain · x + g = 0` at a few heater and
fan values, the roaster's headroom. esp32_roaster_01: 186 °C at 100 % heater and fan (the
roaster "tops out in the 180s", `virtual-roaster.md`), 205 °C at 90/80 %, 224 °C at 100/80 %.

**Refits**: on start, and every 5 min when a roaster's list of usable roasts changed (a roast
completed or was deleted); `POST /api/roaster/{id}/model/fit` (or `roast roaster model --fit`,
the roast preset) fits now. Each fit is a row of `roaster_models` (the newest 20 kept).

## Live

From CHARGE + 30 s to DROP the server predicts every 5 s for each roaster with a running roast
(`GET /api/roaster/{id}/prediction`, 404 outside that). The plan it predicts under:

- **drive**: Profile drive's setpoints for the next 15 min (`Engine::plan_ahead`: the profile
  at profile time + the lookahead in use now), the roaster's PID following them (its reported
  gains; the server's for a server-loop drive), the fan from the profile when it drives the fan,
  else held (fan assist's moves are not planned); a feedforward drive's heater curve open loop;
- **setpoint**: no drive, the roaster's PID in auto: its setpoint held;
- **held**: manual: heater and fan held.

It gives BT every 5 s to two minutes ahead with a 90 % band (the fit's `plan` or `held` 90 %
bound at that horizon; past two minutes it grows with the square root of the time; with no score
yet, 5 °C at two minutes), the scope's projection at 1 and 2 min beside it, and its **reason**:
the plan, how many roasts the model came from and the band at 2 min ("Light espresso's setpoints
(8 s ahead), fan held at 79 %: a model from 12 roasts, ± 3.1 °C at 2 min (90 %)"). When the plan
takes the heater to 100 % within two minutes it says when, and how far BT then falls behind the
profile.

**First crack**: when BT reaches the BT first crack came at on this roaster, from the most
specific history there is: this bean on this profile (1 roast or more), else this profile (2
or more), else this bean, else every roast here; the range from its spread (at least ± 0.5 °C)
and the band. With no first crack marked here yet, the profile's own (its BT at its first
crack time, ± 2 °C). None once first crack is marked, or when BT is already past it.

**DROP**: the profile's auto DROP when it has one (time, BT, or development after first crack),
else its development (drop minus first crack time, after the marked or predicted first crack),
else its drop BT; with BT then.

## Kept and scored

Every 15 s the prediction is kept with the roast (`roast_predictions`: BT now, the model's and
the projection's BT at +60 and +120 s, first crack and DROP), and the BT that came is filled in
when its time comes (not across DROP). `GET /api/sessions/{id}/prediction` scores a roast: RMS,
90 % and bias at 60 and 120 s for the model and the projection, whether the model beat it, and
first crack and DROP as predicted about two minutes before against when they came. The roaster's
model view pools its last 10 roasts.

**Beside today's projections until it beats them**: the scope keeps its dashed `BT →`
projection; the model is its own series, **BT model**, off by default, switched on per browser
in the scope's series toggles. With it on, the scope's readout shows the model's line instead
of the projection's (BT in 1 and 2 min with the band, first crack and DROP with their ranges,
the heater's limit), its reason on hover. When the roaster's Model panel shows it beating the
projection on real roasts, the owner can leave it on.

## RoR warnings

From DRY end (or BT 160 °C, `armed`) to DROP every prediction carries an RoR watch
(`prediction.ror`): the RoR now (`now`, BT's slope over the last 30 s: the watch's own filter,
so the RoR smoothing setting does not move it), its `trend` (its slope over the last minute,
°C/min a minute), the model's RoR in 1 and 2 min under the plan (`at_60`, `at_120`), and a
warning of each kind there is:

| kind | it is | seen when | expected when |
|---|---|---|---|
| `crash` | the RoR falls 8 °C/min or more within a minute, and at least half of it | the highest RoR of the last minute is that far above now | the model's RoR does that in the next two minutes |
| `flick` | after first crack (marked, else BT at first crack's BT) the RoR rises 2 °C/min above its lowest since, for 20 s | the last 20 s all that far above the low before them | the model's RoR does that |
| `stall` | the RoR at 1 °C/min or less for 30 s before DROP | the last 30 s all there | the model's RoR does that while the plan still wants BT to rise (the profile climbing 2 °C/min or more, or BT 3 °C under it): a setpoint reached and held is no stall |

The thresholds are Roastwarden's own, close to the ones the roast page's RoR notes use after
a roast (`ror.ts`, [`roast-guidance.md`](roast-guidance.md)) but not equal, since a warning
looks ahead and a note judges what happened:

| | warning, live (`ror_watch.rs`) | note, after the roast (`ror.ts`) |
|---|---|---|
| crash | 8 °C/min and at least half the RoR, within any minute from DRY end | 8 °C/min within first crack ± 30 s |
| flick | 2 °C/min above its low since first crack, for 20 s | the same |
| stall | 1 °C/min or less for 30 s, while the plan wants BT to rise | 0.5 °C/min or less for 30 s after first crack |

A crash warning takes at least half the RoR, since just after DRY end the RoR still falls fast
on its own (24 to 14 °C/min in a minute on the fluid bed). A stall warns at 1 °C/min, a little
before the RoR is flat, so there is time to act; the note calls a stall only at 0.5 °C/min.
Replayed on eight recorded roasts at these limits, no false crash or stall (below). Nothing is
warned before DROP's time or BT, nor while BT falls faster than 5 °C/min (the beans leaving, or
a cut: plain to see). The model's part reads only its own line (from 30 s ahead, where its 30 s
slope is all the model's), so a mismatch between the model and the samples now is not read as a
change.

Each warning has `t` (when it starts, s since CHARGE), `seen`, `since` (the first prediction
that gave it) and a `reason`: what the RoR does, and what in the plan does it: the heater
reaching 100 % (and where BT settles at this fan, `settles`), the heater or fan moving, BT
passing first crack's BT (the model learnt the roaster's dip there), the profile's own RoR
rising or falling, BT behind the profile with the PID catching up. "BT stall ~9:05: the model
expects RoR under 1 °C/min from 9:05 at BT 182.6 °C, short of DROP at 210 °C: the heater
reaches 100 % at 8:40; at heater 100 % and fan 85 % BT settles near 205 °C".

**Alarms**: `crash_ahead`, `flick_ahead`, `stall_ahead` ([`alarms.md`](alarms.md)) fire on a
warning that two predictions in a row gave, expected within the rule's seconds. **Kept**: the
first time each kind held in a roast, what it said, and the last time it stood
(`roast_ror_warnings`, migration 056). **Scored**: `GET /api/sessions/{id}/prediction` lists
them (`ror_warnings`) against what the roast's RoR did, read afterwards with the "seen" rules
every 5 s from CHARGE + 30 s to 10 s before DROP (`ror_events`): when it came (`came_t`) and how
long ahead the warning was (`lead_s`); a kind that came with no warning is in `ror_events`
alone.

**On recorded roasts** (eight, the ones whose firmware reported its mode and gains, replayed
every 5 s under their recorded setpoints): no crash and no stall but on the roast a lease bug
cut (BT fell 28 °C in 20 s: a crash and then a flick, both seen); on six of the other seven the
RoR rose 2–4.6 °C/min after first crack (a dip at first crack, 8.4 → 5.8 °C/min on one, then
the PID catching up with a profile whose own RoR rises there), foreseen 65 s ahead on two ("BT
passes first crack's 195.2 °C") and seen on the others. Replay a database with the
ignored test, as in "Checking a fit":

```bash
ROASTWARDEN_PREDICT_DIR=dir nix develop -c cargo test -p roastwarden-server --lib ror_watch::tests::watch_exported -- --ignored --nocapture
```

## Where it shows

- **Roast page scope**: the BT model series (a solid line with its band) and the readout,
  with the RoR warnings ("BT stall ~9:05", the reason on hover).
- **Settings → Alarms**: RoR crash, flick and stall ahead.
- **Roasters → a roaster → Overview → Model**: when it was fitted and on how many roasts, the
  two-minute error per mode, where BT settles, the live score on recent roasts with their RoR
  warnings against what came, Refit.
- **CLI**: `roast roaster model <roaster> [--fit]`, `roast live` (the prediction and why, the
  RoR now and ahead and its warnings), `roast review <roast>` (the roast's score with its RoR
  warnings, `prediction` in JSON).
- **API**: `GET /api/roaster/{id}/model`, `POST /api/roaster/{id}/model/fit`,
  `GET /api/roaster/{id}/prediction`, `GET /api/sessions/{id}/prediction`.

## Limits

- Batch size is not in the model: the roasts it was fitted on are 93–102 g. A much larger charge
  heats slower than predicted until roasts of that size are in the fit.
- Fan assist's moves are not planned: the fan holds where it is.
- A heater cut, an e-stop or a person's hand on the heater are not foreseen.
- Above the BT the roasts reached (about 215 °C here) `g` is the prior's, smoothed in.
- Room temperature and the beans' moisture are not inputs.
- The roaster body's heat is not a state. A second, slow lag (tried and not
  built) fits the normal roasts better the slower it is, past 6 min: the body warming over a
  session, so a roast's start depends on the preheat and the roast before it. It moves where
  BT settles by up to 15 °C (200 °C at 100/100 %, 235 at 100/80), so `settles` is uncertain
  by about that much; modelling it needs the body's state at CHARGE.

## Checking a fit

Export a database's roasts to CSV and fit them with the ignored test (prints the score):

```bash
sqlite3 -csv roastwarden.db "select s.batch_number, t.elapsed_seconds, t.bean_temp, t.env_temp, t.setpoint,
  coalesce(json_extract(t.extras,'$.heaterApplied'), t.heater), t.fan, json_extract(t.extras,'$.controlMode'),
  t.rate_of_rise, json_extract(t.extras,'$.Kp'), json_extract(t.extras,'$.Ki'), ''
  from session_telemetry t join roast_sessions s on s.id=t.session_id order by s.batch_number, t.elapsed_seconds" > dir/tele.csv
sqlite3 -csv roastwarden.db "select s.batch_number, e.event_type, e.elapsed_seconds, e.temperature
  from roast_events e join roast_sessions s on s.id=e.session_id order by s.batch_number, e.elapsed_seconds" > dir/events.csv
ROASTWARDEN_PREDICT_DIR=dir nix develop -c cargo test -p roastwarden-server --lib predict::model::tests::fit_exported -- --ignored --nocapture
```

## Later

The same fit is the base for model feedforward (the heater a profile's next minute needs) and
lambda tuning of the PI from the model, and for the late-roast headroom question: where BT
settles says how much heater and fan room is left.
