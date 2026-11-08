# Virtual roaster

Two simulated roasters run inside the server, so you can try Roastwarden without beans or a
roaster: rehearse a profile, an alarm set, Profile drive or fan assist, or learn how adding a
roaster works. Both are closed loop: the heater and fan you set change what happens (Artisan's
simulator replays a recorded roast instead).

- **`virtual_roaster`**, an electric fluid bed. It speaks Roastwarden's own MQTT and has a PID
  of its own, like a roaster built for Roastwarden, and always has full control. Its heat model
  was fitted to a real fluid-bed roaster: at full fan it holds 165 °C on about 88 % heater and
  tops out near 184 °C.
- **`virtual_drum`**, a gas drum. It speaks a dialect of its own in °F, so it is read through a
  normal connection with a mapping and a setup, as a stranger's roaster would be. Burner, air
  and drum speed, an exhaust probe and an inlet channel, and no PID: Profile drive runs the
  server's loop on it.

## Using it

1. Roasters → **Simulator** → On (for the drum, **Simulated drum** → On). Or start the server
   with `ROASTWARDEN_VIRTUAL_ROASTER=1` or `ROASTWARDEN_VIRTUAL_DRUM=1`.
2. Pick `virtual_roaster` (or `virtual_drum`) as the roaster, in the header or in the Roast
   page's Next step.
3. Pick a profile, **Start roast**, set the fan and switch the heater on, then **Charge**: on a
   simulated roaster that also loads cold beans, and BT drops to room temperature. Start
   Profile drive if the profile should drive it.
4. **Drop** marks DROP and dumps the beans; on the fluid bed the roast sequence also cuts the
   heater and cools.

The Simulator panel's **Charge beans** and **Dump beans** do the same by hand. Both roasters run
in real time: replay, alarms and fan assist work on the wall clock, so a simulated roast cannot
be sped up.

## How they behave

| | Fluid bed (`virtual_roaster`) | Gas drum (`virtual_drum`) |
|---|---|---|
| Takes | heater, fan, setpoint, mode, PID gains, feedforward, lease, emergency stop | burner, air, drum speed, emergency stop |
| Safe state | below 40 % fan, or heater power off, no heat reaches the element; an expired lease or emergency stop turns the heater off (emergency stop also runs the fan at 100 %) | emergency stop, or the server gone for 10 s with the burner on: burner 0, air 100 %, drum still turning |
| A roast | BT follows the air with a 60 s lag when beans are loaded | preheated at 60 % burner the probe reads about 218 °C; charged and held there, the turning point is near 1:30 and first crack near 9:20, the exhaust above BT |
| Set up as | fixed: fluid bed, full control | written the first time it is switched on: gas drum, control full, Burner 0–100 % in 5 % steps, Air 0–100 %, Drum 40–70 %, the channel Inlet. What you change afterwards stays |

The fluid bed's own firmware autotune command is not simulated; the server's autotune runs on it
as on any roaster. The drum's model is not fitted to a real machine; it only behaves like one.
A simulated roaster never holds up a server update.

There is no `roast` command for the simulators: an agent switches them with
`roast api PUT /api/virtual-roaster --body '{"enabled": true}'` (or `{"drum": true}`).

<!-- site: end -->

## Design notes

For developers: how the simulated roasters are built and why. The site publishes only the part
above.

Server: `crates/server/src/sim/virtual_roaster.rs`. Dashboard: Roasters → Simulator (and the roaster's Simulator tab; on the
Roast page while it is the selected roaster).

A simulated roaster inside the server, for rehearsing a profile, an alarm
set, a replay or fan assist without beans. It speaks MQTT exactly like the
ESP32 firmware, so nothing else in Roastwarden knows it is not real:

- publishes `roaster/virtual_roaster/telemetry` every second (the firmware's
  JSON: `beanTemp`, `envTemp`, `heater`, `heaterApplied`, `fan`, `setpoint`,
  `controlMode`, `heaterEnable`, gains, …) and a retained status with
  `caps: [pid, ff, lease, ror]`, so a replay runs the loop on the "device";
- obeys `roaster/virtual_roaster/control/*`: setpoint, fan, heater, mode,
  heater_enable, pid, feedforward, lease, emergency_stop, ror_limit (stored,
  not enforced). Autotune is not simulated.

Artisan's simulator replays a recorded roast as input (open loop). This one
is closed loop: the heater and the fan change what happens.

## Model

Fitted to the fluid bed's autotunes:

| condition | real roaster | model |
|---|---|---|
| 100 % fan, holding 165 °C | 88 % heater | 88 % |
| 75 % fan, holding 190 °C | 78 % heater | 78 % |
| 100 % fan, full heat | tops out in the 180s | ~184 °C |

The air (ET) settles at `25 + heater × 164 / fan` °C (fan as a fraction)
with an 8 s lag; the bean probe follows the air with a 60 s lag when beans
are loaded and 6 s when the chamber is empty. Readings are made as
firmware 3.4.0 makes them: four MAX6675 reads a second, each with σ 0.25 °C noise (what the
real roaster's measured idle) and quantised to 0.25 °C, averaged and sent with two decimals
(σ about 0.13 °C); the device PID works from that reading as the firmware does. The device PID is the firmware's
(gains in 0..255 units, D on measurement, bumpless switch to auto,
feedforward), starting from the fluid bed's tuned PI gains (its server autotune at 100 % fan) Kp 8.355 / Ki 0.22269 /
Kd 0 (until 0.30.0: 10.597 / 0.13257 / 61.115, the gains from before its tuning, which made the heater jump). Below 40 % fan or with the heater disabled no heat reaches the
element; an expired lease or an emergency stop puts it in the safe state
(heater off, manual; e-stop also runs the fan at 100 %).

## Use

1. Roasters → Simulator → On (or `ROASTWARDEN_VIRTUAL_ROASTER=1`).
2. Pick `virtual_roaster` as the roaster (header, or Next step on the Roast
   page). A freshly discovered one is listed under "Discovered".
3. Pick a profile, **Start roast**, set the fan and switch the heater on,
   then **Charge**: on the virtual roaster that also loads cold beans (BT
   drops to room temperature). Start Profile drive if the profile should
   drive it.
4. **Drop** marks DROP, cuts the heater and dumps the beans.

The Simulator panel's Charge beans / Dump beans buttons do the same by hand.

API: `GET/PUT /api/virtual-roaster` (`{"enabled": true}`),
`POST /api/virtual-roaster/charge`, `POST /api/virtual-roaster/dump`.

It runs at real time: the server's replay, alarms and fan assist work on
the wall clock, so it cannot be sped up.

## The simulated drum (`virtual_drum`)

Server: `crates/server/src/sim/virtual_drum.rs`; adding a roaster, which it exercises, is
`docs/add-roaster.md`.

A second simulated roaster beside the fluid bed, and a stranger's roaster
on purpose: it speaks no Roastwarden JSON, so the server reads it through exactly the path a real
one takes (an MQTT connection with an `ingest` mapping and command templates,
`docs/ingest-mapping.md`, and a setup, `docs/roaster-setup.md`). Nothing in the gate knows it is
simulated; it is a normal roaster with a normal setup row. It is listed as virtual in
`GET /api/busy` (it never holds a deploy) and switched off is not "offline".

**Wire.** Once a second it publishes `sim/drum/state` in **°F**:

```json
{"temps": {"bean": 392.4, "exhaust": 451.0, "inlet": 541.4}, "burner": 60, "air": 50, "drum": 55}
```

It takes Tasmota-style commands, a plain number 0–100 on `sim/drum/cmnd/burner`, `…/air` and
`…/drum` (anything else is ignored), and `sim/drum/cmnd/stop` (burner 0, air 100; the drum keeps
turning). It has no PID and sends no status or caps, so Profile drive runs the server's loop.

**What the server writes once.** The first time it is switched on (or at start, when it is on),
if there is no roaster `virtual_drum`, the server creates it ("Simulated drum", active), an MQTT
connection (`topic_prefix` `sim/drum`, `telemetry_topic` `sim/drum/state`, `ingest` units F with
`bt ← temps.bean`, `et ← temps.exhaust`, `inlet ← temps.inlet` (°F), `heater ← burner`,
`fan ← air`, `drum ← drum`; templates for heater, fan, drum to `sim/drum/cmnd/…` with 0 decimals
and emergency stop to `sim/drum/cmnd/stop`), and its setup: kind `drum_gas`, control `full`,
outputs Burner 0–100 % in 5 % steps, Air 0–100 %, Drum 40–70 %, no fan floor, and the channel
`inlet` ("Inlet", °C, `docs/channels.md`). When the roaster exists, nothing is written: what the
owner changed since stays.

**Physics.** Lumped and not fitted to a machine; it only behaves like one. The burner heats the
inlet gas (4 s lag; more air flow means cooler gas but more of it), the gas heats the drum's
metal and the beans (convection by flow), the drum heats the beans by contact (more with drum
speed) and loses heat to the room, the exhaust leaves between the inlet and the drum and beans
(above BT while heating), and the bean probe reads the beans with a 50 s lag when loaded and the
drum's air (8 s) when empty. A charge of cold beans pulls the drum down by about 30 °C. Preheated
at 60 % burner (air 50 %, drum 55 %) the probe reads about 218 °C; charged and held at 60 %, the
turning point is at 1:30 (about 102 °C) and first crack (196 °C) at about 9:20, exhaust above BT
all the way. More burner roasts faster, burner off and full air cool it.

**The server going away.** "A 10 s silence from the server while the burner is on" is the
server's presence: the drum reads the retained `roastwarden/server/status`, and when
it says `offline` for 10 s with the burner on, the drum stops (burner 0, air 100). Never heard
counts as online (in loopback mode the drum lives inside the server). A silence of commands
would stop every roast whose burner the owner set once and left alone, so it is not that.

**Use.** Roasters → Simulator → the drum switch, `PUT /api/virtual-roaster {"drum": true}`
(`{"enabled": …}` still switches the fluid bed; either key may be left out), or
`ROASTWARDEN_VIRTUAL_DRUM=1`. Beans in and out: `POST /api/virtual-roaster/charge?roaster=virtual_drum`
and `…/dump?roaster=virtual_drum` (no query: the fluid bed's). `GET /api/virtual-roaster` has
its state under `drum` (`enabled`, `loaded`, `bt`, `exhaust`, `inlet` in °C, `burner`, `air`,
`drum` in %). The browser drive check drives only the fluid bed:
it leans on the fluid bed's CHARGE sequence starting Profile drive, which a drum's records-only
sequence does not do.
