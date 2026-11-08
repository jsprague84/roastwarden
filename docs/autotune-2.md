# Autotune 2: identify a model, then design the PI

Autotune measures how your roaster responds to its heater, then suggests PID gains: the
numbers the roaster's temperature control (auto mode, or a profile driving BT) uses to chase
a setpoint. Good gains follow a profile closely without the heater hunting. The server runs
the test, so it works on any roaster that takes a heater % and reports BT, the virtual
roaster included.

## When to run one

- Once for each fan speed you roast at, near the bean temperature that fan is used at in a
  roast (below first crack). Two or three tests cover a roast; the
  [gain schedule](#using-the-result) fills in between.
- When the roaster changes: a new heater, sensor, or firmware that reads the sensor
  differently.
- When BT overshoots, lags behind a profile, or the heater swings in big jumps.

## Running a test

Roasters → the roaster → Autotune. It needs the Tuning module (Settings → Modules) and a
roaster with control set to full ([roaster setup](roaster-setup.md)). No roast or profile
drive may be running on it.

1. Load beans as for a roast (old or junk beans are fine).
2. Set **Test at bean temperature** and **Fan**. The fan is set before the test starts and
   stays there; it cannot go below the roaster's fan floor.
3. Click **Start autotune**.

The test goes through three phases, shown with a live chart:

| Phase | What happens |
|---|---|
| Heating to the test level | full heater until BT reaches the level. If the roaster cannot get there at this fan, the test runs where BT levels off, and says so |
| Finding the holding power | the heater switches above and below the level until it finds the heater % that holds it |
| Measuring the response | the heater swings unevenly around that power, while every sample is recorded |

It takes about 5 minutes after the heat-up. The roaster stays in manual under a control lease:
if the server goes quiet, the roaster cuts its own heater. The test stops by itself if BT goes
20 °C over the level, a phase stalls, the roaster stops sending, a roast or a drive starts, or
it runs 45 minutes. **Stop the test** ends it at any time. At the end the heater goes off and
the fan stays where it is, so the beans cool as after a roast. You can close the page: the test
carries on, and opening the page again picks it up.

**Advanced** holds Heat-up power, Largest heater swing, Asymmetry and the switching band. The
defaults suit most roasters.

## Reading the result

The result opens with a quality badge, **Good**, **Acceptable** or **Poor**, and any
warnings. A poor result cannot be applied. The common warnings:

- **The holding power moved during the test**: the roaster was still warming up, or the beans
  were drying. Start the test with the roaster warm.
- **The best model leaves the sensor's noise unexplained**: the roaster changed during the
  test. Run it again warm.
- **The process could not reach the level asked**: the heater could not get BT there at this
  fan, so the test ran where BT levelled off. Ask for a lower level, or test with less fan.

Then a sentence on what the roaster acts like near the test level, and a chart of the
measured BT against the model: where the dashed model follows BT, the designs can be trusted.

Below it you pick a design:

| Field | What it means |
|---|---|
| Rule | SIMC PI (recommended), AMIGO PI (conservative), and two classic rules for comparison |
| Tight ↔ Smooth | how fast the loop is asked to be. Tight follows the profile closest and moves the heater most; smooth is calmer and copes better with more beans or another fan. 1.5 is a good start |
| Kp, Ki | the gains. The test designs PI only (no D term) |
| Ms | the safety margin: 1.2–1.4 robust, up to 1.8 balanced. Above 1.8 it warns; above 2.0 it cannot be applied |
| Margins | gain margin, phase margin and delay margin |
| Heater jitter | how much the heater moves from the sensor's noise alone |
| Ramp lag | how far BT trails a 10 °C/min ramp. A profile drive's lookahead closes most of it |
| If the roaster changes | Ms if the roaster's gain or its dead time doubles |

**All designs** lists every rule side by side.

## Using the result

- **Add to schedule at N % fan** adds the gains as a row of the gain schedule (Roasters → the
  roaster → Control → Gain schedule by fan). With the schedule on, the roaster's gains follow
  the fan: yours, a profile's or [fan assist](fan-assist.md)'s, blended between rows.
- **Store on the roaster** sends the gains to the roaster as its stored gains, used when no
  schedule is on.

Tuning history, under the test, lists every run and every change to the stored gains, and
**Undo last change** restores the gains before it.

From the command line, `roast roaster tuning <roaster>` shows the gains, the schedule and the
runs; `--run <id>` reads one run's result in words, and `--tau-c` or `--rule` preview another
design. Starting a test has no command of its own: it drives the heater, so you start it
from the dashboard.

<!-- site: end -->

## Design notes

For developers: how the server's autotune works and why. The site publishes only the part above.

It is made of the crate, the server's test, the dashboard, the CLI and firmware
3.4.0. The method draws on Berner/Hägglund/Åström's asymmetric relay, Skogestad's SIMC, Åström &
Hägglund's AMIGO and robustness measures, and what ABB, Emerson, Honeywell and Siemens tuners
do. It replaces the firmware's former relay test, which was the "onboard" option until firmware
3.5.0 removed it.

It is meant to be robust and reusable for similar processes (ovens, kilns, other heaters), so
the method lives in its own crate, `crates/autotune` (`roastwarden-autotune`), which knows
nothing about coffee: an input in % (the actuator), an output (the measurement), seconds.

## Why

What the firmware's former relay test did (firmware 3.2): a symmetric relay with bias
adaptation, Ku from the describing function with a hysteresis correction, the peaks and
valleys of each cycle, Tu from the switch times, then Ziegler–Nichols or Tyreus–Luyben rules.
On this roaster:

- **The roaster is lag-dominant.** Holding 165–200 °C takes 76–88 % heater; Ku ≈ 23–37
  (0–255 per °C) and Tu ≈ 29–38 s give a time constant around 160 s against a dead time near
  8 s (normalised dead time τ = L/(L+T) ≈ 0.04). Åström & Hägglund: Ku/Tu rules are "not
  appropriate for lag-dominant processes" (Z–N PI comes out at Ms ≈ 2.6). Tyreus–Luyben PI
  happens to land near the right answer here, which is why the current PI rows work.
- **The hysteresis biases Ku and Tu.** A relay with a band ε locks onto the phase
  −180° + arcsin(ε/a), not −180°: with a 1 °C band on a 2–3 °C oscillation Tu reads 15–35 %
  long and Ku low by as much. The √(a² − ε²) correction changes Ku by 2–9 % and leaves the
  frequency wrong.
- **Peaks on a 0.25 °C sensor are coarse**, and the expected maximum of noisy samples is
  biased upward (about +0.6 °C at σ 0.25 over a peak).
- **The D term is useless on this sensor** (each 0.25 °C step became a 7–15 % heater jump;
  "Checking a schedule"). The schedule runs PI.
- **Only the ESP32 firmware can autotune**: a TC4 or Modbus roaster cannot, and the virtual
  roaster ignores the command, so nothing could be tried off the real roaster.

## Design in one paragraph

The **server runs the experiment** (like Profile drive: manual heater %, a control lease,
the fan set first), so any roaster that takes a heater % and reports BT can be tuned, the
virtual roaster included. The experiment is an approach, a symmetric relay that finds the
holding power, then an **asymmetric relay** (the larger swing downward, where a heat-limited
roaster has room). Every sample is kept with the run. The analysis **fits models** to the
record (integrator plus dead time, first and second order plus dead time, with offset and
drift) and checks them three ways (the fit, the relay's first harmonic, Berner's
half-period ratio). The PI comes from **SIMC** on the chosen model, with one knob from tight to
smooth, and every option shows its **robustness** (Ms, margins, what it does if the gain or
the dead time doubles) and its **cost** (heater jitter from the sensor's noise, lag behind a
ramp, load recovery). The gain schedule keeps the **model** per fan and designs the gains from
it. Firmware 3.4.0 reads the thermocouples four times a second and averages.

## The crate (`crates/autotune`)

| Module | What it does |
|---|---|
| `model` | `Model { k, t1, t2, l }` (first or second order plus dead time; `t1 = ∞` is an integrator), frequency response, step response, a simulator on a recorded piecewise-constant input |
| `relay` | the experiment as a state machine: one sample in, the input to apply out |
| `harmonic` | least-squares first and second harmonics of u and y over whole periods (with offset and trend) → frequency-response points; Berner's τ from the half-period ratio |
| `fit` | output-error fits by variable projection (gain, offset, drift and starting state by least squares; lags and dead time searched), the profile interval of `t1`, AIC, a hold-out check |
| `design` | PI rules (SIMC, AMIGO, integrating lambda, and Ku/Tu rules for comparison), gates |
| `robust` | Ms, Mt, gain and phase margins, delay margin, noise gain, the robustness box |
| `analysis` | record + experiment summary → the report (models, checks, designs, warnings) |
| `bench` | a simulated process with a real sensor's faults (steps, noise, oversampling, drift, actuator limits) for tests and for trying settings offline |

No dependencies beyond `serde`. Units are the caller's; Roastwarden converts PI gains to the
firmware's 0–255 units (× 2.55) at the edge.

## The experiment

| Phase | What happens | Ends |
|---|---|---|
| Approach | `approach_power` (100 %) | the output crosses the level, or levels off below it (test where 85 % of the rise holds: a heat-limited roaster), or 15 min |
| Noise | measured over the approach's last 60 samples: second differences, detrended by a moving median, outliers dropped | (part of the approach) |
| Symmetric relay | `bias ± d` with a band `h`; after each clean cycle the bias moves towards equal halves, `d` = the room there is (≤ `max_swing`) | 3 cycles agree (period within 15 %, halves within 15 %), at least 3, at most 10 |
| Asymmetric relay | bias held; the larger swing `γ = 2`× the smaller, on the side with more room; amplitude adapted between `2h` and `12h` peak deviation | 3 cycles, or 2 whose periods agree within 5 % |

- **Band**: `h = max(4σ, 2 × the signal's step, min_hysteresis)`: Berner uses twice the
  noise's half-range (≈ 4–5σ) or 3× the noise level; two steps so one toggle cannot switch.
- **Why the symmetric phase**: the asymmetric relay needs the holding power `u0`, and there
  are no trusted gains to hold the level with first. The symmetric phase finds `u0` without
  any, and its cycles are a second frequency point. The bias is not adapted in the
  asymmetric phase (it would undo the asymmetry, the research's warning about Hang–Åström–Ho).
- **Safety** (the server's, plus the roaster's own interlocks): output more than
  `max_overshoot` (20 °C) above the level fails; a half stuck for `stall_s` nudges the bias,
  and fails after `max_half_s` at full swing; the whole test is capped (45 min); stale
  telemetry (8 s) stops renewing the lease and the roaster goes to its safe state; a roast
  or a drive starting on the roaster stops the test; an operator stop. At the end the heater
  goes to 0 and the fan stays where it is (the roaster cools as after a roast). Starting a
  test ends the last roast's cooling (`SequenceService::end_cooling`): left running, its
  fan-off landed inside the test and the fan interlock kept the heater off for the whole
  approach (found by a test started right after a DROP).
- **The input recorded is the heater the roaster applied** (`heaterApplied`), timed by the
  roaster's clock, so the identified dead time is the process's, not the network's.
- Running it on `esp32_roaster_01` is a control action: the owner starts it from the dashboard;
  a token gets `REAL_HARDWARE` (`docs/agents.md`), as for Profile drive.

## The analysis

1. **Windows**: the relay part of the record (from the first switch), and its converged
   periods.
2. **Harmonics**: least squares of u and y on `[1, t, cos ωt, sin ωt, cos 2ωt, sin 2ωt]`
   over whole periods → `G(iω0)` (both phases) and `G(i2ω0)` (asymmetric). No describing
   function, no peaks, offset and linear drift removed. The amplitude's error is about
   σ·√(2/N): under 1 % on this roaster against 5–10 % from peaks.
3. **Berner's τ** from the asymmetric half-periods: `τ = (γ − ρ)/((γ − 1)(0.35ρ + 0.65))`
   (ρ the half-period ratio, γ the applied asymmetry): a classification cross-check.
4. **Fits** of three structures to the relay window: integrator plus dead time (ITD, one
   searched parameter), first order plus dead time (FOTD, two), second order (SOTD, three),
   each with offset, drift and starting state solved linearly. Reported per fit: parameters,
   fit %, residual RMS against the measured noise (near 1 means only noise is left), AIC, the
   95 % profile interval of the main lag, and a hold-out check (fit without the last cycle,
   predict it; the main lag stays at the full fit's, since a shorter window trades a long lag
   against the drift term).
5. **Choice**: the lowest AIC among fits whose residual is within 2× the noise; then for
   design, **ITD** when τ < 0.1 or the lag is not identified (its `k` and `T` cannot be told
   apart, only `kv = k/T`), else the chosen model (SOTD → FOTD by Skogestad's half rule for a PI).
6. **Quality**: good / acceptable / poor from the fit, the residual against the noise, the
   agreement between the fit's and the harmonics' `G(iω0)` (within 10 % and 10°), the
   hold-out, the bias drift during the symmetric phase (the process changing under the test,
   like beans drying) and whether the experiment converged. Poor runs cannot be applied.

## The design

| Rule | Model | Formula | Character |
|---|---|---|---|
| **SIMC PI** (default) | FOTD | `Kc = T/(k(τc + θ))`, `Ti = min(T, 4(τc + θ))` | the knob `τc` from θ (tight, Ms ≈ 1.6–1.7) to 3θ (smooth, Ms ≈ 1.25); default 1.5θ (Ms ≈ 1.5) |
| | ITD | `Kc = 1/(kv(τc + θ))`, `Ti = 4(τc + θ)` | |
| AMIGO PI | FOTD | `K = 0.15/k + (0.35 − LT/(L+T)²)·T/(kL)`, `Ti = 0.35L + 13LT²/(T² + 12LT + 7L²)` | conservative cross-check, Ms ≈ 1.4 |
| | ITD | `K = 0.35/(kv L)`, `Ti = 13.4 L` | |
| Ku/Tu rules (Z–N PI, Tyreus–Luyben PI) | Ku, Tu of the fitted model at −180° | as before | for comparison; flagged on a lag-dominant process |

Each option shows, from the identified model with the controller's 1 s sampling (half a
sample of extra delay):

- **Ms, Mt, gain margin, phase margin, delay margin** (Ms ≤ 1.6 target; above 1.8 warns;
  above 2.0 cannot be applied).
- **The robustness box**: Ms if the gain doubles or halves, or the dead time doubles.
- **Heater jitter**: `Kc × σ` (the sensor's measured noise) and the heater step one sensor
  step causes.
- **Load recovery**: `1/ki` (integrated error per % of heater lost).
- **Ramp lag**: how far BT trails a 10 °C/min ramp with no lead, `R/(k·ki)` (FOTD; P on error,
  as the firmware's PID). The drive's adaptive lookahead closes this; it says how much there
  is to close.

The gains go to the roaster or to the gain schedule in device units (× 2.55), Kd 0.

## Gain schedule

Gains depend on airflow: a fluid bed roasts at 100 % fan through drying, then lower, and the
roaster that held BT well at one fan hunts or lags at another. The firmware accepts
`control/pid` with `"persist": false`, so the server can switch gains during a roast without
wearing the roaster's flash.

- The setting `gain_schedule:<device_id>` holds `{"enabled": true, "rows": [{"fan_pct": 100,
  "kp": …, "ki": …, "kd": …, "ff": 62, "run_id": "…"}]}` in device PID units (0..255 output
  per °C). `ff` is the run's holding power at that fan (the relay's bias), kept for feedforward
  use, not sent. `GET`/`PUT /api/roaster/{id}/gain_schedule`.
- A row added from a run names the run and its rule; a row whose gains are then typed over by
  hand keeps the run and its holding power but is marked "edited" (the server checks the gains
  against the rule on every save). An emptied Kd box saves as 0; an empty Kp or Ki stops the
  save with a message.
- A row added from an Autotune 2 run also keeps its **model** (`kv`, `L`, and `k`, `T` when
  identified), the rule and `τc`. Between rows the schedule interpolates the model (`kv` and
  `T` log-linearly, `L` linearly) and designs the gains for the fan from it; rows without a
  model blend their gains linearly (`gains_at(fan_pct)`: sorted by fan, held flat beyond the
  first and last row; pure, unit tested). The advice checks Ms at the midpoint between rows
  with each neighbour's model as the true process.
- The scheduler (`crates/server/src/control/schedule.rs`) sends `control/pid` with `persist:
  false` when the fan changes through the API or a drive and the scheduled gains differ from
  the last ones it sent by more than 1 %; a drive's start applies the gains for the current
  fan. When the drive runs the BT loop on the server, the schedule sets the server PID's gains
  instead (device gains / 2.55, since the server PID works in heater %).
- The scheduler follows the **commanded** fan (API or drive), not the reported one: telemetry
  lags a command by a sample, and following it made the gains flip back and forth after a
  manual fan change during a drive. The reported fan is used only when nothing was commanded
  since the server started.
- Scheduled gains live in the roaster's RAM until it reboots or gains are stored again; its
  flash keeps the stored set. Nothing tells the server of a reboot, so it resends the
  scheduled gains when what it last sent is over 30 s old (they go to RAM, so a resend costs
  nothing); without that, a roaster power-cycled overnight ran the next roast's drying on its
  stored gains until the fan first moved.

## Stored gains and undo

- Two tables: `autotune_runs` (one row per run: fan %, test and target temperature, Ku, Tu,
  bias, amplitude, cycles, quality, warnings, the raw payload; the server's own runs with
  `source: "server"` and their analysis) and `pid_changes` (every stored gain change with the
  gains it replaced and its source: manual, an autotune run, undo).
- `crates/server/src/control/tuning.rs` computes every rule's gains from Ku and Tu and clamps
  them as the firmware does (Kp 0.1–100, Ki 0–10, Kd 0–200), so the dashboard can show the
  gains of every rule beside the current ones.
- `GET /api/roaster/{id}/autotune/runs` (the history, with gains for every rule),
  `POST /api/roaster/{id}/autotune/runs/{run_id}/apply` `{"rule": "…", "to": "device" |
  "schedule"}` (refused for failed or poor runs), `GET /api/roaster/{id}/control/pid/history`,
  `POST /api/roaster/{id}/control/pid/undo` (the gains before the last stored change). Stored
  changes made through `control/pid` are recorded too.
- A stored change records the gains it replaced as the last recorded change's set, else the
  gains the telemetry shows: the telemetry shows the gain schedule's RAM gains while the
  schedule runs, so preferring it would let undo store those.
- Results a roaster publishes retained arrive again at every reconnect: a results message
  identical to the last stored run is ignored.
- An autotune does not start while a roast is on the roaster (409: the test switches it to
  manual and runs its own relay), nor at a fan below 45 % (the firmware refuses 40 % and less).

## Checking a schedule

`GET /api/roaster/{id}/gain_schedule` returns `advice`, in words, shown under the schedule
editor and in `roast roaster tuning`:

- **Rows with a D term.** The firmware takes D on the raw MAX6675 reading (0.25 °C steps,
  1 Hz) with no filter, so each step becomes a heater jump of Kd/2.55 % per °C/s. On a recorded
  roast with Kd 61–80 the heater's second-to-second change followed BT's second difference with
  a slope of −28.5 %/°C (r = −0.98): 14 % a second of jitter, and the swings clip at 100 %, which
  takes power away near saturation and feeds noise into what fan assist watches. The fix needs
  no firmware change: add each run to the schedule again with a PI rule (Kd 0). A filtered D in
  the firmware is the alternative.
- **Neighbouring rows that disagree.** Rows within 10 % of fan whose Kp differs by more than
  20 %: two rows at 71 % and 75 % (Kp 16.8 and 13.3) sat where fan assist spends development,
  so each 1 % of fan moved Kp about 5 %. Keep one, or autotune again with beans.

Rows tuned at different temperatures (165, 190 and 200 °C) make "gains by fan" really gains by
operating point; that holds because fan assist lowers the fan as BT rises. A roast at a fixed
fan to a high BT (filter at 100 % to 210 °C) would use a low-temperature row's gains at
210 °C. Scheduling on BT as well as fan is the next step if that shows.

## Where it shows

- **API**: `POST /api/roaster/{id}/tune/start` (level, fan, options), `/tune/stop`,
  `GET /api/roaster/{id}/tune` (live state); runs stay in `/autotune/runs` with `source:
  "server"`, the analysis and (`/samples`) the record; `GET …/runs/{run}/design?rule=simc_pi&tau_c=1.5`
  previews a design; `…/apply` takes `tau_c`.
- **Dashboard**: Roasters → the roaster → Autotune: the server autotune (the onboard test
  went with firmware 3.5.0), its live chart and phase, then the result:
  the model, the record against the model, the options with their figures, the tight ↔ smooth
  knob, Apply or Add to schedule.
- **CLI**: `roast roaster tuning <roaster> --run <id>` reads an analysis; `roast roaster
  tuning design <run> --tau-c 2` previews a design (read-only). Starting a test is control:
  on `virtual_roaster` with `roast api`, never on real hardware from a token.

## Firmware 3.4.0

Both thermocouples are read every 250 ms (a MAX6675 conversion takes at most 220 ms), each
read goes through the spike filter, and four are averaged into the 1 Hz sample. The chip's
own noise (about one step) dithers, so the mean resolves finer than 0.25 °C and has about
half the noise: the PID, RoR, the experiment and the fit all gain. Temperatures go out with
two decimals; `caps` adds `oversample`. The averaging adds about 0.4 s of delay, which the
identified dead time includes: re-identify after changing the signal chain.

## Testing

- The crate's tests run the whole chain on the bench: lag-dominant, balanced and
  second-order processes, 0.25 °C steps with and without oversampling, a drifting process, a
  heat-limited actuator, a level the process cannot reach; the identified model and the
  design's Ms on the true process are checked against known answers.
- The server runs it end to end on the virtual roaster (its heat model was fitted to this
  roaster's autotunes).
- On the roaster, by the owner: one test per fan node with beans (100 % fan near 160 °C,
  ~72 % fan near 190 °C), then a roast on the designed schedule.

## First run (virtual roaster)

190 °C at 72 % fan, beans loaded, through the real service and MQTT: about 6 minutes, quality
good, no warnings. The second-order fit left only the sensor's noise (residual 0.99× σ) and
found kv = 0.0366 °C/s per % against the simulator's true 0.0367; holding power 74.8 % (true
about 75.5 %); the model matched the oscillation's own harmonics within 4 % and 3°. Designed
on an integrator (τ 0.06): SIMC at 1.5θ gives Kp 5.80 / Ki 0.114 (roaster units), Ms 1.50,
1.4 °C behind a 10 °C/min ramp; Z–N PI Ms 2.83 (refused); Tyreus–Luyben PI Ms 1.67. After the
test the heater and heater power were off.

What the run found, fixed: the switching band used the smallest step between samples as the
sensor's step (during the climb that is never one step), so the band was 1.12 °C instead of
about 0.5; the signal's resolution now comes from the values themselves.

## First real run (`esp32_roaster_01`)

160 °C at 100 % fan, firmware 3.4.0, run `1299f564`: about 7½ minutes (2:23 approach), quality
good, no warnings. The first symmetric half started from a 65 % bias and stalled (the holding
power is 83 %); the bias nudge and the symmetric relay found it, as designed.

- **Model**: the second-order fit left only the sensor's noise (residual 1.01× σ, hold-out
  RMSE 0.18 °C against σ 0.185 °C): k 1.46 °C/%, dead time 0.66 s, lags 42 s and 5.2 s. It
  matched the oscillation's own harmonics within 5 % and 0.2°. Half rule: θ 3.25 s, T 45 s,
  τ 0.07, so designed on the integrator, kv 0.0325 °C/s per % (virtual roaster: 0.0366).
- **Heat limit**: holding 83 % at 160 °C leaves 17 % above it; the model's steady state at
  100 % heater is about 184 °C at this fan. The relay's high side was never more than 17 %.
- **Noise** σ 0.185 °C at 100 % fan (the bench's 0.13 °C was without airflow); heater jitter
  ±0.6 % at the default design.
- **Berner's τ** 0.39 against the fit's 0.07: the known sensitivity to the holding power
  (relay 83.4 %, fit 85.5 %; see "Limits"). The fit and the harmonics agree, so the fit stands.
- **Drift** −0.9 %/min in the symmetric phase (below the 1.5 %/min flag): the chamber still
  warming; the fit's drift term absorbs it.
- **Against the firmware's relay** (10-22, 165 °C, 100 % fan): it read Tu 36 s; the model's
  ultimate period is 15 s. A 1 °C band on a ~2 °C oscillation with single 0.25 °C reads
  stretched the period, so the Tyreus–Luyben PI row from it (Kp 7.29, Ki 0.091, Ti 80 s) is
  safe (Ms 1.37 on this model) but slow: 3.2 °C behind a 10 °C/min ramp against 1.3 °C for
  SIMC at 1.5θ (Kp 8.36, Ki 0.223, Ti 37.5 s, Ms 1.50).

### At 72 % fan, 190 °C (runs `493b8160` and `2179feb6`)

Two runs ten minutes apart, the first from a cold roaster (BT 32 °C at the start), the second
from a warm one (58 °C). They agree: kv 0.0288 and 0.0279 °C/s per %, dead time 6.4 and 6.1 s,
holding power 75.4 and 74.6 %, the second-order fits k 1.65 / 1.55 °C/%, lags 52 + 10 s and
51 + 9 s; τ 0.10 both, so designed on the integrator. SIMC at 1.5θ: Kp 5.16 / Ki 0.075 and
Kp 5.56 / Ki 0.085 (Ms 1.50, 3.2–3.4 °C behind a 10 °C/min ramp). Against 100 % fan: a slower
roaster (θ 6 s against 3.3 s) with a kv 13 % lower, so smaller gains.

The cold run was graded acceptable with three warnings, the warm one good with none:

- **drift** −1.7 %/min against −1.3 (the flag is 1.5): a cold roaster's body still takes heat
  during the test, so the holding power falls. Real; start a test warm.
- **poor_fit** residual 2.09× the noise against 1.87× (the line is 2.0): the same drift, which
  the fit's linear term only approximates. Real, and borderline.
- **holdout** 1.24 °C against 0.26: a false alarm. Fitted again on the shorter window, the
  main lag slid to 6000 s and traded against 1.6 °C/s of drift: it fitted the window and
  missed the last cycle. Fixed: the hold-out keeps the full fit's main lag (0.54 °C now, under
  the line; the warm run 0.23). The cold run's record is a test (`testdata/`).

## Limits found while building it

- **Drift.** The trend of each clean cycle's mean input (within a phase) reads within about
  ±0.5 %/min on a process that is not drifting, over a 4-minute test, so only a clear trend
  (above 1.5 %/min, a bean load drying fast) is flagged; smaller drift is absorbed by the fit's
  drift term.
- **Berner's τ** is sensitive to the holding power: a 1.6 % error moved it from 0.04 to 0.23
  on a lag-dominant bench process, so it is figured against the fit's holding power and used
  as a cross-check, not for the design.
- **A short test pins `kv` and the dead time**, not `k` and `T` apart, on a lag-dominant
  process; the fit's `t1` interval says when, and the design then uses the integrator.

## Later

- **Feedforward from the model**: `u_ff = ṙ/kv` streamed during a drive (the research:
  about 9 % heater at 10 °C/min), with the gain schedule's `kv(fan)`. What the adaptive
  lookahead approximates now.
- **Identification from roast logs** (no relay test), now that each sample keeps the applied
  heater (migration 037).
- A step test after the relay (ABB, Siemens) to pin `k` and `T` on a lag-dominant process,
  if a use needs them (feedback does not).
