# Roaster setup: what kind of roaster, and whether Roastwarden controls it

Each roaster has a setup: what kind of roaster it is, what Roastwarden may send it, which
outputs it has and the range of each, and a fan floor. A new roaster starts at control
**off**: Roastwarden records it and sends it nothing, not even STOP, until you choose
otherwise. Every command, whoever sends it (you on the Roast page, the roast sequence at
CHARGE, an alarm action, Profile drive, fan assist, autotune, an agent, Artisan as a screen),
passes one gate in the server that applies the setup. So a gas drum added for logging never
gets a fluid bed's "fan 100 %, heater 60 %" at its first CHARGE.

## Using it

- **Where**: Roasters → the roaster → Control → **Setup**. Add roaster asks for the kind in its
  first step and the control level in its last.
- **Who**: only the owner key changes it. An agent with an admin token may set up a roaster at
  control off; switching control on from a token waits for your approval.
- **Raising control asks once**: "Roastwarden will send commands to …: the roast sequence at
  CHARGE, alarm actions, Profile drive. Check its own safety cutoffs first." Raise it only when
  the roaster has cutoffs of its own ([connect your roaster](connect-your-roaster.md)), and run
  the write check once, cold (Roasters → the roaster → Check).
- The simulated roaster `virtual_roaster` always has full control.

### Kind

| Kind | Outputs | Fan floor | Roast sequence |
|---|---|---|---|
| Fluid bed, electric | Heater, Fan, 0–100 % | 45 % | CHARGE: fan 100 %, heater 60 %, Profile drive starts; DROP: fan 100 % until BT is 45 °C, then off |
| Drum, electric | Heater, Fan, 0–100 % | none | CHARGE and DROP only record |
| Drum, gas | Burner, Air, 0–100 % | none | CHARGE and DROP only record |
| Logger only (reads temperatures) | none | none | CHARGE and DROP only record |
| Other | Heater, Fan, 0–100 % | none | CHARGE and DROP only record |

A kind is a starting point, not a driver. Changing it sets the outputs, the fan floor, the
roast sequence and fan assist to the new kind's defaults (it asks first); declared channels
stay.

### Control level

| Level | Roastwarden sends | What works |
|---|---|---|
| Off · logs only | nothing at all, not even STOP | recording, the scope, RoR, events, alarms that notify, profiles as a background, history, guidance, reports, labels |
| Manual | heater %, fan %, drum % and damper % when declared, heater on/off, STOP | the above, plus the sliders, STOP, alarm actions of those kinds, the roast sequence's fan and heater steps |
| Full | everything: also setpoint, mode, PID gains | everything, plus Profile drive, the server's PID loop, autotune, the gain schedule, fan assist |

With control off, the Roast page shows "Control is off for this roaster: Roastwarden only
records it. Use the roaster's own controls." where the controls were, the header has no STOP,
and Profile drive offers ghost only. A roaster whose own PID is in auto stays in auto when you
lower it to manual: switch its mode to manual first.

### Outputs and their limits

**Outputs → Edit** lists Heater, Fan, Drum and Damper, each with a Label, Min %, Max % and
Step %, then **Save outputs**.

- An output that is off is never sent and its slider is hidden; a command for it is refused
  ("… has no fan output").
- A value outside the range is sent as the nearest end; with a step it is rounded to the step.
  **0 is always off**, so a fan whose lowest speed is 30 % still switches off.
- Drum and damper get sliders on the Roast page once declared. Profile drive, alarm actions and
  the roast sequence do not set them yet.

### Fan floor

While the heater is on, the fan never goes below the floor (0: none). A fan set lower by hand
while heating, or switched off, is raised to it; the roast sequence, fan assist, Profile drive
and the autotune keep to it, and their fan settings must be at or above it. The heater counts
as on when the roaster's latest reading says so (heater power on, or a heater % above 0),
except for 5 s after Roastwarden sent heater off or STOP. A floor needs both a heater and a
fan.

### When the gate refuses

A command the setup does not allow is refused with a reason: "control is off for …:
Roastwarden only records it (Roasters → … → Control)", "control is manual for …: setpoint
needs full control", "… has no fan output". Features that run by themselves skip quietly
instead: the roast sequence only records, fan assist stands down, an alarm still notifies.

From the command line, `roast roaster show <roaster>` prints the setup (kind, control, outputs,
fan floor, channels) and `roast roasters` a control column. `roast roaster control <roaster>
off` lowers control at once; `manual` or `full` asks the owner for approval.

<!-- site: end -->

## Design notes

For developers: how the roaster setup and the gate work and why. The site publishes only the
part above.

The setup follows what similar apps do: in Artisan nothing is sent to a machine until the user
configures control for it, so a roaster added later does not inherit the fluid bed's CHARGE
sequence, while the ESP32 and the virtual roaster keep theirs.

## Why

Without the setup nothing between a feature and the wire would look at the roaster.
`TransportRegistry::send` (`transport.rs`) carries every command from every caller: the control
routes, the roast sequence at CHARGE and DROP, alarm actions, Profile drive, fan assist, the
gain schedule (which sends PID gains unasked on a roaster's first sample after a boot), the
server's autotune and the Modbus TCP slave. A roaster the server has merely heard of (MQTT
auto-discovery creates it `pending`) would get all of it, and an unknown device id would still
get MQTT. The token rule (`tokens_may_control`) applies to API tokens alone. A gas drum roaster
with a TC4 for logging would get "fan 100 %, heater 60 %, drive" at its first CHARGE.

The setup answers two questions: *whether* Roastwarden sends a roaster anything (the control
level), and *what* (its outputs, their ranges, the fan floor while it heats, and the defaults a
kind brings, so a drum roaster does not get the fluid bed's CHARGE sequence or its 45 % fan
floor).

## What it is

Each roaster has a **setup** (table `roaster_setup`, one row per `device_id`):

| Field | Values | Meaning |
|---|---|---|
| `kind` | `fluid_bed_electric`, `drum_electric`, `drum_gas`, `logger`, `other` | What the roaster is; each kind brings its defaults (below, "Kinds' defaults") |
| `control` | `off`, `manual`, `full` | What Roastwarden may send it (below) |
| `outputs` | `{heater, fan, drum, damper}`, each `{label, min, max, step}` or null | Which outputs it has and their ranges (below, "Outputs") |
| `fan_floor_pct` | 0–100, 0 = none | The fan's lowest % while the heater is on (below, "The interlock") |

**Control levels** (the tiers of the interface standard, published in [`hardware-integration-guide.md`](hardware-integration-guide.md), "The interface standard"):

| `control` | Roastwarden sends | Features that work |
|---|---|---|
| `off` (logs only) | nothing at all, not even STOP | Recording, the scope, RoR, events, alarms that notify, profiles as a background, history, guidance, reports, labels |
| `manual` | heater %, fan %, drum % and damper % when declared, heater on/off, emergency stop | The above + the sliders, STOP, alarm actions of those kinds, the roast sequence's fan and heater steps |
| `full` | everything (setpoint, mode, PID gains, feedforward, lease, RoR limit too) | Everything: Profile drive, the server PID, autotune, gain schedule, fan assist |

`off` refuses even emergency stop: "logs only" means Roastwarden writes nothing to the device, and
on a logger the command means nothing anyway. The Roast page says so where STOP and the
controls are ("Control is off for this roaster: Roastwarden only records it. Use the roaster's
own controls."), and the header's STOP is not shown, so nobody relies on a button that does
nothing.

A roaster lowered to `manual` while its own PID is in auto stays in auto: mode is a `full`
command. Switch it to manual mode (on the Roast page, or on the roaster) before lowering.

**Who gets what:**

- A roaster with no setup row is `off`. That covers a roaster found by auto-discovery, one added
  with Roasters → Add, and a device id the server has never seen. Its kind reads as `other`, so
  its roast sequence is off as well as its control.
- Migration 045 gives **every roaster that existed** (each `devices` row) `full` and
  `fluid_bed_electric`, so what they did before the setup existed is unchanged. No device id is
  written into the migration.
- `virtual_roaster` is always `full` and `fluid_bed_electric` in code, whatever its row says (it
  is the simulator for trying control; it may be switched on long after the migration).
- Control goes on only with the owner's yes: the owner key's `PUT`, or an admin token's held
  for the owner's approval (below, "Tokens and the setup"). An agent reads it (`roast roaster
  show`) and asks with `roast roaster control <roaster> manual`; the owner approves or denies.

### Tokens and the setup

An agent setting up a logger (`roast roaster add --kind logger`, `docs/add-roaster.md`) needs
to record its kind, outputs and channels; none of them sends anything while control is `off`.
So a token with the **admin** preset may `PUT` a setup at once when the body leaves control
`off`: setting up a new roaster, changing one at `off`, or **turning control off** (lowering is
always safe). A `PUT` that leaves control `manual` or `full` (switching it on, raising it, or
any change to a roaster that stays on) is **held for the owner's approval** (`control.md`,
"Approvals", action `roaster.control`): 202 with the request, "Set esp32_x's control to
manual", the link through the notification channels, and the change made as the token when
the owner approves. It is held for **every** token, also one that does not ask first (as a
label print is): switching control on is the owner's decision alone. Other presets get 403
`TOKEN_SCOPE`. The hold is in the auth layer (`approvals.rs`: `gated` names the `PUT`,
`raises_control` reads the body's level, so a body at `off` is not held); the route
(`routes/devices.rs`, `put_setup`) refuses a token's control-on `PUT` that comes without the
approval (403 `OWNER_ONLY`). The `PUT` stays exempt from the real-hardware switch (it sends
nothing); what a token may send once control is on still needs that switch. Tested in
`setup.rs` (`an_admin_token_switches_control_on_only_with_the_owners_approval`).

## The gate

One check at the top of `TransportRegistry::send`, before any sink. `src/roasters/setup.rs`
holds it: `Setups` is an in-memory map of the setups (loaded at start; the `PUT` writes the
table and the map under one lock, so `send` does no SQL), and `Limits::shape` is the whole rule,
pure and tested:

1. **Control level** (`Control::allows`): the level must allow the command, else
   `TransportError::ControlOff { device_id, control, command }`.
2. **The output** (`output_of`): fan % needs the fan, drum % the drum, damper % the damper;
   everything else but STOP and the lease needs the heater. An undeclared output refuses.
3. **Range and step**: the value is clamped and rounded (below, "Outputs").
4. **The floor**: a fan below the floor while the heater is on is raised (below, "The
   interlock").

`send` sends the shaped command. It decides "heater on" from the newest sample
(`heater_enable`, or heater output above 0) if it is under 30 s old, and counts the heater off
for 5 s after it sent heater off, heater 0 % or STOP.

The control routes, Profile drive's start and the autotune's start answer a refusal with
**409** and code `CONTROL_OFF` (upper case, as every code in `errors.rs`), message "control is
off for esp32_x: Roastwarden only records it (Roasters → esp32_x → Control)" or "control is
manual for esp32_x: setpoint needs full control (…)". An undeclared output is 409
`OUTPUT_NOT_DECLARED`. `roast` maps both to exit 5 (not allowed) with a hint not to work around
it.

Since every caller goes through `send`, one check covers all of them. The callers that run by
themselves also ask first (`TransportRegistry::limits`), so they skip quietly instead of logging
a refused command every second:

| Caller | With `off` | With `manual` |
|---|---|---|
| Roast sequence (CHARGE, DROP, cooling, cancel) | does nothing; the roast records as usual | sends its fan and heater steps and keeps its cooling watch (it switches the fan off at the end); skips the drive and mode |
| Profile drive (`POST …/replay` in drive mode) | 409 `CONTROL_OFF` before it starts; ghost (overlay only) still works | 409 (needs `full`) |
| Server autotune (`POST …/tune`) | 409 | 409 |
| Fan assist loop | skips the roaster | skips (it acts only in auto, which needs `full`); its reason reads "the roaster's control is below full" |
| Gain schedule (on a fan change, and the gains after a boot) | skips | skips |
| Alarm actions | the gate refuses; the alarm still notifies; the refusal is logged once per alarm | heater/fan/STOP actions run; setpoint and mode are refused |
| Modbus TCP slave | the gate refuses (the write gets a Modbus exception) | as the gate |

## Outputs

A setup carries the roaster's **outputs**:

| Output | Commands that need it | Fields |
|---|---|---|
| `heater` (heater or burner) | heater %, heater on/off, and the PID's: setpoint, mode, PID gains, feedforward, RoR limit | `label` ("Heater", "Burner"), `min`, `max` (%), `step` (% , 0 = any value) |
| `fan` (fan or air) | fan % | the same ("Fan", "Air") |
| `drum` | drum % | the same |
| `damper` | damper % | the same |

An output that is not declared is `null`: its commands are refused at the gate (409
`OUTPUT_NOT_DECLARED`, "esp32_x has no fan output (Roasters → esp32_x → Control)"), and the
dashboard hides its controls. Emergency stop and the lease need no output. **Drum speed and
the damper** (`ingest-mapping.md`, "Drum and damper"): no kind declares them; declared, their
commands pass at manual and full control and are shaped to their range, with no floor.

**Ranges are clamped at the gate**, not refused: a % outside `min..max` is sent as the nearest
bound, and with a `step` it is rounded to the nearest step from `min`. **0 is always off** and
passes as 0, so a fan with a 30 % lowest speed still switches off at COOL END. Every feature
sends through the gate, so the sequence, alarm actions, Profile drive, fan assist, the autotune
and the Modbus slave all respect the range.

The features that need outputs check them too: the sequence filters its steps by outputs
(`for_outputs`), the gain schedule and the autotune need a heater, fan assist both heater and
fan (else its reason: "the roaster has no fan output"), and a drive's fan channel a fan.

## The interlock: the fan floor while the heater is on

`fan_floor_pct` (0 = none): while the heater is on, the fan never goes below it. The fluid bed
has one in firmware (it cuts the heater below 40 % fan); the server keeps a margin above it,
45 %, and four features set a fan (the sequence's fans, fan assist's lowest fan, Profile drive's
fan, the autotune's fan). Those four read the roaster's `fan_floor_pct`, and the gate enforces
it too:

- **At the gate**: a fan % below the floor while the heater is on is raised to the floor (0
  included: switching the fan off while heating is what the interlock is for). *The heater is
  on* when the roaster's latest sample says `heater_enable` (or, when it does not report that,
  heater output above 0), unless the gate itself sent heater off or emergency stop in the last
  5 s (a sample from before the command must not hold the fan up).
- **In the features' settings**: the sequence's CHARGE and cooling fans, fan assist's lowest
  fan and the autotune's fan must be at or above the floor (400 with the reason;
  `SequenceConfig::validate`, `FanAssistConfig::validate` and `StartTune::config` take the
  floor); Profile drive raises a profile fan below it and keeps the floor in its run config
  (`fan_floor_pct`, 45 for a run stored without one).

For the ESP32 fluid bed the floor is 45 %: a hand-set fan below 45 % while the heater is on is
raised to 45 % instead of the firmware cutting the heater at 40 %.

## Kinds' defaults

Choosing a kind writes its defaults; a kind is a starting point the owner edits, not a driver.

| Kind | Outputs | Fan floor | Roast sequence | Fan assist |
|---|---|---|---|---|
| `fluid_bed_electric` | Heater 0–100, Fan 0–100 | 45 % | CHARGE: fan 100, heater 60, drive; DROP: fan 100, cool to 45 °C | its defaults (off until switched on) |
| `drum_electric` | Heater 0–100, Fan 0–100 | none | off (CHARGE and DROP record only) | off |
| `drum_gas` | Burner 0–100, Air 0–100 | none | off | off |
| `logger` | none | none | off | off |
| `other` | Heater 0–100, Fan 0–100 | none | off | off |

- **When**: a `PUT` that changes the kind writes the new kind's outputs and fan floor (unless
  the body gives them) and resets the roast sequence (`SequenceConfig::for_kind`) and fan assist
  (off) to the kind's. A `PUT` that keeps the kind changes only what it gives. The Setup panel
  says so before it saves ("Changing the kind sets the outputs, the roast sequence and fan
  assist to a drum (gas) roaster's defaults").
- **With nothing saved**: a roaster's sequence with no saved settings is its kind's.
- **Migration 046** adds `outputs` (JSON) and `fan_floor_pct` to `roaster_setup`, both
  nullable: NULL is the kind's default, so no row was rewritten and the ESP32 reads heater and
  fan 0–100 % with a 45 % floor; sequence and fan assist settings already saved stay as they
  are. The virtual roaster is fixed: the fluid bed's.

## Where it shows

- **API**: `GET /api/roaster/{device_id}/setup` → `{device_id, kind, control, outputs,
  fan_floor_pct, fixed, updated_at}` (`fixed` true for `virtual_roaster`); `PUT` the same,
  audited as `roaster.setup`, from the owner key or an admin token as above ("Tokens and the
  setup"). `outputs` and `fan_floor_pct` are optional in the `PUT` (400 for a bad range: `0 <=
  min < max <= 100`, `0 <= step <= max - min`, a label of 1–24 characters, a floor at or below
  the fan's `max`, and no floor without both heater and fan). Outputs given without a floor
  drop a floor that no longer fits (no fan, or above its max) rather than refuse. There is no
  `GET /api/roasters`: `roast roasters` reads each roaster's `/setup` for its `kind` and
  `control`.
- **CLI**: `roast roaster show <roaster>` prints the setup with an outputs line; `roast
  roasters` a `control` column; `roast roaster control <roaster> off|manual|full` asks for a
  level (above, "Tokens and the setup").
- **Dashboard**: Roasters → a roaster → Control opens with a **Setup** panel: kind (a select),
  control (three underlined choices, each with a `Hint`), and **Outputs → Edit** (per output:
  declared, label, range, step; and the fan floor), which saves outputs and floor together; a
  kind change asks first. Switching to `full` or `manual` asks once: "Roastwarden will send
  commands to esp32_x: the roast sequence at CHARGE, alarm actions, Profile drive. Check its
  own safety cutoffs first." The panels below (gain schedule, fan assist, sequence, token
  access) show only when control allows what they do; fan assist only with both heater and
  fan, and the sequence and fan assist panels' lowest fan is the floor. The Control tab shows
  Setup for a roaster that is not the selected one (a new roaster usually is not).
  On the Roast page, a roaster with control `off` shows the Control tab's message instead of
  the sliders and the header has no STOP; Profile drive offers ghost only. The Control tab
  shows only declared outputs, with their labels, ranges and steps on the sliders; with no
  heater it hides heater on/off, mode and setpoint; the fan slider shows the floor while the
  heater is on. Until the setup has loaded the dashboard acts as full control with the fluid
  bed's outputs (`effectiveOutputs`), since the server refuses what the setup does not allow;
  the selected roaster's setup reloads with the session poll (10 s) and at once after a save.
  Roasters → Add asks the kind first, saves it at `off` with the roaster, checks, and ends with
  the Control step; a refused `PUT` is a notification and the roaster stays at `off`.
- **Agents**: the skill's roaster section says what each level means, what a missing output
  and the floor mean, and that switching control on is the owner's.

## Tests

- The gate, per level and command (`setup.rs`): `off` refuses everything, `manual` passes
  exactly heater/fan/heater-enable/STOP, `full` passes all; no row is `off`; `virtual_roaster`
  is `full` whatever its row says.
- Outputs: undeclared outputs refused (per command), ranges clamped, steps rounded, 0 passes,
  the floor raises a fan while the heater is on (by `heater_enable`, by heater output, not in
  the 5 s after heater off or STOP), no floor with heater off.
- Kinds: each kind's defaults; a kind change resets sequence and fan assist, a same-kind `PUT`
  does not; a roaster with no saved sequence gets its kind's (a drum's CHARGE sends nothing).
- Migration 045 on a database with two roasters: both `full`; a roaster discovered after it is
  `off`. Migration 046 on a database with a fluid bed row and a drum row.
- In the harness: a new MQTT-discovered roaster gets no command at CHARGE (the loopback broker
  sees no publish to it), `control/heater` answers 409 `CONTROL_OFF`, and after a `PUT` to
  `manual` it passes while `control/setpoint` still answers 409.
- No regression: the ESP32's setup after 046 is fluid bed, heater and fan 0–100, floor 45; the
  virtual roaster's control tests and the drive check in CI pass unchanged.
